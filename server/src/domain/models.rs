use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadRecord {
    pub schema_version: u32,
    pub upload_id: String,
    pub filename: String,
    pub size: u64,
    pub expected_size: Option<u64>,
    pub status: UploadStatus,
    pub path: PathBuf,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UploadStatus {
    Uploading,
    Complete,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadResponse {
    pub upload_id: String,
    pub filename: String,
    pub size: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchUploadResponse {
    pub uploads: Vec<UploadResponse>,
    pub total_size: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitUploadRequest {
    pub filename: String,
    pub size: u64,
}

#[derive(Debug, Deserialize)]
pub struct ChunkQuery {
    pub offset: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChunkUploadResponse {
    pub upload_id: String,
    pub received_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskResponse {
    pub task_id: String,
    pub alias: Option<String>,
    pub url: String,
    pub task_kind: TaskKind,
    pub session_id: Option<String>,
    pub status: TaskStatus,
    pub phase: Option<TaskPhase>,
    pub created_at: DateTime<Utc>,
}

pub type TaskSummary = TaskResponse;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolListResponse {
    pub tools: Vec<ToolDescriptor>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDescriptor {
    pub tool_id: String,
    pub display_name: String,
    pub description: String,
    pub enabled: bool,
    pub source: ToolSource,
    pub read_only: bool,
    pub editable: bool,
    pub exportable: bool,
    pub runnable: bool,
    /// Side-effect-free MCP-native platform tools (e.g. `logagent.runs.get`).
    /// Advertised in `tools/list` but bypass the Tool Runner: `tools/call` serves
    /// them directly (no `ToolRun` is created, so polling does not pollute history).
    #[serde(default)]
    pub platform: bool,
    pub tags: Vec<String>,
    pub backend: String,
    pub accepted_suffixes: Vec<String>,
    pub min_files: usize,
    pub max_files: usize,
    pub params_schema: serde_json::Value,
    pub params_template: serde_json::Value,
    pub output_views: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolSource {
    BuiltIn,
    Configured,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateToolRunRequest {
    #[serde(default)]
    pub upload_ids: Vec<String>,
    #[serde(default)]
    pub params: serde_json::Value,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolRunListResponse {
    pub runs: Vec<TaskSummary>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolRunArtifactsResponse {
    pub task_id: String,
    pub tool_id: String,
    pub result_path: String,
    pub result: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Dev self-test pipeline (P1: docker self-test closed loop). A "run" is a
// persistent workspace shared across multiple tool calls (sync -> build ->
// deploy -> run_tests -> report). The record is the index; files live under
// the run workspace dir. See `services/dev_selftest` and `skills/dev-selftest`.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DevSelftestRunStatus {
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DevSelftestDeployTarget {
    Docker {
        cluster: String,
        exposed_port: Option<u16>,
    },
    Ssh {
        executor_id: String,
    },
    Instance {
        instance_id: String,
        endpoint: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevSelftestStep {
    pub step: String,
    pub status: String,
    pub duration_ms: u128,
    pub error: Option<String>,
    pub evidence_refs: Vec<String>,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevSelftestRunRecord {
    pub schema_version: u32,
    pub run_id: String,
    pub label: Option<String>,
    pub source_ref: Option<String>,
    pub build_artifacts: Vec<String>,
    pub deploy_target: Option<DevSelftestDeployTarget>,
    pub test_run_id: Option<String>,
    pub steps: Vec<DevSelftestStep>,
    pub status: DevSelftestRunStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
}

impl TaskStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskPhase {
    RunTool,
    ExecuteRemoteCommand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)] // RemoteCommandRun retained for on-disk deserialization of legacy task records
pub enum TaskKind {
    ToolRun,
    RemoteCommandRun,
}

pub fn default_task_kind() -> TaskKind {
    TaskKind::ToolRun
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)] // RemoteExecutor retained for on-disk deserialization of legacy task records
pub enum TaskSource {
    Upload,
    RemoteExecutor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskInput {
    pub upload_id: String,
    pub filename: String,
    pub size: u64,
    pub raw_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskError {
    pub phase: Option<TaskPhase>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRecord {
    pub schema_version: u32,
    pub task_id: String,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default = "default_task_kind")]
    pub task_kind: TaskKind,
    pub source: TaskSource,
    pub upload_ids: Vec<String>,
    pub inputs: Vec<TaskInput>,
    pub source_url: Option<String>,
    #[serde(default)]
    pub tool_id: Option<String>,
    #[serde(default)]
    pub tool_params: serde_json::Value,
    #[serde(default)]
    pub tool_result_path: Option<String>,
    #[serde(default)]
    pub remote_executor_id: Option<String>,
    #[serde(default)]
    pub remote_command_id: Option<String>,
    #[serde(default)]
    pub remote_command_params: serde_json::Value,
    #[serde(default)]
    pub remote_result_path: Option<String>,
    #[serde(default)]
    pub instance_id: Option<String>,
    #[serde(default)]
    pub cluster_id: Option<String>,
    #[serde(default)]
    pub node_id: Option<String>,
    #[serde(default = "default_task_question")]
    pub question: String,
    pub status: TaskStatus,
    pub phase: Option<TaskPhase>,
    pub attempts: u32,
    pub error: Option<TaskError>,
    pub manifest_path: Option<String>,
    pub grep_results_path: Option<String>,
    #[serde(default)]
    pub metadata_context_path: Option<String>,
    #[serde(default)]
    pub system_context_path: Option<String>,
    #[serde(default)]
    pub result_json_path: Option<String>,
    #[serde(default)]
    pub result_markdown_path: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TaskRecord {
    pub fn summary(&self, public_base_url: &str) -> TaskSummary {
        TaskSummary {
            task_id: self.task_id.clone(),
            alias: self.alias.clone(),
            url: format!(
                "{}/tasks/{}",
                public_base_url.trim_end_matches('/'),
                self.task_id
            ),
            task_kind: self.task_kind,
            session_id: self.session_id.clone(),
            status: self.status,
            phase: self.phase,
            created_at: self.created_at,
        }
    }
}

pub fn default_task_question() -> String {
    "分析日志中的主要异常、可能原因和建议检查项。".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub upload_id: String,
    pub upload_ids: Vec<String>,
    pub uploads: Vec<ManifestUpload>,
    pub task_id: String,
    pub source: TaskSource,
    pub filename: String,
    pub source_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_inputs_path: Option<String>,
    pub files: Vec<ManifestFile>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestUpload {
    pub upload_id: String,
    pub filename: String,
    pub size: u64,
    pub raw_path: String,
    pub extracted_dir: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_timestamp: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub log_groups: Vec<LogGroupSummary>,
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub ignored_file_count: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignored_path_samples: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestFile {
    pub path: String,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upload_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_timestamp: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compressed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compression: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogGroupSummary {
    pub name: String,
    pub file_count: u64,
    pub compressed_file_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInputIndex {
    pub schema_version: u32,
    pub generated_by: String,
    #[serde(default)]
    pub inputs: Vec<ToolInputEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInputEntry {
    pub path: String,
    pub input_kind: String,
    pub scope: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_timestamp: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_group: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_files: Vec<String>,
    #[serde(default)]
    pub record_count: u64,
}

fn is_zero_u64(value: &u64) -> bool {
    *value == 0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrepResults {
    pub keywords: Vec<String>,
    pub total_matches: usize,
    pub matches: Vec<GrepMatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrepMatch {
    pub file: String,
    pub line: usize,
    pub keyword: String,
    pub text: String,
}
