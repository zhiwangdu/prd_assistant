use std::{collections::BTreeMap, path::PathBuf, sync::Arc, time::Instant};

use chrono::Utc;
use serde::Serialize;
use tokio::{process::Command, time::Duration};
use tracing::info;

use crate::{
    domain::models::{RemoteExecutorRecord, TaskRecord},
    support::{
        config::{AppConfig, RemoteCommandTemplateSettings},
        error::AppError,
    },
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteCommandRunRecord {
    schema_version: u32,
    executor_id: String,
    executor_name: String,
    host: String,
    port: u16,
    user: String,
    command_id: String,
    command_display_name: String,
    command_argv: Vec<String>,
    status: RemoteCommandStatus,
    exit_code: Option<i32>,
    duration_ms: u128,
    stdout_path: String,
    stderr_path: String,
    stdout_preview: String,
    stderr_preview: String,
    warnings: Vec<String>,
    error: Option<String>,
    started_at: chrono::DateTime<Utc>,
    completed_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum RemoteCommandStatus {
    Ok,
    Failed,
    TimedOut,
}

pub fn command_templates(
    config: &AppConfig,
) -> Vec<crate::domain::models::RemoteCommandTemplateDescriptor> {
    config
        .remote_execution
        .commands
        .values()
        .map(
            |command| crate::domain::models::RemoteCommandTemplateDescriptor {
                command_id: command.command_id.clone(),
                display_name: command.display_name.clone(),
                description: command.description.clone(),
                enabled: config.remote_execution.enabled && command.enabled,
                argv: command.argv.clone(),
                timeout_seconds: command
                    .timeout_seconds
                    .unwrap_or(config.remote_execution.command_timeout_seconds),
            },
        )
        .collect()
}

pub fn command_template(
    config: &AppConfig,
    command_id: &str,
) -> Option<RemoteCommandTemplateSettings> {
    config.remote_execution.commands.get(command_id).cloned()
}

/// Outcome of running one command on an executor target. `status` preserves the
/// Ok/Failed/TimedOut/SpawnFailed distinction so callers (e.g. the SSH task path) can map
/// it without losing timeout semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExecutorRunStatus {
    Ok,
    Failed,
    TimedOut,
    SpawnFailed,
}

/// Where a command runs. `Ssh` is the existing remote-execution target; `Docker` launches
/// an ephemeral `docker run --rm` container (used by dev_selftest's inline docker test
/// target). Neither variant opens a free shell — argv is supplied by the caller.
#[derive(Debug, Clone)]
pub enum ExecutorTarget {
    Ssh {
        host: String,
        port: u16,
        user: String,
        connect_timeout_seconds: u64,
        host_key_policy: String,
    },
    Docker {
        image: String,
        /// `None` (default) ⇒ `host`.
        network: Option<String>,
        workdir: Option<String>,
        /// `host:container[:ro|rw]`, already interpolated.
        volumes: Vec<String>,
        /// User-provided env; system env in `ExecutorRunInput::extra_env` overrides these.
        env: BTreeMap<String, String>,
    },
}

#[derive(Debug, Clone)]
pub struct ExecutorRunInput<'a> {
    pub target: &'a ExecutorTarget,
    /// Ssh: remote argv; Docker: in-container command (binary + args).
    pub argv: &'a [String],
    pub timeout_seconds: u64,
    /// Docker only: appended as `--env` **after** `target.env`, so system vars win.
    /// Ignored for Ssh (ssh does not inherit env).
    pub extra_env: BTreeMap<String, String>,
    /// Spawn cwd on the server host (docker run only; Ssh ignores it).
    pub server_cwd: PathBuf,
    /// Program to exec: ssh binary (Ssh) or docker binary (Docker).
    pub launcher: PathBuf,
    pub max_output_bytes: usize,
}

#[derive(Debug, Clone)]
pub struct ExecutorOutcome {
    pub status: ExecutorRunStatus,
    pub exit_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub duration_ms: u128,
    pub error: Option<String>,
}

