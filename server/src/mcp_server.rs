//! Standalone MCP server (stdio + HTTP) for external clients.
//!
//! Unlike `mcp.rs` (task-scoped, drives the analysis agent loop), this module
//! exposes the tool catalog and context resources with no `task_id` dependency:
//! - `tools/list` mirrors `services::tools::descriptors` (runnable tools only).
//! - `tools/call` runs a catalog tool synchronously and persists a `ToolRun`
//!   record (shared with `/api/runs`), reusing `build_tool_run_task` + `run_tool_task`.
//! - `resources/list` + `resources/read` serve skills / metadata / cases / runs /
//!   tools-catalog (no domain-adapters, no task-workspace artifacts).
//! No `log_mcp_call` / `waiting_marker_tool` / `analysis_state` coupling.

use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
    sync::Arc,
};

use axum::{
    body::{Body, Bytes},
    extract::State,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::Response,
};
use serde_json::{json, Value};
use tracing::{info, warn};

use crate::{
    app::AppState,
    domain::models::{TaskKind, TaskPhase, TaskRecord, TaskStatus, ToolDescriptor},
    services::{self, dev_selftest_allowlist, dev_selftest_profiles},
    support::config::AppConfig,
};

/// Run the standalone MCP server over stdio. Logs go to stderr (the protocol
/// owns stdout). Entry: `logagent-server mcp-serve`.
pub async fn run_stdio(config: Arc<AppConfig>, config_path: Option<PathBuf>) -> anyhow::Result<()> {
    if !config.mcp.enabled {
        anyhow::bail!("MCP is disabled by configuration");
    }
    let state = AppState::new_with_config_path(config, config_path)?;
    info!("standalone MCP stdio server started");
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(err) => {
                write_response(&mut stdout, &json_rpc_error(None, -32700, err.to_string()))?;
                continue;
            }
        };
        let response = handle_request(&state, &request).await;
        write_response(&mut stdout, &response)?;
    }
    Ok(())
}

fn write_response(stdout: &mut io::Stdout, response: &Value) -> anyhow::Result<()> {
    stdout.write_all(serde_json::to_string(response)?.as_bytes())?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(())
}

/// HTTP JSON-RPC entry (`POST /api/mcp`). Stateless MCP streamable-http transport:
/// accepts a single request or a batch, responds `application/json` (or a single SSE
/// `event: message` frame when the client sends `Accept: text/event-stream`). No
/// `Mcp-Session-Id` is issued (stateless server). `Authorization` is enforced by the
/// shared `require_api_key` middleware; `Origin` is checked against
/// `mcp.allowed_origins` when that list is non-empty.
pub async fn http_mcp(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Some(rejection) = check_origin(&state.config.mcp, &headers) {
        return rejection;
    }
    let value: Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(err) => {
            return mcp_response(
                &headers,
                StatusCode::OK,
                json_rpc_error(None, -32700, format!("parse error: {err}")),
            )
        }
    };
    let result = handle_http(&state, value).await;
    mcp_response(&headers, StatusCode::OK, result)
}

/// `GET /api/mcp` — this server emits no server-initiated notifications, so the SSE
/// notification channel is not offered; clients drive everything via POST.
pub async fn get_mcp(State(_state): State<Arc<AppState>>, _headers: HeaderMap) -> Response {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::METHOD_NOT_ALLOWED;
    response
        .headers_mut()
        .insert(header::ALLOW, HeaderValue::from_static("POST"));
    response
}

/// Reject cross-origin browser requests unless `Origin` is in the configured allowlist.
/// A missing `Origin` header (non-browser / tunneled clients) is always allowed. An
/// empty allowlist disables the check (localhost / SSH-tunnel usage).
fn check_origin(
    config: &crate::support::config::McpSettings,
    headers: &HeaderMap,
) -> Option<Response> {
    if config.allowed_origins.is_empty() {
        return None;
    }
    let origin = match headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    {
        Some(origin) => origin,
        None => return None,
    };
    if config
        .allowed_origins
        .iter()
        .any(|allowed| allowed == origin)
    {
        None
    } else {
        Some(mcp_response(
            headers,
            StatusCode::FORBIDDEN,
            json_rpc_error(None, -32000, format!("origin not allowed: {origin}")),
        ))
    }
}

