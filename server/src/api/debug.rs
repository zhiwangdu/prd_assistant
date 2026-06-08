use std::sync::Arc;

use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};

use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmDebugResponse {
    pub llm_output_logging: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateLlmDebugRequest {
    pub llm_output_logging: bool,
}

pub async fn get_llm_debug(State(state): State<Arc<AppState>>) -> Json<LlmDebugResponse> {
    Json(LlmDebugResponse {
        llm_output_logging: state.llm.debug_log_responses(),
    })
}

pub async fn update_llm_debug(
    State(state): State<Arc<AppState>>,
    Json(req): Json<UpdateLlmDebugRequest>,
) -> Json<LlmDebugResponse> {
    state.llm.set_debug_log_responses(req.llm_output_logging);
    Json(LlmDebugResponse {
        llm_output_logging: state.llm.debug_log_responses(),
    })
}
