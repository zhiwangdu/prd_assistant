//! Dev self-test pipeline built-in tool group (P1: docker self-test closed loop).
//!
//! Drives a multi-step run — sync -> build -> deploy -> run_tests -> report —
//! shared across separate tool calls via a persistent run workspace
//! (`data/dev_selftest/runs/{run_id}/`) plus a `DevSelftestRunRecord` index.
//! Like the other catalog tools, each call is a `ToolRun` through the shared
//! `build_tool_run_task` + `run_tool_task` boundary, so the group auto-appears in
//! `/api/tools`, MCP `tools/list`, and the WebUI catalog.
//!
//! P1 implements: tarball/git source sync, configured build, `docker_cluster`
//! deploy, a **stub** test runner (the real executor-dispatched test framework is
//! external code, landed in P2), and a rule-based report. All commands/binaries/
//! paths/compose files come from the `dev_selftest` config allowlist; tool params
//! only select profile ids and carry a `runId`.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::{process::Command, time::timeout};
use tracing::warn;

use crate::{
    app::AppState,
    domain::models::{
        DevSelftestDeployTarget, DevSelftestRunRecord, DevSelftestRunStatus, DevSelftestStep,
        TaskRecord, ToolDescriptor, ToolSource,
    },
    services::remote_execution::{self, ExecutorRunInput, ExecutorRunStatus, ExecutorTarget},
    support::{
        config::{
            AppConfig, DevSelftestBuildProfile, DevSelftestSettings, DevSelftestTestDocker,
            DevSelftestTestSuite,
        },
        error::AppError,
        fs_utils::{relative_string, safe_join, sanitize_filename},
        id::next_id,
    },
};

pub const SYNC_WORKSPACE_ID: &str = "logagent.dev_selftest.sync_workspace";
pub const BUILD_ID: &str = "logagent.dev_selftest.build";
pub const DEPLOY_ID: &str = "logagent.dev_selftest.deploy";
pub const RUN_TESTS_ID: &str = "logagent.dev_selftest.run_tests";
pub const REPORT_ID: &str = "logagent.dev_selftest.report";

const PROGRESS_FILE: &str = "progress.json";

pub fn descriptors(config: &AppConfig) -> Vec<ToolDescriptor> {
    let enabled = config.dev_selftest.enabled;
    vec![
        sync_workspace_descriptor(enabled),
        build_descriptor(enabled),
        deploy_descriptor(enabled),
        run_tests_descriptor(enabled),
        report_descriptor(enabled),
    ]
}

pub fn get_descriptor(config: &AppConfig, tool_id: &str) -> Option<ToolDescriptor> {
    descriptors(config)
        .into_iter()
        .find(|d| d.tool_id == tool_id)
}

pub fn is_dev_selftest_tool(tool_id: &str) -> bool {
    matches!(
        tool_id,
        SYNC_WORKSPACE_ID | BUILD_ID | DEPLOY_ID | RUN_TESTS_ID | REPORT_ID
    )
}

pub fn validate_run_params(
    config: &AppConfig,
    tool_id: &str,
    value: &Value,
) -> Result<Value, AppError> {
    if !config.dev_selftest.enabled {
        return Err(AppError::bad_request(
            "dev_selftest is disabled by server config",
        ));
    }
    let normalized = match tool_id {
        SYNC_WORKSPACE_ID => {
            let params: SyncWorkspaceParams = parse_params(value)?;
            if let (Some(repo), Some(git_ref)) =
                (params.git_repo.as_deref(), params.git_ref.as_deref())
            {
                if !config.dev_selftest.git.enabled {
                    return Err(AppError::bad_request("dev_selftest.git is disabled"));
                }
                if !git_repo_allowed(config, repo, git_ref) {
                    return Err(AppError::bad_request(
                        "git repo/ref is not in the configured allowlist",
                    ));
                }
            }
            serde_json::to_value(params)
        }
        BUILD_ID => {
            let params: BuildParams = parse_params(value)?;
            require_profile(config, &params.build_profile, ProfileKind::Build)?;
            serde_json::to_value(params)
        }
        DEPLOY_ID => {
            let params: DeployParams = parse_params(value)?;
            require_profile(config, &params.profile, ProfileKind::Docker)?;
            serde_json::to_value(params)
        }
        RUN_TESTS_ID => {
            let params: RunTestsParams = parse_params(value)?;
            require_profile(config, &params.test_suite, ProfileKind::Test)?;
            serde_json::to_value(params)
        }
        REPORT_ID => {
            let params: ReportParams = parse_params(value)?;
            serde_json::to_value(params)
        }
        _ => return Err(AppError::not_found(format!("unknown toolId {tool_id}"))),
    }
    .map_err(|err| AppError::internal(format!("failed to encode dev_selftest params: {err}")))?;
    Ok(normalized)
}

pub async fn run_dev_selftest_task(
    state: Arc<AppState>,
    task: TaskRecord,
) -> Result<PathBuf, AppError> {
    let tool_id = task
        .tool_id
        .as_deref()
        .ok_or_else(|| AppError::bad_request("tool run task is missing toolId"))?
        .to_string();
    if !state.config.dev_selftest.enabled {
        return Err(AppError::bad_request(
            "dev_selftest is disabled by server config",
        ));
    }
    match tool_id.as_str() {
        SYNC_WORKSPACE_ID => run_sync_workspace(state, task).await,
        BUILD_ID => run_build(state, task).await,
        DEPLOY_ID => run_deploy(state, task).await,
        RUN_TESTS_ID => run_run_tests(state, task).await,
        REPORT_ID => run_report(state, task).await,
        _ => Err(AppError::bad_request(format!("unknown toolId {tool_id}"))),
    }
}

