use std::sync::Arc;

use axum::{extract::State, Json};

use crate::{
    error::AppError,
    id::next_id,
    models::{CreateTaskRequest, TaskContext, TaskResponse, TaskSource, TaskStatus},
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
