use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use serde_json::json;
use tracing::info;

use crate::{
    app::AppState,
    domain::models::{
        CreateRemoteCommandRunRequest, CreateRemoteExecutorRequest, PatchRemoteExecutorRequest,
        RemoteCommandRunListResponse, RemoteCommandRunResultResponse, RemoteCommandRunsQuery,
        RemoteCommandTemplateListResponse, RemoteExecutorListResponse, RemoteExecutorRecord,
        TaskKind, TaskRecord, TaskResponse, TaskSource, TaskStatus,
    },
    services::remote_execution,
    support::{error::AppError, id::next_id},
};

pub async fn list_executors(
    State(state): State<Arc<AppState>>,
) -> Result<Json<RemoteExecutorListResponse>, AppError> {
    Ok(Json(RemoteExecutorListResponse {
        executors: state.executors.list().await,
    }))
}

pub async fn create_executor(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateRemoteExecutorRequest>,
) -> Result<(StatusCode, Json<RemoteExecutorRecord>), AppError> {
    let now = Utc::now();
    let executor = RemoteExecutorRecord {
        schema_version: 1,
        executor_id: next_id("executor"),
        name: normalize_required(req.name, "name", 120)?,
        host: normalize_required(req.host, "host", 255)?,
        port: validate_port(req.port)?,
        user: normalize_required(req.user, "user", 64)?,
        tags: normalize_tags(req.tags)?,
        enabled: req.enabled,
        notes: normalize_optional(req.notes, 500)?,
        last_check: None,
        created_at: now,
        updated_at: now,
    };
    state
        .executors
        .create(executor.clone())
        .await
        .map_err(|err| AppError::internal(format!("failed to persist executor: {err}")))?;
    info!(
        executor_id = %executor.executor_id,
        host = %executor.host,
        user = %executor.user,
        "remote executor created"
    );
    Ok((StatusCode::CREATED, Json(executor)))
}

pub async fn get_executor(
    State(state): State<Arc<AppState>>,
    Path(executor_id): Path<String>,
) -> Result<Json<RemoteExecutorRecord>, AppError> {
    validate_executor_id(&executor_id)?;
    state
        .executors
        .get(&executor_id)
        .await
        .map(Json)
        .ok_or_else(|| AppError::not_found(format!("unknown executorId {executor_id}")))
}

pub async fn patch_executor(
    State(state): State<Arc<AppState>>,
    Path(executor_id): Path<String>,
    Json(req): Json<PatchRemoteExecutorRequest>,
) -> Result<Json<RemoteExecutorRecord>, AppError> {
    validate_executor_id(&executor_id)?;
    let updated = state
        .executors
        .update(&executor_id, |executor| {
            if let Some(name) = req.name {
                executor.name = normalize_required_anyhow(name, "name", 120)?;
            }
            if let Some(host) = req.host {
                executor.host = normalize_required_anyhow(host, "host", 255)?;
            }
            if let Some(port) = req.port {
                executor.port = validate_port_anyhow(port)?;
            }
            if let Some(user) = req.user {
                executor.user = normalize_required_anyhow(user, "user", 64)?;
            }
            if let Some(tags) = req.tags {
                executor.tags = normalize_tags_anyhow(tags)?;
            }
            if let Some(enabled) = req.enabled {
                executor.enabled = enabled;
            }
            if let Some(notes) = req.notes {
                executor.notes = normalize_optional_anyhow(notes, 500)?;
            }
            Ok(())
        })
        .await
        .map_err(|err| AppError::bad_request(format!("failed to update executor: {err}")))?;
    Ok(Json(updated))
}

pub async fn delete_executor(
    State(state): State<Arc<AppState>>,
    Path(executor_id): Path<String>,
) -> Result<Json<RemoteExecutorRecord>, AppError> {
    validate_executor_id(&executor_id)?;
    let updated = state
        .executors
        .disable(&executor_id)
        .await
        .map_err(|err| AppError::bad_request(format!("failed to disable executor: {err}")))?;
    Ok(Json(updated))
}

