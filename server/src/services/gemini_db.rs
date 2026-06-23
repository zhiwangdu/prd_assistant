//! GeminiDB Influx (HuaweiCloud NoSQL) instance-management tools.
//!
//! A self-contained group of six built-in tools that drive the GeminiDB Influx
//! instance lifecycle API (`POST/GET/PUT/DELETE /v3/{project_id}/instances...`).
//! Like the other catalog tools they are exposed via `services::tools::descriptors`
//! and run through the shared `build_tool_run_task` + `run_tool_task` boundary, so
//! they auto-appear in `/api/tools`, MCP `tools/list`, and the WebUI catalog.
//!
//! - Auth: `X-Auth-Token` header, resolved from env only (`huawei_cloud.gemini_db.auth_token_env`).
//! - Endpoint: `huawei_cloud.gemini_db.endpoint` + `project_id` are config defaults;
//!   each run may override them via `endpoint` / `projectId` params (dynamic config).
//! - Bodies: create / SSL / restart forward the caller-supplied `body` verbatim
//!   (the tool owns method + path + auth + endpoint, not field names).

use std::{path::PathBuf, sync::Arc, time::Instant};

use anyhow::Context;
use chrono::Utc;
use reqwest::{header::HeaderValue, Method};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::time::Duration;
use tracing::{info, warn};

use crate::{
    app::AppState,
    domain::models::{TaskRecord, ToolDescriptor, ToolSource},
    support::{
        config::{AppConfig, GeminiDbSettings},
        error::AppError,
        fs_utils::{relative_string, write_json_atomic},
    },
};

pub const CREATE_INSTANCE_ID: &str = "logagent.geminidb.create_instance";
pub const DELETE_INSTANCE_ID: &str = "logagent.geminidb.delete_instance";
pub const LIST_INSTANCES_ID: &str = "logagent.geminidb.list_instances";
pub const RENAME_INSTANCE_ID: &str = "logagent.geminidb.rename_instance";
pub const TOGGLE_SSL_ID: &str = "logagent.geminidb.toggle_ssl";
pub const RESTART_INSTANCE_ID: &str = "logagent.geminidb.restart_instance";

const MAX_RESPONSE_CHARS: usize = 65_536;

/// All GeminiDB tool params share this shape; fields are interpreted per tool.
/// `endpoint` / `projectId` override the config defaults; `name` is the new
/// instance name (rename) or the name filter (list) depending on the tool.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiDbParams {
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub instance_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub body: Option<Value>,
    // List filters (all optional).
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub datastore_type: Option<String>,
    #[serde(default)]
    pub vpc_id: Option<String>,
    #[serde(default)]
    pub subnet_id: Option<String>,
    #[serde(default)]
    pub offset: Option<u32>,
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone)]
struct GeminiDbPlan {
    tool_id: String,
    method: Method,
    path: String,
    query: Vec<(String, String)>,
    body: Option<String>,
    stored_body: Value,
    summary_label: &'static str,
}

