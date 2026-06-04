use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};

use crate::{
    error::AppError,
    id::next_id,
    models::{
        CreateTaskRequest, TaskArtifactsResponse, TaskContext, TaskResponse, TaskSource, TaskStatus,
    },
    pipeline::run_upload_pipeline,
    state::AppState,
};

pub async fn create_task(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateTaskRequest>,
) -> Result<Json<TaskResponse>, AppError> {
    let upload = state
        .uploads
        .get(&req.upload_id)
        .await
        .ok_or_else(|| AppError::bad_request("unknown uploadId"))?;

    let task_id = next_id("task");
    let workspace = state.config.storage.workspace_dir(&task_id);
    let ctx = TaskContext {
        task_id: task_id.clone(),
        source: TaskSource::Upload,
        source_url: req.source_url,
        workspace,
    };

    let output = run_upload_pipeline(state.config.clone(), upload, ctx.clone()).await?;
    let url = format!(
        "{}/tasks/{}",
        state.config.server.public_base_url.trim_end_matches('/'),
        task_id
    );

    Ok(Json(TaskResponse {
        task_id,
        url,
        status: TaskStatus::Done,
        manifest_path: output.manifest_path.display().to_string(),
        grep_results_path: output.grep_results_path.display().to_string(),
    }))
}

pub async fn task_artifacts(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> Result<Json<TaskArtifactsResponse>, AppError> {
    validate_task_id(&task_id)?;

    let workspace = state.config.storage.workspace_dir(&task_id);
    let manifest_path = workspace.join("manifest.json");
    let grep_results_path = workspace.join("grep_results.json");
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

async fn read_json_file(path: &std::path::Path) -> Result<serde_json::Value, AppError> {
    let raw = tokio::fs::read_to_string(path)
        .await
        .map_err(|err| AppError::bad_request(format!("artifact not found: {err}")))?;
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
