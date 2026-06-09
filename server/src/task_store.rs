use std::{collections::HashMap, fs, path::PathBuf, sync::Arc};

use chrono::Utc;
use tokio::sync::RwLock;

use crate::models::{TaskError, TaskPhase, TaskRecord, TaskStatus};

#[derive(Debug, Clone)]
pub struct TaskStore {
    dir: PathBuf,
    inner: Arc<RwLock<HashMap<String, TaskRecord>>>,
}

impl TaskStore {
    pub fn load(dir: PathBuf) -> anyhow::Result<Self> {
        fs::create_dir_all(&dir)?;
        let mut tasks = HashMap::new();
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let raw = fs::read_to_string(&path)?;
            let task: TaskRecord = serde_json::from_str(&raw)
                .map_err(|err| anyhow::anyhow!("invalid task record {}: {err}", path.display()))?;
            validate_loaded_task(&task)
                .map_err(|err| anyhow::anyhow!("invalid task record {}: {err}", path.display()))?;
            if tasks.insert(task.task_id.clone(), task).is_some() {
                anyhow::bail!("duplicate task record in {}", path.display());
            }
        }
        Ok(Self {
            dir,
            inner: Arc::new(RwLock::new(tasks)),
        })
    }

    pub async fn create(&self, task: TaskRecord) -> anyhow::Result<()> {
        let mut tasks = self.inner.write().await;
        if tasks.contains_key(&task.task_id) {
            anyhow::bail!("task {} already exists", task.task_id);
        }
        self.persist(&task)?;
        tasks.insert(task.task_id.clone(), task);
        Ok(())
    }

    pub async fn get(&self, task_id: &str) -> Option<TaskRecord> {
        self.inner.read().await.get(task_id).cloned()
    }

    pub async fn list(&self) -> Vec<TaskRecord> {
        let mut tasks = self
            .inner
            .read()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        tasks.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        tasks
    }

    pub async fn update(
        &self,
        task_id: &str,
        update: impl FnOnce(&mut TaskRecord) -> anyhow::Result<()>,
    ) -> anyhow::Result<TaskRecord> {
        let mut tasks = self.inner.write().await;
        let mut candidate = tasks
            .get(task_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown task {task_id}"))?;
        update(&mut candidate)?;
        candidate.updated_at = Utc::now();
        self.persist(&candidate)?;
        tasks.insert(task_id.to_string(), candidate.clone());
        Ok(candidate)
    }

    pub async fn start_attempt(
        &self,
        task_id: &str,
        initial_phase: TaskPhase,
    ) -> anyhow::Result<TaskRecord> {
        self.update(task_id, |task| {
            if task.status != TaskStatus::Queued {
                anyhow::bail!("task {task_id} is not queued");
            }
            task.status = TaskStatus::Running;
            task.phase = Some(task.phase.unwrap_or(initial_phase));
            task.attempts += 1;
            task.error = None;
            Ok(())
        })
        .await
    }

    pub async fn advance_phase(
        &self,
        task_id: &str,
        expected: TaskPhase,
        next: TaskPhase,
    ) -> anyhow::Result<TaskRecord> {
        self.update(task_id, |task| {
            if task.status != TaskStatus::Running {
                anyhow::bail!("task {task_id} is not running");
            }
            if task.phase != Some(expected) {
                anyhow::bail!(
                    "task {task_id} phase changed while executing: expected {expected:?}, found {:?}",
                    task.phase
                );
            }
            task.phase = Some(next);
            Ok(())
        })
        .await
    }

    pub async fn succeed(
        &self,
        task_id: &str,
        expected: TaskPhase,
        manifest_path: String,
        grep_results_path: String,
        result_json_path: String,
        result_markdown_path: String,
    ) -> anyhow::Result<TaskRecord> {
        self.update(task_id, |task| {
            ensure_running(task)?;
            ensure_phase(task, expected)?;
            task.status = TaskStatus::Succeeded;
            task.phase = None;
            task.error = None;
            task.manifest_path = Some(manifest_path);
            task.grep_results_path = Some(grep_results_path);
            task.result_json_path = Some(result_json_path);
            task.result_markdown_path = Some(result_markdown_path);
            Ok(())
        })
        .await
    }

    pub async fn succeed_tool_run(
        &self,
        task_id: &str,
        expected: TaskPhase,
        tool_result_path: String,
    ) -> anyhow::Result<TaskRecord> {
        self.update(task_id, |task| {
            ensure_running(task)?;
            ensure_phase(task, expected)?;
            task.status = TaskStatus::Succeeded;
            task.phase = None;
            task.error = None;
            task.tool_result_path = Some(tool_result_path);
            Ok(())
        })
        .await
    }

    pub async fn fail(
        &self,
        task_id: &str,
        phase: Option<TaskPhase>,
        message: String,
    ) -> anyhow::Result<TaskRecord> {
        self.update(task_id, |task| {
            ensure_running(task)?;
            if let Some(phase) = phase {
                ensure_phase(task, phase)?;
            }
            task.status = TaskStatus::Failed;
            task.phase = phase;
            task.error = Some(TaskError { phase, message });
            Ok(())
        })
        .await
    }

    pub async fn wait_for_user(&self, task_id: &str) -> anyhow::Result<TaskRecord> {
        self.wait(task_id, TaskStatus::WaitingForUser).await
    }

    pub async fn wait_for_approval(&self, task_id: &str) -> anyhow::Result<TaskRecord> {
        self.wait(task_id, TaskStatus::WaitingForApproval).await
    }

    async fn wait(&self, task_id: &str, status: TaskStatus) -> anyhow::Result<TaskRecord> {
        self.update(task_id, |task| {
            ensure_running(task)?;
            ensure_phase(task, TaskPhase::PlanAnalysis)?;
            task.status = status;
            task.phase = Some(TaskPhase::PlanAnalysis);
            task.error = None;
            Ok(())
        })
        .await
    }

    pub async fn resume_waiting(
        &self,
        task_id: &str,
        expected: TaskStatus,
    ) -> anyhow::Result<TaskRecord> {
        self.update(task_id, |task| {
            if task.status != expected {
                anyhow::bail!(
                    "task {task_id} is not in expected waiting status: expected {expected:?}, found {:?}",
                    task.status
                );
            }
            task.status = TaskStatus::Queued;
            task.phase = Some(TaskPhase::PlanAnalysis);
            task.error = None;
            Ok(())
        })
        .await
    }

    pub async fn recover_incomplete(&self) -> anyhow::Result<Vec<TaskRecord>> {
        let mut tasks = self.inner.write().await;
        let mut recovered = Vec::new();
        for task in tasks.values_mut() {
            if task.status == TaskStatus::Running {
                task.status = TaskStatus::Queued;
                task.error = None;
                task.updated_at = Utc::now();
                self.persist(task)?;
            }
            if task.status == TaskStatus::Queued {
                recovered.push(task.clone());
            }
        }
        recovered.sort_by_key(|task| task.created_at);
        Ok(recovered)
    }

    fn persist(&self, task: &TaskRecord) -> anyhow::Result<()> {
        let path = self.dir.join(format!("{}.json", task.task_id));
        let temp = self.dir.join(format!(".{}.json.tmp", task.task_id));
        fs::write(&temp, serde_json::to_vec_pretty(task)?)?;
        fs::rename(&temp, &path)?;
        Ok(())
    }
}