// ---------- params ----------

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncWorkspaceParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    upload_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    git_repo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    git_ref: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct BuildParams {
    run_id: String,
    build_profile: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeployParams {
    run_id: String,
    profile: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RunTestsParams {
    run_id: String,
    test_suite: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReportParams {
    run_id: String,
}

enum ProfileKind {
    Build,
    Docker,
    Test,
}

fn parse_params<T: serde::de::DeserializeOwned>(value: &Value) -> Result<T, AppError> {
    serde_json::from_value(value.clone())
        .map_err(|err| AppError::bad_request(format!("invalid dev_selftest params: {err}")))
}

fn require_profile(config: &AppConfig, id: &str, kind: ProfileKind) -> Result<(), AppError> {
    let exists = match kind {
        ProfileKind::Build => config.dev_selftest.builds.contains_key(id),
        ProfileKind::Docker => config.dev_selftest.docker.clusters.contains_key(id),
        ProfileKind::Test => config.dev_selftest.test_suites.contains_key(id),
    };
    if exists {
        Ok(())
    } else {
        Err(AppError::bad_request(format!(
            "unknown dev_selftest profile {id}"
        )))
    }
}

fn git_repo_allowed(config: &AppConfig, repo: &str, git_ref: &str) -> bool {
    config
        .dev_selftest
        .git
        .repos
        .iter()
        .any(|allowed| allowed.url == repo && allowed.refs.iter().any(|r| r == git_ref))
}

// ---------- run workspace + progress ----------

fn run_dir(state: &AppState, run_id: &str) -> PathBuf {
    state.config.storage.dev_selftest_run_dir(run_id)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    #[serde(default = "default_progress_schema")]
    schema_version: u32,
    run_id: String,
    steps: Vec<DevSelftestStep>,
}

fn default_progress_schema() -> u32 {
    1
}

fn read_progress(run_dir: &Path) -> Progress {
    match fs::read_to_string(run_dir.join(PROGRESS_FILE)) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_else(|_| Progress {
            schema_version: 1,
            run_id: String::new(),
            steps: Vec::new(),
        }),
        Err(_) => Progress {
            schema_version: 1,
            run_id: String::new(),
            steps: Vec::new(),
        },
    }
}

fn append_step(state: &AppState, run_id: &str, step: DevSelftestStep) -> Result<(), AppError> {
    let dir = run_dir(state, run_id);
    let mut progress = read_progress(&dir);
    if progress.run_id.is_empty() {
        progress.run_id = run_id.to_string();
    }
    // Drop a prior entry for the same step so re-runs replace instead of duplicating.
    progress.steps.retain(|entry| entry.step != step.step);
    progress.steps.push(step);
    write_json_sync(&dir.join(PROGRESS_FILE), &progress)?;
    Ok(())
}

fn new_step(
    name: &str,
    status: &str,
    duration_ms: u128,
    error: Option<String>,
    evidence: Vec<String>,
) -> DevSelftestStep {
    DevSelftestStep {
        step: name.to_string(),
        status: status.to_string(),
        duration_ms,
        error,
        evidence_refs: evidence,
        started_at: Utc::now(),
    }
}

fn write_tool_result(
    workspace: &Path,
    action_id: &str,
    value: &Value,
) -> Result<PathBuf, AppError> {
    let result_dir = workspace.join("tool_results").join(action_id);
    fs::create_dir_all(&result_dir)
        .map_err(|err| AppError::internal(format!("failed to create result dir: {err}")))?;
    let result_path = result_dir.join("result.json");
    write_json_sync(&result_path, value)?;
    Ok(result_path)
}

fn task_action_id(tool_id: &str, task_id: &str) -> String {
    format!(
        "act_dev_selftest_{}_{}",
        tool_id.rsplit('.').next().unwrap_or(tool_id),
        task_id
    )
}

// ---------- bounded command runner ----------

#[derive(Debug, Clone)]
struct BoundedRun {
    ok: bool,
    exit_code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    #[allow(dead_code)]
    duration_ms: u128,
    error: Option<String>,
}

async fn run_bounded_command(
    binary: &Path,
    argv: &[String],
    cwd: &Path,
    env: &BTreeMap<String, String>,
    timeout_secs: u64,
    max_output_bytes: usize,
) -> BoundedRun {
    let started = Instant::now();
    let mut command = Command::new(binary);
    command.kill_on_drop(true);
    command.current_dir(cwd);
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());
    for (key, value) in env {
        command.env(key, value);
    }
    for arg in argv {
        command.arg(arg);
    }
    let child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            return BoundedRun {
                ok: false,
                exit_code: None,
                stdout: Vec::new(),
                stderr: err.to_string().into_bytes(),
                duration_ms: started.elapsed().as_millis(),
                error: Some(format!("failed to spawn {}: {err}", binary.display())),
            }
        }
    };
    match timeout(
        Duration::from_secs(timeout_secs.max(1)),
        child.wait_with_output(),
    )
    .await
    {
        Ok(Ok(output)) => BoundedRun {
            ok: output.status.success(),
            exit_code: output.status.code(),
            stdout: truncate(output.stdout, max_output_bytes),
            stderr: truncate(output.stderr, max_output_bytes),
            duration_ms: started.elapsed().as_millis(),
            error: None,
        },
        Ok(Err(err)) => BoundedRun {
            ok: false,
            exit_code: None,
            stdout: Vec::new(),
            stderr: err.to_string().into_bytes(),
            duration_ms: started.elapsed().as_millis(),
            error: Some(err.to_string()),
        },
        Err(_) => BoundedRun {
            ok: false,
            exit_code: None,
            stdout: Vec::new(),
            stderr: format!("command timed out after {timeout_secs}s").into_bytes(),
            duration_ms: started.elapsed().as_millis(),
            error: Some(format!("command timed out after {timeout_secs}s")),
        },
    }
}

fn truncate(mut bytes: Vec<u8>, max: usize) -> Vec<u8> {
    if bytes.len() > max {
        bytes.truncate(max);
    }
    bytes
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    fs::write(path, bytes)
        .map_err(|err| AppError::internal(format!("failed to write {}: {err}", path.display())))
}

fn write_json_sync<T: Serialize>(path: &Path, value: &T) -> Result<(), AppError> {
    let encoded = serde_json::to_vec_pretty(value)
        .map_err(|err| AppError::internal(format!("failed to encode json: {err}")))?;
    fs::write(path, encoded)
        .map_err(|err| AppError::internal(format!("failed to write {}: {err}", path.display())))
}

// ---------- tools ----------

async fn run_sync_workspace(state: Arc<AppState>, task: TaskRecord) -> Result<PathBuf, AppError> {
    let params: SyncWorkspaceParams = parse_params(&task.tool_params)?;
    let settings = &state.config.dev_selftest;
    let now = Utc::now();

    let (run_id, mut record) = match params.run_id.as_deref() {
        Some(existing) => {
            validate_run_id(existing)?;
            let record = state
                .dev_selftest
                .get(existing)
                .await
                .ok_or_else(|| AppError::bad_request(format!("unknown runId {existing}")))?;
            (existing.to_string(), record)
        }
        None => {
            let run_id = next_id("devselftest");
            let dir = run_dir(&state, &run_id);
            fs::create_dir_all(dir.join("source"))
                .map_err(|err| AppError::internal(format!("failed to create run dir: {err}")))?;
            let record = DevSelftestRunRecord {
                schema_version: 1,
                run_id: run_id.clone(),
                label: params.label.clone(),
                source_ref: None,
                build_artifacts: Vec::new(),
                deploy_target: None,
                test_run_id: None,
                steps: Vec::new(),
                status: DevSelftestRunStatus::Running,
                created_at: now,
                updated_at: now,
            };
            state
                .dev_selftest
                .create(record.clone())
                .await
                .map_err(|err| AppError::internal(format!("failed to persist run: {err}")))?;
            (run_id, record)
        }
    };

    let started = Instant::now();
    let source_dir = run_dir(&state, &run_id).join("source");
    fs::create_dir_all(&source_dir)
        .map_err(|err| AppError::internal(format!("failed to create source dir: {err}")))?;
    let (source_ref, status, error) = if let Some(upload_id) = params.upload_id.as_deref() {
        let upload = state
            .uploads
            .get(upload_id)
            .await
            .ok_or_else(|| AppError::bad_request(format!("unknown uploadId {upload_id}")))?;
        let analyzer =
            crate::services::log_analyzer::LogAnalyzer::new(state.config.log_analyzer.clone());
        analyzer
            .extract_upload(&upload.path, &source_dir, None)
            .map_err(|err| AppError::internal(format!("failed to unpack source: {err}")))?;
        (format!("upload:{}", upload.filename), "OK", None::<String>)
    } else if let (Some(repo), Some(git_ref)) =
        (params.git_repo.as_deref(), params.git_ref.as_deref())
    {
        match git_clone(&settings, repo, git_ref, &source_dir).await {
            Ok(()) => (format!("git:{repo}@{git_ref}"), "OK", None::<String>),
            Err(err) => (String::new(), "FAILED", Some(err)),
        }
    } else {
        ("empty".to_string(), "OK", None::<String>)
    };
    let duration = started.elapsed().as_millis();

    record.source_ref = Some(source_ref.clone());
    let _ = state
        .dev_selftest
        .update(&run_id, |rec| {
            rec.source_ref = Some(source_ref.clone());
            Ok(())
        })
        .await;

    append_step(
        &state,
        &run_id,
        new_step(
            "sync_workspace",
            status,
            duration,
            error.clone(),
            vec!["source/".to_string()],
        ),
    )?;
    if status == "FAILED" {
        mark_failed(&state, &run_id).await;
    }

    let action_id = task_action_id(SYNC_WORKSPACE_ID, &task.task_id);
    let result = json!({
        "schemaVersion": 1,
        "toolId": SYNC_WORKSPACE_ID,
        "actionId": action_id,
        "runId": run_id,
        "sourceRef": source_ref,
        "status": status,
        "error": error,
        "durationMs": duration,
        "createdAt": Utc::now(),
    });
    write_tool_result(
        &state.config.storage.workspace_dir(&task.task_id),
        &action_id,
        &result,
    )
}

