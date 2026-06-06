use std::sync::Arc;

use tokio::sync::Semaphore;
use tracing::{error, warn};

use crate::{
    models::TaskPhase,
    pipeline::{extract_task, generate_task_result, prepare_pipeline_run, search_task},
    state::AppState,
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
            if let Err(err) = execute(state.clone(), &task_id).await {
                error!(task_id, "task execution failed: {err}");
            }
        });
    }
}

async fn execute(state: Arc<AppState>, task_id: &str) -> anyhow::Result<()> {
    let record = match state.tasks.start_attempt(task_id, TaskPhase::Extract).await {
        Ok(record) => record,
        Err(err) => {
            warn!(task_id, "skipping task that is no longer queued: {err}");
            return Ok(());
        }
    };
    let workspace = state.config.storage.workspace_dir(task_id);
    if let Err(err) = prepare_pipeline_run(&workspace).await {
        state
            .tasks
            .fail(task_id, Some(TaskPhase::Extract), err.to_string())
            .await?;
        return Ok(());
    }
    if let Err(err) = extract_task(state.config.clone(), record).await {
        state
            .tasks
            .fail(task_id, Some(TaskPhase::Extract), err.to_string())
            .await?;
        return Ok(());
    }

    state
        .tasks
        .set_phase(task_id, TaskPhase::SearchLogs)
        .await?;
    let output = match search_task(state.config.clone(), task_id).await {
        Ok(output) => output,
        Err(err) => {
            state
                .tasks
                .fail(task_id, Some(TaskPhase::SearchLogs), err.to_string())
                .await?;
            return Ok(());
        }
    };
    state
        .tasks
        .set_phase(task_id, TaskPhase::GenerateResult)
        .await?;
    let task = state
        .tasks
        .get(task_id)
        .await
        .ok_or_else(|| anyhow::anyhow!("task {task_id} disappeared"))?;
    match generate_task_result(state.config.clone(), state.llm.clone(), task).await {
        Ok(result) => {
            state
                .tasks
                .succeed(
                    task_id,
                    output.manifest_path.display().to_string(),
                    output.grep_results_path.display().to_string(),
                    result.result_json_path.display().to_string(),
                    result.result_markdown_path.display().to_string(),
                )
                .await?;
        }
        Err(err) => {
            state
                .tasks
                .fail(task_id, Some(TaskPhase::GenerateResult), err.to_string())
                .await?;
        }
    }
    Ok(())
}
