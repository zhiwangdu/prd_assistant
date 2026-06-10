#![allow(dead_code)]

use std::{
    future::Future,
    path::{Component, Path, PathBuf},
    pin::Pin,
};

use serde::{Deserialize, Serialize};

use crate::{
    domain::models::{TaskRecord, TaskSource},
    services::metadata::TaskMetadataContext,
};

#[derive(Debug, Clone)]
pub struct TaskContext {
    pub task_id: String,
    pub source: TaskSource,
    pub product: Option<String>,
    pub version: Option<String>,
    pub instance_id: Option<String>,
    pub cluster_id: Option<String>,
    pub node_id: Option<String>,
    pub question: String,
    pub workspace: PathBuf,
}

impl TaskContext {
    pub fn from_record(
        task: &TaskRecord,
        workspace: PathBuf,
        metadata: Option<&TaskMetadataContext>,
    ) -> Self {
        Self {
            task_id: task.task_id.clone(),
            source: task.source.clone(),
            product: metadata.and_then(|context| context.product.clone()),
            version: metadata.and_then(|context| context.version.clone()),
            instance_id: task.instance_id.clone(),
            cluster_id: task.cluster_id.clone(),
            node_id: task.node_id.clone(),
            question: task.question.clone(),
            workspace,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    SearchLogs,
    RunTool,
    CollectCodeEvidence,
    CollectEnvironment,
    AskUser,
    FinalAnswer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ActionRisk {
    SafeReadOnly,
    RequiresApproval,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceRef {
    pub artifact_path: String,
    pub selector: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentAction {
    pub schema_version: u32,
    pub action_id: String,
    #[serde(rename = "type")]
    pub kind: ActionKind,
    pub reason: String,
    pub evidence_refs: Vec<EvidenceRef>,
    pub input: serde_json::Value,
    pub risk: ActionRisk,
    pub fingerprint: String,
}

impl AgentAction {
    pub fn decode_input<T: serde::de::DeserializeOwned>(&self) -> anyhow::Result<T> {
        serde_json::from_value(self.input.clone())
            .map_err(|err| anyhow::anyhow!("invalid {:?} action input: {err}", self.kind))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceType {
    LogSearch,
    ToolOutput,
    CodeEvidence,
    EnvironmentEvidence,
    MetadataContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceSummary {
    pub title: String,
    pub details: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceArtifact {
    pub schema_version: u32,
    pub action_id: Option<String>,
    pub evidence_type: EvidenceType,
    pub artifact_path: String,
    pub summary: EvidenceSummary,
}

impl EvidenceArtifact {
    pub fn validate(&self) -> anyhow::Result<()> {
        validate_workspace_relative_path(&self.artifact_path)
    }
}

pub trait EvidenceProvider: Send + Sync {
    fn execute<'a>(
        &'a self,
        context: &'a TaskContext,
        action: &'a AgentAction,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<EvidenceArtifact>> + Send + 'a>>;
}

fn validate_workspace_relative_path(path: &str) -> anyhow::Result<()> {
    let path = Path::new(path);
    let valid = !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)));
    if !valid {
        anyhow::bail!("evidence artifact path must be workspace-relative");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_and_evidence_contracts_use_stable_json_names() {
        #[derive(Debug, Deserialize, PartialEq, Eq)]
        struct ToolInput {
            tool: String,
        }

        let action = AgentAction {
            schema_version: 1,
            action_id: "act_1".to_string(),
            kind: ActionKind::RunTool,
            reason: "inspect query".to_string(),
            evidence_refs: vec![EvidenceRef {
                artifact_path: "grep_results.json".to_string(),
                selector: Some("matches/0".to_string()),
            }],
            input: serde_json::json!({"tool": "flux_query_analyzer"}),
            risk: ActionRisk::SafeReadOnly,
            fingerprint: "run-tool:flux-query:1".to_string(),
        };
        assert_eq!(
            action.decode_input::<ToolInput>().unwrap(),
            ToolInput {
                tool: "flux_query_analyzer".to_string()
            }
        );
        let value = serde_json::to_value(action).unwrap();
        assert_eq!(value["type"], "run_tool");
        assert_eq!(value["risk"], "SAFE_READ_ONLY");
        assert_eq!(
            value["evidenceRefs"][0]["artifactPath"],
            "grep_results.json"
        );

        let artifact = EvidenceArtifact {
            schema_version: 1,
            action_id: Some("act_1".to_string()),
            evidence_type: EvidenceType::ToolOutput,
            artifact_path: "tool_results/act_1/result.json".to_string(),
            summary: EvidenceSummary {
                title: "tool result".to_string(),
                details: vec!["one finding".to_string()],
            },
        };
        artifact.validate().unwrap();
        assert!(EvidenceArtifact {
            artifact_path: "../outside.json".to_string(),
            ..artifact
        }
        .validate()
        .is_err());
    }
}
