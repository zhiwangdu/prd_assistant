use std::sync::Arc;

use axum::{
    body::{to_bytes, Body},
    extract::{FromRequest, Multipart, Path, Query, State},
    http::{header, Request, StatusCode},
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{
    app::AppState,
    stores::{
        case_import_store::{
            compute_missing_fields, default_assistant_question, normalize_import_draft,
            CaseImportDraft, CaseImportMessage, CaseImportMessageRole, CaseImportSession,
            CaseImportSourceType, CaseImportStatus,
        },
        case_store::{CaseRecord, CaseSearchHit, CaseUpdate, ManualCase},
    },
    support::{error::AppError, fs_utils::sanitize_filename, id::next_id},
};

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
pub struct CreateCaseImportRequest {
    pub text: String,
    pub filename: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseImportMessageRequest {
    pub message: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCaseImportDraftRequest {
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseImportResponse {
    pub draft: CaseImportSession,
}

struct CaseImportInput {
    source_type: CaseImportSourceType,
    filename: Option<String>,
    text: String,
}

pub async fn create_case_import(
    State(state): State<Arc<AppState>>,
    req: Request<Body>,
) -> Result<(StatusCode, Json<CaseImportResponse>), AppError> {
    let input = read_case_import_input(state.clone(), req).await?;
    let session = create_import_session(&state, input, Vec::new()).await?;
    info!(
        draft_id = %session.draft_id,
        status = ?session.status,
        ready_to_confirm = session.ready_to_confirm,
        "case import draft created"
    );
    Ok((
        StatusCode::CREATED,
        Json(CaseImportResponse { draft: session }),
    ))
}

pub async fn get_case_import(
    State(state): State<Arc<AppState>>,
    Path(draft_id): Path<String>,
) -> Result<Json<CaseImportResponse>, AppError> {
    validate_case_import_id(&draft_id)?;
    let session =
        state.case_imports.get(&draft_id).await.ok_or_else(|| {
            AppError::not_found(format!("unknown case import draftId {draft_id}"))
        })?;
    Ok(Json(CaseImportResponse { draft: session }))
}

pub async fn post_case_import_message(
    State(state): State<Arc<AppState>>,
    Path(draft_id): Path<String>,
    Json(req): Json<CaseImportMessageRequest>,
) -> Result<Json<CaseImportResponse>, AppError> {
    validate_case_import_id(&draft_id)?;
    let message = req.message.trim();
    if message.is_empty() {
        return Err(AppError::bad_request("message must not be empty"));
    }
    if message.chars().count() > state.config.server.max_input_chars {
        return Err(AppError::bad_request(format!(
            "message exceeds server.max_input_chars {}",
            state.config.server.max_input_chars
        )));
    }
    let mut session =
        state.case_imports.get(&draft_id).await.ok_or_else(|| {
            AppError::not_found(format!("unknown case import draftId {draft_id}"))
        })?;
    if session.status == CaseImportStatus::Saved {
        return Err(AppError::conflict(
            "case import draft is already saved",
            serde_json::json!({ "confirmedCaseId": session.confirmed_case_id }),
        ));
    }
    session.messages.push(CaseImportMessage {
        role: CaseImportMessageRole::User,
        content: message.to_string(),
        created_at: Utc::now(),
    });
    apply_case_import_extraction(&state, &mut session).await?;
    let session =
        state.case_imports.update(session).await.map_err(|err| {
            AppError::internal(format!("failed to update case import draft: {err}"))
        })?;
    info!(
        draft_id = %draft_id,
        status = ?session.status,
        ready_to_confirm = session.ready_to_confirm,
        "case import message recorded"
    );
    Ok(Json(CaseImportResponse { draft: session }))
}

pub async fn update_case_import_draft(
    State(state): State<Arc<AppState>>,
    Path(draft_id): Path<String>,
    Json(req): Json<UpdateCaseImportDraftRequest>,
) -> Result<Json<CaseImportResponse>, AppError> {
    validate_case_import_id(&draft_id)?;
    let mut session =
        state.case_imports.get(&draft_id).await.ok_or_else(|| {
            AppError::not_found(format!("unknown case import draftId {draft_id}"))
        })?;
    if session.status == CaseImportStatus::Saved {
        return Err(AppError::conflict(
            "case import draft is already saved",
            serde_json::json!({ "confirmedCaseId": session.confirmed_case_id }),
        ));
    }
    if let Some(product) = req.product {
        session.structured_case.product = Some(product);
    }
    if let Some(version) = req.version {
        session.structured_case.version = Some(version);
    }
    if let Some(environment) = req.environment {
        session.structured_case.environment = Some(environment);
    }
    if let Some(instance_id) = req.instance_id {
        session.structured_case.instance_id = Some(instance_id);
    }
    if let Some(node_id) = req.node_id {
        session.structured_case.node_id = Some(node_id);
    }
    if let Some(title) = req.title {
        session.structured_case.title = Some(title);
    }
    if let Some(symptom) = req.symptom {
        session.structured_case.symptom = Some(symptom);
    }
    if let Some(root_cause) = req.root_cause {
        session.structured_case.root_cause = Some(root_cause);
    }
    if let Some(solution) = req.solution {
        session.structured_case.solution = Some(solution);
    }
    if let Some(evidence_refs) = req.evidence_refs {
        session.structured_case.evidence_refs = clean_refs(evidence_refs);
    }
    normalize_import_draft(&mut session.structured_case);
    session.missing_fields = compute_missing_fields(&session.structured_case);
    session.ready_to_confirm = session.missing_fields.is_empty();
    session.status = if session.ready_to_confirm {
        CaseImportStatus::Ready
    } else {
        CaseImportStatus::NeedsInput
    };
    session.assistant_question = default_assistant_question(&session.missing_fields);
    session.updated_at = Utc::now();
    let session =
        state.case_imports.update(session).await.map_err(|err| {
            AppError::internal(format!("failed to update case import draft: {err}"))
        })?;
    info!(
        draft_id = %draft_id,
        status = ?session.status,
        ready_to_confirm = session.ready_to_confirm,
        "case import draft updated"
    );
    Ok(Json(CaseImportResponse { draft: session }))
}

pub async fn confirm_case_import(
    State(state): State<Arc<AppState>>,
    Path(draft_id): Path<String>,
) -> Result<(StatusCode, Json<CaseResponse>), AppError> {
    validate_case_import_id(&draft_id)?;
    let mut session =
        state.case_imports.get(&draft_id).await.ok_or_else(|| {
            AppError::not_found(format!("unknown case import draftId {draft_id}"))
        })?;
    if let Some(case_id) = session.confirmed_case_id.as_deref() {
        let record =
            state.cases.get(case_id).await.ok_or_else(|| {
                AppError::not_found(format!("unknown confirmed caseId {case_id}"))
            })?;
        info!(
            draft_id = %draft_id,
            case_id = %case_id,
            "case import confirmation reused existing case"
        );
        return Ok((StatusCode::OK, Json(CaseResponse { case: record })));
    }
    session.missing_fields = compute_missing_fields(&session.structured_case);
    if !session.missing_fields.is_empty() {
        return Err(AppError::conflict(
            "case import draft is missing required fields",
            serde_json::json!({ "missingFields": session.missing_fields }),
        ));
    }
    let draft = session.structured_case.clone();
    let record = state
        .cases
        .create_manual(ManualCase {
            case_id: next_id("case"),
            product: draft.product,
            version: draft.version,
            environment: draft.environment,
            instance_id: draft.instance_id,
            node_id: draft.node_id,
            title: required_case_field(draft.title, "title")?,
            symptom: required_case_field(draft.symptom, "symptom")?,
            root_cause: required_case_field(draft.root_cause, "rootCause")?,
            solution: required_case_field(draft.solution, "solution")?,
            evidence_refs: draft.evidence_refs,
            enabled: true,
        })
        .await
        .map_err(|err| AppError::bad_request(format!("failed to save case: {err}")))?;
    session.status = CaseImportStatus::Saved;
    session.ready_to_confirm = true;
    session.confirmed_case_id = Some(record.case_id.clone());
    session.updated_at = Utc::now();
    state
        .case_imports
        .update(session)
        .await
        .map_err(|err| AppError::internal(format!("failed to update case import draft: {err}")))?;
    info!(
        draft_id = %draft_id,
        case_id = %record.case_id,
        "case import confirmed"
    );
    Ok((StatusCode::CREATED, Json(CaseResponse { case: record })))
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
    info!(
        case_id = %record.case_id,
        enabled = record.enabled,
        "manual case created"
    );
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
    info!(
        case_id = %case_id,
        enabled = record.enabled,
        "case updated"
    );
    Ok(Json(CaseResponse { case: record }))
}

async fn read_case_import_input(
    state: Arc<AppState>,
    req: Request<Body>,
) -> Result<CaseImportInput, AppError> {
    let content_type = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    if content_type.starts_with("multipart/form-data") {
        read_multipart_case_import_input(state, req).await
    } else {
        read_json_case_import_input(&state, req).await
    }
}

async fn read_json_case_import_input(
    state: &AppState,
    req: Request<Body>,
) -> Result<CaseImportInput, AppError> {
    let bytes = to_bytes(req.into_body(), max_body_bytes(state))
        .await
        .map_err(|err| AppError::bad_request(format!("invalid case import JSON body: {err}")))?;
    let req: CreateCaseImportRequest = serde_json::from_slice(&bytes)
        .map_err(|err| AppError::bad_request(format!("invalid case import JSON: {err}")))?;
    Ok(CaseImportInput {
        source_type: CaseImportSourceType::Text,
        filename: sanitize_optional_filename(req.filename)?,
        text: validate_case_import_text(req.text, state.config.server.max_input_chars)?,
    })
}

async fn read_multipart_case_import_input(
    state: Arc<AppState>,
    req: Request<Body>,
) -> Result<CaseImportInput, AppError> {
    let mut multipart = Multipart::from_request(req, &state)
        .await
        .map_err(|err| AppError::bad_request(format!("invalid multipart request: {err}")))?;
    let mut text: Option<String> = None;
    let mut text_filename: Option<String> = None;
    let mut file_text: Option<String> = None;
    let mut file_name: Option<String> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| AppError::bad_request(format!("invalid multipart request: {err}")))?
    {
        let field_name = field.name().unwrap_or_default().to_string();
        if field_name == "filename" {
            let value = field
                .text()
                .await
                .map_err(|err| AppError::bad_request(format!("invalid filename field: {err}")))?;
            text_filename = sanitize_optional_filename(Some(value))?;
            continue;
        }
        if field_name == "text" {
            let value = field
                .text()
                .await
                .map_err(|err| AppError::bad_request(format!("invalid text field: {err}")))?;
            text = Some(value);
            continue;
        }
        if field_name != "file" {
            continue;
        }
        let fallback_name = field.file_name().unwrap_or("case.txt").to_string();
        let safe_name = sanitize_filename(&fallback_name)?;
        let content_type = field.content_type().map(ToOwned::to_owned);
        if !is_supported_case_text_file(&safe_name, content_type.as_deref()) {
            return Err(AppError::bad_request(
                "unsupported case import file type; use UTF-8 .txt/.md/.log/.json/.yaml/.yml/.csv or paste text",
            ));
        }
        let value = read_case_import_file_field(&state, field).await?;
        file_text = Some(value);
        file_name = Some(safe_name);
    }
    if let Some(value) = file_text {
        return Ok(CaseImportInput {
            source_type: CaseImportSourceType::File,
            filename: file_name,
            text: validate_case_import_text(value, state.config.server.max_input_chars)?,
        });
    }
    let value = text.ok_or_else(|| AppError::bad_request("missing text or file field"))?;
    Ok(CaseImportInput {
        source_type: CaseImportSourceType::Text,
        filename: text_filename,
        text: validate_case_import_text(value, state.config.server.max_input_chars)?,
    })
}

async fn read_case_import_file_field(
    state: &AppState,
    mut field: axum::extract::multipart::Field<'_>,
) -> Result<String, AppError> {
    let max_bytes = max_case_import_bytes(state);
    let mut bytes = Vec::new();
    while let Some(chunk) = field
        .chunk()
        .await
        .map_err(|err| AppError::bad_request(format!("failed to read case import file: {err}")))?
    {
        if bytes.len().saturating_add(chunk.len()) > max_bytes {
            return Err(AppError::bad_request(format!(
                "case import file exceeds {} bytes",
                max_bytes
            )));
        }
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes)
        .map_err(|_| AppError::bad_request("case import file must be UTF-8 text"))
}

async fn create_import_session(
    state: &AppState,
    input: CaseImportInput,
    messages: Vec<CaseImportMessage>,
) -> Result<CaseImportSession, AppError> {
    let now = Utc::now();
    let mut session = CaseImportSession {
        schema_version: 1,
        draft_id: next_id("caseimp"),
        source_type: input.source_type,
        filename: input.filename,
        source_text: input.text,
        structured_case: CaseImportDraft::default(),
        missing_fields: Vec::new(),
        assistant_question: None,
        ready_to_confirm: false,
        status: CaseImportStatus::NeedsInput,
        messages,
        confirmed_case_id: None,
        created_at: now,
        updated_at: now,
    };
    apply_case_import_extraction(state, &mut session).await?;
    state
        .case_imports
        .create(session)
        .await
        .map_err(|err| AppError::internal(format!("failed to persist case import draft: {err}")))
}

async fn apply_case_import_extraction(
    _state: &AppState,
    session: &mut CaseImportSession,
) -> Result<(), AppError> {
    // Case import is manual-first: the draft starts empty and the user fills
    // structured fields via update_case_import_draft. This recomputes the
    // derived session state (missing fields, assistant prompt, status) from the
    // current draft without an LLM.
    let mut draft = session.structured_case.clone();
    normalize_import_draft(&mut draft);
    let missing_fields = compute_missing_fields(&draft);
    let assistant_question = if missing_fields.is_empty() {
        None
    } else {
        default_assistant_question(&missing_fields)
    };
    session.structured_case = draft;
    session.missing_fields = missing_fields;
    session.assistant_question = assistant_question.clone();
    session.ready_to_confirm = session.missing_fields.is_empty();
    session.status = if session.ready_to_confirm {
        CaseImportStatus::Ready
    } else {
        CaseImportStatus::NeedsInput
    };
    session.updated_at = Utc::now();
    if let Some(question) = assistant_question {
        let duplicate_last = session
            .messages
            .last()
            .map(|message| {
                message.role == CaseImportMessageRole::Assistant && message.content == question
            })
            .unwrap_or(false);
        if !duplicate_last {
            session.messages.push(CaseImportMessage {
                role: CaseImportMessageRole::Assistant,
                content: question,
                created_at: Utc::now(),
            });
        }
    }
    Ok(())
}

fn validate_case_import_text(value: String, max_chars: usize) -> Result<String, AppError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(AppError::bad_request("case import text must not be empty"));
    }
    let chars = value.chars().count();
    if chars > max_chars {
        return Err(AppError::bad_request(format!(
            "case import text contains {chars} chars and exceeds server.max_input_chars {max_chars}"
        )));
    }
    Ok(value)
}

fn sanitize_optional_filename(value: Option<String>) -> Result<Option<String>, AppError> {
    match clean_optional(value) {
        Some(value) => sanitize_filename(&value).map(Some),
        None => Ok(None),
    }
}

fn is_supported_case_text_file(filename: &str, content_type: Option<&str>) -> bool {
    let extension = filename
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase());
    let extension_supported = matches!(
        extension.as_deref(),
        Some("txt" | "text" | "md" | "markdown" | "log" | "json" | "yaml" | "yml" | "csv")
    );
    let content_type_supported = content_type
        .map(|value| {
            let value = value.to_ascii_lowercase();
            value.starts_with("text/") || value.contains("json") || value.contains("yaml")
        })
        .unwrap_or(false);
    extension_supported || content_type_supported
}

fn max_body_bytes(state: &AppState) -> usize {
    usize::try_from(state.config.storage.max_upload_bytes).unwrap_or(usize::MAX)
}

fn max_case_import_bytes(state: &AppState) -> usize {
    let llm_bytes = state
        .config
        .server
        .max_input_chars
        .saturating_mul(4)
        .max(1024);
    llm_bytes.min(max_body_bytes(state))
}

fn required_case_field(value: Option<String>, field: &str) -> Result<String, AppError> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::bad_request(format!("{field} is required")))
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

fn validate_case_import_id(draft_id: &str) -> Result<(), AppError> {
    let valid = draft_id.starts_with("caseimp_")
        && draft_id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'_' || value == b'-');
    if valid {
        Ok(())
    } else {
        Err(AppError::bad_request("invalid draftId"))
    }
}
