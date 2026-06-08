use std::sync::Arc;
use tracing::warn;

use crate::{
    case_store::CaseStore, config::AppConfig, llm_gateway::LlmGateway, metadata::MetadataStore,
    task_executor::TaskExecutor, task_store::TaskStore, tool_runner::ToolRunner,
    upload_store::UploadStore,
};

#[derive(Debug)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub uploads: UploadStore,
    pub metadata: MetadataStore,
    pub cases: CaseStore,
    pub tasks: TaskStore,
    pub executor: TaskExecutor,
    pub llm: LlmGateway,
    pub tool_runner: ToolRunner,
}

impl AppState {
    pub fn new(config: Arc<AppConfig>) -> anyhow::Result<Arc<Self>> {
        let tasks = TaskStore::load(config.storage.tasks_dir())?;
        let uploads = UploadStore::load(config.storage.uploads_dir())?;
        let cases = CaseStore::load(config.storage.cases_dir())?;
        Ok(Arc::new(Self {
            metadata: MetadataStore::new(config.clone()),
            cases,
            executor: TaskExecutor::new(config.server.max_concurrent_tasks),
            llm: LlmGateway::new(config.llm.clone())?,
            tool_runner: ToolRunner::new(config.tools.clone()),
            config,
            uploads,
            tasks,
        }))
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
        for task in self.tasks.recover_incomplete().await? {
            self.executor.enqueue(self.clone(), task.task_id);
        }
        Ok(())
    }
}
