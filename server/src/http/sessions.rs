use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;

use crate::{
    app::AppState,
    domain::models::{
        AnalysisSessionListResponse, AnalysisSessionRecord, AnalysisSessionStatus,
        AttachSessionUploadsRequest, CreateAnalysisSessionRequest, PatchAnalysisSessionRequest,
        SessionTimelineEvent, SessionTimelineResponse, TaskResponse, UploadStatus,
    },
    http::tasks::{create_log_analysis_task, CreateLogAnalysisTaskInput},
    stores::{analysis_state, session_store::validate_session_id},
    support::{error::AppError, id::next_id},
};

pub async fn create_session(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateAnalysisSessionRequest>,
) -> Result<(StatusCode, Json<AnalysisSessionRecord>), AppError> {
    let session_id = next_id("sess");
    let question = normalize_question(req.question, state.config.llm.max_input_chars / 2)?;
    let title = normalize_title(req.title, &question, "New log analysis session")?;
    let now = Utc::now();
    let record = AnalysisSessionRecord {
        schema_version: 1,
        session_id: session_id.clone(),
        title,
        question,
        source_url: normalize_optional(req.source_url),
        instance_id: normalize_optional(req.instance_id),
        node_id: normalize_optional(req.node_id),
        upload_ids: Vec::new(),
        task_ids: Vec::new(),
        active_task_id: None,
        status: AnalysisSessionStatus::Draft,
        created_at: now,
        updated_at: now,
    };
    state
        .sessions
        .create(record.clone())
        .await
        .map_err(|err| AppError::internal(format!("failed to persist session: {err}")))?;
    Ok((StatusCode::CREATED, Json(record)))
}

pub async fn list_sessions(
    State(state): State<Arc<AppState>>,
) -> Result<Json<AnalysisSessionListResponse>, AppError> {
    let sessions = state
        .sessions
        .list()
        .await
        .into_iter()
        .map(|session| session.summary())
        .collect();
    Ok(Json(AnalysisSessionListResponse { sessions }))
}

pub async fn get_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<AnalysisSessionRecord>, AppError> {
    validate_session_id_for_api(&session_id)?;
    state
        .sessions
        .get(&session_id)
        .await
        .map(Json)
        .ok_or_else(|| AppError::not_found(format!("unknown sessionId {session_id}")))
}

pub async fn patch_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(req): Json<PatchAnalysisSessionRequest>,
) -> Result<Json<AnalysisSessionRecord>, AppError> {
    validate_session_id_for_api(&session_id)?;
    let question = match req.question {
        Some(value) => Some(normalize_question(
            Some(value),
            state.config.llm.max_input_chars / 2,
        )?),
        None => None,
    };
    let title = req.title.and_then(|value| {
        let value = value.trim().to_string();
        (!value.is_empty()).then_some(value)
    });
    let status = req.status;
    if let Some(status) = status {
        if !matches!(
            status,
            AnalysisSessionStatus::Draft | AnalysisSessionStatus::Ready
        ) {
            return Err(AppError::bad_request(
                "session status PATCH only accepts draft or ready",
            ));
        }
    }
    let updated = state
        .sessions
        .update(
            &session_id,
            |session| {
                if let Some(title) = title {
                    session.title = title;
                }
                if let Some(question) = question {
                    session.question = question;
                }
                if let Some(source_url) = req.source_url {
                    session.source_url = normalize_optional(source_url);
                }
                if let Some(instance_id) = req.instance_id {
                    session.instance_id = normalize_optional(instance_id);
                }
                if let Some(node_id) = req.node_id {
                    session.node_id = normalize_optional(node_id);
                }
                if let Some(status) = status {
                    session.status = status;
                }
                Ok(())
            },
            "session_updated",
            "session draft updated".to_string(),
            serde_json::json!({ "fields": "title/question/sourceUrl/instanceId/nodeId/status" }),
        )
        .await
        .map_err(|err| AppError::internal(format!("failed to update session: {err}")))?;
    Ok(Json(updated))
}

pub async fn attach_uploads(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(req): Json<AttachSessionUploadsRequest>,
) -> Result<Json<AnalysisSessionRecord>, AppError> {
    validate_session_id_for_api(&session_id)?;
    let upload_ids = normalize_upload_ids(req.upload_ids)?;
    for upload_id in &upload_ids {
        let upload = state
            .uploads
            .get(upload_id)
            .await
            .ok_or_else(|| AppError::bad_request(format!("unknown uploadId {upload_id}")))?;
        if upload.status != UploadStatus::Complete {
            return Err(AppError::bad_request(format!(
                "uploadId {upload_id} is not complete"
            )));
        }
    }
    let session = state
        .sessions
        .attach_uploads(&session_id, &upload_ids)
        .await
        .map_err(|err| AppError::internal(format!("failed to attach uploads: {err}")))?;
    Ok(Json(session))
}