/// Build the streamable-http response. Content-type follows the request `Accept`
/// header (`text/event-stream` ⇒ single SSE `event: message` frame, else JSON). The
/// request's `MCP-Protocol-Version` header is echoed back (default `2025-06-18`).
fn mcp_response(headers: &HeaderMap, status: StatusCode, value: Value) -> Response {
    let accept = headers
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    let content_type = if accept.contains("text/event-stream") {
        "text/event-stream"
    } else {
        "application/json"
    };
    let body = if content_type == "text/event-stream" {
        let payload = serde_json::to_string(&value).unwrap_or_else(|_| value.to_string());
        format!("event: message\ndata: {payload}\n\n")
    } else {
        serde_json::to_string(&value).unwrap_or_else(|_| value.to_string())
    };
    let protocol_version = headers
        .get("mcp-protocol-version")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("2025-06-18");
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, content_type)
        .header(
            "mcp-protocol-version",
            HeaderValue::from_str(protocol_version)
                .unwrap_or_else(|_| HeaderValue::from_static("2025-06-18")),
        )
        .body(Body::from(body))
        .expect("valid MCP response")
}

/// Handle a single request or a batch array. Reused by stdio and HTTP.
pub async fn handle_http(state: &Arc<AppState>, body: Value) -> Value {
    if !state.config.mcp.enabled {
        let id = body.get("id").cloned();
        return json_rpc_error(id, -32000, "MCP is disabled by configuration");
    }
    if let Some(items) = body.as_array() {
        let mut responses = Vec::with_capacity(items.len());
        for item in items {
            responses.push(handle_request(state, item).await);
        }
        Value::Array(responses)
    } else {
        handle_request(state, &body).await
    }
}

/// Handle a single JSON-RPC request object, returning the full response object.
pub async fn handle_request(state: &Arc<AppState>, request: &Value) -> Value {
    let id = request.get("id").cloned();
    let method = request
        .get("method")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let params = request.get("params").cloned().unwrap_or_else(|| json!({}));
    let result = match method.as_str() {
        "initialize" => Ok(initialize_result()),
        "ping" => Ok(json!({})),
        "prompts/list" => Ok(json!({ "prompts": [] })),
        "tools/list" => Ok(tools_list(state)),
        "tools/call" => {
            let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let arguments = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            Ok(call_tool(state, name, arguments).await)
        }
        "resources/list" => resources_list().await,
        "resources/read" => {
            let uri = params.get("uri").and_then(|v| v.as_str()).unwrap_or("");
            resources_read(state, uri).await
        }
        other => Err(anyhow::anyhow!("unsupported MCP method {other}")),
    };
    match result {
        Ok(result) => {
            info!(method = %method, "MCP request succeeded");
            json!({ "jsonrpc": "2.0", "id": id, "result": result })
        }
        Err(err) => {
            warn!(method = %method, error = %err, "MCP request failed");
            json_rpc_error(id, -32000, format!("{err:#}"))
        }
    }
}

fn json_rpc_error(id: Option<Value>, code: i64, message: impl Into<String>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message.into() }
    })
}

fn initialize_result() -> Value {
    json!({
        "protocolVersion": "2025-06-18",
        "capabilities": { "resources": {}, "tools": {} },
        "serverInfo": { "name": "localtoolhub-mcp", "version": env!("CARGO_PKG_VERSION") }
    })
}

fn tools_list(state: &Arc<AppState>) -> Value {
    let tools: Vec<Value> = services::tools::descriptors(&state.config)
        .into_iter()
        .filter(|descriptor| descriptor.runnable || descriptor.platform)
        .map(|descriptor| {
            json!({
                "name": descriptor.tool_id,
                "description": descriptor.description,
                "inputSchema": mcp_input_schema(&descriptor),
            })
        })
        .collect();
    json!({ "tools": tools })
}

fn mcp_input_schema(descriptor: &ToolDescriptor) -> Value {
    let mut schema = descriptor.params_schema.clone();
    if descriptor.runnable && !descriptor.platform {
        let obj = match schema.as_object_mut() {
            Some(obj) => obj,
            None => return schema,
        };
        let properties = obj.entry("properties").or_insert_with(|| json!({}));
        if let Some(properties) = properties.as_object_mut() {
            properties.entry("runMode").or_insert_with(|| {
                json!({
                    "type": "string",
                    "enum": ["sync", "queued"],
                    "description": "Optional execution mode. Defaults to sync; queued returns a task_* runId for polling with logagent.runs.get/result."
                })
            });
        }
    }
    schema
}

