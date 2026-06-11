use std::{
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::Context;
use chrono::Utc;
use serde::Serialize;
use tokio::{process::Command, time::timeout};

use crate::{
    domain::models::GrepResults,
    services::{
        agent_contracts::write_json_atomic,
        llm_gateway::{
            parse_action_decision_content, validate_agent_decision_with_evidence, AgentDecision,
        },
        tool_runner::ToolRunRecord,
    },
    support::{
        config::{AgentBackendSettings, AgentBackendSettingsEntry, AgentBackendType},
        error::AppError,
    },
};

#[derive(Debug, Clone)]
pub struct AgentBackendRegistry {
    settings: AgentBackendSettings,
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
    pub grep_results: &'a GrepResults,
    pub case_context: Option<&'a serde_json::Value>,
    pub tool_results: &'a [ToolRunRecord],
}

impl AgentBackendRegistry {
    pub fn new(settings: AgentBackendSettings) -> Self {
        Self { settings }
    }

    pub fn summary(&self) -> AgentBackendsSummary {
        AgentBackendsSummary {
            default_backend: self.settings.default_backend.clone(),
            backends: self
                .settings
                .backends
                .values()
                .map(|backend| self.backend_summary(backend))
                .collect(),
        }
    }

    pub async fn test_backend(
        &self,
        backend_id: &str,
    ) -> anyhow::Result<AgentBackendDiagnosticResult> {
        let backend = self
            .settings
            .backends
            .get(backend_id)
            .ok_or_else(|| anyhow::anyhow!("unknown agent backend {backend_id}"))?;
        if !backend.enabled {
            anyhow::bail!("agent backend {backend_id} is disabled");
        }
        match backend.backend_type {
            AgentBackendType::CodexCli
            | AgentBackendType::ClaudeCodeCli
            | AgentBackendType::ClaudeAgentSdk
            | AgentBackendType::OpencodeCli => {
                let command_path = backend.command_path.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("agent backend {backend_id} has no configured command path")
                })?;
                let metadata = tokio::fs::metadata(command_path).await.map_err(|error| {
                    anyhow::anyhow!(
                        "failed to inspect agent backend command {}: {error}",
                        command_path.display()
                    )
                })?;
                if !metadata.is_file() {
                    anyhow::bail!(
                        "agent backend command {} is not a regular file",
                        command_path.display()
                    );
                }
                Ok(AgentBackendDiagnosticResult {
                    backend_id: backend.name.clone(),
                    backend_type: backend.backend_type.as_str().to_string(),
                    enabled: true,
                    status: "configured".to_string(),
                    execution_mode: backend.backend_type.execution_mode().to_string(),
                    details: vec![
                        "Command path exists. Log analysis runtime invokes the adapter during PLAN_ANALYSIS."
                            .to_string(),
                        format!(
                            "Limits: timeout={}s, maxInputBytes={}, maxOutputBytes={}.",
                            backend.timeout_seconds,
                            backend.max_input_bytes,
                            backend.max_output_bytes
                        ),
                    ],
                })
            }
        }
    }

    pub async fn decide_next(
        &self,
        input: AgentBackendDecisionInput<'_>,
    ) -> anyhow::Result<AgentDecision> {
        let backend = self
            .settings
            .backends
            .get(&self.settings.default_backend)
            .ok_or_else(|| anyhow::anyhow!("default agent backend is not configured"))?;
        if !backend.enabled {
            anyhow::bail!(
                "default agent backend {} is disabled",
                self.settings.default_backend
            );
        }
        match backend.backend_type {
            AgentBackendType::ClaudeAgentSdk => self.invoke_claude_agent_sdk(backend, input).await,
            AgentBackendType::CodexCli
            | AgentBackendType::ClaudeCodeCli
            | AgentBackendType::OpencodeCli => {
                let error = format!(
                    "agent backend type {} is configured but not supported for Log Analysis runtime; configure claude_agent_sdk",
                    backend.backend_type.as_str()
                );
                write_failed_agent_response(input.workspace, backend, 0, &error).await?;
                anyhow::bail!(error)
            }
        }
    }

    async fn invoke_claude_agent_sdk(
        &self,
        backend: &AgentBackendSettingsEntry,
        input: AgentBackendDecisionInput<'_>,
    ) -> anyhow::Result<AgentDecision> {
        let command_path = backend
            .command_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("claude_agent_sdk backend has no command path"))?;
        ensure_input_size(input.workspace, backend)?;
        let started = Instant::now();
        let result = run_adapter_command(command_path, backend, input.workspace).await;
        let duration_ms = started.elapsed().as_millis() as u64;
        match result {
            Ok(stdout) => {
                let parsed = parse_adapter_decision(&stdout)
                    .context("claude_agent_sdk adapter stdout did not contain a valid decision");
                match parsed {
                    Ok((decision, raw_response)) => {
                        if let Err(error) = validate_agent_decision_with_evidence(
                            &decision,
                            input.grep_results,
                            input.case_context,
                            input.tool_results,
                        ) {
                            write_failed_agent_response(
                                input.workspace,
                                backend,
                                duration_ms,
                                &error.to_string(),
                            )
                            .await?;
                            return Err(error);
                        }
                        write_success_agent_response(
                            input.workspace,
                            backend,
                            duration_ms,
                            &raw_response,
                            &decision,
                        )
                        .await?;
                        Ok(decision)
                    }
                    Err(error) => {
                        write_failed_agent_response(
                            input.workspace,
                            backend,
                            duration_ms,
                            &error.to_string(),
                        )
                        .await?;
                        Err(error)
                    }
                }
            }
            Err(error) => {
                write_failed_agent_response(
                    input.workspace,
                    backend,
                    duration_ms,
                    &error.to_string(),
                )
                .await?;
                Err(error)
            }
        }
    }

    fn backend_summary(&self, backend: &AgentBackendSettingsEntry) -> AgentBackendSummary {
        AgentBackendSummary {
            id: backend.name.clone(),
            backend_type: backend.backend_type.as_str().to_string(),
            enabled: backend.enabled,
            default_backend: backend.name == self.settings.default_backend,
            command_configured: backend.command_path.is_some(),
            timeout_seconds: backend.timeout_seconds,
            max_input_bytes: backend.max_input_bytes,
            max_output_bytes: backend.max_output_bytes,
            execution_mode: backend.backend_type.execution_mode().to_string(),
        }
    }
}