pub async fn list_command_templates(
    State(state): State<Arc<AppState>>,
) -> Result<Json<RemoteCommandTemplateListResponse>, AppError> {
    Ok(Json(RemoteCommandTemplateListResponse {
        commands: remote_execution::command_templates(&state.config),
    }))
}

pub async fn create_remote_run(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateRemoteCommandRunRequest>,
) -> Result<(StatusCode, Json<TaskResponse>), AppError> {
    if !state.config.remote_execution.enabled {
        return Err(AppError::bad_request("remote execution is disabled"));
    }
    let executor_id = normalize_required(req.executor_id, "executorId", 120)?;
    validate_executor_id(&executor_id)?;
    let command_id = normalize_command_id(req.command_id)?;
    let idempotency_key = req
        .idempotency_key
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if let Some(existing) = existing_remote_run_for_key(&state, idempotency_key.as_deref()).await {
        return Ok((
            StatusCode::ACCEPTED,
            Json(existing.summary(&state.config.server.public_base_url)),
        ));
    }
    let executor = state
        .executors
        .get(&executor_id)
        .await
        .ok_or_else(|| AppError::bad_request(format!("unknown executorId {executor_id}")))?;
    if !executor.enabled {
        return Err(AppError::bad_request(format!(
            "executor {executor_id} is disabled"
        )));
    }
    let command = remote_execution::command_template(&state.config, &command_id)
        .ok_or_else(|| AppError::bad_request(format!("unknown commandId {command_id}")))?;
    if !command.enabled {
        return Err(AppError::bad_request(format!(
            "remote command {command_id} is disabled"
        )));
    }

    let task_id = next_id("task");
    let now = Utc::now();
    let record = TaskRecord {
        schema_version: 7,
        task_id: task_id.clone(),
        alias: Some(format!("{} on {}", command.display_name, executor.name)),
        session_id: None,
        task_kind: TaskKind::RemoteCommandRun,
        analysis_mode: state.config.claude_code.default_mode,
        analysis_language: crate::domain::models::AnalysisLanguage::ZhCn,
        source: TaskSource::RemoteExecutor,
        upload_ids: Vec::new(),
        inputs: Vec::new(),
        source_url: None,
        tool_id: None,
        tool_params: serde_json::Value::Null,
        tool_result_path: None,
        remote_executor_id: Some(executor.executor_id),
        remote_command_id: Some(command.command_id),
        remote_command_params: json!({ "idempotencyKey": idempotency_key }),
        remote_result_path: None,
        instance_id: None,
        cluster_id: None,
        node_id: None,
        question: "Run remote executor command".to_string(),
        status: TaskStatus::Queued,
        phase: None,
        attempts: 0,
        error: None,
        manifest_path: None,
        grep_results_path: None,
        metadata_context_path: None,
        system_context_path: None,
        result_json_path: None,
        result_markdown_path: None,
        created_at: now,
        updated_at: now,
    };
    state
        .tasks
        .create(record.clone())
        .await
        .map_err(|err| AppError::internal(format!("failed to persist remote run: {err}")))?;
    state.executor.enqueue(state.clone(), task_id);
    info!(
        task_id = %record.task_id,
        executor_id = ?record.remote_executor_id,
        command_id = ?record.remote_command_id,
        "remote command run task created"
    );
    Ok((
        StatusCode::ACCEPTED,
        Json(record.summary(&state.config.server.public_base_url)),
    ))
}

pub async fn list_remote_runs(
    State(state): State<Arc<AppState>>,
    Query(query): Query<RemoteCommandRunsQuery>,
) -> Result<Json<RemoteCommandRunListResponse>, AppError> {
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let executor_filter = query.executor_id.as_deref().map(str::trim);
    let runs = state
        .tasks
        .list()
        .await
        .into_iter()
        .filter(|task| task.task_kind == TaskKind::RemoteCommandRun)
        .filter(|task| match executor_filter {
            Some(value) if !value.is_empty() => task.remote_executor_id.as_deref() == Some(value),
            _ => true,
        })
        .take(limit)
        .map(|task| task.summary(&state.config.server.public_base_url))
        .collect();
    Ok(Json(RemoteCommandRunListResponse { runs }))
}