#[derive(Debug, Clone)]
struct GeminiDbEndpointMeta {
    base_url: String,
    project_id: String,
    region: String,
    auth_token_env: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiDbHttpResponse {
    status_code: u16,
    body: String,
    truncated: bool,
}

#[derive(Debug, Clone)]
struct GeminiDbHttpRequest {
    method: Method,
    url: String,
    query: Vec<(String, String)>,
    body: Option<String>,
}

#[allow(async_fn_in_trait)]
trait GeminiDbHttpClient {
    async fn send(&self, request: GeminiDbHttpRequest) -> anyhow::Result<GeminiDbHttpResponse>;
}

pub fn descriptors(config: &AppConfig) -> Vec<ToolDescriptor> {
    let enabled = config.huawei_cloud.gemini_db.enabled;
    vec![
        create_instance_descriptor(enabled),
        delete_instance_descriptor(enabled),
        list_instances_descriptor(enabled),
        rename_instance_descriptor(enabled),
        toggle_ssl_descriptor(enabled),
        restart_instance_descriptor(enabled),
    ]
}

pub fn get_descriptor(config: &AppConfig, tool_id: &str) -> Option<ToolDescriptor> {
    descriptors(config)
        .into_iter()
        .find(|d| d.tool_id == tool_id)
}

pub fn is_gemini_db_tool(tool_id: &str) -> bool {
    matches!(
        tool_id,
        CREATE_INSTANCE_ID
            | DELETE_INSTANCE_ID
            | LIST_INSTANCES_ID
            | RENAME_INSTANCE_ID
            | TOGGLE_SSL_ID
            | RESTART_INSTANCE_ID
    )
}

pub fn validate_run_params(
    config: &AppConfig,
    tool_id: &str,
    value: &Value,
) -> Result<Value, AppError> {
    if !config.huawei_cloud.gemini_db.enabled {
        return Err(AppError::bad_request(
            "GeminiDB Influx tools are disabled by server config",
        ));
    }
    let params = parse_params(value)?;
    match tool_id {
        CREATE_INSTANCE_ID => require_body_object(&params)?,
        DELETE_INSTANCE_ID => {
            require_instance_id(&params)?;
        }
        LIST_INSTANCES_ID => validate_list_params(&params)?,
        RENAME_INSTANCE_ID => {
            require_instance_id(&params)?;
            require_name(&params)?;
        }
        TOGGLE_SSL_ID => {
            require_instance_id(&params)?;
            require_body_object(&params)?;
        }
        RESTART_INSTANCE_ID => {
            require_instance_id(&params)?;
            if let Some(body) = params.body.as_ref() {
                if !body.is_object() {
                    return Err(AppError::bad_request(
                        "body must be a JSON object for restart",
                    ));
                }
            }
        }
        _ => return Err(AppError::not_found(format!("unknown toolId {tool_id}"))),
    }
    serde_json::to_value(params)
        .map_err(|err| AppError::internal(format!("failed to encode GeminiDB params: {err}")))
}

pub async fn run_gemini_db_task(
    state: Arc<AppState>,
    task: TaskRecord,
) -> Result<PathBuf, AppError> {
    let tool_id = task
        .tool_id
        .as_deref()
        .ok_or_else(|| AppError::bad_request("tool run task is missing toolId"))?;
    let settings = &state.config.huawei_cloud.gemini_db;
    if !settings.enabled {
        return Err(AppError::bad_request(
            "GeminiDB Influx tools are disabled by server config",
        ));
    }
    let params = parse_params(&task.tool_params)?;
    let endpoint = resolve_endpoint(&params, settings)?;
    let project_id = resolve_project_id(&params, settings)?;
    let auth_token = settings.auth_token.as_deref().ok_or_else(|| {
        AppError::internal("GeminiDB auth token is missing (enabled requires auth_token_env)")
    })?;
    let plan = build_plan(tool_id, &params, &project_id)?;
    let client = GeminiDbClient::new(auth_token, settings.timeout_seconds)?;
    let meta = GeminiDbEndpointMeta {
        base_url: endpoint,
        project_id,
        region: settings.region.clone(),
        auth_token_env: settings.auth_token_env.clone(),
    };
    let action_id = format!(
        "act_tool_gemini_db_{}_{}",
        safe_suffix(tool_id),
        task.task_id
    );
    let workspace = state.config.storage.workspace_dir(&task.task_id);
    execute_gemini_db_to_artifacts(&workspace, &action_id, plan, &meta, &client).await
}

#[allow(clippy::too_many_arguments)]
async fn execute_gemini_db_to_artifacts<C: GeminiDbHttpClient>(
    workspace: &std::path::Path,
    action_id: &str,
    plan: GeminiDbPlan,
    meta: &GeminiDbEndpointMeta,
    client: &C,
) -> Result<PathBuf, AppError> {
    let result_dir = workspace.join("tool_results").join(action_id);
    tokio::fs::create_dir_all(&result_dir)
        .await
        .map_err(|err| AppError::internal(format!("failed to create tool result dir: {err}")))?;
    let result_path = result_dir.join("result.json");
    let result_artifact_path = relative_string(workspace, &result_path)
        .map_err(|err| AppError::internal(err.to_string()))?;

    let url = format!("{}{}", meta.base_url, plan.path);
    let started = Instant::now();
    info!(
        action_id,
        tool_id = %plan.tool_id,
        method = %plan.method,
        path = %plan.path,
        "starting GeminiDB Influx tool"
    );
    let request = GeminiDbHttpRequest {
        method: plan.method.clone(),
        url: url.clone(),
        query: plan.query.clone(),
        body: plan.body.clone(),
    };
    let timeout_seconds = 60; // bounded by the reqwest client timeout set in GeminiDbClient
    let send_result = run_with_timeout(timeout_seconds, client.send(request)).await;
    let elapsed = started.elapsed().as_millis();

    let (status_code, response_body, truncated, error) = match send_result {
        Ok(response) => (
            response.status_code,
            response.body,
            response.truncated,
            None::<String>,
        ),
        Err(err) => (0, String::new(), false, Some(err.to_string())),
    };
    let http_ok = (200..=299).contains(&status_code);
    let status = if error.is_some() {
        "FAILED"
    } else if http_ok {
        "OK"
    } else {
        "FAILED"
    };
    let summary = match (status, error.as_ref()) {
        ("OK", _) => format!("{} succeeded (HTTP {})", plan.summary_label, status_code),
        (_, Some(err)) => format!("{} failed: {err}", plan.summary_label),
        (_, None) => format!("{} failed with HTTP {}", plan.summary_label, status_code),
    };
    let result = json!({
        "schemaVersion": 1,
        "toolId": plan.tool_id,
        "tool": plan.tool_id,
        "actionId": action_id,
        "status": status,
        "summary": summary,
        "error": error,
        "warnings": if truncated {
            vec![format!("response body truncated to first {MAX_RESPONSE_CHARS} chars")]
        } else {
            Vec::<String>::new()
        },
        "endpoint": {
            "baseUrl": meta.base_url,
            "projectId": meta.project_id,
            "region": meta.region,
        },
        "http": {
            "method": plan.method.as_str(),
            "path": plan.path,
            "url": url,
            "ok": http_ok,
            "statusCode": status_code,
        },
        "request": {
            "method": plan.method.as_str(),
            "path": plan.path,
            "body": plan.stored_body,
        },
        "response": {
            "statusCode": status_code,
            "body": response_body,
            "truncated": truncated,
        },
        "timings": { "totalMs": elapsed },
        "credentialMetadata": { "authTokenEnv": meta.auth_token_env },
        "evidenceRefs": [result_artifact_path],
        "createdAt": Utc::now(),
    });
    write_json_atomic(result_path.clone(), &result).await?;
    if status == "OK" {
        info!(
            action_id,
            status,
            duration_ms = elapsed,
            "GeminiDB Influx tool completed"
        );
    } else {
        warn!(
            action_id,
            status,
            duration_ms = elapsed,
            "GeminiDB Influx tool completed with failure"
        );
    }
    Ok(result_path)
}

fn build_plan(
    tool_id: &str,
    params: &GeminiDbParams,
    project_id: &str,
) -> Result<GeminiDbPlan, AppError> {
    let base = format!("/v3/{project_id}/instances");
    match tool_id {
        CREATE_INSTANCE_ID => {
            let body = params.body.clone().unwrap_or_else(|| json!({}));
            Ok(GeminiDbPlan {
                tool_id: tool_id.to_string(),
                method: Method::POST,
                path: base,
                query: Vec::new(),
                body: Some(body.to_string()),
                stored_body: redact_sensitive(&body),
                summary_label: "create GeminiDB Influx instance",
            })
        }
        DELETE_INSTANCE_ID => {
            let id = require_instance_id(params)?;
            Ok(GeminiDbPlan {
                tool_id: tool_id.to_string(),
                method: Method::DELETE,
                path: format!("{base}/{id}"),
                query: Vec::new(),
                body: None,
                stored_body: Value::Null,
                summary_label: "delete GeminiDB Influx instance",
            })
        }
        LIST_INSTANCES_ID => {
            let query = collect_list_query(params);
            Ok(GeminiDbPlan {
                tool_id: tool_id.to_string(),
                method: Method::GET,
                path: base,
                query,
                body: None,
                stored_body: Value::Null,
                summary_label: "list GeminiDB Influx instances",
            })
        }
        RENAME_INSTANCE_ID => {
            let id = require_instance_id(params)?;
            let name = require_name(params)?;
            let body = json!({ "name": name });
            Ok(GeminiDbPlan {
                tool_id: tool_id.to_string(),
                method: Method::PUT,
                path: format!("{base}/{id}/name"),
                query: Vec::new(),
                body: Some(body.to_string()),
                stored_body: redact_sensitive(&body),
                summary_label: "rename GeminiDB Influx instance",
            })
        }
        TOGGLE_SSL_ID => {
            let id = require_instance_id(params)?;
            let body = params.body.clone().unwrap_or_else(|| json!({}));
            Ok(GeminiDbPlan {
                tool_id: tool_id.to_string(),
                method: Method::PUT,
                path: format!("{base}/{id}/ssl"),
                query: Vec::new(),
                body: Some(body.to_string()),
                stored_body: redact_sensitive(&body),
                summary_label: "toggle GeminiDB Influx instance SSL",
            })
        }
        RESTART_INSTANCE_ID => {
            let id = require_instance_id(params)?;
            let body = params.body.clone().unwrap_or_else(|| json!({}));
            Ok(GeminiDbPlan {
                tool_id: tool_id.to_string(),
                method: Method::POST,
                path: format!("{base}/{id}/restart"),
                query: Vec::new(),
                body: Some(body.to_string()),
                stored_body: redact_sensitive(&body),
                summary_label: "restart GeminiDB Influx instance or node",
            })
        }
        _ => Err(AppError::bad_request(format!("unknown toolId {tool_id}"))),
    }
}

fn collect_list_query(params: &GeminiDbParams) -> Vec<(String, String)> {
    let mut query = Vec::new();
    let mut push = |key: &str, value: &Option<String>| {
        if let Some(value) = value
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            query.push((key.to_string(), value.to_string()));
        }
    };
    push("id", &params.id);
    push("name", &params.name);
    push("mode", &params.mode);
    push("datastore_type", &params.datastore_type);
    push("vpc_id", &params.vpc_id);
    push("subnet_id", &params.subnet_id);
    if let Some(offset) = params.offset {
        query.push(("offset".to_string(), offset.to_string()));
    }
    if let Some(limit) = params.limit {
        query.push(("limit".to_string(), limit.to_string()));
    }
    query
}

