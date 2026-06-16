use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::{process::Command, time::Duration};
use tracing::{info, warn};

use crate::{
    app::AppState,
    domain::{
        contracts::{ActionKind, ActionRisk, AgentAction, EvidenceProvider, TaskContext},
        models::{GrepResults, Manifest, TaskRecord, ToolDescriptor, ToolSource},
    },
    pipeline::{extract_task, prepare_pipeline_run, search_task},
    services::fetch::{FetchRunParams, FETCH_TOOL_ID},
    services::metadata::{MetadataFieldTypesRequest, MetadataTagFieldsRequest},
    support::{
        config::{AppConfig, ToolSettings},
        error::AppError,
        fs_utils::relative_string,
    },
};

pub const PPROF_ANALYZER_ID: &str = "pprof_analyzer";
pub const METADATA_LIST_INSTANCES_ID: &str = "logagent.list_metadata_instances";
pub const METADATA_GET_SNAPSHOT_ID: &str = "logagent.get_metadata_snapshot";
pub const METADATA_GET_FIELD_TYPES_ID: &str = "logagent.get_metadata_field_types";
pub const METADATA_GET_TAG_FIELDS_ID: &str = "logagent.get_metadata_tag_fields";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PprofParams {
    #[serde(default = "default_sample_index")]
    pub sample_index: String,
    #[serde(default = "default_node_count")]
    pub node_count: usize,
    #[serde(default)]
    pub generate_svg: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfiguredToolParams {
    #[serde(default)]
    input_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PprofRunRecord {
    schema_version: u32,
    tool_id: String,
    action_id: String,
    status: ToolTaskStatus,
    profile_type: String,
    sample_index: String,
    total: Option<String>,
    top: Vec<PprofTopEntry>,
    artifacts: PprofArtifacts,
    warnings: Vec<String>,
    error: Option<String>,
    duration_ms: u128,
    created_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum ToolTaskStatus {
    Ok,
    Failed,
    TimedOut,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PprofTopEntry {
    rank: usize,
    flat: String,
    flat_percent: Option<f64>,
    sum_percent: Option<f64>,
    cum: String,
    cum_percent: Option<f64>,
    function: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PprofArtifacts {
    top_text_path: String,
    tree_text_path: String,
    raw_text_path: String,
    svg_path: Option<String>,
    stderr_path: String,
}

struct CommandRun {
    status: ToolTaskStatus,
    exit_code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    duration_ms: u128,
    error: Option<String>,
}

pub fn descriptors(config: &AppConfig) -> Vec<ToolDescriptor> {
    let mut descriptors = Vec::new();
    for tool in config.tools.tools.values() {
        if tool.name == PPROF_ANALYZER_ID {
            descriptors.push(pprof_descriptor(config));
        } else {
            descriptors.push(configured_tool_descriptor(tool));
        }
    }
    descriptors.extend(metadata_descriptors());
    descriptors.push(fetch_descriptor(config));
    descriptors
}

pub fn get_descriptor(config: &AppConfig, tool_id: &str) -> Option<ToolDescriptor> {
    if let Some(tool) = config.tools.tools.get(tool_id) {
        return Some(if tool_id == PPROF_ANALYZER_ID {
            pprof_descriptor(config)
        } else {
            configured_tool_descriptor(tool)
        });
    }
    metadata_descriptors()
        .into_iter()
        .chain(std::iter::once(fetch_descriptor(config)))
        .find(|descriptor| descriptor.tool_id == tool_id)
}

pub fn validate_tool_run_request(
    config: &AppConfig,
    tool_id: &str,
    upload_count: usize,
    params: &serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let descriptor = get_descriptor(config, tool_id)
        .ok_or_else(|| AppError::not_found(format!("unknown toolId {tool_id}")))?;
    if !descriptor.enabled {
        return Err(AppError::bad_request(format!("tool {tool_id} is disabled")));
    }
    if !descriptor.runnable {
        let reason = if descriptor.read_only {
            "is read-only and cannot be run manually"
        } else {
            "does not support manual runs"
        };
        return Err(AppError::bad_request(format!("tool {tool_id} {reason}")));
    }
    if upload_count < descriptor.min_files || upload_count > descriptor.max_files {
        return Err(AppError::bad_request(format!(
            "tool {tool_id} expects {}..{} upload(s)",
            descriptor.min_files, descriptor.max_files
        )));
    }
    match tool_id {
        PPROF_ANALYZER_ID => {
            let parsed = parse_pprof_params(params)?;
            serde_json::to_value(parsed)
                .map_err(|err| AppError::internal(format!("failed to encode pprof params: {err}")))
        }
        METADATA_LIST_INSTANCES_ID => validate_metadata_list_params(params),
        METADATA_GET_SNAPSHOT_ID => validate_metadata_snapshot_params(params),
        METADATA_GET_FIELD_TYPES_ID => validate_metadata_field_types_params(params),
        METADATA_GET_TAG_FIELDS_ID => validate_metadata_tag_fields_params(params),
        FETCH_TOOL_ID => validate_fetch_params(config, params),
        _ if config.tools.tools.contains_key(tool_id) => validate_configured_tool_params(params),
        _ => Err(AppError::not_found(format!("unknown toolId {tool_id}"))),
    }
}

pub async fn run_tool_task(state: Arc<AppState>, task: TaskRecord) -> Result<PathBuf, AppError> {
    match task.tool_id.as_deref() {
        Some(PPROF_ANALYZER_ID) => run_pprof_task(state.config.clone(), task).await,
        Some(
            METADATA_LIST_INSTANCES_ID
            | METADATA_GET_SNAPSHOT_ID
            | METADATA_GET_FIELD_TYPES_ID
            | METADATA_GET_TAG_FIELDS_ID,
        ) => run_metadata_task(state, task).await,
        Some(FETCH_TOOL_ID) => crate::services::fetch::run_fetch_task(state, task).await,
        Some(tool_id) if state.config.tools.tools.contains_key(tool_id) => {
            run_configured_tool_task(state, task).await
        }
        Some(tool_id) => Err(AppError::bad_request(format!("unknown toolId {tool_id}"))),
        None => Err(AppError::bad_request("tool run task is missing toolId")),
    }
}

fn pprof_descriptor(config: &AppConfig) -> ToolDescriptor {
    let enabled = config
        .tools
        .tools
        .get(PPROF_ANALYZER_ID)
        .map(|tool| tool.enabled)
        .unwrap_or(false);
    ToolDescriptor {
        tool_id: PPROF_ANALYZER_ID.to_string(),
        display_name: "Golang pprof Analyzer".to_string(),
        description: "Upload a Go pprof profile and inspect top functions plus raw/tree output."
            .to_string(),
        enabled,
        source: ToolSource::Configured,
        read_only: false,
        editable: true,
        exportable: enabled,
        runnable: enabled,
        tags: vec![
            "configured".to_string(),
            "manual-run".to_string(),
            "pprof".to_string(),
        ],
        backend: "command".to_string(),
        accepted_suffixes: vec![
            ".pprof".to_string(),
            ".prof".to_string(),
            ".profile".to_string(),
            ".pb.gz".to_string(),
        ],
        min_files: 1,
        max_files: 1,
        params_schema: serde_json::json!({
            "sampleIndex": { "type": "string", "default": "samples" },
            "nodeCount": { "type": "integer", "default": 50, "minimum": 1, "maximum": 200 },
            "generateSvg": { "type": "boolean", "default": false }
        }),
        params_template: pprof_params_template(),
        output_views: vec![
            "summary".to_string(),
            "top_table".to_string(),
            "tree_text".to_string(),
            "raw_text".to_string(),
            "svg".to_string(),
        ],
    }
}

fn configured_tool_descriptor(tool: &ToolSettings) -> ToolDescriptor {
    ToolDescriptor {
        tool_id: tool.name.clone(),
        display_name: display_name_from_id(&tool.name),
        description: format!(
            "Configured Tool Runner command with up to {} input file(s).",
            tool.max_input_files
        ),
        enabled: tool.enabled,
        source: ToolSource::Configured,
        read_only: false,
        editable: true,
        exportable: tool.enabled,
        runnable: tool.enabled,
        tags: vec![
            "configured".to_string(),
            "manual-run".to_string(),
            "tool-runner".to_string(),
            "external".to_string(),
        ],
        backend: "command".to_string(),
        accepted_suffixes: tool.match_settings.file_patterns.clone(),
        min_files: 1,
        max_files: tool.max_input_files,
        params_schema: serde_json::json!({
            "configuredArgs": {
                "type": "array",
                "items": { "type": "string" },
                "readOnly": true,
                "value": tool.args.clone()
            },
            "match": {
                "type": "object",
                "properties": {
                    "filePatterns": {
                        "type": "array",
                        "items": { "type": "string" },
                        "value": tool.match_settings.file_patterns.clone()
                    },
                    "keywords": {
                        "type": "array",
                        "items": { "type": "string" },
                        "value": tool.match_settings.keywords.clone()
                    }
                }
            }
        }),
        params_template: serde_json::json!({
            "inputFiles": []
        }),
        output_views: vec![
            "summary".to_string(),
            "findings".to_string(),
            "stdout".to_string(),
            "stderr".to_string(),
        ],
    }
}

fn metadata_descriptors() -> Vec<ToolDescriptor> {
    vec![
        ToolDescriptor {
            tool_id: METADATA_LIST_INSTANCES_ID.to_string(),
            display_name: "Metadata instances".to_string(),
            description: "List imported metadata instance summaries.".to_string(),
            enabled: true,
            source: ToolSource::BuiltIn,
            read_only: true,
            editable: false,
            exportable: false,
            runnable: true,
            tags: metadata_tags(),
            backend: "builtin".to_string(),
            accepted_suffixes: Vec::new(),
            min_files: 0,
            max_files: 0,
            params_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
            params_template: serde_json::json!({}),
            output_views: vec!["json".to_string()],
        },
        ToolDescriptor {
            tool_id: METADATA_GET_SNAPSHOT_ID.to_string(),
            display_name: "Metadata snapshot".to_string(),
            description: "Read one imported metadata snapshot by instance id.".to_string(),
            enabled: true,
            source: ToolSource::BuiltIn,
            read_only: true,
            editable: false,
            exportable: false,
            runnable: true,
            tags: metadata_tags(),
            backend: "builtin".to_string(),
            accepted_suffixes: Vec::new(),
            min_files: 0,
            max_files: 0,
            params_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "instanceId": { "type": "string" }
                },
                "required": ["instanceId"]
            }),
            params_template: serde_json::json!({
                "instanceId": ""
            }),
            output_views: vec!["json".to_string()],
        },
        ToolDescriptor {
            tool_id: METADATA_GET_FIELD_TYPES_ID.to_string(),
            display_name: "Metadata field types".to_string(),
            description:
                "Look up field type metadata for one imported instance, database and measurement."
                    .to_string(),
            enabled: true,
            source: ToolSource::BuiltIn,
            read_only: true,
            editable: false,
            exportable: false,
            runnable: true,
            tags: metadata_tags(),
            backend: "builtin".to_string(),
            accepted_suffixes: Vec::new(),
            min_files: 0,
            max_files: 0,
            params_schema: serde_json::json!({
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
            }),
            params_template: serde_json::json!({
                "instanceId": "",
                "database": "",
                "measurement": "",
                "retentionPolicy": "",
                "field": []
            }),
            output_views: vec!["json".to_string()],
        },
        ToolDescriptor {
            tool_id: METADATA_GET_TAG_FIELDS_ID.to_string(),
            display_name: "Metadata tag fields".to_string(),
            description:
                "List Tag type fields for one imported instance, database and measurement."
                    .to_string(),
            enabled: true,
            source: ToolSource::BuiltIn,
            read_only: true,
            editable: false,
            exportable: false,
            runnable: true,
            tags: metadata_tags(),
            backend: "builtin".to_string(),
            accepted_suffixes: Vec::new(),
            min_files: 0,
            max_files: 0,
            params_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "instanceId": { "type": "string" },
                    "database": { "type": "string" },
                    "measurement": { "type": "string" },
                    "retentionPolicy": { "type": "string" }
                },
                "required": ["instanceId", "database", "measurement"]
            }),
            params_template: serde_json::json!({
                "instanceId": "",
                "database": "",
                "measurement": "",
                "retentionPolicy": ""
            }),
            output_views: vec!["json".to_string()],
        },
    ]
}

