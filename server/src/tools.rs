use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::{process::Command, time::Duration};

use crate::{
    config::{AppConfig, ToolSettings},
    error::AppError,
    fs_utils::relative_string,
    models::{TaskRecord, ToolDescriptor},
};

pub const PPROF_ANALYZER_ID: &str = "pprof_analyzer";

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
    vec![pprof_descriptor(config)]
}

pub fn get_descriptor(config: &AppConfig, tool_id: &str) -> Option<ToolDescriptor> {
    (tool_id == PPROF_ANALYZER_ID).then(|| pprof_descriptor(config))
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
        _ => Err(AppError::not_found(format!("unknown toolId {tool_id}"))),
    }
}

pub async fn run_tool_task(config: Arc<AppConfig>, task: TaskRecord) -> Result<PathBuf, AppError> {
    match task.tool_id.as_deref() {
        Some(PPROF_ANALYZER_ID) => run_pprof_task(config, task).await,
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
        output_views: vec![
            "summary".to_string(),
            "top_table".to_string(),
            "tree_text".to_string(),
            "raw_text".to_string(),
            "svg".to_string(),
        ],
    }
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
    Ok(result_path)
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