fn parse_params(value: &Value) -> Result<GeminiDbParams, AppError> {
    if value.is_null() {
        return Ok(GeminiDbParams::default());
    }
    serde_json::from_value(value.clone())
        .map_err(|err| AppError::bad_request(format!("invalid GeminiDB params: {err}")))
}

fn require_instance_id(params: &GeminiDbParams) -> Result<String, AppError> {
    let id = params
        .instance_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::bad_request("instanceId is required"))?;
    validate_instance_id(id)?;
    Ok(id.to_string())
}

fn require_name(params: &GeminiDbParams) -> Result<String, AppError> {
    let name = params
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::bad_request("name is required"))?;
    if name.len() > 128 {
        return Err(AppError::bad_request("name must be at most 128 characters"));
    }
    Ok(name.to_string())
}

fn require_body_object(params: &GeminiDbParams) -> Result<(), AppError> {
    match params.body.as_ref() {
        Some(Value::Object(_)) => Ok(()),
        Some(_) => Err(AppError::bad_request("body must be a JSON object")),
        None => Err(AppError::bad_request("body is required")),
    }
}

fn validate_list_params(params: &GeminiDbParams) -> Result<(), AppError> {
    // All filters are optional; just reject a non-null body if supplied.
    if let Some(body) = params.body.as_ref() {
        if !body.is_null() {
            return Err(AppError::bad_request("list does not accept a body"));
        }
    }
    Ok(())
}