fn ensure_input_size(workspace: &Path, backend: &AgentBackendSettingsEntry) -> anyhow::Result<()> {
    let total = file_len(workspace.join("analysis_package.json"))?
        + file_len(workspace.join("agent_request.json"))?;
    if total > backend.max_input_bytes as u64 {
        anyhow::bail!(
            "agent backend input exceeded {} bytes: {total}",
            backend.max_input_bytes
        );
    }
    Ok(())
}

fn file_len(path: PathBuf) -> anyhow::Result<u64> {
    Ok(std::fs::metadata(&path)
        .with_context(|| format!("failed to inspect {}", path.display()))?
        .len())
}

async fn run_adapter_command(
    command_path: &Path,
    backend: &AgentBackendSettingsEntry,
    workspace: &Path,
) -> anyhow::Result<String> {
    let child = Command::new(command_path)
        .arg("run")
        .arg("--request")
        .arg("agent_request.json")
        .arg("--package")
        .arg("analysis_package.json")
        .current_dir(workspace)
        .kill_on_drop(true)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .with_context(|| {
            format!(
                "failed to spawn claude_agent_sdk adapter {}",
                command_path.display()
            )
        })?;
    let output = timeout(
        std::time::Duration::from_secs(backend.timeout_seconds),
        child.wait_with_output(),
    )
    .await
    .with_context(|| {
        format!(
            "claude_agent_sdk adapter timed out after {} seconds",
            backend.timeout_seconds
        )
    })?
    .context("failed to wait for claude_agent_sdk adapter")?;
    if output.stdout.len() > backend.max_output_bytes {
        anyhow::bail!(
            "claude_agent_sdk adapter stdout exceeded {} bytes",
            backend.max_output_bytes
        );
    }
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(truncate_bytes(&output.stderr, 4096));
        anyhow::bail!(
            "claude_agent_sdk adapter exited with status {}: {}",
            output.status,
            stderr.trim()
        );
    }
    String::from_utf8(output.stdout).context("claude_agent_sdk adapter stdout is not valid UTF-8")
}

