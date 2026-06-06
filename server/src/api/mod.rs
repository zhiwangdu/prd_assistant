use std::sync::Arc;

use axum::{
    extract::DefaultBodyLimit,
    middleware,
    routing::{get, post},
    Router,
};

use crate::{auth::require_api_key, state::AppState};

mod health;
mod metadata;
mod tasks;
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
        .route("/api/tasks", post(tasks::create_task))
        .route("/api/tasks/:task_id/artifacts", get(tasks::task_artifacts))
        .route(
            "/api/metadata/instances/:instance_id",
            get(metadata::get_instance),
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
