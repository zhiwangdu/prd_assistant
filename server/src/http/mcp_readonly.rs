use std::sync::Arc;

use axum::{body::Bytes, extract::State, Json};
use serde_json::{json, Value};
use tracing::{info, warn};

use crate::{
    app::AppState,
    domain::models::TaskKind,
    http::{skills::normalize_skill_ids, system_context::metadata_context_bundle_item},
    services::{
        metadata::{MetadataFieldTypesRequest, MetadataTagFieldsRequest},
        skill_registry::{ResolveSkillsInput, SkillPreviewRequest},
    },
    stores::system_context_store::{render_system_context_prompt, system_context_bundle},
    support::error::AppError,
};

pub async fn readonly_mcp(State(state): State<Arc<AppState>>, body: Bytes) -> Json<Value> {
    let value: Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(err) => {
            return Json(json_rpc_error(None, -32700, format!("parse error: {err}")));
        }
    };
    if let Some(items) = value.as_array() {
        let mut responses = Vec::with_capacity(items.len());
        for item in items {
            responses.push(handle_one(state.clone(), item.clone()).await);
        }
        Json(Value::Array(responses))
    } else {
        Json(handle_one(state, value).await)
    }
}

async fn handle_one(state: Arc<AppState>, request: Value) -> Value {
    let id = request.get("id").cloned();
    let Some(method) = request.get("method").and_then(Value::as_str) else {
        return json_rpc_error(id, -32600, "JSON-RPC method is required");
    };
    let params = request.get("params").cloned().unwrap_or_else(|| json!({}));
    let result = match method {
        "initialize" => Ok(initialize_result()),
        "ping" => Ok(json!({})),
        "resources/list" => resources_list_result(&state).await,
        "resources/read" => {
            if let Some(uri) = params.get("uri").and_then(Value::as_str) {
                read_resource_result(&state, uri).await
            } else {
                Err(anyhow::anyhow!("resources/read requires params.uri"))
            }
        }
        "tools/list" => Ok(tools_list_result()),
        "tools/call" => {
            if let Some(name) = params.get("name").and_then(Value::as_str) {
                let arguments = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                call_tool(&state, name, arguments).await
            } else {
                Err(anyhow::anyhow!("tools/call requires params.name"))
            }
        }
        "prompts/list" => Ok(json!({ "prompts": [] })),
        _ => Err(anyhow::anyhow!("unsupported read-only MCP method {method}")),
    };
    match result {
        Ok(result) => {
            info!(method, "read-only MCP request succeeded");
            json!({ "jsonrpc": "2.0", "id": id, "result": result })
        }
        Err(err) => {
            warn!(method, error = %err, "read-only MCP request failed");
            json_rpc_error(id, -32000, format!("{err:#}"))
        }
    }
}

fn json_rpc_error(id: Option<Value>, code: i64, message: impl Into<String>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message.into()
        }
    })
}

fn initialize_result() -> Value {
    json!({
        "protocolVersion": "2025-06-18",
        "capabilities": {
            "resources": {},
            "tools": {}
        },
        "serverInfo": {
            "name": "logagent-readonly",
            "version": env!("CARGO_PKG_VERSION")
        }
    })
}

