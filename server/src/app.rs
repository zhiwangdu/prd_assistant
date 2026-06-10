use std::sync::Arc;
use tracing::warn;

use crate::{
    pipeline::executor::TaskExecutor,
    services::{llm_gateway::LlmGateway, metadata::MetadataStore, tool_runner::ToolRunner},
    stores::{
        case_import_store::CaseImportStore, case_store::CaseStore,
        session_store::AnalysisSessionStore, system_context_store::SystemContextStore,
        task_store::TaskStore, upload_store::UploadStore,
    },
    support::config::AppConfig,
};

#[derive(Debug)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub uploads: UploadStore,
    pub metadata: MetadataStore,
    pub cases: CaseStore,
    pub case_imports: CaseImportStore,
    pub system_context: SystemContextStore,
    pub sessions: AnalysisSessionStore,
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
        let case_imports = CaseImportStore::load(config.storage.case_imports_dir())?;
        let system_context = SystemContextStore::load(config.storage.system_context_dir())?;
        let sessions = AnalysisSessionStore::load(
            config.storage.sessions_dir(),
            config.storage.session_workspaces_dir(),
        )?;
        Ok(Arc::new(Self {
            metadata: MetadataStore::new(config.clone()),
            cases,
            case_imports,
            system_context,
            sessions,
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
            self.sessions.sync_task_status(&task).await?;
            self.executor.enqueue(self.clone(), task.task_id);
        }
        Ok(())
    }
}
