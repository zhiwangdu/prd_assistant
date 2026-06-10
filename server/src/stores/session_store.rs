use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::{BufRead, Write},
    path::PathBuf,
    sync::Arc,
};

use chrono::Utc;
use tokio::sync::RwLock;

use crate::domain::models::{
    AnalysisSessionEvent, AnalysisSessionRecord, AnalysisSessionStatus, TaskKind, TaskRecord,
};

#[derive(Debug, Clone)]
pub struct AnalysisSessionStore {
    dir: PathBuf,
    workspace_dir: PathBuf,
    inner: Arc<RwLock<HashMap<String, AnalysisSessionRecord>>>,
}

impl AnalysisSessionStore {
    pub fn load(dir: PathBuf, workspace_dir: PathBuf) -> anyhow::Result<Self> {
        fs::create_dir_all(&dir)?;
        fs::create_dir_all(&workspace_dir)?;
        let mut sessions = HashMap::new();
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let raw = fs::read_to_string(&path)?;
            let session: AnalysisSessionRecord = serde_json::from_str(&raw).map_err(|err| {
                anyhow::anyhow!("invalid session record {}: {err}", path.display())
            })?;
            validate_loaded_session(&session).map_err(|err| {
                anyhow::anyhow!("invalid session record {}: {err}", path.display())
            })?;
            if sessions
                .insert(session.session_id.clone(), session)
                .is_some()
            {
                anyhow::bail!("duplicate session record in {}", path.display());
            }
        }
        Ok(Self {
            dir,
            workspace_dir,
            inner: Arc::new(RwLock::new(sessions)),
        })
    }

    pub async fn create(&self, session: AnalysisSessionRecord) -> anyhow::Result<()> {
        validate_loaded_session(&session)?;
        let mut sessions = self.inner.write().await;
        if sessions.contains_key(&session.session_id) {
            anyhow::bail!("session {} already exists", session.session_id);
        }
        self.persist(&session)?;
        fs::create_dir_all(self.session_workspace_dir(&session.session_id))?;
        sessions.insert(session.session_id.clone(), session.clone());
        self.append_event(AnalysisSessionEvent {
            schema_version: 1,
            session_id: session.session_id.clone(),
            event_type: "session_created".to_string(),
            task_id: None,
            upload_id: None,
            message: format!("session {} created", session.session_id),
            artifact_path: None,
            details: serde_json::json!({
                "title": session.title,
                "status": session.status,
            }),
            created_at: Utc::now(),
        })?;
        Ok(())
    }

    pub async fn get(&self, session_id: &str) -> Option<AnalysisSessionRecord> {
        self.inner.read().await.get(session_id).cloned()
    }

    pub async fn list(&self) -> Vec<AnalysisSessionRecord> {
        let mut sessions = self
            .inner
            .read()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        sessions.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        sessions
    }

    pub async fn update(
        &self,
        session_id: &str,
        update: impl FnOnce(&mut AnalysisSessionRecord) -> anyhow::Result<()>,
        event_type: &str,
        message: String,
        details: serde_json::Value,
    ) -> anyhow::Result<AnalysisSessionRecord> {
        let mut sessions = self.inner.write().await;
        let mut candidate = sessions
            .get(session_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown session {session_id}"))?;
        update(&mut candidate)?;
        validate_loaded_session(&candidate)?;
        candidate.updated_at = Utc::now();
        self.persist(&candidate)?;
        sessions.insert(session_id.to_string(), candidate.clone());
        drop(sessions);
        self.append_event(AnalysisSessionEvent {
            schema_version: 1,
            session_id: session_id.to_string(),
            event_type: event_type.to_string(),
            task_id: None,
            upload_id: None,
            message,
            artifact_path: None,
            details,
            created_at: Utc::now(),
        })?;
        Ok(candidate)
    }

    pub async fn attach_uploads(
        &self,
        session_id: &str,
        upload_ids: &[String],
    ) -> anyhow::Result<AnalysisSessionRecord> {
        let mut unique = Vec::new();
        for upload_id in upload_ids {
            if !unique.iter().any(|value| value == upload_id) {
                unique.push(upload_id.clone());
            }
        }
        if unique.is_empty() {
            anyhow::bail!("missing uploadIds");
        }
        let mut sessions = self.inner.write().await;
        let mut candidate = sessions
            .get(session_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown session {session_id}"))?;
        for upload_id in &unique {
            if !candidate.upload_ids.iter().any(|value| value == upload_id) {
                candidate.upload_ids.push(upload_id.clone());
            }
        }
        if candidate.status == AnalysisSessionStatus::Draft && !candidate.upload_ids.is_empty() {
            candidate.status = AnalysisSessionStatus::Ready;
        }
        candidate.updated_at = Utc::now();
        self.persist(&candidate)?;
        sessions.insert(session_id.to_string(), candidate.clone());
        drop(sessions);
        for upload_id in unique {
            self.append_event(AnalysisSessionEvent {
                schema_version: 1,
                session_id: session_id.to_string(),
                event_type: "upload_attached".to_string(),
                task_id: None,
                upload_id: Some(upload_id.clone()),
                message: format!("upload {upload_id} attached"),
                artifact_path: None,
                details: serde_json::json!({ "uploadId": upload_id }),
                created_at: Utc::now(),
            })?;
        }
        Ok(candidate)
    }

    pub async fn detach_upload(
        &self,
        session_id: &str,
        upload_id: &str,
    ) -> anyhow::Result<AnalysisSessionRecord> {
        let mut sessions = self.inner.write().await;
        let mut candidate = sessions
            .get(session_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown session {session_id}"))?;
        if candidate.status.is_running_like() {
            anyhow::bail!("cannot detach uploads from a running or waiting session");
        }
        if candidate.task_ids.is_empty() {
            candidate.upload_ids.retain(|value| value != upload_id);
        } else if candidate.upload_ids.iter().any(|value| value == upload_id) {
            anyhow::bail!("cannot detach uploads after a task run has been created");
        }
        if candidate.upload_ids.is_empty() && candidate.task_ids.is_empty() {
            candidate.status = AnalysisSessionStatus::Draft;
        }
        candidate.updated_at = Utc::now();
        self.persist(&candidate)?;
        sessions.insert(session_id.to_string(), candidate.clone());
        drop(sessions);
        self.append_event(AnalysisSessionEvent {
            schema_version: 1,
            session_id: session_id.to_string(),
            event_type: "upload_detached".to_string(),
            task_id: None,
            upload_id: Some(upload_id.to_string()),
            message: format!("upload {upload_id} detached"),
            artifact_path: None,
            details: serde_json::json!({ "uploadId": upload_id }),
            created_at: Utc::now(),
        })?;
        Ok(candidate)
    }

    pub async fn add_task_run(
        &self,
        session_id: &str,
        task_id: &str,
    ) -> anyhow::Result<AnalysisSessionRecord> {
        let mut sessions = self.inner.write().await;
        let mut candidate = sessions
            .get(session_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown session {session_id}"))?;
        if !candidate.task_ids.iter().any(|value| value == task_id) {
            candidate.task_ids.push(task_id.to_string());
        }
        candidate.active_task_id = Some(task_id.to_string());
        candidate.status = AnalysisSessionStatus::Ready;
        candidate.updated_at = Utc::now();
        self.persist(&candidate)?;
        sessions.insert(session_id.to_string(), candidate.clone());
        drop(sessions);
        self.append_event(AnalysisSessionEvent {
            schema_version: 1,
            session_id: session_id.to_string(),
            event_type: "task_created".to_string(),
            task_id: Some(task_id.to_string()),
            upload_id: None,
            message: format!("task {task_id} created from session snapshot"),
            artifact_path: None,
            details: serde_json::json!({ "taskId": task_id }),
            created_at: Utc::now(),
        })?;
        Ok(candidate)
    }

    pub async fn sync_task_status(&self, task: &TaskRecord) -> anyhow::Result<()> {
        if task.task_kind != TaskKind::LogAnalysis {
            return Ok(());
        }
        let Some(session_id) = task.session_id.as_deref() else {
            return Ok(());
        };
        let mut sessions = self.inner.write().await;
        let Some(current) = sessions.get(session_id).cloned() else {
            return Ok(());
        };
        if !current.task_ids.iter().any(|value| value == &task.task_id) {
            return Ok(());
        }
        let mut candidate = current;
        candidate.active_task_id = Some(task.task_id.clone());
        candidate.status = AnalysisSessionStatus::from_task_status(task.status);
        candidate.updated_at = Utc::now();
        self.persist(&candidate)?;
        sessions.insert(session_id.to_string(), candidate);
        drop(sessions);
        self.append_event(AnalysisSessionEvent {
            schema_version: 1,
            session_id: session_id.to_string(),
            event_type: "task_status_changed".to_string(),
            task_id: Some(task.task_id.clone()),
            upload_id: None,
            message: format!("task {} status changed to {:?}", task.task_id, task.status),
            artifact_path: task.result_json_path.clone(),
            details: serde_json::json!({
                "taskId": task.task_id,
                "status": task.status,
                "phase": task.phase,
                "attempts": task.attempts,
                "error": task.error,
            }),
            created_at: Utc::now(),
        })?;
        Ok(())
    }

    pub fn read_events(&self, session_id: &str) -> anyhow::Result<Vec<AnalysisSessionEvent>> {
        validate_session_id(session_id)?;
        let path = self.session_events_path(session_id);
        let file = match fs::File::open(&path) {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(err.into()),
        };
        let reader = std::io::BufReader::new(file);
        let mut events = Vec::new();
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            events.push(serde_json::from_str(&line)?);
        }
        Ok(events)
    }

    pub fn record_event(
        &self,
        session_id: &str,
        event_type: &str,
        task_id: Option<String>,
        upload_id: Option<String>,
        message: String,
        artifact_path: Option<String>,
        details: serde_json::Value,
    ) -> anyhow::Result<()> {
        self.append_event(AnalysisSessionEvent {
            schema_version: 1,
            session_id: session_id.to_string(),
            event_type: event_type.to_string(),
            task_id,
            upload_id,
            message,
            artifact_path,
            details,
            created_at: Utc::now(),
        })
    }

    fn append_event(&self, event: AnalysisSessionEvent) -> anyhow::Result<()> {
        validate_session_id(&event.session_id)?;
        let workspace = self.session_workspace_dir(&event.session_id);
        fs::create_dir_all(&workspace)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(workspace.join("session_events.jsonl"))?;
        serde_json::to_writer(&mut file, &event)?;
        file.write_all(b"\n")?;
        file.flush()?;
        Ok(())
    }

    fn session_workspace_dir(&self, session_id: &str) -> PathBuf {
        self.workspace_dir.join(session_id)
    }

    fn session_events_path(&self, session_id: &str) -> PathBuf {
        self.session_workspace_dir(session_id)
            .join("session_events.jsonl")
    }

    fn persist(&self, session: &AnalysisSessionRecord) -> anyhow::Result<()> {
        let path = self.dir.join(format!("{}.json", session.session_id));
        let temp = self.dir.join(format!(".{}.json.tmp", session.session_id));
        fs::write(&temp, serde_json::to_vec_pretty(session)?)?;
        fs::rename(&temp, &path)?;
        Ok(())
    }
}

pub fn validate_session_id(session_id: &str) -> anyhow::Result<()> {
    let valid = session_id.starts_with("sess_")
        && session_id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'_' || value == b'-');
    if valid {
        Ok(())
    } else {
        anyhow::bail!("invalid sessionId")
    }
}

fn validate_loaded_session(session: &AnalysisSessionRecord) -> anyhow::Result<()> {
    validate_session_id(&session.session_id)?;
    if session.schema_version != 1 {
        anyhow::bail!(
            "unsupported session schemaVersion {}",
            session.schema_version
        );
    }
    if session.title.trim().is_empty() {
        anyhow::bail!("session {} is missing title", session.session_id);
    }
    if session.question.chars().count() > 120_000 {
        anyhow::bail!("session {} question is too long", session.session_id);
    }
    for upload_id in &session.upload_ids {
        if !upload_id.starts_with("upl_") {
            anyhow::bail!("session {} contains invalid uploadId", session.session_id);
        }
    }
    for task_id in &session.task_ids {
        if !task_id.starts_with("task_") {
            anyhow::bail!("session {} contains invalid taskId", session.session_id);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "logagent-session-store-{name}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ))
    }

    fn session(id: &str) -> AnalysisSessionRecord {
        let now = Utc::now();
        AnalysisSessionRecord {
            schema_version: 1,
            session_id: id.to_string(),
            title: "Test session".to_string(),
            question: "Why did it fail?".to_string(),
            source_url: None,
            instance_id: None,
            node_id: None,
            system_context_ids: Vec::new(),
            upload_ids: Vec::new(),
            task_ids: Vec::new(),
            active_task_id: None,
            status: AnalysisSessionStatus::Draft,
            created_at: now,
            updated_at: now,
        }
    }

    #[tokio::test]
    async fn persists_updates_and_records_events() {
        let root = temp_dir("basic");
        let store =
            AnalysisSessionStore::load(root.join("sessions"), root.join("workspaces")).unwrap();
        store.create(session("sess_1")).await.unwrap();
        let attached = store
            .attach_uploads("sess_1", &["upl_1".to_string(), "upl_1".to_string()])
            .await
            .unwrap();
        assert_eq!(attached.upload_ids, vec!["upl_1"]);
        assert_eq!(attached.status, AnalysisSessionStatus::Ready);
        store.add_task_run("sess_1", "task_1").await.unwrap();

        let reloaded =
            AnalysisSessionStore::load(root.join("sessions"), root.join("workspaces")).unwrap();
        let loaded = reloaded.get("sess_1").await.unwrap();
        assert_eq!(loaded.task_ids, vec!["task_1"]);
        let events = reloaded.read_events("sess_1").unwrap();
        assert!(events
            .iter()
            .any(|event| event.event_type == "session_created"));
        assert!(events
            .iter()
            .any(|event| event.event_type == "upload_attached"));
        assert!(events
            .iter()
            .any(|event| event.event_type == "task_created"));
        let _ = std::fs::remove_dir_all(root);
    }
}