fn parse_adapter_decision(stdout: &str) -> anyhow::Result<(AgentDecision, serde_json::Value)> {
    let raw_response: serde_json::Value =
        serde_json::from_str(stdout.trim()).context("adapter stdout is not JSON")?;
    let candidate = raw_response
        .get("normalizedDecision")
        .filter(|value| !value.is_null())
        .or_else(|| raw_response.get("decision"))
        .filter(|value| !value.is_null())
        .unwrap_or(&raw_response);
    let decision = parse_action_decision_content(&serde_json::to_string(candidate)?)?;
    Ok((decision, raw_response))
}

async fn write_success_agent_response(
    workspace: &Path,
    backend: &AgentBackendSettingsEntry,
    duration_ms: u64,
    raw_response: &serde_json::Value,
    decision: &AgentDecision,
) -> Result<(), AppError> {
    let response = serde_json::json!({
        "schemaVersion": 1,
        "generatedAt": Utc::now(),
        "backendId": backend.name,
        "backendType": backend.backend_type.as_str(),
        "runtimeStatus": "succeeded",
        "durationMs": duration_ms,
        "decision": raw_response.get("decision").unwrap_or(raw_response),
        "normalizedDecision": decision,
        "usage": raw_response.get("usage"),
        "cost": raw_response.get("cost"),
        "error": null,
    });
    write_json_atomic(workspace.join("agent_response.json"), &response).await
}

async fn write_failed_agent_response(
    workspace: &Path,
    backend: &AgentBackendSettingsEntry,
    duration_ms: u64,
    error: &str,
) -> Result<(), AppError> {
    let response = serde_json::json!({
        "schemaVersion": 1,
        "generatedAt": Utc::now(),
        "backendId": backend.name,
        "backendType": backend.backend_type.as_str(),
        "runtimeStatus": "failed",
        "durationMs": duration_ms,
        "normalizedDecision": null,
        "usage": null,
        "cost": null,
        "error": error,
    });
    write_json_atomic(workspace.join("agent_response.json"), &response).await
}