fn fetch_descriptor(config: &AppConfig) -> ToolDescriptor {
    ToolDescriptor {
        tool_id: FETCH_TOOL_ID.to_string(),
        display_name: "Fetch endpoint".to_string(),
        description: "Run a managed HTTP endpoint imported from a browser DevTools curl command."
            .to_string(),
        enabled: config.fetch.enabled,
        source: ToolSource::BuiltIn,
        read_only: false,
        editable: false,
        exportable: false,
        runnable: config.fetch.enabled,
        tags: vec![
            "built-in".to_string(),
            "fetch".to_string(),
            "http".to_string(),
            "manual-run".to_string(),
        ],
        backend: "fetch".to_string(),
        accepted_suffixes: Vec::new(),
        min_files: 0,
        max_files: 0,
        params_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "fetchId": { "type": "string" },
                "variables": { "type": "object", "additionalProperties": { "type": "string" } },
                "headers": { "type": "object", "additionalProperties": { "type": "string" } },
                "body": { "type": "string" }
            },
            "required": ["fetchId"]
        }),
        params_template: serde_json::json!({
            "fetchId": "",
            "variables": {},
            "headers": {},
            "body": null
        }),
        output_views: vec![
            "summary".to_string(),
            "request".to_string(),
            "response".to_string(),
            "body_artifact".to_string(),
        ],
    }
}