async fn call_tool(state: &Arc<AppState>, name: &str, arguments: Value) -> Value {
    if name == dev_selftest_allowlist::ALLOWLIST_UPDATE_TOOL_ID {
        return tool_call_content(call_allowlist_update(state, arguments).await);
    }
    if name == dev_selftest_profiles::PROFILE_UPSERT_TOOL_ID {
        return tool_call_content(call_profile_upsert(state, arguments).await);
    }
    // MCP-native platform tools bypass the Tool Runner: no ToolRun is created, so
    // polling them never pollutes run history.
    if let Some(outcome) = platform_tool_result(state, name, &arguments).await {
        return tool_call_content(outcome);
    }
    // `runMode: "queued"` enqueues one ToolRun and returns its id without awaiting;
    // default `sync` runs inline (current behavior). The borrow ends before
    // `arguments` is moved into the chosen branch.
    let queued = arguments.get("runMode").and_then(|value| value.as_str()) == Some("queued");
    let outcome = if queued {
        run_catalog_tool_queued(state, name, arguments).await
    } else {
        run_catalog_tool(state, name, arguments).await
    };
    tool_call_content(outcome)
}

async fn call_allowlist_update(state: &Arc<AppState>, arguments: Value) -> anyhow::Result<Value> {
    let request: dev_selftest_allowlist::AllowlistUpdateRequest = serde_json::from_value(arguments)
        .map_err(|err| anyhow::anyhow!("invalid allowlist update arguments: {err}"))?;
    let response = dev_selftest_allowlist::update_allowlist(state, request).await?;
    serde_json::to_value(response)
        .map_err(|err| anyhow::anyhow!("failed to encode allowlist update response: {err}"))
}

async fn call_profile_upsert(state: &Arc<AppState>, arguments: Value) -> anyhow::Result<Value> {
    let request: dev_selftest_profiles::ProfileUpsertRequest = serde_json::from_value(arguments)
        .map_err(|err| anyhow::anyhow!("invalid profile upsert arguments: {err}"))?;
    let response = dev_selftest_profiles::upsert_profile(state, request).await?;
    serde_json::to_value(response)
        .map_err(|err| anyhow::anyhow!("failed to encode profile upsert response: {err}"))
}

fn tool_call_content(outcome: anyhow::Result<Value>) -> Value {
    match outcome {
        Ok(value) => json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string())
            }],
            "isError": false
        }),
        Err(err) => json!({
            "content": [{ "type": "text", "text": format!("{err:#}") }],
            "isError": true
        }),
    }
}

/// Extract tool params from a `tools/call` arguments object. Accepts either
/// `{params: {...}}` (the HTTP `POST /api/tools/:id/runs` envelope) or top-level
/// tool params (MCP-spec, where `arguments` IS the tool input per `inputSchema`),
/// stripping the envelope fields `runMode`/`uploadIds` in the latter case. This
/// lets real MCP clients (Claude Code) call params-taking tools without nesting.
fn mcp_tool_params(arguments: &Value) -> Value {
    if let Some(params) = arguments.get("params") {
        return params.clone();
    }
    let mut params = arguments.clone();
    if let Some(obj) = params.as_object_mut() {
        obj.remove("runMode");
        obj.remove("uploadIds");
    }
    params
}