fn truncate_bytes(value: &[u8], max: usize) -> &[u8] {
    if value.len() <= max {
        value
    } else {
        &value[..max]
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        fs,
        os::unix::fs::PermissionsExt,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::{
        domain::{
            contracts::ActionKind,
            models::{Confidence, GrepMatch, GrepResults},
        },
        support::config::{AgentBackendSettings, AgentBackendSettingsEntry},
    };

    use super::*;

    #[tokio::test]
    async fn claude_agent_sdk_adapter_returns_final_answer() {
        let fixture = Fixture::new();
        let adapter = fixture.write_adapter(
            "final.sh",
            r#"#!/usr/bin/env bash
cat <<'JSON'
{"decision":{"type":"final_answer","result":{"summary":"mock summary","symptoms":["timeout"],"likelyRootCauses":[{"cause":"timeout in logs","evidenceRefs":["grep_results.json#matches/0"]}],"nextChecks":["check timeout"],"fixSuggestions":["increase timeout"],"missingInformation":[],"confidence":"high"}},"usage":{"inputTokens":11},"cost":{"usd":0.01}}
JSON
"#,
        );
        let decision = fixture
            .registry(adapter)
            .decide_next(fixture.input())
            .await
            .unwrap();

        match decision {
            AgentDecision::FinalAnswer { result } => {
                assert_eq!(result.summary, "mock summary");
                assert!(matches!(result.confidence, Confidence::High));
            }
            AgentDecision::Action { .. } => panic!("expected final_answer"),
        }
        let response: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(fixture.workspace.join("agent_response.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(response["runtimeStatus"], "succeeded");
        assert_eq!(response["backendType"], "claude_agent_sdk");
        assert_eq!(response["normalizedDecision"]["type"], "final_answer");
        assert_eq!(response["usage"]["inputTokens"], 11);
    }

    #[tokio::test]
    async fn claude_agent_sdk_adapter_returns_run_tool_action() {
        let fixture = Fixture::new();
        let adapter = fixture.write_adapter(
            "run_tool.sh",
            r#"#!/usr/bin/env bash
cat <<'JSON'
{"decision":{"type":"action","decision":{"type":"run_tool","reason":"need analyzer finding","input":{"tool":"influxql_analyzer","inputFile":"extracted/sample.log"},"risk":"SAFE_READ_ONLY","fingerprint":"tool:influxql"}}}
JSON
"#,
        );
        let decision = fixture
            .registry(adapter)
            .decide_next(fixture.input())
            .await
            .unwrap();

        match decision {
            AgentDecision::Action { decision } => {
                assert_eq!(decision.kind, ActionKind::RunTool);
                assert_eq!(decision.input["tool"], "influxql_analyzer");
            }
            AgentDecision::FinalAnswer { .. } => panic!("expected run_tool action"),
        }
    }

    #[tokio::test]
    async fn claude_agent_sdk_adapter_returns_ask_user_action() {
        let fixture = Fixture::new();
        let adapter = fixture.write_adapter(
            "ask_user.sh",
            r#"#!/usr/bin/env bash
cat <<'JSON'
{"decision":{"type":"action","decision":{"type":"ask_user","reason":"need deployment version","input":{"question":"Which version was running?","answerFormat":"semver","required":true},"risk":"SAFE_READ_ONLY","fingerprint":"ask-version"}}}
JSON
"#,
        );
        let decision = fixture
            .registry(adapter)
            .decide_next(fixture.input())
            .await
            .unwrap();

        match decision {
            AgentDecision::Action { decision } => assert_eq!(decision.kind, ActionKind::AskUser),
            AgentDecision::FinalAnswer { .. } => panic!("expected ask_user action"),
        }
    }

    #[tokio::test]
    async fn claude_agent_sdk_adapter_rejects_invalid_evidence_ref() {
        let fixture = Fixture::new();
        let adapter = fixture.write_adapter(
            "bad_evidence.sh",
            r#"#!/usr/bin/env bash
cat <<'JSON'
{"decision":{"type":"final_answer","result":{"summary":"mock summary","symptoms":["timeout"],"likelyRootCauses":[{"cause":"bad evidence","evidenceRefs":["system_context.json#resources/0"]}],"nextChecks":[],"fixSuggestions":[],"missingInformation":[],"confidence":"low"}}}
JSON
"#,
        );
        let error = fixture
            .registry(adapter)
            .decide_next(fixture.input())
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

    #[tokio::test]
    async fn claude_agent_sdk_adapter_reports_nonzero_exit() {
        let fixture = Fixture::new();
        let adapter = fixture.write_adapter(
            "fail.sh",
            r#"#!/usr/bin/env bash
echo adapter failed >&2
exit 12
"#,
        );
        let error = fixture
            .registry(adapter)
            .decide_next(fixture.input())
            .await
            .unwrap_err()
            .to_string();

        assert!(error.contains("exited with status"));
        let response: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(fixture.workspace.join("agent_response.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(response["runtimeStatus"], "failed");
        assert!(response["error"]
            .as_str()
            .unwrap()
            .contains("adapter failed"));
    }

    struct Fixture {
        root: PathBuf,
        workspace: PathBuf,
        grep: GrepResults,
    }

    impl Fixture {
        fn new() -> Self {
            let suffix = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!("logagent-agent-backend-{suffix}"));
            let workspace = root.join("workspace");
            fs::create_dir_all(&workspace).unwrap();
            fs::write(workspace.join("analysis_package.json"), "{}").unwrap();
            fs::write(workspace.join("agent_request.json"), "{}").unwrap();
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

        fn registry(&self, adapter: PathBuf) -> AgentBackendRegistry {
            AgentBackendRegistry::new(AgentBackendSettings {
                default_backend: "claude_agent_sdk".to_string(),
                backends: BTreeMap::from([(
                    "claude_agent_sdk".to_string(),
                    AgentBackendSettingsEntry {
                        name: "claude_agent_sdk".to_string(),
                        backend_type: AgentBackendType::ClaudeAgentSdk,
                        enabled: true,
                        command_path: Some(adapter),
                        timeout_seconds: 5,
                        max_input_bytes: 1024 * 1024,
                        max_output_bytes: 1024 * 1024,
                    },
                )]),
            })
        }

        fn input(&self) -> AgentBackendDecisionInput<'_> {
            AgentBackendDecisionInput {
                workspace: &self.workspace,
                grep_results: &self.grep,
                case_context: None,
                tool_results: &[],
            }
        }

        fn write_adapter(&self, filename: &str, content: &str) -> PathBuf {
            let path = self.root.join(filename);
            fs::write(&path, content).unwrap();
            let mut permissions = fs::metadata(&path).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&path, permissions).unwrap();
            path
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
