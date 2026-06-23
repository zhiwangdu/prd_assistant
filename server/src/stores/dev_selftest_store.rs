//! Persistent index of dev-self-test runs. Run *files* (source/, build artifacts,
//! logs, progress.json, report) live under `StorageSettings::dev_selftest_run_dir`;
//! this store holds the `DevSelftestRunRecord` metadata (one JSON file per run).
//! Mirrors `RemoteExecutorStore`.

use std::{collections::HashMap, fs, path::PathBuf, sync::Arc};

use chrono::Utc;
use tokio::sync::RwLock;

use crate::domain::models::DevSelftestRunRecord;

#[derive(Debug, Clone)]
pub struct DevSelftestStore {
    dir: PathBuf,
    inner: Arc<RwLock<HashMap<String, DevSelftestRunRecord>>>,
}

impl DevSelftestStore {
    pub fn load(dir: PathBuf) -> anyhow::Result<Self> {
        fs::create_dir_all(&dir)?;
        let mut runs = HashMap::new();
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let raw = fs::read_to_string(&path)?;
            let record: DevSelftestRunRecord = serde_json::from_str(&raw).map_err(|err| {
                anyhow::anyhow!("invalid dev_selftest record {}: {err}", path.display())
            })?;
            validate_run_id(&record.run_id).map_err(|err| {
                anyhow::anyhow!("invalid dev_selftest record {}: {err}", path.display())
            })?;
            if runs.insert(record.run_id.clone(), record).is_some() {
                anyhow::bail!("duplicate dev_selftest record in {}", path.display());
            }
        }
        Ok(Self {
            dir,
            inner: Arc::new(RwLock::new(runs)),
        })
    }

    pub async fn create(&self, record: DevSelftestRunRecord) -> anyhow::Result<()> {
        validate_run_id(&record.run_id)?;
        let mut runs = self.inner.write().await;
        if runs.contains_key(&record.run_id) {
            anyhow::bail!("dev_selftest run {} already exists", record.run_id);
        }
        self.persist(&record)?;
        runs.insert(record.run_id.clone(), record);
        Ok(())
    }

    pub async fn get(&self, run_id: &str) -> Option<DevSelftestRunRecord> {
        self.inner.read().await.get(run_id).cloned()
    }

    #[allow(dead_code)]
    pub async fn list(&self) -> Vec<DevSelftestRunRecord> {
        let mut runs: Vec<_> = self.inner.read().await.values().cloned().collect();
        runs.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        runs
    }

    pub async fn update(
        &self,
        run_id: &str,
        update: impl FnOnce(&mut DevSelftestRunRecord) -> anyhow::Result<()>,
    ) -> anyhow::Result<DevSelftestRunRecord> {
        let mut runs = self.inner.write().await;
        let mut candidate = runs
            .get(run_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown dev_selftest run {run_id}"))?;
        update(&mut candidate)?;
        candidate.updated_at = Utc::now();
        self.persist(&candidate)?;
        runs.insert(run_id.to_string(), candidate.clone());
        Ok(candidate)
    }

    fn persist(&self, record: &DevSelftestRunRecord) -> anyhow::Result<()> {
        let path = self.dir.join(format!("{}.json", record.run_id));
        let temp = self.dir.join(format!(".{}.json.tmp", record.run_id));
        fs::write(&temp, serde_json::to_vec_pretty(record)?)?;
        fs::rename(&temp, &path)?;
        Ok(())
    }
}

fn validate_run_id(run_id: &str) -> anyhow::Result<()> {
    let valid = run_id.starts_with("devselftest_")
        && run_id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'_' || value == b'-');
    if valid {
        Ok(())
    } else {
        anyhow::bail!("invalid dev_selftest run id {run_id}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::DevSelftestRunStatus;
    use chrono::Utc;

    fn record(id: &str) -> DevSelftestRunRecord {
        let now = Utc::now();
        DevSelftestRunRecord {
            schema_version: 1,
            run_id: id.to_string(),
            label: None,
            source_ref: None,
            build_artifacts: Vec::new(),
            deploy_target: None,
            test_run_id: None,
            steps: Vec::new(),
            status: DevSelftestRunStatus::Running,
            created_at: now,
            updated_at: now,
        }
    }

    fn temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "logagent-dev-selftest-store-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[tokio::test]
    async fn create_get_list_update() {
        let dir = temp_dir();
        let store = DevSelftestStore::load(dir.clone()).unwrap();
        store.create(record("devselftest_a")).await.unwrap();
        assert!(store.create(record("devselftest_a")).await.is_err());
        assert!(store.get("devselftest_a").await.is_some());
        assert_eq!(store.list().await.len(), 1);

        let updated = store
            .update("devselftest_a", |rec| {
                rec.label = Some("t".to_string());
                rec.status = DevSelftestRunStatus::Succeeded;
                Ok(())
            })
            .await
            .unwrap();
        assert_eq!(updated.label.as_deref(), Some("t"));
        assert_eq!(updated.status, DevSelftestRunStatus::Succeeded);

        // Reloads from disk.
        let reloaded = DevSelftestStore::load(dir.clone()).unwrap();
        assert_eq!(
            reloaded.get("devselftest_a").await.unwrap().status,
            DevSelftestRunStatus::Succeeded
        );

        // Rejects malformed ids on load.
        fs::write(dir.join("bad.json"), r#"{"runId":"task_x"}"#).unwrap();
        assert!(DevSelftestStore::load(dir.clone()).is_err());

        let _ = std::fs::remove_dir_all(dir);
    }
}
