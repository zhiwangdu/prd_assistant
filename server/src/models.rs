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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskRequest {
    pub upload_id: Option<String>,
    #[serde(default)]
    pub upload_ids: Vec<String>,
    pub source_url: Option<String>,
    pub question: Option<String>,
    pub instance_id: Option<String>,
    pub cluster_id: Option<String>,
    pub node_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskResponse {
    pub task_id: String,
    pub url: String,
    pub status: TaskStatus,
    pub phase: Option<TaskPhase>,
    pub created_at: DateTime<Utc>,
}

pub type TaskSummary = TaskResponse;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskListResponse {
    pub tasks: Vec<TaskSummary>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskArtifactsResponse {
    pub task_id: String,
    pub manifest_path: String,
    pub grep_results_path: String,
    pub manifest: serde_json::Value,
    pub grep_results: serde_json::Value,
    pub metadata_context_path: Option<String>,
    pub metadata_context: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskResultResponse {
    pub task_id: String,
    pub result_json_path: String,
    pub result_markdown_path: String,
    pub result: AnalysisResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskStatus {
    Queued,
    Running,
    WaitingForUser,
    WaitingForApproval,
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
    Extract,
    SearchLogs,
    RunTool,
    PlanAnalysis,
    GenerateResult,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskSource {
    Upload,
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
    pub source: TaskSource,
    pub upload_ids: Vec<String>,
    pub inputs: Vec<TaskInput>,
    pub source_url: Option<String>,
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
            url: format!(
                "{}/tasks/{}",
                public_base_url.trim_end_matches('/'),
                self.task_id
            ),
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
    pub files: Vec<ManifestFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestUpload {
    pub upload_id: String,
    pub filename: String,
    pub size: u64,
    pub raw_path: String,
    pub extracted_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestFile {
    pub path: String,
    pub size: u64,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RootCause {
    pub cause: String,
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisResult {
    pub schema_version: u32,
    pub summary: String,
    pub symptoms: Vec<String>,
    pub likely_root_causes: Vec<RootCause>,
    pub next_checks: Vec<String>,
    pub fix_suggestions: Vec<String>,
    pub missing_information: Vec<String>,
    pub confidence: Confidence,
}

#[derive(Debug)]
pub struct ResultOutput {
    pub result_json_path: PathBuf,
    pub result_markdown_path: PathBuf,
}