fn validate_instance_id(value: &str) -> Result<(), AppError> {
    let valid = !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if valid {
        Ok(())
    } else {
        Err(AppError::bad_request(
            "instanceId must contain only letters, digits, '_' or '-'",
        ))
    }
}

fn resolve_endpoint(
    params: &GeminiDbParams,
    settings: &GeminiDbSettings,
) -> Result<String, AppError> {
    let override_value = params
        .endpoint
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let endpoint = match override_value {
        Some(value) => {
            validate_endpoint_url(value)?;
            value.trim_end_matches('/').to_string()
        }
        None => {
            if settings.endpoint.is_empty() {
                return Err(AppError::bad_request(
                    "GeminiDB endpoint is not configured; set huawei_cloud.gemini_db.endpoint or pass endpoint",
                ));
            }
            settings.endpoint.clone()
        }
    };
    Ok(endpoint)
}

fn resolve_project_id(
    params: &GeminiDbParams,
    settings: &GeminiDbSettings,
) -> Result<String, AppError> {
    let override_value = params
        .project_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let project_id = match override_value {
        Some(value) => {
            validate_project_id(value)?;
            value.to_string()
        }
        None => {
            if settings.project_id.is_empty() {
                return Err(AppError::bad_request(
                    "GeminiDB project id is not configured; set huawei_cloud.gemini_db.project_id(_env) or pass projectId",
                ));
            }
            settings.project_id.clone()
        }
    };
    Ok(project_id)
}

fn validate_endpoint_url(endpoint: &str) -> Result<(), AppError> {
    let parsed = reqwest::Url::parse(endpoint)
        .map_err(|err| AppError::bad_request(format!("invalid GeminiDB endpoint: {err}")))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(AppError::bad_request(
            "GeminiDB endpoint must use http or https",
        ));
    }
    if parsed.host_str().is_none() {
        return Err(AppError::bad_request("GeminiDB endpoint must include host"));
    }
    if parsed.path() != "/" && !parsed.path().is_empty() {
        return Err(AppError::bad_request(
            "GeminiDB endpoint must not include a path",
        ));
    }
    if !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(AppError::bad_request(
            "GeminiDB endpoint must not include credentials, query, or fragment",
        ));
    }
    Ok(())
}

fn validate_project_id(value: &str) -> Result<(), AppError> {
    let valid = value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if valid {
        Ok(())
    } else {
        Err(AppError::bad_request(
            "projectId must contain only letters, digits, '_' or '-'",
        ))
    }
}

fn redact_sensitive(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (key, val) in map {
                let lower = key.to_lowercase();
                let sensitive = lower.contains("password")
                    || lower.contains("secret")
                    || lower.contains("token")
                    || lower == "ak"
                    || lower == "sk"
                    || lower.contains("accesskey")
                    || lower.contains("secretkey");
                if sensitive {
                    out.insert(key.clone(), Value::String("<redacted>".to_string()));
                } else {
                    out.insert(key.clone(), redact_sensitive(val));
                }
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(redact_sensitive).collect()),
        other => other.clone(),
    }
}

fn safe_suffix(tool_id: &str) -> String {
    tool_id.rsplit('.').next().unwrap_or(tool_id).to_string()
}

async fn run_with_timeout<T>(
    timeout_seconds: u64,
    future: impl std::future::Future<Output = anyhow::Result<T>>,
) -> anyhow::Result<T> {
    tokio::time::timeout(Duration::from_secs(timeout_seconds), future)
        .await
        .map_err(|_| anyhow::anyhow!("GeminiDB request timed out after {timeout_seconds}s"))?
}

// ---------- descriptors ----------

