use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;

use crate::{
    error::AppError,
    id::next_id,
    models::{
        CreateTaskRequest, TaskArtifactsResponse, TaskListResponse, TaskRecord, TaskResponse,
        TaskSource, TaskStatus,
    },
    pipeline::prepare_raw_snapshot,
    state::AppState,
};

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
        uploads.push(upload);
    }

    let task_id = next_id("task");
    let workspace = state.config.storage.workspace_dir(&task_id);
    let inputs = prepare_raw_snapshot(&workspace, &uploads).await?;
    let now = Utc::now();
    let record = TaskRecord {
        schema_version: 1,
        task_id: task_id.clone(),
        source: TaskSource::Upload,
        upload_ids,
        inputs,
        source_url: req.source_url,
        status: TaskStatus::Queued,
        phase: None,
        attempts: 0,
        error: None,
        manifest_path: None,
        grep_results_path: None,
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

    Ok(Json(TaskArtifactsResponse {
        task_id,
        manifest_path: manifest_path.display().to_string(),
        grep_results_path: grep_results_path.display().to_string(),
        manifest,
        grep_results,
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

async fn read_json_file(path: &std::path::Path) -> Result<serde_json::Value, AppError> {
    let raw = tokio::fs::read_to_string(path)
        .await
        .map_err(|err| AppError::internal(format!("artifact not found: {err}")))?;
    serde_json::from_str(&raw)
        .map_err(|err| AppError::internal(format!("failed to parse artifact JSON: {err}")))
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    use crate::{
        api,
        config::{AppConfig, AuthSettings, LogAnalyzerSettings, ServerSettings, StorageSettings},
        models::UploadRecord,
    };

    #[tokio::test]
    async fn task_api_creates_lists_and_reads_details() {
        let (state, root) = test_state();
        let upload_path = root.join("sample.log");
        std::fs::write(&upload_path, "ERROR sample\n").unwrap();
        state
            .uploads
            .insert(UploadRecord {
                upload_id: "upl_test".to_string(),
                filename: "sample.log".to_string(),
                size: 13,
                path: upload_path,
            })
            .await;
        let app = api::router(state.clone()).with_state(state);
        let response = app
            .clone()
            .oneshot(
                Request::post("/api/tasks")
                    .header("authorization", "Bearer test-key")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"uploadId":"upl_test"}"#))
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

        let detail = app
            .oneshot(
                Request::get(format!("/api/tasks/{task_id}"))
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(detail.status(), StatusCode::OK);
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
                status: TaskStatus::Queued,
                phase: None,
                attempts: 0,
                error: None,
                manifest_path: None,
                grep_results_path: None,
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
        let _ = std::fs::remove_dir_all(root);
    }

    fn test_state() -> (Arc<AppState>, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "logagent-task-api-{}",
            Utc::now().timestamp_nanos_opt().unwrap()
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
        });
        config.prepare_dirs().unwrap();
        (AppState::new(config).unwrap(), root)
    }
}