async fn resources_list_result(state: &AppState) -> anyhow::Result<Value> {
    let mut resources = vec![
        resource(
            "logagent://skills",
            "skills",
            "Indexed LogAgent diagnostic skills.",
        ),
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
            "logagent://tools/catalog",
            "tools_catalog",
            "Configured read-only tool catalog.",
        ),
        resource(
            "logagent://domain-adapters",
            "domain_adapters",
            "Built-in domain adapter summaries.",
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

async fn read_resource_result(state: &AppState, uri: &str) -> anyhow::Result<Value> {
    let value = match uri {
        "logagent://skills" => json!({
            "schemaVersion": 1,
            "skills": state.skills.list()
        }),
        "logagent://metadata/instances" => json!({
            "schemaVersion": 1,
            "instances": state.metadata.list_instances().await
        }),
        "logagent://cases/recent" => json!({
            "schemaVersion": 1,
            "cases": state.cases.search(None, 20, false).await
        }),
        "logagent://tools/catalog" => tool_catalog(state),
        "logagent://domain-adapters" => json!({
            "schemaVersion": 1,
            "domainAdapters": state.domain_adapters.summaries()
        }),
        _ if uri.starts_with("logagent://skills/") => {
            let skill_id = uri.trim_start_matches("logagent://skills/");
            let skill = state
                .skills
                .get(skill_id)
                .ok_or_else(|| anyhow::anyhow!("unknown skillId {skill_id}"))?;
            serde_json::to_value(skill)?
        }
        _ if uri.starts_with("logagent://metadata/instances/") && uri.ends_with("/snapshot") => {
            let prefix = "logagent://metadata/instances/";
            let instance_id = uri
                .strip_prefix(prefix)
                .and_then(|value| value.strip_suffix("/snapshot"))
                .ok_or_else(|| anyhow::anyhow!("invalid metadata snapshot URI"))?;
            serde_json::to_value(state.metadata.get_instance_snapshot(instance_id).await?)?
        }
        _ => anyhow::bail!("unknown read-only MCP resource {uri}"),
    };
    Ok(json!({
        "contents": [{
            "uri": uri,
            "mimeType": "application/json",
            "text": serde_json::to_string_pretty(&value)?
        }]
    }))
}

fn tools_list_result() -> Value {
    json!({
        "tools": [
            tool_schema("logagent.search_cases", "Search enabled LogAgent memory cases.", json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 50 },
                    "includeDisabled": { "type": "boolean" }
                }
            })),
            tool_schema("logagent.get_case", "Read one LogAgent memory case.", json!({
                "type": "object",
                "properties": { "caseId": { "type": "string" } },
                "required": ["caseId"]
            })),
            tool_schema("logagent.list_skills", "List indexed diagnostic skills.", json!({
                "type": "object",
                "properties": {}
            })),
            tool_schema("logagent.get_skill", "Read one diagnostic skill summary and SKILL.md injection content.", json!({
                "type": "object",
                "properties": { "skillId": { "type": "string" } },
                "required": ["skillId"]
            })),
            tool_schema("logagent.get_skill_reference", "Read one declared skill reference without writing server artifacts.", json!({
                "type": "object",
                "properties": {
                    "skillId": { "type": "string" },
                    "referenceId": { "type": "string" },
                    "path": { "type": "string" }
                },
                "required": ["skillId"]
            })),
            tool_schema("logagent.preview_system_context", "Preview selected skills and metadata adapter background.", json!({
                "type": "object",
                "properties": {
                    "skillIds": { "type": "array", "items": { "type": "string" } },
                    "product": { "type": "string" },
                    "version": { "type": "string" },
                    "environment": { "type": "string" },
                    "instanceId": { "type": "string" }
                }
            })),
            tool_schema("logagent.list_metadata_instances", "List imported metadata instances.", json!({
                "type": "object",
                "properties": {}
            })),
            tool_schema("logagent.get_metadata_snapshot", "Read one imported metadata snapshot.", json!({
                "type": "object",
                "properties": { "instanceId": { "type": "string" } },
                "required": ["instanceId"]
            })),
            tool_schema("logagent.get_metadata_field_types", "Look up field type metadata for one imported instance/database/measurement. Omit retentionPolicy to use the database default and omit field to return all fields.", json!({
                "type": "object",
                "properties": {
                    "instanceId": { "type": "string" },
                    "database": { "type": "string" },
                    "measurement": { "type": "string" },
                    "retentionPolicy": { "type": "string" },
                    "field": {
                        "oneOf": [
                            { "type": "string" },
                            {
                                "type": "array",
                                "items": { "type": "string" },
                                "minItems": 1
                            }
                        ]
                    }
                },
                "required": ["instanceId", "database", "measurement"]
            })),
            tool_schema("logagent.get_metadata_tag_fields", "List Tag type fields for one imported instance/database/measurement. Omit retentionPolicy to use the database default.", json!({
                "type": "object",
                "properties": {
                    "instanceId": { "type": "string" },
                    "database": { "type": "string" },
                    "measurement": { "type": "string" },
                    "retentionPolicy": { "type": "string" }
                },
                "required": ["instanceId", "database", "measurement"]
            })),
            tool_schema("logagent.list_tools", "List configured tool catalog metadata.", json!({
                "type": "object",
                "properties": {}
            })),
            tool_schema("logagent.list_domain_adapters", "List built-in domain adapters.", json!({
                "type": "object",
                "properties": {}
            }))
        ]
    })
}

