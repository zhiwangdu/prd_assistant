use std::sync::Arc;

use axum::{
    extract::DefaultBodyLimit,
    middleware,
    routing::{get, post},
    Router,
};

use crate::{app::AppState, support::auth::require_api_key};

mod artifacts;
mod executors;
mod health;
mod runs;
mod tools;
mod uploads;

pub fn router(state: Arc<AppState>) -> Router<Arc<AppState>> {
    let max_body_bytes =
        usize::try_from(state.config.storage.max_upload_bytes).unwrap_or(usize::MAX);
    let protected = Router::new()
        .route("/api/uploads", post(uploads::upload))
        .route("/api/uploads/batch", post(uploads::batch_upload))
        .route("/api/uploads/init", post(uploads::init_upload))
        .route(
            "/api/uploads/:upload_id/chunks",
            post(uploads::upload_chunk),
        )
        .route(
            "/api/uploads/:upload_id/complete",
            post(uploads::complete_upload),
        )
        .route("/api/tools", get(tools::list_tools))
        .route("/api/tools/:tool_id", get(tools::get_tool))
        .route("/api/tools/:tool_id/runs", post(tools::create_tool_run))
        .route("/api/tools/runs", get(tools::list_tool_runs))
        .route("/api/tools/runs/:task_id", get(tools::get_tool_run))
        .route(
            "/api/tools/runs/:task_id/result",
            get(tools::tool_run_result),
        )
        .route(
            "/api/tools/runs/:task_id/artifacts",
            get(tools::tool_run_artifacts),
        )
        .route("/api/runs", get(runs::list_runs))
        .route("/api/runs/:run_id", get(runs::get_run))
        .route("/api/runs/:run_id/result", get(runs::run_result))
        .route("/api/runs/:run_id/artifacts", get(runs::run_artifacts))
        .route("/api/artifacts/*artifact_id", get(artifacts::get_artifact))
        .route(
            "/api/mcp",
            post(crate::mcp_server::http_mcp).get(crate::mcp_server::get_mcp),
        )
        .route(
            "/api/executors",
            get(executors::list_executors).post(executors::create_executor),
        )
        .route(
            "/api/executors/:executor_id",
            get(executors::get_executor)
                .patch(executors::patch_executor)
                .delete(executors::delete_executor),
        )
        .route(
            "/api/executor-command-templates",
            get(executors::list_command_templates),
        )
        .route(
            "/api/executor-runs",
            get(executors::list_remote_runs).post(executors::create_remote_run),
        )
        .route(
            "/api/executor-runs/:task_id",
            get(executors::get_remote_run),
        )
        .route(
            "/api/executor-runs/:task_id/result",
            get(executors::remote_run_result),
        )
        .layer(DefaultBodyLimit::max(max_body_bytes))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_api_key,
        ));

    Router::new()
        .route("/health", get(health::health))
        .merge(protected)
}
