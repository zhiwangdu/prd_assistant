use std::{collections::HashMap, fs, path::PathBuf, sync::Arc};

use chrono::Utc;
use tokio::sync::RwLock;

use crate::domain::models::RemoteExecutorRecord;

#[derive(Debug, Clone)]
pub struct RemoteExecutorStore {
    dir: PathBuf,
    inner: Arc<RwLock<HashMap<String, RemoteExecutorRecord>>>,
}

impl RemoteExecutorStore {
    pub fn load(dir: PathBuf) -> anyhow::Result<Self> {
        fs::create_dir_all(&dir)?;
        let mut executors = HashMap::new();
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let raw = fs::read_to_string(&path)?;
            let executor: RemoteExecutorRecord = serde_json::from_str(&raw).map_err(|err| {
                anyhow::anyhow!("invalid executor record {}: {err}", path.display())
            })?;
            validate_executor(&executor).map_err(|err| {
                anyhow::anyhow!("invalid executor record {}: {err}", path.display())
            })?;
            if executors
                .insert(executor.executor_id.clone(), executor)
                .is_some()
            {
                anyhow::bail!("duplicate executor record in {}", path.display());
            }
        }
        Ok(Self {
            dir,
            inner: Arc::new(RwLock::new(executors)),
        })
    }

    pub async fn create(&self, executor: RemoteExecutorRecord) -> anyhow::Result<()> {
        validate_executor(&executor)?;
        let mut executors = self.inner.write().await;
        if executors.contains_key(&executor.executor_id) {
            anyhow::bail!("executor {} already exists", executor.executor_id);
        }
        self.persist(&executor)?;
        executors.insert(executor.executor_id.clone(), executor);
        Ok(())
    }

    /// Seed a config-declared executor at startup: validate, then insert only if no record
    /// with the same id exists (never overwrites an API-created/modified one).
    pub async fn create_if_absent(&self, executor: RemoteExecutorRecord) -> anyhow::Result<()> {
        validate_executor(&executor)?;
        let mut executors = self.inner.write().await;
        if executors.contains_key(&executor.executor_id) {
            return Ok(());
        }
        self.persist(&executor)?;
        executors.insert(executor.executor_id.clone(), executor);
        Ok(())
    }

    pub async fn get(&self, executor_id: &str) -> Option<RemoteExecutorRecord> {
        self.inner.read().await.get(executor_id).cloned()
    }

    pub async fn list(&self) -> Vec<RemoteExecutorRecord> {
        let mut executors = self
            .inner
            .read()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        executors.sort_by(|left, right| {
            right
                .enabled
                .cmp(&left.enabled)
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.created_at.cmp(&right.created_at))
        });
        executors
    }

    pub async fn update(
        &self,
        executor_id: &str,
        update: impl FnOnce(&mut RemoteExecutorRecord) -> anyhow::Result<()>,
    ) -> anyhow::Result<RemoteExecutorRecord> {
        let mut executors = self.inner.write().await;
        let mut candidate = executors
            .get(executor_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown executor {executor_id}"))?;
        update(&mut candidate)?;
        candidate.updated_at = Utc::now();
        validate_executor(&candidate)?;
        self.persist(&candidate)?;
        executors.insert(executor_id.to_string(), candidate.clone());
        Ok(candidate)
    }

    pub async fn disable(&self, executor_id: &str) -> anyhow::Result<RemoteExecutorRecord> {
        self.update(executor_id, |executor| {
            executor.enabled = false;
            Ok(())
        })
        .await
    }

    fn persist(&self, executor: &RemoteExecutorRecord) -> anyhow::Result<()> {
        let path = self.dir.join(format!("{}.json", executor.executor_id));
        let temp = self.dir.join(format!(".{}.json.tmp", executor.executor_id));
        fs::write(&temp, serde_json::to_vec_pretty(executor)?)?;
        fs::rename(&temp, &path)?;
        Ok(())
    }
}

