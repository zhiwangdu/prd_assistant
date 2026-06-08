use std::{
    collections::BTreeSet,
    fs,
    future::Future,
    path::{Component, Path},
    pin::Pin,
    time::Instant,
};

use serde::{Deserialize, Serialize};
use tokio::{process::Command, time::Duration};

use crate::{
    config::{ToolSettings, ToolsSettings},
    contracts::{
        ActionKind, ActionRisk, AgentAction, EvidenceArtifact, EvidenceProvider, EvidenceRef,
        EvidenceSummary, EvidenceType, TaskContext,
    },
    fs_utils::relative_string,
    models::{GrepResults, Manifest},
};

#[derive(Debug, Clone)]
pub struct ToolRunner {
    settings: ToolsSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolActionInput {
    pub tool: String,
    pub input_file: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ToolRunStatus {
    Ok,
    Failed,
    TimedOut,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolRunRecord {
    pub schema_version: u32,
    pub tool: String,
    pub action_id: String,
    pub status: ToolRunStatus,
    pub exit_code: Option<i32>,
    pub duration_ms: u128,
    pub command: Vec<String>,
    pub input_file: Option<String>,
    pub stdout_path: String,
    pub stderr_path: String,
    pub summary: String,
    #[serde(default)]
    pub findings: Vec<ToolFinding>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolFinding {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u64>,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
struct ParsedToolOutput {
    summary: Option<String>,
    findings: Vec<ToolFinding>,
}

impl ToolRunner {
    pub fn new(settings: ToolsSettings) -> Self {
        Self { settings }
    }

    pub fn rule_based_actions(&self, manifest: &Manifest, grep: &GrepResults) -> Vec<AgentAction> {
        let mut actions = Vec::new();
        for tool in self.settings.tools.values().filter(|tool| tool.enabled) {
            for input_file in select_input_files(tool, manifest, grep) {
                let action_id = format!(
                    "act_tool_{}_{}",
                    safe_action_suffix(&tool.name),
                    stable_input_hash(&input_file)
                );
                actions.push(AgentAction {
                    schema_version: 1,
                    action_id,
                    kind: ActionKind::RunTool,
                    reason: format!("rule matched configured tool {}", tool.name),
                    evidence_refs: vec![
                        EvidenceRef {
                            artifact_path: "manifest.json".to_string(),
                            selector: None,
                        },
                        EvidenceRef {
                            artifact_path: "grep_results.json".to_string(),
                            selector: None,
                        },
                    ],
                    input: serde_json::json!({
                        "tool": tool.name,
                        "inputFile": input_file,
                    }),
                    risk: ActionRisk::SafeReadOnly,
                    fingerprint: format!("run_tool:{}:{input_file}", tool.name),
                });
            }
        }
        actions
    }

    async fn execute_action(
        &self,
        context: &TaskContext,
        action: &AgentAction,
    ) -> anyhow::Result<EvidenceArtifact> {
        if action.kind != ActionKind::RunTool {
            anyhow::bail!("ToolRunner cannot execute {:?} actions", action.kind);
        }
        validate_action_id(&action.action_id)?;
        let input = action.decode_input::<ToolActionInput>()?;
        let tool = self
            .settings
            .tools
            .get(&input.tool)
            .filter(|tool| tool.enabled)
            .ok_or_else(|| anyhow::anyhow!("tool {} is not configured or enabled", input.tool))?;
        let result_dir = context
            .workspace
            .join("tool_results")
            .join(&action.action_id);
        let result_path = result_dir.join("result.json");
        if result_path.exists() {
            let record = read_record(&result_path)?;
            return artifact_from_record(&context.workspace, &result_path, record);
        }

        fs::create_dir_all(&result_dir)?;
        let stdout_path = result_dir.join("stdout.txt");
        let stderr_path = result_dir.join("stderr.txt");
        let (command, input_file) = build_command(context, action, tool, &input)?;
        let started = Instant::now();
        let output = run_command(context, tool, &command).await;
        let duration_ms = started.elapsed().as_millis();
        let record = match output {
            Ok(output) => {
                let stdout = truncate_bytes(&output.stdout, tool.max_output_bytes);
                let stderr = truncate_bytes(&output.stderr, tool.max_output_bytes);
                fs::write(&stdout_path, stdout)?;
                fs::write(&stderr_path, stderr)?;
                let parsed = parse_tool_stdout(stdout);
                let status = if output.status.success() {
                    ToolRunStatus::Ok
                } else {
                    ToolRunStatus::Failed
                };
                let fallback_summary = match status {
                    ToolRunStatus::Ok => format!("tool {} completed successfully", tool.name),
                    ToolRunStatus::Failed => {
                        format!("tool {} exited with non-zero status", tool.name)
                    }
                    ToolRunStatus::TimedOut => unreachable!(),
                };
                ToolRunRecord {
                    schema_version: 2,
                    tool: tool.name.clone(),
                    action_id: action.action_id.clone(),
                    status,
                    exit_code: output.status.code(),
                    duration_ms,
                    command,
                    input_file,
                    stdout_path: relative_string(&context.workspace, &stdout_path)?,
                    stderr_path: relative_string(&context.workspace, &stderr_path)?,
                    summary: parsed.summary.unwrap_or(fallback_summary),
                    findings: parsed.findings,
                    error: None,
                }
            }
            Err(ToolExecutionError::TimedOut) => {
                fs::write(&stdout_path, b"")?;
                fs::write(&stderr_path, b"tool timed out")?;
                ToolRunRecord {
                    schema_version: 2,
                    tool: tool.name.clone(),
                    action_id: action.action_id.clone(),
                    status: ToolRunStatus::TimedOut,
                    exit_code: None,
                    duration_ms,
                    command,
                    input_file,
                    stdout_path: relative_string(&context.workspace, &stdout_path)?,
                    stderr_path: relative_string(&context.workspace, &stderr_path)?,
                    summary: format!(
                        "tool {} timed out after {} seconds",
                        tool.name, tool.timeout_seconds
                    ),
                    findings: Vec::new(),
                    error: Some("tool timed out".to_string()),
                }
            }
            Err(ToolExecutionError::Spawn(message)) => {
                fs::write(&stdout_path, b"")?;
                fs::write(&stderr_path, message.as_bytes())?;
                ToolRunRecord {
                    schema_version: 2,
                    tool: tool.name.clone(),
                    action_id: action.action_id.clone(),
                    status: ToolRunStatus::Failed,
                    exit_code: None,
                    duration_ms,
                    command,
                    input_file,
                    stdout_path: relative_string(&context.workspace, &stdout_path)?,
                    stderr_path: relative_string(&context.workspace, &stderr_path)?,
                    summary: format!("tool {} could not be started", tool.name),
                    findings: Vec::new(),
                    error: Some(message),
                }
            }
        };
        write_record(&result_path, &record)?;
        artifact_from_record(&context.workspace, &result_path, record)
    }
}

impl EvidenceProvider for ToolRunner {
    fn execute<'a>(
        &'a self,
        context: &'a TaskContext,
        action: &'a AgentAction,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<EvidenceArtifact>> + Send + 'a>> {
        Box::pin(async move { self.execute_action(context, action).await })
    }
}

fn select_input_files(tool: &ToolSettings, manifest: &Manifest, grep: &GrepResults) -> Vec<String> {
    let limit = tool.max_input_files.max(1);
    let mut selected = Vec::new();

    for file in &manifest.files {
        if selected.len() >= limit {
            return selected;
        }
        if tool
            .match_settings
            .file_patterns
            .iter()
            .any(|pattern| matches_pattern(pattern, &file.path.to_ascii_lowercase()))
        {
            push_selected(&mut selected, &file.path);
        }
    }

    if selected.len() >= limit || tool.match_settings.keywords.is_empty() {
        selected.truncate(limit);
        return selected;
    }

    let manifest_paths = manifest
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect::<BTreeSet<_>>();
    for entry in &grep.matches {
        if selected.len() >= limit {
            break;
        }
        if !manifest_paths.contains(entry.file.as_str()) {
            continue;
        }
        let text = entry.text.to_ascii_lowercase();
        if tool
            .match_settings
            .keywords
            .iter()
            .any(|keyword| text.contains(keyword))
        {
            push_selected(&mut selected, &entry.file);
        }
    }

    selected.truncate(limit);
    selected
}

fn push_selected(selected: &mut Vec<String>, manifest_path: &str) {
    let input_file = format!("extracted/{manifest_path}");
    if !selected.iter().any(|value| value == &input_file) {
        selected.push(input_file);
    }
}

fn matches_pattern(pattern: &str, value: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(suffix) = pattern.strip_prefix('*') {
        return value.ends_with(suffix);
    }
    value == pattern
}

fn build_command(
    context: &TaskContext,
    action: &AgentAction,
    tool: &ToolSettings,
    input: &ToolActionInput,
) -> anyhow::Result<(Vec<String>, Option<String>)> {
    let input_file = input
        .input_file
        .as_deref()
        .map(validate_workspace_relative_path)
        .transpose()?;
    let input_path = input_file.map(|path| context.workspace.join(path));
    let mut command = Vec::with_capacity(tool.args.len() + 1);
    command.push(tool.path.display().to_string());
    for arg in &tool.args {
        command.push(render_arg(arg, context, input_path.as_deref(), action)?);
    }
    Ok((
        command,
        input_file.map(|path| path.to_string_lossy().replace('\\', "/")),
    ))
}

fn render_arg(
    arg: &str,
    context: &TaskContext,
    input_path: Option<&Path>,
    action: &AgentAction,
) -> anyhow::Result<String> {
    let mut rendered = arg.replace("{workspace}", &context.workspace.display().to_string());
    rendered = rendered.replace(
        "{manifest_path}",
        &context
            .workspace
            .join("manifest.json")
            .display()
            .to_string(),
    );
    rendered = rendered.replace(
        "{grep_results_path}",
        &context
            .workspace
            .join("grep_results.json")
            .display()
            .to_string(),
    );
    rendered = rendered.replace("{action_id}", &action.action_id);
    if rendered.contains("{input_file}") {
        let input_path =
            input_path.ok_or_else(|| anyhow::anyhow!("tool argument requires inputFile"))?;
        rendered = rendered.replace("{input_file}", &input_path.display().to_string());
    }
    if rendered.contains('{') || rendered.contains('}') {
        anyhow::bail!("unsupported tool argument placeholder in {arg}");
    }
    Ok(rendered)
}

async fn run_command(
    context: &TaskContext,
    tool: &ToolSettings,
    command: &[String],
) -> Result<std::process::Output, ToolExecutionError> {
    let mut process = Command::new(&tool.path);
    process
        .args(command.iter().skip(1))
        .current_dir(&context.workspace)
        .kill_on_drop(true)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let child = process
        .spawn()
        .map_err(|err| ToolExecutionError::Spawn(err.to_string()))?;
    match tokio::time::timeout(
        Duration::from_secs(tool.timeout_seconds),
        child.wait_with_output(),
    )
    .await
    {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(err)) => Err(ToolExecutionError::Spawn(err.to_string())),
        Err(_) => Err(ToolExecutionError::TimedOut),
    }
}

enum ToolExecutionError {
    TimedOut,
    Spawn(String),
}

fn artifact_from_record(
    workspace: &Path,
    result_path: &Path,
    record: ToolRunRecord,
) -> anyhow::Result<EvidenceArtifact> {
    let mut details = vec![record.summary.clone()];
    details.extend(
        record
            .findings
            .iter()
            .take(5)
            .map(|finding| finding_detail(finding)),
    );
    let artifact = EvidenceArtifact {
        schema_version: 1,
        action_id: Some(record.action_id),
        evidence_type: EvidenceType::ToolOutput,
        artifact_path: relative_string(workspace, result_path)?,
        summary: EvidenceSummary {
            title: format!("{} {:?}", record.tool, record.status),
            details,
        },
    };
    artifact.validate()?;
    Ok(artifact)
}

fn parse_tool_stdout(stdout: &[u8]) -> ParsedToolOutput {
    let text = String::from_utf8_lossy(stdout);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return ParsedToolOutput::default();
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) else {
        return ParsedToolOutput::default();
    };
    parse_tool_output_value(&value)
}

fn parse_tool_output_value(value: &serde_json::Value) -> ParsedToolOutput {
    match value {
        serde_json::Value::Object(object) => {
            if is_influxql_report(object) {
                return parse_influxql_report(object);
            }
            if is_influxql_compare_report(object) {
                return parse_influxql_compare_report(object);
            }
            let summary = string_field(object, &["summary", "message", "title"]);
            let findings = object
                .get("findings")
                .or_else(|| object.get("issues"))
                .or_else(|| object.get("diagnostics"))
                .map(parse_findings_value)
                .unwrap_or_default();
            ParsedToolOutput { summary, findings }
        }
        serde_json::Value::Array(_) => ParsedToolOutput {
            summary: None,
            findings: parse_findings_value(value),
        },
        serde_json::Value::String(message) => ParsedToolOutput {
            summary: Some(message.clone()),
            findings: Vec::new(),
        },
        _ => ParsedToolOutput::default(),
    }
}

fn is_influxql_report(object: &serde_json::Map<String, serde_json::Value>) -> bool {
    object.contains_key("total_records")
        && object.contains_key("total_statements")
        && object.contains_key("fingerprints")
}

fn is_influxql_compare_report(object: &serde_json::Map<String, serde_json::Value>) -> bool {
    object.contains_key("batch_a")
        && object.contains_key("batch_b")
        && object.contains_key("statement_delta")
}

fn parse_influxql_report(object: &serde_json::Map<String, serde_json::Value>) -> ParsedToolOutput {
    let total_records = u64_field(object, "total_records").unwrap_or_default();
    let records_in_window = u64_field(object, "records_in_window").unwrap_or_default();
    let total_statements = u64_field(object, "total_statements").unwrap_or_default();
    let parse_error_count = u64_field(object, "parse_error_count").unwrap_or_default();
    let rule_summary = influxql_rule_summary(object);
    let mut summary = format!(
        "influxql report: records={total_records}, recordsInWindow={records_in_window}, statements={total_statements}, parseErrors={parse_error_count}"
    );
    if !rule_summary.is_empty() {
        summary.push_str(&format!(", specialRules={rule_summary}"));
    }

    let mut findings = Vec::new();
    findings.extend(influxql_special_rule_findings(object));
    findings.extend(influxql_parse_error_findings(object));
    findings.extend(influxql_realtime_findings(object));
    findings.extend(influxql_fingerprint_findings(object));

    ParsedToolOutput {
        summary: Some(summary),
        findings,
    }
}

fn parse_influxql_compare_report(
    object: &serde_json::Map<String, serde_json::Value>,
) -> ParsedToolOutput {
    let statement_delta = number_to_string(object.get("statement_delta")).unwrap_or_default();
    let qps_delta = number_to_string(object.get("qps_delta")).unwrap_or_default();
    let batch_a = compare_batch_summary(object.get("batch_a"));
    let batch_b = compare_batch_summary(object.get("batch_b"));
    let summary = format!(
        "influxql compare report: statementDelta={statement_delta}, qpsDelta={qps_delta}, batchA={batch_a}, batchB={batch_b}"
    );
    let mut findings = Vec::new();
    findings.extend(compare_fingerprint_findings(
        object,
        "new_fingerprints",
        "new fingerprint",
    ));
    findings.extend(compare_fingerprint_findings(
        object,
        "removed_fingerprints",
        "removed fingerprint",
    ));
    findings.extend(compare_fingerprint_findings(
        object,
        "changed_fingerprints",
        "changed fingerprint",
    ));
    findings.extend(compare_rule_delta_findings(object));

    ParsedToolOutput {
        summary: Some(summary),
        findings,
    }
}

fn compare_batch_summary(value: Option<&serde_json::Value>) -> String {
    let Some(object) = value.and_then(|value| value.as_object()) else {
        return "unknown".to_string();
    };
    let statements = number_to_string(object.get("total_statements")).unwrap_or_else(|| "0".into());
    let parse_errors =
        number_to_string(object.get("parse_error_count")).unwrap_or_else(|| "0".into());
    let qps = number_to_string(object.get("qps")).unwrap_or_else(|| "0".into());
    let duration =
        number_to_string(object.get("effective_duration_seconds")).unwrap_or_else(|| "0".into());
    format!("statements={statements}, parseErrors={parse_errors}, qps={qps}, durationSeconds={duration}")
}

fn compare_fingerprint_findings(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    label: &str,
) -> Vec<ToolFinding> {
    object
        .get(key)
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .take(8)
                .filter_map(|item| {
                    let item = item.as_object()?;
                    let status = string_field(item, &["status"]).unwrap_or_else(|| label.into());
                    let statement_type =
                        string_field(item, &["statement_type"]).unwrap_or_else(|| "unknown".into());
                    let normalized = string_field(item, &["normalized_query"])
                        .or_else(|| string_field(item, &["fingerprint"]))
                        .unwrap_or_else(|| "unknown".into());
                    let count_a =
                        number_to_string(item.get("count_a")).unwrap_or_else(|| "0".into());
                    let count_b =
                        number_to_string(item.get("count_b")).unwrap_or_else(|| "0".into());
                    let count_delta =
                        number_to_string(item.get("count_delta")).unwrap_or_else(|| "0".into());
                    let qps_a = number_to_string(item.get("qps_a")).unwrap_or_else(|| "0".into());
                    let qps_b = number_to_string(item.get("qps_b")).unwrap_or_else(|| "0".into());
                    let qps_delta =
                        number_to_string(item.get("qps_delta")).unwrap_or_else(|| "0".into());
                    let rules = item
                        .get("rules")
                        .and_then(|value| value.as_array())
                        .map(|rules| {
                            rules
                                .iter()
                                .filter_map(|rule| rule.as_str())
                                .collect::<Vec<_>>()
                                .join(",")
                        })
                        .unwrap_or_default();
                    Some(ToolFinding {
                        severity: Some(compare_fingerprint_severity(key, &count_delta).to_string()),
                        file: None,
                        line: None,
                        message: format!(
                            "{label}: status={status}, statementType={statement_type}, count={count_a}->{count_b} (delta={count_delta}), qps={qps_a}->{qps_b} (delta={qps_delta}), rules={}, query={}",
                            if rules.is_empty() { "-" } else { rules.as_str() },
                            normalized
                        ),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn compare_rule_delta_findings(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Vec<ToolFinding> {
    object
        .get("rule_deltas")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .take(8)
                .filter_map(|item| {
                    let item = item.as_object()?;
                    let rule = string_field(item, &["rule"]).unwrap_or_else(|| "unknown".into());
                    let count_a =
                        number_to_string(item.get("count_a")).unwrap_or_else(|| "0".into());
                    let count_b =
                        number_to_string(item.get("count_b")).unwrap_or_else(|| "0".into());
                    let count_delta =
                        number_to_string(item.get("count_delta")).unwrap_or_else(|| "0".into());
                    let qps_a = number_to_string(item.get("qps_a")).unwrap_or_else(|| "0".into());
                    let qps_b = number_to_string(item.get("qps_b")).unwrap_or_else(|| "0".into());
                    let qps_delta =
                        number_to_string(item.get("qps_delta")).unwrap_or_else(|| "0".into());
                    Some(ToolFinding {
                        severity: Some(compare_delta_severity(&count_delta).to_string()),
                        file: None,
                        line: None,
                        message: format!(
                            "rule delta: rule={rule}, count={count_a}->{count_b} (delta={count_delta}), qps={qps_a}->{qps_b} (delta={qps_delta})"
                        ),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn compare_fingerprint_severity(key: &str, count_delta: &str) -> &'static str {
    match key {
        "removed_fingerprints" => "low",
        "new_fingerprints" => "high",
        _ => compare_delta_severity(count_delta),
    }
}

fn compare_delta_severity(count_delta: &str) -> &'static str {
    match count_delta.trim().parse::<f64>() {
        Ok(value) if value > 0.0 => "high",
        Ok(value) if value < 0.0 => "low",
        _ => "medium",
    }
}

fn influxql_rule_summary(object: &serde_json::Map<String, serde_json::Value>) -> String {
    object
        .get("special_rules")
        .and_then(|value| value.as_array())
        .map(|rules| {
            rules
                .iter()
                .take(8)
                .filter_map(|rule| {
                    let rule = rule.as_object()?;
                    let name = string_field(rule, &["rule"])?;
                    let count = number_to_string(rule.get("count")).unwrap_or_else(|| "0".into());
                    Some(format!("{name}:{count}"))
                })
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
}

fn influxql_special_rule_findings(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Vec<ToolFinding> {
    object
        .get("special_rules")
        .and_then(|value| value.as_array())
        .map(|rules| {
            rules
                .iter()
                .take(12)
                .filter_map(|rule| {
                    let rule = rule.as_object()?;
                    let name = string_field(rule, &["rule"])?;
                    let count = number_to_string(rule.get("count")).unwrap_or_else(|| "0".into());
                    let fingerprint_count = rule
                        .get("fingerprints")
                        .and_then(|value| value.as_array())
                        .map(|items| items.len())
                        .unwrap_or_default();
                    Some(ToolFinding {
                        severity: Some(influxql_rule_severity(&name).to_string()),
                        file: None,
                        line: None,
                        message: format!(
                            "rule {name} matched {count} statement(s) across {fingerprint_count} fingerprint(s): {}",
                            influxql_rule_description(&name)
                        ),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn influxql_parse_error_findings(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Vec<ToolFinding> {
    object
        .get("parse_errors")
        .and_then(|value| value.as_array())
        .map(|errors| {
            errors
                .iter()
                .take(5)
                .filter_map(|error| {
                    let error = error.as_object()?;
                    let message = string_field(error, &["error"])?;
                    let count = number_to_string(error.get("count")).unwrap_or_else(|| "0".into());
                    Some(ToolFinding {
                        severity: Some("high".to_string()),
                        file: None,
                        line: None,
                        message: format!("parse error occurred {count} time(s): {message}"),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn influxql_realtime_findings(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Vec<ToolFinding> {
    let Some(realtime) = object
        .get("realtime_query")
        .and_then(|value| value.as_object())
    else {
        return Vec::new();
    };
    let total = u64_field(realtime, "total").unwrap_or_default();
    if total == 0 {
        return Vec::new();
    }
    let non_realtime = u64_field(realtime, "non_realtime").unwrap_or_default();
    let unknown = u64_field(realtime, "unknown").unwrap_or_default();
    let realtime_count = u64_field(realtime, "realtime").unwrap_or_default();
    let mut findings = Vec::new();
    if non_realtime > 0 {
        findings.push(ToolFinding {
            severity: Some("medium".to_string()),
            file: None,
            line: None,
            message: format!(
                "realtime query classification found {non_realtime}/{total} non-realtime select-like statement(s)"
            ),
        });
    }
    if unknown > 0 {
        let reason = first_realtime_sample_reason(realtime, "sample_unknown")
            .map(|reason| format!("; sample reason: {reason}"))
            .unwrap_or_default();
        findings.push(ToolFinding {
            severity: Some("low".to_string()),
            file: None,
            line: None,
            message: format!(
                "realtime query classification is unknown for {unknown}/{total} select-like statement(s); realtime={realtime_count}{reason}"
            ),
        });
    }
    findings
}

fn influxql_fingerprint_findings(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Vec<ToolFinding> {
    object
        .get("fingerprints")
        .and_then(|value| value.as_array())
        .map(|fingerprints| {
            fingerprints
                .iter()
                .take(5)
                .filter_map(|fingerprint| {
                    let fingerprint = fingerprint.as_object()?;
                    let count = u64_field(fingerprint, "count").unwrap_or_default();
                    let rules = fingerprint
                        .get("rules")
                        .and_then(|value| value.as_array())
                        .map(|rules| {
                            rules
                                .iter()
                                .filter_map(|value| value.as_str())
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    if count <= 1 && rules.is_empty() {
                        return None;
                    }
                    let statement_type =
                        string_field(fingerprint, &["statement_type"]).unwrap_or_default();
                    let normalized_query =
                        string_field(fingerprint, &["normalized_query"]).unwrap_or_default();
                    let rule_text = if rules.is_empty() {
                        "none".to_string()
                    } else {
                        rules.join(", ")
                    };
                    Some(ToolFinding {
                        severity: Some("low".to_string()),
                        file: None,
                        line: None,
                        message: format!(
                            "fingerprint {statement_type} occurred {count} time(s), rules=[{rule_text}], normalized={normalized_query}"
                        ),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn first_realtime_sample_reason(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<String> {
    object
        .get(key)?
        .as_array()?
        .first()?
        .as_object()
        .and_then(|sample| string_field(sample, &["reason"]))
}

fn influxql_rule_severity(rule: &str) -> &'static str {
    match rule {
        "write_or_destructive" | "large_limit" | "no_time_filter" => "high",
        "group_by_high_cardinality_risk" | "not_realtime_query" => "medium",
        "has_regex" | "has_wildcard" | "meta_query" => "low",
        _ => "low",
    }
}

fn influxql_rule_description(rule: &str) -> &'static str {
    match rule {
        "no_time_filter" => "SELECT has no explicit time predicate",
        "has_regex" => "query uses regex matching or regex measurement/source",
        "has_wildcard" => "query uses wildcard selection, grouping, or metadata scope",
        "large_limit" => "LIMIT or SLIMIT is greater than or equal to the configured threshold",
        "group_by_high_cardinality_risk" => {
            "non-time GROUP BY dimensions exceed the configured threshold"
        }
        "meta_query" => "metadata or explain query",
        "write_or_destructive" => "query writes data or performs destructive changes",
        "not_realtime_query" => "select-like query is explicitly non-realtime",
        _ => "unrecognized analyzer rule",
    }
}

fn parse_findings_value(value: &serde_json::Value) -> Vec<ToolFinding> {
    let Some(items) = value.as_array() else {
        return Vec::new();
    };
    items.iter().filter_map(parse_finding_value).collect()
}

fn parse_finding_value(value: &serde_json::Value) -> Option<ToolFinding> {
    match value {
        serde_json::Value::String(message) => non_empty(message).map(|message| ToolFinding {
            severity: None,
            file: None,
            line: None,
            message,
        }),
        serde_json::Value::Object(object) => {
            let message = string_field(
                object,
                &[
                    "message",
                    "summary",
                    "description",
                    "detail",
                    "title",
                    "cause",
                ],
            )?;
            Some(ToolFinding {
                severity: string_field(object, &["severity", "level", "status"]),
                file: string_field(object, &["file", "path", "filename"]),
                line: number_field(object, &["line", "lineNumber", "startLine"]),
                message,
            })
        }
        _ => None,
    }
}

fn string_field(
    object: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<String> {
    keys.iter().find_map(|key| match object.get(*key)? {
        serde_json::Value::String(value) => non_empty(value),
        serde_json::Value::Number(value) => Some(value.to_string()),
        _ => None,
    })
}

fn number_field(object: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|key| match object.get(*key)? {
        serde_json::Value::Number(value) => value.as_u64(),
        serde_json::Value::String(value) => value.trim().parse().ok(),
        _ => None,
    })
}

fn u64_field(object: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<u64> {
    match object.get(key)? {
        serde_json::Value::Number(value) => value.as_u64(),
        serde_json::Value::String(value) => value.trim().parse().ok(),
        _ => None,
    }
}

fn number_to_string(value: Option<&serde_json::Value>) -> Option<String> {
    match value? {
        serde_json::Value::Number(number) => Some(number.to_string()),
        serde_json::Value::String(value) => non_empty(value),
        _ => None,
    }
}

fn non_empty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn finding_detail(finding: &ToolFinding) -> String {
    let location = match (&finding.file, finding.line) {
        (Some(file), Some(line)) => format!("{file}:{line}"),
        (Some(file), None) => file.clone(),
        (None, Some(line)) => format!("line {line}"),
        (None, None) => "-".to_string(),
    };
    match &finding.severity {
        Some(severity) => format!("{severity} {location}: {}", finding.message),
        None => format!("{location}: {}", finding.message),
    }
}

fn read_record(path: &Path) -> anyhow::Result<ToolRunRecord> {
    let raw = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&raw)?)
}

fn write_record(path: &Path, record: &ToolRunRecord) -> anyhow::Result<()> {
    let temp = path.with_file_name(".result.json.tmp");
    fs::write(&temp, serde_json::to_vec_pretty(record)?)?;
    fs::rename(&temp, path)?;
    Ok(())
}

fn truncate_bytes(bytes: &[u8], max: usize) -> &[u8] {
    if bytes.len() <= max {
        bytes
    } else {
        &bytes[..max]
    }
}

fn validate_workspace_relative_path(path: &str) -> anyhow::Result<&Path> {
    let path = Path::new(path);
    let valid = !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)));
    if !valid {
        anyhow::bail!("tool inputFile must be workspace-relative");
    }
    Ok(path)
}

fn validate_action_id(action_id: &str) -> anyhow::Result<()> {
    let valid = action_id.starts_with("act_")
        && action_id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'_' || value == b'-');
    if valid {
        Ok(())
    } else {
        anyhow::bail!("invalid action id {action_id}")
    }
}

fn safe_action_suffix(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn stable_input_hash(value: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use std::{os::unix::fs::PermissionsExt, path::PathBuf};

    use chrono::Utc;

    use super::*;
    use crate::{
        config::{ToolMatchSettings, ToolSettings, ToolsSettings},
        contracts::EvidenceProvider,
        models::{GrepMatch, ManifestFile, ManifestUpload, TaskSource},
    };

    #[tokio::test]
    async fn executes_configured_tool_and_reuses_existing_result() {
        let fixture = Fixture::new("tool-runner-ok");
        let tool_path = fixture.write_tool(
            "fake_tool.sh",
            "#!/usr/bin/env bash\nprintf 'tool saw %s' \"$1\"\nprintf 'warn' >&2\n",
        );
        let runner = ToolRunner::new(settings(tool_path.clone(), 5));
        let action = action("act_tool_fake", "fake", Some("extracted/sample.log"));
        let artifact = runner.execute(&fixture.context(), &action).await.unwrap();
        assert_eq!(
            artifact.artifact_path,
            "tool_results/act_tool_fake/result.json"
        );

        let record = read_record(&fixture.workspace.join(&artifact.artifact_path)).unwrap();
        assert_eq!(record.status, ToolRunStatus::Ok);
        assert_eq!(record.exit_code, Some(0));
        assert!(
            std::fs::read_to_string(fixture.workspace.join(record.stdout_path))
                .unwrap()
                .contains("sample.log")
        );

        std::fs::remove_file(&tool_path).unwrap();
        let reused = runner.execute(&fixture.context(), &action).await.unwrap();
        assert_eq!(reused.artifact_path, artifact.artifact_path);
    }

    #[tokio::test]
    async fn parses_json_summary_and_findings_from_stdout() {
        let fixture = Fixture::new("tool-runner-json");
        let tool_path = fixture.write_tool(
            "json_tool.sh",
            r#"#!/usr/bin/env bash
cat <<'JSON'
{"summary":"found planner issue","findings":[{"severity":"medium","file":"query.flux","line":12,"message":"filter pushdown failed"}]}
JSON
"#,
        );
        let runner = ToolRunner::new(settings(tool_path, 5));
        let action = action("act_tool_json", "fake", Some("extracted/sample.log"));
        let artifact = runner.execute(&fixture.context(), &action).await.unwrap();
        let record = read_record(&fixture.workspace.join(&artifact.artifact_path)).unwrap();

        assert_eq!(record.schema_version, 2);
        assert_eq!(record.summary, "found planner issue");
        assert_eq!(
            record.findings,
            vec![ToolFinding {
                severity: Some("medium".to_string()),
                file: Some("query.flux".to_string()),
                line: Some(12),
                message: "filter pushdown failed".to_string(),
            }]
        );
        assert_eq!(
            artifact.summary.details,
            vec![
                "found planner issue".to_string(),
                "medium query.flux:12: filter pushdown failed".to_string(),
            ]
        );
    }

    #[test]
    fn parses_array_and_alternate_finding_fields() {
        let parsed = parse_tool_stdout(
            br#"[{"level":"high","path":"query.sql","lineNumber":"7","description":"full scan"},{"message":"missing retention policy"}]"#,
        );

        assert_eq!(parsed.summary, None);
        assert_eq!(parsed.findings.len(), 2);
        assert_eq!(parsed.findings[0].severity.as_deref(), Some("high"));
        assert_eq!(parsed.findings[0].file.as_deref(), Some("query.sql"));
        assert_eq!(parsed.findings[0].line, Some(7));
        assert_eq!(parsed.findings[0].message, "full scan");
        assert_eq!(parsed.findings[1].message, "missing retention policy");
    }

    #[test]
    fn parses_influxql_analyzer_report_into_summary_and_findings() {
        let parsed = parse_tool_stdout(
            br#"{
  "total_records": 2,
  "records_in_window": 2,
  "total_statements": 2,
  "parse_error_count": 1,
  "fingerprints": [
    {
      "statement_type": "SELECT",
      "normalized_query": "SELECT * FROM cpu LIMIT 1",
      "count": 1,
      "rules": ["large_limit", "no_time_filter"]
    }
  ],
  "special_rules": [
    {"rule": "large_limit", "count": 1, "fingerprints": ["fp1"]},
    {"rule": "no_time_filter", "count": 1, "fingerprints": ["fp1"]}
  ],
  "parse_errors": [
    {"error": "found BAD, expected SELECT", "count": 1, "sample_queries": ["BAD"]}
  ],
  "realtime_query": {
    "total": 1,
    "realtime": 0,
    "non_realtime": 0,
    "unknown": 1,
    "sample_unknown": [{"reason": "query has no where time predicate"}]
  }
}"#,
        );

        let summary = parsed.summary.unwrap();
        assert!(summary.contains("records=2"));
        assert!(summary.contains("specialRules=large_limit:1, no_time_filter:1"));
        assert!(parsed
            .findings
            .iter()
            .any(|finding| finding.severity.as_deref() == Some("high")
                && finding.message.contains("rule large_limit")));
        assert!(parsed
            .findings
            .iter()
            .any(|finding| finding.message.contains("parse error occurred 1 time")));
        assert!(parsed.findings.iter().any(|finding| finding
            .message
            .contains("realtime query classification is unknown")));
        assert!(parsed.findings.iter().any(|finding| finding
            .message
            .contains("fingerprint SELECT occurred 1 time")));
    }

    #[test]
    fn parses_influxql_compare_report_into_findings() {
        let parsed = parse_tool_stdout(
            br#"{
  "batch_a": {"total_statements": 10},
  "batch_b": {"total_statements": 14, "qps": 2.5, "effective_duration_seconds": 5},
  "statement_delta": 4,
  "qps_delta": 0.5,
  "new_fingerprints": [{"fingerprint": "fp-new", "statement_type":"SELECT", "normalized_query":"SELECT * FROM cpu", "status":"new", "count_a":0, "count_b":4, "count_delta":4, "qps_a":0, "qps_b":0.5, "qps_delta":0.5, "rules":["no_time_filter"]}],
  "removed_fingerprints": [],
  "changed_fingerprints": [],
  "rule_deltas": [{"rule": "large_limit", "count_a":1, "count_b":3, "count_delta":2, "qps_a":0.1, "qps_b":0.3, "qps_delta":0.2}]
}"#,
        );

        let summary = parsed.summary.as_deref().unwrap();
        assert!(summary.contains("statementDelta=4"));
        assert!(summary.contains("batchB=statements=14"));
        assert!(parsed
            .findings
            .iter()
            .any(|finding| finding.message.contains("count=0->4")
                && finding.message.contains("rules=no_time_filter")
                && finding.severity.as_deref() == Some("high")));
        assert!(parsed
            .findings
            .iter()
            .any(|finding| finding.message.contains("rule=large_limit")
                && finding.message.contains("qps=0.1->0.3")));
    }

    #[test]
    fn ignores_non_json_stdout_for_structured_fields() {
        let parsed = parse_tool_stdout(b"plain text output\n");

        assert_eq!(parsed.summary, None);
        assert!(parsed.findings.is_empty());
    }

    #[tokio::test]
    async fn records_timeout_as_tool_evidence() {
        let fixture = Fixture::new("tool-runner-timeout");
        let tool_path = fixture.write_tool("slow_tool.sh", "#!/usr/bin/env bash\nsleep 2\n");
        let runner = ToolRunner::new(settings(tool_path, 1));
        let action = action("act_tool_slow", "fake", Some("extracted/sample.log"));
        let artifact = runner.execute(&fixture.context(), &action).await.unwrap();
        let record = read_record(&fixture.workspace.join(&artifact.artifact_path)).unwrap();
        assert_eq!(record.status, ToolRunStatus::TimedOut);
    }

    #[test]
    fn selects_rule_based_actions_from_manifest_or_grep() {
        let runner = ToolRunner::new(settings(PathBuf::from("/bin/echo"), 5));
        let actions = runner.rule_based_actions(&manifest(), &grep());
        assert_eq!(actions.len(), 1);
        assert_eq!(
            actions[0].action_id,
            format!(
                "act_tool_fake_{}",
                stable_input_hash("extracted/sample.log")
            )
        );
        let input = actions[0].decode_input::<ToolActionInput>().unwrap();
        assert_eq!(input.tool, "fake");
        assert_eq!(input.input_file.as_deref(), Some("extracted/sample.log"));
    }

    #[test]
    fn rule_based_actions_select_multiple_inputs_with_stable_ids() {
        let mut tool_settings = settings(PathBuf::from("/bin/echo"), 5);
        let tool = tool_settings.tools.get_mut("fake").unwrap();
        tool.max_input_files = 2;
        tool.match_settings.file_patterns = vec!["*.flux".to_string()];
        tool.match_settings.keywords = vec!["select".to_string()];
        let runner = ToolRunner::new(tool_settings);
        let manifest = Manifest {
            files: vec![
                ManifestFile {
                    path: "queries/one.flux".to_string(),
                    size: 1,
                },
                ManifestFile {
                    path: "queries/two.sql".to_string(),
                    size: 1,
                },
                ManifestFile {
                    path: "queries/three.sql".to_string(),
                    size: 1,
                },
            ],
            ..manifest()
        };
        let grep = GrepResults {
            keywords: vec!["select".to_string()],
            total_matches: 2,
            matches: vec![
                GrepMatch {
                    file: "queries/two.sql".to_string(),
                    line: 1,
                    keyword: "select".to_string(),
                    text: "select * from cpu".to_string(),
                },
                GrepMatch {
                    file: "queries/three.sql".to_string(),
                    line: 1,
                    keyword: "select".to_string(),
                    text: "select * from mem".to_string(),
                },
            ],
        };

        let actions = runner.rule_based_actions(&manifest, &grep);

        assert_eq!(actions.len(), 2);
        let inputs = actions
            .iter()
            .map(|action| action.decode_input::<ToolActionInput>().unwrap().input_file)
            .collect::<Vec<_>>();
        assert_eq!(
            inputs,
            vec![
                Some("extracted/queries/one.flux".to_string()),
                Some("extracted/queries/two.sql".to_string()),
            ]
        );
        assert_eq!(
            actions[0].action_id,
            format!(
                "act_tool_fake_{}",
                stable_input_hash("extracted/queries/one.flux")
            )
        );
        assert_eq!(
            actions[1].action_id,
            format!(
                "act_tool_fake_{}",
                stable_input_hash("extracted/queries/two.sql")
            )
        );
    }

    fn settings(path: PathBuf, timeout_seconds: u64) -> ToolsSettings {
        ToolsSettings {
            tools: [(
                "fake".to_string(),
                ToolSettings {
                    name: "fake".to_string(),
                    enabled: true,
                    path,
                    timeout_seconds,
                    max_output_bytes: 1024,
                    max_input_files: 1,
                    args: vec!["{input_file}".to_string()],
                    match_settings: ToolMatchSettings {
                        file_patterns: vec!["*.log".to_string()],
                        keywords: vec!["query".to_string()],
                    },
                },
            )]
            .into_iter()
            .collect(),
        }
    }

    fn action(action_id: &str, tool: &str, input_file: Option<&str>) -> AgentAction {
        AgentAction {
            schema_version: 1,
            action_id: action_id.to_string(),
            kind: ActionKind::RunTool,
            reason: "test".to_string(),
            evidence_refs: vec![],
            input: serde_json::json!({
                "tool": tool,
                "inputFile": input_file,
            }),
            risk: ActionRisk::SafeReadOnly,
            fingerprint: format!("run_tool:{tool}"),
        }
    }

    fn manifest() -> Manifest {
        Manifest {
            upload_id: "upl_1".to_string(),
            upload_ids: vec!["upl_1".to_string()],
            uploads: vec![ManifestUpload {
                upload_id: "upl_1".to_string(),
                filename: "sample.log".to_string(),
                size: 12,
                raw_path: "raw/upl_1/sample.log".to_string(),
                extracted_dir: "extracted/sample".to_string(),
            }],
            task_id: "task_1".to_string(),
            source: TaskSource::Upload,
            filename: "sample.log".to_string(),
            source_url: None,
            files: vec![ManifestFile {
                path: "sample.log".to_string(),
                size: 12,
            }],
        }
    }

    fn grep() -> GrepResults {
        GrepResults {
            keywords: vec!["query".to_string()],
            total_matches: 1,
            matches: vec![GrepMatch {
                file: "sample.log".to_string(),
                line: 1,
                keyword: "query".to_string(),
                text: "query failed".to_string(),
            }],
        }
    }

    struct Fixture {
        root: PathBuf,
        workspace: PathBuf,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "logagent-{name}-{}",
                Utc::now().timestamp_nanos_opt().unwrap()
            ));
            let workspace = root.join("workspace");
            std::fs::create_dir_all(workspace.join("extracted")).unwrap();
            std::fs::write(workspace.join("extracted/sample.log"), "ERROR sample\n").unwrap();
            Self { root, workspace }
        }

        fn write_tool(&self, filename: &str, content: &str) -> PathBuf {
            let path = self.root.join(filename);
            std::fs::write(&path, content).unwrap();
            let mut permissions = std::fs::metadata(&path).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&path, permissions).unwrap();
            path
        }

        fn context(&self) -> TaskContext {
            TaskContext {
                task_id: "task_1".to_string(),
                source: TaskSource::Upload,
                product: None,
                version: None,
                instance_id: None,
                cluster_id: None,
                node_id: None,
                question: "test".to_string(),
                workspace: self.workspace.clone(),
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}
