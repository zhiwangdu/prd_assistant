use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{
    app::AppState,
    domain::models::{TaskKind, TaskRecord, TaskResponse, TaskStatus},
    support::error::AppError,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunsQuery {
    pub kind: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunListResponse {
    pub runs: Vec<TaskResponse>,
}

pub async fn list_runs(
    State(state): State<Arc<AppState>>,
    Query(query): Query<RunsQuery>,
) -> Result<Json<RunListResponse>, AppError> {
    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let kind = match query
        .kind
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        None => None,
        Some("tool_run") => Some(TaskKind::ToolRun),
        Some("remote_command_run") => Some(TaskKind::RemoteCommandRun),
        Some(other) => {
            return Err(AppError::bad_request(format!("unknown run kind {other}")));
        }
    };
    let runs = state
        .tasks
        .list()
        .await
        .into_iter()
        .filter(|task| match kind {
            Some(kind) => task.task_kind == kind,
            None => true,
        })
        .take(limit)
        .map(|task| task.summary(&state.config.server.public_base_url))
        .collect();
    Ok(Json(RunListResponse { runs }))
}

pub async fn get_run(
    State(state): State<Arc<AppState>>,
    Path(run_id): Path<String>,
) -> Result<Json<TaskRecord>, AppError> {
    let task = load_run(&state, &run_id).await?;
    Ok(Json(task))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunResultResponse {
    pub run_id: String,
    pub task_kind: TaskKind,
    pub result_path: String,
    pub result: serde_json::Value,
}

pub async fn run_result(
    State(state): State<Arc<AppState>>,
    Path(run_id): Path<String>,
) -> Result<Json<RunResultResponse>, AppError> {
    let task = load_run(&state, &run_id).await?;
    if task.status != TaskStatus::Succeeded {
        return Err(AppError::conflict(
            "run result is only available after success",
            serde_json::json!({ "status": task.status }),
        ));
    }
    let result_path = result_path_for(&task)
        .ok_or_else(|| AppError::internal("successful run is missing a result artifact path"))?;
    let result = read_json_file(std::path::Path::new(&result_path)).await?;
    Ok(Json(RunResultResponse {
        run_id: task.task_id.clone(),
        task_kind: task.task_kind,
        result_path,
        result,
    }))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunArtifactEntry {
    pub name: &'static str,
    pub logical_path: String,
    pub bytes: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunArtifactsResponse {
    pub run_id: String,
    pub artifacts: Vec<RunArtifactEntry>,
}

pub async fn run_artifacts(
    State(state): State<Arc<AppState>>,
    Path(run_id): Path<String>,
) -> Result<Json<RunArtifactsResponse>, AppError> {
    let task = load_run(&state, &run_id).await?;
    let workspace = state.config.storage.workspace_dir(&task.task_id);
    let candidates: Vec<(&'static str, Option<&str>)> = vec![
        ("manifest", task.manifest_path.as_deref()),
        ("grep_results", task.grep_results_path.as_deref()),
        ("tool_result", task.tool_result_path.as_deref()),
        ("remote_result", task.remote_result_path.as_deref()),
        ("result_json", task.result_json_path.as_deref()),
        ("result_markdown", task.result_markdown_path.as_deref()),
        ("metadata_context", task.metadata_context_path.as_deref()),
        ("system_context", task.system_context_path.as_deref()),
    ];
    let mut artifacts = Vec::new();
    for (name, path) in candidates {
        if let Some(path) = path {
            if let Ok(entry) = artifact_entry(name, &workspace, std::path::Path::new(path)) {
                artifacts.push(entry);
            }
        }
    }
    Ok(Json(RunArtifactsResponse {
        run_id: task.task_id,
        artifacts,
    }))
}

async fn load_run(state: &Arc<AppState>, run_id: &str) -> Result<TaskRecord, AppError> {
    validate_run_id(run_id)?;
    state
        .tasks
        .get(run_id)
        .await
        .ok_or_else(|| AppError::not_found(format!("unknown runId {run_id}")))
}

fn result_path_for(task: &TaskRecord) -> Option<String> {
    match task.task_kind {
        TaskKind::ToolRun => task.tool_result_path.clone(),
        TaskKind::RemoteCommandRun => task.remote_result_path.clone(),
    }
}

fn artifact_entry(
    name: &'static str,
    workspace: &std::path::Path,
    path: &std::path::Path,
) -> anyhow::Result<RunArtifactEntry> {
    let metadata = std::fs::metadata(path)?;
    if !metadata.is_file() {
        anyhow::bail!("artifact is not a regular file");
    }
    let logical_path = crate::support::fs_utils::relative_string(workspace, path)?;
    Ok(RunArtifactEntry {
        name,
        logical_path,
        bytes: metadata.len(),
    })
}

async fn read_json_file(path: &std::path::Path) -> Result<serde_json::Value, AppError> {
    let raw = tokio::fs::read_to_string(path)
        .await
        .map_err(|err| AppError::internal(format!("artifact not found: {err}")))?;
    serde_json::from_str(&raw)
        .map_err(|err| AppError::internal(format!("failed to parse artifact JSON: {err}")))
}

fn validate_run_id(run_id: &str) -> Result<(), AppError> {
    let valid = run_id.starts_with("task_")
        && run_id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'_' || value == b'-');
    if valid {
        Ok(())
    } else {
        Err(AppError::bad_request("invalid runId"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use chrono::Utc;
    use std::{collections::BTreeMap, sync::Arc};
    use tower::ServiceExt;

    use crate::{
        domain::models::{TaskKind, TaskRecord, TaskSource, TaskStatus},
        http,
        support::config::{
            AppConfig, AuthSettings, LogAnalyzerSettings, McpSettings, ServerSettings,
            StorageSettings, ToolsSettings,
        },
    };

    #[tokio::test]
    async fn runs_api_lists_gets_result_and_artifacts() {
        let (state, root) = test_state("runs-api");
        let task_id = "task_runs_api";
        let workspace = state.config.storage.workspace_dir(task_id);
        std::fs::create_dir_all(&workspace).unwrap();
        let result_path = workspace.join("result.json");
        std::fs::write(&result_path, r#"{"status":"OK","summary":"ok"}"#).unwrap();
        state
            .tasks
            .create(seed_record(task_id, &result_path))
            .await
            .unwrap();
        let app = http::router(state.clone()).with_state(state);

        let body = get_json(&app, "/api/runs").await;
        assert!(body["runs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|run| run["taskId"] == task_id));

        let filtered = get_json(&app, "/api/runs?kind=tool_run").await;
        assert!(filtered["runs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|run| run["taskId"] == task_id));
        let empty = get_json(&app, "/api/runs?kind=remote_command_run").await;
        assert!(empty["runs"].as_array().unwrap().is_empty());
        let bad_kind = get_status(&app, "/api/runs?kind=bogus").await;
        assert_eq!(bad_kind, StatusCode::BAD_REQUEST);

        let detail = get_json(&app, &format!("/api/runs/{task_id}")).await;
        assert_eq!(detail["taskKind"], "tool_run");

        let result = get_json(&app, &format!("/api/runs/{task_id}/result")).await;
        assert_eq!(result["result"]["status"], "OK");

        let artifacts = get_json(&app, &format!("/api/runs/{task_id}/artifacts")).await;
        assert!(artifacts["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["name"] == "tool_result" && entry["logicalPath"] == "result.json"));

        let download = app
            .clone()
            .oneshot(
                Request::get(format!("/api/artifacts/{task_id}/result.json"))
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(download.status(), StatusCode::OK);
        let bytes = to_bytes(download.into_body(), usize::MAX).await.unwrap();
        assert!(String::from_utf8_lossy(&bytes).contains("OK"));

        // Missing artifact -> 404; malformed id (no slash) -> 400.
        assert_eq!(
            get_status(&app, &format!("/api/artifacts/{task_id}/missing.json")).await,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            get_status(&app, &format!("/api/artifacts/{task_id}")).await,
            StatusCode::BAD_REQUEST
        );

        let _ = std::fs::remove_dir_all(root);
    }

    fn seed_record(task_id: &str, result_path: &std::path::Path) -> TaskRecord {
        let now = Utc::now();
        TaskRecord {
            schema_version: 6,
            task_id: task_id.to_string(),
            alias: None,
            session_id: None,
            task_kind: TaskKind::ToolRun,
            source: TaskSource::Upload,
            upload_ids: Vec::new(),
            inputs: Vec::new(),
            source_url: None,
            tool_id: Some("echo_checker".to_string()),
            tool_params: serde_json::json!({}),
            tool_result_path: Some(result_path.to_string_lossy().to_string()),
            remote_executor_id: None,
            remote_command_id: None,
            remote_command_params: serde_json::Value::Null,
            remote_result_path: None,
            instance_id: None,
            cluster_id: None,
            node_id: None,
            question: "test".to_string(),
            status: TaskStatus::Succeeded,
            phase: None,
            attempts: 1,
            error: None,
            manifest_path: None,
            grep_results_path: None,
            metadata_context_path: None,
            system_context_path: None,
            result_json_path: None,
            result_markdown_path: None,
            created_at: now,
            updated_at: now,
        }
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

    async fn get_status(app: &axum::Router, path: &str) -> StatusCode {
        app.clone()
            .oneshot(
                Request::get(path)
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
            .status()
    }

    fn test_state(prefix: &str) -> (Arc<AppState>, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "logagent-{prefix}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let config = Arc::new(AppConfig {
            server: ServerSettings {
                bind: "127.0.0.1:0".to_string(),
                public_base_url: "http://127.0.0.1:0".to_string(),
                max_concurrent_tasks: 2,
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
            log_analyzer: LogAnalyzerSettings {
                keywords: vec!["error".to_string()],
                max_matches: 20,
            },
            tools: ToolsSettings {
                tools: BTreeMap::new(),
            },
            remote_execution: crate::support::config::RemoteExecutionSettings::default(),
            mcp: McpSettings::default(),
            dev_selftest: crate::support::config::DevSelftestSettings::default(),
        });
        config.prepare_dirs().unwrap();
        (AppState::new(config).unwrap(), root)
    }
}