async fn git_clone(
    settings: &DevSelftestSettings,
    repo: &str,
    git_ref: &str,
    dest: &Path,
) -> Result<(), String> {
    let _run = run_bounded_command(
        &settings.git.binary,
        &[
            "clone".to_string(),
            "--depth".to_string(),
            "1".to_string(),
            "--branch".to_string(),
            git_ref.to_string(),
            repo.to_string(),
            dest.to_string_lossy().to_string(),
        ],
        dest.parent().unwrap_or_else(|| Path::new(".")),
        &BTreeMap::new(),
        settings.build_timeout_seconds,
        settings.max_output_bytes,
    )
    .await;
    if _run.ok {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&_run.stderr).trim().to_string())
    }
}

async fn run_build(state: Arc<AppState>, task: TaskRecord) -> Result<PathBuf, AppError> {
    let params: BuildParams = parse_params(&task.tool_params)?;
    validate_run_id(&params.run_id)?;
    let record = state
        .dev_selftest
        .get(&params.run_id)
        .await
        .ok_or_else(|| AppError::bad_request(format!("unknown runId {}", params.run_id)))?;
    let profile = state
        .config
        .dev_selftest
        .builds
        .get(&params.build_profile)
        .ok_or_else(|| {
            AppError::bad_request(format!("unknown build profile {}", params.build_profile))
        })?
        .clone();

    let run_root = run_dir(&state, &record.run_id);
    let source_dir = run_root.join("source");
    let cwd = if profile.working_dir.trim().is_empty() {
        source_dir.clone()
    } else {
        safe_join(&source_dir, &PathBuf::from(&profile.working_dir))
            .map_err(|err| AppError::bad_request(format!("invalid working_dir: {err}")))?
    };
    fs::create_dir_all(&cwd)
        .map_err(|err| AppError::internal(format!("failed to create build cwd: {err}")))?;

    let (binary, argv) = split_command(&profile.command)?;
    let started = Instant::now();
    let run = run_bounded_command(
        &binary,
        &argv,
        &cwd,
        &BTreeMap::new(),
        profile
            .timeout_seconds
            .unwrap_or(state.config.dev_selftest.build_timeout_seconds),
        state.config.dev_selftest.max_output_bytes,
    )
    .await;
    let duration = started.elapsed().as_millis();

    let logs_dir = run_root.join("logs");
    fs::create_dir_all(&logs_dir)
        .map_err(|err| AppError::internal(format!("failed to create logs dir: {err}")))?;
    write_bytes(&logs_dir.join("build.stdout.txt"), &run.stdout)?;
    write_bytes(&logs_dir.join("build.stderr.txt"), &run.stderr)?;

    let artifacts = collect_artifacts(&cwd, &profile, &run_root.join("artifacts"))?;
    let _ = state
        .dev_selftest
        .update(&record.run_id, |rec| {
            rec.build_artifacts = artifacts.iter().map(|a| a.clone()).collect();
            Ok(())
        })
        .await;

    let status = if run.ok { "OK" } else { "FAILED" };
    let error = if run.ok {
        None
    } else {
        Some(
            run.error
                .clone()
                .unwrap_or_else(|| format!("exit code {:?}", run.exit_code)),
        )
    };
    append_step(
        &state,
        &record.run_id,
        new_step(
            "build",
            status,
            duration,
            error.clone(),
            artifacts.iter().map(|a| format!("artifacts/{a}")).collect(),
        ),
    )?;
    if !run.ok {
        mark_failed(&state, &record.run_id).await;
    }

    let action_id = task_action_id(BUILD_ID, &task.task_id);
    let result = json!({
        "schemaVersion": 1,
        "toolId": BUILD_ID,
        "actionId": action_id,
        "runId": record.run_id,
        "buildProfile": params.build_profile,
        "status": status,
        "exitCode": run.exit_code,
        "artifacts": artifacts,
        "error": error,
        "durationMs": duration,
        "createdAt": Utc::now(),
    });
    write_tool_result(
        &state.config.storage.workspace_dir(&task.task_id),
        &action_id,
        &result,
    )
}

fn split_command(command: &[String]) -> Result<(PathBuf, Vec<String>), AppError> {
    let mut iter = command.iter();
    let binary = iter
        .next()
        .ok_or_else(|| AppError::bad_request("build command must not be empty"))?
        .clone();
    let argv = iter.cloned().collect();
    Ok((PathBuf::from(binary), argv))
}

fn collect_artifacts(
    cwd: &Path,
    profile: &DevSelftestBuildProfile,
    artifacts_dir: &Path,
) -> Result<Vec<String>, AppError> {
    fs::create_dir_all(artifacts_dir)
        .map_err(|err| AppError::internal(format!("failed to create artifacts dir: {err}")))?;
    let mut collected = Vec::new();
    for glob in &profile.artifact_globs {
        for entry in glob_match(cwd, glob)? {
            let name = sanitize_filename(&entry.file_name().unwrap_or_default().to_string_lossy())?;
            let dest = artifacts_dir.join(&name);
            if entry.is_file() {
                fs::copy(&entry, &dest)
                    .map_err(|err| AppError::internal(format!("failed to copy artifact: {err}")))?;
                if !collected.contains(&name) {
                    collected.push(name);
                }
            }
        }
    }
    Ok(collected)
}

