use std::{path::PathBuf, sync::Arc};

use tracing::{info, warn};

use crate::{
    pipeline::executor::TaskExecutor,
    services::{dev_selftest_allowlist::DevSelftestGitAllowlist, tool_runner::ToolRunner},
    stores::{
        dev_selftest_store::DevSelftestStore, task_store::TaskStore, upload_store::UploadStore,
    },
    support::config::AppConfig,
};

#[derive(Debug)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub config_path: Option<PathBuf>,
    pub dev_selftest_git_allowlist: DevSelftestGitAllowlist,
    pub uploads: UploadStore,
    pub dev_selftest: DevSelftestStore,
    pub tasks: TaskStore,
    pub executor: TaskExecutor,
    pub tool_runner: ToolRunner,
}

impl AppState {
    #[allow(dead_code)]
    pub fn new(config: Arc<AppConfig>) -> anyhow::Result<Arc<Self>> {
        Self::new_with_config_path(config, None)
    }

    pub fn new_with_config_path(
        config: Arc<AppConfig>,
        config_path: Option<PathBuf>,
    ) -> anyhow::Result<Arc<Self>> {
        info!(
            data_dir = %config.storage.data_dir.display(),
            max_concurrent_tasks = config.server.max_concurrent_tasks,
            "initializing app state"
        );
        let tasks = TaskStore::load(config.storage.tasks_dir())?;
        let uploads = UploadStore::load(config.storage.uploads_dir())?;
        let dev_selftest = DevSelftestStore::load(config.storage.dev_selftest_dir())?;
        let state = Arc::new(Self {
            config_path,
            dev_selftest_git_allowlist: DevSelftestGitAllowlist::new(
                config.dev_selftest.git.repos.clone(),
            ),
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
}
