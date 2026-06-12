use std::{path::Path, time::Instant};

use anyhow::Context;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::{io::AsyncWriteExt, process::Command, time::timeout};
use tracing::{error, info, warn};

use crate::{
    domain::models::GrepResults,
    services::{
        agent_contracts::write_json_atomic,
        llm_gateway::{validate_final_answer_with_evidence, FinalAnswerDecision},
        tool_runner::ToolRunRecord,
    },
    support::{
        config::{AnalysisMode, ClaudeCodeSettings, PermissionProfileSettings},
        error::AppError,
    },
};

const CLAUDE_PROMPT_PATH: &str = "claude_prompt.md";

#[derive(Debug, Clone)]
pub struct AgentBackendRegistry {
    settings: ClaudeCodeSettings,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentBackendsSummary {
    pub default_backend: String,
    pub backends: Vec<AgentBackendSummary>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentBackendSummary {
    pub id: String,
    pub backend_type: String,
    pub enabled: bool,
    pub default_backend: bool,
    pub command_configured: bool,
    pub timeout_seconds: u64,
    pub max_input_bytes: usize,
    pub max_output_bytes: usize,
    pub execution_mode: String,
    pub default_mode: String,
    pub permission_profile: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentBackendDiagnosticResult {
    pub backend_id: String,
    pub backend_type: String,
    pub enabled: bool,
    pub status: String,
    pub execution_mode: String,
    pub details: Vec<String>,
}

pub struct AgentBackendDecisionInput<'a> {
    pub workspace: &'a Path,
    pub analysis_mode: AnalysisMode,
    pub grep_results: &'a GrepResults,
    pub case_context: Option<&'a serde_json::Value>,
    pub tool_results: &'a [ToolRunRecord],
}

#[derive(Debug, Clone)]
pub enum ClaudeSessionOutcome {
    FinalAnswer { result: FinalAnswerDecision },
    WaitingForUser { prompt: ClaudeUserPrompt },
    WaitingForApproval { approval: ClaudeApprovalRequest },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeUserPrompt {
    #[serde(default)]
    pub question_id: Option<String>,
    pub question: String,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default = "default_required")]
    pub required: bool,
    #[serde(default)]
    pub answer_format: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeApprovalRequest {
    #[serde(default)]
    pub action_id: Option<String>,
    #[serde(default = "default_approval_action_type")]
    pub action_type: String,
    pub reason: String,
    #[serde(default)]
    pub input: serde_json::Value,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeStructuredOutput {
    pub runtime_status: String,
    #[serde(default)]
    pub final_answer: Option<FinalAnswerDecision>,
    #[serde(default)]
    pub pending_prompt: Option<ClaudeUserPrompt>,
    #[serde(default)]
    pub pending_approval: Option<ClaudeApprovalRequest>,
}

impl AgentBackendRegistry {
    pub fn new(settings: ClaudeCodeSettings) -> Self {
        Self { settings }
    }

    pub fn summary(&self) -> AgentBackendsSummary {
        let profile = self
            .settings
            .permission_profiles
            .get(&self.settings.default_mode)
            .map(|profile| profile.name.clone())
            .unwrap_or_else(|| self.settings.default_mode.as_str().to_string());
        AgentBackendsSummary {
            default_backend: "claude_code".to_string(),
            backends: vec![AgentBackendSummary {
                id: "claude_code".to_string(),
                backend_type: "claude_code_cli".to_string(),
                enabled: true,
                default_backend: true,
                command_configured: true,
                timeout_seconds: self.settings.max_session_seconds,
                max_input_bytes: 0,
                max_output_bytes: self.settings.max_output_bytes,
                execution_mode: "claude_code_mcp_session".to_string(),
                default_mode: self.settings.default_mode.as_str().to_string(),
                permission_profile: profile,
            }],
        }
    }

    pub async fn test_backend(
        &self,
        backend_id: &str,
    ) -> anyhow::Result<AgentBackendDiagnosticResult> {
        if backend_id != "claude_code" {
            anyhow::bail!("unknown Claude Code backend {backend_id}");
        }
        let metadata = tokio::fs::metadata(&self.settings.command_path)
            .await
            .map_err(|error| {
                anyhow::anyhow!(
                    "failed to inspect Claude Code command {}: {error}",
                    self.settings.command_path.display()
                )
            })?;
        if !metadata.is_file() {
            anyhow::bail!(
                "Claude Code command {} is not a regular file",
                self.settings.command_path.display()
            );
        }
        Ok(AgentBackendDiagnosticResult {
            backend_id: "claude_code".to_string(),
            backend_type: "claude_code_cli".to_string(),
            enabled: true,
            status: "configured".to_string(),
            execution_mode: "claude_code_mcp_session".to_string(),
            details: vec![
                "Command path exists. PLAN_ANALYSIS invokes Claude Code with --mcp-config."
                    .to_string(),
                format!(
                    "Default mode={}, timeout={}s, maxOutputBytes={}.",
                    self.settings.default_mode.as_str(),
                    self.settings.max_session_seconds,
                    self.settings.max_output_bytes
                ),
            ],
        })
    }

    pub async fn decide_next(
        &self,
        input: AgentBackendDecisionInput<'_>,
    ) -> anyhow::Result<ClaudeSessionOutcome> {
        let profile = self
            .settings
            .permission_profiles
            .get(&input.analysis_mode)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "analysis mode {} has no Claude Code permission profile",
                    input.analysis_mode.as_str()
                )
            })?;
        let started = Instant::now();
        let resume_session_id = read_existing_claude_session_id(input.workspace).await?;
        info!(
            workspace = %input.workspace.display(),
            analysis_mode = %input.analysis_mode.as_str(),
            resume_session_id = ?resume_session_id,
            "starting Claude Code session"
        );
        write_claude_session_state(
            input.workspace,
            input.analysis_mode,
            profile,
            resume_session_id.as_deref(),
            "starting",
            None,
            None,
        )
        .await?;
        let result = run_claude_code_command(
            &self.settings.command_path,
            &self.settings,
            input.analysis_mode,
            profile,
            input.workspace,
            resume_session_id.as_deref(),
        )
        .await;
        let duration_ms = started.elapsed().as_millis() as u64;
        match result {
            Ok(stdout) => {
                let parsed = parse_claude_session_output(&stdout)
                    .context("Claude Code stdout did not contain a valid structured outcome");
                match parsed {
                    Ok((outcome, raw_response, structured_output)) => {
                        if let ClaudeSessionOutcome::FinalAnswer { result } = &outcome {
                            if let Err(error) = validate_final_answer_with_evidence(
                                result,
                                input.grep_results,
                                input.case_context,
                                input.tool_results,
                            ) {
                                write_failed_agent_response(
                                    input.workspace,
                                    input.analysis_mode,
                                    profile,
                                    duration_ms,
                                    &error.to_string(),
                                    Some(&stdout),
                                )
                                .await?;
                                write_claude_session_state(
                                    input.workspace,
                                    input.analysis_mode,
                                    profile,
                                    resume_session_id.as_deref(),
                                    "failed",
                                    Some(duration_ms),
                                    Some(&error.to_string()),
                                )
                                .await?;
                                error!(
                                    workspace = %input.workspace.display(),
                                    analysis_mode = %input.analysis_mode.as_str(),
                                    duration_ms,
                                    error = %error,
                                    "Claude Code final answer failed evidence validation"
                                );
                                return Err(error);
                            }
                        }
                        let claude_session_id =
                            raw_session_id(&raw_response).or_else(|| resume_session_id.clone());
                        write_success_agent_response(
                            input.workspace,
                            input.analysis_mode,
                            profile,
                            duration_ms,
                            claude_session_id.as_deref(),
                            &raw_response,
                            &structured_output,
                        )
                        .await?;
                        write_claude_session_state(
                            input.workspace,
                            input.analysis_mode,
                            profile,
                            claude_session_id.as_deref(),
                            "succeeded",
                            Some(duration_ms),
                            None,
                        )
                        .await?;
                        info!(
                            workspace = %input.workspace.display(),
                            analysis_mode = %input.analysis_mode.as_str(),
                            claude_session_id = ?claude_session_id,
                            runtime_status = %structured_output.runtime_status,
                            duration_ms,
                            "Claude Code session completed"
                        );
                        Ok(outcome)
                    }
                    Err(error) => {
                        write_failed_agent_response(
                            input.workspace,
                            input.analysis_mode,
                            profile,
                            duration_ms,
                            &format!("{error:#}"),
                            Some(&stdout),
                        )
                        .await?;
                        write_claude_session_state(
                            input.workspace,
                            input.analysis_mode,
                            profile,
                            resume_session_id.as_deref(),
                            "failed",
                            Some(duration_ms),
                            Some(&format!("{error:#}")),
                        )
                        .await?;
                        error!(
                            workspace = %input.workspace.display(),
                            analysis_mode = %input.analysis_mode.as_str(),
                            duration_ms,
                            "Claude Code output parsing failed"
                        );
                        Err(error)
                    }
                }
            }
            Err(error) => {
                write_failed_agent_response(
                    input.workspace,
                    input.analysis_mode,
                    profile,
                    duration_ms,
                    &format!("{error:#}"),
                    None,
                )
                .await?;
                write_claude_session_state(
                    input.workspace,
                    input.analysis_mode,
                    profile,
                    resume_session_id.as_deref(),
                    "failed",
                    Some(duration_ms),
                    Some(&format!("{error:#}")),
                )
                .await?;
                error!(
                    workspace = %input.workspace.display(),
                    analysis_mode = %input.analysis_mode.as_str(),
                    duration_ms,
                    "Claude Code session failed"
                );
                Err(error)
            }
        }
    }
}

