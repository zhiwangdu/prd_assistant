use std::sync::Arc;

use tokio::sync::Semaphore;
use tracing::{error, info, warn};

use crate::{
    app::AppState,
    domain::models::{TaskKind, TaskPhase, TaskRecord},
};

#[derive(Debug)]
pub struct TaskExecutor {
    permits: Arc<Semaphore>,
}

impl TaskExecutor {
    pub fn new(max_concurrent_tasks: usize) -> Self {
        Self {
            permits: Arc::new(Semaphore::new(max_concurrent_tasks.max(1))),
        }
    }

    pub fn enqueue(&self, state: Arc<AppState>, task_id: String) {
        info!(task_id = %task_id, "task enqueued");
        let permits = self.permits.clone();
        tokio::spawn(async move {
            let permit = match permits.acquire_owned().await {
                Ok(permit) => permit,
                Err(err) => {
                    error!(task_id, "task executor closed: {err}");
                    return;
                }
            };
            let _permit = permit;
            info!(task_id = %task_id, "task execution started");
            if let Err(err) = execute(state.clone(), &task_id).await {
                error!(task_id, "task execution failed: {err}");
            }
        });
    }
}

async fn execute(state: Arc<AppState>, task_id: &str) -> anyhow::Result<()> {
    let initial_phase = state
        .tasks
        .get(task_id)
        .await
        .map(|task| match task.task_kind {
            TaskKind::ToolRun => TaskPhase::RunTool,
            TaskKind::RemoteCommandRun => TaskPhase::ExecuteRemoteCommand,
        })
        .unwrap_or(TaskPhase::RunTool);
    let task = match state.tasks.start_attempt(task_id, initial_phase).await {
        Ok(record) => record,
        Err(err) => {
            warn!(task_id, "skipping task that is no longer queued: {err}");
            return Ok(());
        }
    };
    info!(
        task_id = %task.task_id,
        task_kind = ?task.task_kind,
        phase = ?task.phase,
        attempt = task.attempts,
        "task attempt started"
    );

    let phase = task
        .phase
        .ok_or_else(|| anyhow::anyhow!("running task {task_id} has no phase"))?;
    if let Err(err) = dispatch_phase(state.clone(), task, phase).await {
        error!(
            task_id = %task_id,
            phase = ?phase,
            error = %err,
            "task phase failed"
        );
        let _ = state
            .tasks
            .fail(task_id, Some(phase), err.to_string())
            .await?;
        info!(task_id = %task_id, phase = ?phase, "task marked failed");
    }
    Ok(())
}

async fn dispatch_phase(
    state: Arc<AppState>,
    task: TaskRecord,
    phase: TaskPhase,
) -> anyhow::Result<()> {
    info!(
        task_id = %task.task_id,
        phase = ?phase,
        task_kind = ?task.task_kind,
        "task phase started"
    );
    match phase {
        TaskPhase::RunTool => {
            if task.task_kind != TaskKind::ToolRun {
                anyhow::bail!("RUN_TOOL phase requires a tool run task");
            }
            let result_path = crate::services::tools::run_tool_task(state.clone(), task.clone())
                .await?
                .display()
                .to_string();
            let completed = state
                .tasks
                .succeed_tool_run(&task.task_id, TaskPhase::RunTool, result_path)
                .await?;
            info!(
                task_id = %completed.task_id,
                tool_id = ?completed.tool_id,
                "tool run task succeeded"
            );
            Ok(())
        }
        TaskPhase::ExecuteRemoteCommand => {
            if task.task_kind != TaskKind::RemoteCommandRun {
                anyhow::bail!("EXECUTE_REMOTE_COMMAND phase requires a remote command task");
            }
            let executor_id = task
                .remote_executor_id
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("remote command task is missing executor id"))?;
            let executor = state
                .executors
                .get(executor_id)
                .await
                .ok_or_else(|| anyhow::anyhow!("unknown executor {executor_id}"))?;
            let result_path = crate::services::remote_execution::run_remote_command_task(
                state.config.clone(),
                executor,
                task.clone(),
            )
            .await?
            .display()
            .to_string();
            let completed = state
                .tasks
                .succeed_remote_command_run(
                    &task.task_id,
                    TaskPhase::ExecuteRemoteCommand,
                    result_path,
                )
                .await?;
            info!(
                task_id = %completed.task_id,
                executor_id = ?completed.remote_executor_id,
                command_id = ?completed.remote_command_id,
                "remote command task succeeded"
            );
            Ok(())
        }
    }
}