/// Run a catalog tool by toolId synchronously and persist a `ToolRun` record.
async fn run_catalog_tool(
    state: &Arc<AppState>,
    tool_id: &str,
    arguments: Value,
) -> anyhow::Result<Value> {
    let descriptor = services::tools::get_descriptor(&state.config, tool_id)
        .ok_or_else(|| anyhow::anyhow!("unknown toolId {tool_id}"))?;
    if !descriptor.runnable {
        anyhow::bail!("tool {tool_id} is not runnable");
    }
    let upload_ids: Vec<String> = arguments
        .get("uploadIds")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|value| value.as_str().map(ToString::to_string))
                .collect()
        })
        .unwrap_or_default();
    let params = mcp_tool_params(&arguments);
    let task = services::tools::build_tool_run_task(state, tool_id, upload_ids, &params).await?;
    let task_id = task.task_id.clone();
    state
        .tasks
        .create(task.clone())
        .await
        .map_err(|err| anyhow::anyhow!("failed to persist run: {err}"))?;
    state
        .tasks
        .start_attempt(&task_id, TaskPhase::RunTool)
        .await?;
    let result_path = match services::tools::run_tool_task(state.clone(), task).await {
        Ok(path) => path,
        Err(err) => {
            let message = format!("{err:#}");
            let _ = state
                .tasks
                .fail(&task_id, Some(TaskPhase::RunTool), message.clone())
                .await;
            return Err(err.into());
        }
    };
    let result_path_str = result_path.display().to_string();
    if let Err(err) = state
        .tasks
        .succeed_tool_run(&task_id, TaskPhase::RunTool, result_path_str)
        .await
    {
        let message = format!("{err:#}");
        let _ = state
            .tasks
            .fail(&task_id, Some(TaskPhase::RunTool), message)
            .await;
        return Err(err);
    }
    let raw = tokio::fs::read_to_string(&result_path).await?;
    Ok(serde_json::from_str(&raw)?)
}

/// `runMode: "queued"` path: build + persist one `ToolRun`, enqueue it, and return
/// its id immediately without awaiting execution. Mirrors the HTTP
/// `POST /api/tools/:id/runs` (202 ACCEPTED) contract. Ordinary tools do not spawn
/// child runs — there is exactly one run per queued call.
async fn run_catalog_tool_queued(
    state: &Arc<AppState>,
    tool_id: &str,
    arguments: Value,
) -> anyhow::Result<Value> {
    let descriptor = services::tools::get_descriptor(&state.config, tool_id)
        .ok_or_else(|| anyhow::anyhow!("unknown toolId {tool_id}"))?;
    if !descriptor.runnable {
        anyhow::bail!("tool {tool_id} is not runnable");
    }
    if descriptor.platform {
        anyhow::bail!("platform tools do not support runMode:queued");
    }
    let upload_ids: Vec<String> = arguments
        .get("uploadIds")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|value| value.as_str().map(ToString::to_string))
                .collect()
        })
        .unwrap_or_default();
    let params = mcp_tool_params(&arguments);
    let task = services::tools::build_tool_run_task(state, tool_id, upload_ids, &params).await?;
    let task_id = task.task_id.clone();
    state
        .tasks
        .create(task)
        .await
        .map_err(|err| anyhow::anyhow!("failed to persist run: {err}"))?;
    state.executor.enqueue(state.clone(), task_id.clone());
    let base = state.config.server.public_base_url.trim_end_matches('/');
    Ok(json!({
        "runId": task_id,
        "toolId": tool_id,
        "status": "QUEUED",
        "url": format!("{base}/api/runs/{task_id}"),
    }))
}

/// MCP-native platform tools. Returns `Some` only for platform tool names, serving
/// them directly from `TaskStore` without creating a `ToolRun` (so polling never
/// pollutes run history).
async fn platform_tool_result(
    state: &Arc<AppState>,
    name: &str,
    arguments: &Value,
) -> Option<anyhow::Result<Value>> {
    let run_id = arguments
        .get("runId")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    match name {
        "logagent.runs.get" => Some(runs_get(state, &run_id).await),
        "logagent.runs.result" => Some(runs_result(state, &run_id).await),
        _ => None,
    }
}

async fn runs_get(state: &Arc<AppState>, run_id: &str) -> anyhow::Result<Value> {
    validate_run_id(run_id)?;
    let task = state
        .tasks
        .get(run_id)
        .await
        .ok_or_else(|| anyhow::anyhow!("unknown runId {run_id}"))?;
    Ok(json!({
        "runId": task.task_id,
        "taskKind": task.task_kind,
        "status": task.status,
        "phase": task.phase,
        "toolId": task.tool_id,
        "remoteExecutorId": task.remote_executor_id,
        "remoteCommandId": task.remote_command_id,
        "error": task.error,
        "createdAt": task.created_at,
        "updatedAt": task.updated_at,
        "resultAvailable": task.status == TaskStatus::Succeeded && result_path_for(&task).is_some(),
    }))
}

