use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use serde::Deserialize;
use tracing::info;

use crate::{
    app::AppState,
    domain::models::{
        TaskKind, TaskRecord, TaskResponse, TaskSource, TaskStatus, ToolRunListResponse,
    },
    pipeline::prepare_raw_snapshot,
    services::fetch::{
        endpoint_draft_from_curl, preview_curl, FetchEndpointView, FetchImportPreview,
        FetchRunParams, FETCH_TOOL_ID,
    },
    support::{error::AppError, id::next_id},
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchImportPreviewRequest {
    pub curl: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateFetchEndpointRequest {
    pub curl: String,
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchFetchEndpointRequest {
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<Option<String>>,
    pub tags: Option<Vec<String>>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateFetchRunRequest {
    #[serde(default)]
    pub variables: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub headers: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub body: Option<String>,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchRunsQuery {
    pub fetch_id: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchEndpointListResponse {
    pub endpoints: Vec<FetchEndpointView>,
}

pub async fn import_preview(
    State(state): State<Arc<AppState>>,
    Json(req): Json<FetchImportPreviewRequest>,
) -> Result<Json<FetchImportPreview>, AppError> {
    ensure_fetch_enabled(&state)?;
    preview_curl(&req.curl)
        .map(Json)
        .map_err(|err| AppError::bad_request(format!("{err:#}")))
}

pub async fn list_endpoints(
    State(state): State<Arc<AppState>>,
) -> Result<Json<FetchEndpointListResponse>, AppError> {
    ensure_fetch_enabled(&state)?;
    Ok(Json(FetchEndpointListResponse {
        endpoints: state.fetch.list().await,
    }))
}

pub async fn create_endpoint(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateFetchEndpointRequest>,
) -> Result<(StatusCode, Json<FetchEndpointView>), AppError> {
    ensure_fetch_enabled(&state)?;
    let fetch_id = next_id("fetch");
    let name = req
        .name
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fetch_id.clone());
    let tags = normalize_tags(req.tags)?;
    let draft = endpoint_draft_from_curl(
        &req.curl,
        fetch_id.clone(),
        name,
        req.description,
        tags,
        req.enabled,
    )
    .map_err(|err| AppError::bad_request(format!("{err:#}")))?;
    let endpoint = state
        .fetch
        .create(draft)
        .await
        .map_err(|err| AppError::internal(format!("failed to save fetch endpoint: {err:#}")))?;
    info!(fetch_id = %fetch_id, "fetch endpoint created");
    Ok((StatusCode::CREATED, Json(endpoint)))
}

pub async fn get_endpoint(
    State(state): State<Arc<AppState>>,
    Path(fetch_id): Path<String>,
) -> Result<Json<FetchEndpointView>, AppError> {
    ensure_fetch_enabled(&state)?;
    validate_fetch_id(&fetch_id)?;
    state
        .fetch
        .get_view(&fetch_id)
        .await
        .map(Json)
        .ok_or_else(|| AppError::not_found(format!("unknown fetchId {fetch_id}")))
}

pub async fn patch_endpoint(
    State(state): State<Arc<AppState>>,
    Path(fetch_id): Path<String>,
    Json(req): Json<PatchFetchEndpointRequest>,
) -> Result<Json<FetchEndpointView>, AppError> {
    ensure_fetch_enabled(&state)?;
    validate_fetch_id(&fetch_id)?;
    let tags = req.tags.map(normalize_tags).transpose()?;
    let endpoint = state
        .fetch
        .update_metadata(&fetch_id, req.name, req.description, tags, req.enabled)
        .await
        .map_err(|err| AppError::bad_request(format!("{err:#}")))?;
    Ok(Json(endpoint))
}

pub async fn delete_endpoint(
    State(state): State<Arc<AppState>>,
    Path(fetch_id): Path<String>,
) -> Result<StatusCode, AppError> {
    ensure_fetch_enabled(&state)?;
    validate_fetch_id(&fetch_id)?;
    if state
        .fetch
        .delete(&fetch_id)
        .await
        .map_err(|err| AppError::internal(format!("failed to delete fetch endpoint: {err:#}")))?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::not_found(format!("unknown fetchId {fetch_id}")))
    }
}

pub async fn create_run(
    State(state): State<Arc<AppState>>,
    Path(fetch_id): Path<String>,
    Json(req): Json<CreateFetchRunRequest>,
) -> Result<(StatusCode, Json<TaskResponse>), AppError> {
    ensure_fetch_enabled(&state)?;
    validate_fetch_id(&fetch_id)?;
    let endpoint = state
        .fetch
        .get_view(&fetch_id)
        .await
        .ok_or_else(|| AppError::not_found(format!("unknown fetchId {fetch_id}")))?;
    if !endpoint.enabled {
        return Err(AppError::bad_request(format!(
            "fetch endpoint {fetch_id} is disabled"
        )));
    }
    let _idempotency_key = req
        .idempotency_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let params = FetchRunParams {
        fetch_id: fetch_id.clone(),
        variables: req.variables,
        headers: req.headers,
        body: req.body,
    };
    let normalized_params = serde_json::to_value(params)
        .map_err(|err| AppError::internal(format!("failed to encode fetch params: {err}")))?;
    let task_id = next_id("task");
    let workspace = state.config.storage.workspace_dir(&task_id);
    let inputs = prepare_raw_snapshot(&workspace, &[]).await?;
    let now = Utc::now();
    let record = TaskRecord {
        schema_version: 6,
        task_id: task_id.clone(),
        alias: Some(format!("Fetch {}", endpoint.name)),
        session_id: None,
        task_kind: TaskKind::ToolRun,
        source: TaskSource::Upload,
        upload_ids: Vec::new(),
        inputs,
        source_url: None,
        tool_id: Some(FETCH_TOOL_ID.to_string()),
        tool_params: normalized_params,
        tool_result_path: None,
        remote_executor_id: None,
        remote_command_id: None,
        remote_command_params: serde_json::Value::Null,
        remote_result_path: None,
        instance_id: None,
        cluster_id: None,
        node_id: None,
        question: format!("Run fetch endpoint {fetch_id}"),
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
        .map_err(|err| AppError::internal(format!("failed to persist fetch run: {err}")))?;
    let _ = state.fetch.set_last_run(&fetch_id, task_id.clone()).await;
    state.executor.enqueue(state.clone(), task_id);
    Ok((
        StatusCode::ACCEPTED,
        Json(record.summary(&state.config.server.public_base_url)),
    ))
}

pub async fn list_runs(
    State(state): State<Arc<AppState>>,
    Query(query): Query<FetchRunsQuery>,
) -> Result<Json<ToolRunListResponse>, AppError> {
    ensure_fetch_enabled(&state)?;
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let runs = state
        .tasks
        .list()
        .await
        .into_iter()
        .filter(|task| task.task_kind == TaskKind::ToolRun)
        .filter(|task| task.tool_id.as_deref() == Some(FETCH_TOOL_ID))
        .filter(|task| match query.fetch_id.as_deref() {
            Some(fetch_id) if !fetch_id.trim().is_empty() => {
                task.tool_params
                    .get("fetchId")
                    .and_then(serde_json::Value::as_str)
                    == Some(fetch_id)
            }
            _ => true,
        })
        .take(limit)
        .map(|task| task.summary(&state.config.server.public_base_url))
        .collect();
    Ok(Json(ToolRunListResponse { runs }))
}

fn ensure_fetch_enabled(state: &AppState) -> Result<(), AppError> {
    if state.config.fetch.enabled {
        Ok(())
    } else {
        Err(AppError::bad_request("fetch is disabled by server config"))
    }
}

fn validate_fetch_id(fetch_id: &str) -> Result<(), AppError> {
    let valid = fetch_id.starts_with("fetch_")
        && fetch_id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'_' || value == b'-');
    if valid {
        Ok(())
    } else {
        Err(AppError::bad_request("invalid fetchId"))
    }
}

fn normalize_tags(tags: Vec<String>) -> Result<Vec<String>, AppError> {
    tags.into_iter()
        .map(|tag| {
            let tag = tag.trim().to_string();
            if tag.is_empty() {
                return Err(AppError::bad_request("tags must not be empty"));
            }
            if tag.len() > 64 {
                return Err(AppError::bad_request("tags must be at most 64 characters"));
            }
            Ok(tag)
        })
        .collect()
}

fn default_enabled() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
        routing::get,
        Router,
    };
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
    use std::{
        collections::BTreeMap,
        path::PathBuf,
        sync::{
            atomic::{AtomicU64, Ordering},
            Arc,
        },
    };
    use tower::ServiceExt;

    use crate::{
        http,
        support::config::{
            AppConfig, AuthSettings, FetchAllowedHost, FetchSettings, LogAnalyzerSettings,
            McpSettings, ServerSettings, SkillSettings, StorageSettings, ToolsSettings,
        },
    };

    #[tokio::test]
    async fn fetch_endpoint_crud_and_run_redacts_credentials() {
        let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream.local_addr().unwrap();
        tokio::spawn(async move {
            let app = Router::new().route(
                "/ok",
                get(|| async { axum::Json(serde_json::json!({ "ok": true })) }),
            );
            axum::serve(upstream, app).await.unwrap();
        });
        let (state, _root) = test_state(upstream_addr.port());
        let app = http::router(state.clone()).with_state(state);
        let curl = format!(
            "curl 'http://127.0.0.1:{}/ok?api_key=secret&limit=1' -H 'Authorization: Bearer secret-token'",
            upstream_addr.port()
        );

        let preview = post_json(
            &app,
            "/api/fetch/imports/preview",
            serde_json::json!({ "curl": curl }),
        )
        .await;
        assert_eq!(preview["endpoint"]["query"][0]["value"], "<redacted>");
        let preview_text = serde_json::to_string(&preview).unwrap();
        assert!(!preview_text.contains("secret-token"));
        assert!(!preview_text.contains("api_key=secret"));

        let endpoint = post_json(
            &app,
            "/api/fetch/endpoints",
            serde_json::json!({
                "curl": curl,
                "name": "Local OK",
                "tags": ["smoke"]
            }),
        )
        .await;
        let fetch_id = endpoint["fetchId"].as_str().unwrap();
        assert!(fetch_id.starts_with("fetch_"));

        let run = post_json(
            &app,
            &format!("/api/fetch/endpoints/{fetch_id}/runs"),
            serde_json::json!({}),
        )
        .await;
        let task_id = run["taskId"].as_str().unwrap();
        wait_for_run(&app, task_id).await;
        let result = get_json(&app, &format!("/api/tools/runs/{task_id}/result")).await;
        assert_eq!(result["toolId"], FETCH_TOOL_ID);
        assert_eq!(result["result"]["statusCode"], 200);
        assert_eq!(result["result"]["httpOk"], true);
        assert!(result["result"]["response"]["bodyArtifactPath"]
            .as_str()
            .unwrap()
            .contains("response_body.bin"));
        let result_text = serde_json::to_string(&result).unwrap();
        assert!(!result_text.contains("secret-token"));
        assert!(!result_text.contains("api_key=secret"));
        assert!(result_text.contains("#response"));
    }

    async fn post_json(
        app: &axum::Router,
        path: &str,
        payload: serde_json::Value,
    ) -> serde_json::Value {
        let response = app
            .clone()
            .oneshot(
                Request::post(path)
                    .header("authorization", "Bearer test-key")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(
            response.status().is_success(),
            "unexpected status {}",
            response.status()
        );
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    async fn get_json(app: &axum::Router, path: &str) -> serde_json::Value {
        let response = app
            .clone()
            .oneshot(
                Request::get(path)
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    async fn wait_for_run(app: &axum::Router, task_id: &str) {
        for _ in 0..60 {
            let run = get_json(app, &format!("/api/tools/runs/{task_id}")).await;
            match run["status"].as_str() {
                Some("SUCCEEDED") => return,
                Some("FAILED") => panic!("fetch run failed: {run}"),
                _ => tokio::time::sleep(std::time::Duration::from_millis(50)).await,
            }
        }
        panic!("fetch run did not finish");
    }

    fn test_state(port: u16) -> (Arc<AppState>, PathBuf) {
        let root = test_root("fetch-http");
        let config = Arc::new(AppConfig {
            server: ServerSettings {
                bind: "127.0.0.1:0".to_string(),
                public_base_url: "http://127.0.0.1:0".to_string(),
                max_concurrent_tasks: 1,
                max_input_chars: 60_000,
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
            tools: ToolsSettings {
                tools: BTreeMap::new(),
            },
            fetch: FetchSettings {
                enabled: true,
                secret_key_env: Some("LOGAGENT_TEST_FETCH_KEY".to_string()),
                secret_key: Some([3u8; 32]),
                allowed_hosts: vec![FetchAllowedHost {
                    scheme: Some("http".to_string()),
                    host: "127.0.0.1".to_string(),
                    port: Some(port),
                }],
                request_timeout_seconds: 5,
                max_request_bytes: 1024 * 1024,
                max_response_bytes: 1024 * 1024,
                max_redirects: 2,
            },
            huawei_cloud: crate::support::config::HuaweiCloudSettings::default(),
            remote_execution: crate::support::config::RemoteExecutionSettings::default(),
            mcp: McpSettings::default(),
            dev_selftest: crate::support::config::DevSelftestSettings::default(),
        });
        config.prepare_dirs().unwrap();
        (AppState::new(config).unwrap(), root)
    }

    fn test_root(prefix: &str) -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        std::env::temp_dir().join(format!(
            "{prefix}-{}-{}-{}",
            std::process::id(),
            BASE64.encode([4, 5, 6]).replace('=', ""),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }
}
