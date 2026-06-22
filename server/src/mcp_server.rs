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

use axum::{body::Bytes, extract::State, Json};
use serde_json::{json, Value};
use tracing::{info, warn};

use crate::{app::AppState, domain::models::TaskPhase, services, support::config::AppConfig};

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

/// HTTP JSON-RPC entry (`POST /api/mcp`). Accepts a single request or a batch.
pub async fn http_mcp(State(state): State<Arc<AppState>>, body: Bytes) -> Json<Value> {
    let value: Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(err) => return Json(json_rpc_error(None, -32700, format!("parse error: {err}"))),
    };
    Json(handle_http(&state, value).await)
}

/// Handle a single request or a batch array. Reused by stdio and HTTP.
pub async fn handle_http(state: &Arc<AppState>, body: Value) -> Value {
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
        "serverInfo": { "name": "logagent-mcp", "version": env!("CARGO_PKG_VERSION") }
    })
}

fn tools_list(state: &Arc<AppState>) -> Value {
    let tools: Vec<Value> = services::tools::descriptors(&state.config)
        .into_iter()
        .filter(|descriptor| descriptor.runnable)
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
    match run_catalog_tool(state, name, arguments).await {
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

    use crate::{
        domain::models::TaskKind,
        support::config::{
            AnalysisSettings, AppConfig, AuthSettings, ClaudeCodeSettings, EmbeddingSettings,
            LlmProvider, LlmSettings, LogAnalyzerSettings, McpSettings, ServerSettings,
            SkillSettings, StorageSettings, ToolsSettings,
        },
    };

    fn request(id: i64, method: &str, params: Value) -> Value {
        json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
    }

    #[tokio::test]
    async fn mcp_server_lists_runs_and_reads_resources() {
        let (state, root) = test_state("mcp-server");

        let initialized = handle_request(&state, &request(1, "initialize", json!({}))).await;
        assert_eq!(initialized["result"]["serverInfo"]["name"], "logagent-mcp");

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

    fn test_state(prefix: &str) -> (Arc<AppState>, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "logagent-{prefix}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
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
            tools: ToolsSettings {
                tools: BTreeMap::new(),
            },
            fetch: crate::support::config::FetchSettings::default(),
            huawei_cloud: crate::support::config::HuaweiCloudSettings::default(),
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
}
