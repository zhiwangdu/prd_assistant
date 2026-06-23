use std::{
    collections::BTreeMap,
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
        models::{
            GrepResults, Manifest, TaskKind, TaskRecord, TaskSource, TaskStatus, ToolDescriptor,
            ToolInputIndex, ToolSource, UploadStatus,
        },
    },
    pipeline::{extract_task, prepare_pipeline_run, prepare_raw_snapshot, search_task},
    services::dev_selftest,
    services::fetch::{FetchRunParams, FETCH_TOOL_ID},
    services::gemini_db,
    services::huawei_package_sync::{
        validate_params as validate_huawei_package_sync_params, HUAWEI_PACKAGE_SYNC_TOOL_ID,
    },
    services::metadata::{MetadataFieldTypesRequest, MetadataTagFieldsRequest},
    support::{
        config::{AppConfig, ToolSettings},
        error::AppError,
        fs_utils::relative_string,
        id::next_id,
    },
};

pub const PPROF_ANALYZER_ID: &str = "pprof_analyzer";
pub const PREPROCESS_LOG_PACKAGE_ID: &str = "logagent.preprocess_log_package";
pub const METADATA_LIST_INSTANCES_ID: &str = "logagent.list_metadata_instances";
pub const METADATA_GET_SNAPSHOT_ID: &str = "logagent.get_metadata_snapshot";
pub const METADATA_GET_FIELD_TYPES_ID: &str = "logagent.get_metadata_field_types";
pub const METADATA_GET_TAG_FIELDS_ID: &str = "logagent.get_metadata_tag_fields";
/// Built-in orchestrator: batch upload -> preprocess -> InfluxQL analyzer.
pub const BATCH_INFLUXQL_ANALYSIS_ID: &str = "logagent.batch_influxql_analysis";
/// The configured analyzer binary this orchestrator drives.
const INFLUXQL_ANALYZER_ID: &str = "influxql_analyzer";

/// MCP-native platform tool ids. Side-effect-free run queries served directly from
/// `TaskStore` (no `ToolRun` created) — see `mcp_server::platform_tool_result`.
pub const RUNS_GET_ID: &str = "logagent.runs.get";
pub const RUNS_RESULT_ID: &str = "logagent.runs.result";