async fn runs_result(state: &Arc<AppState>, run_id: &str) -> anyhow::Result<Value> {
    validate_run_id(run_id)?;
    let task = state
        .tasks
        .get(run_id)
        .await
        .ok_or_else(|| anyhow::anyhow!("unknown runId {run_id}"))?;
    if task.status != TaskStatus::Succeeded {
        anyhow::bail!(
            "run {run_id} result is only available after success (status {:?})",
            task.status
        );
    }
    let path = result_path_for(&task)
        .ok_or_else(|| anyhow::anyhow!("successful run is missing a result artifact path"))?;
    let raw = tokio::fs::read_to_string(&path)
        .await
        .map_err(|err| anyhow::anyhow!("artifact not found: {err}"))?;
    let result: Value = serde_json::from_str(&raw)
        .map_err(|err| anyhow::anyhow!("failed to parse artifact JSON: {err}"))?;
    Ok(json!({
        "runId": task.task_id,
        "taskKind": task.task_kind,
        "toolId": task.tool_id,
        "resultPath": path,
        "result": result,
    }))
}

fn result_path_for(task: &TaskRecord) -> Option<String> {
    match task.task_kind {
        TaskKind::ToolRun => task.tool_result_path.clone(),
        TaskKind::RemoteCommandRun => task.remote_result_path.clone(),
    }
}

fn validate_run_id(run_id: &str) -> anyhow::Result<()> {
    let valid = run_id.starts_with("task_")
        && run_id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'_' || value == b'-');
    if valid {
        Ok(())
    } else {
        anyhow::bail!("invalid runId")
    }
}

async fn resources_list() -> anyhow::Result<Value> {
    let resources = vec![
        resource(
            dev_selftest_allowlist::CONFIG_RESOURCE_URI,
            "dev_selftest_config",
            "Current dev_selftest git allowlist defaults and profile ids.",
        ),
        resource(
            "logagent://runs/recent",
            "runs_recent",
            "Recent run records.",
        ),
        resource(
            "logagent://tools/catalog",
            "tools_catalog",
            "Configured tool catalog.",
        ),
    ];
    Ok(json!({ "resources": resources }))
}

fn resource(
    uri: impl Into<String>,
    name: impl Into<String>,
    description: impl Into<String>,
) -> Value {
    json!({
        "uri": uri.into(),
        "name": name.into(),
        "description": description.into(),
        "mimeType": "application/json"
    })
}