fn metadata_tags() -> Vec<String> {
    vec![
        "built-in".to_string(),
        "metadata".to_string(),
        "read-only".to_string(),
        "manual-run".to_string(),
    ]
}

fn pprof_params_template() -> serde_json::Value {
    serde_json::json!({
        "sampleIndex": default_sample_index(),
        "nodeCount": default_node_count(),
        "generateSvg": false
    })
}

fn validate_configured_tool_params(
    value: &serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let params = parse_configured_tool_params(value)?;
    serde_json::to_value(params).map_err(|err| {
        AppError::internal(format!("failed to encode configured tool params: {err}"))
    })
}

fn parse_configured_tool_params(
    value: &serde_json::Value,
) -> Result<ConfiguredToolParams, AppError> {
    if value.is_null() {
        return Ok(ConfiguredToolParams {
            input_files: Vec::new(),
        });
    }
    serde_json::from_value(value.clone())
        .map_err(|err| AppError::bad_request(format!("invalid configured tool params: {err}")))
}

fn validate_metadata_list_params(value: &serde_json::Value) -> Result<serde_json::Value, AppError> {
    if value.is_null() {
        return Ok(serde_json::json!({}));
    }
    let object = value
        .as_object()
        .ok_or_else(|| AppError::bad_request("metadata list params must be an object"))?;
    if object.is_empty() {
        Ok(serde_json::json!({}))
    } else {
        Err(AppError::bad_request(
            "metadata list params must not contain fields",
        ))
    }
}

fn validate_metadata_snapshot_params(
    value: &serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let instance_id = required_string_param(value, "instanceId")?;
    Ok(serde_json::json!({ "instanceId": instance_id }))
}