pub async fn get_remote_run(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> Result<Json<TaskRecord>, AppError> {
    validate_task_id(&task_id)?;
    let task = remote_run_task(&state, &task_id).await?;
    Ok(Json(task))
}

pub async fn remote_run_result(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> Result<Json<RemoteCommandRunResultResponse>, AppError> {
    validate_task_id(&task_id)?;
    let task = remote_run_task(&state, &task_id).await?;
    if task.status != TaskStatus::Succeeded {
        return Err(AppError::conflict(
            "remote command result is only available after success",
            json!({ "status": task.status }),
        ));
    }
    let result_path = task
        .remote_result_path
        .clone()
        .ok_or_else(|| AppError::internal("successful remote run is missing result path"))?;
    let result = read_json_file(std::path::Path::new(&result_path)).await?;
    Ok(Json(RemoteCommandRunResultResponse {
        task_id,
        executor_id: task
            .remote_executor_id
            .ok_or_else(|| AppError::internal("remote run is missing executorId"))?,
        command_id: task
            .remote_command_id
            .ok_or_else(|| AppError::internal("remote run is missing commandId"))?,
        result_path,
        result,
    }))
}

async fn existing_remote_run_for_key(
    state: &AppState,
    idempotency_key: Option<&str>,
) -> Option<TaskRecord> {
    let key = idempotency_key?;
    state.tasks.list().await.into_iter().find(|task| {
        task.task_kind == TaskKind::RemoteCommandRun
            && task
                .remote_command_params
                .get("idempotencyKey")
                .and_then(|value| value.as_str())
                == Some(key)
    })
}

async fn remote_run_task(state: &AppState, task_id: &str) -> Result<TaskRecord, AppError> {
    let task = state
        .tasks
        .get(task_id)
        .await
        .ok_or_else(|| AppError::not_found(format!("unknown taskId {task_id}")))?;
    if task.task_kind != TaskKind::RemoteCommandRun {
        return Err(AppError::bad_request(format!(
            "{task_id} is not a remote command run"
        )));
    }
    Ok(task)
}

async fn read_json_file(path: &std::path::Path) -> Result<serde_json::Value, AppError> {
    let raw = tokio::fs::read_to_string(path)
        .await
        .map_err(|err| AppError::internal(format!("artifact not found: {err}")))?;
    serde_json::from_str(&raw)
        .map_err(|err| AppError::internal(format!("failed to parse artifact JSON: {err}")))
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

fn validate_executor_id(executor_id: &str) -> Result<(), AppError> {
    let valid = executor_id.starts_with("executor_")
        && executor_id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'_' || value == b'-');
    if valid {
        Ok(())
    } else {
        Err(AppError::bad_request("invalid executorId"))
    }
}

fn normalize_command_id(command_id: String) -> Result<String, AppError> {
    let command_id = command_id.trim().to_string();
    let valid = !command_id.is_empty()
        && command_id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'_' || value == b'-');
    if valid {
        Ok(command_id)
    } else {
        Err(AppError::bad_request("invalid commandId"))
    }
}

fn normalize_required(value: String, field: &str, max_chars: usize) -> Result<String, AppError> {
    normalize_required_anyhow(value, field, max_chars)
        .map_err(|err| AppError::bad_request(err.to_string()))
}

fn normalize_required_anyhow(
    value: String,
    field: &str,
    max_chars: usize,
) -> anyhow::Result<String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        anyhow::bail!("{field} must not be empty");
    }
    if value.chars().count() > max_chars {
        anyhow::bail!("{field} exceeds maximum length of {max_chars}");
    }
    Ok(value)
}

fn normalize_optional(value: Option<String>, max_chars: usize) -> Result<Option<String>, AppError> {
    normalize_optional_anyhow(value, max_chars)
        .map_err(|err| AppError::bad_request(err.to_string()))
}

