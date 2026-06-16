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
        CreateToolRunRequest, TaskKind, TaskRecord, TaskResponse, TaskSource, TaskStatus,
        ToolListResponse, ToolRunArtifactsResponse, ToolRunListResponse, UploadStatus,
    },
    pipeline::prepare_raw_snapshot,
    services::tools,
    support::{error::AppError, id::next_id},
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
    let normalized_params =
        tools::validate_tool_run_request(&state.config, &tool_id, upload_ids.len(), &req.params)?;

    let mut uploads = Vec::with_capacity(upload_ids.len());
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
        uploads.push(upload);
    }

    let task_id = next_id("task");
    let workspace = state.config.storage.workspace_dir(&task_id);
    let inputs = prepare_raw_snapshot(&workspace, &uploads).await?;
    let now = Utc::now();
    let record = TaskRecord {
        schema_version: 6,
        task_id: task_id.clone(),
        alias: None,
        session_id: None,
        task_kind: TaskKind::ToolRun,
        analysis_mode: state.config.claude_code.default_mode,
        analysis_language: crate::domain::models::AnalysisLanguage::ZhCn,
        source: TaskSource::Upload,
        upload_ids,
        inputs,
        source_url: None,
        tool_id: Some(tool_id),
        tool_params: normalized_params,
        tool_result_path: None,
        remote_executor_id: None,
        remote_command_id: None,
        remote_command_params: serde_json::Value::Null,
        remote_result_path: None,
        instance_id: None,
        cluster_id: None,
        node_id: None,
        question: "Run selected tool".to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use std::sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    };
    use tower::ServiceExt;

    use crate::{
        domain::models::{UploadRecord, UploadStatus},
        http,
        services::metadata::MetadataImportRequest,
        support::config::{
            AnalysisSettings, AppConfig, AuthSettings, EmbeddingSettings, LlmProvider, LlmSettings,
            LogAnalyzerSettings, ServerSettings, StorageSettings, ToolMatchSettings, ToolSettings,
            ToolsSettings,
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
        let metadata_field_types = tools
            .iter()
            .find(|tool| tool["toolId"] == "logagent.get_metadata_field_types")
            .unwrap();
        assert_eq!(metadata_field_types["source"], "built_in");
        assert_eq!(metadata_field_types["readOnly"], true);
        assert_eq!(metadata_field_types["editable"], false);
        assert_eq!(metadata_field_types["exportable"], false);
        assert_eq!(metadata_field_types["runnable"], true);
        assert!(metadata_field_types["paramsTemplate"].is_object());
        assert!(metadata_field_types["tags"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tag| tag == "metadata"));
        let metadata_tag_fields = tools
            .iter()
            .find(|tool| tool["toolId"] == "logagent.get_metadata_tag_fields")
            .unwrap();
        assert_eq!(metadata_tag_fields["source"], "built_in");
        assert_eq!(metadata_tag_fields["readOnly"], true);
        assert_eq!(metadata_tag_fields["editable"], false);
        assert_eq!(metadata_tag_fields["exportable"], false);
        assert_eq!(metadata_tag_fields["runnable"], true);
        assert_eq!(metadata_tag_fields["minFiles"], 0);
        assert_eq!(metadata_tag_fields["maxFiles"], 0);
        assert!(metadata_tag_fields["paramsTemplate"].is_object());
        assert!(metadata_tag_fields["paramsTemplate"].get("field").is_none());

        let metadata_created = app
            .clone()
            .oneshot(
                Request::post("/api/tools/logagent.list_metadata_instances/runs")
                    .header("authorization", "Bearer test-key")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"params":{}}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(metadata_created.status(), StatusCode::ACCEPTED);
        let metadata_body = to_bytes(metadata_created.into_body(), usize::MAX)
            .await
            .unwrap();
        let metadata_body: serde_json::Value = serde_json::from_slice(&metadata_body).unwrap();
        let metadata_task_id = metadata_body["taskId"].as_str().unwrap();
        wait_for_tool_run(&app, metadata_task_id, "SUCCEEDED").await;
        let metadata_result = app
            .clone()
            .oneshot(
                Request::get(format!("/api/tools/runs/{metadata_task_id}/result"))
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(metadata_result.status(), StatusCode::OK);
        let metadata_result = to_bytes(metadata_result.into_body(), usize::MAX)
            .await
            .unwrap();
        let metadata_result: serde_json::Value = serde_json::from_slice(&metadata_result).unwrap();
        assert_eq!(
            metadata_result["toolId"],
            "logagent.list_metadata_instances"
        );
        assert_eq!(metadata_result["result"]["status"], "OK");
        assert!(metadata_result["result"]["result"]["instances"].is_array());

        let preview = state
            .metadata
            .create_import_preview(MetadataImportRequest {
                template_type: "json".to_string(),
                filename: Some("metadata.json".to_string()),
                instance_id: None,
                remark: None,
                content: serde_json::json!({
                    "instances": [{
                        "instanceId": "inst-tools",
                        "clusterId": "inst-tools"
                    }],
                    "clusters": [{
                        "clusterId": "inst-tools",
                        "databases": [{
                            "name": "mydb",
                            "defaultRetentionPolicy": "autogen",
                            "retentionPolicies": [{
                                "name": "autogen",
                                "measurements": [{
                                    "name": "cpu",
                                    "schema": [
                                        { "name": "host", "typ": 6 },
                                        { "name": "usage", "typ": 3 }
                                    ]
                                }]
                            }]
                        }]
                    }]
                })
                .to_string(),
            })
            .await
            .unwrap();
        state
            .metadata
            .confirm_import(&preview.import_id)
            .await
            .unwrap();
        let tag_created = app
            .clone()
            .oneshot(
                Request::post("/api/tools/logagent.get_metadata_tag_fields/runs")
                    .header("authorization", "Bearer test-key")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"params":{"instanceId":"inst-tools","database":"mydb","measurement":"cpu"}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(tag_created.status(), StatusCode::ACCEPTED);
        let tag_body = to_bytes(tag_created.into_body(), usize::MAX).await.unwrap();
        let tag_body: serde_json::Value = serde_json::from_slice(&tag_body).unwrap();
        let tag_task_id = tag_body["taskId"].as_str().unwrap();
        wait_for_tool_run(&app, tag_task_id, "SUCCEEDED").await;
        let tag_result = app
            .clone()
            .oneshot(
                Request::get(format!("/api/tools/runs/{tag_task_id}/result"))
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(tag_result.status(), StatusCode::OK);
        let tag_result = to_bytes(tag_result.into_body(), usize::MAX).await.unwrap();
        let tag_result: serde_json::Value = serde_json::from_slice(&tag_result).unwrap();
        assert_eq!(tag_result["toolId"], "logagent.get_metadata_tag_fields");
        assert_eq!(tag_result["result"]["status"], "OK");
        let fields = tag_result["result"]["result"]["result"]["fields"]
            .as_array()
            .unwrap();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0]["name"], "host");
        assert_eq!(fields[0]["typeLabel"], "Tag");

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

        let log_tasks = app
            .oneshot(
                Request::get("/api/tasks")
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = to_bytes(log_tasks.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(body["tasks"].as_array().unwrap().is_empty());
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
            skills: crate::support::config::SkillSettings {
                enabled: false,
                roots: Vec::new(),
                max_skill_chars: 4000,
                max_reference_chars: 20_000,
            },
            log_analyzer: LogAnalyzerSettings {
                keywords: vec!["error".to_string()],
                max_matches: 20,
            },
            tools: ToolsSettings { tools },
            remote_execution: crate::support::config::RemoteExecutionSettings::default(),
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
            claude_code: crate::support::config::ClaudeCodeSettings::default(),
            mcp: crate::support::config::McpSettings::default(),
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
        for _ in 0..100 {
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
            if body["status"] == expected_status {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("tool run did not reach {expected_status}");
    }
}
