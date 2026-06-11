use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{
    app::AppState,
    services::{
        agent_backend::{AgentBackendDiagnosticResult, AgentBackendsSummary},
        domain_adapters::DomainAdapterSummary,
        llm_gateway::{LlmChatTestResult, LlmModelsTestResult, LlmSettingsSummary},
    },
    support::error::AppError,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmSettingsResponse {
    pub llm: LlmSettingsSummary,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentBackendsSettingsResponse {
    pub agent_backends: AgentBackendsSummary,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainAdaptersResponse {
    pub domain_adapters: Vec<DomainAdapterSummary>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmTestResponse<T> {
    pub ok: bool,
    pub result: Option<T>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmChatTestRequest {
    pub message: String,
}

pub async fn llm_settings(State(state): State<Arc<AppState>>) -> Json<LlmSettingsResponse> {
    Json(LlmSettingsResponse {
        llm: state.llm.settings_summary(),
    })
}

pub async fn agent_backends(
    State(state): State<Arc<AppState>>,
) -> Json<AgentBackendsSettingsResponse> {
    Json(AgentBackendsSettingsResponse {
        agent_backends: state.agent_backends.summary(),
    })
}

pub async fn agent_backend_test(
    State(state): State<Arc<AppState>>,
    Path(backend_id): Path<String>,
) -> Json<LlmTestResponse<AgentBackendDiagnosticResult>> {
    Json(match state.agent_backends.test_backend(&backend_id).await {
        Ok(result) => LlmTestResponse {
            ok: true,
            result: Some(result),
            error: None,
        },
        Err(error) => LlmTestResponse {
            ok: false,
            result: None,
            error: Some(format!("{error:#}")),
        },
    })
}

pub async fn domain_adapters(State(state): State<Arc<AppState>>) -> Json<DomainAdaptersResponse> {
    Json(DomainAdaptersResponse {
        domain_adapters: state.domain_adapters.summaries(),
    })
}

pub async fn llm_models(
    State(state): State<Arc<AppState>>,
) -> Json<LlmTestResponse<LlmModelsTestResult>> {
    Json(match state.llm.test_list_models().await {
        Ok(result) => LlmTestResponse {
            ok: true,
            result: Some(result),
            error: None,
        },
        Err(error) => LlmTestResponse {
            ok: false,
            result: None,
            error: Some(format!("{error:#}")),
        },
    })
}

pub async fn llm_chat(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LlmChatTestRequest>,
) -> Result<Json<LlmTestResponse<LlmChatTestResult>>, AppError> {
    let message = req.message.trim();
    if message.is_empty() {
        return Err(AppError::bad_request("message must not be empty"));
    }
    if message.chars().count() > state.config.llm.max_input_chars {
        return Err(AppError::bad_request(format!(
            "message exceeds llm.max_input_chars {}",
            state.config.llm.max_input_chars
        )));
    }
    Ok(Json(match state.llm.test_chat_message(message).await {
        Ok(result) => LlmTestResponse {
            ok: true,
            result: Some(result),
            error: None,
        },
        Err(error) => LlmTestResponse {
            ok: false,
            result: None,
            error: Some(format!("{error:#}")),
        },
    }))
}
