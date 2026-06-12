use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::{
    domain::models::{GrepResults, Manifest, TaskRecord},
    services::tool_runner::ToolRunRecord,
    stores::analysis_state::AnalysisSnapshotResponse,
    support::{
        config::{AnalysisMode, ClaudeCodeSettings, McpSettings, ToolsSettings},
        error::AppError,
    },
};

#[derive(Debug, Clone)]
pub struct AgentContractInput<'a> {
    pub task: &'a TaskRecord,
    pub manifest: &'a Manifest,
    pub grep_results: &'a GrepResults,
    pub metadata_context: Option<&'a serde_json::Value>,
    pub system_context: Option<&'a serde_json::Value>,
    pub case_context: Option<&'a serde_json::Value>,
    pub tool_results: &'a [ToolRunRecord],
    pub analysis_snapshot: &'a AnalysisSnapshotResponse,
    pub claude_code: &'a ClaudeCodeSettings,
    pub mcp: &'a McpSettings,
    pub config_path: &'a Path,
    pub analysis_mode: AnalysisMode,
    pub tools: &'a ToolsSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentContractArtifacts {
    pub analysis_package_path: String,
    pub claude_mcp_config_path: String,
    pub agent_response_path: String,
    pub claude_session_path: String,
    pub mcp_calls_path: String,
}

pub async fn write_agent_contracts(
    workspace: &Path,
    input: AgentContractInput<'_>,
) -> Result<AgentContractArtifacts, AppError> {
    let now = Utc::now();
    let permission_profile = input
        .claude_code
        .permission_profiles
        .get(&input.analysis_mode)
        .ok_or_else(|| AppError::internal("analysis mode permission profile is not configured"))?;
    let diagnostic_skills = input
        .system_context
        .and_then(|context| context.get("resources"))
        .and_then(|value| value.as_array())
        .map(|resources| {
            resources
                .iter()
                .filter(|resource| {
                    resource.get("kind").and_then(|value| value.as_str())
                        == Some("diagnostic_skill")
                })
                .map(|resource| {
                    serde_json::json!({
                        "skillId": resource.get("skillId"),
                        "revision": resource.get("revision"),
                        "title": resource.get("title"),
                        "summary": resource.get("summary"),
                        "references": resource.get("references"),
                        "finalEvidenceAllowed": false,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let package = serde_json::json!({
        "schemaVersion": 2,
        "generatedAt": now,
        "purpose": "diagnostic_evidence_package",
        "runtimeStatus": "ready_for_claude_code",
        "task": {
            "taskId": input.task.task_id,
            "taskKind": input.task.task_kind,
            "sessionId": input.task.session_id,
            "analysisMode": input.analysis_mode,
            "question": input.task.question,
            "sourceUrl": input.task.source_url,
            "instanceId": input.task.instance_id,
            "clusterId": input.task.cluster_id,
            "nodeId": input.task.node_id,
            "uploadIds": input.task.upload_ids,
            "inputs": input.task.inputs,
        },
        "evidence": {
            "sessionTextInputRef": "session_text_input.json#question",
            "manifestPath": "manifest.json",
            "manifest": input.manifest,
            "grepResultsPath": "grep_results.json",
            "grepResults": input.grep_results,
            "metadataContextPath": input.metadata_context.map(|_| "metadata_context.json"),
            "metadataContext": input.metadata_context,
            "systemContextPath": input.system_context.map(|_| "system_context.json"),
            "systemContext": input.system_context,
            "diagnosticSkills": diagnostic_skills,
            "caseContextPath": input.case_context.map(|_| "case_context.json"),
            "caseContext": input.case_context,
            "toolCapabilities": tool_capabilities(input.tools),
            "toolResults": input.tool_results,
        },
        "analysisState": {
            "statePath": "analysis_state.json",
            "eventsPath": "analysis_events.jsonl",
            "state": input.analysis_snapshot.state,
            "eventCount": input.analysis_snapshot.events.len(),
        },
        "boundaries": {
            "logagentRole": "collect_and_govern_evidence",
            "claudeCodeRole": "reason_with_mcp_evidence_and_return_structured_outcome",
            "mcpEnabled": input.mcp.enabled,
            "mcpTransport": input.mcp.transport,
            "permissionProfile": permission_profile.name,
            "nativeBashAllowed": permission_profile.native_bash,
            "nativeEditAllowed": permission_profile.native_edit,
            "worktreeRequired": permission_profile.worktree_required,
            "serverOwnsEvidencePersistence": true,
            "remoteCollectionRequiresApproval": true,
            "systemContextIsFinalEvidence": false,
            "diagnosticSkillsAreFinalEvidence": false,
            "skillReferencesAreFinalEvidence": false,
        },
    });
    let config_path = input
        .config_path
        .to_str()
        .ok_or_else(|| AppError::internal("config path must be valid UTF-8 for MCP config"))?;
    let exe = std::env::current_exe().map_err(|err| {
        AppError::internal(format!("failed to resolve current executable: {err}"))
    })?;
    let exe = exe
        .to_str()
        .ok_or_else(|| AppError::internal("server executable path must be valid UTF-8"))?;
    let mcp_config = serde_json::json!({
        "mcpServers": {
            "logagent": {
                "command": exe,
                "args": [
                    "mcp",
                    "--config",
                    config_path,
                    "--task-id",
                    input.task.task_id,
                    "--mode",
                    input.analysis_mode.as_str()
                ]
            }
        }
    });

    write_json_atomic(workspace.join("analysis_package.json"), &package).await?;
    write_json_atomic(workspace.join("claude_mcp_config.json"), &mcp_config).await?;

    Ok(AgentContractArtifacts {
        analysis_package_path: "analysis_package.json".to_string(),
        claude_mcp_config_path: "claude_mcp_config.json".to_string(),
        agent_response_path: "agent_response.json".to_string(),
        claude_session_path: "claude_session.json".to_string(),
        mcp_calls_path: "mcp_calls.jsonl".to_string(),
    })
}

pub async fn write_json_atomic<T: Serialize>(path: PathBuf, value: &T) -> Result<(), AppError> {
    let tmp = path.with_extension("json.tmp");
    let encoded = serde_json::to_vec_pretty(value)
        .map_err(|err| AppError::internal(format!("failed to encode agent contract: {err}")))?;
    tokio::fs::write(&tmp, encoded)
        .await
        .map_err(|err| AppError::internal(format!("failed to write agent contract: {err}")))?;
    tokio::fs::rename(&tmp, &path)
        .await
        .map_err(|err| AppError::internal(format!("failed to persist agent contract: {err}")))?;
    Ok(())
}

fn tool_capabilities(settings: &ToolsSettings) -> Vec<serde_json::Value> {
    settings
        .tools
        .values()
        .filter(|tool| tool.enabled)
        .map(|tool| {
            serde_json::json!({
                "toolId": tool.name,
                "actionType": "run_tool",
                "timeoutSeconds": tool.timeout_seconds,
                "maxOutputBytes": tool.max_output_bytes,
                "maxInputFiles": tool.max_input_files,
                "match": {
                    "filePatterns": tool.match_settings.file_patterns,
                    "keywords": tool.match_settings.keywords,
                },
                "findingEvidenceRef": "tool_results/<action_id>/result.json#findings/<index>",
                "executionBoundary": "server_tool_runner_whitelist"
            })
        })
        .collect()
}