fn platform_run_descriptors() -> Vec<ToolDescriptor> {
    let tags = vec![
        "built-in".to_string(),
        "platform".to_string(),
        "runs".to_string(),
        "read-only".to_string(),
    ];
    let params_schema = serde_json::json!({
        "type": "object",
        "properties": { "runId": { "type": "string" } },
        "required": ["runId"]
    });
    let params_template = serde_json::json!({ "runId": "" });
    vec![
        ToolDescriptor {
            tool_id: RUNS_GET_ID.to_string(),
            display_name: "Runs: get status".to_string(),
            description: "Read one run's status by runId. MCP-native and side-effect-free: no run record is created, so polling does not pollute run history. Use after a runMode:'queued' tools/call.".to_string(),
            enabled: true,
            source: ToolSource::BuiltIn,
            read_only: true,
            editable: false,
            exportable: false,
            runnable: false,
            platform: true,
            tags: tags.clone(),
            backend: "platform".to_string(),
            accepted_suffixes: Vec::new(),
            min_files: 0,
            max_files: 0,
            params_schema: params_schema.clone(),
            params_template: params_template.clone(),
            output_views: vec!["json".to_string()],
        },
        ToolDescriptor {
            tool_id: RUNS_RESULT_ID.to_string(),
            display_name: "Runs: get result".to_string(),
            description: "Read one successful run's structured result by runId. MCP-native and side-effect-free: no run record is created.".to_string(),
            enabled: true,
            source: ToolSource::BuiltIn,
            read_only: true,
            editable: false,
            exportable: false,
            runnable: false,
            platform: true,
            tags,
            backend: "platform".to_string(),
            accepted_suffixes: Vec::new(),
            min_files: 0,
            max_files: 0,
            params_schema,
            params_template,
            output_views: vec!["json".to_string()],
        },
    ]
}

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
    descriptors.push(preprocess_log_package_descriptor());
    descriptors.push(batch_influxql_analysis_descriptor(config));
    descriptors.extend(metadata_descriptors());
    descriptors.push(fetch_descriptor(config));
    descriptors.push(huawei_package_sync_descriptor(config));
    descriptors.extend(gemini_db::descriptors(config));
    descriptors.extend(dev_selftest::descriptors(config));
    descriptors.extend(platform_run_descriptors());
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
    if tool_id == PREPROCESS_LOG_PACKAGE_ID {
        return Some(preprocess_log_package_descriptor());
    }
    if tool_id == BATCH_INFLUXQL_ANALYSIS_ID {
        return Some(batch_influxql_analysis_descriptor(config));
    }
    if let Some(descriptor) = gemini_db::get_descriptor(config, tool_id) {
        return Some(descriptor);
    }
    if let Some(descriptor) = dev_selftest::get_descriptor(config, tool_id) {
        return Some(descriptor);
    }
    if tool_id == RUNS_GET_ID || tool_id == RUNS_RESULT_ID {
        return platform_run_descriptors()
            .into_iter()
            .find(|descriptor| descriptor.tool_id == tool_id);
    }
    metadata_descriptors()
        .into_iter()
        .chain([
            fetch_descriptor(config),
            huawei_package_sync_descriptor(config),
        ])
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
        PREPROCESS_LOG_PACKAGE_ID => validate_preprocess_log_package_params(params),
        BATCH_INFLUXQL_ANALYSIS_ID => validate_batch_influxql_params(params),
        METADATA_LIST_INSTANCES_ID => validate_metadata_list_params(params),
        METADATA_GET_SNAPSHOT_ID => validate_metadata_snapshot_params(params),
        METADATA_GET_FIELD_TYPES_ID => validate_metadata_field_types_params(params),
        METADATA_GET_TAG_FIELDS_ID => validate_metadata_tag_fields_params(params),
        FETCH_TOOL_ID => validate_fetch_params(config, params),
        HUAWEI_PACKAGE_SYNC_TOOL_ID => validate_huawei_package_sync_run_params(config, params),
        id if gemini_db::is_gemini_db_tool(id) => {
            gemini_db::validate_run_params(config, id, params)
        }
        id if dev_selftest::is_dev_selftest_tool(id) => {
            dev_selftest::validate_run_params(config, id, params)
        }
        _ if config.tools.tools.contains_key(tool_id) => validate_configured_tool_params(params),
        _ => Err(AppError::not_found(format!("unknown toolId {tool_id}"))),
    }
}

/// Build a queued `TaskKind::ToolRun` record for `tool_id` from already-validated
/// upload ids and params. Shared by the HTTP `POST /api/tools/:id/runs` path (which
/// then enqueues it) and the MCP `tools/call` path (which runs it synchronously).
pub async fn build_tool_run_task(
    state: &Arc<AppState>,
    tool_id: &str,
    upload_ids: Vec<String>,
    params: &serde_json::Value,
) -> Result<TaskRecord, AppError> {
    let normalized_params =
        validate_tool_run_request(&state.config, tool_id, upload_ids.len(), params)?;
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
    Ok(TaskRecord {
        schema_version: 6,
        task_id,
        alias: None,
        session_id: None,
        task_kind: TaskKind::ToolRun,
        source: TaskSource::Upload,
        upload_ids,
        inputs,
        source_url: None,
        tool_id: Some(tool_id.to_string()),
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
    })
}