fn tool_schema(name: &str, description: &str, input_schema: Value) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema
    })
}

async fn call_tool(state: &AppState, name: &str, arguments: Value) -> anyhow::Result<Value> {
    let result = match name {
        "logagent.search_cases" => {
            let query = optional_string(&arguments, "query");
            let limit = optional_usize(&arguments, "limit")
                .unwrap_or(5)
                .clamp(1, 50);
            let include_disabled = arguments
                .get("includeDisabled")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            json!({
                "cases": state
                    .cases
                    .search(query.as_deref(), limit, include_disabled)
                    .await
            })
        }
        "logagent.get_case" => {
            let case_id = required_string(&arguments, "caseId")?;
            let case = state
                .cases
                .get(&case_id)
                .await
                .ok_or_else(|| anyhow::anyhow!("unknown caseId {case_id}"))?;
            json!({ "case": case })
        }
        "logagent.list_skills" => json!({ "skills": state.skills.list() }),
        "logagent.get_skill" => {
            let skill_id = required_string(&arguments, "skillId")?;
            let skill = state
                .skills
                .get(&skill_id)
                .ok_or_else(|| anyhow::anyhow!("unknown skillId {skill_id}"))?;
            json!({ "skill": skill })
        }
        "logagent.get_skill_reference" => {
            let skill_id = required_string(&arguments, "skillId")?;
            let reference_id = optional_string(&arguments, "referenceId");
            let reference_path = optional_string(&arguments, "path");
            let reference = state
                .skills
                .read_reference(
                    &skill_id,
                    reference_id.as_deref(),
                    reference_path.as_deref(),
                )
                .await?;
            json!({
                "skillId": reference.skill_id,
                "skillRevision": reference.skill_revision,
                "reference": reference.reference,
                "content": reference.content,
                "truncated": reference.truncated,
                "finalEvidenceAllowed": false
            })
        }
        "logagent.preview_system_context" => {
            let req = serde_json::from_value::<SkillPreviewRequest>(arguments)?;
            preview_system_context(state, req).await?
        }
        "logagent.list_metadata_instances" => {
            json!({ "instances": state.metadata.list_instances().await })
        }
        "logagent.get_metadata_snapshot" => {
            let instance_id = required_string(&arguments, "instanceId")?;
            json!({ "snapshot": state.metadata.get_instance_snapshot(&instance_id).await? })
        }
        "logagent.get_metadata_field_types" => {
            let request = serde_json::from_value::<MetadataFieldTypesRequest>(arguments)?;
            json!({ "result": state.metadata.get_metadata_field_types(request).await? })
        }
        "logagent.get_metadata_tag_fields" => {
            let request = serde_json::from_value::<MetadataTagFieldsRequest>(arguments)?;
            json!({ "result": state.metadata.get_metadata_tag_fields(request).await? })
        }
        "logagent.list_tools" => tool_catalog(state),
        "logagent.list_domain_adapters" => {
            json!({ "domainAdapters": state.domain_adapters.summaries() })
        }
        other => anyhow::bail!("unknown or unsupported read-only MCP tool {other}"),
    };
    Ok(json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&result)?
        }],
        "isError": false
    }))
}

async fn preview_system_context(
    state: &AppState,
    req: SkillPreviewRequest,
) -> Result<Value, AppError> {
    let metadata_context = if let Some(instance_id) = req
        .instance_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(
            state
                .metadata
                .resolve_task_context(Some(instance_id.to_string()), None, None)
                .await?,
        )
    } else {
        None
    };
    let product = metadata_context
        .as_ref()
        .and_then(|context| context.product.as_deref())
        .or(req.product.as_deref());
    let version = metadata_context
        .as_ref()
        .and_then(|context| context.version.as_deref())
        .or(req.version.as_deref());
    let environment = metadata_context
        .as_ref()
        .and_then(|context| context.environment.as_deref())
        .or(req.environment.as_deref());
    let explicit_skill_ids = normalize_skill_ids(req.skill_ids)?;
    let mut resources = state.skills.resolve_items(ResolveSkillsInput {
        explicit_skill_ids: &explicit_skill_ids,
        task_kind: TaskKind::LogAnalysis,
        product,
        version,
        environment,
    })?;
    if let Some(metadata_context) = metadata_context.as_ref() {
        if metadata_context.instance_id.is_some() {
            resources.push(metadata_context_bundle_item(metadata_context));
        }
    }
    let bundle = system_context_bundle(resources.clone());
    Ok(json!({
        "resources": resources,
        "prompt": render_system_context_prompt(&bundle)
    }))
}