/// Run `argv` on `target` with a hard timeout and output cap. Pure utility — does NOT
/// check `remote_execution.enabled` (the gate lives at the task/handler entry), so
/// dev_selftest can reuse the docker branch even when remote SSH execution is disabled.
pub async fn run_executor_command(input: ExecutorRunInput<'_>) -> ExecutorOutcome {
    let started = Instant::now();
    let timeout_seconds = input.timeout_seconds.max(1);
    let mut command = Command::new(&input.launcher);
    command.kill_on_drop(true);
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());
    match input.target {
        ExecutorTarget::Ssh {
            host,
            port,
            user,
            connect_timeout_seconds,
            host_key_policy,
        } => {
            // Bit-for-bit the same ssh invocation the old run_remote_command_task built.
            command
                .arg("-o")
                .arg("BatchMode=yes")
                .arg("-o")
                .arg(format!("ConnectTimeout={connect_timeout_seconds}"))
                .arg("-o")
                .arg(format!(
                    "StrictHostKeyChecking={}",
                    strict_host_key_checking_value(host_key_policy)
                ))
                .arg("-p")
                .arg(port.to_string())
                .arg(format!("{user}@{host}"));
            for arg in input.argv {
                command.arg(arg);
            }
        }
        ExecutorTarget::Docker {
            image,
            network,
            workdir,
            volumes,
            env,
        } => {
            command.current_dir(&input.server_cwd);
            command.arg("run").arg("--rm");
            command
                .arg("--network")
                .arg(network.as_deref().unwrap_or("host"));
            if let Some(workdir) = workdir {
                command.arg("--workdir").arg(workdir);
            }
            // System env (extra_env) overrides user env (target.env): insert user first,
            // then overwrite with system.
            let mut merged = env.clone();
            for (key, value) in &input.extra_env {
                merged.insert(key.clone(), value.clone());
            }
            for (key, value) in &merged {
                command.arg("--env").arg(format!("{key}={value}"));
            }
            for volume in volumes {
                command.arg("--volume").arg(volume);
            }
            command.arg(image);
            for arg in input.argv {
                command.arg(arg);
            }
        }
    }
    let output = tokio::time::timeout(Duration::from_secs(timeout_seconds), command.output()).await;
    let duration_ms = started.elapsed().as_millis();
    match output {
        Ok(Ok(output)) => {
            let status = if output.status.success() {
                ExecutorRunStatus::Ok
            } else {
                ExecutorRunStatus::Failed
            };
            let (stdout, stdout_truncated) = cap_output(output.stdout, input.max_output_bytes);
            let (stderr, stderr_truncated) = cap_output(output.stderr, input.max_output_bytes);
            ExecutorOutcome {
                status,
                exit_code: output.status.code(),
                stdout,
                stderr,
                stdout_truncated,
                stderr_truncated,
                duration_ms,
                error: None,
            }
        }
        Ok(Err(err)) => ExecutorOutcome {
            status: ExecutorRunStatus::SpawnFailed,
            exit_code: None,
            stdout: Vec::new(),
            stderr: err.to_string().into_bytes(),
            stdout_truncated: false,
            stderr_truncated: false,
            duration_ms,
            error: Some(format!(
                "failed to spawn {}: {err}",
                input.launcher.display()
            )),
        },
        Err(_) => ExecutorOutcome {
            status: ExecutorRunStatus::TimedOut,
            exit_code: None,
            stdout: Vec::new(),
            stderr: Vec::new(),
            stdout_truncated: false,
            stderr_truncated: false,
            duration_ms,
            error: Some(format!("command timed out after {timeout_seconds}s")),
        },
    }
}