async fn run_claude_code_command(
    command_path: &Path,
    settings: &ClaudeCodeSettings,
    analysis_mode: AnalysisMode,
    profile: &PermissionProfileSettings,
    workspace: &Path,
    resume_session_id: Option<&str>,
) -> anyhow::Result<String> {
    let prompt = build_claude_code_prompt(analysis_mode, profile)
        .await
        .context("failed to build Claude Code prompt")?;
    let prompt_bytes = prompt.len() as u64;
    tokio::fs::write(workspace.join(CLAUDE_PROMPT_PATH), &prompt)
        .await
        .context("failed to write Claude Code prompt artifact")?;
    info!(
        command = %command_path.display(),
        workspace = %workspace.display(),
        analysis_mode = %analysis_mode.as_str(),
        permission_profile = %profile.name,
        resume_session_id = ?resume_session_id,
        timeout_seconds = settings.max_session_seconds,
        prompt_bytes,
        "spawning Claude Code CLI"
    );
    let mut command = Command::new(command_path);
    command
        .arg("--print")
        .arg("--output-format")
        .arg("json")
        .arg("--json-schema")
        .arg(claude_session_json_schema().to_string())
        .arg("--mcp-config")
        .arg("claude_mcp_config.json")
        .arg("--strict-mcp-config")
        .arg("--permission-mode")
        .arg(&profile.permission_mode)
        .arg("--tools")
        .arg(&profile.tools);
    if !profile.allowed_tools.is_empty() {
        command
            .arg("--allowedTools")
            .arg(profile.allowed_tools.join(","));
    }
    if !profile.disallowed_tools.is_empty() {
        command
            .arg("--disallowedTools")
            .arg(profile.disallowed_tools.join(","));
    }
    if let Some(session_id) = resume_session_id {
        command.arg("--resume").arg(session_id);
    }
    command
        .current_dir(workspace)
        .kill_on_drop(true)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = command
        .spawn()
        .with_context(|| format!("failed to spawn Claude Code CLI {}", command_path.display()))?;
    let mut stdin = child
        .stdin
        .take()
        .context("failed to open Claude Code stdin")?;
    stdin
        .write_all(prompt.as_bytes())
        .await
        .context("failed to write Claude Code prompt to stdin")?;
    drop(stdin);
    let output = timeout(
        std::time::Duration::from_secs(settings.max_session_seconds),
        child.wait_with_output(),
    )
    .await
    .with_context(|| {
        format!(
            "Claude Code session timed out after {} seconds",
            settings.max_session_seconds
        )
    })?
    .context("failed to wait for Claude Code session")?;
    if output.stdout.len() > settings.max_output_bytes {
        warn!(
            stdout_bytes = output.stdout.len(),
            max_output_bytes = settings.max_output_bytes,
            "Claude Code stdout exceeded configured limit"
        );
        anyhow::bail!(
            "Claude Code stdout exceeded {} bytes",
            settings.max_output_bytes
        );
    }
    if !output.status.success() {
        error!(
            status = %output.status,
            stderr_bytes = output.stderr.len(),
            "Claude Code CLI exited unsuccessfully"
        );
        anyhow::bail!(
            "Claude Code exited with status {} (stderrBytes={})",
            output.status,
            output.stderr.len()
        );
    }
    String::from_utf8(output.stdout).context("Claude Code stdout is not valid UTF-8")
}