pub async fn run_tool_task(state: Arc<AppState>, task: TaskRecord) -> Result<PathBuf, AppError> {
    match task.tool_id.as_deref() {
        Some(PPROF_ANALYZER_ID) => run_pprof_task(state.config.clone(), task).await,
        Some(PREPROCESS_LOG_PACKAGE_ID) => run_preprocess_log_package_task(state, task).await,
        Some(BATCH_INFLUXQL_ANALYSIS_ID) => run_batch_influxql_analysis_task(state, task).await,
        Some(
            METADATA_LIST_INSTANCES_ID
            | METADATA_GET_SNAPSHOT_ID
            | METADATA_GET_FIELD_TYPES_ID
            | METADATA_GET_TAG_FIELDS_ID,
        ) => run_metadata_task(state, task).await,
        Some(FETCH_TOOL_ID) => crate::services::fetch::run_fetch_task(state, task).await,
        Some(HUAWEI_PACKAGE_SYNC_TOOL_ID) => {
            crate::services::huawei_package_sync::run_huawei_package_sync_task(state, task).await
        }
        Some(tool_id) if gemini_db::is_gemini_db_tool(tool_id) => {
            gemini_db::run_gemini_db_task(state, task).await
        }
        Some(tool_id) if dev_selftest::is_dev_selftest_tool(tool_id) => {
            dev_selftest::run_dev_selftest_task(state, task).await
        }
        Some(tool_id) if state.config.tools.tools.contains_key(tool_id) => {
            run_configured_tool_task(state, task).await
        }
        Some(tool_id) => Err(AppError::bad_request(format!("unknown toolId {tool_id}"))),
        None => Err(AppError::bad_request("tool run task is missing toolId")),
    }
}

fn preprocess_log_package_descriptor() -> ToolDescriptor {
    ToolDescriptor {
        tool_id: PREPROCESS_LOG_PACKAGE_ID.to_string(),
        platform: false,
        display_name: "Log package preprocessor".to_string(),
        description:
            "Expand node log packages, normalize rotated logs, and materialize analyzer inputs."
                .to_string(),
        enabled: true,
        source: ToolSource::BuiltIn,
        read_only: true,
        editable: false,
        exportable: false,
        runnable: true,
        tags: vec![
            "built-in".to_string(),
            "log".to_string(),
            "preprocess".to_string(),
            "manual-run".to_string(),
        ],
        backend: "builtin".to_string(),
        accepted_suffixes: vec![".tar.gz".to_string()],
        min_files: 1,
        max_files: 100,
        params_schema: serde_json::json!({
            "type": "object",
            "properties": {}
        }),
        params_template: serde_json::json!({}),
        output_views: vec![
            "summary".to_string(),
            "nodes".to_string(),
            "log_groups".to_string(),
            "tool_inputs".to_string(),
            "warnings".to_string(),
        ],
    }
}