async fn resources_read(state: &Arc<AppState>, uri: &str) -> anyhow::Result<Value> {
    let value = match uri {
        "logagent://runs/recent" => {
            let runs: Vec<Value> = state
                .tasks
                .list()
                .await
                .into_iter()
                .take(20)
                .map(|task| {
                    json!({
                        "taskId": task.task_id,
                        "taskKind": task.task_kind,
                        "status": task.status,
                        "toolId": task.tool_id,
                        "createdAt": task.created_at,
                    })
                })
                .collect();
            json!({ "schemaVersion": 1, "runs": runs })
        }
        "logagent://tools/catalog" => {
            json!({ "schemaVersion": 1, "tools": services::tools::descriptors(&state.config) })
        }
        dev_selftest_allowlist::CONFIG_RESOURCE_URI => {
            serde_json::to_value(dev_selftest_allowlist::summary_for_state(state))
                .map_err(|err| anyhow::anyhow!("failed to encode dev_selftest config: {err}"))?
        }
        _ => anyhow::bail!("unknown resource URI {uri}"),
    };
    Ok(json!({
        "contents": [{
            "uri": uri,
            "mimeType": "application/json",
            "text": serde_json::to_string_pretty(&value)?
        }]
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::collections::BTreeMap;

    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    use crate::{
        domain::models::TaskKind,
        support::config::{
            AppConfig, AuthSettings, DevSelftestGitRepo, DevSelftestGitSettings,
            LogAnalyzerSettings, McpSettings, ServerSettings, StorageSettings, ToolsSettings,
        },
    };

    const TEST_GIT_REPO: &str = "https://example.test/project.git";
    const TEST_GIT_REF: &str = "main";

    fn request(id: i64, method: &str, params: Value) -> Value {
        json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
    }

    #[test]
    fn mcp_tool_params_accepts_envelope_and_top_level() {
        // HTTP-style envelope: {params: {...}} wins.
        assert_eq!(
            mcp_tool_params(&json!({"params": {"a": 1}, "runMode": "queued"})),
            json!({"a": 1})
        );
        // MCP-spec top-level args: runMode/uploadIds stripped, rest kept.
        assert_eq!(
            mcp_tool_params(&json!({"a": 1, "runMode": "queued"})),
            json!({"a": 1})
        );
        assert_eq!(
            mcp_tool_params(&json!({"a": 1, "uploadIds": ["upl_1"]})),
            json!({"a": 1})
        );
        // Empty arguments -> empty params.
        assert_eq!(mcp_tool_params(&json!({})), json!({}));
    }

    #[tokio::test]
    async fn tools_list_advertises_run_mode_for_runnable_tools() {
        let (state, root) = test_state("mcp-runmode-schema");
        let listed = handle_request(&state, &request(1, "tools/list", json!({}))).await;
        let tools = listed["result"]["tools"].as_array().unwrap();

        let build = tools
            .iter()
            .find(|tool| tool["name"] == "logagent.dev_selftest.build")
            .unwrap();
        assert_eq!(
            build["inputSchema"]["properties"]["runMode"]["enum"],
            json!(["sync", "queued"])
        );

        let runs_get = tools
            .iter()
            .find(|tool| tool["name"] == "logagent.runs.get")
            .unwrap();
        assert!(runs_get["inputSchema"]["properties"]
            .get("runMode")
            .is_none());

        let _ = std::fs::remove_dir_all(root);
    }

    /// Decode the `tools/call` text content payload into its JSON value.
    fn call_payload(response: &Value) -> serde_json::Value {
        let text = response["result"]["content"][0]["text"].as_str().unwrap();
        serde_json::from_str(text).unwrap()
    }

    #[tokio::test]
    async fn tools_call_queued_returns_runid_and_is_pollable() {
        let (state, root) = test_state("mcp-queued");
        // queued tools/call returns a runId immediately without awaiting.
        let queued = handle_request(
            &state,
            &request(
                1,
                "tools/call",
                json!({
                    "name": "logagent.dev_selftest.sync_workspace",
                    "arguments": {
                        "label": "queued",
                        "gitRepo": TEST_GIT_REPO,
                        "gitRef": TEST_GIT_REF,
                        "runMode": "queued"
                    }
                }),
            ),
        )
        .await;
        assert_eq!(queued["result"]["isError"], false);
        let parsed = call_payload(&queued);
        let run_id = parsed["runId"].as_str().unwrap().to_string();
        assert_eq!(parsed["status"], "QUEUED");

        // Poll logagent.runs.get until SUCCEEDED.
        let mut status = serde_json::Value::Null;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            let poll = handle_request(
                &state,
                &request(
                    2,
                    "tools/call",
                    json!({ "name": "logagent.runs.get", "arguments": { "runId": run_id } }),
                ),
            )
            .await;
            status = call_payload(&poll)["status"].clone();
            if status == "SUCCEEDED" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert_eq!(status, "SUCCEEDED", "queued run did not succeed");

        // logagent.runs.result returns the structured result.
        let result = handle_request(
            &state,
            &request(
                3,
                "tools/call",
                json!({ "name": "logagent.runs.result", "arguments": { "runId": run_id } }),
            ),
        )
        .await;
        let parsed = call_payload(&result);
        assert_eq!(parsed["result"]["status"], "OK");

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn platform_run_tools_create_no_run_record() {
        let (state, root) = test_state("mcp-platform");
        let before = state
            .tasks
            .list()
            .await
            .into_iter()
            .filter(|task| task.task_kind == TaskKind::ToolRun)
            .count();

        // runs.get with an unknown (but well-formed) runId errors, creates no task.
        let unknown = handle_request(
            &state,
            &request(
                1,
                "tools/call",
                json!({
                    "name": "logagent.runs.get",
                    "arguments": { "runId": "task_does_not_exist" }
                }),
            ),
        )
        .await;
        assert_eq!(unknown["result"]["isError"], true);

        // runs.get with a malformed runId errors, creates no task.
        let bad = handle_request(
            &state,
            &request(
                2,
                "tools/call",
                json!({ "name": "logagent.runs.get", "arguments": { "runId": "not-a-task-id" } }),
            ),
        )
        .await;
        assert_eq!(bad["result"]["isError"], true);

        let after = state
            .tasks
            .list()
            .await
            .into_iter()
            .filter(|task| task.task_kind == TaskKind::ToolRun)
            .count();
        assert_eq!(
            before, after,
            "platform run tools must not create run records"
        );

        // tools/list advertises the platform tools.
        let listed = handle_request(&state, &request(3, "tools/list", json!({}))).await;
        let names: Vec<&str> = listed["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"logagent.runs.get"));
        assert!(names.contains(&"logagent.runs.result"));
        assert!(names.contains(&"logagent.dev_selftest.profiles.upsert"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn http_mcp_negotiates_content_type_and_echoes_protocol_version() {
        let (state, root) = test_state("mcp-streamable");
        let app = crate::http::router(state.clone()).with_state(state.clone());

        // JSON response by default; MCP-Protocol-Version is echoed.
        let resp = app
            .clone()
            .oneshot(
                Request::post("/api/mcp")
                    .header("authorization", "Bearer test-key")
                    .header("content-type", "application/json")
                    .header("mcp-protocol-version", "2025-06-18")
                    .body(Body::from(
                        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers()
                .get("content-type")
                .unwrap()
                .to_str()
                .unwrap(),
            "application/json"
        );
        assert_eq!(
            resp.headers()
                .get("mcp-protocol-version")
                .unwrap()
                .to_str()
                .unwrap(),
            "2025-06-18"
        );

        // SSE response when Accept: text/event-stream.
        let resp = app
            .clone()
            .oneshot(
                Request::post("/api/mcp")
                    .header("authorization", "Bearer test-key")
                    .header("content-type", "application/json")
                    .header("accept", "text/event-stream")
                    .body(Body::from(
                        r#"{"jsonrpc":"2.0","id":2,"method":"ping","params":{}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers()
                .get("content-type")
                .unwrap()
                .to_str()
                .unwrap(),
            "text/event-stream"
        );
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let text = String::from_utf8_lossy(&body);
        assert!(text.starts_with("event: message\ndata: "));

        // GET -> 405 with Allow: POST.
        let resp = app
            .oneshot(
                Request::get("/api/mcp")
                    .header("authorization", "Bearer test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(
            resp.headers().get("allow").unwrap().to_str().unwrap(),
            "POST"
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn http_mcp_rejects_disallowed_origin() {
        let (state, root) =
            test_state_with_origins("mcp-origin", vec!["https://allowed.example".to_string()]);
        let app = crate::http::router(state.clone()).with_state(state);
        let resp = app
            .oneshot(
                Request::post("/api/mcp")
                    .header("authorization", "Bearer test-key")
                    .header("origin", "https://evil.example")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"jsonrpc":"2.0","id":1,"method":"ping","params":{}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn mcp_server_lists_runs_and_reads_resources() {
        let (state, root) = test_state("mcp-server");

        let initialized = handle_request(&state, &request(1, "initialize", json!({}))).await;
        assert_eq!(
            initialized["result"]["serverInfo"]["name"],
            "localtoolhub-mcp"
        );

        let listed = handle_request(&state, &request(2, "tools/list", json!({}))).await;
        let names: Vec<&str> = listed["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"logagent.dev_selftest.sync_workspace"));
        assert!(names.contains(&"logagent.dev_selftest.allowlist.update"));
        assert!(names.contains(&"logagent.dev_selftest.profiles.upsert"));

        // tools/call a runnable built-in that needs no uploads.
        let called = handle_request(
            &state,
            &request(
                3,
                "tools/call",
                json!({
                    "name": "logagent.dev_selftest.sync_workspace",
                    "arguments": {
                        "label": "mcp",
                        "gitRepo": TEST_GIT_REPO,
                        "gitRef": TEST_GIT_REF
                    }
                }),
            ),
        )
        .await;
        assert_eq!(called["result"]["isError"], false);
        assert!(state
            .tasks
            .list()
            .await
            .iter()
            .any(|task| task.task_kind == TaskKind::ToolRun));

        // Unknown tool -> isError.
        let unknown = handle_request(
            &state,
            &request(
                4,
                "tools/call",
                json!({ "name": "does.not.exist", "arguments": {} }),
            ),
        )
        .await;
        assert_eq!(unknown["result"]["isError"], true);

        // resources/list exposes the base context URIs plus dev_selftest config discovery.
        let resources = handle_request(&state, &request(5, "resources/list", json!({}))).await;
        let uris: Vec<&str> = resources["result"]["resources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["uri"].as_str().unwrap())
            .collect();
        for expected in [
            "logagent://dev_selftest/config",
            "logagent://runs/recent",
            "logagent://tools/catalog",
        ] {
            assert!(uris.contains(&expected), "missing resource {expected}");
        }
        assert_eq!(
            uris.len(),
            3,
            "only dev_selftest/config, runs/recent and tools/catalog resources remain"
        );

        let config_resource = handle_request(
            &state,
            &request(
                6,
                "resources/read",
                json!({ "uri": "logagent://dev_selftest/config" }),
            ),
        )
        .await;
        let config_text = config_resource["result"]["contents"][0]["text"]
            .as_str()
            .unwrap();
        let config_value: Value = serde_json::from_str(config_text).unwrap();
        assert_eq!(config_value["defaultGitRepo"], TEST_GIT_REPO);
        assert_eq!(config_value["defaultGitRef"], TEST_GIT_REF);
        assert!(config_value["buildProfileDetails"].is_array());
        assert!(config_value["testSuiteDetails"].is_array());

        // Batch over HTTP.
        let batch = handle_http(
            &state,
            json!([
                request(7, "ping", json!({})),
                request(8, "prompts/list", json!({}))
            ]),
        )
        .await;
        assert_eq!(batch[0]["result"], json!({}));
        assert_eq!(batch[1]["result"]["prompts"].as_array().unwrap().len(), 0);

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn http_mcp_respects_disabled_config() {
        let (state, root) = test_state_with_mcp_enabled("mcp-server-disabled", false, Vec::new());

        let response = handle_http(&state, request(1, "tools/list", json!({}))).await;
        assert_eq!(response["error"]["code"], -32000);
        assert!(response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("disabled"));

        let _ = std::fs::remove_dir_all(root);
    }

    fn test_state(prefix: &str) -> (Arc<AppState>, std::path::PathBuf) {
        test_state_with_mcp_enabled(prefix, true, Vec::new())
    }

    fn test_state_with_origins(
        prefix: &str,
        origins: Vec<String>,
    ) -> (Arc<AppState>, std::path::PathBuf) {
        test_state_with_mcp_enabled(prefix, true, origins)
    }

    fn test_state_with_mcp_enabled(
        prefix: &str,
        mcp_enabled: bool,
        allowed_origins: Vec<String>,
    ) -> (Arc<AppState>, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "logagent-{prefix}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let fake_git = write_fake_git(&root);
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
            mcp: McpSettings {
                enabled: mcp_enabled,
                allowed_origins,
            },
            dev_selftest: crate::support::config::DevSelftestSettings {
                enabled: true,
                git: DevSelftestGitSettings {
                    enabled: true,
                    binary: fake_git,
                    repos: vec![DevSelftestGitRepo {
                        url: TEST_GIT_REPO.to_string(),
                        refs: vec![TEST_GIT_REF.to_string()],
                    }],
                },
                ..crate::support::config::DevSelftestSettings::default()
            },
        });
        config.prepare_dirs().unwrap();
        (AppState::new(config).unwrap(), root)
    }

    #[cfg(unix)]
    fn write_fake_git(root: &std::path::Path) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let fake_git = root.join("fake-git.sh");
        std::fs::write(
            &fake_git,
            r#"#!/usr/bin/env bash
set -euo pipefail
if [ "${1:-}" = "clone" ]; then
  dest="${@: -1}"
  mkdir -p "$dest/.git"
fi
exit 0
"#,
        )
        .unwrap();
        let mut perms = std::fs::metadata(&fake_git).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&fake_git, perms).unwrap();
        fake_git
    }

    #[cfg(windows)]
    fn write_fake_git(root: &std::path::Path) -> std::path::PathBuf {
        let fake_git = root.join("fake-git.cmd");
        std::fs::write(&fake_git, "@echo off\r\nexit /B 0\r\n").unwrap();
        fake_git
    }
}
