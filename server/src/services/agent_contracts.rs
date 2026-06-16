use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::{
    domain::models::{GrepResults, Manifest, TaskRecord},
    services::{
        metadata::{metadata_context_outline, TaskMetadataContext},
        tool_runner::ToolRunRecord,
    },
    stores::analysis_state::{AnalysisSnapshotResponse, UserMessageResumeMode},
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
    pub metadata_context: Option<&'a TaskMetadataContext>,
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
    let metadata_context_outline = input.metadata_context.map(metadata_context_outline);
    let finalize_requested = input
        .analysis_snapshot
        .state
        .user_messages
        .last()
        .map(|message| message.resume_mode == UserMessageResumeMode::Finalize)
        .unwrap_or(false);
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
            "analysisLanguage": input.task.analysis_language,
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
            "metadataContextOutline": metadata_context_outline,
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
            "finalizeRequested": finalize_requested,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::models::{
            GrepResults, Manifest, TaskKind, TaskPhase, TaskRecord, TaskSource, TaskStatus,
        },
        services::metadata::{
            ClusterMetadata, DatabaseMetadata, FieldSchemaMetadata, InstanceMetadata,
            MeasurementMetadata, RetentionPolicyMetadata, TaskMetadataContext,
        },
        stores::analysis_state,
        support::config::{AnalysisMode, ClaudeCodeSettings, McpSettings},
    };

    #[tokio::test]
    async fn analysis_package_uses_metadata_outline_instead_of_full_context() {
        let workspace = temp_dir("agent-contracts-metadata-outline");
        std::fs::create_dir_all(&workspace).unwrap();
        let task = task_record("task_meta");
        analysis_state::initialize(&workspace, &task).unwrap();
        let snapshot = analysis_state::read_snapshot(&workspace).unwrap();
        let manifest = Manifest {
            upload_id: String::new(),
            upload_ids: Vec::new(),
            uploads: Vec::new(),
            task_id: task.task_id.clone(),
            source: TaskSource::Upload,
            filename: String::new(),
            source_url: None,
            files: Vec::new(),
        };
        let grep_results = GrepResults {
            keywords: Vec::new(),
            total_matches: 0,
            matches: Vec::new(),
        };
        let metadata_context = metadata_context_fixture();

        write_agent_contracts(
            &workspace,
            AgentContractInput {
                task: &task,
                manifest: &manifest,
                grep_results: &grep_results,
                metadata_context: Some(&metadata_context),
                system_context: None,
                case_context: None,
                tool_results: &[],
                analysis_snapshot: &snapshot,
                claude_code: &ClaudeCodeSettings::default(),
                mcp: &McpSettings::default(),
                config_path: &workspace.join("logagent-test.yaml"),
                analysis_mode: AnalysisMode::Diagnose,
                tools: &ToolsSettings::default(),
            },
        )
        .await
        .unwrap();

        let package: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(workspace.join("analysis_package.json")).unwrap(),
        )
        .unwrap();
        let evidence = package["evidence"].as_object().unwrap();
        assert!(!evidence.contains_key("metadataContext"));
        assert_eq!(
            package["evidence"]["metadataContextOutline"]["kind"],
            "metadata_context_outline"
        );
        assert_eq!(
            package["evidence"]["metadataContextOutline"]["metadataContextPath"],
            "metadata_context.json"
        );
        assert_eq!(
            package["evidence"]["metadataContextOutline"]["counts"]["measurements"],
            1
        );
        let encoded = serde_json::to_string(&package).unwrap();
        assert!(!encoded.contains("cpu_0000"));
        assert!(!encoded.contains("retentionPolicies\":["));

        let _ = std::fs::remove_dir_all(workspace);
    }

    fn temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "logagent-{name}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ))
    }

    fn task_record(task_id: &str) -> TaskRecord {
        let now = Utc::now();
        TaskRecord {
            schema_version: 4,
            task_id: task_id.to_string(),
            alias: None,
            session_id: Some("sess_test".to_string()),
            task_kind: TaskKind::LogAnalysis,
            analysis_mode: AnalysisMode::Diagnose,
            analysis_language: crate::domain::models::AnalysisLanguage::ZhCn,
            source: TaskSource::Upload,
            upload_ids: Vec::new(),
            inputs: Vec::new(),
            source_url: None,
            tool_id: None,
            tool_params: serde_json::Value::Null,
            tool_result_path: None,
            remote_executor_id: None,
            remote_command_id: None,
            remote_command_params: serde_json::Value::Null,
            remote_result_path: None,
            instance_id: Some("prod-a".to_string()),
            cluster_id: Some("prod-a".to_string()),
            node_id: None,
            question: "why".to_string(),
            status: TaskStatus::Running,
            phase: Some(TaskPhase::PlanAnalysis),
            attempts: 1,
            error: None,
            manifest_path: None,
            grep_results_path: None,
            metadata_context_path: Some("metadata_context.json".to_string()),
            system_context_path: None,
            result_json_path: None,
            result_markdown_path: None,
            created_at: now,
            updated_at: now,
        }
    }

    fn metadata_context_fixture() -> TaskMetadataContext {
        TaskMetadataContext {
            schema_version: 1,
            resolved_at: Utc::now(),
            instance_id: Some("prod-a".to_string()),
            cluster_id: Some("prod-a".to_string()),
            node_id: None,
            product: Some("opengemini".to_string()),
            version: Some("1.3.0".to_string()),
            environment: Some("prod".to_string()),
            instance: Some(InstanceMetadata {
                instance_id: "prod-a".to_string(),
                cluster_id: Some("prod-a".to_string()),
                product: Some("opengemini".to_string()),
                version: Some("1.3.0".to_string()),
                environment: Some("prod".to_string()),
                ..InstanceMetadata::default()
            }),
            cluster: Some(ClusterMetadata {
                cluster_id: "prod-a".to_string(),
                databases: vec![DatabaseMetadata {
                    name: "mydb".to_string(),
                    retention_policies: vec![RetentionPolicyMetadata {
                        name: "autogen".to_string(),
                        measurements: vec![MeasurementMetadata {
                            name: "cpu_0000".to_string(),
                            schema: vec![FieldSchemaMetadata {
                                name: "usage".to_string(),
                                typ: Some(3),
                                end_time: None,
                            }],
                            ..MeasurementMetadata::default()
                        }],
                        ..RetentionPolicyMetadata::default()
                    }],
                    ..DatabaseMetadata::default()
                }],
                ..ClusterMetadata::default()
            }),
            node: None,
            cluster_nodes: Vec::new(),
        }
    }
}