fn validate_metadata_field_types_params(
    value: &serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let object = value
        .as_object()
        .ok_or_else(|| AppError::bad_request("metadata field type params must be an object"))?;
    let instance_id = required_string_param(value, "instanceId")?;
    let database = required_string_param(value, "database")?;
    let measurement = required_string_param(value, "measurement")?;
    let mut normalized = serde_json::Map::new();
    normalized.insert("instanceId".to_string(), serde_json::json!(instance_id));
    normalized.insert("database".to_string(), serde_json::json!(database));
    normalized.insert("measurement".to_string(), serde_json::json!(measurement));
    if let Some(retention_policy) = object
        .get("retentionPolicy")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        normalized.insert(
            "retentionPolicy".to_string(),
            serde_json::json!(retention_policy),
        );
    }
    if let Some(field) = object.get("field") {
        match field {
            serde_json::Value::String(value) => {
                let value = value.trim();
                if !value.is_empty() {
                    normalized.insert("field".to_string(), serde_json::json!(value));
                }
            }
            serde_json::Value::Array(values) => {
                let fields = values
                    .iter()
                    .map(|value| {
                        value
                            .as_str()
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .ok_or_else(|| {
                                AppError::bad_request("field entries must be non-empty strings")
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if !fields.is_empty() {
                    normalized.insert("field".to_string(), serde_json::json!(fields));
                }
            }
            serde_json::Value::Null => {}
            _ => {
                return Err(AppError::bad_request(
                    "field must be a string or string array",
                ))
            }
        }
    }
    Ok(serde_json::Value::Object(normalized))
}

fn validate_metadata_tag_fields_params(
    value: &serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let object = value
        .as_object()
        .ok_or_else(|| AppError::bad_request("metadata tag field params must be an object"))?;
    if object.contains_key("field") {
        return Err(AppError::bad_request(
            "metadata tag field params do not support field",
        ));
    }
    let instance_id = required_string_param(value, "instanceId")?;
    let database = required_string_param(value, "database")?;
    let measurement = required_string_param(value, "measurement")?;
    let mut normalized = serde_json::Map::new();
    normalized.insert("instanceId".to_string(), serde_json::json!(instance_id));
    normalized.insert("database".to_string(), serde_json::json!(database));
    normalized.insert("measurement".to_string(), serde_json::json!(measurement));
    if let Some(retention_policy) = object
        .get("retentionPolicy")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        normalized.insert(
            "retentionPolicy".to_string(),
            serde_json::json!(retention_policy),
        );
    }
    Ok(serde_json::Value::Object(normalized))
}

fn validate_fetch_params(
    config: &AppConfig,
    value: &serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    if !config.fetch.enabled {
        return Err(AppError::bad_request("fetch is disabled by server config"));
    }
    let params: FetchRunParams = serde_json::from_value(value.clone())
        .map_err(|err| AppError::bad_request(format!("invalid fetch params: {err}")))?;
    if params.fetch_id.trim().is_empty() {
        return Err(AppError::bad_request("fetchId is required"));
    }
    serde_json::to_value(params)
        .map_err(|err| AppError::internal(format!("failed to encode fetch params: {err}")))
}

fn required_string_param(value: &serde_json::Value, key: &str) -> Result<String, AppError> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| AppError::bad_request(format!("{key} is required")))
}

