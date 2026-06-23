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
    domain::models::{TaskKind, TaskPhase, TaskRecord, TaskStatus},
    services,
    support::config::AppConfig,
};

/// Run the standalone MCP server over stdio. Logs go to stderr (the protocol
/// owns stdout). Entry: `logagent-server mcp-serve`.
pub async fn run_stdio(config: Arc<AppConfig>) -> anyhow::Result<()> {
    if !config.mcp.enabled {
        anyhow::bail!("MCP is disabled by configuration");
    }
    let state = AppState::new(config)?;
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
        "resources/list" => resources_list(state).await,
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
                "inputSchema": descriptor.params_schema,
            })
        })
        .collect();
    json!({ "tools": tools })
}

async fn call_tool(state: &Arc<AppState>, name: &str, arguments: Value) -> Value {
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
    let params = arguments
        .get("params")
        .cloned()
        .unwrap_or_else(|| json!({}));
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
    let params = arguments
        .get("params")
        .cloned()
        .unwrap_or_else(|| json!({}));
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

async fn resources_list(state: &Arc<AppState>) -> anyhow::Result<Value> {
    let mut resources = vec![
        resource("logagent://skills", "skills", "Indexed diagnostic skills."),
        resource(
            "logagent://metadata/instances",
            "metadata_instances",
            "Imported metadata instance summaries.",
        ),
        resource(
            "logagent://cases/recent",
            "cases_recent",
            "Recent enabled memory cases.",
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
    for skill in state.skills.list() {
        resources.push(resource(
            format!("logagent://skills/{}", skill.skill_id),
            format!("skill_{}", skill.skill_id),
            format!("Diagnostic skill {}", skill.display_name),
        ));
    }
    for instance in state.metadata.list_instances().await {
        resources.push(resource(
            format!(
                "logagent://metadata/instances/{}/snapshot",
                instance.instance_id
            ),
            format!("metadata_snapshot_{}", instance.instance_id),
            format!("Metadata snapshot for instance {}", instance.instance_id),
        ));
    }
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
        "logagent://skills" => json!({ "schemaVersion": 1, "skills": state.skills.list() }),
        "logagent://metadata/instances" => {
            json!({ "schemaVersion": 1, "instances": state.metadata.list_instances().await })
        }
        "logagent://cases/recent" => {
            json!({ "schemaVersion": 1, "cases": state.cases.search(None, 20, false).await })
        }
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
        _ if uri.starts_with("logagent://skills/") => {
            let skill_id = uri.trim_start_matches("logagent://skills/");
            let skill = state
                .skills
                .get(skill_id)
                .ok_or_else(|| anyhow::anyhow!("unknown skillId {skill_id}"))?;
            serde_json::to_value(skill)?
        }
        _ if uri.starts_with("logagent://metadata/instances/") && uri.ends_with("/snapshot") => {
            let instance_id = uri
                .strip_prefix("logagent://metadata/instances/")
                .and_then(|value| value.strip_suffix("/snapshot"))
                .ok_or_else(|| anyhow::anyhow!("invalid metadata snapshot URI"))?;
            serde_json::to_value(state.metadata.get_instance_snapshot(instance_id).await?)?
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
            AppConfig, AuthSettings, LogAnalyzerSettings, McpSettings, ServerSettings,
            SkillSettings, StorageSettings, ToolsSettings,
        },
    };

    fn request(id: i64, method: &str, params: Value) -> Value {
        json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
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
                    "name": "logagent.list_metadata_instances",
                    "arguments": { "runMode": "queued" }
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
        for _ in 0..200 {
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
        assert!(names.contains(&"logagent.list_metadata_instances"));

        // tools/call a runnable built-in that needs no uploads.
        let called = handle_request(
            &state,
            &request(
                3,
                "tools/call",
                json!({ "name": "logagent.list_metadata_instances", "arguments": {} }),
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

        // resources/list exposes the base context URIs.
        let resources = handle_request(&state, &request(5, "resources/list", json!({}))).await;
        let uris: Vec<&str> = resources["result"]["resources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["uri"].as_str().unwrap())
            .collect();
        for expected in [
            "logagent://skills",
            "logagent://metadata/instances",
            "logagent://cases/recent",
            "logagent://runs/recent",
            "logagent://tools/catalog",
        ] {
            assert!(uris.contains(&expected), "missing resource {expected}");
        }

        // Batch over HTTP.
        let batch = handle_http(
            &state,
            json!([
                request(6, "ping", json!({})),
                request(7, "prompts/list", json!({}))
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
            fetch: crate::support::config::FetchSettings::default(),
            huawei_cloud: crate::support::config::HuaweiCloudSettings::default(),
            remote_execution: crate::support::config::RemoteExecutionSettings::default(),
            mcp: McpSettings {
                enabled: mcp_enabled,
                allowed_origins,
            },
        });
        config.prepare_dirs().unwrap();
        (AppState::new(config).unwrap(), root)
    }
}