async fn build_claude_code_prompt(
    analysis_mode: AnalysisMode,
    profile: &PermissionProfileSettings,
) -> anyhow::Result<String> {
    Ok(format!(
        r#"You are Claude Code running as the LogAgent domain diagnostic enhancement layer.

Use LogAgent MCP resources and tools for task evidence. Do not invent evidence refs. System Context, diagnostic skills, and skill_references/* are background only and must not be cited as final root cause evidence. Historical Cases can guide analysis, but current-task evidence must support final conclusions.

Mode: {mode}
Permission profile: {profile}
Native Bash allowed: {native_bash}
Native Edit allowed: {native_edit}

Before analyzing, call MCP resources/list and then read the resource named analysis_package. Use that task package as the primary context. Read manifest, grep_results, metadata_context, system_context, case_context, and tool_results resources as needed instead of relying on this startup prompt for evidence.

Return exactly one JSON object matching the schema:
- runtimeStatus="completed" with finalAnswer when analysis can finish.
- runtimeStatus="waiting_for_user" with pendingPrompt when user information is required.
- runtimeStatus="waiting_for_approval" with pendingApproval when an approval-gated action is required.

The finalAnswer fields are summary, symptoms, likelyRootCauses, nextChecks, fixSuggestions, missingInformation, confidence. Final root cause evidence refs may use session_text_input.json#question, grep_results.json#matches/<index>, case_context.json#cases/<index>, or tool_results/<action_id>/result.json#findings/<index>. Do not use system_context.json, diagnostic_skill, or skill_references/* refs as final root cause evidence.
"#,
        mode = analysis_mode.as_str(),
        profile = profile.name,
        native_bash = profile.native_bash,
        native_edit = profile.native_edit,
    ))
}

fn claude_session_json_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "runtimeStatus": { "enum": ["completed", "waiting_for_user", "waiting_for_approval"] },
            "finalAnswer": final_answer_schema(),
            "pendingPrompt": {
                "type": "object",
                "properties": {
                    "questionId": { "type": "string" },
                    "question": { "type": "string" },
                    "reason": { "type": "string" },
                    "required": { "type": "boolean" },
                    "answerFormat": { "type": "string" }
                },
                "required": ["question"],
                "additionalProperties": true
            },
            "pendingApproval": {
                "type": "object",
                "properties": {
                    "actionId": { "type": "string" },
                    "actionType": { "type": "string" },
                    "reason": { "type": "string" },
                    "input": { "type": "object" },
                    "evidenceRefs": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["reason"],
                "additionalProperties": true
            }
        },
        "required": ["runtimeStatus"],
        "additionalProperties": true
    })
}

fn final_answer_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "summary": { "type": "string" },
            "symptoms": { "type": "array", "items": { "type": "string" } },
            "likelyRootCauses": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "cause": { "type": "string" },
                        "evidenceRefs": { "type": "array", "items": { "type": "string" } }
                    },
                    "required": ["cause", "evidenceRefs"],
                    "additionalProperties": true
                }
            },
            "nextChecks": { "type": "array", "items": { "type": "string" } },
            "fixSuggestions": { "type": "array", "items": { "type": "string" } },
            "missingInformation": { "type": "array", "items": { "type": "string" } },
            "confidence": { "enum": ["low", "medium", "high"] }
        },
        "required": ["summary", "symptoms", "likelyRootCauses", "nextChecks", "fixSuggestions", "missingInformation", "confidence"],
        "additionalProperties": true
    })
}

fn parse_claude_session_output(
    stdout: &str,
) -> anyhow::Result<(
    ClaudeSessionOutcome,
    serde_json::Value,
    ClaudeStructuredOutput,
)> {
    let raw_response: serde_json::Value =
        serde_json::from_str(stdout.trim()).context("Claude Code stdout is not JSON")?;
    if raw_response
        .get("is_error")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        anyhow::bail!(
            "Claude Code returned an error result: {}",
            raw_response
                .get("result")
                .and_then(|value| value.as_str())
                .unwrap_or("missing result")
        );
    }
    let candidate = raw_response
        .get("structured_output")
        .filter(|value| !value.is_null())
        .or_else(|| raw_response.get("structuredOutput"))
        .filter(|value| !value.is_null())
        .or_else(|| raw_response.get("result"))
        .filter(|value| !value.is_null())
        .unwrap_or(&raw_response);
    let content = match candidate {
        serde_json::Value::String(value) => strip_json_code_fence(value).to_string(),
        value => serde_json::to_string(value)?,
    };
    let structured: ClaudeStructuredOutput =
        serde_json::from_str(&content).context("invalid Claude structured output")?;
    let status = structured.runtime_status.as_str();
    let outcome = match status {
        "completed" | "succeeded" | "final_answer" => ClaudeSessionOutcome::FinalAnswer {
            result: structured
                .final_answer
                .clone()
                .ok_or_else(|| anyhow::anyhow!("completed Claude output is missing finalAnswer"))?,
        },
        "waiting_for_user" => ClaudeSessionOutcome::WaitingForUser {
            prompt: structured.pending_prompt.clone().ok_or_else(|| {
                anyhow::anyhow!("waiting_for_user output is missing pendingPrompt")
            })?,
        },
        "waiting_for_approval" => ClaudeSessionOutcome::WaitingForApproval {
            approval: structured.pending_approval.clone().ok_or_else(|| {
                anyhow::anyhow!("waiting_for_approval output is missing pendingApproval")
            })?,
        },
        value => anyhow::bail!("unsupported Claude runtimeStatus {value}"),
    };
    Ok((outcome, raw_response, structured))
}

fn strip_json_code_fence(value: &str) -> &str {
    let trimmed = value.trim();
    let Some(rest) = trimmed.strip_prefix("```") else {
        return trimmed;
    };
    let rest = rest.strip_prefix("json").unwrap_or(rest).trim_start();
    rest.strip_suffix("```").unwrap_or(rest).trim()
}

async fn read_existing_claude_session_id(workspace: &Path) -> anyhow::Result<Option<String>> {
    let path = workspace.join("claude_session.json");
    let raw = match tokio::fs::read_to_string(path).await {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err.into()),
    };
    let value: serde_json::Value = serde_json::from_str(&raw)?;
    Ok(value
        .get("claudeSessionId")
        .and_then(|value| value.as_str())
        .map(ToString::to_string))
}

async fn write_claude_session_state(
    workspace: &Path,
    analysis_mode: AnalysisMode,
    profile: &PermissionProfileSettings,
    claude_session_id: Option<&str>,
    runtime_status: &str,
    duration_ms: Option<u64>,
    error: Option<&str>,
) -> Result<(), AppError> {
    let prompt_delivery = prompt_delivery_metadata(workspace).await;
    let value = serde_json::json!({
        "schemaVersion": 1,
        "generatedAt": Utc::now(),
        "runtimeStatus": runtime_status,
        "claudeSessionId": claude_session_id,
        "analysisMode": analysis_mode,
        "permissionProfile": profile.name,
        "mcpConfigPath": "claude_mcp_config.json",
        "lastClaudeResponsePath": "agent_response.json",
        "promptDelivery": prompt_delivery,
        "durationMs": duration_ms,
        "nativeToolPolicy": native_tool_policy(profile),
        "error": error,
    });
    write_json_atomic(workspace.join("claude_session.json"), &value).await
}

async fn write_success_agent_response(
    workspace: &Path,
    analysis_mode: AnalysisMode,
    profile: &PermissionProfileSettings,
    duration_ms: u64,
    claude_session_id: Option<&str>,
    raw_response: &serde_json::Value,
    structured_output: &ClaudeStructuredOutput,
) -> Result<(), AppError> {
    let prompt_delivery = prompt_delivery_metadata(workspace).await;
    let response = serde_json::json!({
        "schemaVersion": 2,
        "generatedAt": Utc::now(),
        "runtimeStatus": "succeeded",
        "claudeSessionId": claude_session_id,
        "analysisMode": analysis_mode,
        "permissionProfile": profile.name,
        "promptDelivery": prompt_delivery,
        "structuredOutput": structured_output,
        "usage": raw_response.get("usage").cloned().unwrap_or(serde_json::Value::Null),
        "cost": raw_response
            .get("cost")
            .cloned()
            .or_else(|| {
                raw_response
                    .get("total_cost_usd")
                    .cloned()
                    .map(|usd| serde_json::json!({ "usd": usd }))
            })
            .unwrap_or(serde_json::Value::Null),
        "mcpCallsPath": "mcp_calls.jsonl",
        "nativeToolPolicy": native_tool_policy(profile),
        "durationMs": duration_ms,
        "error": null,
        "rawStdoutPreview": null,
    });
    write_json_atomic(workspace.join("agent_response.json"), &response).await
}

async fn write_failed_agent_response(
    workspace: &Path,
    analysis_mode: AnalysisMode,
    profile: &PermissionProfileSettings,
    duration_ms: u64,
    error: &str,
    raw_stdout: Option<&str>,
) -> Result<(), AppError> {
    let prompt_delivery = prompt_delivery_metadata(workspace).await;
    let response = serde_json::json!({
        "schemaVersion": 2,
        "generatedAt": Utc::now(),
        "runtimeStatus": "failed",
        "claudeSessionId": null,
        "analysisMode": analysis_mode,
        "permissionProfile": profile.name,
        "promptDelivery": prompt_delivery,
        "structuredOutput": null,
        "usage": null,
        "cost": null,
        "mcpCallsPath": "mcp_calls.jsonl",
        "nativeToolPolicy": native_tool_policy(profile),
        "durationMs": duration_ms,
        "error": error,
        "rawStdoutPreview": raw_stdout.map(|value| truncate_string(value, 16384)),
    });
    write_json_atomic(workspace.join("agent_response.json"), &response).await
}

async fn prompt_delivery_metadata(workspace: &Path) -> serde_json::Value {
    let prompt_bytes = tokio::fs::metadata(workspace.join(CLAUDE_PROMPT_PATH))
        .await
        .ok()
        .map(|metadata| metadata.len());
    serde_json::json!({
        "mode": "stdin_file",
        "promptPath": CLAUDE_PROMPT_PATH,
        "promptBytes": prompt_bytes,
        "largeContextVia": "mcp_resource",
        "analysisPackageResourceName": "analysis_package",
    })
}

fn raw_session_id(raw_response: &serde_json::Value) -> Option<String> {
    raw_response
        .get("session_id")
        .or_else(|| raw_response.get("sessionId"))
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
}

fn native_tool_policy(profile: &PermissionProfileSettings) -> serde_json::Value {
    serde_json::json!({
        "permissionMode": profile.permission_mode,
        "tools": profile.tools,
        "allowedTools": profile.allowed_tools,
        "disallowedTools": profile.disallowed_tools,
        "nativeBash": profile.native_bash,
        "nativeEdit": profile.native_edit,
        "worktreeRequired": profile.worktree_required,
    })
}

fn default_required() -> bool {
    true
}

fn default_approval_action_type() -> String {
    "collect_environment".to_string()
}

fn truncate_string(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        fs,
        io::Write,
        os::unix::fs::PermissionsExt,
        path::PathBuf,
        process,
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use crate::{
        domain::models::{Confidence, GrepMatch, GrepResults},
        support::config::{AnalysisMode, ClaudeCodeSettings, PermissionProfileSettings},
    };

    use super::*;

    const PACKAGE_MARKER: &str = "ANALYSIS_PACKAGE_BODY_SHOULD_NOT_BE_IN_PROMPT";

    #[tokio::test]
    async fn claude_code_session_returns_final_answer() {
        let fixture = Fixture::new();
        let claude = fixture.write_claude(
            r#"#!/usr/bin/env bash
printf '%s\n' "$*" > claude_args.txt
cat > claude_stdin.txt
cat <<'JSON'
{"type":"result","subtype":"success","is_error":false,"session_id":"sess-claude-1","structured_output":{"runtimeStatus":"completed","finalAnswer":{"summary":"direct cli summary","symptoms":["timeout"],"likelyRootCauses":[{"cause":"timeout in logs","evidenceRefs":["grep_results.json#matches/0"]}],"nextChecks":["check timeout"],"fixSuggestions":["increase timeout"],"missingInformation":[],"confidence":"medium"}},"usage":{"input_tokens":22},"total_cost_usd":0.02}
JSON
"#,
        );
        let outcome = fixture
            .registry(claude)
            .decide_next(fixture.input(AnalysisMode::Diagnose))
            .await
            .unwrap();

        match outcome {
            ClaudeSessionOutcome::FinalAnswer { result } => {
                assert_eq!(result.summary, "direct cli summary");
                assert!(matches!(result.confidence, Confidence::Medium));
            }
            _ => panic!("expected final answer"),
        }

        let args = fs::read_to_string(fixture.workspace.join("claude_args.txt")).unwrap();
        assert!(args.contains("--mcp-config claude_mcp_config.json"));
        assert!(args.contains("--strict-mcp-config"));
        assert!(args.contains("--json-schema"));
        assert!(args.contains("--permission-mode dontAsk"));
        assert!(!args.contains(PACKAGE_MARKER));
        let stdin = fs::read_to_string(fixture.workspace.join("claude_stdin.txt")).unwrap();
        assert!(stdin.contains("resources/list"));
        assert!(stdin.contains("analysis_package"));
        assert!(!stdin.contains(PACKAGE_MARKER));
        let prompt = fs::read_to_string(fixture.workspace.join(CLAUDE_PROMPT_PATH)).unwrap();
        assert_eq!(stdin, prompt);
        let response: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(fixture.workspace.join("agent_response.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(response["runtimeStatus"], "succeeded");
        assert_eq!(response["claudeSessionId"], "sess-claude-1");
        assert_eq!(response["promptDelivery"]["mode"], "stdin_file");
        assert_eq!(response["promptDelivery"]["promptPath"], CLAUDE_PROMPT_PATH);
        assert_eq!(
            response["promptDelivery"]["largeContextVia"],
            "mcp_resource"
        );
        assert_eq!(response["structuredOutput"]["runtimeStatus"], "completed");
        assert_eq!(response["usage"]["input_tokens"], 22);
        assert_eq!(response["cost"]["usd"], 0.02);
        assert!(fixture.workspace.join("claude_session.json").exists());
    }

    #[tokio::test]
    async fn claude_code_session_keeps_large_package_out_of_cli_prompt() {
        let fixture = Fixture::new();
        fs::write(
            fixture.workspace.join("analysis_package.json"),
            format!(
                r#"{{"schemaVersion":2,"payload":"{}"}}"#,
                "x".repeat(2 * 1024 * 1024)
            ),
        )
        .unwrap();
        let claude = fixture.write_claude(
            r#"#!/usr/bin/env bash
printf '%s\n' "$*" > claude_args.txt
cat > claude_stdin.txt
cat <<'JSON'
{"structured_output":{"runtimeStatus":"completed","finalAnswer":{"summary":"large package summary","symptoms":["timeout"],"likelyRootCauses":[{"cause":"timeout in logs","evidenceRefs":["grep_results.json#matches/0"]}],"nextChecks":["check timeout"],"fixSuggestions":[],"missingInformation":[],"confidence":"low"}}}
JSON
"#,
        );

        let outcome = fixture
            .registry(claude)
            .decide_next(fixture.input(AnalysisMode::Diagnose))
            .await
            .unwrap();

        assert!(matches!(outcome, ClaudeSessionOutcome::FinalAnswer { .. }));
        let args = fs::read_to_string(fixture.workspace.join("claude_args.txt")).unwrap();
        let stdin = fs::read_to_string(fixture.workspace.join("claude_stdin.txt")).unwrap();
        assert!(args.len() < 16 * 1024);
        assert!(stdin.len() < 16 * 1024);
        assert!(stdin.contains("analysis_package"));
        assert!(!stdin.contains(&"x".repeat(1024)));
    }

    #[tokio::test]
    async fn claude_code_session_returns_pending_prompt() {
        let fixture = Fixture::new();
        let claude = fixture.write_claude(
            r#"#!/usr/bin/env bash
cat > /dev/null
cat <<'JSON'
{"structured_output":{"runtimeStatus":"waiting_for_user","pendingPrompt":{"questionId":"q1","question":"Which version?","reason":"need version","required":true,"answerFormat":"semver"}}}
JSON
"#,
        );
        let outcome = fixture
            .registry(claude)
            .decide_next(fixture.input(AnalysisMode::Diagnose))
            .await
            .unwrap();

        match outcome {
            ClaudeSessionOutcome::WaitingForUser { prompt } => {
                assert_eq!(prompt.question_id.as_deref(), Some("q1"));
                assert_eq!(prompt.question, "Which version?");
            }
            _ => panic!("expected pending prompt"),
        }
    }

    #[tokio::test]
    async fn claude_code_session_rejects_invalid_evidence_ref() {
        let fixture = Fixture::new();
        let claude = fixture.write_claude(
            r#"#!/usr/bin/env bash
cat > /dev/null
cat <<'JSON'
{"structured_output":{"runtimeStatus":"completed","finalAnswer":{"summary":"bad evidence","symptoms":["timeout"],"likelyRootCauses":[{"cause":"bad","evidenceRefs":["system_context.json#resources/0"]}],"nextChecks":[],"fixSuggestions":[],"missingInformation":[],"confidence":"low"}}}
JSON
"#,
        );
        let error = fixture
            .registry(claude)
            .decide_next(fixture.input(AnalysisMode::Diagnose))
            .await
            .unwrap_err()
            .to_string();

        assert!(error.contains("invalid evidence ref"));
        let response: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(fixture.workspace.join("agent_response.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(response["runtimeStatus"], "failed");
    }

    struct Fixture {
        root: PathBuf,
        workspace: PathBuf,
        grep: GrepResults,
    }

    static NEXT_FIXTURE_ID: AtomicU64 = AtomicU64::new(0);

    impl Fixture {
        fn new() -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let id = NEXT_FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "logagent-claude-code-{}-{id}-{nanos}",
                process::id()
            ));
            let workspace = root.join("workspace");
            fs::create_dir_all(&workspace).unwrap();
            fs::write(
                workspace.join("analysis_package.json"),
                format!(r#"{{"marker":"{PACKAGE_MARKER}"}}"#),
            )
            .unwrap();
            fs::write(workspace.join("claude_mcp_config.json"), "{}").unwrap();
            fs::write(
                workspace.join("analysis_state.json"),
                r#"{"userMessages":[]}"#,
            )
            .unwrap();
            Self {
                root,
                workspace,
                grep: GrepResults {
                    keywords: vec!["timeout".to_string()],
                    total_matches: 1,
                    matches: vec![GrepMatch {
                        file: "sample.log".to_string(),
                        line: 10,
                        keyword: "timeout".to_string(),
                        text: "ERROR timeout".to_string(),
                    }],
                },
            }
        }

        fn registry(&self, claude: PathBuf) -> AgentBackendRegistry {
            AgentBackendRegistry::new(ClaudeCodeSettings {
                command_path: claude,
                default_mode: AnalysisMode::Diagnose,
                max_session_seconds: 5,
                max_output_bytes: 1024 * 1024,
                permission_profiles: BTreeMap::from([(
                    AnalysisMode::Diagnose,
                    PermissionProfileSettings {
                        name: "diagnose".to_string(),
                        permission_mode: "dontAsk".to_string(),
                        tools: String::new(),
                        allowed_tools: Vec::new(),
                        disallowed_tools: vec!["Bash".to_string(), "Edit".to_string()],
                        native_bash: false,
                        native_edit: false,
                        worktree_required: false,
                    },
                )]),
            })
        }

        fn input(&self, analysis_mode: AnalysisMode) -> AgentBackendDecisionInput<'_> {
            AgentBackendDecisionInput {
                workspace: &self.workspace,
                analysis_mode,
                grep_results: &self.grep,
                case_context: None,
                tool_results: &[],
            }
        }

        fn write_claude(&self, content: &str) -> PathBuf {
            let path = self.root.join("claude");
            let mut file = fs::File::create(&path).unwrap();
            file.write_all(content.as_bytes()).unwrap();
            file.sync_all().unwrap();
            drop(file);
            let mut permissions = fs::metadata(&path).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&path, permissions).unwrap();
            std::thread::sleep(Duration::from_millis(10));
            path
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
