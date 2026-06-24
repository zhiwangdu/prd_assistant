use std::{collections::BTreeMap, path::PathBuf, time::Instant};

use serde::Serialize;
use tokio::{process::Command, time::Duration};

use crate::support::config::{AppConfig, RemoteCommandTemplateSettings};

pub fn command_template(
    config: &AppConfig,
    command_id: &str,
) -> Option<RemoteCommandTemplateSettings> {
    config.remote_execution.commands.get(command_id).cloned()
}

/// Outcome of running one command on an executor target. `status` preserves the
/// Ok/Failed/TimedOut/SpawnFailed distinction so callers can map it without losing
/// timeout semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExecutorRunStatus {
    Ok,
    Failed,
    TimedOut,
    SpawnFailed,
}

/// Where a command runs. `Docker` launches an ephemeral `docker run --rm` container
/// (used by dev_selftest's inline docker test target). argv is supplied by the caller —
/// no free shell is opened.
#[derive(Debug, Clone)]
pub enum ExecutorTarget {
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
    /// In-container command (binary + args).
    pub argv: &'a [String],
    pub timeout_seconds: u64,
    /// Appended as `--env` **after** `target.env`, so system vars win.
    pub extra_env: BTreeMap<String, String>,
    /// Spawn cwd on the server host.
    pub server_cwd: PathBuf,
    /// Program to exec: the docker binary.
    pub launcher: PathBuf,
    pub max_output_bytes: usize,
}

#[derive(Debug, Clone)]
pub struct ExecutorOutcome {
    pub status: ExecutorRunStatus,
    pub exit_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub duration_ms: u128,
    pub error: Option<String>,
}

/// Run `argv` on `target` with a hard timeout and output cap. Pure utility — does NOT
/// check any enable flag (the gate lives at the caller), so dev_selftest can reuse the
/// docker branch directly.
pub async fn run_executor_command(input: ExecutorRunInput<'_>) -> ExecutorOutcome {
    let started = Instant::now();
    let timeout_seconds = input.timeout_seconds.max(1);
    let mut command = Command::new(&input.launcher);
    command.kill_on_drop(true);
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());
    match input.target {
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
            let stdout = cap_output(output.stdout, input.max_output_bytes);
            let stderr = cap_output(output.stderr, input.max_output_bytes);
            ExecutorOutcome {
                status,
                exit_code: output.status.code(),
                stdout,
                stderr,
                duration_ms,
                error: None,
            }
        }
        Ok(Err(err)) => ExecutorOutcome {
            status: ExecutorRunStatus::SpawnFailed,
            exit_code: None,
            stdout: Vec::new(),
            stderr: err.to_string().into_bytes(),
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
            duration_ms,
            error: Some(format!("command timed out after {timeout_seconds}s")),
        },
    }
}

fn cap_output(mut output: Vec<u8>, max_bytes: usize) -> Vec<u8> {
    if output.len() > max_bytes {
        output.truncate(max_bytes);
    }
    output
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
