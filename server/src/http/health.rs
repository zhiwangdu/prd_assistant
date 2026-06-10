use axum::Json;

use crate::domain::models::HealthResponse;

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}