fn tool_catalog(state: &AppState) -> Value {
    let configured = state
        .config
        .tools
        .tools
        .values()
        .map(|tool| {
            json!({
                "toolId": tool.name,
                "enabled": tool.enabled,
                "timeoutSeconds": tool.timeout_seconds,
                "maxInputFiles": tool.max_input_files,
                "configuredArgs": tool.args,
                "match": {
                    "filePatterns": tool.match_settings.file_patterns,
                    "keywords": tool.match_settings.keywords
                }
            })
        })
        .collect::<Vec<_>>();
    json!({
        "schemaVersion": 1,
        "tools": crate::services::tools::descriptors(&state.config),
        "configuredTools": configured
    })
}

fn required_string(arguments: &Value, key: &str) -> anyhow::Result<String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| anyhow::anyhow!("{key} is required"))
}

fn optional_string(arguments: &Value, key: &str) -> Option<String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn optional_usize(arguments: &Value, key: &str) -> Option<usize> {
    arguments
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use chrono::Utc;
    use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
    use tower::ServiceExt;

    use crate::{
        http,
        services::metadata::MetadataImportRequest,
        stores::case_store::ManualCase,
        support::config::{
            AnalysisSettings, AppConfig, AuthSettings, ClaudeCodeSettings, EmbeddingSettings,
            LlmProvider, LlmSettings, LogAnalyzerSettings, McpSettings, ServerSettings,
            SkillSettings, StorageSettings, ToolMatchSettings, ToolSettings, ToolsSettings,
        },
    };

    #[tokio::test]
    async fn readonly_mcp_exposes_resources_and_tools_without_task_access() {
        let (state, root) = test_state().await;
        let app = http::router(state.clone()).with_state(state);

        let initialized = post_mcp(
            &app,
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize"
            }),
        )
        .await;
        assert_eq!(
            initialized["result"]["serverInfo"]["name"],
            "logagent-readonly"
        );

        let tools_list = post_mcp(
            &app,
            json!({
                "jsonrpc": "2.0",
                "id": 11,
                "method": "tools/list"
            }),
        )
        .await;
        let tools = tools_list["result"]["tools"].as_array().unwrap();
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == "logagent.get_metadata_tag_fields"));

        let listed = post_mcp(
            &app,
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "resources/list"
            }),
        )
        .await;
        let resources = listed["result"]["resources"].as_array().unwrap();
        assert!(resources
            .iter()
            .any(|resource| resource["uri"] == "logagent://skills/opengemini-diagnosis"));
        assert!(resources
            .iter()
            .any(|resource| resource["uri"] == "logagent://metadata/instances/inst-1/snapshot"));

        let skill = post_mcp(
            &app,
            json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "resources/read",
                "params": { "uri": "logagent://skills/opengemini-diagnosis" }
            }),
        )
        .await;
        let text = skill["result"]["contents"][0]["text"].as_str().unwrap();
        assert!(text.contains("Use current evidence first."));

        let reference = post_mcp(
            &app,
            json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "tools/call",
                "params": {
                    "name": "logagent.get_skill_reference",
                    "arguments": {
                        "skillId": "opengemini-diagnosis",
                        "path": "references/topology.md"
                    }
                }
            }),
        )
        .await;
        let text = reference["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("Topology reference content."));
        assert!(text.contains("\"finalEvidenceAllowed\": false"));

        let cases = post_mcp(
            &app,
            json!({
                "jsonrpc": "2.0",
                "id": 5,
                "method": "tools/call",
                "params": {
                    "name": "logagent.search_cases",
                    "arguments": { "query": "time filter", "limit": 5 }
                }
            }),
        )
        .await;
        let text = cases["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("No time filter"));

        let metadata = post_mcp(
            &app,
            json!({
                "jsonrpc": "2.0",
                "id": 6,
                "method": "tools/call",
                "params": {
                    "name": "logagent.get_metadata_snapshot",
                    "arguments": { "instanceId": "inst-1" }
                }
            }),
        )
        .await;
        let text = metadata["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("cluster-1"));

        let field_types = post_mcp(
            &app,
            json!({
                "jsonrpc": "2.0",
                "id": 7,
                "method": "tools/call",
                "params": {
                    "name": "logagent.get_metadata_field_types",
                    "arguments": {
                        "instanceId": "inst-1",
                        "database": "mydb",
                        "measurement": "cpu",
                        "field": "usage"
                    }
                }
            }),
        )
        .await;
        let text = field_types["result"]["content"][0]["text"]
            .as_str()
            .unwrap();
        assert!(text.contains("\"typeLabel\": \"Float\""));

        let tag_fields = post_mcp(
            &app,
            json!({
                "jsonrpc": "2.0",
                "id": 12,
                "method": "tools/call",
                "params": {
                    "name": "logagent.get_metadata_tag_fields",
                    "arguments": {
                        "instanceId": "inst-1",
                        "database": "mydb",
                        "measurement": "cpu"
                    }
                }
            }),
        )
        .await;
        let text = tag_fields["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"name\": \"host\""));
        assert!(text.contains("\"typeLabel\": \"Tag\""));
        assert!(!text.contains("\"name\": \"usage\""));

        let catalog = post_mcp(
            &app,
            json!({
                "jsonrpc": "2.0",
                "id": 8,
                "method": "tools/call",
                "params": {
                    "name": "logagent.list_tools",
                    "arguments": {}
                }
            }),
        )
        .await;
        let text = catalog["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"toolId\": \"fake_tool\""));
        assert!(text.contains("\"source\": \"configured\""));
        assert!(text.contains("\"toolId\": \"logagent.get_metadata_field_types\""));
        assert!(text.contains("\"toolId\": \"logagent.get_metadata_tag_fields\""));
        assert!(text.contains("\"toolId\": \"logagent.fetch\""));
        assert!(text.contains("\"source\": \"built_in\""));
        assert!(text.contains("\"exportable\": false"));

        let rejected = post_mcp(
            &app,
            json!({
                "jsonrpc": "2.0",
                "id": 9,
                "method": "tools/call",
                "params": {
                    "name": "logagent.run_domain_tool",
                    "arguments": { "tool": "fake_tool" }
                }
            }),
        )
        .await;
        assert!(rejected["error"]["message"]
            .as_str()
            .unwrap()
            .contains("unknown or unsupported"));

        let rejected_fetch = post_mcp(
            &app,
            json!({
                "jsonrpc": "2.0",
                "id": 13,
                "method": "tools/call",
                "params": {
                    "name": "logagent.fetch",
                    "arguments": { "fetchId": "fetch_123" }
                }
            }),
        )
        .await;
        assert!(rejected_fetch["error"]["message"]
            .as_str()
            .unwrap()
            .contains("unknown or unsupported"));

        let bad_ref = post_mcp(
            &app,
            json!({
                "jsonrpc": "2.0",
                "id": 10,
                "method": "tools/call",
                "params": {
                    "name": "logagent.get_skill_reference",
                    "arguments": {
                        "skillId": "opengemini-diagnosis",
                        "path": "../secret.md"
                    }
                }
            }),
        )
        .await;
        assert!(bad_ref["error"]["message"]
            .as_str()
            .unwrap()
            .contains("workspace-relative without traversal"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn readonly_mcp_requires_api_key() {
        let (state, root) = test_state().await;
        let app = http::router(state.clone()).with_state(state);
        let response = app
            .oneshot(
                Request::post("/api/mcp/readonly")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let _ = std::fs::remove_dir_all(root);
    }

    async fn post_mcp(app: &axum::Router, request: Value) -> Value {
        let response = app
            .clone()
            .oneshot(
                Request::post("/api/mcp/readonly")
                    .header("authorization", "Bearer test-key")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    async fn test_state() -> (Arc<AppState>, PathBuf) {
        let root = temp_root("readonly-mcp");
        write_skill(&root);
        let tool_path = write_executable(&root);
        let mut tools = BTreeMap::new();
        tools.insert(
            "fake_tool".to_string(),
            ToolSettings {
                name: "fake_tool".to_string(),
                enabled: true,
                path: tool_path,
                timeout_seconds: 5,
                max_output_bytes: 1024 * 1024,
                max_input_files: 1,
                args: vec!["--json".to_string()],
                match_settings: ToolMatchSettings {
                    file_patterns: vec!["*.log".to_string()],
                    keywords: vec!["error".to_string()],
                },
            },
        );
        let config = Arc::new(AppConfig {
            config_path: root.join("logagent-test.yaml"),
            server: ServerSettings {
                bind: "127.0.0.1:0".to_string(),
                public_base_url: "http://127.0.0.1:0".to_string(),
                max_concurrent_tasks: 1,
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
                enabled: true,
                roots: vec![root.join("skills")],
                max_skill_chars: 4000,
                max_reference_chars: 20_000,
            },
            log_analyzer: LogAnalyzerSettings {
                keywords: vec!["error".to_string()],
                max_matches: 20,
            },
            tools: ToolsSettings { tools },
            fetch: crate::support::config::FetchSettings::default(),
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
        let state = AppState::new(config).unwrap();
        let preview = state
            .metadata
            .create_import_preview(MetadataImportRequest {
                template_type: "json".to_string(),
                filename: Some("metadata.json".to_string()),
                instance_id: None,
                remark: None,
                content: serde_json::json!({
                    "instances": [{
                        "instanceId": "inst-1",
                        "clusterId": "cluster-1",
                        "product": "opengemini",
                        "version": "1.0",
                        "environment": "test"
                    }],
                    "clusters": [{
                        "clusterId": "cluster-1",
                        "name": "cluster-1",
                        "product": "opengemini",
                        "version": "1.0",
                        "environment": "test",
                        "nodes": ["node-1"],
                        "databases": [{
                            "name": "mydb",
                            "defaultRetentionPolicy": "autogen",
                            "retentionPolicies": [{
                                "name": "autogen",
                                "measurements": [{
                                    "name": "cpu_0000",
                                    "logicalName": "cpu",
                                    "schema": [
                                        { "name": "host", "typ": 6 },
                                        { "name": "usage", "typ": 3 }
                                    ]
                                }]
                            }]
                        }]
                    }],
                    "nodes": [{
                        "nodeId": "node-1",
                        "instanceId": "inst-1",
                        "host": "127.0.0.1",
                        "role": "data"
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
        state
            .cases
            .create_manual(ManualCase {
                case_id: "case_readonly".to_string(),
                product: Some("opengemini".to_string()),
                version: Some("1.0".to_string()),
                environment: Some("test".to_string()),
                instance_id: Some("inst-1".to_string()),
                node_id: Some("node-1".to_string()),
                title: "No time filter".to_string(),
                symptom: "Slow query".to_string(),
                root_cause: "Query has no time filter".to_string(),
                solution: "Add bounded time predicate".to_string(),
                evidence_refs: vec!["INC-1".to_string()],
                enabled: true,
            })
            .await
            .unwrap();
        (state, root)
    }

    fn temp_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "logagent-{name}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ))
    }

    fn write_skill(root: &PathBuf) {
        let skill = root.join("skills/opengemini-diagnosis");
        std::fs::create_dir_all(skill.join("references")).unwrap();
        std::fs::write(
            skill.join("SKILL.md"),
            "---\nname: openGemini Diagnosis\ndescription: Diagnose openGemini.\n---\nUse current evidence first.\n",
        )
        .unwrap();
        std::fs::write(
            skill.join("references/topology.md"),
            "Topology reference content.",
        )
        .unwrap();
        std::fs::write(
            skill.join("logagent.json"),
            r#"{"schemaVersion":1,"skillId":"opengemini-diagnosis","displayName":"openGemini diagnosis","products":["opengemini"],"taskKinds":["log_analysis"],"includeByDefault":true,"references":[{"path":"references/topology.md","title":"Topology","summary":"Topology rules"}]}"#,
        )
        .unwrap();
    }

    fn write_executable(root: &PathBuf) -> PathBuf {
        let path = root.join("bin/fake-tool");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "#!/usr/bin/env sh\nprintf '{}\\n'\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mut permissions = std::fs::metadata(&path).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&path, permissions).unwrap();
        }
        path
    }
}