fn glob_match(root: &Path, pattern: &str) -> Result<Vec<PathBuf>, AppError> {
    // Minimal glob: a single `*` segment matches any filename within one directory level.
    // Patterns without `*` match a literal relative path. Recursive `**` is not supported.
    let mut matches = Vec::new();
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return Ok(matches);
    }
    if pattern.contains("**") {
        warn!("dev_selftest artifact glob with '**' is not supported; only single-level '*' is matched");
    }
    if !pattern.contains('*') {
        let candidate = safe_join(root, &PathBuf::from(pattern))
            .map_err(|err| AppError::internal(err.to_string()))?;
        if candidate.exists() {
            matches.push(candidate);
        }
        return Ok(matches);
    }
    let segments: Vec<&str> = pattern.split('/').collect();
    let (parent_segments, leaf) = segments.split_at(segments.len().saturating_sub(1));
    let leaf = leaf.first().copied().unwrap_or("");
    let mut parent = root.to_path_buf();
    for seg in parent_segments {
        parent = safe_join(&parent, &PathBuf::from(seg))
            .map_err(|err| AppError::internal(err.to_string()))?;
    }
    if !parent.is_dir() {
        return Ok(matches);
    }
    for entry in fs::read_dir(&parent)
        .map_err(|err| AppError::internal(format!("failed to read dir: {err}")))?
    {
        let entry = entry.map_err(|err| AppError::internal(format!("dir entry: {err}")))?;
        let name = entry.file_name().to_string_lossy().to_string();
        if glob_leaf_matches(leaf, &name) {
            matches.push(entry.path());
        }
    }
    matches.sort();
    Ok(matches)
}

fn glob_leaf_matches(pattern: &str, name: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some((pre, rest)) = pattern.split_once('*') {
        let suf = rest.trim_start_matches('*');
        return name.starts_with(pre) && name.ends_with(suf) && name.len() >= pre.len() + suf.len();
    }
    name == pattern
}

/// Env vars describing the current run's directories, passed to `docker compose` and
/// the health-check command so a compose file / health cmd can mount or reference the
/// run's synced source and built artifacts via `${DEVSELFTEST_SOURCE_DIR}` etc. Generic
/// (not openGemini-specific); values are absolute paths.
fn deploy_env(
    run_root: &Path,
    source_dir: &Path,
    artifacts_dir: &Path,
    project_name: &str,
) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    env.insert(
        "DEVSELFTEST_RUN_DIR".to_string(),
        run_root.to_string_lossy().into_owned(),
    );
    env.insert(
        "DEVSELFTEST_SOURCE_DIR".to_string(),
        source_dir.to_string_lossy().into_owned(),
    );
    env.insert(
        "DEVSELFTEST_ARTIFACTS_DIR".to_string(),
        artifacts_dir.to_string_lossy().into_owned(),
    );
    env.insert(
        "DEVSELFTEST_PROJECT_NAME".to_string(),
        project_name.to_string(),
    );
    env
}

async fn run_deploy(state: Arc<AppState>, task: TaskRecord) -> Result<PathBuf, AppError> {
    let params: DeployParams = parse_params(&task.tool_params)?;
    validate_run_id(&params.run_id)?;
    let record = state
        .dev_selftest
        .get(&params.run_id)
        .await
        .ok_or_else(|| AppError::bad_request(format!("unknown runId {}", params.run_id)))?;
    let cluster = state
        .config
        .dev_selftest
        .docker
        .clusters
        .get(&params.profile)
        .ok_or_else(|| AppError::bad_request(format!("unknown docker cluster {}", params.profile)))?
        .clone();

    let run_root = run_dir(&state, &record.run_id);
    let source_dir = run_root.join("source");
    let artifacts_dir = run_root.join("artifacts");
    let project_name = format!(
        "devselftest_{}_{}",
        sanitize_filename(&record.run_id)?,
        sanitize_filename(&params.profile)?
    );
    let env = deploy_env(&run_root, &source_dir, &artifacts_dir, &project_name);
    let started = Instant::now();
    let run = run_bounded_command(
        &state.config.dev_selftest.docker.binary,
        &[
            "compose".to_string(),
            "-p".to_string(),
            project_name.clone(),
            "-f".to_string(),
            cluster.compose_file.to_string_lossy().to_string(),
            "up".to_string(),
            "-d".to_string(),
        ],
        &run_root,
        &env,
        state.config.dev_selftest.build_timeout_seconds,
        state.config.dev_selftest.max_output_bytes,
    )
    .await;
    let duration = started.elapsed().as_millis();

    let logs_dir = run_root.join("logs");
    fs::create_dir_all(&logs_dir)
        .map_err(|err| AppError::internal(format!("failed to create logs dir: {err}")))?;
    write_bytes(&logs_dir.join("deploy.stdout.txt"), &run.stdout)?;
    write_bytes(&logs_dir.join("deploy.stderr.txt"), &run.stderr)?;

    // Health check (declared command, e.g. curl or `true`). Failure does not roll back in P1.
    let mut health_ok = run.ok;
    let mut health_error = None::<String>;
    if run.ok {
        if let Some(hc) = &cluster.health_check {
            match run_health_check(state.clone(), hc, &run_root, &env).await {
                Ok(()) => {}
                Err(err) => {
                    health_ok = false;
                    health_error = Some(err);
                }
            }
        }
    }

    let status = if run.ok && health_ok { "OK" } else { "FAILED" };
    let error = if run.ok && health_ok {
        None
    } else if !run.ok {
        Some(
            run.error
                .clone()
                .unwrap_or_else(|| format!("exit code {:?}", run.exit_code)),
        )
    } else {
        health_error.clone()
    };

    let target = DevSelftestDeployTarget::Docker {
        cluster: params.profile.clone(),
        exposed_port: cluster.exposed_port,
    };
    let _ = state
        .dev_selftest
        .update(&record.run_id, |rec| {
            rec.deploy_target = Some(target.clone());
            Ok(())
        })
        .await;

    append_step(
        &state,
        &record.run_id,
        new_step(
            "deploy",
            status,
            duration,
            error.clone(),
            vec![
                "logs/deploy.stdout.txt".to_string(),
                "logs/deploy.stderr.txt".to_string(),
            ],
        ),
    )?;
    if status == "FAILED" {
        mark_failed(&state, &record.run_id).await;
    }

    let action_id = task_action_id(DEPLOY_ID, &task.task_id);
    let result = json!({
        "schemaVersion": 1,
        "toolId": DEPLOY_ID,
        "actionId": action_id,
        "runId": record.run_id,
        "profile": params.profile,
        "projectName": project_name,
        "status": status,
        "exitCode": run.exit_code,
        "deployTarget": target,
        "error": error,
        "durationMs": duration,
        "createdAt": Utc::now(),
    });
    write_tool_result(
        &state.config.storage.workspace_dir(&task.task_id),
        &action_id,
        &result,
    )
}

