use std::{collections::HashMap, sync::Arc};

use tokio::sync::RwLock;

use crate::{config::AppConfig, models::UploadRecord};

#[derive(Debug)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub uploads: UploadStore,
}

impl AppState {
    pub fn new(config: Arc<AppConfig>) -> Arc<Self> {
        Arc::new(Self {
            config,
            uploads: UploadStore::default(),
        })
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
