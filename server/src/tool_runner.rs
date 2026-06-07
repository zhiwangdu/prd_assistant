use std::{
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
            let Some(input_file) = select_input_file(tool, manifest, grep) else {
                continue;
            };
            let action_id = format!("act_tool_{}", safe_action_suffix(&tool.name));
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

fn select_input_file(
    tool: &ToolSettings,
    manifest: &Manifest,
    grep: &GrepResults,
) -> Option<String> {
    let matched_file = manifest.files.iter().find(|file| {
        tool.match_settings
            .file_patterns
            .iter()
            .any(|pattern| matches_pattern(pattern, &file.path.to_ascii_lowercase()))
    });
    if let Some(file) = matched_file {
        return Some(format!("extracted/{}", file.path));
    }

    let keyword_match = grep.matches.iter().any(|entry| {
        let text = entry.text.to_ascii_lowercase();
        tool.match_settings
            .keywords
            .iter()
            .any(|keyword| text.contains(keyword))
    });
    if keyword_match {
        return manifest
            .files
            .first()
            .map(|file| format!("extracted/{}", file.path));
    }
    None
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
        let input = actions[0].decode_input::<ToolActionInput>().unwrap();
        assert_eq!(input.tool, "fake");
        assert_eq!(input.input_file.as_deref(), Some("extracted/sample.log"));
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
