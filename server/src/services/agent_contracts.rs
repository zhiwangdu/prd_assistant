use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::{
    domain::models::{GrepResults, Manifest, TaskRecord},
    services::tool_runner::ToolRunRecord,
    stores::analysis_state::AnalysisSnapshotResponse,
    support::{config::AgentBackendSettings, error::AppError},
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
    pub agent_backends: &'a AgentBackendSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentContractArtifacts {
    pub analysis_package_path: String,
    pub agent_request_path: String,
    pub agent_response_path: String,
}

pub async fn write_agent_contracts(
    workspace: &Path,
    input: AgentContractInput<'_>,
) -> Result<AgentContractArtifacts, AppError> {
    let backend = input
        .agent_backends
        .backends
        .get(&input.agent_backends.default_backend)
        .ok_or_else(|| AppError::internal("default agent backend is not configured"))?;
    let now = Utc::now();
    let package = serde_json::json!({
        "schemaVersion": 1,
        "generatedAt": now,
        "purpose": "diagnostic_evidence_package",
        "runtimeStatus": "contract_only_not_invoked",
        "task": {
            "taskId": input.task.task_id,
            "taskKind": input.task.task_kind,
            "sessionId": input.task.session_id,
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
            "caseContextPath": input.case_context.map(|_| "case_context.json"),
            "caseContext": input.case_context,
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
            "agentBackendRole": "reason_over_package_and_return_structured_result",
            "serverExecutionBoundary": true,
            "freeformShellAllowed": false,
            "freeformSshAllowed": false,
            "workspaceWriteAllowed": false,
        },
    });
    let request = serde_json::json!({
        "schemaVersion": 1,
        "generatedAt": now,
        "backend": {
            "backendId": backend.name,
            "backendType": backend.backend_type.as_str(),
            "executionMode": backend.backend_type.execution_mode(),
            "runtimeStatus": "contract_only_not_invoked",
            "timeoutSeconds": backend.timeout_seconds,
            "maxInputBytes": backend.max_input_bytes,
            "maxOutputBytes": backend.max_output_bytes,
        },
        "input": {
            "analysisPackagePath": "analysis_package.json",
            "question": input.task.question,
        },
        "allowedOutputs": {
            "finalAnswer": {
                "summary": "string",
                "symptoms": ["string"],
                "likelyRootCauses": [{"cause": "string", "evidenceRefs": ["string"]}],
                "nextChecks": ["string"],
                "fixSuggestions": ["string"],
                "missingInformation": ["string"],
                "confidence": "low|medium|high"
            },
            "actions": [
                "search_logs",
                "run_tool",
                "collect_code_evidence",
                "collect_environment",
                "ask_user",
                "final_answer"
            ],
            "evidenceRefs": [
                "session_text_input.json#question",
                "grep_results.json#matches/<index>",
                "case_context.json#cases/<index>",
                "tool_results/<action_id>/result.json#findings/<index>"
            ]
        },
        "executionPolicy": {
            "externalBackendMayExecuteCommands": false,
            "externalBackendMayMutateLogAgentState": false,
            "serverValidatesActions": true,
            "remoteCollectionRequiresApproval": true,
        }
    });
    let response = serde_json::json!({
        "schemaVersion": 1,
        "generatedAt": now,
        "backendId": backend.name,
        "backendType": backend.backend_type.as_str(),
        "runtimeStatus": "not_invoked",
        "reason": "LogAgent currently freezes the external agent contract while the production runtime still uses internal_llm for PLAN_ANALYSIS.",
        "expectedShape": {
            "type": "final_answer|action",
            "result": "AnalysisResult when type=final_answer",
            "action": "AgentAction-compatible decision when type=action"
        }
    });

    write_json_atomic(workspace.join("analysis_package.json"), &package).await?;
    write_json_atomic(workspace.join("agent_request.json"), &request).await?;
    write_json_atomic(workspace.join("agent_response.json"), &response).await?;

    Ok(AgentContractArtifacts {
        analysis_package_path: "analysis_package.json".to_string(),
        agent_request_path: "agent_request.json".to_string(),
        agent_response_path: "agent_response.json".to_string(),
    })
}

async fn write_json_atomic<T: Serialize>(path: PathBuf, value: &T) -> Result<(), AppError> {
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