async fn run_health_check(
    state: Arc<AppState>,
    hc: &crate::support::config::DevSelftestHealthCheck,
    cwd: &Path,
    env: &BTreeMap<String, String>,
) -> Result<(), String> {
    if hc.cmd.is_empty() {
        return Ok(());
    }
    let deadline = Instant::now() + Duration::from_secs(hc.timeout_seconds.max(1));
    loop {
        let (binary, argv) = split_command(&hc.cmd).map_err(|e| e.to_string())?;
        let run = run_bounded_command(
            &binary,
            &argv,
            cwd,
            env,
            hc.timeout_seconds.max(1),
            state.config.dev_selftest.max_output_bytes,
        )
        .await;
        if run.ok {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "health check failed: {}",
                String::from_utf8_lossy(&run.stderr).trim()
            ));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn run_run_tests(state: Arc<AppState>, task: TaskRecord) -> Result<PathBuf, AppError> {
    let params: RunTestsParams = parse_params(&task.tool_params)?;
    validate_run_id(&params.run_id)?;
    let record = state
        .dev_selftest
        .get(&params.run_id)
        .await
        .ok_or_else(|| AppError::bad_request(format!("unknown runId {}", params.run_id)))?;
    let suite = state
        .config
        .dev_selftest
        .test_suites
        .get(&params.test_suite)
        .ok_or_else(|| AppError::bad_request(format!("unknown test suite {}", params.test_suite)))?
        .clone();

    let run_root = run_dir(&state, &record.run_id);
    let started = Instant::now();
    // Docker test suites dispatch through the executor docker runner (`docker run --rm
    // --network ... <image> <argv>`); suites without a `docker` block keep the P1 local
    // stub. Both produce a BoundedRun so the log/step/result handling below is shared.
    let run = if let Some(docker) = &suite.docker {
        run_docker_test(&state, &record, &suite, docker, &run_root).await?
    } else {
        let env = target_env(&record, &suite);
        let (binary, argv) = split_command(&suite.argv)?;
        run_bounded_command(
            &binary,
            &argv,
            &run_root,
            &env,
            suite
                .timeout_seconds
                .unwrap_or(state.config.dev_selftest.build_timeout_seconds),
            state.config.dev_selftest.max_output_bytes,
        )
        .await
    };
    let duration = started.elapsed().as_millis();

    let logs_dir = run_root.join("logs");
    fs::create_dir_all(&logs_dir)
        .map_err(|err| AppError::internal(format!("failed to create logs dir: {err}")))?;
    write_bytes(&logs_dir.join("tests.stdout.txt"), &run.stdout)?;
    write_bytes(&logs_dir.join("tests.stderr.txt"), &run.stderr)?;

    let status = if run.ok { "OK" } else { "FAILED" };
    let error = if run.ok {
        None
    } else {
        Some(
            run.error
                .clone()
                .unwrap_or_else(|| format!("exit code {:?}", run.exit_code)),
        )
    };
    append_step(
        &state,
        &record.run_id,
        new_step(
            "run_tests",
            status,
            duration,
            error.clone(),
            vec![
                "logs/tests.stdout.txt".to_string(),
                "logs/tests.stderr.txt".to_string(),
            ],
        ),
    )?;
    if !run.ok {
        mark_failed(&state, &record.run_id).await;
    }

    let executor_info = suite.docker.as_ref().map(|docker| {
        json!({
            "kind": "docker",
            "image": docker.image,
            "network": docker.network.clone().unwrap_or_else(|| "host".to_string()),
        })
    });
    let action_id = task_action_id(RUN_TESTS_ID, &task.task_id);
    let result = json!({
        "schemaVersion": 1,
        "toolId": RUN_TESTS_ID,
        "actionId": action_id,
        "runId": record.run_id,
        "testSuite": params.test_suite,
        "status": status,
        "exitCode": run.exit_code,
        "executor": executor_info,
        "stdoutPath": "logs/tests.stdout.txt",
        "stderrPath": "logs/tests.stderr.txt",
        "error": error,
        "durationMs": duration,
        "createdAt": Utc::now(),
    });
    write_tool_result(
        &state.config.storage.workspace_dir(&task.task_id),
        &action_id,
        &result,
    )
}

/// Dispatch a docker test suite through the executor docker runner. argv/timeout come from
/// the referenced `remote_execution.commands` template (`suite.command`) or, failing that,
/// `suite.argv`. Volume host sides are interpolated from the run-directory env
/// (`${DEVSELFTEST_*}`); system env (run dirs + `DEVSELFTEST_HOST/PORT`) is injected with
/// final priority so a misconfigured user env cannot redirect the test at the wrong target.
async fn run_docker_test(
    state: &AppState,
    record: &DevSelftestRunRecord,
    suite: &DevSelftestTestSuite,
    docker: &DevSelftestTestDocker,
    run_root: &Path,
) -> Result<BoundedRun, AppError> {
    let (argv, timeout_seconds) = match suite.command.as_deref() {
        Some(command_id) => {
            let template = remote_execution::command_template(&state.config, command_id)
                .ok_or_else(|| AppError::bad_request(format!("unknown command {command_id}")))?;
            if !template.enabled {
                return Err(AppError::bad_request(format!(
                    "command {command_id} is disabled"
                )));
            }
            (template.argv, template.timeout_seconds)
        }
        None => (suite.argv.clone(), suite.timeout_seconds),
    };
    if argv.is_empty() {
        return Err(AppError::bad_request("docker test suite has empty argv"));
    }

    let source_dir = run_root.join("source");
    let artifacts_dir = run_root.join("artifacts");
    let cluster = match &record.deploy_target {
        Some(DevSelftestDeployTarget::Docker { cluster, .. }) => cluster.clone(),
        _ => String::new(),
    };
    let project_name = if cluster.is_empty() {
        format!("devselftest_{}", sanitize_filename(&record.run_id)?)
    } else {
        format!(
            "devselftest_{}_{}",
            sanitize_filename(&record.run_id)?,
            sanitize_filename(&cluster)?
        )
    };
    let env_map = deploy_env(run_root, &source_dir, &artifacts_dir, &project_name);

    // Interpolate ${DEVSELFTEST_*} in volume host sides; the host must be absolute after.
    let mut volumes = Vec::with_capacity(docker.volumes.len());
    for volume in &docker.volumes {
        volumes.push(interpolate_volume(volume, &env_map)?);
    }

    // User env: suite.env then docker.env (docker.env wins). System env below wins over both.
    let mut user_env = suite.env.clone();
    for (key, value) in &docker.env {
        user_env.insert(key.clone(), value.clone());
    }
    let mut extra_env = env_map.clone();
    extra_env.insert("DEVSELFTEST_HOST".to_string(), "127.0.0.1".to_string());
    if let Some(DevSelftestDeployTarget::Docker {
        exposed_port: Some(port),
        ..
    }) = &record.deploy_target
    {
        extra_env.insert("DEVSELFTEST_PORT".to_string(), port.to_string());
    }

    let target = ExecutorTarget::Docker {
        image: docker.image.clone(),
        network: docker.network.clone(),
        workdir: docker.workdir.clone(),
        volumes,
        env: user_env,
    };
    let input = ExecutorRunInput {
        target: &target,
        argv: &argv,
        timeout_seconds: timeout_seconds.unwrap_or(state.config.dev_selftest.build_timeout_seconds),
        extra_env,
        server_cwd: run_root.to_path_buf(),
        launcher: state.config.dev_selftest.docker.binary.clone(),
        max_output_bytes: state.config.dev_selftest.max_output_bytes,
    };
    let outcome = remote_execution::run_executor_command(input).await;
    Ok(BoundedRun {
        ok: outcome.status == ExecutorRunStatus::Ok,
        exit_code: outcome.exit_code,
        stdout: outcome.stdout,
        stderr: outcome.stderr,
        duration_ms: outcome.duration_ms,
        error: outcome.error,
    })
}

/// Replace `${DEVSELFTEST_*}` placeholders in a volume spec using the run-directory env,
/// then assert the host side (before the first `:`) is an absolute path.
fn interpolate_volume(
    volume: &str,
    env_map: &BTreeMap<String, String>,
) -> Result<String, AppError> {
    let mut result = volume.to_string();
    for (key, value) in env_map {
        result = result.replace(&format!("${{{key}}}"), value);
    }
    let host = result.split(':').next().unwrap_or("");
    if !host.starts_with('/') {
        return Err(AppError::bad_request(format!(
            "docker volume host must be an absolute path after interpolation: {volume}"
        )));
    }
    Ok(result)
}

fn target_env(
    record: &DevSelftestRunRecord,
    suite: &DevSelftestTestSuite,
) -> BTreeMap<String, String> {
    let mut env = suite.env.clone();
    if let Some(DevSelftestDeployTarget::Docker {
        exposed_port: Some(port),
        ..
    }) = &record.deploy_target
    {
        env.entry("DEVSELFTEST_HOST".to_string())
            .or_insert_with(|| "127.0.0.1".to_string());
        env.insert("DEVSELFTEST_PORT".to_string(), port.to_string());
    }
    env
}

async fn run_report(state: Arc<AppState>, task: TaskRecord) -> Result<PathBuf, AppError> {
    let params: ReportParams = parse_params(&task.tool_params)?;
    validate_run_id(&params.run_id)?;
    let record = state
        .dev_selftest
        .get(&params.run_id)
        .await
        .ok_or_else(|| AppError::bad_request(format!("unknown runId {}", params.run_id)))?;
    let run_root = run_dir(&state, &record.run_id);
    let progress = read_progress(&run_root);

    let steps: Vec<Value> = progress
        .steps
        .iter()
        .map(|step| {
            json!({
                "step": step.step,
                "status": step.status,
                "durationMs": step.duration_ms,
                "error": step.error,
                "evidenceRefs": step.evidence_refs,
            })
        })
        .collect();
    let failed_steps: Vec<&str> = progress
        .steps
        .iter()
        .filter(|step| step.status != "OK")
        .map(|step| step.step.as_str())
        .collect();
    let overall = if failed_steps.is_empty() {
        "SUCCEEDED"
    } else {
        "FAILED"
    };

    let markdown = render_markdown(&record, &progress.steps, overall, &failed_steps);
    let report_path = run_root.join("report.md");
    fs::write(&report_path, markdown.as_bytes())
        .map_err(|err| AppError::internal(format!("failed to write report: {err}")))?;
    let report_json_path = run_root.join("report.json");
    let report_value = json!({
        "schemaVersion": 1,
        "runId": record.run_id,
        "status": overall,
        "sourceRef": record.source_ref,
        "buildArtifacts": record.build_artifacts,
        "deployTarget": record.deploy_target,
        "steps": steps,
        "failedSteps": failed_steps,
    });
    write_json_sync(&report_json_path, &report_value)?;

    append_step(
        &state,
        &record.run_id,
        new_step(
            "report",
            "OK",
            0,
            None,
            vec!["report.md".to_string(), "report.json".to_string()],
        ),
    )?;

    let action_id = task_action_id(REPORT_ID, &task.task_id);
    let result = json!({
        "schemaVersion": 1,
        "toolId": REPORT_ID,
        "actionId": action_id,
        "runId": record.run_id,
        "status": overall,
        "reportPath": relative_string(&run_root, &report_path)
            .map_err(|err| AppError::internal(err.to_string()))?,
        "failedSteps": failed_steps,
        "steps": steps,
        "createdAt": Utc::now(),
    });
    write_tool_result(
        &state.config.storage.workspace_dir(&task.task_id),
        &action_id,
        &result,
    )
}

fn render_markdown(
    record: &DevSelftestRunRecord,
    steps: &[DevSelftestStep],
    overall: &str,
    failed_steps: &[&str],
) -> String {
    let mut md = String::new();
    md.push_str(&format!(
        "# Dev self-test report\n\n- **Run:** `{}`\n",
        record.run_id
    ));
    md.push_str(&format!("- **Status:** {}\n", overall));
    if let Some(source) = &record.source_ref {
        md.push_str(&format!("- **Source:** {}\n", source));
    }
    if !record.build_artifacts.is_empty() {
        md.push_str(&format!(
            "- **Build artifacts:** {}\n",
            record.build_artifacts.join(", ")
        ));
    }
    if let Some(target) = &record.deploy_target {
        md.push_str(&format!("- **Deploy target:** {:?}\n", target));
    }
    md.push_str("\n## Steps\n\n| Step | Status | Duration (ms) | Error |\n|---|---|---|---|\n");
    for step in steps {
        md.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            step.step,
            step.status,
            step.duration_ms,
            step.error.clone().unwrap_or_default(),
        ));
    }
    if !failed_steps.is_empty() {
        md.push_str(&format!(
            "\n**Failed steps:** {}\n",
            failed_steps.join(", ")
        ));
    }
    md.push_str("\n_Evidence files (logs, artifacts, progress.json) live alongside this report in the run workspace._\n");
    md
}

