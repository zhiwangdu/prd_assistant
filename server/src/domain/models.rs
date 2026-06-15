use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::support::config::AnalysisMode;

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
    pub session_id: Option<String>,
    pub source_url: Option<String>,
    pub question: Option<String>,
    pub instance_id: Option<String>,
    pub cluster_id: Option<String>,
    pub node_id: Option<String>,
    #[serde(default)]
    pub analysis_mode: Option<AnalysisMode>,
    #[serde(default)]
    pub system_context_ids: Vec<String>,
    #[serde(default)]
    pub skill_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskResponse {
    pub task_id: String,
    pub alias: Option<String>,
    pub url: String,
    pub task_kind: TaskKind,
    pub session_id: Option<String>,
    pub analysis_mode: AnalysisMode,
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
    pub tags: Vec<String>,
    pub backend: String,
    pub accepted_suffixes: Vec<String>,
    pub min_files: usize,
    pub max_files: usize,
    pub params_schema: serde_json::Value,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteExecutorRecord {
    pub schema_version: u32,
    pub executor_id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub enabled: bool,
    pub notes: Option<String>,
    pub last_check: Option<RemoteExecutorCheck>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteExecutorCheck {
    pub checked_at: DateTime<Utc>,
    pub status: RemoteExecutorCheckStatus,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RemoteExecutorCheckStatus {
    Ok,
    Failed,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteExecutorListResponse {
    pub executors: Vec<RemoteExecutorRecord>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRemoteExecutorRequest {
    pub name: String,
    pub host: String,
    #[serde(default = "default_remote_executor_port")]
    pub port: u16,
    pub user: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "default_remote_executor_enabled")]
    pub enabled: bool,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchRemoteExecutorRequest {
    pub name: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub user: Option<String>,
    pub tags: Option<Vec<String>>,
    pub enabled: Option<bool>,
    #[serde(default)]
    pub notes: Option<Option<String>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteCommandTemplateDescriptor {
    pub command_id: String,
    pub display_name: String,
    pub description: String,
    pub enabled: bool,
    pub argv: Vec<String>,
    pub timeout_seconds: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteCommandTemplateListResponse {
    pub commands: Vec<RemoteCommandTemplateDescriptor>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRemoteCommandRunRequest {
    pub executor_id: String,
    pub command_id: String,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteCommandRunsQuery {
    pub executor_id: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteCommandRunListResponse {
    pub runs: Vec<TaskSummary>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteCommandRunResultResponse {
    pub task_id: String,
    pub executor_id: String,
    pub command_id: String,
    pub result_path: String,
    pub result: serde_json::Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskArtifactsResponse {
    pub task_id: String,
    pub manifest_path: String,
    pub grep_results_path: String,
    pub manifest: serde_json::Value,
    pub grep_results: serde_json::Value,
    pub text_input_path: Option<String>,
    pub text_input: Option<serde_json::Value>,
    pub metadata_context_path: Option<String>,
    pub metadata_context: Option<serde_json::Value>,
    pub case_context_path: Option<String>,
    pub case_context: Option<serde_json::Value>,
    pub system_context_path: Option<String>,
    pub system_context: Option<serde_json::Value>,
    pub analysis_package_path: Option<String>,
    pub analysis_package: Option<serde_json::Value>,
    pub agent_response_path: Option<String>,
    pub agent_response: Option<serde_json::Value>,
    pub claude_mcp_config_path: Option<String>,
    pub claude_mcp_config: Option<serde_json::Value>,
    pub claude_session_path: Option<String>,
    pub claude_session: Option<serde_json::Value>,
    pub mcp_calls_path: Option<String>,
    pub mcp_calls: Vec<serde_json::Value>,
    pub tool_results: Vec<serde_json::Value>,
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
    ExecuteRemoteCommand,
    PlanAnalysis,
    GenerateResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    LogAnalysis,
    ToolRun,
    RemoteCommandRun,
}

pub fn default_task_kind() -> TaskKind {
    TaskKind::LogAnalysis
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
    #[serde(default = "default_analysis_mode")]
    pub analysis_mode: AnalysisMode,
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
            analysis_mode: self.analysis_mode,
            status: self.status,
            phase: self.phase,
            created_at: self.created_at,
        }
    }
}

pub fn default_analysis_mode() -> AnalysisMode {
    AnalysisMode::Diagnose
}

pub fn default_task_question() -> String {
    "分析日志中的主要异常、可能原因和建议检查项。".to_string()
}

pub fn default_remote_executor_port() -> u16 {
    22
}

pub fn default_remote_executor_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisSessionStatus {
    Draft,
    Ready,
    Running,
    WaitingForUser,
    WaitingForApproval,
    Succeeded,
    Failed,
}

impl AnalysisSessionStatus {
    pub fn from_task_status(status: TaskStatus) -> Self {
        match status {
            TaskStatus::Queued => Self::Ready,
            TaskStatus::Running => Self::Running,
            TaskStatus::WaitingForUser => Self::WaitingForUser,
            TaskStatus::WaitingForApproval => Self::WaitingForApproval,
            TaskStatus::Succeeded => Self::Succeeded,
            TaskStatus::Failed => Self::Failed,
        }
    }

    pub fn is_running_like(self) -> bool {
        matches!(
            self,
            Self::Running | Self::WaitingForUser | Self::WaitingForApproval
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisSessionRecord {
    pub schema_version: u32,
    pub session_id: String,
    pub title: String,
    pub question: String,
    pub source_url: Option<String>,
    pub instance_id: Option<String>,
    pub node_id: Option<String>,
    #[serde(default)]
    pub system_context_ids: Vec<String>,
    #[serde(default)]
    pub skill_ids: Vec<String>,
    pub upload_ids: Vec<String>,
    pub task_ids: Vec<String>,
    pub active_task_id: Option<String>,
    pub status: AnalysisSessionStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AnalysisSessionRecord {
    pub fn summary(&self) -> AnalysisSessionSummary {
        AnalysisSessionSummary {
            session_id: self.session_id.clone(),
            title: self.title.clone(),
            source_url: self.source_url.clone(),
            instance_id: self.instance_id.clone(),
            node_id: self.node_id.clone(),
            system_context_count: self.system_context_ids.len(),
            skill_count: self.skill_ids.len(),
            upload_count: self.upload_ids.len(),
            task_count: self.task_ids.len(),
            active_task_id: self.active_task_id.clone(),
            status: self.status,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisSessionSummary {
    pub session_id: String,
    pub title: String,
    pub source_url: Option<String>,
    pub instance_id: Option<String>,
    pub node_id: Option<String>,
    pub system_context_count: usize,
    pub skill_count: usize,
    pub upload_count: usize,
    pub task_count: usize,
    pub active_task_id: Option<String>,
    pub status: AnalysisSessionStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisSessionListResponse {
    pub sessions: Vec<AnalysisSessionSummary>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAnalysisSessionRequest {
    pub title: Option<String>,
    pub question: Option<String>,
    pub source_url: Option<String>,
    pub instance_id: Option<String>,
    pub node_id: Option<String>,
    #[serde(default)]
    pub system_context_ids: Vec<String>,
    #[serde(default)]
    pub skill_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchAnalysisSessionRequest {
    pub title: Option<String>,
    pub question: Option<String>,
    #[serde(default)]
    pub source_url: Option<Option<String>>,
    #[serde(default)]
    pub instance_id: Option<Option<String>>,
    #[serde(default)]
    pub node_id: Option<Option<String>>,
    #[serde(default)]
    pub system_context_ids: Option<Vec<String>>,
    #[serde(default)]
    pub skill_ids: Option<Vec<String>>,
    pub status: Option<AnalysisSessionStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemContextKind {
    PromptPack,
    ArchitectureDoc,
    Runbook,
    Glossary,
    ToolCapability,
    MetadataInstance,
    KnowledgeNote,
    DiagnosticSkill,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemContextScope {
    Global,
    LogAnalysis,
    CaseImport,
    ToolRun,
}

impl Default for SystemContextScope {
    fn default() -> Self {
        Self::Global
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemContextVersionStatus {
    Draft,
    Active,
    Archived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemContextContentType {
    Text,
    Markdown,
    Mermaid,
    JsonSummary,
    MetadataAdapter,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemContextPromptPolicy {
    #[serde(default = "default_system_context_include_by_default")]
    pub include_by_default: bool,
    #[serde(default = "default_system_context_max_chars")]
    pub max_chars: usize,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub allowed_task_kinds: Vec<TaskKind>,
}

impl Default for SystemContextPromptPolicy {
    fn default() -> Self {
        Self {
            include_by_default: true,
            max_chars: default_system_context_max_chars(),
            priority: 0,
            allowed_task_kinds: Vec::new(),
        }
    }
}

pub fn default_system_context_include_by_default() -> bool {
    true
}

pub fn default_system_context_max_chars() -> usize {
    4000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemContextVersion {
    pub version_id: String,
    pub revision: u32,
    pub status: SystemContextVersionStatus,
    pub content_type: SystemContextContentType,
    pub content: String,
    pub summary: Option<String>,
    #[serde(default)]
    pub prompt_policy: SystemContextPromptPolicy,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemContextResource {
    pub schema_version: u32,
    pub context_id: String,
    pub kind: SystemContextKind,
    pub title: String,
    pub description: Option<String>,
    #[serde(default)]
    pub scope: SystemContextScope,
    pub enabled: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    pub product: Option<String>,
    pub version: Option<String>,
    pub environment: Option<String>,
    pub active_version_id: Option<String>,
    #[serde(default)]
    pub versions: Vec<SystemContextVersion>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl SystemContextResource {
    pub fn active_version(&self) -> Option<&SystemContextVersion> {
        self.active_version_id
            .as_deref()
            .and_then(|id| {
                self.versions
                    .iter()
                    .find(|version| version.version_id == id)
            })
            .or_else(|| {
                self.versions
                    .iter()
                    .find(|version| version.status == SystemContextVersionStatus::Active)
            })
    }

    #[allow(dead_code)]
    pub fn summary(&self, source: &'static str) -> SystemContextResourceSummary {
        let active = self.active_version();
        SystemContextResourceSummary {
            context_id: self.context_id.clone(),
            kind: self.kind,
            title: self.title.clone(),
            description: self.description.clone(),
            scope: self.scope,
            enabled: self.enabled,
            tags: self.tags.clone(),
            product: self.product.clone(),
            version: self.version.clone(),
            environment: self.environment.clone(),
            active_version_id: self.active_version_id.clone(),
            active_summary: active.and_then(|version| version.summary.clone()),
            content_type: active.map(|version| version.content_type),
            source: source.to_string(),
            updated_at: self.updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemContextResourceSummary {
    pub context_id: String,
    pub kind: SystemContextKind,
    pub title: String,
    pub description: Option<String>,
    pub scope: SystemContextScope,
    pub enabled: bool,
    pub tags: Vec<String>,
    pub product: Option<String>,
    pub version: Option<String>,
    pub environment: Option<String>,
    pub active_version_id: Option<String>,
    pub active_summary: Option<String>,
    pub content_type: Option<SystemContextContentType>,
    pub source: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemContextListResponse {
    pub resources: Vec<SystemContextResourceSummary>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSystemContextResourceRequest {
    pub kind: SystemContextKind,
    pub title: String,
    pub description: Option<String>,
    #[serde(default)]
    pub scope: SystemContextScope,
    #[serde(default = "default_system_context_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    pub product: Option<String>,
    pub version: Option<String>,
    pub environment: Option<String>,
    pub content_type: SystemContextContentType,
    pub content: String,
    pub summary: Option<String>,
    #[serde(default)]
    pub prompt_policy: SystemContextPromptPolicy,
}

pub fn default_system_context_enabled() -> bool {
    true
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchSystemContextResourceRequest {
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<Option<String>>,
    pub scope: Option<SystemContextScope>,
    pub enabled: Option<bool>,
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub product: Option<Option<String>>,
    #[serde(default)]
    pub version: Option<Option<String>>,
    #[serde(default)]
    pub environment: Option<Option<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSystemContextVersionRequest {
    pub content_type: SystemContextContentType,
    pub content: String,
    pub summary: Option<String>,
    #[serde(default)]
    pub prompt_policy: SystemContextPromptPolicy,
    #[serde(default)]
    pub activate: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchSystemContextVersionRequest {
    pub content_type: Option<SystemContextContentType>,
    pub content: Option<String>,
    #[serde(default)]
    pub summary: Option<Option<String>>,
    pub prompt_policy: Option<SystemContextPromptPolicy>,
    pub status: Option<SystemContextVersionStatus>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemContextPreviewRequest {
    #[serde(default)]
    pub context_ids: Vec<String>,
    pub task_kind: Option<TaskKind>,
    pub instance_id: Option<String>,
    pub product: Option<String>,
    pub version: Option<String>,
    pub environment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemContextBundle {
    pub schema_version: u32,
    pub resolved_at: DateTime<Utc>,
    pub resources: Vec<SystemContextBundleItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemContextBundleItem {
    pub context_id: String,
    pub version_id: Option<String>,
    pub kind: SystemContextKind,
    pub title: String,
    pub content_type: SystemContextContentType,
    pub summary: Option<String>,
    pub content: String,
    pub source: String,
    pub prompt_priority: i32,
    pub prompt_chars: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_root: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<SkillReferenceSummary>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemContextPreviewResponse {
    pub resources: Vec<SystemContextBundleItem>,
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillReferenceSummary {
    pub reference_id: String,
    pub path: String,
    pub title: String,
    pub summary: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachSessionUploadsRequest {
    pub upload_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisSessionEvent {
    pub schema_version: u32,
    pub session_id: String,
    pub event_type: String,
    pub task_id: Option<String>,
    pub upload_id: Option<String>,
    pub message: String,
    pub artifact_path: Option<String>,
    pub details: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTimelineEvent {
    pub source: String,
    pub event_type: String,
    pub session_id: String,
    pub task_id: Option<String>,
    pub phase: Option<TaskPhase>,
    pub action_id: Option<String>,
    pub message: String,
    pub evidence_refs: Vec<String>,
    pub artifact_path: Option<String>,
    pub details: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTimelineResponse {
    pub session_id: String,
    pub events: Vec<SessionTimelineEvent>,
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
