use std::sync::Arc;

use axum::{
    extract::State,
    http::{header, HeaderMap, Request},
    middleware::Next,
    response::Response,
};

use crate::{app::AppState, support::error::AppError};

pub async fn require_api_key(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, AppError> {
    let expected = &state.config.auth.api_keys;
    if expected.is_empty() {
        return Err(AppError::internal("server has no configured API keys"));
    }

    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or_else(|| AppError::unauthorized("missing bearer token"))?;

    if !expected.iter().any(|key| key == token) {
        return Err(AppError::unauthorized("invalid bearer token"));
    }

    Ok(next.run(request).await)
}