fn normalize_optional_anyhow(
    value: Option<String>,
    max_chars: usize,
) -> anyhow::Result<Option<String>> {
    match value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        Some(value) if value.chars().count() > max_chars => {
            anyhow::bail!("value exceeds maximum length of {max_chars}");
        }
        Some(value) => Ok(Some(value)),
        None => Ok(None),
    }
}

fn normalize_tags(tags: Vec<String>) -> Result<Vec<String>, AppError> {
    normalize_tags_anyhow(tags).map_err(|err| AppError::bad_request(err.to_string()))
}

fn normalize_tags_anyhow(tags: Vec<String>) -> anyhow::Result<Vec<String>> {
    let mut normalized = Vec::new();
    for tag in tags {
        let tag = tag.trim().to_string();
        if tag.is_empty() {
            continue;
        }
        if tag.chars().count() > 64 {
            anyhow::bail!("tag exceeds maximum length of 64");
        }
        if !normalized.iter().any(|existing| existing == &tag) {
            normalized.push(tag);
        }
    }
    if normalized.len() > 20 {
        anyhow::bail!("tags exceed maximum length of 20");
    }
    Ok(normalized)
}

fn validate_port(port: u16) -> Result<u16, AppError> {
    validate_port_anyhow(port).map_err(|err| AppError::bad_request(err.to_string()))
}

