//! Dev self-test pipeline built-in tool group (docker self-test closed loop).
//!
//! Drives a multi-step run — sync -> build -> deploy -> run_tests -> report —
//! shared across separate tool calls via a persistent run workspace
//! (`data/dev_selftest/runs/{run_id}/`) plus a `DevSelftestRunRecord` index.
//! Like the other catalog tools, each call is a `ToolRun` through the shared
//! `build_tool_run_task` + `run_tool_task` boundary, so the group auto-appears in
//! `/api/tools`, MCP `tools/list`, and the WebUI catalog.
//!
//! Implements git-only source sync, configured build, `docker_cluster` deploy,
//! inline Docker tests (or a local stub when no Docker target is configured), an
//! optional Docker compose cleanup step, and a rule-based report. All
//! commands/binaries/paths/compose files come from the `dev_selftest` config
//! allowlist; tool params only select profile ids and carry a `runId`.

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
    services::{
        dev_selftest_allowlist,
        dev_selftest_profiles::DevSelftestProfilesSnapshot,
        remote_execution::{self, ExecutorRunInput, ExecutorRunStatus, ExecutorTarget},
    },
    support::{
        config::{
            AppConfig, DevSelftestBuildProfile, DevSelftestGitRepo, DevSelftestSettings,
            DevSelftestTestDocker, DevSelftestTestSuite,
        },
        docker_target::is_safe_env_name,
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
pub const CLEANUP_ID: &str = "logagent.dev_selftest.cleanup";
pub const DIAGNOSE_ID: &str = "logagent.dev_selftest.diagnose";

const PROGRESS_FILE: &str = "progress.json";
const TEST_PARAMS_MAX_KEYS: usize = 16;
const TEST_PARAMS_MAX_VALUE_BYTES: usize = 2048;
const TEST_PARAMS_MAX_TOTAL_BYTES: usize = 8192;

pub fn descriptors(config: &AppConfig) -> Vec<ToolDescriptor> {
    let enabled = config.dev_selftest.enabled;
    vec![
        sync_workspace_descriptor(enabled),
        build_descriptor(enabled),
        deploy_descriptor(enabled),
        run_tests_descriptor(enabled),
        report_descriptor(enabled),
        cleanup_descriptor(enabled),
        diagnose_descriptor(enabled),
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
        SYNC_WORKSPACE_ID
            | BUILD_ID
            | DEPLOY_ID
            | RUN_TESTS_ID
            | REPORT_ID
            | CLEANUP_ID
            | DIAGNOSE_ID
    )
}

#[allow(dead_code)]
pub fn validate_run_params(
    config: &AppConfig,
    tool_id: &str,
    value: &Value,
) -> Result<Value, AppError> {
    validate_run_params_with_git_repos(
        config,
        &config.dev_selftest.git.repos,
        &DevSelftestProfilesSnapshot {
            builds: config.dev_selftest.builds.clone(),
            test_suites: config.dev_selftest.test_suites.clone(),
        },
        tool_id,
        value,
    )
}

pub fn validate_run_params_with_git_repos(
    config: &AppConfig,
    git_repos: &[DevSelftestGitRepo],
    profiles: &DevSelftestProfilesSnapshot,
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
            let repo = params
                .git_repo
                .as_deref()
                .ok_or_else(|| AppError::bad_request("gitRepo is required"))?;
            let git_ref = params
                .git_ref
                .as_deref()
                .ok_or_else(|| AppError::bad_request("gitRef is required"))?;
            if !config.dev_selftest.git.enabled {
                return Err(AppError::bad_request("dev_selftest.git is disabled"));
            }
            if !dev_selftest_allowlist::repo_ref_allowed(git_repos, repo, git_ref) {
                return Err(AppError::bad_request(
                    "git repo/ref is not in the configured allowlist",
                ));
            }
            serde_json::to_value(params)
        }
        BUILD_ID => {
            let mut params: BuildParams = parse_params(value)?;
            let profile = require_build_profile(profiles, &params.build_profile)?;
            params.profile_snapshot = Some(profile.clone());
            serde_json::to_value(params)
        }
        DEPLOY_ID => {
            let params: DeployParams = parse_params(value)?;
            require_docker_profile(config, &params.profile)?;
            serde_json::to_value(params)
        }
        RUN_TESTS_ID => {
            let mut params: RunTestsParams = parse_params(value)?;
            let suite = require_test_suite(profiles, &params.test_suite)?;
            validate_test_params(&params.test_params)?;
            params.profile_snapshot = Some(suite.clone());
            serde_json::to_value(params)
        }
        REPORT_ID => {
            let params: ReportParams = parse_params(value)?;
            serde_json::to_value(params)
        }
        CLEANUP_ID => {
            let params: CleanupParams = parse_params(value)?;
            validate_run_id(&params.run_id)?;
            if let Some(profile) = params
                .profile
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                require_docker_profile(config, profile)?;
            }
            serde_json::to_value(params)
        }
        DIAGNOSE_ID => {
            let params: DiagnoseParams = parse_params(value)?;
            validate_run_id(&params.run_id)?;
            if let Some(task_run_id) = params
                .task_run_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                validate_task_run_id(task_run_id)?;
            }
            if let Some(profile) = params
                .profile
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                require_docker_profile(config, profile)?;
            }
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
        CLEANUP_ID => run_cleanup(state, task).await,
        DIAGNOSE_ID => run_diagnose(state, task).await,
        _ => Err(AppError::bad_request(format!("unknown toolId {tool_id}"))),
    }
}