fn common_tags() -> Vec<String> {
    vec![
        "built-in".to_string(),
        "huawei-cloud".to_string(),
        "gemini-db".to_string(),
        "manual-run".to_string(),
    ]
}

fn base_descriptor(
    tool_id: &str,
    display_name: &str,
    description: &str,
    enabled: bool,
) -> ToolDescriptor {
    ToolDescriptor {
        tool_id: tool_id.to_string(),
        display_name: display_name.to_string(),
        description: description.to_string(),
        enabled,
        source: ToolSource::BuiltIn,
        read_only: false,
        editable: false,
        exportable: false,
        runnable: enabled,
        tags: common_tags(),
        backend: "gemini_db_influx".to_string(),
        accepted_suffixes: Vec::new(),
        min_files: 0,
        max_files: 0,
        params_schema: Value::Null,
        params_template: Value::Null,
        output_views: vec![
            "summary".to_string(),
            "request".to_string(),
            "response".to_string(),
            "json".to_string(),
        ],
    }
}

fn create_instance_descriptor(enabled: bool) -> ToolDescriptor {
    let mut d = base_descriptor(
        CREATE_INSTANCE_ID,
        "GeminiDB Influx: Create instance",
        "Create a GeminiDB Influx instance (POST /v3/{projectId}/instances). The request body is forwarded verbatim per the HuaweiCloud NoSQL API.",
        enabled,
    );
    d.params_schema = json!({
        "type": "object",
        "properties": {
            "endpoint": { "type": "string" },
            "projectId": { "type": "string" },
            "body": { "type": "object", "description": "Full create-instance request body per the HuaweiCloud NoSQL API (name, datastore, flavor_ref, volume, region, vpc_id, subnet_id, security_group_id, password, mode, ...)." }
        },
        "required": ["body"]
    });
    d.params_template = json!({ "endpoint": "", "projectId": "", "body": {} });
    d
}

fn delete_instance_descriptor(enabled: bool) -> ToolDescriptor {
    let mut d = base_descriptor(
        DELETE_INSTANCE_ID,
        "GeminiDB Influx: Delete instance",
        "Delete a GeminiDB Influx instance (DELETE /v3/{projectId}/instances/{instanceId}).",
        enabled,
    );
    d.params_schema = json!({
        "type": "object",
        "properties": {
            "endpoint": { "type": "string" },
            "projectId": { "type": "string" },
            "instanceId": { "type": "string" }
        },
        "required": ["instanceId"]
    });
    d.params_template = json!({ "endpoint": "", "projectId": "", "instanceId": "" });
    d
}

fn list_instances_descriptor(enabled: bool) -> ToolDescriptor {
    let mut d = base_descriptor(
        LIST_INSTANCES_ID,
        "GeminiDB Influx: List instances",
        "Query GeminiDB Influx instances and details (GET /v3/{projectId}/instances). All filters are optional; pass id to fetch a specific instance.",
        enabled,
    );
    d.read_only = true;
    d.params_schema = json!({
        "type": "object",
        "properties": {
            "endpoint": { "type": "string" },
            "projectId": { "type": "string" },
            "id": { "type": "string" },
            "name": { "type": "string" },
            "mode": { "type": "string" },
            "datastoreType": { "type": "string" },
            "vpcId": { "type": "string" },
            "subnetId": { "type": "string" },
            "offset": { "type": "integer", "minimum": 0 },
            "limit": { "type": "integer", "minimum": 1 }
        }
    });
    d.params_template = json!({
        "endpoint": "", "projectId": "",
        "id": "", "name": "", "mode": "", "datastoreType": "",
        "vpcId": "", "subnetId": "", "offset": 0, "limit": 100
    });
    d
}

fn rename_instance_descriptor(enabled: bool) -> ToolDescriptor {
    let mut d = base_descriptor(
        RENAME_INSTANCE_ID,
        "GeminiDB Influx: Rename instance",
        "Edit the name of a GeminiDB Influx instance (PUT /v3/{projectId}/instances/{instanceId}/name).",
        enabled,
    );
    d.params_schema = json!({
        "type": "object",
        "properties": {
            "endpoint": { "type": "string" },
            "projectId": { "type": "string" },
            "instanceId": { "type": "string" },
            "name": { "type": "string", "description": "New instance name." }
        },
        "required": ["instanceId", "name"]
    });
    d.params_template = json!({ "endpoint": "", "projectId": "", "instanceId": "", "name": "" });
    d
}

fn toggle_ssl_descriptor(enabled: bool) -> ToolDescriptor {
    let mut d = base_descriptor(
        TOGGLE_SSL_ID,
        "GeminiDB Influx: Toggle SSL",
        "Enable or disable SSL on a GeminiDB Influx instance (PUT /v3/{projectId}/instances/{instanceId}/ssl). The request body is forwarded verbatim per the HuaweiCloud NoSQL API.",
        enabled,
    );
    d.params_schema = json!({
        "type": "object",
        "properties": {
            "endpoint": { "type": "string" },
            "projectId": { "type": "string" },
            "instanceId": { "type": "string" },
            "body": { "type": "object", "description": "SSL toggle request body per the HuaweiCloud NoSQL API." }
        },
        "required": ["instanceId", "body"]
    });
    d.params_template = json!({ "endpoint": "", "projectId": "", "instanceId": "", "body": {} });
    d
}