async fn mark_failed(state: &AppState, run_id: &str) {
    let _ = state
        .dev_selftest
        .update(run_id, |rec| {
            rec.status = DevSelftestRunStatus::Failed;
            Ok(())
        })
        .await;
}

fn validate_run_id(run_id: &str) -> Result<(), AppError> {
    let valid = run_id.starts_with("devselftest_")
        && run_id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'_' || value == b'-');
    if valid {
        Ok(())
    } else {
        Err(AppError::bad_request("invalid runId"))
    }
}

// ---------- descriptors ----------

fn common_tags() -> Vec<String> {
    vec![
        "built-in".to_string(),
        "dev-selftest".to_string(),
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
        platform: false,
        tags: common_tags(),
        backend: "dev_selftest".to_string(),
        accepted_suffixes: Vec::new(),
        min_files: 0,
        max_files: 0,
        params_schema: Value::Null,
        params_template: Value::Null,
        output_views: vec!["summary".to_string(), "json".to_string()],
    }
}

fn sync_workspace_descriptor(enabled: bool) -> ToolDescriptor {
    let mut d = base_descriptor(
        SYNC_WORKSPACE_ID,
        "Dev self-test: sync workspace",
        "Create or reuse a dev-self-test run and populate its source/ from an uploaded tarball, a configured git repo+ref, or leave it empty (stub). Returns runId.",
        enabled,
    );
    d.params_schema = json!({
        "type": "object",
        "properties": {
            "runId": { "type": "string", "description": "Omit to create a new run." },
            "label": { "type": "string" },
            "uploadId": { "type": "string", "description": "An uploaded source tarball (.tar.gz/.tar/.zip) to unpack into source/." },
            "gitRepo": { "type": "string", "description": "Must be in the configured git repos allowlist." },
            "gitRef": { "type": "string", "description": "Must be in the repo's allowed refs." }
        }
    });
    d.params_template =
        json!({ "runId": "", "label": "", "uploadId": "", "gitRepo": "", "gitRef": "" });
    d
}