// ---------- params ----------

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
struct SyncWorkspaceParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    label: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    profile_snapshot: Option<DevSelftestBuildProfile>,
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
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    test_params: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    profile_snapshot: Option<DevSelftestTestSuite>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReportParams {
    run_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CleanupParams {
    run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    profile: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnoseParams {
    run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    task_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    step: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    profile: Option<String>,
    #[serde(default = "default_include_docker_probes")]
    include_docker_probes: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    max_evidence_bytes: Option<usize>,
}

fn default_include_docker_probes() -> bool {
    true
}

fn parse_params<T: serde::de::DeserializeOwned>(value: &Value) -> Result<T, AppError> {
    serde_json::from_value(value.clone())
        .map_err(|err| AppError::bad_request(format!("invalid dev_selftest params: {err}")))
}

fn validate_test_params(params: &BTreeMap<String, String>) -> Result<(), AppError> {
    test_param_env(params).map(|_| ())
}

fn test_param_env(params: &BTreeMap<String, String>) -> Result<BTreeMap<String, String>, AppError> {
    if params.len() > TEST_PARAMS_MAX_KEYS {
        return Err(AppError::bad_request(format!(
            "testParams supports at most {TEST_PARAMS_MAX_KEYS} keys"
        )));
    }

    let mut total_bytes = 0usize;
    let mut env = BTreeMap::new();
    let mut normalized_to_key = BTreeMap::<String, String>::new();
    for (key, value) in params {
        validate_test_param_key(key)?;
        validate_test_param_value(key, value, &mut total_bytes)?;
        let normalized = normalize_test_param_key(key)?;
        if let Some(existing) = normalized_to_key.insert(normalized.clone(), key.clone()) {
            return Err(AppError::bad_request(format!(
                "testParams keys '{existing}' and '{key}' both map to {normalized}"
            )));
        }
        let env_name = format!("DEVSELFTEST_PARAM_{normalized}");
        if !is_safe_env_name(&env_name) {
            return Err(AppError::bad_request(format!(
                "testParams key '{key}' maps to invalid env name {env_name}"
            )));
        }
        env.insert(env_name, value.clone());
    }
    Ok(env)
}

fn validate_test_param_key(key: &str) -> Result<(), AppError> {
    if key.is_empty() || key.len() > 64 {
        return Err(AppError::bad_request(
            "testParams keys must be 1-64 ASCII characters",
        ));
    }
    let mut bytes = key.bytes();
    let first = bytes.next().unwrap_or_default();
    if !first.is_ascii_alphabetic() {
        return Err(AppError::bad_request(format!(
            "testParams key '{key}' must start with an ASCII letter"
        )));
    }
    if !bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
        return Err(AppError::bad_request(format!(
            "testParams key '{key}' may only contain ASCII letters, digits, '_' and '-'"
        )));
    }
    let compact = key
        .bytes()
        .filter(|b| *b != b'_' && *b != b'-')
        .map(|b| b.to_ascii_lowercase() as char)
        .collect::<String>();
    for forbidden in [
        "password",
        "passwd",
        "token",
        "secret",
        "apikey",
        "credential",
        "auth",
    ] {
        if compact.contains(forbidden) {
            return Err(AppError::bad_request(format!(
                "testParams key '{key}' looks secret-like and is not allowed"
            )));
        }
    }
    Ok(())
}

fn validate_test_param_value(
    key: &str,
    value: &str,
    total_bytes: &mut usize,
) -> Result<(), AppError> {
    let len = value.len();
    if len == 0 {
        return Err(AppError::bad_request(format!(
            "testParams value for '{key}' must not be empty"
        )));
    }
    if len > TEST_PARAMS_MAX_VALUE_BYTES {
        return Err(AppError::bad_request(format!(
            "testParams value for '{key}' exceeds {TEST_PARAMS_MAX_VALUE_BYTES} bytes"
        )));
    }
    *total_bytes = total_bytes.saturating_add(len);
    if *total_bytes > TEST_PARAMS_MAX_TOTAL_BYTES {
        return Err(AppError::bad_request(format!(
            "testParams total value size exceeds {TEST_PARAMS_MAX_TOTAL_BYTES} bytes"
        )));
    }
    Ok(())
}

fn normalize_test_param_key(key: &str) -> Result<String, AppError> {
    let mut normalized = String::new();
    let mut prev_was_separator = false;
    let mut prev_was_lower_or_digit = false;
    for b in key.bytes() {
        match b {
            b'_' | b'-' => {
                if !normalized.is_empty() && !prev_was_separator {
                    normalized.push('_');
                    prev_was_separator = true;
                }
                prev_was_lower_or_digit = false;
            }
            b'A'..=b'Z' => {
                if prev_was_lower_or_digit && !prev_was_separator && !normalized.is_empty() {
                    normalized.push('_');
                }
                normalized.push(b as char);
                prev_was_separator = false;
                prev_was_lower_or_digit = false;
            }
            b'a'..=b'z' => {
                normalized.push(b.to_ascii_uppercase() as char);
                prev_was_separator = false;
                prev_was_lower_or_digit = true;
            }
            b'0'..=b'9' => {
                normalized.push(b as char);
                prev_was_separator = false;
                prev_was_lower_or_digit = true;
            }
            _ => {
                return Err(AppError::bad_request(format!(
                    "testParams key '{key}' contains invalid characters"
                )));
            }
        }
    }
    while normalized.ends_with('_') {
        normalized.pop();
    }
    if normalized.is_empty() {
        return Err(AppError::bad_request(format!(
            "testParams key '{key}' maps to an empty env suffix"
        )));
    }
    Ok(normalized)
}

fn test_params_summary(params: &BTreeMap<String, String>) -> Result<Value, AppError> {
    let env = test_param_env(params)?;
    let mut case_name = None::<String>;
    let entries = params
        .iter()
        .map(|(key, value)| {
            let normalized = normalize_test_param_key(key)?;
            if normalized == "CASE_NAME" {
                case_name = Some(value.clone());
            }
            Ok(json!({
                "key": key,
                "envName": format!("DEVSELFTEST_PARAM_{normalized}"),
                "valueBytes": value.len(),
            }))
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    Ok(json!({
        "count": env.len(),
        "caseName": case_name,
        "params": entries,
    }))
}

fn require_docker_profile(config: &AppConfig, id: &str) -> Result<(), AppError> {
    if config.dev_selftest.docker.clusters.contains_key(id) {
        Ok(())
    } else {
        Err(AppError::bad_request(format!(
            "unknown dev_selftest profile {id}"
        )))
    }
}

fn require_build_profile<'a>(
    profiles: &'a DevSelftestProfilesSnapshot,
    id: &str,
) -> Result<&'a DevSelftestBuildProfile, AppError> {
    profiles
        .builds
        .get(id)
        .ok_or_else(|| AppError::bad_request(format!("unknown dev_selftest profile {id}")))
}

fn require_test_suite<'a>(
    profiles: &'a DevSelftestProfilesSnapshot,
    id: &str,
) -> Result<&'a DevSelftestTestSuite, AppError> {
    profiles
        .test_suites
        .get(id)
        .ok_or_else(|| AppError::bad_request(format!("unknown dev_selftest profile {id}")))
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
    let repo = params
        .git_repo
        .as_deref()
        .ok_or_else(|| AppError::bad_request("gitRepo is required"))?;
    let git_ref = params
        .git_ref
        .as_deref()
        .ok_or_else(|| AppError::bad_request("gitRef is required"))?;
    let source_ref = format!("git:{repo}@{git_ref}");
    let (status, error) = match git_sync(settings, repo, git_ref, &source_dir).await {
        Ok(()) => ("OK", None::<String>),
        Err(err) => ("FAILED", Some(err)),
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

async fn git_sync(
    settings: &DevSelftestSettings,
    repo: &str,
    git_ref: &str,
    dest: &Path,
) -> Result<(), String> {
    if dest.join(".git").is_dir() {
        git_pull(settings, repo, git_ref, dest).await
    } else {
        let has_files = fs::read_dir(dest)
            .map_err(|err| format!("failed to inspect source dir: {err}"))?
            .next()
            .is_some();
        if has_files {
            return Err(
                "source directory exists but is not a git checkout; create a new runId".to_string(),
            );
        }
        git_clone(settings, repo, git_ref, dest).await
    }
}

async fn git_clone(
    settings: &DevSelftestSettings,
    repo: &str,
    git_ref: &str,
    dest: &Path,
) -> Result<(), String> {
    let dest_arg = dest.to_string_lossy().to_string();
    git_command(
        settings,
        &[
            "clone", "--depth", "1", "--branch", git_ref, repo, &dest_arg,
        ],
        dest.parent().unwrap_or_else(|| Path::new(".")),
    )
    .await
}

async fn git_pull(
    settings: &DevSelftestSettings,
    repo: &str,
    git_ref: &str,
    dest: &Path,
) -> Result<(), String> {
    git_command(settings, &["remote", "set-url", "origin", repo], dest).await?;
    git_command(settings, &["fetch", "--prune", "origin", git_ref], dest).await?;
    if git_command(settings, &["checkout", git_ref], dest)
        .await
        .is_err()
    {
        let remote_ref = format!("origin/{git_ref}");
        git_command(
            settings,
            &["checkout", "-b", git_ref, "--track", &remote_ref],
            dest,
        )
        .await?;
    }
    git_command(settings, &["pull", "--ff-only", "origin", git_ref], dest).await
}

async fn git_command(
    settings: &DevSelftestSettings,
    args: &[&str],
    cwd: &Path,
) -> Result<(), String> {
    let run = run_bounded_command(
        &settings.git.binary,
        &args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>(),
        cwd,
        &BTreeMap::new(),
        settings.build_timeout_seconds,
        settings.max_output_bytes,
    )
    .await;
    if run.ok {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&run.stderr).trim().to_string();
    if !stderr.is_empty() {
        return Err(stderr);
    }
    let stdout = String::from_utf8_lossy(&run.stdout).trim().to_string();
    if !stdout.is_empty() {
        return Err(stdout);
    }
    Err(run
        .error
        .unwrap_or_else(|| format!("git command failed with exit code {:?}", run.exit_code)))
}

async fn run_build(state: Arc<AppState>, task: TaskRecord) -> Result<PathBuf, AppError> {
    let params: BuildParams = parse_params(&task.tool_params)?;
    validate_run_id(&params.run_id)?;
    let record = state
        .dev_selftest
        .get(&params.run_id)
        .await
        .ok_or_else(|| AppError::bad_request(format!("unknown runId {}", params.run_id)))?;
    let profile = params
        .profile_snapshot
        .clone()
        .or_else(|| {
            state
                .dev_selftest_profiles
                .snapshot()
                .builds
                .get(&params.build_profile)
                .cloned()
        })
        .ok_or_else(|| {
            AppError::bad_request(format!("unknown build profile {}", params.build_profile))
        })?;

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

    let started = Instant::now();
    let run = if let Some(docker) = &profile.docker {
        run_docker_build(&state, &record, &profile, docker, &run_root).await?
    } else {
        let (binary, argv) = split_command(&profile.command)?;
        run_bounded_command(
            &binary,
            &argv,
            &cwd,
            &BTreeMap::new(),
            profile
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
    let mut evidence_refs = vec![
        "logs/build.stdout.txt".to_string(),
        "logs/build.stderr.txt".to_string(),
    ];
    evidence_refs.extend(artifacts.iter().map(|a| format!("artifacts/{a}")));
    append_step(
        &state,
        &record.run_id,
        new_step("build", status, duration, error.clone(), evidence_refs),
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
        "executor": profile.docker.as_ref().map(|docker| {
            json!({
                "kind": "docker",
                "image": docker.image.clone(),
                "network": docker.network.clone().unwrap_or_else(|| "host".to_string()),
            })
        }),
        "artifacts": artifacts,
        "stdoutPath": "logs/build.stdout.txt",
        "stderrPath": "logs/build.stderr.txt",
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

/// Run a build profile inside a Docker image. The profile argv is an in-container
/// command, usually a stable script baked into the image. The runner always mounts
/// the synced source and artifact directories at conventional paths so image
/// authors can avoid depending on host-specific paths.
async fn run_docker_build(
    state: &AppState,
    record: &DevSelftestRunRecord,
    profile: &DevSelftestBuildProfile,
    docker: &DevSelftestTestDocker,
    run_root: &Path,
) -> Result<BoundedRun, AppError> {
    if profile.command.is_empty() {
        return Err(AppError::bad_request("docker build profile has empty argv"));
    }

    let source_dir = run_root.join("source");
    let artifacts_dir = run_root.join("artifacts");
    fs::create_dir_all(&artifacts_dir)
        .map_err(|err| AppError::internal(format!("failed to create artifacts dir: {err}")))?;
    let project_name = format!("devselftest_{}_build", sanitize_filename(&record.run_id)?);
    let env_map = deploy_env(run_root, &source_dir, &artifacts_dir, &project_name);

    let mut volumes = Vec::with_capacity(docker.volumes.len() + 2);
    volumes.push(format!("{}:/workspace/source:rw", source_dir.display()));
    volumes.push(format!(
        "{}:/workspace/artifacts:rw",
        artifacts_dir.display()
    ));
    for volume in &docker.volumes {
        let interpolated = interpolate_volume(volume, &env_map)?;
        if !volumes.contains(&interpolated) {
            volumes.push(interpolated);
        }
    }

    let target = ExecutorTarget::Docker {
        image: docker.image.clone(),
        network: docker.network.clone(),
        workdir: docker
            .workdir
            .clone()
            .or_else(|| Some("/workspace/source".to_string())),
        volumes,
        env: docker.env.clone(),
    };
    let input = ExecutorRunInput {
        target: &target,
        argv: &profile.command,
        timeout_seconds: profile
            .timeout_seconds
            .unwrap_or(state.config.dev_selftest.build_timeout_seconds),
        extra_env: env_map,
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

fn add_deploy_port(env: &mut BTreeMap<String, String>, exposed_port: Option<u16>) {
    if let Some(port) = exposed_port {
        env.insert("DEVSELFTEST_PORT".to_string(), port.to_string());
    }
}

fn compose_project_name(run_id: &str, profile: &str) -> Result<String, AppError> {
    Ok(format!(
        "devselftest_{}_{}",
        sanitize_filename(run_id)?,
        sanitize_filename(profile)?
    ))
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
    let project_name = compose_project_name(&record.run_id, &params.profile)?;
    let mut env = deploy_env(&run_root, &source_dir, &artifacts_dir, &project_name);
    add_deploy_port(&mut env, cluster.exposed_port);
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

    // Health check (declared command, e.g. curl or `true`). Failure does not roll back.
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
        "stdoutPath": "logs/deploy.stdout.txt",
        "stderrPath": "logs/deploy.stderr.txt",
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

async fn run_cleanup(state: Arc<AppState>, task: TaskRecord) -> Result<PathBuf, AppError> {
    let params: CleanupParams = parse_params(&task.tool_params)?;
    validate_run_id(&params.run_id)?;
    let record = state
        .dev_selftest
        .get(&params.run_id)
        .await
        .ok_or_else(|| AppError::bad_request(format!("unknown runId {}", params.run_id)))?;
    let profile = params
        .profile
        .clone()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| match &record.deploy_target {
            Some(DevSelftestDeployTarget::Docker { cluster, .. }) => Some(cluster.clone()),
            _ => None,
        })
        .ok_or_else(|| {
            AppError::bad_request(
                "cleanup profile is required when the run has no docker deploy target",
            )
        })?;
    let cluster = state
        .config
        .dev_selftest
        .docker
        .clusters
        .get(&profile)
        .ok_or_else(|| AppError::bad_request(format!("unknown docker cluster {profile}")))?
        .clone();

    let run_root = run_dir(&state, &record.run_id);
    let source_dir = run_root.join("source");
    let artifacts_dir = run_root.join("artifacts");
    let project_name = compose_project_name(&record.run_id, &profile)?;
    let mut env = deploy_env(&run_root, &source_dir, &artifacts_dir, &project_name);
    add_deploy_port(&mut env, cluster.exposed_port);
    let started = Instant::now();
    let run = run_bounded_command(
        &state.config.dev_selftest.docker.binary,
        &[
            "compose".to_string(),
            "-p".to_string(),
            project_name.clone(),
            "-f".to_string(),
            cluster.compose_file.to_string_lossy().to_string(),
            "down".to_string(),
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
    write_bytes(&logs_dir.join("cleanup.stdout.txt"), &run.stdout)?;
    write_bytes(&logs_dir.join("cleanup.stderr.txt"), &run.stderr)?;

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
            "cleanup",
            status,
            duration,
            error.clone(),
            vec![
                "logs/cleanup.stdout.txt".to_string(),
                "logs/cleanup.stderr.txt".to_string(),
            ],
        ),
    )?;

    let action_id = task_action_id(CLEANUP_ID, &task.task_id);
    let result = json!({
        "schemaVersion": 1,
        "toolId": CLEANUP_ID,
        "actionId": action_id,
        "runId": record.run_id,
        "profile": profile,
        "projectName": project_name,
        "status": status,
        "exitCode": run.exit_code,
        "stdoutPath": "logs/cleanup.stdout.txt",
        "stderrPath": "logs/cleanup.stderr.txt",
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

async fn run_diagnose(state: Arc<AppState>, task: TaskRecord) -> Result<PathBuf, AppError> {
    let params: DiagnoseParams = parse_params(&task.tool_params)?;
    validate_run_id(&params.run_id)?;
    let record = state
        .dev_selftest
        .get(&params.run_id)
        .await
        .ok_or_else(|| AppError::bad_request(format!("unknown runId {}", params.run_id)))?;
    let run_root = run_dir(&state, &record.run_id);
    let progress = read_progress(&run_root);
    let evidence_limit = diagnostic_limit(
        params.max_evidence_bytes,
        state.config.dev_selftest.max_output_bytes,
    );
    let selected_step = select_diagnostic_step(&progress.steps, params.step.as_deref())?;
    let evidence_paths = selected_step
        .as_ref()
        .map(|step| evidence_paths_for_step(step))
        .unwrap_or_default();
    let evidence = read_evidence_set(&run_root, &evidence_paths, evidence_limit).await?;

    let (docker_profile, docker_project, docker_probes) = if params.include_docker_probes {
        match docker_probe_context(&state, &record, params.profile.as_deref())? {
            Some((profile, project_name, cluster)) => {
                let probes = run_docker_probes(
                    &state,
                    &run_root,
                    &profile,
                    &project_name,
                    &cluster,
                    evidence_limit,
                )
                .await?;
                (Some(profile), Some(project_name), probes)
            }
            None => (None, None, Vec::new()),
        }
    } else {
        (None, None, Vec::new())
    };

    let task_context = task_context(&state, params.task_run_id.as_deref()).await?;
    let step_name = selected_step
        .as_ref()
        .map(|step| step.step.as_str())
        .unwrap_or("unknown");
    let corpus = diagnostic_corpus(
        selected_step.as_ref(),
        &evidence,
        &docker_probes,
        task_context.as_ref(),
    );
    let category = classify_diagnostic(step_name, &corpus, &docker_probes);
    let confidence = diagnostic_confidence(&category, &evidence, &docker_probes);
    let recommendations = diagnostic_recommendations(
        &category,
        &record.run_id,
        docker_profile.as_deref(),
        docker_project.as_deref(),
    );
    let summary = diagnostic_summary(&category, selected_step.as_ref(), docker_project.as_deref());

    let action_id = task_action_id(DIAGNOSE_ID, &task.task_id);
    let result = json!({
        "schemaVersion": 1,
        "toolId": DIAGNOSE_ID,
        "actionId": action_id,
        "runId": record.run_id,
        "status": "OK",
        "diagnosedStep": selected_step.as_ref().map(|step| step.step.clone()),
        "category": category,
        "confidence": confidence,
        "summary": summary,
        "profile": docker_profile,
        "projectName": docker_project,
        "taskRun": task_context,
        "evidence": evidence,
        "dockerProbes": docker_probes,
        "recommendedActions": recommendations,
        "maxEvidenceBytes": evidence_limit,
        "createdAt": Utc::now(),
    });
    write_tool_result(
        &state.config.storage.workspace_dir(&task.task_id),
        &action_id,
        &result,
    )
}

fn diagnostic_limit(requested: Option<usize>, configured_max: usize) -> usize {
    let cap = configured_max.clamp(1024, 64 * 1024);
    requested.unwrap_or(16 * 1024).clamp(1024, cap)
}

fn select_diagnostic_step(
    steps: &[DevSelftestStep],
    requested: Option<&str>,
) -> Result<Option<DevSelftestStep>, AppError> {
    if let Some(step) = requested.map(str::trim).filter(|value| !value.is_empty()) {
        return steps
            .iter()
            .find(|entry| entry.step == step)
            .cloned()
            .map(Some)
            .ok_or_else(|| {
                AppError::bad_request(format!("step {step} is not present in run progress"))
            });
    }
    if let Some(step) = steps
        .iter()
        .find(|entry| entry.status != "OK" && entry.step != "cleanup")
        .cloned()
    {
        return Ok(Some(step));
    }
    if let Some(step) = steps.iter().find(|entry| entry.status != "OK").cloned() {
        return Ok(Some(step));
    }
    Ok(steps.last().cloned())
}

fn evidence_paths_for_step(step: &DevSelftestStep) -> Vec<String> {
    let mut paths = Vec::new();
    for path in default_evidence_paths(&step.step) {
        push_unique(&mut paths, path);
    }
    for path in &step.evidence_refs {
        push_unique(&mut paths, path.clone());
    }
    push_unique(&mut paths, PROGRESS_FILE.to_string());
    if matches!(
        step.step.as_str(),
        "report" | "deploy" | "run_tests" | "cleanup"
    ) {
        push_unique(&mut paths, "report.json".to_string());
    }
    paths
}

fn default_evidence_paths(step: &str) -> Vec<String> {
    match step {
        "build" => vec![
            "logs/build.stdout.txt".to_string(),
            "logs/build.stderr.txt".to_string(),
        ],
        "deploy" => vec![
            "logs/deploy.stdout.txt".to_string(),
            "logs/deploy.stderr.txt".to_string(),
        ],
        "run_tests" => vec![
            "logs/tests.stdout.txt".to_string(),
            "logs/tests.stderr.txt".to_string(),
        ],
        "cleanup" => vec![
            "logs/cleanup.stdout.txt".to_string(),
            "logs/cleanup.stderr.txt".to_string(),
        ],
        "report" => vec!["report.md".to_string(), "report.json".to_string()],
        _ => Vec::new(),
    }
}

fn push_unique(items: &mut Vec<String>, value: String) {
    if !items.contains(&value) {
        items.push(value);
    }
}

async fn read_evidence_set(
    run_root: &Path,
    paths: &[String],
    limit: usize,
) -> Result<Vec<Value>, AppError> {
    let mut evidence = Vec::new();
    for logical_path in paths {
        evidence.push(read_evidence(run_root, logical_path, limit).await?);
    }
    Ok(evidence)
}

async fn read_evidence(
    run_root: &Path,
    logical_path: &str,
    limit: usize,
) -> Result<Value, AppError> {
    let resolved = safe_join(run_root, &PathBuf::from(logical_path)).map_err(|err| {
        AppError::bad_request(format!("unsafe evidence path {logical_path}: {err}"))
    })?;
    let metadata = match tokio::fs::metadata(&resolved).await {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(json!({
                "path": logical_path,
                "exists": false,
                "bytes": 0,
                "truncated": false,
                "text": ""
            }));
        }
        Err(err) => {
            return Err(AppError::internal(format!(
                "failed to inspect evidence {logical_path}: {err}"
            )));
        }
    };
    if !metadata.is_file() {
        return Ok(json!({
            "path": logical_path,
            "exists": false,
            "bytes": metadata.len(),
            "truncated": false,
            "text": ""
        }));
    }
    let bytes = tokio::fs::read(&resolved).await.map_err(|err| {
        AppError::internal(format!("failed to read evidence {logical_path}: {err}"))
    })?;
    let (text, truncated) = bounded_tail_text(&bytes, limit);
    Ok(json!({
        "path": logical_path,
        "exists": true,
        "bytes": bytes.len() as u64,
        "truncated": truncated,
        "text": redact_secrets(&text)
    }))
}

fn bounded_tail_text(bytes: &[u8], limit: usize) -> (String, bool) {
    if bytes.len() <= limit {
        return (String::from_utf8_lossy(bytes).into_owned(), false);
    }
    let start = bytes.len().saturating_sub(limit);
    (String::from_utf8_lossy(&bytes[start..]).into_owned(), true)
}

fn redact_secrets(text: &str) -> String {
    text.lines()
        .map(redact_secret_line)
        .collect::<Vec<_>>()
        .join("\n")
}

fn redact_secret_line(line: &str) -> String {
    let lower = line.to_ascii_lowercase();
    if lower.contains("authorization:") {
        return redact_after_delimiter(line, ':');
    }
    if lower.contains("bearer ") {
        return "<redacted bearer token>".to_string();
    }
    for key in ["password", "passwd", "token", "secret", "api_key", "apikey"] {
        if lower.contains(key) {
            if line.contains('=') {
                return redact_after_delimiter(line, '=');
            }
            if line.contains(':') {
                return redact_after_delimiter(line, ':');
            }
            return "<redacted secret>".to_string();
        }
    }
    line.to_string()
}

fn redact_after_delimiter(line: &str, delimiter: char) -> String {
    match line.split_once(delimiter) {
        Some((prefix, _)) => format!("{prefix}{delimiter} <redacted>"),
        None => "<redacted secret>".to_string(),
    }
}

fn docker_probe_context(
    state: &AppState,
    record: &DevSelftestRunRecord,
    requested_profile: Option<&str>,
) -> Result<
    Option<(
        String,
        String,
        crate::support::config::DevSelftestDockerCluster,
    )>,
    AppError,
> {
    let profile = requested_profile
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| match &record.deploy_target {
            Some(DevSelftestDeployTarget::Docker { cluster, .. }) => Some(cluster.clone()),
            _ => None,
        });
    let Some(profile) = profile else {
        return Ok(None);
    };
    let cluster = state
        .config
        .dev_selftest
        .docker
        .clusters
        .get(&profile)
        .ok_or_else(|| AppError::bad_request(format!("unknown docker cluster {profile}")))?
        .clone();
    let project_name = compose_project_name(&record.run_id, &profile)?;
    Ok(Some((profile, project_name, cluster)))
}

async fn run_docker_probes(
    state: &AppState,
    run_root: &Path,
    profile: &str,
    project_name: &str,
    cluster: &crate::support::config::DevSelftestDockerCluster,
    limit: usize,
) -> Result<Vec<Value>, AppError> {
    let source_dir = run_root.join("source");
    let artifacts_dir = run_root.join("artifacts");
    let mut env = deploy_env(run_root, &source_dir, &artifacts_dir, project_name);
    add_deploy_port(&mut env, cluster.exposed_port);
    let mut probes = Vec::new();
    probes.push(
        run_docker_probe(
            state,
            run_root,
            &env,
            "compose_ps",
            vec![
                "compose".to_string(),
                "-p".to_string(),
                project_name.to_string(),
                "-f".to_string(),
                cluster.compose_file.to_string_lossy().to_string(),
                "ps".to_string(),
                "--all".to_string(),
            ],
            limit,
        )
        .await,
    );
    probes.push(
        run_docker_probe(
            state,
            run_root,
            &env,
            "compose_logs_tail",
            vec![
                "compose".to_string(),
                "-p".to_string(),
                project_name.to_string(),
                "-f".to_string(),
                cluster.compose_file.to_string_lossy().to_string(),
                "logs".to_string(),
                "--no-color".to_string(),
                "--tail".to_string(),
                "80".to_string(),
            ],
            limit,
        )
        .await,
    );
    probes.push(
        run_docker_probe(
            state,
            run_root,
            &env,
            "docker_ps_project",
            vec![
                "ps".to_string(),
                "-a".to_string(),
                "--filter".to_string(),
                format!("label=com.docker.compose.project={project_name}"),
                "--format".to_string(),
                "{{.Names}}\t{{.Status}}\t{{.Ports}}".to_string(),
            ],
            limit,
        )
        .await,
    );
    if let Some(port) = cluster.exposed_port {
        probes.push(
            run_docker_probe(
                state,
                run_root,
                &env,
                "docker_ps_port",
                vec![
                    "ps".to_string(),
                    "--filter".to_string(),
                    format!("publish={port}"),
                    "--format".to_string(),
                    "{{.Names}}\t{{.Status}}\t{{.Ports}}".to_string(),
                ],
                limit,
            )
            .await,
        );
    }
    for probe in &mut probes {
        if let Some(object) = probe.as_object_mut() {
            object.insert("profile".to_string(), json!(profile));
        }
    }
    Ok(probes)
}

async fn run_docker_probe(
    state: &AppState,
    run_root: &Path,
    env: &BTreeMap<String, String>,
    name: &str,
    argv: Vec<String>,
    limit: usize,
) -> Value {
    let timeout_seconds = state.config.dev_selftest.build_timeout_seconds.clamp(1, 10);
    let run = run_bounded_command(
        &state.config.dev_selftest.docker.binary,
        &argv,
        run_root,
        env,
        timeout_seconds,
        limit,
    )
    .await;
    let stdout = redact_secrets(&String::from_utf8_lossy(&run.stdout));
    let stderr = redact_secrets(&String::from_utf8_lossy(&run.stderr));
    json!({
        "name": name,
        "argv": argv,
        "status": if run.ok { "OK" } else { "FAILED" },
        "exitCode": run.exit_code,
        "stdout": stdout,
        "stderr": stderr,
        "error": run.error,
        "durationMs": run.duration_ms
    })
}

async fn task_context(
    state: &AppState,
    task_run_id: Option<&str>,
) -> Result<Option<Value>, AppError> {
    let Some(task_run_id) = task_run_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    validate_task_run_id(task_run_id)?;
    let Some(task) = state.tasks.get(task_run_id).await else {
        return Ok(Some(json!({
            "runId": task_run_id,
            "exists": false
        })));
    };
    Ok(Some(json!({
        "runId": task.task_id,
        "exists": true,
        "toolId": task.tool_id,
        "status": task.status,
        "phase": task.phase,
        "error": task.error,
        "resultAvailable": task.tool_result_path.is_some() || task.remote_result_path.is_some()
    })))
}

fn diagnostic_corpus(
    step: Option<&DevSelftestStep>,
    evidence: &[Value],
    probes: &[Value],
    task_context: Option<&Value>,
) -> String {
    let mut corpus = String::new();
    if let Some(step) = step {
        corpus.push_str(&step.step);
        corpus.push('\n');
        corpus.push_str(&step.status);
        corpus.push('\n');
        if let Some(error) = &step.error {
            corpus.push_str(error);
            corpus.push('\n');
        }
    }
    for item in evidence {
        if let Some(text) = item.get("text").and_then(|value| value.as_str()) {
            corpus.push_str(text);
            corpus.push('\n');
        }
    }
    for item in probes {
        for field in ["stdout", "stderr", "error"] {
            if let Some(text) = item.get(field).and_then(|value| value.as_str()) {
                corpus.push_str(text);
                corpus.push('\n');
            }
        }
    }
    if let Some(task_context) = task_context {
        corpus.push_str(&task_context.to_string());
    }
    corpus.to_ascii_lowercase()
}

fn classify_diagnostic(step: &str, corpus: &str, probes: &[Value]) -> String {
    if step == "build" {
        return "build_failed".to_string();
    }
    if step == "run_tests" {
        return "test_failed".to_string();
    }
    if step == "cleanup" {
        return "cleanup_failed".to_string();
    }
    if step != "deploy" {
        return "insufficient_evidence".to_string();
    }

    let current_project_containers = probe_stdout_nonempty(probes, "compose_ps")
        || probe_stdout_nonempty(probes, "docker_ps_project");
    let port_conflict = contains_any(
        corpus,
        &[
            "port is already allocated",
            "ports are not available",
            "address already in use",
            "bind for 0.0.0.0",
            "listen tcp",
        ],
    ) || probe_stdout_nonempty(probes, "docker_ps_port");
    if current_project_containers
        && (port_conflict
            || contains_any(
                corpus,
                &["already exists", "is already in use by container"],
            ))
    {
        return "stale_compose_project".to_string();
    }
    if port_conflict {
        return "port_conflict".to_string();
    }
    if contains_any(
        corpus,
        &["health check failed", "healthcheck", "health check"],
    ) {
        return "health_check_failed".to_string();
    }
    if contains_any(
        corpus,
        &["exited", "restarting", "unhealthy", "panic", "fatal"],
    ) {
        return "container_crash".to_string();
    }
    if corpus.trim().is_empty() {
        "insufficient_evidence".to_string()
    } else {
        "compose_up_failed".to_string()
    }
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

fn probe_stdout_nonempty(probes: &[Value], name: &str) -> bool {
    probes.iter().any(|probe| {
        probe.get("name").and_then(|value| value.as_str()) == Some(name)
            && probe
                .get("stdout")
                .and_then(|value| value.as_str())
                .map(|text| {
                    !text.trim().is_empty() && !text.to_ascii_lowercase().contains("no containers")
                })
                .unwrap_or(false)
    })
}

fn diagnostic_confidence(category: &str, evidence: &[Value], probes: &[Value]) -> &'static str {
    match category {
        "port_conflict" | "stale_compose_project" if !probes.is_empty() => "high",
        "port_conflict" | "stale_compose_project" => "medium",
        "health_check_failed" | "container_crash" if !probes.is_empty() => "medium",
        "build_failed" | "test_failed" | "cleanup_failed" if !evidence.is_empty() => "medium",
        "insufficient_evidence" => "low",
        _ => "medium",
    }
}

fn diagnostic_summary(
    category: &str,
    step: Option<&DevSelftestStep>,
    project_name: Option<&str>,
) -> String {
    let step_name = step.map(|step| step.step.as_str()).unwrap_or("unknown");
    match category {
        "port_conflict" => format!(
            "{step_name} failed because Docker reports an exposed port is already allocated."
        ),
        "stale_compose_project" => format!(
            "{step_name} failed and the derived compose project {} still has containers.",
            project_name.unwrap_or("<unknown>")
        ),
        "health_check_failed" => format!(
            "{step_name} reached docker compose but the configured health check did not pass."
        ),
        "container_crash" => format!(
            "{step_name} reached docker compose and one or more containers appear unhealthy or exited."
        ),
        "build_failed" => "Remote build failed; inspect build stdout/stderr and fix source or build profile before retrying.".to_string(),
        "test_failed" => "Remote test suite failed; inspect test stdout/stderr and keep the environment until the failure is understood.".to_string(),
        "cleanup_failed" => "Cleanup failed; inspect cleanup stdout/stderr and retry cleanup if the compose project still exists.".to_string(),
        "compose_up_failed" => format!("{step_name} failed during docker compose up."),
        _ => "The run does not contain enough evidence for a specific diagnosis.".to_string(),
    }
}

fn diagnostic_recommendations(
    category: &str,
    run_id: &str,
    profile: Option<&str>,
    project_name: Option<&str>,
) -> Vec<Value> {
    let mut recommendations = Vec::new();
    match category {
        "port_conflict" => {
            recommendations.push(json!({
                "kind": "inspect_port_owner",
                "message": "Review docker_ps_port evidence to identify which container owns the exposed port."
            }));
            if let Some(profile) = profile {
                recommendations.push(cleanup_recommendation(run_id, profile, project_name));
            }
        }
        "stale_compose_project" => {
            if let Some(profile) = profile {
                recommendations.push(cleanup_recommendation(run_id, profile, project_name));
            }
        }
        "health_check_failed" | "container_crash" => {
            recommendations.push(json!({
                "kind": "inspect_compose_logs",
                "message": "Inspect compose logs and service status before cleanup; the environment is preserved for debugging."
            }));
            if let Some(profile) = profile {
                recommendations.push(cleanup_recommendation(run_id, profile, project_name));
            }
        }
        "build_failed" => recommendations.push(json!({
            "kind": "fix_and_resync",
            "message": "Fix the source or build profile, commit and push, rerun sync_workspace with the same devselftest runId, then rerun build."
        })),
        "test_failed" => recommendations.push(json!({
            "kind": "inspect_test_output",
            "message": "Use the test stdout/stderr evidence to decide whether to fix source, test suite, or deploy profile."
        })),
        "cleanup_failed" => recommendations.push(json!({
            "kind": "retry_cleanup",
            "message": "Retry logagent.dev_selftest.cleanup with the same runId/profile after inspecting cleanup evidence."
        })),
        _ => recommendations.push(json!({
            "kind": "collect_report",
            "message": "Run logagent.dev_selftest.report, then rerun diagnose with a specific failed step if needed."
        })),
    }
    recommendations
}

fn cleanup_recommendation(run_id: &str, profile: &str, project_name: Option<&str>) -> Value {
    json!({
        "kind": "cleanup",
        "message": "If this run's compose environment can be released, call cleanup; it runs docker compose down without deleting evidence.",
        "tool": CLEANUP_ID,
        "arguments": {
            "runId": run_id,
            "profile": profile
        },
        "projectName": project_name
    })
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
    let suite = params
        .profile_snapshot
        .clone()
        .or_else(|| {
            state
                .dev_selftest_profiles
                .snapshot()
                .test_suites
                .get(&params.test_suite)
                .cloned()
        })
        .ok_or_else(|| {
            AppError::bad_request(format!("unknown test suite {}", params.test_suite))
        })?;

    let run_root = run_dir(&state, &record.run_id);
    let started = Instant::now();
    // Dispatch priority: an inline docker target (`suite.docker`) > local stub argv.
    // Both produce a BoundedRun so the log/step/result handling below is shared.
    let run = if let Some(docker) = &suite.docker {
        run_docker_test(
            &state,
            &record,
            &suite,
            docker,
            &run_root,
            &params.test_params,
        )
        .await?
    } else {
        let env = target_env(&record, &suite, &params.test_params)?;
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
            "image": docker.image.clone(),
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
        "testParamsSummary": test_params_summary(&params.test_params)?,
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
    test_params: &BTreeMap<String, String>,
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
    for (key, value) in test_param_env(test_params)? {
        extra_env.insert(key, value);
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
    test_params: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, AppError> {
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
    for (key, value) in test_param_env(test_params)? {
        env.insert(key, value);
    }
    Ok(env)
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
        .filter(|step| step.status != "OK" && step.step != "cleanup")
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

fn validate_task_run_id(run_id: &str) -> Result<(), AppError> {
    let valid = run_id.starts_with("task_")
        && run_id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'_' || value == b'-');
    if valid {
        Ok(())
    } else {
        Err(AppError::bad_request("invalid taskRunId"))
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
        "Create or reuse a dev-self-test run and populate source/ from a configured git repo/ref. New runs clone; existing git workspaces pull fast-forward updates. Returns runId.",
        enabled,
    );
    d.params_schema = json!({
        "type": "object",
        "properties": {
            "runId": { "type": "string", "description": "Omit to create a new run." },
            "label": { "type": "string" },
            "gitRepo": { "type": "string", "description": "Must be in the configured git repos allowlist." },
            "gitRef": { "type": "string", "description": "Must be in the repo's allowed refs. Usually the branch pushed by the Windows-side MCP client." }
        },
        "required": ["gitRepo", "gitRef"]
    });
    d.params_template = json!({ "runId": "", "label": "", "gitRepo": "", "gitRef": "" });
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
        "Deploy via a configured docker_cluster profile (docker compose up -d + declared health check).",
        enabled,
    );
    d.params_schema = json!({
        "type": "object",
        "properties": {
            "runId": { "type": "string" },
            "profile": { "type": "string", "description": "A configured dev_selftest.docker.clusters profile id." }
        },
        "required": ["runId", "profile"]
    });
    d.params_template = json!({ "runId": "" });
    d
}

fn run_tests_descriptor(enabled: bool) -> ToolDescriptor {
    let mut d = base_descriptor(
        RUN_TESTS_ID,
        "Dev self-test: run tests",
        "Run a configured test suite against the run's deployed target. Suites with a docker target run in an inline Docker container; others use the local stub argv. Runnable sync or runMode:'queued'.",
        enabled,
    );
    d.params_schema = json!({
        "type": "object",
        "properties": {
            "runId": { "type": "string" },
            "testSuite": { "type": "string", "description": "A configured dev_selftest.test_suites profile id." },
            "testParams": {
                "type": "object",
                "description": "Optional non-secret runtime parameters passed to the test process as DEVSELFTEST_PARAM_* env vars. Values are visible in docker argv; never pass credentials.",
                "additionalProperties": { "type": "string" },
                "maxProperties": TEST_PARAMS_MAX_KEYS
            }
        },
        "required": ["runId", "testSuite"]
    });
    d.params_template = json!({ "runId": "", "testSuite": "", "testParams": {} });
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

fn cleanup_descriptor(enabled: bool) -> ToolDescriptor {
    let mut d = base_descriptor(
        CLEANUP_ID,
        "Dev self-test: cleanup",
        "Optionally clean up the run's Docker compose environment with the configured compose profile. This runs docker compose down for the derived project name and keeps source, logs, artifacts, progress, and reports for audit.",
        enabled,
    );
    d.params_schema = json!({
        "type": "object",
        "properties": {
            "runId": { "type": "string" },
            "profile": {
                "type": "string",
                "description": "Optional dev_selftest.docker.clusters profile id. Omit to use the run's deployed docker target."
            }
        },
        "required": ["runId"]
    });
    d.params_template = json!({ "runId": "", "profile": "" });
    d
}

fn diagnose_descriptor(enabled: bool) -> ToolDescriptor {
    let mut d = base_descriptor(
        DIAGNOSE_ID,
        "Dev self-test: diagnose",
        "Read a dev_selftest run's bounded evidence and run allowlisted read-only Docker probes to classify build/deploy/test/cleanup failures.",
        enabled,
    );
    d.read_only = true;
    d.params_schema = json!({
        "type": "object",
        "properties": {
            "runId": { "type": "string", "description": "Persistent devselftest_* run id." },
            "taskRunId": { "type": "string", "description": "Optional queued task_* id to include in diagnostic context." },
            "step": {
                "type": "string",
                "enum": ["sync_workspace", "build", "deploy", "run_tests", "report", "cleanup"],
                "description": "Optional step to diagnose. Omit to use the first failed step."
            },
            "profile": {
                "type": "string",
                "description": "Optional dev_selftest.docker.clusters profile id for Docker probes. Omit to use the run's deployed docker target."
            },
            "includeDockerProbes": {
                "type": "boolean",
                "default": true,
                "description": "When true, run allowlisted read-only docker compose/docker ps probes for Docker deploy targets."
            },
            "maxEvidenceBytes": {
                "type": "integer",
                "minimum": 1024,
                "description": "Maximum bytes returned per evidence/probe text, capped by server dev_selftest limits."
            }
        },
        "required": ["runId"]
    });
    d.params_template = json!({
        "runId": "",
        "taskRunId": "",
        "step": "",
        "profile": "",
        "includeDockerProbes": true,
        "maxEvidenceBytes": 16384
    });
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_GIT_REPO: &str = "https://example.test/project.git";
    const TEST_GIT_REF: &str = "main";

    fn sync_params(label: &str) -> Value {
        json!({
            "label": label,
            "gitRepo": TEST_GIT_REPO,
            "gitRef": TEST_GIT_REF,
        })
    }

    #[test]
    fn descriptors_gated_by_enabled() {
        let config = test_config(false);
        let ds = descriptors(&config);
        assert_eq!(ds.len(), 7);
        assert!(ds.iter().all(|d| !d.enabled && !d.runnable));
        assert!(ds.iter().all(|d| d.backend == "dev_selftest"));
        let config = test_config(true);
        let ds = descriptors(&config);
        assert!(ds.iter().all(|d| d.enabled && d.runnable));
        assert!(get_descriptor(&config, BUILD_ID).is_some());
        assert!(get_descriptor(&config, CLEANUP_ID).is_some());
        assert!(get_descriptor(&config, DIAGNOSE_ID).is_some());
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
        assert!(
            validate_run_params(&config, SYNC_WORKSPACE_ID, &json!({"label":"missing-git"}))
                .is_err()
        );
        assert!(validate_run_params(
            &config,
            SYNC_WORKSPACE_ID,
            &json!({"label":"upload","uploadId":"upl_1"})
        )
        .is_err());
        assert!(validate_run_params(
            &config,
            SYNC_WORKSPACE_ID,
            &json!({"gitRepo":TEST_GIT_REPO,"gitRef":"unknown"})
        )
        .is_err());
        assert!(validate_run_params(&config, SYNC_WORKSPACE_ID, &sync_params("git")).is_ok());
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
        assert!(
            validate_run_params(&config, CLEANUP_ID, &json!({"runId":"devselftest_x"})).is_ok()
        );
        assert!(validate_run_params(
            &config,
            CLEANUP_ID,
            &json!({"runId":"devselftest_x","profile":"local"})
        )
        .is_ok());
        assert!(validate_run_params(
            &config,
            CLEANUP_ID,
            &json!({"runId":"devselftest_x","profile":"missing"})
        )
        .is_err());
        assert!(validate_run_params(
            &config,
            DIAGNOSE_ID,
            &json!({"runId":"devselftest_x","profile":"local","taskRunId":"task_1"})
        )
        .is_ok());
        assert!(validate_run_params(
            &config,
            DIAGNOSE_ID,
            &json!({"runId":"devselftest_x","profile":"missing"})
        )
        .is_err());
        assert!(validate_run_params(
            &config,
            DIAGNOSE_ID,
            &json!({"runId":"devselftest_x","taskRunId":"devselftest_wrong"})
        )
        .is_err());
    }

    #[test]
    fn validate_run_id_format() {
        assert!(validate_run_id("devselftest_abc-1").is_ok());
        assert!(validate_run_id("task_x").is_err());
        assert!(validate_run_id("devselftest_bad/id").is_err());
    }

    #[test]
    fn validates_and_summarizes_test_params() {
        let params = BTreeMap::from([
            ("caseName".to_string(), "opengemini_rw_smoke".to_string()),
            ("instance-id".to_string(), "inst-1".to_string()),
        ]);
        let env = test_param_env(&params).unwrap();
        assert_eq!(
            env.get("DEVSELFTEST_PARAM_CASE_NAME").unwrap(),
            "opengemini_rw_smoke"
        );
        assert_eq!(env.get("DEVSELFTEST_PARAM_INSTANCE_ID").unwrap(), "inst-1");
        let summary = test_params_summary(&params).unwrap();
        assert_eq!(summary["count"], 2);
        assert_eq!(summary["caseName"], "opengemini_rw_smoke");
        assert_eq!(summary["params"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn rejects_bad_test_params() {
        assert!(
            test_param_env(&BTreeMap::from([("apiToken".to_string(), "x".to_string())])).is_err()
        );
        assert!(
            test_param_env(&BTreeMap::from([("caseName".to_string(), String::new())])).is_err()
        );
        assert!(test_param_env(&BTreeMap::from([
            ("caseName".to_string(), "a".to_string()),
            ("case-name".to_string(), "b".to_string()),
        ]))
        .is_err());

        let config = test_config(true);
        assert!(validate_run_params(
            &config,
            RUN_TESTS_ID,
            &json!({
                "runId": "devselftest_x",
                "testSuite": "stub",
                "testParams": { "caseName": 1 }
            })
        )
        .is_err());
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
        let mut with_port = env.clone();
        add_deploy_port(&mut with_port, Some(18086));
        assert_eq!(with_port.get("DEVSELFTEST_PORT").unwrap(), "18086");
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
            DevSelftestDockerSettings, DevSelftestGitRepo, DevSelftestGitSettings,
            DevSelftestSettings, DevSelftestTestSuite, LogAnalyzerSettings, McpSettings,
            RemoteExecutionSettings, ServerSettings, StorageSettings, ToolsSettings,
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
                docker: None,
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
            log_analyzer: LogAnalyzerSettings {
                keywords: Vec::new(),
                max_matches: 0,
            },
            tools: ToolsSettings::default(),
            remote_execution: RemoteExecutionSettings::default(),
            mcp: McpSettings::default(),
            dev_selftest: DevSelftestSettings {
                enabled,
                build_timeout_seconds: 30,
                max_output_bytes: 1024,
                git: DevSelftestGitSettings {
                    enabled: true,
                    binary: PathBuf::from("/usr/bin/git"),
                    repos: vec![DevSelftestGitRepo {
                        url: TEST_GIT_REPO.to_string(),
                        refs: vec![TEST_GIT_REF.to_string()],
                    }],
                },
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
  echo "cloned" > "$dest/SYNCED.txt"
  exit 0
fi
if [ "${1:-}" = "remote" ] || [ "${1:-}" = "fetch" ] || [ "${1:-}" = "checkout" ]; then
  exit 0
fi
if [ "${1:-}" = "pull" ]; then
  echo "pulled" >> "SYNCED.txt"
  exit 0
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
        let fake_git = write_fake_git(&root);

        let mut config = test_config(true);
        config.storage.data_dir = root.join("data");
        config.dev_selftest.max_output_bytes = 16 * 1024;
        config.dev_selftest.docker.binary = fake_docker;
        config.dev_selftest.git.binary = fake_git;
        let config = Arc::new(config);
        config.prepare_dirs().unwrap();
        (crate::app::AppState::new(config).unwrap(), root)
    }

    #[cfg(all(test, unix))]
    struct ToolOut {
        status: String,
        run_id: String,
        value: Value,
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
            value,
        }
    }

    #[tokio::test]
    #[cfg(all(test, unix))]
    async fn docker_selftest_closed_loop() {
        let (state, root) = test_state_with_dev_selftest("dev-selftest-loop");

        let sync = run_tool(&state, SYNC_WORKSPACE_ID, sync_params("loop")).await;
        assert_eq!(sync.status, "OK");
        let run_id = sync.run_id;
        let resync = run_tool(
            &state,
            SYNC_WORKSPACE_ID,
            json!({"runId":run_id.clone(),"gitRepo":TEST_GIT_REPO,"gitRef":TEST_GIT_REF}),
        )
        .await;
        assert_eq!(resync.status, "OK");
        let synced = std::fs::read_to_string(
            state
                .config
                .storage
                .dev_selftest_run_dir(&run_id)
                .join("source/SYNCED.txt"),
        )
        .unwrap();
        assert!(synced.contains("pulled"), "synced marker: {synced}");

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
        config.dev_selftest.max_output_bytes = 16 * 1024;
        config.dev_selftest.docker.binary = fake_docker;
        config.dev_selftest.git.binary = write_fake_git(&root);
        config.dev_selftest.builds.insert(
            "docker_build".to_string(),
            DevSelftestBuildProfile {
                display_name: "docker build".to_string(),
                command: vec!["/usr/local/bin/build-selftest".to_string()],
                working_dir: String::new(),
                artifact_globs: Vec::new(),
                timeout_seconds: Some(30),
                docker: Some(DevSelftestTestDocker {
                    image: "selftest-builder:latest".to_string(),
                    network: Some("host".to_string()),
                    workdir: None,
                    volumes: Vec::new(),
                    env: BTreeMap::new(),
                }),
            },
        );
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
    async fn docker_build_profile_dispatch() {
        let (state, root) = test_state_with_docker_suite("dev-selftest-docker-build");

        let sync = run_tool(&state, SYNC_WORKSPACE_ID, sync_params("docker-build")).await;
        assert_eq!(sync.status, "OK");
        let run_id = sync.run_id;

        let build = run_tool(
            &state,
            BUILD_ID,
            json!({"runId":run_id,"buildProfile":"docker_build"}),
        )
        .await;
        assert_eq!(build.status, "OK");

        let run_dir = state.config.storage.dev_selftest_run_dir(&run_id);
        let stdout = std::fs::read_to_string(run_dir.join("logs/build.stdout.txt")).unwrap();
        assert!(
            stdout.contains("run --rm --network host --workdir /workspace/source"),
            "stdout: {stdout}"
        );
        assert!(
            stdout.contains(&format!(
                "--volume {}:/workspace/source:rw",
                run_dir.join("source").display()
            )),
            "stdout: {stdout}"
        );
        assert!(
            stdout.contains(&format!(
                "--volume {}:/workspace/artifacts:rw",
                run_dir.join("artifacts").display()
            )),
            "stdout: {stdout}"
        );
        assert!(
            stdout.contains("selftest-builder:latest /usr/local/bin/build-selftest"),
            "stdout: {stdout}"
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    #[cfg(all(test, unix))]
    async fn docker_executor_test_dispatch() {
        let (state, root) = test_state_with_docker_suite("dev-selftest-docker-exec");

        let sync = run_tool(&state, SYNC_WORKSPACE_ID, sync_params("docker-exec")).await;
        assert_eq!(sync.status, "OK");
        let run_id = sync.run_id;

        // run_tests dispatches the smoke suite through the executor docker runner. The fake
        // docker echoes its argv into the captured tests stdout.
        let tests = run_tool(
            &state,
            RUN_TESTS_ID,
            json!({
                "runId": run_id,
                "testSuite": "smoke",
                "testParams": {
                    "caseName": "opengemini_rw_smoke",
                    "instanceId": "inst-1"
                }
            }),
        )
        .await;
        assert_eq!(tests.status, "OK");
        assert_eq!(
            tests.value["testParamsSummary"]["caseName"],
            "opengemini_rw_smoke"
        );

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
        assert!(
            stdout.contains("--env DEVSELFTEST_PARAM_CASE_NAME=opengemini_rw_smoke"),
            "stdout: {stdout}"
        );
        assert!(
            stdout.contains("--env DEVSELFTEST_PARAM_INSTANCE_ID=inst-1"),
            "stdout: {stdout}"
        );

        let report = run_tool(&state, REPORT_ID, json!({"runId":run_id})).await;
        assert_eq!(report.status, "SUCCEEDED");

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    #[cfg(all(test, unix))]
    async fn cleanup_downs_deployed_compose_project_and_keeps_evidence() {
        let (state, root) = test_state_with_docker_suite("dev-selftest-cleanup");

        let sync = run_tool(&state, SYNC_WORKSPACE_ID, sync_params("cleanup")).await;
        assert_eq!(sync.status, "OK");
        let run_id = sync.run_id;
        let deploy = run_tool(&state, DEPLOY_ID, json!({"runId":run_id,"profile":"local"})).await;
        assert_eq!(deploy.status, "OK");
        let report = run_tool(&state, REPORT_ID, json!({"runId":run_id})).await;
        assert_eq!(report.status, "SUCCEEDED");

        let cleanup = run_tool(&state, CLEANUP_ID, json!({"runId":run_id})).await;
        assert_eq!(cleanup.status, "OK");
        assert_eq!(cleanup.value["profile"], "local");
        assert_eq!(
            cleanup.value["projectName"].as_str().unwrap(),
            format!("devselftest_{run_id}_local")
        );

        let run_dir = state.config.storage.dev_selftest_run_dir(&run_id);
        let stdout = std::fs::read_to_string(run_dir.join("logs/cleanup.stdout.txt")).unwrap();
        assert!(
            stdout.contains(&format!(
                "compose -p devselftest_{run_id}_local -f /opt/dev_selftest/docker-compose.yml down"
            )),
            "stdout: {stdout}"
        );
        assert!(!stdout.contains("--volumes"), "stdout: {stdout}");
        assert!(run_dir.join("source/SYNCED.txt").is_file());
        assert!(run_dir.join("report.md").is_file());

        let progress: Progress =
            serde_json::from_str(&std::fs::read_to_string(run_dir.join(PROGRESS_FILE)).unwrap())
                .unwrap();
        assert!(progress.steps.iter().any(|step| step.step == "cleanup"
            && step.evidence_refs
                == vec![
                    "logs/cleanup.stdout.txt".to_string(),
                    "logs/cleanup.stderr.txt".to_string()
                ]));

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    #[cfg(all(test, unix))]
    async fn cleanup_failure_does_not_change_report_verdict() {
        use std::os::unix::fs::PermissionsExt;
        let (state, root) = test_state_with_docker_suite("dev-selftest-cleanup-fail");

        let sync = run_tool(&state, SYNC_WORKSPACE_ID, sync_params("cleanup-fail")).await;
        assert_eq!(sync.status, "OK");
        let run_id = sync.run_id;
        let deploy = run_tool(&state, DEPLOY_ID, json!({"runId":run_id,"profile":"local"})).await;
        assert_eq!(deploy.status, "OK");

        let fake_docker = state.config.dev_selftest.docker.binary.clone();
        std::fs::write(
            &fake_docker,
            "#!/usr/bin/env bash\nprintf 'ARGS:'; for a in \"$@\"; do printf ' %s' \"$a\"; done; echo\nexit 7\n",
        )
        .unwrap();
        let mut perms = std::fs::metadata(&fake_docker).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&fake_docker, perms).unwrap();

        let cleanup = run_tool(&state, CLEANUP_ID, json!({"runId":run_id})).await;
        assert_eq!(cleanup.status, "FAILED");
        let report = run_tool(&state, REPORT_ID, json!({"runId":run_id})).await;
        assert_eq!(report.status, "SUCCEEDED");
        assert_eq!(report.value["failedSteps"], json!([]));
        assert!(report.value["steps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|step| step["step"] == "cleanup" && step["status"] == "FAILED"));

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    #[cfg(all(test, unix))]
    async fn diagnose_classifies_deploy_port_conflict_and_redacts_evidence() {
        use std::os::unix::fs::PermissionsExt;
        let (state, root) = test_state_with_docker_suite("dev-selftest-diagnose-port");

        let sync = run_tool(&state, SYNC_WORKSPACE_ID, sync_params("diagnose-port")).await;
        assert_eq!(sync.status, "OK");
        let run_id = sync.run_id;

        let fake_docker = state.config.dev_selftest.docker.binary.clone();
        std::fs::write(
            &fake_docker,
            r#"#!/usr/bin/env bash
if [ "${1:-}" = "compose" ]; then
  case "$*" in
    *" up -d"*)
      echo "Error response from daemon: Bind for 0.0.0.0:8086 failed: port is already allocated" >&2
      echo "password=super-secret" >&2
      exit 1
      ;;
    *" ps --all"*)
      exit 0
      ;;
    *" logs "*)
      echo "no containers"
      exit 0
      ;;
  esac
fi
if [ "${1:-}" = "ps" ]; then
  if [[ "$*" == *"publish=8086"* ]]; then
    printf 'old_container\tUp 2 hours\t0.0.0.0:8086->8086/tcp\n'
  fi
  exit 0
fi
exit 0
"#,
        )
        .unwrap();
        let mut perms = std::fs::metadata(&fake_docker).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&fake_docker, perms).unwrap();

        let deploy = run_tool(&state, DEPLOY_ID, json!({"runId":run_id,"profile":"local"})).await;
        assert_eq!(deploy.status, "FAILED");
        assert_eq!(deploy.value["stdoutPath"], "logs/deploy.stdout.txt");
        assert_eq!(deploy.value["stderrPath"], "logs/deploy.stderr.txt");

        let diagnose = run_tool(
            &state,
            DIAGNOSE_ID,
            json!({"runId":run_id,"maxEvidenceBytes":4096}),
        )
        .await;
        assert_eq!(diagnose.status, "OK");
        assert_eq!(diagnose.value["category"], "port_conflict");
        assert_eq!(diagnose.value["confidence"], "high");
        assert!(diagnose
            .value
            .to_string()
            .contains("port is already allocated"));
        assert!(diagnose.value.to_string().contains("old_container"));
        assert!(!diagnose.value.to_string().contains("super-secret"));
        assert!(diagnose.value["recommendedActions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|action| action["tool"] == CLEANUP_ID));

        let _ = std::fs::remove_dir_all(root);
    }
}
