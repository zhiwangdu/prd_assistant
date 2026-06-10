use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{
    app::AppState,
    domain::models::{AnalysisResult, TaskStatus},
    stores::case_store::{CaseRecord, CaseSearchHit, CaseUpdate, ManualCase, NewCase},
    support::{error::AppError, id::next_id},
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmCaseRequest {
    pub title: Option<String>,
    pub symptom: Option<String>,
    pub root_cause: Option<String>,
    pub solution: Option<String>,
    #[serde(default)]
    pub evidence_refs: Option<Vec<String>>,
    pub product: Option<String>,
    pub version: Option<String>,
    pub environment: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateManualCaseRequest {
    pub title: String,
    pub symptom: String,
    pub root_cause: String,
    pub solution: String,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    pub product: Option<String>,
    pub version: Option<String>,
    pub environment: Option<String>,
    pub instance_id: Option<String>,
    pub node_id: Option<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListCasesQuery {
    pub query: Option<String>,
    pub limit: Option<usize>,
    #[serde(default)]
    pub include_disabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCaseRequest {
    pub product: Option<String>,
    pub version: Option<String>,
    pub environment: Option<String>,
    pub instance_id: Option<String>,
    pub node_id: Option<String>,
    pub title: Option<String>,
    pub symptom: Option<String>,
    pub root_cause: Option<String>,
    pub solution: Option<String>,
    pub evidence_refs: Option<Vec<String>>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseResponse {
    pub case: CaseRecord,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseListResponse {
    pub cases: Vec<CaseSearchHit>,
}

pub async fn create_manual_case(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateManualCaseRequest>,
) -> Result<(StatusCode, Json<CaseResponse>), AppError> {
    let record = state
        .cases
        .create_manual(ManualCase {
            case_id: next_id("case"),
            product: clean_optional(req.product),
            version: clean_optional(req.version),
            environment: clean_optional(req.environment),
            instance_id: clean_optional(req.instance_id),
            node_id: clean_optional(req.node_id),
            title: req.title,
            symptom: req.symptom,
            root_cause: req.root_cause,
            solution: req.solution,
            evidence_refs: clean_refs(req.evidence_refs),
            enabled: req.enabled,
        })
        .await
        .map_err(|err| AppError::bad_request(format!("failed to save case: {err}")))?;
    Ok((StatusCode::CREATED, Json(CaseResponse { case: record })))
}

pub async fn confirm_task_case(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
    Json(req): Json<ConfirmCaseRequest>,
) -> Result<(StatusCode, Json<CaseResponse>), AppError> {
    validate_task_id(&task_id)?;
    let task = state
        .tasks
        .get(&task_id)
        .await
        .ok_or_else(|| AppError::not_found(format!("unknown taskId {task_id}")))?;
    if task.status != TaskStatus::Succeeded {
        return Err(AppError::conflict(
            "only successful tasks can be confirmed as cases",
            serde_json::json!({ "status": task.status }),
        ));
    }
    let result_json_path = task
        .result_json_path
        .clone()
        .ok_or_else(|| AppError::internal("successful task is missing resultJsonPath"))?;
    let result = read_result(&result_json_path).await?;
    let metadata = read_metadata_context(&state, &task_id).await?;
    let record = state
        .cases
        .create_or_get_for_task(NewCase {
            case_id: next_id("case"),
            task,
            result,
            source_result_path: result_json_path,
            product: clean_optional(req.product).or_else(|| metadata_field(&metadata, "product")),
            version: clean_optional(req.version).or_else(|| metadata_field(&metadata, "version")),
            environment: clean_optional(req.environment)
                .or_else(|| metadata_field(&metadata, "environment")),
            title: clean_optional(req.title),
            symptom: clean_optional(req.symptom),
            root_cause: clean_optional(req.root_cause),
            solution: clean_optional(req.solution),
            evidence_refs: req.evidence_refs.map(clean_refs),
        })
        .await
        .map_err(|err| AppError::bad_request(format!("failed to save case: {err}")))?;
    Ok((StatusCode::CREATED, Json(CaseResponse { case: record })))
}

pub async fn list_cases(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListCasesQuery>,
) -> Result<Json<CaseListResponse>, AppError> {
    let limit = query.limit.unwrap_or(5).clamp(1, 50);
    let query_text = query
        .query
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty());
    let cases = state
        .cases
        .search(query_text, limit, query.include_disabled)
        .await;
    Ok(Json(CaseListResponse { cases }))
}

pub async fn get_case(
    State(state): State<Arc<AppState>>,
    Path(case_id): Path<String>,
) -> Result<Json<CaseResponse>, AppError> {
    validate_case_id(&case_id)?;
    state
        .cases
        .get(&case_id)
        .await
        .map(|case| Json(CaseResponse { case }))
        .ok_or_else(|| AppError::not_found(format!("unknown caseId {case_id}")))
}

pub async fn update_case(
    State(state): State<Arc<AppState>>,
    Path(case_id): Path<String>,
    Json(req): Json<UpdateCaseRequest>,
) -> Result<Json<CaseResponse>, AppError> {
    validate_case_id(&case_id)?;
    let record = state
        .cases
        .update(
            &case_id,
            CaseUpdate {
                product: clean_optional(req.product),
                version: clean_optional(req.version),
                environment: clean_optional(req.environment),
                instance_id: clean_optional(req.instance_id),
                node_id: clean_optional(req.node_id),
                title: clean_optional(req.title),
                symptom: clean_optional(req.symptom),
                root_cause: clean_optional(req.root_cause),
                solution: clean_optional(req.solution),
                evidence_refs: req.evidence_refs.map(clean_refs),
                enabled: req.enabled,
            },
        )
        .await
        .map_err(|err| AppError::not_found(format!("failed to update case: {err}")))?;
    Ok(Json(CaseResponse { case: record }))
}

async fn read_result(path: &str) -> Result<AnalysisResult, AppError> {
    let raw = tokio::fs::read_to_string(path)
        .await
        .map_err(|err| AppError::internal(format!("result not found: {err}")))?;
    serde_json::from_str(&raw)
        .map_err(|err| AppError::internal(format!("failed to parse result JSON: {err}")))
}

async fn read_metadata_context(
    state: &AppState,
    task_id: &str,
) -> Result<Option<serde_json::Value>, AppError> {
    let path = state
        .config
        .storage
        .workspace_dir(task_id)
        .join("metadata_context.json");
    match tokio::fs::read_to_string(&path).await {
        Ok(raw) => serde_json::from_str(&raw).map(Some).map_err(|err| {
            AppError::internal(format!("failed to parse metadata context JSON: {err}"))
        }),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(AppError::internal(format!(
            "failed to read metadata context: {err}"
        ))),
    }
}

fn metadata_field(metadata: &Option<serde_json::Value>, key: &str) -> Option<String> {
    metadata
        .as_ref()
        .and_then(|value| value.get(key))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn clean_refs(refs: Vec<String>) -> Vec<String> {
    refs.into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .take(64)
        .collect()
}

fn default_enabled() -> bool {
    true
}

fn validate_task_id(task_id: &str) -> Result<(), AppError> {
    let valid = task_id.starts_with("task_")
        && task_id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'_' || value == b'-');
    if valid {
        Ok(())
    } else {
        Err(AppError::bad_request("invalid taskId"))
    }
}

fn validate_case_id(case_id: &str) -> Result<(), AppError> {
    let valid = case_id.starts_with("case_")
        && case_id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'_' || value == b'-');
    if valid {
        Ok(())
    } else {
        Err(AppError::bad_request("invalid caseId"))
    }
}
