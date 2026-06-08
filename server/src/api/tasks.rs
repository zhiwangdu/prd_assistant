use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use serde::Deserialize;

use crate::{
    analysis_state::{self, AnalysisSnapshotResponse},
    error::AppError,
    id::next_id,
    models::{
        default_task_question, AnalysisResult, CreateTaskRequest, TaskArtifactsResponse,
        TaskListResponse, TaskRecord, TaskResponse, TaskResultResponse, TaskSource, TaskStatus,
        UploadStatus,
    },
    pipeline::{prepare_raw_snapshot, write_case_context, write_metadata_context},
    state::AppState,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskMessageRequest {
    pub question_id: Option<String>,
    pub message: String,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionDecisionRequest {
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
    let upload_ids = task_upload_ids(&req)?;
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
            normalize_optional_id(req.instance_id),
            normalize_optional_id(req.cluster_id),
            normalize_optional_id(req.node_id),
        )
        .await?;
    let task_id = next_id("task");
    let workspace = state.config.storage.workspace_dir(&task_id);
    let inputs = prepare_raw_snapshot(&workspace, &uploads).await?;
    let metadata_context_path = write_metadata_context(&workspace, &metadata_context).await?;
    let question = normalize_question(req.question, state.config.llm.max_input_chars / 2)?;
    let recalled_cases = state.cases.search(Some(&question), 5, false).await;
    write_case_context(&workspace, &question, &recalled_cases).await?;
    let now = Utc::now();
    let record = TaskRecord {
        schema_version: 4,
        task_id: task_id.clone(),
        source: TaskSource::Upload,
        upload_ids,
        inputs,
        source_url: req.source_url,
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
    state.executor.enqueue(state.clone(), task_id);
    Ok((
        StatusCode::ACCEPTED,
        Json(record.summary(&state.config.server.public_base_url)),
    ))
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
    analysis_state::record_user_message(&workspace, message_id, question_id, message)
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
    state.executor.enqueue(state.clone(), task_id);
    Ok(Json(resumed.summary(&state.config.server.public_base_url)))
}

pub async fn post_action_decision(
    State(state): State<Arc<AppState>>,
    Path((task_id, action_id)): Path<(String, String)>,
    Json(req): Json<ActionDecisionRequest>,
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
    state.executor.enqueue(state.clone(), task_id);
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
    let tool_results = read_tool_results(&state.config.storage.workspace_dir(&task_id)).await?;

    Ok(Json(TaskArtifactsResponse {
        task_id,
        manifest_path: manifest_path.display().to_string(),
        grep_results_path: grep_results_path.display().to_string(),
        manifest,
        grep_results,
        metadata_context_path: metadata_context_path.map(|path| path.display().to_string()),
        metadata_context,
        case_context_path,
        case_context,
        tool_results,
    }))
}

fn task_upload_ids(req: &CreateTaskRequest) -> Result<Vec<String>, AppError> {
    let mut upload_ids = Vec::new();
    if let Some(upload_id) = req.upload_id.as_ref().filter(|value| !value.is_empty()) {
        upload_ids.push(upload_id.clone());
    }
    for upload_id in req.upload_ids.iter().filter(|value| !value.is_empty()) {
        if !upload_ids.iter().any(|value| value == upload_id) {
            upload_ids.push(upload_id.clone());
        }
    }
    if upload_ids.is_empty() {
        Err(AppError::bad_request("missing uploadId or uploadIds"))
    } else {
        Ok(upload_ids)
    }
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
    use std::sync::atomic::{AtomicU64, Ordering};
    use tower::ServiceExt;

    use crate::{
        api,
        config::{
            AnalysisSettings, AppConfig, AuthSettings, LlmProvider, LlmSettings,
            LogAnalyzerSettings, ServerSettings, StorageSettings, ToolsSettings,
        },
        metadata::MetadataImportRequest,
        models::{TaskInput, UploadRecord, UploadStatus},
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
        create_test_upload(&state, "upl_test", UploadStatus::Complete).await;
        let app = api::router(state.clone()).with_state(state.clone());
        let response = app
            .clone()
            .oneshot(
                Request::post("/api/tasks")
                    .header("authorization", "Bearer test-key")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"uploadId":"upl_test","question":"Why did the sample fail?"}"#,
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
                source: TaskSource::Upload,
                upload_ids: vec!["upl_test".to_string()],
                inputs: vec![],
                source_url: None,
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
                result_json_path: None,
                result_markdown_path: None,
                created_at: now,
                updated_at: now,
            })
            .await
            .unwrap();
        let app = api::router(state.clone()).with_state(state);

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
        create_test_upload(&state, "upl_case", UploadStatus::Complete).await;
        let app = api::router(state.clone()).with_state(state.clone());
        let response = app
            .clone()
            .oneshot(
                Request::post("/api/tasks")
                    .header("authorization", "Bearer test-key")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"uploadId":"upl_case","question":"slow query has no time filter"}"#,
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
                        r#"{"uploadId":"upl_case_recall","question":"time filter regression"}"#,
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
        let now = Utc::now();
        state
            .tasks
            .create(TaskRecord {
                schema_version: 4,
                task_id: task_id.to_string(),
                source: TaskSource::Upload,
                upload_ids: vec!["upl_1".to_string()],
                inputs: vec![TaskInput {
                    upload_id: "upl_1".to_string(),
                    filename: "sample.log".to_string(),
                    size: 1,
                    raw_path: "raw/upl_1/sample.log".to_string(),
                }],
                source_url: None,
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
                result_json_path: None,
                result_markdown_path: None,
                created_at: now,
                updated_at: now,
            })
            .await
            .unwrap();
        let app = api::router(state.clone()).with_state(state);

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
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn task_message_resumes_waiting_for_user_task() {
        let (state, root) = test_state();
        create_test_upload(&state, "upl_ask_user", UploadStatus::Complete).await;
        let app = api::router(state.clone()).with_state(state);
        let response = app
            .clone()
            .oneshot(
                Request::post("/api/tasks")
                    .header("authorization", "Bearer test-key")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"uploadId":"upl_ask_user","question":"ASK_USER_MVP 请先追问"}"#,
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
        create_test_upload(&state, "upl_approval", UploadStatus::Complete).await;
        let app = api::router(state.clone()).with_state(state);
        let response = app
            .clone()
            .oneshot(
                Request::post("/api/tasks")
                    .header("authorization", "Bearer test-key")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"uploadId":"upl_approval","question":"APPROVAL_MVP 请请求环境采集审批"}"#,
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
        let (state, root) = test_state_with_llm(LlmSettings {
            provider: LlmProvider::OpenAiCompatible,
            base_url: Some("not a valid URL".to_string()),
            api_key: Some("test-key".to_string()),
            model: "test-model".to_string(),
            request_timeout_seconds: 1,
            max_input_chars: 60_000,
            max_output_tokens: 100,
        });
        create_test_upload(&state, "upl_failure", UploadStatus::Complete).await;
        let app = api::router(state.clone()).with_state(state);
        let response = app
            .clone()
            .oneshot(
                Request::post("/api/tasks")
                    .header("authorization", "Bearer test-key")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"uploadId":"upl_failure"}"#))
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
        create_test_upload(&state, "upl_incomplete", UploadStatus::Uploading).await;
        let app = api::router(state.clone()).with_state(state);

        let response = app
            .oneshot(
                Request::post("/api/tasks")
                    .header("authorization", "Bearer test-key")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"uploadId":"upl_incomplete"}"#))
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
        create_test_upload(&state, "upl_metadata", UploadStatus::Complete).await;
        let preview = state
            .metadata
            .create_import_preview(MetadataImportRequest {
                template_type: "yaml".to_string(),
                filename: None,
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
        let app = api::router(state.clone()).with_state(state);
        let response = app
            .clone()
            .oneshot(
                Request::post("/api/tasks")
                    .header("authorization", "Bearer test-key")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"uploadId":"upl_metadata","instanceId":"i-1"}"#,
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

    fn test_state() -> (Arc<AppState>, std::path::PathBuf) {
        test_state_with_llm(LlmSettings {
            provider: LlmProvider::Stub,
            base_url: None,
            api_key: None,
            model: "stub".to_string(),
            request_timeout_seconds: 1,
            max_input_chars: 60_000,
            max_output_tokens: 100,
        })
    }

    fn test_state_with_llm(llm: LlmSettings) -> (Arc<AppState>, std::path::PathBuf) {
        static NEXT_TEST_ROOT: AtomicU64 = AtomicU64::new(1);
        let root = std::env::temp_dir().join(format!(
            "logagent-task-api-{}-{}",
            std::process::id(),
            NEXT_TEST_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        let config = Arc::new(AppConfig {
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
            log_analyzer: LogAnalyzerSettings {
                keywords: vec!["error".to_string()],
                max_matches: 20,
            },
            tools: ToolsSettings::default(),
            llm,
            analysis: test_analysis_settings(),
        });
        config.prepare_dirs().unwrap();
        (AppState::new(config).unwrap(), root)
    }

    fn test_analysis_settings() -> AnalysisSettings {
        AnalysisSettings {
            max_rounds: 4,
            max_llm_calls: 4,
            max_actions: 6,
            max_repeated_action_fingerprints: 1,
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