fn safe_action_suffix(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn stable_hash_hex(value: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn display_name_from_id(tool_id: &str) -> String {
    tool_id
        .split(['_', '-', '.'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

async fn run_pprof_task(config: Arc<AppConfig>, task: TaskRecord) -> Result<PathBuf, AppError> {
    let tool = config
        .tools
        .tools
        .get(PPROF_ANALYZER_ID)
        .filter(|tool| tool.enabled)
        .ok_or_else(|| AppError::bad_request("pprof_analyzer is not configured or enabled"))?
        .clone();
    let params = parse_pprof_params(&task.tool_params)?;
    let workspace = config.storage.workspace_dir(&task.task_id);
    let input = task
        .inputs
        .first()
        .ok_or_else(|| AppError::bad_request("pprof analyzer requires one upload"))?;
    validate_profile_suffix(&input.filename)?;
    let profile_path = workspace.join(validate_workspace_relative_path(&input.raw_path)?);

    let action_id = format!("act_tool_pprof_analyzer_{}", task.task_id);
    let result_dir = workspace.join("tool_results").join(&action_id);
    fs::create_dir_all(&result_dir)
        .map_err(|err| AppError::internal(format!("failed to create tool result dir: {err}")))?;
    let tmp_dir = result_dir.join("tmp");
    fs::create_dir_all(&tmp_dir)
        .map_err(|err| AppError::internal(format!("failed to create pprof temp dir: {err}")))?;

    let started = Instant::now();
    info!(
        task_id = %task.task_id,
        action_id = %action_id,
        input_file = %input.filename,
        sample_index = %params.sample_index,
        node_count = params.node_count,
        generate_svg = params.generate_svg,
        "starting pprof analyzer task"
    );
    let mut warnings = Vec::new();
    let top = run_pprof_command(
        &workspace,
        &tmp_dir,
        &tool,
        &[
            "-top".to_string(),
            format!("-sample_index={}", params.sample_index),
            format!("-nodecount={}", params.node_count),
            "-symbolize=none".to_string(),
            profile_path.display().to_string(),
        ],
    )
    .await;
    let tree = run_pprof_command(
        &workspace,
        &tmp_dir,
        &tool,
        &[
            "-tree".to_string(),
            format!("-sample_index={}", params.sample_index),
            format!("-nodecount={}", params.node_count),
            "-symbolize=none".to_string(),
            profile_path.display().to_string(),
        ],
    )
    .await;
    let raw = run_pprof_command(
        &workspace,
        &tmp_dir,
        &tool,
        &[
            "-raw".to_string(),
            format!("-sample_index={}", params.sample_index),
            "-symbolize=none".to_string(),
            profile_path.display().to_string(),
        ],
    )
    .await;

    let svg = if params.generate_svg {
        Some(
            run_pprof_command(
                &workspace,
                &tmp_dir,
                &tool,
                &[
                    "-svg".to_string(),
                    format!("-sample_index={}", params.sample_index),
                    format!("-nodecount={}", params.node_count),
                    "-symbolize=none".to_string(),
                    profile_path.display().to_string(),
                ],
            )
            .await,
        )
    } else {
        None
    };

    write_bytes(&result_dir.join("top.txt"), &top.stdout)?;
    write_bytes(&result_dir.join("tree.txt"), &tree.stdout)?;
    write_bytes(&result_dir.join("raw.txt"), &raw.stdout)?;
    let mut stderr = Vec::new();
    append_command_stderr(&mut stderr, "top", &top);
    append_command_stderr(&mut stderr, "tree", &tree);
    append_command_stderr(&mut stderr, "raw", &raw);
    let svg_path = match svg.as_ref() {
        Some(svg) if matches!(svg.status, ToolTaskStatus::Ok) => {
            let path = result_dir.join("graph.svg");
            write_bytes(&path, &svg.stdout)?;
            append_command_stderr(&mut stderr, "svg", svg);
            Some(
                relative_string(&workspace, &path)
                    .map_err(|err| AppError::internal(err.to_string()))?,
            )
        }
        Some(svg) => {
            warnings.push(format!(
                "SVG generation failed: {}",
                svg.error
                    .clone()
                    .unwrap_or_else(|| format!("exitCode={:?}", svg.exit_code))
            ));
            append_command_stderr(&mut stderr, "svg", svg);
            None
        }
        None => None,
    };
    write_bytes(&result_dir.join("stderr.txt"), &stderr)?;

    let top_text = String::from_utf8_lossy(&top.stdout);
    let (profile_type, total, top_entries) = parse_top_text(&top_text);
    let status = combined_status([&top, &tree, &raw]);
    let error = if matches!(status, ToolTaskStatus::Ok) {
        None
    } else {
        Some("one or more pprof commands failed".to_string())
    };
    let record = PprofRunRecord {
        schema_version: 1,
        tool_id: PPROF_ANALYZER_ID.to_string(),
        action_id,
        status,
        profile_type,
        sample_index: params.sample_index,
        total,
        top: top_entries,
        artifacts: PprofArtifacts {
            top_text_path: relative_string(&workspace, &result_dir.join("top.txt"))
                .map_err(|err| AppError::internal(err.to_string()))?,
            tree_text_path: relative_string(&workspace, &result_dir.join("tree.txt"))
                .map_err(|err| AppError::internal(err.to_string()))?,
            raw_text_path: relative_string(&workspace, &result_dir.join("raw.txt"))
                .map_err(|err| AppError::internal(err.to_string()))?,
            svg_path,
            stderr_path: relative_string(&workspace, &result_dir.join("stderr.txt"))
                .map_err(|err| AppError::internal(err.to_string()))?,
        },
        warnings,
        error,
        duration_ms: started.elapsed().as_millis(),
        created_at: Utc::now(),
    };
    let result_path = result_dir.join("result.json");
    write_json(&result_path, &record)?;
    if matches!(record.status, ToolTaskStatus::Ok) {
        info!(
            task_id = %task.task_id,
            action_id = %record.action_id,
            duration_ms = record.duration_ms,
            result_path = %result_path.display(),
            "pprof analyzer task completed"
        );
    } else {
        warn!(
            task_id = %task.task_id,
            action_id = %record.action_id,
            status = ?record.status,
            duration_ms = record.duration_ms,
            result_path = %result_path.display(),
            "pprof analyzer task completed with warnings or errors"
        );
    }
    Ok(result_path)
}

async fn run_metadata_task(state: Arc<AppState>, task: TaskRecord) -> Result<PathBuf, AppError> {
    let tool_id = task
        .tool_id
        .as_deref()
        .ok_or_else(|| AppError::bad_request("tool run task is missing toolId"))?;
    let tool_id = tool_id.to_string();
    let workspace = state.config.storage.workspace_dir(&task.task_id);
    let action_id = format!(
        "act_tool_metadata_{}_{}",
        safe_action_suffix(&tool_id),
        task.task_id
    );
    let result_dir = workspace.join("tool_results").join(&action_id);
    fs::create_dir_all(&result_dir)
        .map_err(|err| AppError::internal(format!("failed to create tool result dir: {err}")))?;
    let started = Instant::now();
    let result = match tool_id.as_str() {
        METADATA_LIST_INSTANCES_ID => {
            validate_metadata_list_params(&task.tool_params)?;
            serde_json::json!({ "instances": state.metadata.list_instances().await })
        }
        METADATA_GET_SNAPSHOT_ID => {
            let instance_id = required_string_param(&task.tool_params, "instanceId")?;
            serde_json::json!({ "snapshot": state.metadata.get_instance_snapshot(&instance_id).await? })
        }
        METADATA_GET_FIELD_TYPES_ID => {
            let request: MetadataFieldTypesRequest =
                serde_json::from_value(task.tool_params.clone()).map_err(|err| {
                    AppError::bad_request(format!("invalid metadata field type params: {err}"))
                })?;
            serde_json::json!({ "result": state.metadata.get_metadata_field_types(request).await? })
        }
        METADATA_GET_TAG_FIELDS_ID => {
            let request: MetadataTagFieldsRequest =
                serde_json::from_value(task.tool_params.clone()).map_err(|err| {
                    AppError::bad_request(format!("invalid metadata tag field params: {err}"))
                })?;
            serde_json::json!({ "result": state.metadata.get_metadata_tag_fields(request).await? })
        }
        _ => {
            return Err(AppError::bad_request(format!(
                "unknown metadata tool {tool_id}"
            )))
        }
    };
    let record = serde_json::json!({
        "schemaVersion": 1,
        "toolId": tool_id,
        "actionId": action_id,
        "status": "OK",
        "params": task.tool_params,
        "result": result,
        "durationMs": started.elapsed().as_millis(),
        "createdAt": Utc::now()
    });
    let result_path = result_dir.join("result.json");
    write_json(&result_path, &record)?;
    Ok(result_path)
}

async fn run_configured_tool_task(
    state: Arc<AppState>,
    task: TaskRecord,
) -> Result<PathBuf, AppError> {
    let tool_id = task
        .tool_id
        .as_deref()
        .ok_or_else(|| AppError::bad_request("tool run task is missing toolId"))?
        .to_string();
    let tool = state
        .config
        .tools
        .tools
        .get(&tool_id)
        .filter(|tool| tool.enabled)
        .ok_or_else(|| AppError::bad_request(format!("tool {tool_id} is disabled")))?
        .clone();
    let params = parse_configured_tool_params(&task.tool_params)?;
    let workspace = state.config.storage.workspace_dir(&task.task_id);
    prepare_pipeline_run(&workspace).await?;
    extract_task(state.config.clone(), task.clone()).await?;
    search_task(state.config.clone(), &task.task_id).await?;

    let input_files = if params.input_files.is_empty() {
        let manifest: Manifest = read_json_file_sync(&workspace.join("manifest.json"))?;
        let grep: GrepResults = read_json_file_sync(&workspace.join("grep_results.json"))?;
        let selected = state
            .tool_runner
            .rule_based_actions(&manifest, &grep)
            .into_iter()
            .filter(|action| {
                action.input.get("tool").and_then(serde_json::Value::as_str)
                    == Some(tool_id.as_str())
            })
            .filter_map(|action| {
                action
                    .input
                    .get("inputFile")
                    .and_then(serde_json::Value::as_str)
                    .map(ToString::to_string)
            })
            .collect::<Vec<_>>();
        if selected.is_empty() {
            return Err(AppError::bad_request(format!(
                "tool {tool_id} did not match any extracted input file; set params.inputFiles explicitly"
            )));
        }
        selected
    } else {
        params.input_files
    };
    if input_files.len() > tool.max_input_files {
        return Err(AppError::bad_request(format!(
            "tool {tool_id} accepts at most {} input file(s)",
            tool.max_input_files
        )));
    }

    let mut results = Vec::new();
    let context = TaskContext::from_record(&task, workspace.clone(), None);
    for input_file in normalized_input_files(input_files, &workspace)? {
        let action = AgentAction {
            schema_version: 1,
            action_id: format!(
                "act_tool_manual_{}_{}",
                safe_action_suffix(&tool_id),
                stable_hash_hex(&format!("{}:{input_file}", task.task_id))
            ),
            kind: ActionKind::RunTool,
            reason: "manual tool run".to_string(),
            evidence_refs: Vec::new(),
            input: serde_json::json!({
                "tool": tool_id,
                "inputFile": input_file
            }),
            risk: ActionRisk::SafeReadOnly,
            fingerprint: format!("manual_tool:{tool_id}:{input_file}"),
        };
        let artifact = state
            .tool_runner
            .execute(&context, &action)
            .await
            .map_err(|err| AppError::internal(format!("tool runner failed: {err:#}")))?;
        let output: serde_json::Value =
            read_json_file_sync(&workspace.join(&artifact.artifact_path))?;
        results.push(serde_json::json!({
            "actionId": action.action_id,
            "inputFile": input_file,
            "artifactPath": artifact.artifact_path,
            "summary": artifact.summary,
            "result": output
        }));
    }

    let status = aggregate_tool_status(&results);
    let action_id = format!(
        "act_tool_manual_{}_{}",
        safe_action_suffix(&tool_id),
        task.task_id
    );
    let result_dir = workspace.join("tool_results").join(&action_id);
    fs::create_dir_all(&result_dir)
        .map_err(|err| AppError::internal(format!("failed to create tool result dir: {err}")))?;
    let record = serde_json::json!({
        "schemaVersion": 1,
        "toolId": tool_id,
        "actionId": action_id,
        "status": status,
        "params": task.tool_params,
        "inputFiles": results
            .iter()
            .filter_map(|result| result.get("inputFile").cloned())
            .collect::<Vec<_>>(),
        "results": results,
        "createdAt": Utc::now()
    });
    let result_path = result_dir.join("result.json");
    write_json(&result_path, &record)?;
    Ok(result_path)
}

fn normalized_input_files(
    input_files: Vec<String>,
    workspace: &Path,
) -> Result<Vec<String>, AppError> {
    let mut normalized = Vec::new();
    for input_file in input_files {
        let trimmed = input_file.trim();
        if trimmed.is_empty() {
            return Err(AppError::bad_request(
                "params.inputFiles entries must be non-empty strings",
            ));
        }
        if !trimmed.starts_with("extracted/") {
            return Err(AppError::bad_request(
                "params.inputFiles entries must be extracted/ relative paths",
            ));
        }
        let path = validate_workspace_relative_path(trimmed)
            .map_err(|_| AppError::bad_request("params.inputFiles contains unsafe path"))?;
        if !workspace.join(path).is_file() {
            return Err(AppError::bad_request(format!(
                "params.inputFiles entry does not exist: {trimmed}"
            )));
        }
        if !normalized.iter().any(|existing| existing == trimmed) {
            normalized.push(trimmed.to_string());
        }
    }
    if normalized.is_empty() {
        return Err(AppError::bad_request(
            "tool run did not select any input files",
        ));
    }
    Ok(normalized)
}

fn aggregate_tool_status(results: &[serde_json::Value]) -> &'static str {
    let mut failed = false;
    for result in results {
        let status = result
            .get("result")
            .and_then(|result| result.get("status"))
            .and_then(serde_json::Value::as_str);
        match status {
            Some("TIMED_OUT") => return "TIMED_OUT",
            Some("FAILED") => failed = true,
            _ => {}
        }
    }
    if failed {
        "FAILED"
    } else {
        "OK"
    }
}

fn read_json_file_sync<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, AppError> {
    let raw = fs::read_to_string(path)
        .map_err(|err| AppError::internal(format!("failed to read {}: {err}", path.display())))?;
    serde_json::from_str(&raw)
        .map_err(|err| AppError::internal(format!("failed to parse {}: {err}", path.display())))
}

async fn run_pprof_command(
    workspace: &Path,
    tmp_dir: &Path,
    tool: &ToolSettings,
    pprof_args: &[String],
) -> CommandRun {
    let started = Instant::now();
    let mut process = Command::new(&tool.path);
    process
        .arg("tool")
        .arg("pprof")
        .args(pprof_args)
        .current_dir(workspace)
        .env("PPROF_TMPDIR", tmp_dir)
        .kill_on_drop(true)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let child = match process.spawn() {
        Ok(child) => child,
        Err(err) => {
            return CommandRun {
                status: ToolTaskStatus::Failed,
                exit_code: None,
                stdout: Vec::new(),
                stderr: err.to_string().into_bytes(),
                duration_ms: started.elapsed().as_millis(),
                error: Some(err.to_string()),
            }
        }
    };
    match tokio::time::timeout(
        Duration::from_secs(tool.timeout_seconds),
        child.wait_with_output(),
    )
    .await
    {
        Ok(Ok(output)) => {
            let status = if output.status.success() {
                ToolTaskStatus::Ok
            } else {
                ToolTaskStatus::Failed
            };
            CommandRun {
                status,
                exit_code: output.status.code(),
                stdout: truncate_bytes(&output.stdout, tool.max_output_bytes),
                stderr: truncate_bytes(&output.stderr, tool.max_output_bytes),
                duration_ms: started.elapsed().as_millis(),
                error: (!matches!(status, ToolTaskStatus::Ok))
                    .then(|| format!("pprof exited with status {:?}", output.status.code())),
            }
        }
        Ok(Err(err)) => CommandRun {
            status: ToolTaskStatus::Failed,
            exit_code: None,
            stdout: Vec::new(),
            stderr: err.to_string().into_bytes(),
            duration_ms: started.elapsed().as_millis(),
            error: Some(err.to_string()),
        },
        Err(_) => CommandRun {
            status: ToolTaskStatus::TimedOut,
            exit_code: None,
            stdout: Vec::new(),
            stderr: b"pprof command timed out".to_vec(),
            duration_ms: started.elapsed().as_millis(),
            error: Some("pprof command timed out".to_string()),
        },
    }
}

fn parse_pprof_params(value: &serde_json::Value) -> Result<PprofParams, AppError> {
    let params: PprofParams = if value.is_null() {
        PprofParams {
            sample_index: default_sample_index(),
            node_count: default_node_count(),
            generate_svg: false,
        }
    } else {
        serde_json::from_value(value.clone())
            .map_err(|err| AppError::bad_request(format!("invalid pprof params: {err}")))?
    };
    let sample_index = params.sample_index.trim();
    let valid_sample_index = !sample_index.is_empty()
        && sample_index
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'_' || value == b'-');
    if !valid_sample_index {
        return Err(AppError::bad_request(
            "sampleIndex must contain only letters, digits, '_' or '-'",
        ));
    }
    Ok(PprofParams {
        sample_index: sample_index.to_string(),
        node_count: params.node_count.clamp(1, 200),
        generate_svg: params.generate_svg,
    })
}

fn validate_profile_suffix(filename: &str) -> Result<(), AppError> {
    let lower = filename.to_ascii_lowercase();
    let valid = [".pprof", ".prof", ".profile", ".pb.gz"]
        .iter()
        .any(|suffix| lower.ends_with(suffix));
    if valid {
        Ok(())
    } else {
        Err(AppError::bad_request(
            "pprof analyzer accepts .pprof, .prof, .profile or .pb.gz files",
        ))
    }
}

fn validate_workspace_relative_path(value: &str) -> Result<&Path, AppError> {
    let path = Path::new(value);
    let valid = !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)));
    if valid {
        Ok(path)
    } else {
        Err(AppError::internal("tool task contains unsafe raw path"))
    }
}