fn restart_instance_descriptor(enabled: bool) -> ToolDescriptor {
    let mut d = base_descriptor(
        RESTART_INSTANCE_ID,
        "GeminiDB Influx: Restart instance/node",
        "Restart a GeminiDB Influx instance or a single node (POST /v3/{projectId}/instances/{instanceId}/restart). The optional body is forwarded verbatim per the HuaweiCloud NoSQL API.",
        enabled,
    );
    d.params_schema = json!({
        "type": "object",
        "properties": {
            "endpoint": { "type": "string" },
            "projectId": { "type": "string" },
            "instanceId": { "type": "string" },
            "body": { "type": "object", "description": "Optional restart body per the HuaweiCloud NoSQL API (e.g. node targeting). Omit to restart the whole instance." }
        },
        "required": ["instanceId"]
    });
    d.params_template = json!({ "endpoint": "", "projectId": "", "instanceId": "", "body": {} });
    d
}

// ---------- HTTP client ----------

struct GeminiDbClient {
    client: reqwest::Client,
    auth_token: String,
}

impl GeminiDbClient {
    fn new(auth_token: &str, timeout_seconds: u64) -> Result<Self, AppError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_seconds.max(1)))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|err| {
                AppError::internal(format!("failed to build GeminiDB HTTP client: {err}"))
            })?;
        Ok(Self {
            client,
            auth_token: auth_token.to_string(),
        })
    }
}

impl GeminiDbHttpClient for GeminiDbClient {
    async fn send(&self, request: GeminiDbHttpRequest) -> anyhow::Result<GeminiDbHttpResponse> {
        let mut builder = self
            .client
            .request(request.method, request.url)
            .header("X-Auth-Token", HeaderValue::from_str(&self.auth_token)?)
            .header("Content-Type", "application/json");
        if !request.query.is_empty() {
            builder = builder.query(&request.query);
        }
        if let Some(body) = request.body {
            builder = builder.body(body);
        }
        let response = builder
            .send()
            .await
            .context("failed to send GeminiDB request")?;
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        let (body, truncated) = truncate_text(&text, MAX_RESPONSE_CHARS);
        Ok(GeminiDbHttpResponse {
            status_code: status.as_u16(),
            body,
            truncated,
        })
    }
}

