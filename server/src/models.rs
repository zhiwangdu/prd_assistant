use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadRecord {
    pub upload_id: String,
    pub filename: String,
    pub size: u64,
    pub path: PathBuf,
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
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskResponse {
    pub task_id: String,
    pub url: String,
    pub status: TaskStatus,
    pub manifest_path: String,
    pub grep_results_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskArtifactsResponse {
    pub task_id: String,
    pub manifest_path: String,
    pub grep_results_path: String,
    pub manifest: serde_json::Value,
    pub grep_results: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskStatus {
    Done,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskSource {
    Upload,
}

#[derive(Debug, Clone)]
pub struct TaskContext {
    pub task_id: String,
    pub source: TaskSource,
    pub source_url: Option<String>,
    pub workspace: PathBuf,
}

#[derive(Debug, Serialize)]
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestUpload {
    pub upload_id: String,
    pub filename: String,
    pub size: u64,
    pub raw_path: String,
    pub extracted_dir: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestFile {
    pub path: String,
    pub size: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrepResults {
    pub keywords: Vec<String>,
    pub total_matches: usize,
    pub matches: Vec<GrepMatch>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrepMatch {
    pub file: String,
    pub line: usize,
    pub keyword: String,
    pub text: String,
}

#[derive(Debug)]
pub struct PipelineOutput {
    pub manifest_path: PathBuf,
    pub grep_results_path: PathBuf,
}