fn build_descriptor(enabled: bool) -> ToolDescriptor {
    let mut d = base_descriptor(
        BUILD_ID,
        "Dev self-test: build",
        "Run a configured build profile in the run's source/ and collect declared artifacts into artifacts/.",
        enabled,
    );
    d.params_schema = json!({
        "type": "object",
        "properties": {
            "runId": { "type": "string" },
            "buildProfile": { "type": "string", "description": "A configured dev_selftest.builds profile id." }
        },
        "required": ["runId", "buildProfile"]
    });
    d.params_template = json!({ "runId": "", "buildProfile": "" });
    d
}

fn deploy_descriptor(enabled: bool) -> ToolDescriptor {
    let mut d = base_descriptor(
        DEPLOY_ID,
        "Dev self-test: deploy",
        "Deploy via a configured profile. P1 supports docker_cluster (docker compose up -d + declared health check).",
        enabled,
    );
    d.params_schema = json!({
        "type": "object",
        "properties": {
            "runId": { "type": "string" },
            "profile": { "type": "string", "description": "A configured dev_selftest.docker.clusters profile id (P1)." }
        },
        "required": ["runId", "profile"]
    });
    d.params_template = json!({ "runId": "", "profile": "" });
    d
}

fn run_tests_descriptor(enabled: bool) -> ToolDescriptor {
    let mut d = base_descriptor(
        RUN_TESTS_ID,
        "Dev self-test: run tests",
        "Run a configured test suite against the run's deployed target. P1 is a stub runner (local command); real executor-dispatched test framework lands in P2. Runnable sync or runMode:'queued'.",
        enabled,
    );
    d.params_schema = json!({
        "type": "object",
        "properties": {
            "runId": { "type": "string" },
            "testSuite": { "type": "string", "description": "A configured dev_selftest.test_suites profile id." }
        },
        "required": ["runId", "testSuite"]
    });
    d.params_template = json!({ "runId": "", "testSuite": "" });
    d
}

