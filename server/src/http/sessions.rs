use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use tracing::info;

use crate::{
    app::AppState,
    domain::models::{
        AnalysisSessionListResponse, AnalysisSessionRecord, AnalysisSessionStatus,
        AttachSessionUploadsRequest, CreateAnalysisSessionRequest, PatchAnalysisSessionRequest,
        SessionTimelineEvent, SessionTimelineResponse, TaskResponse, UploadStatus,
    },
    http::{
        skills::normalize_skill_ids,
        tasks::{create_log_analysis_task, CreateLogAnalysisTaskInput},
    },
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
        analysis_language: req.analysis_language.unwrap_or_default(),
        system_context_ids: normalize_context_ids(req.system_context_ids)?,
        skill_ids: normalize_skill_ids(req.skill_ids)?,
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
    info!(
        session_id = %record.session_id,
        question_chars = record.question.chars().count(),
        system_context_count = record.system_context_ids.len(),
        skill_count = record.skill_ids.len(),
        "analysis session created"
    );
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
                if let Some(language) = req.analysis_language {
                    session.analysis_language = language;
                }
                if let Some(system_context_ids) = req.system_context_ids {
                    session.system_context_ids = normalize_context_ids(system_context_ids)?;
                }
                if let Some(skill_ids) = req.skill_ids {
                    session.skill_ids = normalize_skill_ids(skill_ids)?;
                }
                if let Some(status) = status {
                    session.status = status;
                }
                Ok(())
            },
            "session_updated",
            "session draft updated".to_string(),
            serde_json::json!({ "fields": "title/question/sourceUrl/instanceId/nodeId/analysisLanguage/systemContextIds/skillIds/status" }),
        )
        .await
        .map_err(|err| AppError::internal(format!("failed to update session: {err}")))?;
    info!(
        session_id = %session_id,
        status = ?updated.status,
        upload_count = updated.upload_ids.len(),
        system_context_count = updated.system_context_ids.len(),
        skill_count = updated.skill_ids.len(),
        "analysis session updated"
    );
    Ok(Json(updated))
}

pub async fn delete_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<StatusCode, AppError> {
    validate_session_id_for_api(&session_id)?;
    let session = state
        .sessions
        .get(&session_id)
        .await
        .ok_or_else(|| AppError::not_found(format!("unknown sessionId {session_id}")))?;
    if session.status.is_running_like() {
        return Err(AppError::conflict(
            "cannot delete a running or waiting session",
            serde_json::json!({ "sessionId": session_id }),
        ));
    }
    for task_id in &session.task_ids {
        if let Some(task) = state.tasks.get(task_id).await {
            if !task.status.is_terminal() {
                return Err(AppError::conflict(
                    "cannot delete a session with an unfinished task",
                    serde_json::json!({ "sessionId": session_id, "taskId": task.task_id, "status": task.status }),
                ));
            }
        }
    }
    let deleted = state
        .sessions
        .delete(&session_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to delete session: {err}")))?;
    info!(
        session_id = %deleted.session_id,
        task_count = deleted.task_ids.len(),
        upload_count = deleted.upload_ids.len(),
        "analysis session deleted"
    );
    Ok(StatusCode::NO_CONTENT)
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
    info!(
        session_id = %session_id,
        upload_count = upload_ids.len(),
        total_upload_count = session.upload_ids.len(),
        "uploads attached to analysis session"
    );
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
    info!(
        session_id = %session_id,
        upload_id = %upload_id,
        total_upload_count = session.upload_ids.len(),
        "upload detached from analysis session"
    );
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
            analysis_mode: state.config.claude_code.default_mode,
            analysis_language: session.analysis_language,
            skill_ids: session.skill_ids.clone(),
        },
    )
    .await?;
    state
        .executor
        .enqueue(state.clone(), record.task_id.clone());
    info!(
        session_id = %session_id,
        task_id = %record.task_id,
        upload_count = record.upload_ids.len(),
        "analysis task created from session"
    );
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

pub fn normalize_context_ids(context_ids: Vec<String>) -> Result<Vec<String>, AppError> {
    let mut normalized = Vec::new();
    for context_id in context_ids {
        let context_id = context_id.trim().to_string();
        if context_id.is_empty() {
            continue;
        }
        crate::stores::system_context_store::validate_context_id_for_api(&context_id)?;
        if !normalized.iter().any(|value| value == &context_id) {
            normalized.push(context_id);
        }
    }
    if normalized.len() > 32 {
        return Err(AppError::bad_request("too many systemContextIds"));
    }
    Ok(normalized)
}

fn validate_session_id_for_api(session_id: &str) -> Result<(), AppError> {
    validate_session_id(session_id).map_err(|_| AppError::bad_request("invalid sessionId"))
}
