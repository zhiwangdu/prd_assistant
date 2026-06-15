use std::sync::Arc;
use tracing::{info, warn};

use crate::{
    pipeline::executor::TaskExecutor,
    services::{
        agent_backend::AgentBackendRegistry, domain_adapters::DomainAdapterRegistry,
        llm_gateway::LlmGateway, metadata::MetadataStore, skill_registry::SkillRegistry,
        tool_runner::ToolRunner,
    },
    stores::{
        case_import_store::CaseImportStore, case_store::CaseStore,
        executor_store::RemoteExecutorStore, session_store::AnalysisSessionStore,
        system_context_store::SystemContextStore, task_store::TaskStore, upload_store::UploadStore,
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
    pub executors: RemoteExecutorStore,
    pub system_context: SystemContextStore,
    pub skills: SkillRegistry,
    pub sessions: AnalysisSessionStore,
    pub tasks: TaskStore,
    pub executor: TaskExecutor,
    pub llm: LlmGateway,
    pub agent_backends: AgentBackendRegistry,
    pub domain_adapters: DomainAdapterRegistry,
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
        let cases = CaseStore::load_with_memory(
            config.storage.cases_dir(),
            config.storage.memory_db_path(),
        )?;
        let case_imports = CaseImportStore::load(config.storage.case_imports_dir())?;
        let executors = RemoteExecutorStore::load(config.storage.executors_dir())?;
        let system_context = SystemContextStore::load(config.storage.system_context_dir())?;
        let skills = SkillRegistry::load(config.skills.clone())?;
        let sessions = AnalysisSessionStore::load(
            config.storage.sessions_dir(),
            config.storage.session_workspaces_dir(),
        )?;
        let state = Arc::new(Self {
            metadata: MetadataStore::new(config.clone()),
            cases,
            case_imports,
            executors,
            system_context,
            skills,
            sessions,
            executor: TaskExecutor::new(config.server.max_concurrent_tasks),
            llm: LlmGateway::new(config.llm.clone())?,
            agent_backends: AgentBackendRegistry::new(config.claude_code.clone()),
            domain_adapters: DomainAdapterRegistry::builtin(),
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
            self.sessions.sync_task_status(&task).await?;
            self.executor.enqueue(self.clone(), task.task_id);
        }
        Ok(())
    }
}