pub fn validate_executor(executor: &RemoteExecutorRecord) -> anyhow::Result<()> {
    use crate::domain::models::ExecutorKind;
    validate_executor_id(&executor.executor_id)?;
    validate_non_empty("name", &executor.name, 120)?;
    match executor.kind {
        ExecutorKind::Ssh => {
            validate_non_empty("host", &executor.host, 255)?;
            validate_non_empty("user", &executor.user, 64)?;
            if executor.port == 0 {
                anyhow::bail!("executor port must be greater than zero");
            }
        }
        ExecutorKind::Docker => {
            let docker = executor.docker.as_ref().ok_or_else(|| {
                anyhow::anyhow!(
                    "docker executor {} is missing its docker spec",
                    executor.executor_id
                )
            })?;
            let context = format!("executor {}", executor.executor_id);
            crate::support::docker_target::validate_docker_target(docker, &context, false)?;
        }
    }
    if executor.tags.len() > 20 {
        anyhow::bail!("executor tags exceed maximum length of 20");
    }
    for tag in &executor.tags {
        validate_non_empty("tag", tag, 64)?;
    }
    if let Some(notes) = &executor.notes {
        if notes.chars().count() > 500 {
            anyhow::bail!("executor notes exceed maximum length of 500");
        }
    }
    Ok(())
}

fn validate_executor_id(executor_id: &str) -> anyhow::Result<()> {
    let valid = executor_id.starts_with("executor_")
        && executor_id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'_' || value == b'-');
    if valid {
        Ok(())
    } else {
        anyhow::bail!("invalid executor id {executor_id}")
    }
}

fn validate_non_empty(field: &str, value: &str, max_chars: usize) -> anyhow::Result<()> {
    let value = value.trim();
    if value.is_empty() {
        anyhow::bail!("executor {field} must not be empty");
    }
    if value.chars().count() > max_chars {
        anyhow::bail!("executor {field} exceeds maximum length of {max_chars}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{domain::models::ExecutorKind, support::docker_target::DockerTargetSpec};
    use std::collections::BTreeMap;

    fn ssh_record() -> RemoteExecutorRecord {
        RemoteExecutorRecord {
            schema_version: 1,
            executor_id: "executor_ssh_1".to_string(),
            name: "ssh".to_string(),
            kind: ExecutorKind::Ssh,
            host: "h".to_string(),
            port: 22,
            user: "u".to_string(),
            docker: None,
            tags: Vec::new(),
            enabled: true,
            notes: None,
            last_check: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn docker_record(spec: Option<DockerTargetSpec>) -> RemoteExecutorRecord {
        RemoteExecutorRecord {
            schema_version: 1,
            executor_id: "executor_docker_1".to_string(),
            name: "docker".to_string(),
            kind: ExecutorKind::Docker,
            host: String::new(),
            port: 0,
            user: String::new(),
            docker: spec,
            tags: Vec::new(),
            enabled: true,
            notes: None,
            last_check: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn spec(image: &str) -> DockerTargetSpec {
        DockerTargetSpec {
            image: image.to_string(),
            network: Some("host".to_string()),
            workdir: None,
            volumes: vec!["/h:/c:ro".to_string()],
            env: BTreeMap::new(),
        }
    }

    #[test]
    fn validate_ssh_record_branch() {
        assert!(validate_executor(&ssh_record()).is_ok());
        let mut bad = ssh_record();
        bad.host = String::new();
        assert!(validate_executor(&bad)
            .unwrap_err()
            .to_string()
            .contains("host"));
    }

    #[test]
    fn validate_docker_record_branch() {
        assert!(validate_executor(&docker_record(Some(spec("alpine:3.20")))).is_ok());
        // missing docker spec
        assert!(validate_executor(&docker_record(None))
            .unwrap_err()
            .to_string()
            .contains("missing its docker spec"));
        // invalid image (flag-like)
        let err = validate_executor(&docker_record(Some(spec("-flag"))))
            .unwrap_err()
            .to_string();
        assert!(err.contains("must not start with '-'"));
    }

    #[tokio::test]
    async fn create_if_absent_does_not_overwrite() {
        let root = std::env::temp_dir().join(format!(
            "logagent-exec-store-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let store = RemoteExecutorStore::load(root.clone()).unwrap();
        let mut rec = ssh_record();
        rec.executor_id = "executor_x".to_string();
        store.create(rec.clone()).await.unwrap();
        // seed with a different host — create_if_absent must keep the original.
        let mut seed = rec.clone();
        seed.host = "other".to_string();
        store.create_if_absent(seed).await.unwrap();
        let got = store.get("executor_x").await.unwrap();
        assert_eq!(got.host, "h");
        let _ = std::fs::remove_dir_all(root);
    }
}