fn report_descriptor(enabled: bool) -> ToolDescriptor {
    let mut d = base_descriptor(
        REPORT_ID,
        "Dev self-test: report",
        "Aggregate the run's progress ledger and step evidence into report.json + report.md (statuses, durations, errors, artifact links).",
        enabled,
    );
    d.params_schema = json!({
        "type": "object",
        "properties": { "runId": { "type": "string" } },
        "required": ["runId"]
    });
    d.params_template = json!({ "runId": "" });
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptors_gated_by_enabled() {
        let config = test_config(false);
        let ds = descriptors(&config);
        assert_eq!(ds.len(), 5);
        assert!(ds.iter().all(|d| !d.enabled && !d.runnable));
        assert!(ds.iter().all(|d| d.backend == "dev_selftest"));
        let config = test_config(true);
        let ds = descriptors(&config);
        assert!(ds.iter().all(|d| d.enabled && d.runnable));
        assert!(get_descriptor(&config, BUILD_ID).is_some());
    }

    #[test]
    fn validate_rejects_when_disabled() {
        let config = test_config(false);
        assert!(validate_run_params(
            &config,
            BUILD_ID,
            &json!({"runId":"devselftest_x","buildProfile":"stub"})
        )
        .is_err());
    }

    #[test]
    fn validate_requires_known_profiles() {
        let config = test_config(true);
        assert!(validate_run_params(
            &config,
            BUILD_ID,
            &json!({"runId":"devselftest_x","buildProfile":"missing"})
        )
        .is_err());
        assert!(validate_run_params(
            &config,
            BUILD_ID,
            &json!({"runId":"devselftest_x","buildProfile":"stub"})
        )
        .is_ok());
        assert!(validate_run_params(
            &config,
            DEPLOY_ID,
            &json!({"runId":"devselftest_x","profile":"local"})
        )
        .is_ok());
        assert!(validate_run_params(
            &config,
            RUN_TESTS_ID,
            &json!({"runId":"devselftest_x","testSuite":"stub"})
        )
        .is_ok());
    }

    #[test]
    fn validate_run_id_format() {
        assert!(validate_run_id("devselftest_abc-1").is_ok());
        assert!(validate_run_id("task_x").is_err());
        assert!(validate_run_id("devselftest_bad/id").is_err());
    }

    #[test]
    fn deploy_env_exposes_run_directories() {
        let env = deploy_env(
            std::path::Path::new("/run/root"),
            std::path::Path::new("/run/root/source"),
            std::path::Path::new("/run/root/artifacts"),
            "devselftest_1_local",
        );
        assert_eq!(env.get("DEVSELFTEST_RUN_DIR").unwrap(), "/run/root");
        assert_eq!(
            env.get("DEVSELFTEST_SOURCE_DIR").unwrap(),
            "/run/root/source"
        );
        assert_eq!(
            env.get("DEVSELFTEST_ARTIFACTS_DIR").unwrap(),
            "/run/root/artifacts"
        );
        assert_eq!(
            env.get("DEVSELFTEST_PROJECT_NAME").unwrap(),
            "devselftest_1_local"
        );
        assert_eq!(env.len(), 4);
    }

    #[test]
    fn glob_leaf_matches_patterns() {
        assert!(glob_leaf_matches("*", "influxql-analyzer"));
        assert!(glob_leaf_matches("*.txt", "a.txt"));
        assert!(!glob_leaf_matches("*.txt", "a.log"));
        assert!(glob_leaf_matches("influx*", "influxql-analyzer"));
        assert!(glob_leaf_matches("influx-analyzer", "influx-analyzer"));
    }

    fn test_config(enabled: bool) -> AppConfig {
        use crate::support::config::{
            AuthSettings, DevSelftestBuildProfile, DevSelftestDockerCluster,
            DevSelftestDockerSettings, DevSelftestGitSettings, DevSelftestSettings,
            DevSelftestTestSuite, FetchSettings, HuaweiCloudSettings, LogAnalyzerSettings,
            McpSettings, RemoteExecutionSettings, ServerSettings, SkillSettings, StorageSettings,
            ToolsSettings,
        };
        use std::collections::BTreeMap;
        use std::path::PathBuf;
        let mut builds = BTreeMap::new();
        builds.insert(
            "stub".to_string(),
            DevSelftestBuildProfile {
                display_name: "stub".to_string(),
                command: vec!["true".to_string()],
                working_dir: String::new(),
                artifact_globs: Vec::new(),
                timeout_seconds: None,
            },
        );
        let mut clusters = BTreeMap::new();
        clusters.insert(
            "local".to_string(),
            DevSelftestDockerCluster {
                compose_file: PathBuf::from("/opt/dev_selftest/docker-compose.yml"),
                exposed_port: Some(8086),
                health_check: None,
            },
        );
        let mut suites = BTreeMap::new();
        suites.insert(
            "stub".to_string(),
            DevSelftestTestSuite {
                display_name: "stub".to_string(),
                argv: vec!["true".to_string()],
                command: None,
                timeout_seconds: None,
                env: BTreeMap::new(),
                docker: None,
            },
        );
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
            dev_selftest: DevSelftestSettings {
                enabled,
                build_timeout_seconds: 30,
                max_output_bytes: 1024,
                git: DevSelftestGitSettings::default(),
                builds,
                docker: DevSelftestDockerSettings {
                    binary: PathBuf::from("/usr/bin/docker"),
                    clusters,
                },
                test_suites: suites,
            },
        }
    }

    #[cfg(all(test, unix))]
    fn test_state_with_dev_selftest(
        prefix: &str,
    ) -> (Arc<crate::app::AppState>, std::path::PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!(
            "logagent-{prefix}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let fake_docker = root.join("fake-docker.sh");
        std::fs::write(&fake_docker, "#!/usr/bin/env bash\nexit 0\n").unwrap();
        let mut perms = std::fs::metadata(&fake_docker).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&fake_docker, perms).unwrap();

        let mut config = test_config(true);
        config.storage.data_dir = root.join("data");
        config.dev_selftest.docker.binary = fake_docker;
        let config = Arc::new(config);
        config.prepare_dirs().unwrap();
        (crate::app::AppState::new(config).unwrap(), root)
    }

    #[cfg(all(test, unix))]
    struct ToolOut {
        status: String,
        run_id: String,
    }

    #[cfg(all(test, unix))]
    async fn run_tool(state: &Arc<crate::app::AppState>, tool_id: &str, params: Value) -> ToolOut {
        use crate::services::tools::{build_tool_run_task, run_tool_task};
        let task = build_tool_run_task(state, tool_id, Vec::new(), &params)
            .await
            .unwrap();
        state.tasks.create(task.clone()).await.unwrap();
        let path = run_tool_task(state.clone(), task).await.unwrap();
        let value: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        ToolOut {
            status: value["status"].as_str().unwrap_or("").to_string(),
            run_id: value["runId"].as_str().unwrap_or("").to_string(),
        }
    }

    #[tokio::test]
    #[cfg(all(test, unix))]
    async fn docker_selftest_closed_loop() {
        let (state, root) = test_state_with_dev_selftest("dev-selftest-loop");

        let sync = run_tool(&state, SYNC_WORKSPACE_ID, json!({"label":"loop"})).await;
        assert_eq!(sync.status, "OK");
        let run_id = sync.run_id;

        let build = run_tool(
            &state,
            BUILD_ID,
            json!({"runId":run_id,"buildProfile":"stub"}),
        )
        .await;
        assert_eq!(build.status, "OK");
        let deploy = run_tool(&state, DEPLOY_ID, json!({"runId":run_id,"profile":"local"})).await;
        assert_eq!(deploy.status, "OK");
        let tests = run_tool(
            &state,
            RUN_TESTS_ID,
            json!({"runId":run_id,"testSuite":"stub"}),
        )
        .await;
        assert_eq!(tests.status, "OK");
        let report = run_tool(&state, REPORT_ID, json!({"runId":run_id})).await;
        assert_eq!(report.status, "SUCCEEDED");

        let run_dir = state.config.storage.dev_selftest_run_dir(&run_id);
        assert!(run_dir.join("report.md").is_file());
        let markdown = std::fs::read_to_string(run_dir.join("report.md")).unwrap();
        assert!(markdown.contains("SUCCEEDED"));
        assert!(markdown.contains("sync_workspace"));
        assert!(markdown.contains("run_tests"));

        let progress: Progress =
            serde_json::from_str(&std::fs::read_to_string(run_dir.join(PROGRESS_FILE)).unwrap())
                .unwrap();
        assert_eq!(progress.steps.len(), 5);

        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(all(test, unix))]
    fn test_state_with_docker_suite(
        prefix: &str,
    ) -> (Arc<crate::app::AppState>, std::path::PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!(
            "logagent-{prefix}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&root).unwrap();
        // Fake docker echoes its argv (so the test can inspect the run_tests dispatch) and
        // exits 0 (success).
        let fake_docker = root.join("fake-docker.sh");
        std::fs::write(
            &fake_docker,
            "#!/usr/bin/env bash\nprintf 'ARGS:'; for a in \"$@\"; do printf ' %s' \"$a\"; done; echo\nexit 0\n",
        )
        .unwrap();
        let mut perms = std::fs::metadata(&fake_docker).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&fake_docker, perms).unwrap();

        let mut config = test_config(true);
        config.storage.data_dir = root.join("data");
        config.dev_selftest.docker.binary = fake_docker;
        config.dev_selftest.test_suites.insert(
            "smoke".to_string(),
            DevSelftestTestSuite {
                display_name: "smoke".to_string(),
                argv: vec!["sh".to_string(), "/tests/smoke.sh".to_string()],
                command: None,
                timeout_seconds: Some(30),
                env: BTreeMap::new(),
                docker: Some(DevSelftestTestDocker {
                    image: "alpine:3.20".to_string(),
                    network: Some("host".to_string()),
                    workdir: None,
                    volumes: vec!["/repo/tests:/tests:ro".to_string()],
                    env: BTreeMap::new(),
                }),
            },
        );
        let config = Arc::new(config);
        config.prepare_dirs().unwrap();
        (crate::app::AppState::new(config).unwrap(), root)
    }

    #[tokio::test]
    #[cfg(all(test, unix))]
    async fn docker_executor_test_dispatch() {
        let (state, root) = test_state_with_docker_suite("dev-selftest-docker-exec");

        let sync = run_tool(&state, SYNC_WORKSPACE_ID, json!({"label":"docker-exec"})).await;
        assert_eq!(sync.status, "OK");
        let run_id = sync.run_id;

        // run_tests dispatches the smoke suite through the executor docker runner. The fake
        // docker echoes its argv into the captured tests stdout.
        let tests = run_tool(
            &state,
            RUN_TESTS_ID,
            json!({"runId":run_id,"testSuite":"smoke"}),
        )
        .await;
        assert_eq!(tests.status, "OK");

        let run_dir = state.config.storage.dev_selftest_run_dir(&run_id);
        let stdout = std::fs::read_to_string(run_dir.join("logs/tests.stdout.txt")).unwrap();
        assert!(
            stdout.contains("run --rm --network host"),
            "stdout: {stdout}"
        );
        assert!(
            stdout.contains("--volume /repo/tests:/tests:ro alpine:3.20 sh /tests/smoke.sh"),
            "stdout: {stdout}"
        );
        assert!(
            stdout.contains("--env DEVSELFTEST_HOST=127.0.0.1"),
            "stdout: {stdout}"
        );

        let report = run_tool(&state, REPORT_ID, json!({"runId":run_id})).await;
        assert_eq!(report.status, "SUCCEEDED");

        let _ = std::fs::remove_dir_all(root);
    }
}
