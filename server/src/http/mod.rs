use std::sync::Arc;

use axum::{
    extract::DefaultBodyLimit,
    middleware,
    routing::{delete, get, post},
    Router,
};

use crate::{app::AppState, support::auth::require_api_key};

mod cases;
mod debug;
mod health;
mod metadata;
mod sessions;
mod system_context;
mod tasks;
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
        .route(
            "/api/tasks",
            post(tasks::create_task).get(tasks::list_tasks),
        )
        .route(
            "/api/sessions",
            post(sessions::create_session).get(sessions::list_sessions),
        )
        .route(
            "/api/sessions/:session_id",
            get(sessions::get_session).patch(sessions::patch_session),
        )
        .route(
            "/api/sessions/:session_id/uploads",
            post(sessions::attach_uploads),
        )
        .route(
            "/api/sessions/:session_id/uploads/:upload_id",
            delete(sessions::detach_upload),
        )
        .route(
            "/api/sessions/:session_id/tasks",
            post(sessions::create_session_task),
        )
        .route(
            "/api/sessions/:session_id/timeline",
            get(sessions::session_timeline),
        )
        .route("/api/tasks/:task_id", get(tasks::get_task))
        .route("/api/tasks/:task_id/analysis", get(tasks::task_analysis))
        .route(
            "/api/tasks/:task_id/messages",
            post(tasks::post_task_message),
        )
        .route(
            "/api/tasks/:task_id/actions/:action_id/decision",
            post(tasks::post_action_decision),
        )
        .route("/api/tasks/:task_id/case", post(cases::confirm_task_case))
        .route("/api/tasks/:task_id/result", get(tasks::task_result))
        .route("/api/tasks/:task_id/artifacts", get(tasks::task_artifacts))
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
        .route(
            "/api/debug/llm",
            get(debug::get_llm_debug).put(debug::update_llm_debug),
        )
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
            get(metadata::get_instance),
        )
        .route(
            "/api/metadata/instances/:instance_id/snapshot",
            get(metadata::get_instance_snapshot),
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
