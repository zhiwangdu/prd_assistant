use std::sync::Arc;

use tracing::{info, warn};

use crate::{
    pipeline::executor::TaskExecutor,
    services::tool_runner::ToolRunner,
    stores::{
        dev_selftest_store::DevSelftestStore, executor_store::RemoteExecutorStore,
        task_store::TaskStore, upload_store::UploadStore,
    },
    support::config::AppConfig,
};

#[derive(Debug)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub uploads: UploadStore,
    pub executors: RemoteExecutorStore,
    pub dev_selftest: DevSelftestStore,
    pub tasks: TaskStore,
    pub executor: TaskExecutor,
    pub tool_runner: ToolRunner,
}

impl AppState {
    pub fn new(config: Arc<AppConfig>) -> anyhow::Result<Arc<Self>> {
        info!(
            data_dir = %config.storage.data_dir.display(),
            max_concurrent_tasks = config.server.max_concurrent_tasks,
            "initializing app state"
        );
        let tasks = TaskStore::load(config.storage.tasks_dir())?;
        let uploads = UploadStore::load(config.storage.uploads_dir())?;
        let executors = RemoteExecutorStore::load(config.storage.executors_dir())?;
        let dev_selftest = DevSelftestStore::load(config.storage.dev_selftest_dir())?;
        let state = Arc::new(Self {
            executors,
            dev_selftest,
            executor: TaskExecutor::new(config.server.max_concurrent_tasks),
            tool_runner: ToolRunner::new(config.tools.clone()),
            config,
            uploads,
            tasks,
        });
        info!("app state initialized");
        Ok(state)
    }

    pub async fn recover_tasks(self: &Arc<Self>) -> anyhow::Result<()> {
        let known = self
            .tasks
            .list()
            .await
            .into_iter()
            .map(|task| task.task_id)
            .collect::<std::collections::HashSet<_>>();
        for entry in std::fs::read_dir(self.config.storage.workspaces_dir())? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let task_id = entry.file_name().to_string_lossy().into_owned();
                if task_id.starts_with("task_") && !known.contains(&task_id) {
                    warn!(task_id, path = %entry.path().display(), "orphan task workspace");
                }
            }
        }
        let recoverable = self.tasks.recover_incomplete().await?;
        info!(count = recoverable.len(), "recovering incomplete tasks");
        for task in recoverable {
            info!(
                task_id = %task.task_id,
                status = ?task.status,
                phase = ?task.phase,
                attempts = task.attempts,
                "enqueueing recovered task"
            );
            self.executor.enqueue(self.clone(), task.task_id);
        }
        Ok(())
    }

    /// Seed config-declared executor records (`remote_execution.executors`) at startup,
    /// creating each only if no record with that id already exists (never overwrites an
    /// API-created/modified one). Validation already ran at config load; a persist failure
    /// is logged and skipped rather than aborting startup.
    pub async fn seed_executors(self: &Arc<Self>) -> anyhow::Result<()> {
        for seeded in &self.config.remote_execution.executors {
            let executor_id = seeded.executor_id.clone();
            if let Err(err) = self.executors.create_if_absent(seeded.record.clone()).await {
                warn!(executor_id = %executor_id, error = %err, "failed to seed executor; skipping");
            }
        }
        Ok(())
    }
}
