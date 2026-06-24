use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue},
    response::IntoResponse,
};

use crate::{
    app::AppState,
    support::{error::AppError, fs_utils::safe_join},
};

/// Download a run artifact by logical path: `GET /api/artifacts/<runId>/<relativePath>`.
///
/// The artifact id is `<runId>/<relativePath>`. The relative path is joined to the
/// run workspace with `safe_join`, which rejects traversal and absolute segments, so
/// only files inside the run workspace are reachable.
pub async fn get_artifact(
    State(state): State<Arc<AppState>>,
    Path(artifact_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let (run_id, relative) = split_artifact_id(&artifact_id)?;
    validate_run_id(&run_id)?;
    // Confirm the run exists so unknown run ids are 404 rather than leaking which
    // paths exist on disk.
    state
        .tasks
        .get(&run_id)
        .await
        .ok_or_else(|| AppError::not_found(format!("unknown runId {run_id}")))?;
    let workspace = state.config.storage.workspace_dir(&run_id);
    let resolved = safe_join(&workspace, std::path::Path::new(&relative))
        .map_err(|err| AppError::bad_request(format!("unsafe artifact path: {err}")))?;
    let bytes = tokio::fs::read(&resolved)
        .await
        .map_err(|err| AppError::not_found(format!("artifact not found: {err}")))?;
    let mut headers = HeaderMap::new();
    let content_type = content_type_for(&resolved);
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(content_type).unwrap(),
    );
    Ok((headers, bytes))
}

fn split_artifact_id(artifact_id: &str) -> Result<(String, String), AppError> {
    let (run_id, rest) = artifact_id
        .split_once('/')
        .ok_or_else(|| AppError::bad_request("artifact id must be <runId>/<relativePath>"))?;
    if run_id.is_empty() || rest.is_empty() {
        return Err(AppError::bad_request(
            "artifact id must be <runId>/<relativePath>",
        ));
    }
    Ok((run_id.to_string(), rest.to_string()))
}

fn content_type_for(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|value| value.to_str()) {
        Some("json") => "application/json",
        Some("md") => "text/markdown; charset=utf-8",
        Some("txt") | Some("log") => "text/plain; charset=utf-8",
        Some("html") | Some("svg") => "text/html; charset=utf-8",
        _ => "application/octet-stream",
    }
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
    async fn artifact_download_rejects_traversal_and_unknown_run() {
        let (state, root) = test_state("artifacts-api");
        let task_id = "task_artifacts_api";
        let workspace = state.config.storage.workspace_dir(task_id);
        std::fs::create_dir_all(&workspace).unwrap();
        let result_path = workspace.join("result.json");
        std::fs::write(&result_path, r#"{"status":"OK"}"#).unwrap();
        state
            .tasks
            .create(seed_record(task_id, &result_path))
            .await
            .unwrap();
        let app = http::router(state.clone()).with_state(state);

        // Valid download.
        let response = app
            .clone()
            .oneshot(
                Request::get(format!("/api/artifacts/{task_id}/result.json"))
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert!(String::from_utf8_lossy(&bytes).contains("OK"));

        // Traversal is rejected by safe_join (encoded %2F stays as '/', '..' rejected).
        let response = app
            .clone()
            .oneshot(
                Request::get(format!("/api/artifacts/{task_id}/..%2F..%2Fetc%2Fhostname"))
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(response.status(), StatusCode::OK);

        // Unknown run id -> 404.
        let response = app
            .clone()
            .oneshot(
                Request::get("/api/artifacts/task_does_not_exist/result.json")
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

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
