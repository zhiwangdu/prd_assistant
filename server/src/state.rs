use std::{collections::HashMap, sync::Arc};

use tokio::sync::RwLock;
use tracing::warn;

use crate::{
    config::AppConfig, metadata::MetadataStore, models::UploadRecord, task_executor::TaskExecutor,
    task_store::TaskStore,
};

#[derive(Debug)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub uploads: UploadStore,
    pub metadata: MetadataStore,
    pub tasks: TaskStore,
    pub executor: TaskExecutor,
}

impl AppState {
    pub fn new(config: Arc<AppConfig>) -> anyhow::Result<Arc<Self>> {
        let tasks = TaskStore::load(config.storage.tasks_dir())?;
        Ok(Arc::new(Self {
            metadata: MetadataStore::new(config.clone()),
            executor: TaskExecutor::new(config.server.max_concurrent_tasks),
            config,
            uploads: UploadStore::default(),
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

#[derive(Debug, Default)]
pub struct UploadStore {
    inner: Arc<RwLock<HashMap<String, UploadRecord>>>,
}

impl Clone for UploadStore {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl UploadStore {
    pub async fn insert(&self, record: UploadRecord) {
        self.inner
            .write()
            .await
            .insert(record.upload_id.clone(), record);
    }

    pub async fn get(&self, upload_id: &str) -> Option<UploadRecord> {
        self.inner.read().await.get(upload_id).cloned()
    }

    pub async fn update_size(&self, upload_id: &str, size: u64) -> Option<UploadRecord> {
        let mut uploads = self.inner.write().await;
        let record = uploads.get_mut(upload_id)?;
        record.size = size;
        Some(record.clone())
    }
}