pub async fn run_remote_command_task(
    config: Arc<AppConfig>,
    executor: RemoteExecutorRecord,
    task: TaskRecord,
) -> Result<PathBuf, AppError> {
    if !config.remote_execution.enabled {
        return Err(AppError::bad_request("remote execution is disabled"));
    }
    if !executor.enabled {
        return Err(AppError::bad_request(format!(
            "executor {} is disabled",
            executor.executor_id
        )));
    }
    let command_id = task
        .remote_command_id
        .as_deref()
        .ok_or_else(|| AppError::bad_request("remote command run is missing commandId"))?;
    let template = command_template(&config, command_id)
        .ok_or_else(|| AppError::bad_request(format!("unknown commandId {command_id}")))?;
    if !template.enabled {
        return Err(AppError::bad_request(format!(
            "remote command {command_id} is disabled"
        )));
    }
    if template.argv.is_empty() {
        return Err(AppError::bad_request(format!(
            "remote command {command_id} has empty argv"
        )));
    }

    let workspace = config.storage.workspace_dir(&task.task_id);
    let result_dir = workspace.join("remote_command");
    tokio::fs::create_dir_all(&result_dir)
        .await
        .map_err(|err| AppError::internal(format!("failed to create remote result dir: {err}")))?;
    let stdout_path = result_dir.join("stdout.txt");
    let stderr_path = result_dir.join("stderr.txt");
    let result_path = result_dir.join("result.json");

    let started_at = Utc::now();
    info!(
        task_id = %task.task_id,
        executor_id = %executor.executor_id,
        host = %executor.host,
        command_id = %template.command_id,
        "starting remote command run"
    );
    let timeout_seconds = template
        .timeout_seconds
        .unwrap_or(config.remote_execution.command_timeout_seconds)
        .max(1);
    let target = ExecutorTarget::Ssh {
        host: executor.host.clone(),
        port: executor.port,
        user: executor.user.clone(),
        connect_timeout_seconds: config.remote_execution.connect_timeout_seconds,
        host_key_policy: config.remote_execution.host_key_policy.clone(),
    };
    let input = ExecutorRunInput {
        target: &target,
        argv: &template.argv,
        timeout_seconds,
        extra_env: BTreeMap::new(),
        server_cwd: workspace.clone(),
        launcher: config.remote_execution.ssh_binary.clone(),
        max_output_bytes: config.remote_execution.max_output_bytes,
    };
    let outcome = run_executor_command(input).await;
    let completed_at = Utc::now();
    let mut warnings = Vec::new();
    if outcome.stdout_truncated {
        warnings.push(format!(
            "stdout truncated to {} bytes",
            config.remote_execution.max_output_bytes
        ));
    }
    if outcome.stderr_truncated {
        warnings.push(format!(
            "stderr truncated to {} bytes",
            config.remote_execution.max_output_bytes
        ));
    }
    tokio::fs::write(&stdout_path, &outcome.stdout)
        .await
        .map_err(|err| AppError::internal(format!("failed to write remote stdout: {err}")))?;
    tokio::fs::write(&stderr_path, &outcome.stderr)
        .await
        .map_err(|err| AppError::internal(format!("failed to write remote stderr: {err}")))?;
    let status = match outcome.status {
        ExecutorRunStatus::Ok => RemoteCommandStatus::Ok,
        ExecutorRunStatus::TimedOut => RemoteCommandStatus::TimedOut,
        ExecutorRunStatus::Failed | ExecutorRunStatus::SpawnFailed => RemoteCommandStatus::Failed,
    };
    let record = RemoteCommandRunRecord {
        schema_version: 1,
        executor_id: executor.executor_id,
        executor_name: executor.name,
        host: executor.host,
        port: executor.port,
        user: executor.user,
        command_id: template.command_id,
        command_display_name: template.display_name,
        command_argv: template.argv,
        status,
        exit_code: outcome.exit_code,
        duration_ms: outcome.duration_ms,
        stdout_path: stdout_path.display().to_string(),
        stderr_path: stderr_path.display().to_string(),
        stdout_preview: preview(&outcome.stdout),
        stderr_preview: preview(&outcome.stderr),
        warnings,
        error: outcome.error,
        started_at,
        completed_at,
    };
    tokio::fs::write(
        &result_path,
        serde_json::to_vec_pretty(&record)
            .map_err(|err| AppError::internal(format!("failed to encode remote result: {err}")))?,
    )
    .await
    .map_err(|err| AppError::internal(format!("failed to write remote result: {err}")))?;
    Ok(result_path)
}

fn strict_host_key_checking_value(policy: &str) -> &'static str {
    match policy {
        "strict" => "yes",
        "no" => "no",
        _ => "accept-new",
    }
}

fn cap_output(mut output: Vec<u8>, max_bytes: usize) -> (Vec<u8>, bool) {
    if output.len() <= max_bytes {
        return (output, false);
    }
    output.truncate(max_bytes);
    (output, true)
}

