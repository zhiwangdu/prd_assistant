use std::sync::Arc;

use axum::{
    extract::DefaultBodyLimit,
    middleware,
    routing::{get, post},
    Router,
};

use crate::{auth::require_api_key, state::AppState};

mod health;
mod tasks;
mod uploads;

pub fn router(state: Arc<AppState>) -> Router<Arc<AppState>> {
    let max_body_bytes =
        usize::try_from(state.config.storage.max_upload_bytes).unwrap_or(usize::MAX);
    let protected = Router::new()
        .route("/api/uploads", post(uploads::upload))
        .route("/api/tasks", post(tasks::create_task))
        .layer(DefaultBodyLimit::max(max_body_bytes))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_api_key,
        ));

    Router::new()
        .route("/health", get(health::health))
        .merge(protected)
}