fn append_command_stderr(buffer: &mut Vec<u8>, label: &str, run: &CommandRun) {
    buffer.extend_from_slice(
        format!("== {label} ({:?}, {}ms) ==\n", run.status, run.duration_ms).as_bytes(),
    );
    buffer.extend_from_slice(&run.stderr);
    if !run.stderr.ends_with(b"\n") {
        buffer.push(b'\n');
    }
}

fn combined_status<'a>(runs: impl IntoIterator<Item = &'a CommandRun>) -> ToolTaskStatus {
    let mut failed = false;
    for run in runs {
        match run.status {
            ToolTaskStatus::TimedOut => return ToolTaskStatus::TimedOut,
            ToolTaskStatus::Failed => failed = true,
            ToolTaskStatus::Ok => {}
        }
    }
    if failed {
        ToolTaskStatus::Failed
    } else {
        ToolTaskStatus::Ok
    }
}

fn parse_top_text(text: &str) -> (String, Option<String>, Vec<PprofTopEntry>) {
    let mut profile_type = "unknown".to_string();
    let mut total = None;
    let mut entries = Vec::new();
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("Type:") {
            profile_type = value.trim().to_string();
        }
        if line.contains(" total") && line.contains("Showing nodes accounting for") {
            total = line
                .split(" of ")
                .nth(1)
                .and_then(|value| value.split(" total").next())
                .map(|value| value.trim().to_string());
        }
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() < 6
            || !fields[1].ends_with('%')
            || !fields[2].ends_with('%')
            || !fields[4].ends_with('%')
        {
            continue;
        }
        entries.push(PprofTopEntry {
            rank: entries.len() + 1,
            flat: fields[0].to_string(),
            flat_percent: parse_percent(fields[1]),
            sum_percent: parse_percent(fields[2]),
            cum: fields[3].to_string(),
            cum_percent: parse_percent(fields[4]),
            function: fields[5..].join(" "),
        });
    }
    (profile_type, total, entries)
}

