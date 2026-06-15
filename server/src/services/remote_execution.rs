use std::{path::PathBuf, sync::Arc, time::Instant};

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
    let started = Instant::now();
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
    let mut command = Command::new(&config.remote_execution.ssh_binary);
    command.kill_on_drop(true);
    command
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg(format!(
            "ConnectTimeout={}",
            config.remote_execution.connect_timeout_seconds
        ))
        .arg("-o")
        .arg(format!(
            "StrictHostKeyChecking={}",
            strict_host_key_checking_value(&config.remote_execution.host_key_policy)
        ))
        .arg("-p")
        .arg(executor.port.to_string())
        .arg(format!("{}@{}", executor.user, executor.host));
    for arg in &template.argv {
        command.arg(arg);
    }

    let output = tokio::time::timeout(Duration::from_secs(timeout_seconds), command.output()).await;
    let completed_at = Utc::now();
    let mut warnings = Vec::new();
    let (status, exit_code, stdout, stderr, error) = match output {
        Ok(Ok(output)) => {
            let status = if output.status.success() {
                RemoteCommandStatus::Ok
            } else {
                RemoteCommandStatus::Failed
            };
            (
                status,
                output.status.code(),
                output.stdout,
                output.stderr,
                None,
            )
        }
        Ok(Err(err)) => (
            RemoteCommandStatus::Failed,
            None,
            Vec::new(),
            Vec::new(),
            Some(format!("failed to start ssh: {err}")),
        ),
        Err(_) => (
            RemoteCommandStatus::TimedOut,
            None,
            Vec::new(),
            Vec::new(),
            Some(format!("remote command timed out after {timeout_seconds}s")),
        ),
    };
    let (stdout, stdout_truncated) = cap_output(stdout, config.remote_execution.max_output_bytes);
    let (stderr, stderr_truncated) = cap_output(stderr, config.remote_execution.max_output_bytes);
    if stdout_truncated {
        warnings.push(format!(
            "stdout truncated to {} bytes",
            config.remote_execution.max_output_bytes
        ));
    }
    if stderr_truncated {
        warnings.push(format!(
            "stderr truncated to {} bytes",
            config.remote_execution.max_output_bytes
        ));
    }
    tokio::fs::write(&stdout_path, &stdout)
        .await
        .map_err(|err| AppError::internal(format!("failed to write remote stdout: {err}")))?;
    tokio::fs::write(&stderr_path, &stderr)
        .await
        .map_err(|err| AppError::internal(format!("failed to write remote stderr: {err}")))?;
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
        exit_code,
        duration_ms: started.elapsed().as_millis(),
        stdout_path: stdout_path.display().to_string(),
        stderr_path: stderr_path.display().to_string(),
        stdout_preview: preview(&stdout),
        stderr_preview: preview(&stderr),
        warnings,
        error,
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