fn truncate_text(value: &str, max_chars: usize) -> (String, bool) {
    if value.chars().count() <= max_chars {
        (value.to_string(), false)
    } else {
        (value.chars().take(max_chars).collect(), true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc as StdArc,
    };

    #[derive(Clone)]
    struct FakeClient {
        calls: StdArc<AtomicUsize>,
        last: StdArc<std::sync::Mutex<Option<GeminiDbHttpRequest>>>,
        status: u16,
        body: String,
    }

    impl FakeClient {
        fn new(status: u16, body: &str) -> Self {
            Self {
                calls: StdArc::new(AtomicUsize::new(0)),
                last: StdArc::new(std::sync::Mutex::new(None)),
                status,
                body: body.to_string(),
            }
        }

        fn last_request(&self) -> GeminiDbHttpRequest {
            self.last.lock().unwrap().clone().unwrap()
        }
    }

    impl GeminiDbHttpClient for FakeClient {
        async fn send(&self, request: GeminiDbHttpRequest) -> anyhow::Result<GeminiDbHttpResponse> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            *self.last.lock().unwrap() = Some(request.clone());
            Ok(GeminiDbHttpResponse {
                status_code: self.status,
                body: self.body.clone(),
                truncated: false,
            })
        }
    }

    fn enabled_settings() -> GeminiDbSettings {
        GeminiDbSettings {
            enabled: true,
            timeout_seconds: 5,
            endpoint: "https://nosql.cn-north-4.myhuaweicloud.com".to_string(),
            project_id: "pid-123".to_string(),
            project_id_env: Some("LOGAGENT_TEST_GEMINI_PID".to_string()),
            auth_token_env: Some("LOGAGENT_TEST_GEMINI_TOKEN".to_string()),
            auth_token: Some("secret-token".to_string()),
            region: "cn-north-4".to_string(),
        }
    }

    async fn run_plan(plan: GeminiDbPlan, client: &FakeClient) -> (PathBuf, serde_json::Value) {
        let root = std::env::temp_dir().join(format!("gemini-db-test-{}", std::process::id()));
        let workspace = root.join(&plan.tool_id);
        tokio::fs::create_dir_all(&workspace).await.unwrap();
        let meta = GeminiDbEndpointMeta {
            base_url: "https://nosql.cn-north-4.myhuaweicloud.com".to_string(),
            project_id: "pid-123".to_string(),
            region: "cn-north-4".to_string(),
            auth_token_env: Some("LOGAGENT_TEST_GEMINI_TOKEN".to_string()),
        };
        let path = execute_gemini_db_to_artifacts(&workspace, "act_test", plan, &meta, client)
            .await
            .unwrap();
        let value: serde_json::Value =
            serde_json::from_slice(&tokio::fs::read(&path).await.unwrap()).unwrap();
        (path, value)
    }

    fn plan_for(tool_id: &str, params: &GeminiDbParams) -> GeminiDbPlan {
        build_plan(tool_id, params, "pid-123").unwrap()
    }

    #[test]
    fn descriptors_listed_and_gated() {
        let mut config = test_config();
        // disabled by default
        let disabled = descriptors(&config);
        assert_eq!(disabled.len(), 6);
        assert!(disabled.iter().all(|d| !d.enabled && !d.runnable));
        assert!(disabled.iter().all(|d| d.backend == "gemini_db_influx"));
        assert!(disabled
            .iter()
            .all(|d| d.tags.contains(&"gemini-db".to_string())));
        let ids: Vec<_> = disabled.iter().map(|d| d.tool_id.clone()).collect();
        assert!(ids.contains(&CREATE_INSTANCE_ID.to_string()));
        assert!(get_descriptor(&config, RESTART_INSTANCE_ID).is_some());

        config.huawei_cloud.gemini_db.enabled = true;
        let enabled = descriptors(&config);
        assert!(enabled.iter().all(|d| d.enabled && d.runnable));
    }

    #[test]
    fn validates_create_requires_body_object() {
        let mut config = test_config();
        config.huawei_cloud.gemini_db.enabled = true;
        assert!(validate_run_params(&config, CREATE_INSTANCE_ID, &json!({})).is_err());
        assert!(validate_run_params(&config, CREATE_INSTANCE_ID, &json!({"body": [1]})).is_err());
        assert!(
            validate_run_params(&config, CREATE_INSTANCE_ID, &json!({"body": {"name": "x"}}))
                .is_ok()
        );
    }

    #[test]
    fn validates_path_tools_require_instance_id() {
        let mut config = test_config();
        config.huawei_cloud.gemini_db.enabled = true;
        for tool in [DELETE_INSTANCE_ID, TOGGLE_SSL_ID, RESTART_INSTANCE_ID] {
            assert!(validate_run_params(&config, tool, &json!({})).is_err());
        }
        assert!(validate_run_params(
            &config,
            DELETE_INSTANCE_ID,
            &json!({"instanceId": "bad/id"})
        )
        .is_err());
        assert!(validate_run_params(
            &config,
            DELETE_INSTANCE_ID,
            &json!({"instanceId": "550e8400-e29b-41d4-a716-446655440000"})
        )
        .is_ok());
        assert!(validate_run_params(
            &config,
            TOGGLE_SSL_ID,
            &json!({"instanceId": "inst-1", "body": {"ssl": true}})
        )
        .is_ok());
        assert!(validate_run_params(
            &config,
            RESTART_INSTANCE_ID,
            &json!({"instanceId": "inst-1"})
        )
        .is_ok());
    }

    #[test]
    fn validates_rename_requires_name() {
        let mut config = test_config();
        config.huawei_cloud.gemini_db.enabled = true;
        assert!(
            validate_run_params(&config, RENAME_INSTANCE_ID, &json!({"instanceId": "i"})).is_err()
        );
        assert!(validate_run_params(
            &config,
            RENAME_INSTANCE_ID,
            &json!({"instanceId": "i", "name": "new-name"})
        )
        .is_ok());
    }

    #[test]
    fn validates_list_accepts_empty_and_rejects_body() {
        let mut config = test_config();
        config.huawei_cloud.gemini_db.enabled = true;
        assert!(validate_run_params(&config, LIST_INSTANCES_ID, &json!({})).is_ok());
        assert!(
            validate_run_params(&config, LIST_INSTANCES_ID, &json!({"id": "x", "limit": 10}))
                .is_ok()
        );
        assert!(
            validate_run_params(&config, LIST_INSTANCES_ID, &json!({"body": {"x": 1}})).is_err()
        );
    }

    fn test_config() -> AppConfig {
        use crate::support::config::{
            AuthSettings, FetchSettings, HuaweiCloudSettings, LogAnalyzerSettings, McpSettings,
            RemoteExecutionSettings, ServerSettings, SkillSettings, StorageSettings, ToolsSettings,
        };
        use std::path::PathBuf;
        AppConfig {
            server: ServerSettings {
                bind: String::new(),
                public_base_url: String::new(),
                max_concurrent_tasks: 1,
                max_input_chars: 1000,
            },
            auth: AuthSettings {
                api_keys: Vec::new(),
            },
            storage: StorageSettings {
                data_dir: PathBuf::new(),
                max_upload_bytes: 0,
                max_chunk_bytes: 0,
            },
            skills: SkillSettings {
                enabled: false,
                roots: Vec::new(),
                max_skill_chars: 1000,
                max_reference_chars: 1000,
            },
            log_analyzer: LogAnalyzerSettings {
                keywords: Vec::new(),
                max_matches: 0,
            },
            tools: ToolsSettings::default(),
            fetch: FetchSettings::default(),
            huawei_cloud: HuaweiCloudSettings::default(),
            remote_execution: RemoteExecutionSettings::default(),
            mcp: McpSettings::default(),
        }
    }

    #[test]
    fn build_plan_paths_and_methods() {
        let params = GeminiDbParams {
            instance_id: Some("inst-1".to_string()),
            name: Some("new-name".to_string()),
            body: Some(json!({"ssl": true})),
            ..Default::default()
        };
        let create = plan_for(
            CREATE_INSTANCE_ID,
            &GeminiDbParams {
                body: Some(json!({"name":"x"})),
                ..Default::default()
            },
        );
        assert_eq!(create.method, Method::POST);
        assert_eq!(create.path, "/v3/pid-123/instances");

        let delete = plan_for(DELETE_INSTANCE_ID, &params);
        assert_eq!(delete.method, Method::DELETE);
        assert_eq!(delete.path, "/v3/pid-123/instances/inst-1");

        let rename = plan_for(RENAME_INSTANCE_ID, &params);
        assert_eq!(rename.method, Method::PUT);
        assert_eq!(rename.path, "/v3/pid-123/instances/inst-1/name");
        assert_eq!(rename.stored_body, json!({"name": "new-name"}));

        let ssl = plan_for(TOGGLE_SSL_ID, &params);
        assert_eq!(ssl.method, Method::PUT);
        assert_eq!(ssl.path, "/v3/pid-123/instances/inst-1/ssl");

        let restart = plan_for(RESTART_INSTANCE_ID, &params);
        assert_eq!(restart.method, Method::POST);
        assert_eq!(restart.path, "/v3/pid-123/instances/inst-1/restart");

        let list = plan_for(
            LIST_INSTANCES_ID,
            &GeminiDbParams {
                id: Some("inst-1".to_string()),
                limit: Some(10),
                ..Default::default()
            },
        );
        assert_eq!(list.method, Method::GET);
        assert_eq!(list.path, "/v3/pid-123/instances");
        assert_eq!(
            list.query,
            vec![
                ("id".to_string(), "inst-1".to_string()),
                ("limit".to_string(), "10".to_string())
            ]
        );
    }

    #[tokio::test]
    async fn create_writes_ok_result_and_forwards_body() {
        let client = FakeClient::new(200, r#"{"id":"inst-new","job_id":"job-1"}"#);
        let plan = plan_for(
            CREATE_INSTANCE_ID,
            &GeminiDbParams {
                body: Some(json!({"name":"x","password":"hunter2"})),
                ..Default::default()
            },
        );
        let (_path, result) = run_plan(plan, &client).await;
        assert_eq!(result["status"], "OK");
        assert_eq!(result["http"]["method"], "POST");
        assert_eq!(result["http"]["statusCode"], 200);
        // password is redacted in the stored request body
        assert_eq!(result["request"]["body"]["password"], "<redacted>");
        assert_eq!(result["request"]["body"]["name"], "x");
        // the forwarded body still carries the real password
        let sent = client.last_request();
        assert!(sent.body.unwrap().contains("hunter2"));
        // response body captured
        assert!(result["response"]["body"]
            .as_str()
            .unwrap()
            .contains("inst-new"));
        // no token in the persisted result
        assert!(!serde_json::to_string(&result)
            .unwrap()
            .contains("secret-token"));
    }

    #[tokio::test]
    async fn non_2xx_marks_failed() {
        let client = FakeClient::new(404, r#"{"error_msg":"not found"}"#);
        let plan = plan_for(
            DELETE_INSTANCE_ID,
            &GeminiDbParams {
                instance_id: Some("inst-1".to_string()),
                ..Default::default()
            },
        );
        let (_path, result) = run_plan(plan, &client).await;
        assert_eq!(result["status"], "FAILED");
        assert_eq!(result["http"]["statusCode"], 404);
        assert_eq!(result["http"]["method"], "DELETE");
    }

    #[tokio::test]
    async fn list_query_forwarded() {
        let client = FakeClient::new(200, r#"{"instances":[],"total_count":0}"#);
        let plan = plan_for(
            LIST_INSTANCES_ID,
            &GeminiDbParams {
                name: Some("gemini-".to_string()),
                limit: Some(50),
                ..Default::default()
            },
        );
        let (_path, result) = run_plan(plan, &client).await;
        assert_eq!(result["status"], "OK");
        let sent = client.last_request();
        assert_eq!(sent.method, Method::GET);
        assert!(sent
            .query
            .iter()
            .any(|(k, v)| k == "name" && v == "gemini-"));
        assert!(sent.query.iter().any(|(k, v)| k == "limit" && v == "50"));
    }
}
