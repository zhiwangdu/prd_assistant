use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};

use crate::{
    app::AppState,
    domain::models::TaskKind,
    http::system_context::metadata_context_bundle_item,
    services::skill_registry::{
        ResolveSkillsInput, SkillDetailResponse, SkillImportRequest, SkillListResponse,
        SkillPreviewRequest, SkillPreviewResponse,
    },
    stores::system_context_store::{render_system_context_prompt, system_context_bundle},
    support::error::AppError,
};

pub async fn list_skills(State(state): State<Arc<AppState>>) -> Json<SkillListResponse> {
    Json(SkillListResponse {
        skills: state.skills.list(),
    })
}

pub async fn get_skill(
    State(state): State<Arc<AppState>>,
    Path(skill_id): Path<String>,
) -> Result<Json<SkillDetailResponse>, AppError> {
    state
        .skills
        .get(skill_id.trim())
        .map(Json)
        .ok_or_else(|| AppError::not_found(format!("unknown skillId {}", skill_id.trim())))
}

pub async fn import_skill(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SkillImportRequest>,
) -> Result<(StatusCode, Json<SkillDetailResponse>), AppError> {
    let skill = state.skills.import_markdown(req)?;
    Ok((StatusCode::CREATED, Json(skill)))
}

pub async fn preview_skills(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SkillPreviewRequest>,
) -> Result<Json<SkillPreviewResponse>, AppError> {
    let metadata_context = if let Some(instance_id) = req
        .instance_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(
            state
                .metadata
                .resolve_task_context(Some(instance_id.to_string()), None, None)
                .await?,
        )
    } else {
        None
    };
    let product = metadata_context
        .as_ref()
        .and_then(|context| context.product.as_deref())
        .or(req.product.as_deref());
    let version = metadata_context
        .as_ref()
        .and_then(|context| context.version.as_deref())
        .or(req.version.as_deref());
    let environment = metadata_context
        .as_ref()
        .and_then(|context| context.environment.as_deref())
        .or(req.environment.as_deref());
    let explicit_skill_ids = normalize_skill_ids(req.skill_ids)?;
    let mut resources = state.skills.resolve_items(ResolveSkillsInput {
        explicit_skill_ids: &explicit_skill_ids,
        task_kind: TaskKind::LogAnalysis,
        product,
        version,
        environment,
    })?;
    if let Some(metadata_context) = metadata_context.as_ref() {
        if metadata_context.instance_id.is_some() {
            resources.push(metadata_context_bundle_item(metadata_context));
        }
    }
    let bundle = system_context_bundle(resources.clone());
    Ok(Json(SkillPreviewResponse {
        resources,
        prompt: render_system_context_prompt(&bundle),
    }))
}

pub fn normalize_skill_ids(skill_ids: Vec<String>) -> Result<Vec<String>, AppError> {
    let mut normalized = Vec::new();
    for skill_id in skill_ids {
        let skill_id = skill_id.trim().to_string();
        if skill_id.is_empty() {
            continue;
        }
        let valid = skill_id.len() <= 120
            && skill_id.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-' || byte == b'.'
            });
        if !valid {
            return Err(AppError::bad_request("invalid skillId"));
        }
        if !normalized.iter().any(|value| value == &skill_id) {
            normalized.push(skill_id);
        }
    }
    if normalized.len() > 32 {
        return Err(AppError::bad_request("too many skillIds"));
    }
    Ok(normalized)
}
