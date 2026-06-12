use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use serde::Deserialize;
use tracing::info;

use crate::support::config::AnalysisMode;
use crate::{
    app::AppState,
    domain::models::{
        default_task_question, AnalysisResult, CreateTaskRequest, SystemContextKind,
        TaskArtifactsResponse, TaskKind, TaskListResponse, TaskRecord, TaskResponse,
        TaskResultResponse, TaskSource, TaskStatus, UploadStatus,
    },
    http::system_context::metadata_context_bundle_item,
    pipeline::{
        prepare_raw_snapshot, write_case_context, write_metadata_context, write_session_text_input,
        write_system_context,
    },
    services::skill_registry::ResolveSkillsInput,
    stores::{
        analysis_state::{self, AnalysisSnapshotResponse},
        system_context_store::system_context_bundle,
    },
    support::{error::AppError, id::next_id},
};

pub struct CreateLogAnalysisTaskInput {
    pub session_id: String,
    pub upload_ids: Vec<String>,
    pub source_url: Option<String>,
    pub question: Option<String>,
    pub instance_id: Option<String>,
    pub cluster_id: Option<String>,
    pub node_id: Option<String>,
    pub analysis_mode: AnalysisMode,
    pub skill_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskMessageRequest {
    pub question_id: Option<String>,
    pub message: String,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalDecisionRequest {
    pub decision: ApprovalDecision,
    pub reason: Option<String>,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    Approved,
    Rejected,
}

pub async fn create_task(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateTaskRequest>,
) -> Result<(StatusCode, Json<TaskResponse>), AppError> {
    let session_id = normalize_optional_id(req.session_id.clone())
        .ok_or_else(|| AppError::bad_request("sessionId is required for log analysis tasks"))?;
    state
        .sessions
        .get(&session_id)
        .await
        .ok_or_else(|| AppError::bad_request(format!("unknown sessionId {session_id}")))?;
    let upload_ids = task_upload_ids(&req);
    let _legacy_system_context_ids = req.system_context_ids;
    let record = create_log_analysis_task(
        state.clone(),
        CreateLogAnalysisTaskInput {
            session_id,
            upload_ids,
            source_url: req.source_url,
            question: req.question,
            instance_id: req.instance_id,
            cluster_id: req.cluster_id,
            node_id: req.node_id,
            analysis_mode: req
                .analysis_mode
                .unwrap_or(state.config.claude_code.default_mode),
            skill_ids: req.skill_ids,
        },
    )
    .await?;
    state
        .executor
        .enqueue(state.clone(), record.task_id.clone());
    info!(
        task_id = %record.task_id,
        session_id = ?record.session_id,
        upload_count = record.upload_ids.len(),
        analysis_mode = %record.analysis_mode.as_str(),
        "log analysis task created through compatibility API"
    );
    Ok((
        StatusCode::ACCEPTED,
        Json(record.summary(&state.config.server.public_base_url)),
    ))
}

pub async fn create_log_analysis_task(
    state: Arc<AppState>,
    input: CreateLogAnalysisTaskInput,
) -> Result<TaskRecord, AppError> {
    let session_id = input.session_id;
    let upload_ids = input.upload_ids;
    info!(
        session_id = %session_id,
        upload_count = upload_ids.len(),
        analysis_mode = %input.analysis_mode.as_str(),
        "preparing log analysis task snapshot"
    );
    let mut uploads = Vec::with_capacity(upload_ids.len());
    for upload_id in &upload_ids {
        let upload = state
            .uploads
            .get(upload_id)
            .await
            .ok_or_else(|| AppError::bad_request(format!("unknown uploadId {upload_id}")))?;
        if upload.status != UploadStatus::Complete {
            return Err(AppError::bad_request(format!(
                "uploadId {upload_id} is not complete"
            )));
        }
        uploads.push(upload);
    }

    let metadata_context = state
        .metadata
        .resolve_task_context(
            normalize_optional_id(input.instance_id),
            normalize_optional_id(input.cluster_id),
            normalize_optional_id(input.node_id),
        )
        .await?;
    let task_id = next_id("task");
    let workspace = state.config.storage.workspace_dir(&task_id);
    let inputs = prepare_raw_snapshot(&workspace, &uploads).await?;
    let metadata_context_path = write_metadata_context(&workspace, &metadata_context).await?;
    let explicit_skill_ids = crate::http::skills::normalize_skill_ids(input.skill_ids)?;
    let mut system_context_items = state.skills.resolve_items(ResolveSkillsInput {
        explicit_skill_ids: &explicit_skill_ids,
        task_kind: TaskKind::LogAnalysis,
        product: metadata_context.product.as_deref(),
        version: metadata_context.version.as_deref(),
        environment: metadata_context.environment.as_deref(),
    })?;
    if metadata_context.instance_id.is_some() {
        system_context_items.push(metadata_context_bundle_item(&metadata_context));
    }
    let system_context = system_context_bundle(system_context_items);
    let system_context_path = write_system_context(&workspace, &system_context).await?;
    let question = normalize_question(input.question, state.config.llm.max_input_chars / 2)?;
    let text_input_path = write_session_text_input(&workspace, &question).await?;
    let recalled_cases = state.cases.search(Some(&question), 5, false).await;
    write_case_context(&workspace, &question, &recalled_cases).await?;
    state
        .sessions
        .record_event(
            &session_id,
            "metadata_context_recorded",
            Some(task_id.clone()),
            None,
            "metadata context snapshot recorded".to_string(),
            Some(metadata_context_path.display().to_string()),
            serde_json::json!({
                "instanceId": metadata_context.instance_id.clone(),
                "clusterId": metadata_context.cluster_id.clone(),
                "nodeId": metadata_context.node_id.clone(),
                "product": metadata_context.product.clone(),
                "version": metadata_context.version.clone(),
                "environment": metadata_context.environment.clone(),
                "clusterNodes": metadata_context.cluster_nodes.len(),
            }),
        )
        .map_err(|err| AppError::internal(format!("failed to record session event: {err}")))?;
    state
        .sessions
        .record_event(
            &session_id,
            "system_context_recorded",
            Some(task_id.clone()),
            None,
            "system context snapshot recorded".to_string(),
            Some(system_context_path.display().to_string()),
            serde_json::json!({
                "resourceCount": system_context.resources.len(),
                "skillCount": system_context.resources.iter().filter(|item| item.kind == SystemContextKind::DiagnosticSkill).count(),
                "resources": system_context.resources.iter().map(|item| serde_json::json!({
                    "contextId": item.context_id,
                    "versionId": item.version_id,
                    "kind": item.kind,
                    "title": item.title,
                    "source": item.source,
                    "skillId": item.skill_id,
                    "revision": item.revision,
                })).collect::<Vec<_>>(),
            }),
        )
        .map_err(|err| AppError::internal(format!("failed to record session event: {err}")))?;
    state
        .sessions
        .record_event(
            &session_id,
            "case_context_recorded",
            Some(task_id.clone()),
            None,
            "case recall context snapshot recorded".to_string(),
            Some(workspace.join("case_context.json").display().to_string()),
            serde_json::json!({
                "query": question.clone(),
                "caseRecallCount": recalled_cases.len(),
            }),
        )
        .map_err(|err| AppError::internal(format!("failed to record session event: {err}")))?;
    if upload_ids.is_empty() {
        state
            .sessions
            .record_event(
                &session_id,
                "text_input_recorded",
                Some(task_id.clone()),
                None,
                "text-only analysis input recorded from session question".to_string(),
                Some(text_input_path.display().to_string()),
                serde_json::json!({
                    "questionChars": question.chars().count(),
                    "uploadCount": 0,
                    "evidenceRef": "session_text_input.json#question",
                }),
            )
            .map_err(|err| AppError::internal(format!("failed to record session event: {err}")))?;
    }
    let now = Utc::now();
    let record = TaskRecord {
        schema_version: 7,
        task_id: task_id.clone(),
        alias: None,
        session_id: Some(session_id.clone()),
        task_kind: TaskKind::LogAnalysis,
        analysis_mode: input.analysis_mode,
        source: TaskSource::Upload,
        upload_ids,
        inputs,
        source_url: input.source_url,
        tool_id: None,
        tool_params: serde_json::Value::Null,
        tool_result_path: None,
        instance_id: metadata_context.instance_id.clone(),
        cluster_id: metadata_context.cluster_id.clone(),
        node_id: metadata_context.node_id.clone(),
        question,
        status: TaskStatus::Queued,
        phase: None,
        attempts: 0,
        error: None,
        manifest_path: None,
        grep_results_path: None,
        metadata_context_path: Some(metadata_context_path.display().to_string()),
        system_context_path: Some(system_context_path.display().to_string()),
        result_json_path: None,
        result_markdown_path: None,
        created_at: now,
        updated_at: now,
    };
    state
        .tasks
        .create(record.clone())
        .await
        .map_err(|err| AppError::internal(format!("failed to persist task: {err}")))?;
    state
        .sessions
        .add_task_run(&session_id, &task_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to update session: {err}")))?;
    info!(
        task_id = %task_id,
        session_id = %session_id,
        upload_count = record.upload_ids.len(),
        input_count = record.inputs.len(),
        case_recall_count = recalled_cases.len(),
        system_context_count = system_context.resources.len(),
        "log analysis task persisted"
    );
    Ok(record)
}

pub async fn task_result(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> Result<Json<TaskResultResponse>, AppError> {
    validate_task_id(&task_id)?;
    let task = state
        .tasks
        .get(&task_id)
        .await
        .ok_or_else(|| AppError::not_found(format!("unknown taskId {task_id}")))?;
    if task.status != TaskStatus::Succeeded {
        return Err(AppError::conflict(
            "task result is only available after success",
            serde_json::json!({ "status": task.status }),
        ));
    }
    if task.task_kind != TaskKind::LogAnalysis {
        return Err(AppError::bad_request("task is not a log analysis task"));
    }
    let result_json_path = task
        .result_json_path
        .map(std::path::PathBuf::from)
        .ok_or_else(|| AppError::internal("successful task is missing resultJsonPath"))?;
    let result_markdown_path = task
        .result_markdown_path
        .map(std::path::PathBuf::from)
        .ok_or_else(|| AppError::internal("successful task is missing resultMarkdownPath"))?;
    let result = read_typed_json_file::<AnalysisResult>(&result_json_path).await?;
    Ok(Json(TaskResultResponse {
        task_id,
        result_json_path: result_json_path.display().to_string(),
        result_markdown_path: result_markdown_path.display().to_string(),
        result,
    }))
}

pub async fn list_tasks(
    State(state): State<Arc<AppState>>,
) -> Result<Json<TaskListResponse>, AppError> {
    let tasks = state
        .tasks
        .list()
        .await
        .into_iter()
        .filter(|task| task.task_kind == TaskKind::LogAnalysis)
        .map(|task| task.summary(&state.config.server.public_base_url))
        .collect();
    Ok(Json(TaskListResponse { tasks }))
}

pub async fn get_task(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> Result<Json<TaskRecord>, AppError> {
    validate_task_id(&task_id)?;
    state
        .tasks
        .get(&task_id)
        .await
        .map(Json)
        .ok_or_else(|| AppError::not_found(format!("unknown taskId {task_id}")))
}

pub async fn task_analysis(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> Result<Json<AnalysisSnapshotResponse>, AppError> {
    validate_task_id(&task_id)?;
    state
        .tasks
        .get(&task_id)
        .await
        .ok_or_else(|| AppError::not_found(format!("unknown taskId {task_id}")))?;
    let workspace = state.config.storage.workspace_dir(&task_id);
    analysis_state::read_snapshot(&workspace)
        .map(Json)
        .map_err(|err| AppError::not_found(format!("analysis state not found: {err}")))
}

pub async fn post_task_message(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
    Json(req): Json<TaskMessageRequest>,
) -> Result<Json<TaskResponse>, AppError> {
    validate_task_id(&task_id)?;
    let message = req.message.trim().to_string();
    if message.is_empty() {
        return Err(AppError::bad_request("message must not be empty"));
    }
    if message.chars().count() > state.config.llm.max_input_chars / 2 {
        return Err(AppError::bad_request("message is too long"));
    }
    let task = state
        .tasks
        .get(&task_id)
        .await
        .ok_or_else(|| AppError::not_found(format!("unknown taskId {task_id}")))?;
    let workspace = state.config.storage.workspace_dir(&task_id);
    let snapshot = analysis_state::read_snapshot(&workspace).map_err(|err| {
        AppError::conflict(
            "analysis state is not available",
            serde_json::json!({"errorDetail": err.to_string()}),
        )
    })?;

    if let Some(key) = req
        .idempotency_key
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        if snapshot
            .state
            .user_messages
            .iter()
            .any(|record| record.message_id == key)
        {
            info!(
                task_id = %task_id,
                idempotency_key = %key,
                "duplicate user message ignored"
            );
            return Ok(Json(task.summary(&state.config.server.public_base_url)));
        }
    }

    if task.status != TaskStatus::WaitingForUser {
        return Err(AppError::conflict(
            "task is not waiting for user input",
            serde_json::json!({ "status": task.status }),
        ));
    }

    let question_id = normalize_optional_id(req.question_id);
    if let Some(question_id) = &question_id {
        if !snapshot
            .state
            .pending_user_prompts
            .iter()
            .any(|prompt| prompt.question_id == *question_id)
        {
            return Err(AppError::bad_request(format!(
                "unknown pending questionId {question_id}"
            )));
        }
    }
    let message_id = req
        .idempotency_key
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| next_id("msg"));
    analysis_state::record_user_message(
        &workspace,
        message_id.clone(),
        question_id.clone(),
        message,
    )
    .map_err(|err| AppError::internal(format!("failed to record user message: {err}")))?;
    let resumed = state
        .tasks
        .resume_waiting(&task_id, TaskStatus::WaitingForUser)
        .await
        .map_err(|err| {
            AppError::conflict(
                "failed to resume task",
                serde_json::json!({"errorDetail": err.to_string()}),
            )
        })?;
    state
        .sessions
        .sync_task_status(&resumed)
        .await
        .map_err(|err| AppError::internal(format!("failed to sync session status: {err}")))?;
    state.executor.enqueue(state.clone(), task_id);
    info!(
        task_id = %resumed.task_id,
        question_id = ?question_id,
        message_id = %message_id,
        "user message recorded and task resumed"
    );
    Ok(Json(resumed.summary(&state.config.server.public_base_url)))
}

pub async fn post_action_decision(
    State(state): State<Arc<AppState>>,
    Path((task_id, action_id)): Path<(String, String)>,
    Json(req): Json<ApprovalDecisionRequest>,
) -> Result<Json<TaskResponse>, AppError> {
    validate_task_id(&task_id)?;
    validate_action_id(&action_id)?;
    let task = state
        .tasks
        .get(&task_id)
        .await
        .ok_or_else(|| AppError::not_found(format!("unknown taskId {task_id}")))?;
    let workspace = state.config.storage.workspace_dir(&task_id);
    let snapshot = analysis_state::read_snapshot(&workspace).map_err(|err| {
        AppError::conflict(
            "analysis state is not available",
            serde_json::json!({"errorDetail": err.to_string()}),
        )
    })?;
    let idempotency_key = req
        .idempotency_key
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if let Some(key) = idempotency_key.as_deref() {
        if snapshot.events.iter().any(|event| {
            event.event_type == analysis_state::AnalysisEventType::ApprovalDecisionRecorded
                && event
                    .details
                    .get("idempotencyKey")
                    .and_then(|value| value.as_str())
                    == Some(key)
        }) {
            info!(
                task_id = %task_id,
                action_id = %action_id,
                idempotency_key = %key,
                "duplicate approval decision ignored"
            );
            return Ok(Json(task.summary(&state.config.server.public_base_url)));
        }
    }

    if task.status != TaskStatus::WaitingForApproval {
        return Err(AppError::conflict(
            "task is not waiting for approval",
            serde_json::json!({ "status": task.status }),
        ));
    }
    let pending = snapshot
        .state
        .pending_approvals
        .iter()
        .find(|approval| approval.action_id == action_id)
        .cloned()
        .ok_or_else(|| AppError::bad_request(format!("unknown pending actionId {action_id}")))?;
    let approved = req.decision == ApprovalDecision::Approved;
    if approved {
        write_mock_environment_evidence(&workspace, &pending).map_err(|err| {
            AppError::internal(format!("failed to write environment evidence: {err}"))
        })?;
    }
    analysis_state::record_approval_decision(
        &workspace,
        &action_id,
        approved,
        req.reason
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        idempotency_key,
    )
    .map_err(|err| AppError::internal(format!("failed to record approval decision: {err}")))?;
    let resumed = state
        .tasks
        .resume_waiting(&task_id, TaskStatus::WaitingForApproval)
        .await
        .map_err(|err| {
            AppError::conflict(
                "failed to resume task",
                serde_json::json!({"errorDetail": err.to_string()}),
            )
        })?;
    state
        .sessions
        .sync_task_status(&resumed)
        .await
        .map_err(|err| AppError::internal(format!("failed to sync session status: {err}")))?;
    state.executor.enqueue(state.clone(), task_id);
    info!(
        task_id = %resumed.task_id,
        action_id = %action_id,
        approved,
        "approval decision recorded and task resumed"
    );
    Ok(Json(resumed.summary(&state.config.server.public_base_url)))
}

pub async fn task_artifacts(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> Result<Json<TaskArtifactsResponse>, AppError> {
    validate_task_id(&task_id)?;
    let task = state
        .tasks
        .get(&task_id)
        .await
        .ok_or_else(|| AppError::not_found(format!("unknown taskId {task_id}")))?;
    if task.status != TaskStatus::Succeeded {
        return Err(AppError::conflict(
            "task artifacts are only available after success",
            serde_json::json!({ "status": task.status }),
        ));
    }
    if task.task_kind != TaskKind::LogAnalysis {
        return Err(AppError::bad_request("task is not a log analysis task"));
    }
    let manifest_path = task
        .manifest_path
        .map(std::path::PathBuf::from)
        .ok_or_else(|| AppError::internal("successful task is missing manifestPath"))?;
    let grep_results_path = task
        .grep_results_path
        .map(std::path::PathBuf::from)
        .ok_or_else(|| AppError::internal("successful task is missing grepResultsPath"))?;
    let manifest = read_json_file(&manifest_path).await?;
    let grep_results = read_json_file(&grep_results_path).await?;
    let metadata_context_path = match task.metadata_context_path {
        Some(path) => {
            let path = std::path::PathBuf::from(path);
            let expected = state
                .config
                .storage
                .workspace_dir(&task_id)
                .join("metadata_context.json");
            if path != expected {
                return Err(AppError::internal(
                    "task contains invalid metadataContextPath",
                ));
            }
            Some(path)
        }
        None => None,
    };
    let metadata_context = match metadata_context_path.as_deref() {
        Some(path) => Some(read_json_file(path).await?),
        None => None,
    };
    let text_input_path = state
        .config
        .storage
        .workspace_dir(&task_id)
        .join("session_text_input.json");
    let (text_input_path, text_input) = match tokio::fs::read_to_string(&text_input_path).await {
        Ok(raw) => {
            let value = serde_json::from_str(&raw).map_err(|err| {
                AppError::internal(format!("failed to parse session text input JSON: {err}"))
            })?;
            (Some(text_input_path.display().to_string()), Some(value))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => (None, None),
        Err(err) => {
            return Err(AppError::internal(format!(
                "failed to read session text input: {err}"
            )))
        }
    };
    let case_context_path = state
        .config
        .storage
        .workspace_dir(&task_id)
        .join("case_context.json");
    let (case_context_path, case_context) =
        match tokio::fs::read_to_string(&case_context_path).await {
            Ok(raw) => {
                let value = serde_json::from_str(&raw).map_err(|err| {
                    AppError::internal(format!("failed to parse case context JSON: {err}"))
                })?;
                (Some(case_context_path.display().to_string()), Some(value))
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => (None, None),
            Err(err) => {
                return Err(AppError::internal(format!(
                    "failed to read case context: {err}"
                )))
            }
        };
    let system_context_path = match task.system_context_path {
        Some(path) => {
            let path = std::path::PathBuf::from(path);
            let expected = state
                .config
                .storage
                .workspace_dir(&task_id)
                .join("system_context.json");
            if path != expected {
                return Err(AppError::internal(
                    "task contains invalid systemContextPath",
                ));
            }
            Some(path)
        }
        None => None,
    };
    let system_context = match system_context_path.as_deref() {
        Some(path) => Some(read_json_file(path).await?),
        None => None,
    };
    let workspace = state.config.storage.workspace_dir(&task_id);
    let (analysis_package_path, analysis_package) =
        read_optional_artifact(&workspace, "analysis_package.json").await?;
    let (agent_response_path, agent_response) =
        read_optional_artifact(&workspace, "agent_response.json").await?;
    let (claude_mcp_config_path, claude_mcp_config) =
        read_optional_artifact(&workspace, "claude_mcp_config.json").await?;
    let (claude_session_path, claude_session) =
        read_optional_artifact(&workspace, "claude_session.json").await?;
    let (mcp_calls_path, mcp_calls) =
        read_optional_jsonl_artifact(&workspace, "mcp_calls.jsonl").await?;
    let tool_results = read_tool_results(&workspace).await?;

    Ok(Json(TaskArtifactsResponse {
        task_id,
        manifest_path: manifest_path.display().to_string(),
        grep_results_path: grep_results_path.display().to_string(),
        manifest,
        grep_results,
        text_input_path,
        text_input,
        metadata_context_path: metadata_context_path.map(|path| path.display().to_string()),
        metadata_context,
        case_context_path,
        case_context,
        system_context_path: system_context_path.map(|path| path.display().to_string()),
        system_context,
        analysis_package_path,
        analysis_package,
        agent_response_path,
        agent_response,
        claude_mcp_config_path,
        claude_mcp_config,
        claude_session_path,
        claude_session,
        mcp_calls_path,
        mcp_calls,
        tool_results,
    }))
}

fn task_upload_ids(req: &CreateTaskRequest) -> Vec<String> {
    let mut upload_ids = Vec::new();
    if let Some(upload_id) = req.upload_id.as_ref().filter(|value| !value.is_empty()) {
        upload_ids.push(upload_id.clone());
    }
    for upload_id in req.upload_ids.iter().filter(|value| !value.is_empty()) {
        if !upload_ids.iter().any(|value| value == upload_id) {
            upload_ids.push(upload_id.clone());
        }
    }
    upload_ids
}

fn normalize_question(question: Option<String>, max_chars: usize) -> Result<String, AppError> {
    let question = question
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(default_task_question);
    if question.chars().count() > max_chars {
        return Err(AppError::bad_request(format!(
            "question exceeds maximum length of {max_chars} characters"
        )));
    }
    Ok(question)
}

fn normalize_optional_id(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

async fn read_json_file(path: &std::path::Path) -> Result<serde_json::Value, AppError> {
    let raw = tokio::fs::read_to_string(path)
        .await
        .map_err(|err| AppError::internal(format!("artifact not found: {err}")))?;
    serde_json::from_str(&raw)
        .map_err(|err| AppError::internal(format!("failed to parse artifact JSON: {err}")))
}

async fn read_optional_artifact(
    workspace: &std::path::Path,
    name: &str,
) -> Result<(Option<String>, Option<serde_json::Value>), AppError> {
    let path = workspace.join(name);
    match tokio::fs::read_to_string(&path).await {
        Ok(raw) => {
            let value = serde_json::from_str(&raw).map_err(|err| {
                AppError::internal(format!("failed to parse artifact {name}: {err}"))
            })?;
            Ok((Some(path.display().to_string()), Some(value)))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok((None, None)),
        Err(err) => Err(AppError::internal(format!(
            "failed to read artifact {name}: {err}"
        ))),
    }
}

async fn read_optional_jsonl_artifact(
    workspace: &std::path::Path,
    name: &str,
) -> Result<(Option<String>, Vec<serde_json::Value>), AppError> {
    let path = workspace.join(name);
    let raw = match tokio::fs::read_to_string(&path).await {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok((None, Vec::new())),
        Err(err) => {
            return Err(AppError::internal(format!(
                "failed to read artifact {name}: {err}"
            )))
        }
    };
    let mut values = Vec::new();
    for (index, line) in raw.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let value = serde_json::from_str(line).map_err(|err| {
            AppError::internal(format!("failed to parse {name} line {}: {err}", index + 1))
        })?;
        values.push(value);
    }
    Ok((Some(path.display().to_string()), values))
}

async fn read_typed_json_file<T: serde::de::DeserializeOwned>(
    path: &std::path::Path,
) -> Result<T, AppError> {
    let raw = tokio::fs::read_to_string(path)
        .await
        .map_err(|err| AppError::internal(format!("result not found: {err}")))?;
    serde_json::from_str(&raw)
        .map_err(|err| AppError::internal(format!("failed to parse result JSON: {err}")))
}

async fn read_tool_results(
    workspace: &std::path::Path,
) -> Result<Vec<serde_json::Value>, AppError> {
    let root = workspace.join("tool_results");
    let mut entries = match tokio::fs::read_dir(&root).await {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => {
            return Err(AppError::internal(format!(
                "failed to read tool results: {err}"
            )))
        }
    };
    let mut paths = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|err| AppError::internal(format!("failed to list tool results: {err}")))?
    {
        let result_path = entry.path().join("result.json");
        if result_path.exists() {
            paths.push(result_path);
        }
    }
    paths.sort();
    let mut results = Vec::with_capacity(paths.len());
    for path in paths {
        results.push(read_json_file(&path).await?);
    }
    Ok(results)
}

fn validate_task_id(task_id: &str) -> Result<(), AppError> {
    let valid = task_id.starts_with("task_")
        && task_id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'_' || value == b'-');
    if valid {
        Ok(())
    } else {
        Err(AppError::bad_request("invalid taskId"))
    }
}

fn validate_action_id(action_id: &str) -> Result<(), AppError> {
    let valid = action_id.starts_with("act_")
        && action_id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'_' || value == b'-');
    if valid {
        Ok(())
    } else {
        Err(AppError::bad_request("invalid actionId"))
    }
}

fn write_mock_environment_evidence(
    workspace: &std::path::Path,
    pending: &analysis_state::PendingApproval,
) -> anyhow::Result<()> {
    let result_dir = workspace
        .join("environment_evidence")
        .join(&pending.action_id);
    std::fs::create_dir_all(&result_dir)?;
    let artifact_path = format!("environment_evidence/{}/result.json", pending.action_id);
    let result = serde_json::json!({
        "schemaVersion": 1,
        "actionId": pending.action_id,
        "status": "MOCK",
        "summary": "mock environment evidence captured after user approval",
        "input": pending.input,
        "createdAt": Utc::now(),
    });
    std::fs::write(
        result_dir.join("result.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    analysis_state::record_environment_artifact(
        workspace,
        &pending.action_id,
        artifact_path,
        "mock environment evidence captured after user approval".to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use std::{
        collections::BTreeMap,
        os::unix::fs::PermissionsExt,
        sync::atomic::{AtomicU64, Ordering},
    };
    use tower::ServiceExt;

    use crate::{
        domain::models::{
            AnalysisSessionRecord, AnalysisSessionStatus, TaskInput, UploadRecord, UploadStatus,
        },
        http,
        services::metadata::MetadataImportRequest,
        support::config::{
            AnalysisMode, AnalysisSettings, AppConfig, AuthSettings, ClaudeCodeSettings,
            EmbeddingSettings, LlmProvider, LlmSettings, LogAnalyzerSettings, McpSettings,
            PermissionProfileSettings, ServerSettings, SkillSettings, StorageSettings,
            ToolsSettings,
        },
    };

    #[test]
    fn validates_question_length() {
        assert_eq!(
            normalize_question(None, 100).unwrap(),
            default_task_question()
        );
        assert!(normalize_question(Some("x".repeat(11)), 10).is_err());
    }

    #[tokio::test]
    async fn task_api_creates_lists_and_reads_details() {
        let (state, root) = test_state();
        create_test_session(&state, "sess_test").await;
        create_test_upload(&state, "upl_test", UploadStatus::Complete).await;
        let app = http::router(state.clone()).with_state(state.clone());
        let response = app
            .clone()
            .oneshot(
                Request::post("/api/tasks")
                    .header("authorization", "Bearer test-key")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"sessionId":"sess_test","uploadId":"upl_test","question":"Why did the sample fail?"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(
            status,
            StatusCode::ACCEPTED,
            "unexpected response: {}",
            String::from_utf8_lossy(&body)
        );
        let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let task_id = created["taskId"].as_str().unwrap();

        let list = app
            .clone()
            .oneshot(
                Request::get("/api/tasks")
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list.status(), StatusCode::OK);

        let mut terminal = None;
        for _ in 0..100 {
            let detail = app
                .clone()
                .oneshot(
                    Request::get(format!("/api/tasks/{task_id}"))
                        .header("authorization", "Bearer test-key")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let body = to_bytes(detail.into_body(), usize::MAX).await.unwrap();
            let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
            if body["status"] == "SUCCEEDED" || body["status"] == "FAILED" {
                terminal = Some(body);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let terminal = terminal.expect("task did not reach a terminal state");
        assert_eq!(terminal["status"], "SUCCEEDED", "{terminal}");
        assert_eq!(terminal["question"], "Why did the sample fail?");
        let alias = terminal["alias"].as_str().expect("task alias is set");
        assert!(!alias.trim().is_empty());
        assert!(!alias.contains("task_"));

        let result = app
            .clone()
            .oneshot(
                Request::get(format!("/api/tasks/{task_id}/result"))
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(result.status(), StatusCode::OK);
        let body = to_bytes(result.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["result"]["schemaVersion"], 1);
        assert!(body["result"]["summary"]
            .as_str()
            .unwrap()
            .contains("Why did the sample fail?"));

        let analysis = app
            .oneshot(
                Request::get(format!("/api/tasks/{task_id}/analysis"))
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(analysis.status(), StatusCode::OK);
        let body = to_bytes(analysis.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["state"]["taskId"], task_id);
        assert_eq!(body["state"]["status"], "SUCCEEDED");
        assert!(body["state"]["evidence"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["evidenceType"] == "log_search"));
        assert!(body["events"].as_array().unwrap().len() >= 3);
        assert!(!body["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["eventType"]
                .as_str()
                .unwrap_or_default()
                .contains("task_alias")
                || event["details"].to_string().contains("task_alias")));
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn session_task_can_run_with_question_only() {
        let (state, root) = test_state();
        create_test_session(&state, "sess_test").await;
        let app = http::router(state.clone()).with_state(state);

        let response = app
            .clone()
            .oneshot(
                Request::post("/api/sessions/sess_test/tasks")
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(
            status,
            StatusCode::ACCEPTED,
            "unexpected response: {}",
            String::from_utf8_lossy(&body)
        );
        let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let task_id = created["taskId"].as_str().unwrap();

        let terminal = wait_for_task_status(&app, task_id, "SUCCEEDED").await;
        assert!(terminal["uploadIds"].as_array().unwrap().is_empty());
        assert!(terminal["inputs"].as_array().unwrap().is_empty());

        let artifacts = app
            .clone()
            .oneshot(
                Request::get(format!("/api/tasks/{task_id}/artifacts"))
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(artifacts.status(), StatusCode::OK);
        let body = to_bytes(artifacts.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["manifest"]["filename"], "session_text_input");
        assert!(body["manifest"]["uploadIds"].as_array().unwrap().is_empty());
        assert!(body["manifest"]["uploads"].as_array().unwrap().is_empty());
        assert!(body["manifest"]["files"].as_array().unwrap().is_empty());
        assert_eq!(body["grepResults"]["totalMatches"], 0);
        assert_eq!(body["textInput"]["question"], default_task_question());
        assert!(body["textInputPath"]
            .as_str()
            .unwrap()
            .ends_with("session_text_input.json"));

        let timeline = app
            .oneshot(
                Request::get("/api/sessions/sess_test/timeline")
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(timeline.status(), StatusCode::OK);
        let body = to_bytes(timeline.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(body["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["eventType"] == "text_input_recorded"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn task_api_returns_not_found_and_artifact_conflict() {
        let (state, root) = test_state();
        let now = Utc::now();
        state
            .tasks
            .create(TaskRecord {
                schema_version: 1,
                task_id: "task_queued".to_string(),
                alias: None,
                session_id: Some("sess_test".to_string()),
                task_kind: TaskKind::LogAnalysis,
                analysis_mode: AnalysisMode::Diagnose,
                source: TaskSource::Upload,
                upload_ids: vec!["upl_test".to_string()],
                inputs: vec![],
                source_url: None,
                tool_id: None,
                tool_params: serde_json::Value::Null,
                tool_result_path: None,
                instance_id: None,
                cluster_id: None,
                node_id: None,
                question: default_task_question(),
                status: TaskStatus::Queued,
                phase: None,
                attempts: 0,
                error: None,
                manifest_path: None,
                grep_results_path: None,
                metadata_context_path: None,
                system_context_path: None,
                result_json_path: None,
                result_markdown_path: None,
                created_at: now,
                updated_at: now,
            })
            .await
            .unwrap();
        let app = http::router(state.clone()).with_state(state);

        let missing = app
            .clone()
            .oneshot(
                Request::get("/api/tasks/task_missing")
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);

        let conflict = app
            .clone()
            .oneshot(
                Request::get("/api/tasks/task_queued/artifacts")
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(conflict.status(), StatusCode::CONFLICT);
        let body = to_bytes(conflict.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["status"], "QUEUED");

        let result_conflict = app
            .oneshot(
                Request::get("/api/tasks/task_queued/result")
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(result_conflict.status(), StatusCode::CONFLICT);
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn successful_task_can_be_confirmed_as_case_and_recalled() {
        let (state, root) = test_state();
        create_test_session(&state, "sess_test").await;
        create_test_upload(&state, "upl_case", UploadStatus::Complete).await;
        let app = http::router(state.clone()).with_state(state.clone());
        let response = app
            .clone()
            .oneshot(
                Request::post("/api/tasks")
                    .header("authorization", "Bearer test-key")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"sessionId":"sess_test","uploadId":"upl_case","question":"slow query has no time filter"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let task_id = created["taskId"].as_str().unwrap();
        wait_for_task_status(&app, task_id, "SUCCEEDED").await;

        let response = app
            .clone()
            .oneshot(
                Request::post(format!("/api/tasks/{task_id}/case"))
                    .header("authorization", "Bearer test-key")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"title":"No time filter case","rootCause":"missing time filter","solution":"add bounded time predicate"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let case_id = body["case"]["caseId"].as_str().unwrap();
        assert_eq!(body["case"]["schemaVersion"], 2);
        assert_eq!(body["case"]["sourceType"], "task");
        assert_eq!(body["case"]["taskId"], task_id);
        assert_eq!(body["case"]["rootCause"], "missing time filter");

        let list = app
            .clone()
            .oneshot(
                Request::get("/api/cases?query=time%20filter")
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list.status(), StatusCode::OK);
        let body = to_bytes(list.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["cases"][0]["caseId"], case_id);
        assert!(body["cases"][0]["score"].as_f64().unwrap() > 0.0);

        create_test_upload(&state, "upl_case_recall", UploadStatus::Complete).await;
        let response = app
            .clone()
            .oneshot(
                Request::post("/api/tasks")
                    .header("authorization", "Bearer test-key")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"sessionId":"sess_test","uploadId":"upl_case_recall","question":"time filter regression"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let recall_task_id = created["taskId"].as_str().unwrap();
        wait_for_task_status(&app, recall_task_id, "SUCCEEDED").await;
        let artifacts = app
            .clone()
            .oneshot(
                Request::get(format!("/api/tasks/{recall_task_id}/artifacts"))
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(artifacts.status(), StatusCode::OK);
        let body = to_bytes(artifacts.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["caseContext"]["cases"][0]["caseId"], case_id);

        let disabled = app
            .clone()
            .oneshot(
                Request::patch(format!("/api/cases/{case_id}"))
                    .header("authorization", "Bearer test-key")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"enabled":false}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(disabled.status(), StatusCode::OK);

        let list = app
            .clone()
            .oneshot(
                Request::get("/api/cases?query=time%20filter")
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = to_bytes(list.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(body["cases"].as_array().unwrap().is_empty());

        let list = app
            .oneshot(
                Request::get("/api/cases?query=time%20filter&includeDisabled=true")
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = to_bytes(list.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["cases"][0]["caseId"], case_id);
        assert_eq!(body["cases"][0]["enabled"], false);
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn manual_case_can_be_created_and_recalled() {
        let (state, root) = test_state();
        let app = http::router(state.clone()).with_state(state.clone());
        let response = app
            .clone()
            .oneshot(
                Request::post("/api/cases")
                    .header("authorization", "Bearer test-key")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"title":"Manual WAL saturation","symptom":"write latency increased","rootCause":"wal disk saturation","solution":"move shards and expand disk","product":"opengemini","instanceId":"inst-prod-1","nodeId":"node-3","evidenceRefs":["INC-123"]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let case_id = body["case"]["caseId"].as_str().unwrap();
        assert_eq!(body["case"]["schemaVersion"], 2);
        assert_eq!(body["case"]["sourceType"], "manual");
        assert!(body["case"]["taskId"].is_null());
        assert!(body["case"]["sourceResultPath"].is_null());

        let list = app
            .clone()
            .oneshot(
                Request::get("/api/cases?query=inst-prod-1%20wal")
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list.status(), StatusCode::OK);
        let body = to_bytes(list.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["cases"][0]["caseId"], case_id);

        let updated = app
            .oneshot(
                Request::patch(format!("/api/cases/{case_id}"))
                    .header("authorization", "Bearer test-key")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"environment":"prod","nodeId":"node-4"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(updated.status(), StatusCode::OK);
        let body = to_bytes(updated.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["case"]["environment"], "prod");
        assert_eq!(body["case"]["nodeId"], "node-4");
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn case_import_can_collect_missing_info_and_confirm() {
        let (state, root) = test_state();
        let app = http::router(state.clone()).with_state(state.clone());
        let created = app
            .clone()
            .oneshot(
                Request::post("/api/cases/imports")
                    .header("authorization", "Bearer test-key")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"text":"Title: Manual WAL saturation\nSymptom: write latency increased\nRoot cause: wal disk saturation"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(created.status(), StatusCode::CREATED);
        let body = to_bytes(created.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let draft_id = body["draft"]["draftId"].as_str().unwrap();
        assert_eq!(body["draft"]["readyToConfirm"], false);
        assert_eq!(body["draft"]["missingFields"][0]["field"], "solution");

        let answered = app
            .clone()
            .oneshot(
                Request::post(format!("/api/cases/imports/{draft_id}/messages"))
                    .header("authorization", "Bearer test-key")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"message":"Solution: move shards and expand disk"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(answered.status(), StatusCode::OK);
        let body = to_bytes(answered.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["draft"]["readyToConfirm"], true);

        let confirmed = app
            .clone()
            .oneshot(
                Request::post(format!("/api/cases/imports/{draft_id}/confirm"))
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(confirmed.status(), StatusCode::CREATED);
        let body = to_bytes(confirmed.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["case"]["sourceType"], "manual");
        assert_eq!(body["case"]["solution"], "move shards and expand disk");

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn task_artifacts_include_tool_results() {
        let (state, root) = test_state();
        let task_id = "task_tool_artifacts";
        let workspace = state.config.storage.workspace_dir(task_id);
        std::fs::create_dir_all(workspace.join("tool_results/act_tool_fake")).unwrap();
        let manifest_path = workspace.join("manifest.json");
        let grep_path = workspace.join("grep_results.json");
        std::fs::write(
            &manifest_path,
            r#"{"uploadId":"upl_1","uploadIds":["upl_1"],"uploads":[],"taskId":"task_tool_artifacts","source":"upload","filename":"sample.log","sourceUrl":null,"files":[]}"#,
        )
        .unwrap();
        std::fs::write(
            &grep_path,
            r#"{"keywords":[],"totalMatches":0,"matches":[]}"#,
        )
        .unwrap();
        std::fs::write(
            workspace.join("tool_results/act_tool_fake/result.json"),
            r#"{"schemaVersion":1,"tool":"fake","actionId":"act_tool_fake","status":"OK","exitCode":0,"durationMs":1,"command":["/bin/echo"],"inputFile":"extracted/sample.log","stdoutPath":"tool_results/act_tool_fake/stdout.txt","stderrPath":"tool_results/act_tool_fake/stderr.txt","summary":"tool completed","error":null}"#,
        )
        .unwrap();
        std::fs::write(
            workspace.join("analysis_package.json"),
            r#"{"schemaVersion":2,"runtimeStatus":"ready_for_claude_code"}"#,
        )
        .unwrap();
        std::fs::write(
            workspace.join("claude_mcp_config.json"),
            r#"{"mcpServers":{"logagent":{"command":"/usr/bin/logagent-server","args":["mcp"]}}}"#,
        )
        .unwrap();
        std::fs::write(
            workspace.join("agent_response.json"),
            r#"{"schemaVersion":2,"runtimeStatus":"succeeded","claudeSessionId":"sess-test","structuredOutput":{"runtimeStatus":"completed"},"durationMs":1}"#,
        )
        .unwrap();
        std::fs::write(
            workspace.join("claude_session.json"),
            r#"{"schemaVersion":1,"runtimeStatus":"succeeded","claudeSessionId":"sess-test","mcpConfigPath":"claude_mcp_config.json"}"#,
        )
        .unwrap();
        std::fs::write(
            workspace.join("mcp_calls.jsonl"),
            r#"{"schemaVersion":1,"name":"logagent.search_logs","status":"succeeded"}"#,
        )
        .unwrap();
        let now = Utc::now();
        state
            .tasks
            .create(TaskRecord {
                schema_version: 4,
                task_id: task_id.to_string(),
                alias: None,
                session_id: Some("sess_test".to_string()),
                task_kind: TaskKind::LogAnalysis,
                analysis_mode: AnalysisMode::Diagnose,
                source: TaskSource::Upload,
                upload_ids: vec!["upl_1".to_string()],
                inputs: vec![TaskInput {
                    upload_id: "upl_1".to_string(),
                    filename: "sample.log".to_string(),
                    size: 1,
                    raw_path: "raw/upl_1/sample.log".to_string(),
                }],
                source_url: None,
                tool_id: None,
                tool_params: serde_json::Value::Null,
                tool_result_path: None,
                instance_id: None,
                cluster_id: None,
                node_id: None,
                question: default_task_question(),
                status: TaskStatus::Succeeded,
                phase: None,
                attempts: 1,
                error: None,
                manifest_path: Some(manifest_path.display().to_string()),
                grep_results_path: Some(grep_path.display().to_string()),
                metadata_context_path: None,
                system_context_path: None,
                result_json_path: None,
                result_markdown_path: None,
                created_at: now,
                updated_at: now,
            })
            .await
            .unwrap();
        let app = http::router(state.clone()).with_state(state);

        let response = app
            .oneshot(
                Request::get(format!("/api/tasks/{task_id}/artifacts"))
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["toolResults"][0]["tool"], "fake");
        assert_eq!(body["toolResults"][0]["status"], "OK");
        assert_eq!(
            body["analysisPackage"]["runtimeStatus"],
            "ready_for_claude_code"
        );
        assert_eq!(
            body["claudeMcpConfig"]["mcpServers"]["logagent"]["args"][0],
            "mcp"
        );
        assert_eq!(body["agentResponse"]["runtimeStatus"], "succeeded");
        assert_eq!(body["claudeSession"]["claudeSessionId"], "sess-test");
        assert_eq!(body["mcpCalls"][0]["name"], "logagent.search_logs");
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn task_message_resumes_waiting_for_user_task() {
        let (state, root) = test_state();
        create_test_session(&state, "sess_test").await;
        create_test_upload(&state, "upl_ask_user", UploadStatus::Complete).await;
        let app = http::router(state.clone()).with_state(state);
        let response = app
            .clone()
            .oneshot(
                Request::post("/api/tasks")
                    .header("authorization", "Bearer test-key")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"sessionId":"sess_test","uploadId":"upl_ask_user","question":"ASK_USER_MVP 请先追问"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let task_id = created["taskId"].as_str().unwrap();

        let waiting = wait_for_task_status(&app, task_id, "WAITING_FOR_USER").await;
        assert_eq!(waiting["phase"], "PLAN_ANALYSIS");
        let analysis = get_analysis_json(&app, task_id).await;
        let question_id = analysis["state"]["pendingUserPrompts"][0]["questionId"]
            .as_str()
            .unwrap()
            .to_string();

        let response = app
            .clone()
            .oneshot(
                Request::post(format!("/api/tasks/{task_id}/messages"))
                    .header("authorization", "Bearer test-key")
                    .header("content-type", "application/json")
                    .body(Body::from(format!(
                        r#"{{"questionId":"{question_id}","message":"异常发生在 10:00-10:30","idempotencyKey":"msg-test-1"}}"#
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let terminal = wait_for_task_status(&app, task_id, "SUCCEEDED").await;
        assert_eq!(terminal["status"], "SUCCEEDED");
        let analysis = get_analysis_json(&app, task_id).await;
        assert_eq!(
            analysis["state"]["userMessages"][0]["messageId"],
            "msg-test-1"
        );
        assert!(analysis["state"]["pendingUserPrompts"]
            .as_array()
            .unwrap()
            .is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn approval_decision_resumes_waiting_for_approval_task() {
        let (state, root) = test_state();
        create_test_session(&state, "sess_test").await;
        create_test_upload(&state, "upl_approval", UploadStatus::Complete).await;
        let app = http::router(state.clone()).with_state(state);
        let response = app
            .clone()
            .oneshot(
                Request::post("/api/tasks")
                    .header("authorization", "Bearer test-key")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"sessionId":"sess_test","uploadId":"upl_approval","question":"APPROVAL_MVP 请请求环境采集审批"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let task_id = created["taskId"].as_str().unwrap();

        let waiting = wait_for_task_status(&app, task_id, "WAITING_FOR_APPROVAL").await;
        assert_eq!(waiting["phase"], "PLAN_ANALYSIS");
        let analysis = get_analysis_json(&app, task_id).await;
        let action_id = analysis["state"]["pendingApprovals"][0]["actionId"]
            .as_str()
            .unwrap()
            .to_string();

        let response = app
            .clone()
            .oneshot(
                Request::post(format!(
                    "/api/tasks/{task_id}/actions/{action_id}/decision"
                ))
                .header("authorization", "Bearer test-key")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"decision":"approved","reason":"允许 mock 环境采集","idempotencyKey":"approval-test-1"}"#,
                ))
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let terminal = wait_for_task_status(&app, task_id, "SUCCEEDED").await;
        assert_eq!(terminal["status"], "SUCCEEDED");
        let analysis = get_analysis_json(&app, task_id).await;
        assert!(analysis["state"]["pendingApprovals"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(analysis["state"]["evidence"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["evidenceType"] == "environment_evidence"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn llm_failure_marks_plan_analysis_phase() {
        let (state, root) = test_state_with_adapter(
            LlmSettings {
                provider: LlmProvider::OpenAiCompatible,
                base_url: Some("not a valid URL".to_string()),
                api_key: Some("test-key".to_string()),
                binary_path: None,
                binary_max_output_bytes: 1024 * 1024,
                model: "test-model".to_string(),
                request_timeout_seconds: 1,
                max_input_chars: 60_000,
                max_output_tokens: 100,
            },
            r#"#!/usr/bin/env bash
echo adapter failed >&2
exit 17
"#,
        );
        create_test_session(&state, "sess_test").await;
        create_test_upload(&state, "upl_failure", UploadStatus::Complete).await;
        let app = http::router(state.clone()).with_state(state);
        let response = app
            .clone()
            .oneshot(
                Request::post("/api/tasks")
                    .header("authorization", "Bearer test-key")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"sessionId":"sess_test","uploadId":"upl_failure"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let task_id = created["taskId"].as_str().unwrap();
        let mut terminal = None;
        for _ in 0..100 {
            let response = app
                .clone()
                .oneshot(
                    Request::get(format!("/api/tasks/{task_id}"))
                        .header("authorization", "Bearer test-key")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
            if body["status"] == "FAILED" {
                terminal = Some(body);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let terminal = terminal.expect("task did not fail");
        assert_eq!(terminal["phase"], "PLAN_ANALYSIS");
        assert_eq!(terminal["error"]["phase"], "PLAN_ANALYSIS");
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn task_api_rejects_incomplete_uploads() {
        let (state, root) = test_state();
        create_test_session(&state, "sess_test").await;
        create_test_upload(&state, "upl_incomplete", UploadStatus::Uploading).await;
        let app = http::router(state.clone()).with_state(state);

        let response = app
            .oneshot(
                Request::post("/api/tasks")
                    .header("authorization", "Bearer test-key")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"sessionId":"sess_test","uploadId":"upl_incomplete"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert!(String::from_utf8_lossy(&body).contains("is not complete"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn task_api_persists_and_serves_metadata_context() {
        let (state, root) = test_state();
        create_test_session(&state, "sess_test").await;
        create_test_upload(&state, "upl_metadata", UploadStatus::Complete).await;
        let preview = state
            .metadata
            .create_import_preview(MetadataImportRequest {
                template_type: "yaml".to_string(),
                filename: None,
                instance_id: None,
                remark: None,
                content: r#"
instances:
  - instanceId: i-1
    clusterId: c-1
    nodeId: n-1
    product: opengemini
    version: 1.3.0
    environment: test
clusters:
  - clusterId: c-1
    product: opengemini
    nodes: [n-1]
nodes:
  - nodeId: n-1
    instanceId: i-1
    role: data
    status: active
"#
                .to_string(),
            })
            .await
            .unwrap();
        state
            .metadata
            .confirm_import(&preview.import_id)
            .await
            .unwrap();
        let app = http::router(state.clone()).with_state(state);
        let response = app
            .clone()
            .oneshot(
                Request::post("/api/tasks")
                    .header("authorization", "Bearer test-key")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"sessionId":"sess_test","uploadId":"upl_metadata","instanceId":"i-1"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let task_id = created["taskId"].as_str().unwrap();

        let mut terminal = None;
        for _ in 0..100 {
            let response = app
                .clone()
                .oneshot(
                    Request::get(format!("/api/tasks/{task_id}"))
                        .header("authorization", "Bearer test-key")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
            if body["status"] == "SUCCEEDED" {
                terminal = Some(body);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let terminal = terminal.expect("task did not succeed");
        assert_eq!(terminal["instanceId"], "i-1");
        assert_eq!(terminal["clusterId"], "c-1");
        assert_eq!(terminal["nodeId"], "n-1");

        let artifacts = app
            .oneshot(
                Request::get(format!("/api/tasks/{task_id}/artifacts"))
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(artifacts.status(), StatusCode::OK);
        let body = to_bytes(artifacts.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["metadataContext"]["product"], "opengemini");
        assert_eq!(body["metadataContext"]["version"], "1.3.0");
        assert_eq!(body["metadataContext"]["clusterNodes"][0]["nodeId"], "n-1");
        assert!(body["metadataContextPath"]
            .as_str()
            .unwrap()
            .ends_with("metadata_context.json"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn task_api_snapshots_explicit_and_auto_skills() {
        let skill_root = std::env::temp_dir().join(format!(
            "logagent-task-api-skills-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        write_test_skill(
            &skill_root,
            "auto-open",
            "auto-open",
            "Auto openGemini",
            true,
            30,
            Some("references/auto.md"),
        );
        write_test_skill(
            &skill_root,
            "explicit-only",
            "explicit-only",
            "Explicit only",
            false,
            10,
            None,
        );
        let (state, root) = test_state_with_adapter_and_skills(
            LlmSettings {
                provider: LlmProvider::Stub,
                base_url: None,
                api_key: None,
                binary_path: None,
                binary_max_output_bytes: 1024 * 1024,
                model: "stub".to_string(),
                request_timeout_seconds: 1,
                max_input_chars: 60_000,
                max_output_tokens: 100,
            },
            DEFAULT_MOCK_CLAUDE_ADAPTER,
            SkillSettings {
                enabled: true,
                roots: vec![skill_root.clone()],
                max_skill_chars: 4000,
                max_reference_chars: 20_000,
            },
        );
        create_test_session(&state, "sess_test").await;
        create_test_upload(&state, "upl_skills", UploadStatus::Complete).await;
        let preview = state
            .metadata
            .create_import_preview(MetadataImportRequest {
                template_type: "yaml".to_string(),
                filename: None,
                instance_id: None,
                remark: None,
                content: r#"
instances:
  - instanceId: i-1
    clusterId: c-1
    nodeId: n-1
    product: opengemini
    version: 1.3.0
    environment: test
clusters:
  - clusterId: c-1
    product: opengemini
    nodes: [n-1]
nodes:
  - nodeId: n-1
    instanceId: i-1
    role: data
    status: active
"#
                .to_string(),
            })
            .await
            .unwrap();
        state
            .metadata
            .confirm_import(&preview.import_id)
            .await
            .unwrap();
        let app = http::router(state.clone()).with_state(state.clone());
        let response = app
            .clone()
            .oneshot(
                Request::post("/api/tasks")
                    .header("authorization", "Bearer test-key")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"sessionId":"sess_test","uploadId":"upl_skills","instanceId":"i-1","skillIds":["explicit-only"]}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let task_id = created["taskId"].as_str().unwrap();
        let _terminal = wait_for_task_status(&app, task_id, "SUCCEEDED").await;

        let artifacts = app
            .oneshot(
                Request::get(format!("/api/tasks/{task_id}/artifacts"))
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(artifacts.status(), StatusCode::OK);
        let body = to_bytes(artifacts.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let system_context = &body["systemContext"];
        assert_eq!(system_context["schemaVersion"], 2);
        let resources = system_context["resources"].as_array().unwrap();
        let skill_ids = resources
            .iter()
            .filter(|resource| resource["kind"] == "diagnostic_skill")
            .filter_map(|resource| resource["skillId"].as_str())
            .collect::<Vec<_>>();
        assert_eq!(skill_ids, vec!["auto-open", "explicit-only"]);
        let metadata = resources
            .iter()
            .find(|resource| resource["kind"] == "metadata_instance")
            .expect("metadata adapter is snapshotted");
        assert_eq!(metadata["title"], "Metadata instance i-1");
        let auto_skill = resources
            .iter()
            .find(|resource| resource["skillId"] == "auto-open")
            .unwrap();
        assert_eq!(auto_skill["references"].as_array().unwrap().len(), 1);
        assert!(auto_skill["revision"].as_str().unwrap().len() >= 8);

        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(skill_root);
    }

    fn test_state() -> (Arc<AppState>, std::path::PathBuf) {
        test_state_with_llm(LlmSettings {
            provider: LlmProvider::Stub,
            base_url: None,
            api_key: None,
            binary_path: None,
            binary_max_output_bytes: 1024 * 1024,
            model: "stub".to_string(),
            request_timeout_seconds: 1,
            max_input_chars: 60_000,
            max_output_tokens: 100,
        })
    }

    fn test_state_with_llm(llm: LlmSettings) -> (Arc<AppState>, std::path::PathBuf) {
        test_state_with_adapter(llm, DEFAULT_MOCK_CLAUDE_ADAPTER)
    }

    const DEFAULT_MOCK_CLAUDE_ADAPTER: &str = r#"#!/usr/bin/env bash
set -euo pipefail
package="analysis_package.json"
if grep -q 'ASK_USER_MVP' "$package" && ! grep -q 'msg-test-1' "$package"; then
  cat <<'JSON'
{"structured_output":{"runtimeStatus":"waiting_for_user","pendingPrompt":{"questionId":"q-time-window","question":"请补充异常时间窗口","reason":"need time window from user","answerFormat":"time range","required":true}},"session_id":"sess-claude-http"}
JSON
  exit 0
fi
if grep -q 'APPROVAL_MVP' "$package" && ! grep -q 'environment_evidence/' "$package"; then
  cat <<'JSON'
{"structured_output":{"runtimeStatus":"waiting_for_approval","pendingApproval":{"actionId":"act_collect_env_http","actionType":"collect_environment","reason":"need approved environment evidence","input":{"scope":"node","commands":["uptime"]},"evidenceRefs":[]}},"session_id":"sess-claude-http"}
JSON
  exit 0
fi
if grep -q '"matches": \[\]' "$package" || grep -q '"matches":\[\]' "$package"; then
  evidence='session_text_input.json#question'
else
  evidence='grep_results.json#matches/0'
fi
cat <<JSON
{"structured_output":{"runtimeStatus":"completed","finalAnswer":{"summary":"Why did the sample fail? mock summary","symptoms":["failure"],"likelyRootCauses":[{"cause":"current task evidence explains the failure","evidenceRefs":["$evidence"]}],"nextChecks":["check current evidence"],"fixSuggestions":["fix the reported issue"],"missingInformation":[],"confidence":"high"}},"usage":{"inputTokens":32,"outputTokens":18},"cost":{"usd":0.001},"session_id":"sess-claude-http"}
JSON
"#;

    fn test_state_with_adapter(
        llm: LlmSettings,
        adapter_content: &str,
    ) -> (Arc<AppState>, std::path::PathBuf) {
        test_state_with_adapter_and_skills(
            llm,
            adapter_content,
            SkillSettings {
                enabled: false,
                roots: Vec::new(),
                max_skill_chars: 4000,
                max_reference_chars: 20_000,
            },
        )
    }

    fn test_state_with_adapter_and_skills(
        llm: LlmSettings,
        adapter_content: &str,
        skills: SkillSettings,
    ) -> (Arc<AppState>, std::path::PathBuf) {
        static NEXT_TEST_ROOT: AtomicU64 = AtomicU64::new(1);
        let root = std::env::temp_dir().join(format!(
            "logagent-task-api-{}-{}",
            std::process::id(),
            NEXT_TEST_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        let adapter = root.join("mock_claude.sh");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&adapter, adapter_content).unwrap();
        let mut permissions = std::fs::metadata(&adapter).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&adapter, permissions).unwrap();
        let config = Arc::new(AppConfig {
            config_path: root.join("logagent-test.yaml"),
            server: ServerSettings {
                bind: "127.0.0.1:0".to_string(),
                public_base_url: "http://127.0.0.1:0".to_string(),
                max_concurrent_tasks: 2,
            },
            auth: AuthSettings {
                api_keys: vec!["test-key".to_string()],
            },
            storage: StorageSettings {
                data_dir: root.join("data"),
                max_upload_bytes: 1024 * 1024,
                max_chunk_bytes: 512 * 1024,
            },
            skills,
            log_analyzer: LogAnalyzerSettings {
                keywords: vec!["error".to_string()],
                max_matches: 20,
            },
            tools: ToolsSettings::default(),
            llm,
            claude_code: test_claude_code_settings(adapter),
            mcp: McpSettings::default(),
            analysis: test_analysis_settings(),
            embedding: test_embedding_settings(),
        });
        config.prepare_dirs().unwrap();
        (AppState::new(config).unwrap(), root)
    }

    fn write_test_skill(
        root: &std::path::Path,
        dir_name: &str,
        skill_id: &str,
        display_name: &str,
        include_by_default: bool,
        priority: i32,
        reference_path: Option<&str>,
    ) {
        let skill_dir = root.join(dir_name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            format!(
                "---\nname: {display_name}\ndescription: {display_name} diagnostics.\n---\nUse current task evidence first.\n"
            ),
        )
        .unwrap();
        let references = if let Some(reference_path) = reference_path {
            let reference_file = skill_dir.join(reference_path);
            std::fs::create_dir_all(reference_file.parent().unwrap()).unwrap();
            std::fs::write(&reference_file, "Reference content.").unwrap();
            serde_json::json!([
                {
                    "path": reference_path,
                    "title": "Reference",
                    "summary": "Reference summary"
                }
            ])
        } else {
            serde_json::json!([])
        };
        let manifest = serde_json::json!({
            "schemaVersion": 1,
            "skillId": skill_id,
            "displayName": display_name,
            "products": ["opengemini"],
            "taskKinds": ["log_analysis"],
            "includeByDefault": include_by_default,
            "priority": priority,
            "references": references
        });
        std::fs::write(
            skill_dir.join("logagent.json"),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();
    }

    fn test_analysis_settings() -> AnalysisSettings {
        AnalysisSettings {
            max_rounds: 4,
            max_llm_calls: 4,
            max_actions: 6,
            max_repeated_action_fingerprints: 1,
        }
    }

    fn test_embedding_settings() -> EmbeddingSettings {
        EmbeddingSettings {
            enabled: false,
            provider: "openai_compatible".to_string(),
            model: "text-embedding-3-small".to_string(),
            api_key_env: None,
            store: "sqlite".to_string(),
        }
    }

    fn test_claude_code_settings(command_path: std::path::PathBuf) -> ClaudeCodeSettings {
        ClaudeCodeSettings {
            command_path,
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
        }
    }

    async fn create_test_upload(state: &Arc<AppState>, upload_id: &str, status: UploadStatus) {
        let upload_dir = state.config.storage.upload_dir(upload_id);
        std::fs::create_dir_all(&upload_dir).unwrap();
        let path = upload_dir.join("sample.log");
        std::fs::write(&path, "ERROR sample\n").unwrap();
        let now = Utc::now();
        state
            .uploads
            .create(UploadRecord {
                schema_version: 1,
                upload_id: upload_id.to_string(),
                filename: "sample.log".to_string(),
                size: 13,
                expected_size: Some(13),
                status,
                path,
                created_at: now,
                updated_at: now,
            })
            .await
            .unwrap();
    }

    async fn create_test_session(state: &Arc<AppState>, session_id: &str) {
        let now = Utc::now();
        state
            .sessions
            .create(AnalysisSessionRecord {
                schema_version: 1,
                session_id: session_id.to_string(),
                title: "Test session".to_string(),
                question: default_task_question(),
                source_url: None,
                instance_id: None,
                node_id: None,
                system_context_ids: Vec::new(),
                skill_ids: Vec::new(),
                upload_ids: Vec::new(),
                task_ids: Vec::new(),
                active_task_id: None,
                status: AnalysisSessionStatus::Draft,
                created_at: now,
                updated_at: now,
            })
            .await
            .unwrap();
    }

    async fn wait_for_task_status(
        app: &axum::Router,
        task_id: &str,
        expected_status: &str,
    ) -> serde_json::Value {
        for _ in 0..150 {
            let response = app
                .clone()
                .oneshot(
                    Request::get(format!("/api/tasks/{task_id}"))
                        .header("authorization", "Bearer test-key")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
            if body["status"] == expected_status {
                return body;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("task {task_id} did not reach {expected_status}");
    }

    async fn get_analysis_json(app: &axum::Router, task_id: &str) -> serde_json::Value {
        let response = app
            .clone()
            .oneshot(
                Request::get(format!("/api/tasks/{task_id}/analysis"))
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }
}