fn batch_influxql_analysis_descriptor(config: &AppConfig) -> ToolDescriptor {
    let influxql_enabled = config
        .tools
        .tools
        .get(INFLUXQL_ANALYZER_ID)
        .map(|tool| tool.enabled)
        .unwrap_or(false);
    ToolDescriptor {
        tool_id: BATCH_INFLUXQL_ANALYSIS_ID.to_string(),
        platform: false,
        display_name: "Batch InfluxQL log analysis".to_string(),
        description: "Upload a batch of node log packages, unpack + preprocess, and run the InfluxQL analyzer across every materialized query input."
            .to_string(),
        enabled: influxql_enabled,
        source: ToolSource::BuiltIn,
        read_only: true,
        editable: false,
        exportable: false,
        runnable: influxql_enabled,
        tags: vec![
            "built-in".to_string(),
            "log".to_string(),
            "influxql".to_string(),
            "batch".to_string(),
        ],
        backend: "builtin".to_string(),
        accepted_suffixes: vec![
            ".tar.gz".to_string(),
            ".tgz".to_string(),
            ".tar".to_string(),
        ],
        min_files: 1,
        max_files: 100,
        params_schema: serde_json::json!({
            "type": "object",
            "properties": {}
        }),
        params_template: serde_json::json!({}),
        output_views: vec![
            "summary".to_string(),
            "preprocess".to_string(),
            "findings".to_string(),
            "warnings".to_string(),
        ],
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
        platform: false,
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
        platform: false,
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
            platform: false,
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
            platform: false,
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
            platform: false,
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
            platform: false,
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
        platform: false,
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

fn huawei_package_sync_descriptor(config: &AppConfig) -> ToolDescriptor {
    let enabled = config.huawei_cloud.package_sync.enabled;
    ToolDescriptor {
        tool_id: HUAWEI_PACKAGE_SYNC_TOOL_ID.to_string(),
        platform: false,
        display_name: "Huawei OBS + GaussDB Package Sync".to_string(),
        description:
            "Upload one package to Huawei OBS, execute a GaussDB update SQL, then query OBS/GaussDB summary."
                .to_string(),
        enabled,
        source: ToolSource::BuiltIn,
        read_only: false,
        editable: false,
        exportable: false,
        runnable: enabled,
        tags: vec![
            "built-in".to_string(),
            "huawei-cloud".to_string(),
            "obs".to_string(),
            "gaussdb".to_string(),
            "manual-run".to_string(),
        ],
        backend: "huawei_cloud_package_sync".to_string(),
        accepted_suffixes: vec!["*".to_string()],
        min_files: 1,
        max_files: 1,
        params_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "objectKey": {
                    "type": "string",
                    "description": "Optional OBS object key. Leave blank to use configured prefix plus uploaded filename."
                },
                "updateSql": {
                    "type": "string",
                    "description": "GaussDB SQL executed after OBS PUT."
                },
                "querySql": {
                    "type": "string",
                    "description": "GaussDB query SQL executed after OBS HEAD."
                }
            },
            "required": ["updateSql", "querySql"]
        }),
        params_template: serde_json::json!({
            "objectKey": "",
            "updateSql": "",
            "querySql": ""
        }),
        output_views: vec![
            "summary".to_string(),
            "obs".to_string(),
            "gaussdb".to_string(),
            "json".to_string(),
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

fn validate_preprocess_log_package_params(
    value: &serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    validate_metadata_list_params(value)
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

fn validate_huawei_package_sync_run_params(
    config: &AppConfig,
    value: &serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    if !config.huawei_cloud.package_sync.enabled {
        return Err(AppError::bad_request(
            "Huawei package sync is disabled by server config",
        ));
    }
    let params = validate_huawei_package_sync_params(value)?;
    serde_json::to_value(params).map_err(|err| {
        AppError::internal(format!(
            "failed to encode Huawei package sync params: {err}"
        ))
    })
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

async fn run_preprocess_log_package_task(
    state: Arc<AppState>,
    task: TaskRecord,
) -> Result<PathBuf, AppError> {
    validate_preprocess_log_package_params(&task.tool_params)?;
    let workspace = state.config.storage.workspace_dir(&task.task_id);
    let started = Instant::now();
    prepare_pipeline_run(&workspace).await?;
    extract_task(state.config.clone(), task.clone()).await?;

    let manifest: Manifest = read_json_file_sync(&workspace.join("manifest.json"))?;
    let tool_input_index = manifest
        .tool_inputs_path
        .as_deref()
        .and_then(|path| read_json_file_sync::<ToolInputIndex>(&workspace.join(path)).ok());

    let mut nodes = BTreeMap::<String, serde_json::Value>::new();
    for upload in &manifest.uploads {
        let Some(node_id) = upload.node_id.as_deref() else {
            continue;
        };
        let entry = nodes.entry(node_id.to_string()).or_insert_with(|| {
            serde_json::json!({
                "nodeId": node_id,
                "instanceIds": [],
                "packages": 0_u64,
                "timestamps": [],
                "logGroups": {},
                "ignoredFileCount": 0_u64,
                "warnings": []
            })
        });
        entry["packages"] = serde_json::json!(entry["packages"].as_u64().unwrap_or(0) + 1);
        if let Some(instance_id) = upload.instance_id.as_deref() {
            push_json_string_unique(&mut entry["instanceIds"], instance_id);
        }
        if let Some(timestamp) = upload.package_timestamp.as_deref() {
            push_json_string_unique(&mut entry["timestamps"], timestamp);
        }
        entry["ignoredFileCount"] = serde_json::json!(
            entry["ignoredFileCount"].as_u64().unwrap_or(0) + upload.ignored_file_count
        );
        for warning in &upload.warnings {
            push_json_string_unique(&mut entry["warnings"], warning);
        }
        for group in &upload.log_groups {
            if !entry["logGroups"].is_object() {
                entry["logGroups"] = serde_json::json!({});
            }
            let groups = entry["logGroups"]
                .as_object_mut()
                .expect("logGroups object");
            let group_entry = groups.entry(group.name.clone()).or_insert_with(|| {
                serde_json::json!({
                    "fileCount": 0_u64,
                    "compressedFileCount": 0_u64
                })
            });
            group_entry["fileCount"] = serde_json::json!(
                group_entry["fileCount"].as_u64().unwrap_or(0) + group.file_count
            );
            group_entry["compressedFileCount"] = serde_json::json!(
                group_entry["compressedFileCount"].as_u64().unwrap_or(0)
                    + group.compressed_file_count
            );
        }
    }

    let tool_inputs = tool_input_index
        .as_ref()
        .map(|index| {
            index
                .inputs
                .iter()
                .map(|input| {
                    serde_json::json!({
                        "path": input.path,
                        "inputKind": input.input_kind,
                        "scope": input.scope,
                        "toolIds": input.tool_ids,
                        "nodeId": input.node_id,
                        "instanceId": input.instance_id,
                        "packageTimestamp": input.package_timestamp,
                        "logGroup": input.log_group,
                        "recordCount": input.record_count,
                        "sourceFiles": input.source_files,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let action_id = format!("act_tool_preprocess_{}", task.task_id);
    let result_dir = workspace.join("tool_results").join(&action_id);
    fs::create_dir_all(&result_dir)
        .map_err(|err| AppError::internal(format!("failed to create tool result dir: {err}")))?;
    let result = serde_json::json!({
        "schemaVersion": 1,
        "toolId": PREPROCESS_LOG_PACKAGE_ID,
        "actionId": action_id,
        "status": "OK",
        "summary": format!(
            "preprocessed {} upload(s), {} node(s), {} extracted file(s), {} materialized tool input(s)",
            manifest.uploads.len(),
            nodes.len(),
            manifest.files.len(),
            tool_inputs.len()
        ),
        "manifestPath": "manifest.json",
        "toolInputsPath": manifest.tool_inputs_path,
        "nodes": nodes.into_values().collect::<Vec<_>>(),
        "toolInputs": tool_inputs,
        "durationMs": started.elapsed().as_millis(),
        "createdAt": Utc::now()
    });
    let result_path = result_dir.join("result.json");
    write_json(&result_path, &result)?;
    Ok(result_path)
}

fn validate_batch_influxql_params(
    params: &serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    if !params.is_object() {
        return Err(AppError::bad_request(
            "batch influxql analysis params must be a JSON object",
        ));
    }
    Ok(serde_json::json!({}))
}

/// One run: unpack + preprocess a batch of log packages, then run the InfluxQL
/// analyzer on every materialized `tool_inputs/influxql_analyzer/*.jsonl` input.
/// Reuses `extract_task` (preprocess) and `tool_runner.execute` (analyzer binary).
async fn run_batch_influxql_analysis_task(
    state: Arc<AppState>,
    task: TaskRecord,
) -> Result<PathBuf, AppError> {
    // The analyzer binary must be configured and enabled; the batch tool mirrors
    // its enabled state in the descriptor, but re-check here for direct callers.
    state
        .config
        .tools
        .tools
        .get(INFLUXQL_ANALYZER_ID)
        .filter(|tool| tool.enabled)
        .ok_or_else(|| AppError::bad_request("influxql_analyzer is not configured or enabled"))?;
    validate_batch_influxql_params(&task.tool_params)?;
    let workspace = state.config.storage.workspace_dir(&task.task_id);
    let started = Instant::now();
    prepare_pipeline_run(&workspace).await?;
    extract_task(state.config.clone(), task.clone()).await?;

    let manifest: Manifest = read_json_file_sync(&workspace.join("manifest.json"))?;
    let tool_input_index = manifest
        .tool_inputs_path
        .as_deref()
        .and_then(|path| read_json_file_sync::<ToolInputIndex>(&workspace.join(path)).ok());
    let influxql_inputs: Vec<_> = tool_input_index
        .as_ref()
        .map(|index| {
            index
                .inputs
                .iter()
                .filter(|input| {
                    input
                        .tool_ids
                        .iter()
                        .any(|tool_id| tool_id == INFLUXQL_ANALYZER_ID)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    const MAX_INPUTS: usize = 200;
    let mut warnings = Vec::new();
    if influxql_inputs.is_empty() {
        warnings
            .push("no InfluxQL query inputs materialized from the uploaded packages".to_string());
    }
    if influxql_inputs.len() > MAX_INPUTS {
        warnings.push(format!(
            "more than {MAX_INPUTS} InfluxQL inputs; analyzing the first {MAX_INPUTS}"
        ));
    }

    let context = TaskContext::from_record(&task, workspace.clone(), None);
    let mut findings = Vec::new();
    let mut failed = 0usize;
    for input in influxql_inputs.iter().take(MAX_INPUTS) {
        let action = AgentAction {
            schema_version: 1,
            action_id: format!(
                "act_tool_batch_influxql_{}_{}",
                task.task_id,
                stable_hash_hex(&format!("{}:{}", task.task_id, input.path))
            ),
            kind: ActionKind::RunTool,
            reason: "batch influxql analysis".to_string(),
            evidence_refs: Vec::new(),
            input: serde_json::json!({
                "tool": INFLUXQL_ANALYZER_ID,
                "inputFile": input.path,
            }),
            risk: ActionRisk::SafeReadOnly,
            fingerprint: format!("batch_influxql:{}:{}", task.task_id, input.path),
        };
        match state.tool_runner.execute(&context, &action).await {
            Ok(artifact) => {
                let output: serde_json::Value =
                    read_json_file_sync(&workspace.join(&artifact.artifact_path))?;
                findings.push(serde_json::json!({
                    "inputFile": input.path,
                    "nodeId": input.node_id,
                    "instanceId": input.instance_id,
                    "packageTimestamp": input.package_timestamp,
                    "artifactPath": artifact.artifact_path,
                    "summary": artifact.summary,
                    "result": output,
                }));
            }
            Err(err) => {
                failed += 1;
                warnings.push(format!(
                    "influxql analyzer failed for {}: {err:#}",
                    input.path
                ));
            }
        }
    }

    let nodes = influxql_inputs
        .iter()
        .filter_map(|input| input.node_id.as_deref())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let status = if findings.is_empty() {
        "FAILED"
    } else if failed > 0 {
        "PARTIAL"
    } else {
        "OK"
    };
    let action_id = format!("act_tool_batch_influxql_{}", task.task_id);
    let result_dir = workspace.join("tool_results").join(&action_id);
    fs::create_dir_all(&result_dir)
        .map_err(|err| AppError::internal(format!("failed to create tool result dir: {err}")))?;
    let result = serde_json::json!({
        "schemaVersion": 1,
        "toolId": BATCH_INFLUXQL_ANALYSIS_ID,
        "actionId": action_id,
        "status": status,
        "preprocessSummary": {
            "uploads": manifest.uploads.len(),
            "extractedFiles": manifest.files.len(),
            "influxqlInputs": influxql_inputs.len(),
            "nodes": nodes,
        },
        "analyzedInputs": findings.len(),
        "failedCount": failed,
        "findings": findings,
        "warnings": warnings,
        "durationMs": started.elapsed().as_millis(),
        "createdAt": Utc::now(),
    });
    let result_path = result_dir.join("result.json");
    write_json(&result_path, &result)?;
    info!(
        task_id = %task.task_id,
        action_id = %action_id,
        status = status,
        inputs = influxql_inputs.len(),
        findings = findings.len(),
        failed = failed,
        duration_ms = started.elapsed().as_millis(),
        "batch influxql analysis task completed"
    );
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
            .rule_based_actions(&workspace, &manifest, &grep)
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
        if !trimmed.starts_with("extracted/") && !trimmed.starts_with("tool_inputs/") {
            return Err(AppError::bad_request(
                "params.inputFiles entries must be extracted/ or tool_inputs/ relative paths",
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

fn push_json_string_unique(array_value: &mut serde_json::Value, value: &str) {
    if !array_value.is_array() {
        *array_value = serde_json::json!([]);
    }
    let array = array_value.as_array_mut().expect("array value");
    if !array
        .iter()
        .any(|existing| existing.as_str() == Some(value))
    {
        array.push(serde_json::json!(value));
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

    use crate::support::config::{
        AuthSettings, FetchSettings, HuaweiCloudSettings, LogAnalyzerSettings, McpSettings,
        RemoteExecutionSettings, ServerSettings, SkillSettings, StorageSettings, ToolsSettings,
    };

    fn test_app_config() -> AppConfig {
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
            dev_selftest: crate::support::config::DevSelftestSettings::default(),
        }
    }

    fn config_with_influxql(enabled: bool) -> AppConfig {
        let mut config = test_app_config();
        config.tools.tools.insert(
            INFLUXQL_ANALYZER_ID.to_string(),
            ToolSettings {
                name: INFLUXQL_ANALYZER_ID.to_string(),
                enabled,
                path: PathBuf::from("/dev/null/influxql-analyzer"),
                timeout_seconds: 30,
                max_output_bytes: 1024,
                max_input_files: 3,
                args: vec![
                    "-input".to_string(),
                    "{input_file}".to_string(),
                    "-output".to_string(),
                    "json".to_string(),
                ],
                match_settings: crate::support::config::ToolMatchSettings::default(),
            },
        );
        config
    }

    #[test]
    fn batch_influxql_descriptor_disabled_when_influxql_absent() {
        let descriptor = batch_influxql_analysis_descriptor(&test_app_config());
        assert_eq!(descriptor.tool_id, BATCH_INFLUXQL_ANALYSIS_ID);
        assert!(!descriptor.enabled);
        assert!(!descriptor.runnable);
    }

    #[test]
    fn batch_influxql_descriptor_disabled_when_influxql_disabled() {
        let descriptor = batch_influxql_analysis_descriptor(&config_with_influxql(false));
        assert!(!descriptor.enabled);
        assert!(!descriptor.runnable);
    }

    #[test]
    fn batch_influxql_descriptor_enabled_when_influxql_enabled() {
        let descriptor = batch_influxql_analysis_descriptor(&config_with_influxql(true));
        assert!(descriptor.enabled);
        assert!(descriptor.runnable);
        assert!(matches!(descriptor.source, ToolSource::BuiltIn));
        assert!(descriptor.tags.contains(&"influxql".to_string()));
    }

    #[test]
    fn batch_influxql_descriptor_listed_in_catalog() {
        let config = config_with_influxql(true);
        let ids: Vec<String> = descriptors(&config)
            .into_iter()
            .map(|descriptor| descriptor.tool_id)
            .collect();
        assert!(ids.contains(&BATCH_INFLUXQL_ANALYSIS_ID.to_string()));
        // get_descriptor resolves it too.
        assert!(get_descriptor(&config, BATCH_INFLUXQL_ANALYSIS_ID).is_some());
    }

    #[test]
    fn validates_batch_influxql_params() {
        assert!(validate_batch_influxql_params(&serde_json::json!({})).is_ok());
        assert!(validate_batch_influxql_params(&serde_json::json!({"x": 1})).is_ok());
        assert!(validate_batch_influxql_params(&serde_json::json!([1, 2])).is_err());
        assert!(validate_batch_influxql_params(&serde_json::json!("oops")).is_err());
    }
}