fn preview(output: &[u8]) -> String {
    let text = String::from_utf8_lossy(output);
    text.chars().take(4000).collect()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{
        os::unix::fs::PermissionsExt,
        path::Path,
        sync::atomic::{AtomicU64, Ordering},
    };

    fn write_fake_docker(root: &Path) -> PathBuf {
        let path = root.join("fake-docker.sh");
        std::fs::write(
            &path,
            "#!/usr/bin/env bash\n\
             printf 'ARGS:'; for a in \"$@\"; do printf ' %s' \"$a\"; done; printf '\\n'\n\
             [ -n \"$LOGAGENT_FAKE_DOCKER_SLEEP\" ] && sleep \"$LOGAGENT_FAKE_DOCKER_SLEEP\"\n\
             exit \"${LOGAGENT_FAKE_DOCKER_EXIT:-0}\"\n",
        )
        .unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
        path
    }

    fn set_env(name: &str, value: Option<&str>) {
        // SAFETY: these test-only env vars are not read by any runtime code and are only
        // touched within this single test function (no parallel access).
        unsafe {
            match value {
                Some(v) => std::env::set_var(name, v),
                None => std::env::remove_var(name),
            }
        }
    }

    #[tokio::test]
    async fn run_executor_command_docker_target() {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let root = std::env::temp_dir().join(format!(
            "logagent-exec-runner-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        let fake_docker = write_fake_docker(&root);

        let target = ExecutorTarget::Docker {
            image: "alpine:3.20".to_string(),
            network: None,
            workdir: Some("/tests".to_string()),
            volumes: vec!["/repo/tests:/tests:ro".to_string()],
            env: BTreeMap::from([("FOO".to_string(), "bar".to_string())]),
        };

        // Ok: full docker run argv (network default host, env sorted, volume, image, argv).
        let mut extra_env = BTreeMap::new();
        extra_env.insert("DEVSELFTEST_HOST".to_string(), "127.0.0.1".to_string());
        extra_env.insert("DEVSELFTEST_PORT".to_string(), "8086".to_string());
        let input = ExecutorRunInput {
            target: &target,
            argv: &["sh".to_string(), "/tests/smoke.sh".to_string()],
            timeout_seconds: 5,
            extra_env,
            server_cwd: root.clone(),
            launcher: fake_docker.clone(),
            max_output_bytes: 4096,
        };
        let outcome = run_executor_command(input).await;
        assert_eq!(outcome.status, ExecutorRunStatus::Ok);
        assert_eq!(outcome.exit_code, Some(0));
        let stdout = String::from_utf8_lossy(&outcome.stdout);
        assert!(
            stdout.contains(
                "run --rm --network host --workdir /tests \
                 --env DEVSELFTEST_HOST=127.0.0.1 --env DEVSELFTEST_PORT=8086 --env FOO=bar \
                 --volume /repo/tests:/tests:ro alpine:3.20 sh /tests/smoke.sh"
            ),
            "stdout was: {stdout}"
        );

        // Failed: non-zero exit.
        set_env("LOGAGENT_FAKE_DOCKER_EXIT", Some("1"));
        let input = ExecutorRunInput {
            target: &target,
            argv: &["sh".to_string()],
            timeout_seconds: 5,
            extra_env: BTreeMap::new(),
            server_cwd: root.clone(),
            launcher: fake_docker.clone(),
            max_output_bytes: 4096,
        };
        let outcome = run_executor_command(input).await;
        set_env("LOGAGENT_FAKE_DOCKER_EXIT", None);
        assert_eq!(outcome.status, ExecutorRunStatus::Failed);
        assert_eq!(outcome.exit_code, Some(1));

        // TimedOut: child outlasts the timeout.
        set_env("LOGAGENT_FAKE_DOCKER_SLEEP", Some("2"));
        let input = ExecutorRunInput {
            target: &target,
            argv: &["sh".to_string()],
            timeout_seconds: 1,
            extra_env: BTreeMap::new(),
            server_cwd: root.clone(),
            launcher: fake_docker.clone(),
            max_output_bytes: 4096,
        };
        let outcome = run_executor_command(input).await;
        set_env("LOGAGENT_FAKE_DOCKER_SLEEP", None);
        assert_eq!(outcome.status, ExecutorRunStatus::TimedOut);

        // SpawnFailed: launcher does not exist.
        let input = ExecutorRunInput {
            target: &target,
            argv: &["sh".to_string()],
            timeout_seconds: 5,
            extra_env: BTreeMap::new(),
            server_cwd: root.clone(),
            launcher: root.join("does-not-exist"),
            max_output_bytes: 4096,
        };
        let outcome = run_executor_command(input).await;
        assert_eq!(outcome.status, ExecutorRunStatus::SpawnFailed);

        let _ = std::fs::remove_dir_all(root);
    }
}
