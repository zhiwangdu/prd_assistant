use std::sync::Arc;

use axum::{
    extract::DefaultBodyLimit,
    middleware,
    routing::{get, post},
    Router,
};

use crate::{app::AppState, support::auth::require_api_key};

mod artifacts;
mod cases;
mod executors;
mod exports;
mod fetch;
mod health;
mod mcp_readonly;
mod metadata;
mod runs;
mod skills;
mod system_context;
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
        .route("/api/fetch/imports/preview", post(fetch::import_preview))
        .route(
            "/api/fetch/endpoints",
            get(fetch::list_endpoints).post(fetch::create_endpoint),
        )
        .route(
            "/api/fetch/endpoints/:fetch_id",
            get(fetch::get_endpoint)
                .patch(fetch::patch_endpoint)
                .delete(fetch::delete_endpoint),
        )
        .route(
            "/api/fetch/endpoints/:fetch_id/runs",
            post(fetch::create_run),
        )
        .route("/api/fetch/runs", get(fetch::list_runs))
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
        .route(
            "/api/cases",
            post(cases::create_manual_case).get(cases::list_cases),
        )
        .route("/api/cases/imports", post(cases::create_case_import))
        .route(
            "/api/cases/imports/:draft_id",
            get(cases::get_case_import).patch(cases::update_case_import_draft),
        )
        .route(
            "/api/cases/imports/:draft_id/messages",
            post(cases::post_case_import_message),
        )
        .route(
            "/api/cases/imports/:draft_id/confirm",
            post(cases::confirm_case_import),
        )
        .route(
            "/api/cases/:case_id",
            get(cases::get_case).patch(cases::update_case),
        )
        .route("/api/mcp/readonly", post(mcp_readonly::readonly_mcp))
        .route("/api/exports/skills.zip", get(exports::skills_zip))
        .route("/api/exports/tools.zip", get(exports::tools_zip))
        .route("/api/skills", get(skills::list_skills))
        .route("/api/skills/imports", post(skills::import_skill))
        .route("/api/skills/preview", post(skills::preview_skills))
        .route("/api/skills/:skill_id", get(skills::get_skill))
        .route(
            "/api/system-context/resources",
            get(system_context::list_resources).post(system_context::create_resource),
        )
        .route(
            "/api/system-context/resources/:context_id",
            get(system_context::get_resource).patch(system_context::patch_resource),
        )
        .route(
            "/api/system-context/resources/:context_id/versions",
            post(system_context::create_version),
        )
        .route(
            "/api/system-context/resources/:context_id/versions/:version_id",
            axum::routing::patch(system_context::patch_version),
        )
        .route(
            "/api/system-context/resources/:context_id/versions/:version_id/activate",
            post(system_context::activate_version),
        )
        .route("/api/system-context/preview", post(system_context::preview))
        .route("/api/metadata/instances", get(metadata::list_instances))
        .route(
            "/api/metadata/instances/:instance_id",
            get(metadata::get_instance).delete(metadata::delete_instance),
        )
        .route(
            "/api/metadata/instances/:instance_id/snapshot",
            get(metadata::get_instance_snapshot),
        )
        .route(
            "/api/metadata/instances/:instance_id/refresh",
            post(metadata::refresh_instance_snapshot),
        )
        .route(
            "/api/metadata/clusters/:cluster_id",
            get(metadata::get_cluster),
        )
        .route(
            "/api/metadata/clusters/:cluster_id/nodes",
            get(metadata::list_cluster_nodes),
        )
        .route("/api/metadata/imports", post(metadata::create_import))
        .route("/api/metadata/imports/fetch", post(metadata::fetch_import))
        .route(
            "/api/metadata/snapshots/fetch",
            post(metadata::fetch_snapshot),
        )
        .route(
            "/api/metadata/imports/:import_id/preview",
            get(metadata::get_import_preview),
        )
        .route(
            "/api/metadata/imports/:import_id/confirm",
            post(metadata::confirm_import),
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