fn parse_percent(value: &str) -> Option<f64> {
    value.trim_end_matches('%').parse::<f64>().ok()
}

fn truncate_bytes(value: &[u8], max_bytes: usize) -> Vec<u8> {
    if value.len() <= max_bytes {
        value.to_vec()
    } else {
        value[..max_bytes].to_vec()
    }
}

fn write_bytes(path: &Path, content: &[u8]) -> Result<(), AppError> {
    fs::write(path, content)
        .map_err(|err| AppError::internal(format!("failed to write {}: {err}", path.display())))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), AppError> {
    fs::write(
        path,
        serde_json::to_vec_pretty(value)
            .map_err(|err| AppError::internal(format!("failed to encode JSON: {err}")))?,
    )
    .map_err(|err| AppError::internal(format!("failed to write {}: {err}", path.display())))
}

fn default_sample_index() -> String {
    "samples".to_string()
}

fn default_node_count() -> usize {
    50
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pprof_top_text() {
        let text = r#"File: sample
Type: cpu
Showing nodes accounting for 970ms, 100% of 970ms total
      flat  flat%   sum%        cum   cum%
     490ms 50.52% 50.52%      900ms 92.78%  pkg.hot
     120ms 12.37% 62.89%      120ms 12.37%  runtime.morestack
"#;

        let (profile_type, total, entries) = parse_top_text(text);

        assert_eq!(profile_type, "cpu");
        assert_eq!(total.as_deref(), Some("970ms"));
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].function, "pkg.hot");
        assert_eq!(entries[0].flat_percent, Some(50.52));
    }

    #[test]
    fn validates_pprof_params() {
        let params = parse_pprof_params(&serde_json::json!({
            "sampleIndex": "inuse_space",
            "nodeCount": 500,
            "generateSvg": true
        }))
        .unwrap();
        assert_eq!(params.sample_index, "inuse_space");
        assert_eq!(params.node_count, 200);
        assert!(params.generate_svg);

        assert!(parse_pprof_params(&serde_json::json!({"sampleIndex": "bad/value"})).is_err());
    }
}