pub async fn detach_upload(
    State(state): State<Arc<AppState>>,
    Path((session_id, upload_id)): Path<(String, String)>,
) -> Result<Json<AnalysisSessionRecord>, AppError> {
    validate_session_id_for_api(&session_id)?;
    let session = state
        .sessions
        .detach_upload(&session_id, &upload_id)
        .await
        .map_err(|err| {
            AppError::conflict(
                "failed to detach upload",
                serde_json::json!({ "errorDetail": err.to_string() }),
            )
        })?;
    Ok(Json(session))
}

pub async fn create_session_task(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<(StatusCode, Json<TaskResponse>), AppError> {
    validate_session_id_for_api(&session_id)?;
    let session = state
        .sessions
        .get(&session_id)
        .await
        .ok_or_else(|| AppError::not_found(format!("unknown sessionId {session_id}")))?;
    let record = create_log_analysis_task(
        state.clone(),
        CreateLogAnalysisTaskInput {
            session_id: session.session_id.clone(),
            upload_ids: session.upload_ids.clone(),
            source_url: session.source_url.clone(),
            question: Some(session.question.clone()),
            instance_id: session.instance_id.clone(),
            cluster_id: None,
            node_id: session.node_id.clone(),
        },
    )
    .await?;
    state
        .executor
        .enqueue(state.clone(), record.task_id.clone());
    Ok((
        StatusCode::ACCEPTED,
        Json(record.summary(&state.config.server.public_base_url)),
    ))
}

pub async fn session_timeline(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<SessionTimelineResponse>, AppError> {
    validate_session_id_for_api(&session_id)?;
    let session = state
        .sessions
        .get(&session_id)
        .await
        .ok_or_else(|| AppError::not_found(format!("unknown sessionId {session_id}")))?;
    let mut events = Vec::new();
    for event in state
        .sessions
        .read_events(&session_id)
        .map_err(|err| AppError::internal(format!("failed to read session events: {err}")))?
    {
        events.push(SessionTimelineEvent {
            source: "session".to_string(),
            event_type: event.event_type,
            session_id: event.session_id,
            task_id: event.task_id,
            phase: None,
            action_id: None,
            message: event.message,
            evidence_refs: Vec::new(),
            artifact_path: event.artifact_path,
            details: event.details,
            created_at: event.created_at,
        });
    }
    for task_id in &session.task_ids {
        let workspace = state.config.storage.workspace_dir(task_id);
        let Ok(snapshot) = analysis_state::read_snapshot(&workspace) else {
            continue;
        };
        for event in snapshot.events {
            let event_type = serde_json::to_value(event.event_type)
                .ok()
                .and_then(|value| value.as_str().map(ToString::to_string))
                .unwrap_or_else(|| "analysis_event".to_string());
            events.push(SessionTimelineEvent {
                source: "task".to_string(),
                event_type,
                session_id: session_id.clone(),
                task_id: Some(event.task_id),
                phase: event.phase,
                action_id: event.action_id,
                message: event.message,
                evidence_refs: event.evidence_refs,
                artifact_path: event.artifact_path,
                details: event.details,
                created_at: event.created_at,
            });
        }
    }
    events.sort_by(|left, right| left.created_at.cmp(&right.created_at));
    Ok(Json(SessionTimelineResponse { session_id, events }))
}

fn normalize_upload_ids(upload_ids: Vec<String>) -> Result<Vec<String>, AppError> {
    let mut normalized = Vec::new();
    for upload_id in upload_ids {
        let upload_id = upload_id.trim().to_string();
        if upload_id.is_empty() {
            continue;
        }
        if !upload_id.starts_with("upl_") {
            return Err(AppError::bad_request("invalid uploadId"));
        }
        if !normalized.iter().any(|value| value == &upload_id) {
            normalized.push(upload_id);
        }
    }
    if normalized.is_empty() {
        Err(AppError::bad_request("missing uploadIds"))
    } else {
        Ok(normalized)
    }
}

fn normalize_title(
    title: Option<String>,
    question: &str,
    fallback: &str,
) -> Result<String, AppError> {
    let title = title
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            let mut value = question.chars().take(80).collect::<String>();
            if value.trim().is_empty() {
                value = fallback.to_string();
            }
            value
        });
    if title.chars().count() > 160 {
        return Err(AppError::bad_request(
            "title exceeds maximum length of 160 characters",
        ));
    }
    Ok(title)
}

fn normalize_question(question: Option<String>, max_chars: usize) -> Result<String, AppError> {
    let question = question
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(crate::domain::models::default_task_question);
    if question.chars().count() > max_chars {
        return Err(AppError::bad_request(format!(
            "question exceeds maximum length of {max_chars} characters"
        )));
    }
    Ok(question)
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn validate_session_id_for_api(session_id: &str) -> Result<(), AppError> {
    validate_session_id(session_id).map_err(|_| AppError::bad_request("invalid sessionId"))
}
