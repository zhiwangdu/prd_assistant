use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use tracing::info;

use crate::{
    app::AppState,
    domain::models::{
        CreateToolRunRequest, TaskKind, TaskRecord, TaskResponse, TaskStatus, ToolListResponse,
        ToolRunArtifactsResponse, ToolRunListResponse,
    },
    services::tools,
    support::error::AppError,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolRunsQuery {
    pub tool_id: Option<String>,
    pub limit: Option<usize>,
}

pub async fn list_tools(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ToolListResponse>, AppError> {
    Ok(Json(ToolListResponse {
        tools: tools::descriptors(&state.config),
    }))
}

pub async fn get_tool(
    State(state): State<Arc<AppState>>,
    Path(tool_id): Path<String>,
) -> Result<Json<crate::domain::models::ToolDescriptor>, AppError> {
    tools::get_descriptor(&state.config, &tool_id)
        .map(Json)
        .ok_or_else(|| AppError::not_found(format!("unknown toolId {tool_id}")))
}

pub async fn create_tool_run(
    State(state): State<Arc<AppState>>,
    Path(tool_id): Path<String>,
    Json(req): Json<CreateToolRunRequest>,
) -> Result<(StatusCode, Json<TaskResponse>), AppError> {
    let _idempotency_key = req
        .idempotency_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let upload_ids = req
        .upload_ids
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let record = tools::build_tool_run_task(&state, &tool_id, upload_ids, &req.params).await?;
    let task_id = record.task_id.clone();
    state
        .tasks
        .create(record.clone())
        .await
        .map_err(|err| AppError::internal(format!("failed to persist tool run: {err}")))?;
    state.executor.enqueue(state.clone(), task_id);
    info!(
        task_id = %record.task_id,
        tool_id = ?record.tool_id,
        upload_count = record.upload_ids.len(),
        "manual tool run task created"
    );
    Ok((
        StatusCode::ACCEPTED,
        Json(record.summary(&state.config.server.public_base_url)),
    ))
}

pub async fn list_tool_runs(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ToolRunsQuery>,
) -> Result<Json<ToolRunListResponse>, AppError> {
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let runs = state
        .tasks
        .list()
        .await
        .into_iter()
        .filter(|task| task.task_kind == TaskKind::ToolRun)
        .filter(|task| match query.tool_id.as_deref() {
            Some(tool_id) if !tool_id.trim().is_empty() => task.tool_id.as_deref() == Some(tool_id),
            _ => true,
        })
        .take(limit)
        .map(|task| task.summary(&state.config.server.public_base_url))
        .collect();
    Ok(Json(ToolRunListResponse { runs }))
}

pub async fn get_tool_run(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> Result<Json<TaskRecord>, AppError> {
    validate_task_id(&task_id)?;
    let task = state
        .tasks
        .get(&task_id)
        .await
        .ok_or_else(|| AppError::not_found(format!("unknown taskId {task_id}")))?;
    if task.task_kind != TaskKind::ToolRun {
        return Err(AppError::bad_request(format!(
            "{task_id} is not a tool run"
        )));
    }
    Ok(Json(task))
}

pub async fn tool_run_result(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> Result<Json<ToolRunArtifactsResponse>, AppError> {
    read_tool_run_result(state, task_id).await.map(Json)
}

pub async fn tool_run_artifacts(
    State(state): State<Arc<AppState>>,
    Path(task_id): Path<String>,
) -> Result<Json<ToolRunArtifactsResponse>, AppError> {
    read_tool_run_result(state, task_id).await.map(Json)
}

async fn read_tool_run_result(
    state: Arc<AppState>,
    task_id: String,
) -> Result<ToolRunArtifactsResponse, AppError> {
    validate_task_id(&task_id)?;
    let task = state
        .tasks
        .get(&task_id)
        .await
        .ok_or_else(|| AppError::not_found(format!("unknown taskId {task_id}")))?;
    if task.task_kind != TaskKind::ToolRun {
        return Err(AppError::bad_request(format!(
            "{task_id} is not a tool run"
        )));
    }
    if task.status != TaskStatus::Succeeded {
        return Err(AppError::conflict(
            "tool run result is only available after success",
            serde_json::json!({ "status": task.status }),
        ));
    }
    let result_path = task
        .tool_result_path
        .ok_or_else(|| AppError::internal("successful tool run is missing toolResultPath"))?;
    let result = read_json_file(std::path::Path::new(&result_path)).await?;
    Ok(ToolRunArtifactsResponse {
        task_id,
        tool_id: task
            .tool_id
            .ok_or_else(|| AppError::internal("tool run is missing toolId"))?,
        result_path,
        result,
    })
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

// These integration tests spawn fake `bash` tool wrappers and set Unix exec
// permissions, so they only compile/run on Unix. The non-test tool handlers are
// cross-platform; pure-logic tests live alongside the services they exercise.
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use chrono::Utc;
    use std::sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    };
    use tower::ServiceExt;

    use crate::{
        domain::models::{UploadRecord, UploadStatus},
        http,
        support::config::{
            AppConfig, AuthSettings, LogAnalyzerSettings, ServerSettings, StorageSettings,
            ToolMatchSettings, ToolSettings, ToolsSettings,
        },
    };

    #[tokio::test]
    async fn pprof_tool_run_reuses_uploads_tasks_and_result_api() {
        let (state, root) = test_state_with_pprof_tool();
        create_test_upload(&state, "upl_pprof", "sample.pb.gz").await;
        let app = http::router(state.clone()).with_state(state.clone());

        let list = app
            .clone()
            .oneshot(
                Request::get("/api/tools")
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list.status(), StatusCode::OK);
        let body = to_bytes(list.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let tools = body["tools"].as_array().unwrap();
        let pprof = tools
            .iter()
            .find(|tool| tool["toolId"] == "pprof_analyzer")
            .unwrap();
        assert_eq!(pprof["enabled"], true);
        assert_eq!(pprof["source"], "configured");
        assert_eq!(pprof["runnable"], true);
        assert_eq!(pprof["exportable"], true);

        let created = app
            .clone()
            .oneshot(
                Request::post("/api/tools/pprof_analyzer/runs")
                    .header("authorization", "Bearer test-key")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"uploadIds":["upl_pprof"],"params":{"sampleIndex":"samples","nodeCount":20}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(created.status(), StatusCode::ACCEPTED);
        let body = to_bytes(created.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["taskKind"], "tool_run");
        let task_id = body["taskId"].as_str().unwrap();

        wait_for_tool_run(&app, task_id, "SUCCEEDED").await;
        let result = app
            .clone()
            .oneshot(
                Request::get(format!("/api/tools/runs/{task_id}/result"))
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(result.status(), StatusCode::OK);
        let body = to_bytes(result.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["toolId"], "pprof_analyzer");
        assert_eq!(body["result"]["status"], "OK");
        assert_eq!(body["result"]["top"][0]["function"], "pkg.hot");
        assert!(body["result"]["artifacts"]["topTextPath"]
            .as_str()
            .unwrap()
            .ends_with("top.txt"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn configured_tool_run_extracts_upload_and_runs_command() {
        let (state, root) = test_state_with_configured_tool();
        create_test_upload(&state, "upl_log", "sample.log").await;
        let app = http::router(state.clone()).with_state(state.clone());

        let list = app
            .clone()
            .oneshot(
                Request::get("/api/tools")
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list.status(), StatusCode::OK);
        let body = to_bytes(list.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let checker = body["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool["toolId"] == "echo_checker")
            .unwrap();
        assert_eq!(checker["runnable"], true);
        assert_eq!(
            checker["paramsTemplate"]["inputFiles"]
                .as_array()
                .unwrap()
                .len(),
            0
        );

        let created = app
            .clone()
            .oneshot(
                Request::post("/api/tools/echo_checker/runs")
                    .header("authorization", "Bearer test-key")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"uploadIds":["upl_log"],"params":{"inputFiles":[]}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(created.status(), StatusCode::ACCEPTED);
        let body = to_bytes(created.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let task_id = body["taskId"].as_str().unwrap();

        wait_for_tool_run(&app, task_id, "SUCCEEDED").await;
        let result = app
            .clone()
            .oneshot(
                Request::get(format!("/api/tools/runs/{task_id}/result"))
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(result.status(), StatusCode::OK);
        let body = to_bytes(result.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["toolId"], "echo_checker");
        assert_eq!(body["result"]["status"], "OK");
        assert_eq!(
            body["result"]["results"][0]["result"]["summary"],
            "manual ok"
        );
        assert!(body["result"]["results"][0]["result"]["inputFile"]
            .as_str()
            .unwrap()
            .starts_with("extracted/sample/"));
        let _ = std::fs::remove_dir_all(root);
    }

    fn test_state_with_pprof_tool() -> (Arc<AppState>, std::path::PathBuf) {
        let root = test_root("logagent-tools-api-pprof");
        let tool_path = write_fake_go(&root);
        let mut tools = std::collections::BTreeMap::new();
        tools.insert(
            "pprof_analyzer".to_string(),
            ToolSettings {
                name: "pprof_analyzer".to_string(),
                enabled: true,
                path: tool_path,
                timeout_seconds: 5,
                max_output_bytes: 1024 * 1024,
                max_input_files: 1,
                args: Vec::new(),
                match_settings: ToolMatchSettings::default(),
            },
        );
        test_state_with_tools(root, tools)
    }

    fn test_state_with_configured_tool() -> (Arc<AppState>, std::path::PathBuf) {
        let root = test_root("logagent-tools-api-configured");
        let tool_path = write_fake_checker(&root);
        let mut tools = std::collections::BTreeMap::new();
        tools.insert(
            "echo_checker".to_string(),
            ToolSettings {
                name: "echo_checker".to_string(),
                enabled: true,
                path: tool_path,
                timeout_seconds: 5,
                max_output_bytes: 1024 * 1024,
                max_input_files: 2,
                args: vec![
                    "--input".to_string(),
                    "{input_file}".to_string(),
                    "--manifest".to_string(),
                    "{manifest_path}".to_string(),
                    "--grep".to_string(),
                    "{grep_results_path}".to_string(),
                ],
                match_settings: ToolMatchSettings {
                    file_patterns: vec!["*.log".to_string()],
                    keywords: Vec::new(),
                },
            },
        );
        test_state_with_tools(root, tools)
    }

    fn test_root(prefix: &str) -> std::path::PathBuf {
        static NEXT_TEST_ROOT: AtomicU64 = AtomicU64::new(1);
        std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            NEXT_TEST_ROOT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn test_state_with_tools(
        root: std::path::PathBuf,
        tools: std::collections::BTreeMap<String, ToolSettings>,
    ) -> (Arc<AppState>, std::path::PathBuf) {
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
            tools: ToolsSettings { tools },
            remote_execution: crate::support::config::RemoteExecutionSettings::default(),
            mcp: crate::support::config::McpSettings::default(),
            dev_selftest: crate::support::config::DevSelftestSettings::default(),
        });
        config.prepare_dirs().unwrap();
        (AppState::new(config).unwrap(), root)
    }

    fn write_fake_go(root: &std::path::Path) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;

        std::fs::create_dir_all(root).unwrap();
        let path = root.join("fake-go.sh");
        std::fs::write(
            &path,
            r#"#!/usr/bin/env bash
mode="$3"
if [[ "$mode" == "-top" ]]; then
  cat <<'OUT'
File: sample
Type: cpu
Showing nodes accounting for 970ms, 100% of 970ms total
      flat  flat%   sum%        cum   cum%
     490ms 50.52% 50.52%      900ms 92.78%  pkg.hot
OUT
elif [[ "$mode" == "-tree" ]]; then
  printf 'tree output\n'
elif [[ "$mode" == "-raw" ]]; then
  printf 'raw output\n'
elif [[ "$mode" == "-svg" ]]; then
  printf '<svg></svg>\n'
else
  echo "unexpected mode $mode" >&2
  exit 2
fi
"#,
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).unwrap();
        path
    }

    fn write_fake_checker(root: &std::path::Path) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;

        std::fs::create_dir_all(root).unwrap();
        let path = root.join("fake-checker.sh");
        std::fs::write(
            &path,
            r#"#!/usr/bin/env bash
set -euo pipefail
input=""
manifest=""
grep=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --input) input="$2"; shift 2 ;;
    --manifest) manifest="$2"; shift 2 ;;
    --grep) grep="$2"; shift 2 ;;
    *) shift ;;
  esac
done
test -f "$input"
test -f "$manifest"
test -f "$grep"
printf '{"summary":"manual ok","findings":[{"message":"checked input"}]}\n'
"#,
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).unwrap();
        path
    }

    async fn create_test_upload(state: &Arc<AppState>, upload_id: &str, filename: &str) {
        let upload_dir = state.config.storage.upload_dir(upload_id);
        std::fs::create_dir_all(&upload_dir).unwrap();
        let path = upload_dir.join(filename);
        let content = b"fake upload";
        std::fs::write(&path, content).unwrap();
        let now = Utc::now();
        state
            .uploads
            .create(UploadRecord {
                schema_version: 1,
                upload_id: upload_id.to_string(),
                filename: filename.to_string(),
                size: content.len() as u64,
                expected_size: Some(content.len() as u64),
                status: UploadStatus::Complete,
                path,
                created_at: now,
                updated_at: now,
            })
            .await
            .unwrap();
    }

    async fn wait_for_tool_run(app: &axum::Router, task_id: &str, expected_status: &str) {
        let mut last_status = serde_json::Value::Null;
        let mut last_error = serde_json::Value::Null;
        for _ in 0..500 {
            let response = app
                .clone()
                .oneshot(
                    Request::get(format!("/api/tools/runs/{task_id}"))
                        .header("authorization", "Bearer test-key")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
            last_status = body["status"].clone();
            last_error = body["error"].clone();
            if body["status"] == expected_status {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        panic!(
            "tool run did not reach {expected_status}; last status={last_status}, error={last_error}"
        );
    }
}