fn ensure_running(task: &TaskRecord) -> anyhow::Result<()> {
    if task.status.is_terminal() {
        anyhow::bail!("terminal task {} cannot be overwritten", task.task_id);
    }
    if task.status != TaskStatus::Running {
        anyhow::bail!("task {} is not running", task.task_id);
    }
    Ok(())
}

fn ensure_phase(task: &TaskRecord, expected: TaskPhase) -> anyhow::Result<()> {
    if task.phase != Some(expected) {
        anyhow::bail!(
            "task {} phase changed while executing: expected {expected:?}, found {:?}",
            task.task_id,
            task.phase
        );
    }
    Ok(())
}

fn validate_loaded_task(task: &TaskRecord) -> anyhow::Result<()> {
    if task.status == TaskStatus::Running && task.phase.is_none() {
        anyhow::bail!("RUNNING task {} is missing phase", task.task_id);
    }
    if matches!(
        task.status,
        TaskStatus::WaitingForUser | TaskStatus::WaitingForApproval
    ) && task.phase != Some(TaskPhase::PlanAnalysis)
    {
        anyhow::bail!(
            "waiting task {} must retain PLAN_ANALYSIS phase",
            task.task_id
        );
    }
    if task.status == TaskStatus::Succeeded && task.phase.is_some() {
        anyhow::bail!("SUCCEEDED task {} must not retain phase", task.task_id);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{TaskInput, TaskSource};

    fn task(id: &str, created_at: chrono::DateTime<Utc>) -> TaskRecord {
        TaskRecord {
            schema_version: 1,
            task_id: id.to_string(),
            task_kind: crate::models::TaskKind::LogAnalysis,
            source: TaskSource::Upload,
            upload_ids: vec!["upl_1".to_string()],
            inputs: vec![TaskInput {
                upload_id: "upl_1".to_string(),
                filename: "sample.log".to_string(),
                size: 1,
                raw_path: "raw/upl_1/sample.log".to_string(),
            }],
            source_url: None,
            tool_id: None,
            tool_params: serde_json::Value::Null,
            tool_result_path: None,
            instance_id: None,
            cluster_id: None,
            node_id: None,
            question: crate::models::default_task_question(),
            status: TaskStatus::Queued,
            phase: None,
            attempts: 0,
            error: None,
            manifest_path: None,
            grep_results_path: None,
            metadata_context_path: None,
            result_json_path: None,
            result_markdown_path: None,
            created_at,
            updated_at: created_at,
        }
    }

    #[tokio::test]
    async fn persists_lists_and_recovers_tasks() {
        let dir = temp_dir("task-store");
        let store = TaskStore::load(dir.clone()).unwrap();
        store
            .create(task("task_old", Utc::now() - chrono::Duration::seconds(1)))
            .await
            .unwrap();
        store.create(task("task_new", Utc::now())).await.unwrap();
        store
            .start_attempt("task_old", TaskPhase::Extract)
            .await
            .unwrap();
        store
            .advance_phase("task_old", TaskPhase::Extract, TaskPhase::SearchLogs)
            .await
            .unwrap();

        let reloaded = TaskStore::load(dir.clone()).unwrap();
        assert_eq!(reloaded.list().await[0].task_id, "task_new");
        let recovered = reloaded.recover_incomplete().await.unwrap();
        assert_eq!(
            recovered
                .iter()
                .map(|task| task.task_id.as_str())
                .collect::<Vec<_>>(),
            vec!["task_old", "task_new"]
        );
        let task_old = reloaded.get("task_old").await.unwrap();
        assert_eq!(task_old.attempts, 1);
        assert_eq!(task_old.phase, Some(TaskPhase::SearchLogs));
        let resumed = reloaded
            .start_attempt("task_old", TaskPhase::Extract)
            .await
            .unwrap();
        assert_eq!(resumed.attempts, 2);
        assert_eq!(resumed.phase, Some(TaskPhase::SearchLogs));
        reloaded
            .start_attempt("task_new", TaskPhase::Extract)
            .await
            .unwrap();
        reloaded
            .fail(
                "task_new",
                Some(TaskPhase::Extract),
                "expected failure".to_string(),
            )
            .await
            .unwrap();
        assert!(reloaded
            .succeed(
                "task_new",
                TaskPhase::Extract,
                "manifest.json".to_string(),
                "grep_results.json".to_string(),
                "result.json".to_string(),
                "result.md".to_string()
            )
            .await
            .is_err());
        let _ = fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn phase_advancement_rejects_stale_dispatchers() {
        let dir = temp_dir("task-store-phase");
        let store = TaskStore::load(dir.clone()).unwrap();
        store.create(task("task_1", Utc::now())).await.unwrap();
        store
            .start_attempt("task_1", TaskPhase::Extract)
            .await
            .unwrap();

        assert!(store
            .advance_phase("task_1", TaskPhase::SearchLogs, TaskPhase::GenerateResult)
            .await
            .is_err());
        assert!(store
            .fail(
                "task_1",
                Some(TaskPhase::SearchLogs),
                "stale failure".to_string()
            )
            .await
            .is_err());
        assert!(store
            .succeed(
                "task_1",
                TaskPhase::SearchLogs,
                "manifest.json".to_string(),
                "grep_results.json".to_string(),
                "result.json".to_string(),
                "result.md".to_string()
            )
            .await
            .is_err());
        assert_eq!(
            store.get("task_1").await.unwrap().phase,
            Some(TaskPhase::Extract)
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn corrupt_task_json_fails_loading() {
        let dir = temp_dir("task-store-corrupt");
        fs::write(dir.join("task_bad.json"), b"{bad").unwrap();
        assert!(TaskStore::load(dir.clone()).is_err());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn inconsistent_task_state_fails_loading() {
        let dir = temp_dir("task-store-inconsistent");
        let mut record = task("task_bad_state", Utc::now());
        record.status = TaskStatus::Running;
        record.phase = None;
        fs::write(
            dir.join("task_bad_state.json"),
            serde_json::to_vec_pretty(&record).unwrap(),
        )
        .unwrap();
        assert!(TaskStore::load(dir.clone()).is_err());
        let _ = fs::remove_dir_all(dir);
    }

    fn temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "logagent-{name}-{}",
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }
}