fn validate_port_anyhow(port: u16) -> anyhow::Result<u16> {
    if port == 0 {
        anyhow::bail!("port must be greater than zero");
    }
    Ok(port)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use std::{
        collections::BTreeMap,
        os::unix::fs::PermissionsExt,
        sync::{
            atomic::{AtomicU64, Ordering},
            Arc,
        },
    };
    use tower::ServiceExt;

    use crate::{
        http,
        support::config::{
            AnalysisSettings, AppConfig, AuthSettings, ClaudeCodeSettings, EmbeddingSettings,
            LlmProvider, LlmSettings, LogAnalyzerSettings, McpSettings,
            RemoteCommandTemplateSettings, RemoteExecutionSettings, ServerSettings, SkillSettings,
            StorageSettings, ToolsSettings,
        },
    };

    #[tokio::test]
    async fn executor_api_runs_configured_command_through_fake_ssh() {
        let (state, root) = test_state();
        let app = http::router(state.clone()).with_state(state.clone());

        let created = app
            .clone()
            .oneshot(
                Request::post("/api/executors")
                    .header("authorization", "Bearer test-key")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"name":"ecs-smoke","host":"112.74.50.120","port":22,"user":"root","tags":["smoke"],"enabled":true}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(created.status(), StatusCode::CREATED);
        let body = to_bytes(created.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let executor_id = body["executorId"].as_str().unwrap();

        let templates = app
            .clone()
            .oneshot(
                Request::get("/api/executor-command-templates")
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(templates.status(), StatusCode::OK);
        let body = to_bytes(templates.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["commands"][0]["commandId"], "smoke_ls_root");

        let run = app
            .clone()
            .oneshot(
                Request::post("/api/executor-runs")
                    .header("authorization", "Bearer test-key")
                    .header("content-type", "application/json")
                    .body(Body::from(format!(
                        r#"{{"executorId":"{executor_id}","commandId":"smoke_ls_root","idempotencyKey":"idem-1"}}"#
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(run.status(), StatusCode::ACCEPTED);
        let body = to_bytes(run.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["taskKind"], "remote_command_run");
        let task_id = body["taskId"].as_str().unwrap();

        wait_for_remote_run(&app, task_id, "SUCCEEDED").await;
        let result = app
            .oneshot(
                Request::get(format!("/api/executor-runs/{task_id}/result"))
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(result.status(), StatusCode::OK);
        let body = to_bytes(result.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["executorId"], executor_id);
        assert_eq!(body["commandId"], "smoke_ls_root");
        assert_eq!(body["result"]["status"], "OK");
        assert!(body["result"]["stdoutPreview"]
            .as_str()
            .unwrap()
            .contains("ls -la /root"));
        let _ = std::fs::remove_dir_all(root);
    }

    fn test_state() -> (Arc<AppState>, std::path::PathBuf) {
        static NEXT_TEST_ROOT: AtomicU64 = AtomicU64::new(1);
        let root = std::env::temp_dir().join(format!(
            "logagent-executors-api-{}-{}",
            std::process::id(),
            NEXT_TEST_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        let ssh_binary = write_fake_ssh(&root);
        let config = Arc::new(AppConfig {
            config_path: root.join("logagent-test.yaml"),
            server: ServerSettings {
                bind: "127.0.0.1:0".to_string(),
                public_base_url: "http://127.0.0.1:0".to_string(),
                max_concurrent_tasks: 2,
            },
            auth: AuthSettings {
                api_keys: vec!["test-key".to_string()],
            },
            storage: StorageSettings {
                data_dir: root.join("data"),
                max_upload_bytes: 1024 * 1024,
                max_chunk_bytes: 512 * 1024,
            },
            skills: SkillSettings {
                enabled: false,
                roots: Vec::new(),
                max_skill_chars: 4000,
                max_reference_chars: 20_000,
            },
            log_analyzer: LogAnalyzerSettings {
                keywords: vec!["error".to_string()],
                max_matches: 20,
            },
            tools: ToolsSettings::default(),
            fetch: crate::support::config::FetchSettings::default(),
            huawei_cloud: crate::support::config::HuaweiCloudSettings::default(),
            remote_execution: RemoteExecutionSettings {
                enabled: true,
                ssh_binary,
                host_key_policy: "accept-new".to_string(),
                connect_timeout_seconds: 2,
                command_timeout_seconds: 5,
                max_output_bytes: 1024 * 1024,
                commands: BTreeMap::from([(
                    "smoke_ls_root".to_string(),
                    RemoteCommandTemplateSettings {
                        command_id: "smoke_ls_root".to_string(),
                        display_name: "Smoke: list /root".to_string(),
                        description: "fake smoke".to_string(),
                        enabled: true,
                        argv: vec!["ls".to_string(), "-la".to_string(), "/root".to_string()],
                        timeout_seconds: Some(5),
                    },
                )]),
            },
            llm: LlmSettings {
                provider: LlmProvider::Stub,
                base_url: None,
                api_key: None,
                binary_path: None,
                binary_max_output_bytes: 1024 * 1024,
                model: "stub".to_string(),
                request_timeout_seconds: 1,
                max_input_chars: 60_000,
                max_output_tokens: 100,
            },
            claude_code: ClaudeCodeSettings::default(),
            mcp: McpSettings::default(),
            analysis: AnalysisSettings {
                max_rounds: 4,
                max_llm_calls: 4,
                max_actions: 6,
                max_repeated_action_fingerprints: 1,
            },
            embedding: EmbeddingSettings {
                enabled: false,
                provider: "openai_compatible".to_string(),
                model: "text-embedding-3-small".to_string(),
                api_key_env: None,
                store: "sqlite".to_string(),
            },
        });
        config.prepare_dirs().unwrap();
        (AppState::new(config).unwrap(), root)
    }

    fn write_fake_ssh(root: &std::path::Path) -> std::path::PathBuf {
        std::fs::create_dir_all(root).unwrap();
        let path = root.join("fake-ssh.sh");
        std::fs::write(
            &path,
            r#"#!/usr/bin/env bash
printf 'fake ssh args:'
for arg in "$@"; do
  printf ' %s' "$arg"
done
printf '\n'
printf 'fake stderr\n' >&2
"#,
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).unwrap();
        path
    }

    async fn wait_for_remote_run(app: &axum::Router, task_id: &str, expected_status: &str) {
        for _ in 0..100 {
            let response = app
                .clone()
                .oneshot(
                    Request::get(format!("/api/executor-runs/{task_id}"))
                        .header("authorization", "Bearer test-key")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
            if body["status"] == expected_status {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("remote run did not reach {expected_status}");
    }
}
