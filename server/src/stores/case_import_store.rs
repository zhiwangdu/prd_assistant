use std::{collections::HashMap, fs, path::PathBuf, sync::Arc};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct CaseImportStore {
    dir: PathBuf,
    inner: Arc<RwLock<HashMap<String, CaseImportSession>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseImportSession {
    pub schema_version: u32,
    pub draft_id: String,
    pub source_type: CaseImportSourceType,
    pub filename: Option<String>,
    pub source_text: String,
    pub structured_case: CaseImportDraft,
    pub missing_fields: Vec<CaseMissingField>,
    pub assistant_question: Option<String>,
    pub ready_to_confirm: bool,
    pub status: CaseImportStatus,
    pub messages: Vec<CaseImportMessage>,
    pub confirmed_case_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseImportSourceType {
    Text,
    File,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseImportStatus {
    NeedsInput,
    Ready,
    Saved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseImportMessageRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseImportMessage {
    pub role: CaseImportMessageRole,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseImportDraft {
    pub title: Option<String>,
    pub symptom: Option<String>,
    pub root_cause: Option<String>,
    pub solution: Option<String>,
    pub product: Option<String>,
    pub version: Option<String>,
    pub environment: Option<String>,
    pub instance_id: Option<String>,
    pub node_id: Option<String>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseMissingField {
    pub field: String,
    pub label: String,
    pub question: String,
}

impl CaseImportStore {
    pub fn load(dir: PathBuf) -> anyhow::Result<Self> {
        fs::create_dir_all(&dir)?;
        let mut sessions = HashMap::new();
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let raw = fs::read_to_string(&path)?;
            let mut session: CaseImportSession = serde_json::from_str(&raw).map_err(|err| {
                anyhow::anyhow!("invalid case import session {}: {err}", path.display())
            })?;
            normalize_session(&mut session);
            validate_session(&session).map_err(|err| {
                anyhow::anyhow!("invalid case import session {}: {err}", path.display())
            })?;
            if sessions.insert(session.draft_id.clone(), session).is_some() {
                anyhow::bail!("duplicate case import session in {}", path.display());
            }
        }
        Ok(Self {
            dir,
            inner: Arc::new(RwLock::new(sessions)),
        })
    }

    pub async fn create(
        &self,
        mut session: CaseImportSession,
    ) -> anyhow::Result<CaseImportSession> {
        normalize_session(&mut session);
        validate_session(&session)?;
        let mut sessions = self.inner.write().await;
        if sessions.contains_key(&session.draft_id) {
            anyhow::bail!("duplicate case import draft id");
        }
        self.persist(&session)?;
        sessions.insert(session.draft_id.clone(), session.clone());
        Ok(session)
    }

    pub async fn get(&self, draft_id: &str) -> Option<CaseImportSession> {
        self.inner.read().await.get(draft_id).cloned()
    }

    pub async fn update(
        &self,
        mut session: CaseImportSession,
    ) -> anyhow::Result<CaseImportSession> {
        normalize_session(&mut session);
        validate_session(&session)?;
        let mut sessions = self.inner.write().await;
        if !sessions.contains_key(&session.draft_id) {
            anyhow::bail!("unknown case import draft");
        }
        self.persist(&session)?;
        sessions.insert(session.draft_id.clone(), session.clone());
        Ok(session)
    }

    fn persist(&self, session: &CaseImportSession) -> anyhow::Result<()> {
        let path = self.dir.join(format!("{}.json", session.draft_id));
        let temp = self.dir.join(format!(".{}.json.tmp", session.draft_id));
        fs::write(&temp, serde_json::to_vec_pretty(session)?)?;
        fs::rename(&temp, &path)?;
        Ok(())
    }
}

pub fn normalize_import_draft(draft: &mut CaseImportDraft) {
    draft.title = clean_optional_text(draft.title.take()).map(|value| truncate(value, 160));
    draft.symptom = clean_optional_text(draft.symptom.take()).map(|value| truncate(value, 4_000));
    draft.root_cause =
        clean_optional_text(draft.root_cause.take()).map(|value| truncate(value, 4_000));
    draft.solution = clean_optional_text(draft.solution.take()).map(|value| truncate(value, 4_000));
    draft.product = clean_optional_text(draft.product.take());
    draft.version = clean_optional_text(draft.version.take());
    draft.environment = clean_optional_text(draft.environment.take());
    draft.instance_id = clean_optional_text(draft.instance_id.take());
    draft.node_id = clean_optional_text(draft.node_id.take());
    draft.evidence_refs = draft
        .evidence_refs
        .iter()
        .map(|value| clean_text(value))
        .filter(|value| !value.is_empty())
        .take(64)
        .collect();
}

pub fn compute_missing_fields(draft: &CaseImportDraft) -> Vec<CaseMissingField> {
    let mut missing = Vec::new();
    if is_empty(&draft.title) {
        missing.push(CaseMissingField {
            field: "title".to_string(),
            label: "标题".to_string(),
            question: "请补充这个 Case 的简短标题。".to_string(),
        });
    }
    if is_empty(&draft.symptom) {
        missing.push(CaseMissingField {
            field: "symptom".to_string(),
            label: "现象".to_string(),
            question: "请补充故障现象，包括用户感知、报错或异常表现。".to_string(),
        });
    }
    if is_empty(&draft.root_cause) {
        missing.push(CaseMissingField {
            field: "rootCause".to_string(),
            label: "根因".to_string(),
            question: "请补充最终确认的根因；如果还没有结论，请说明当前已确认到哪一步。"
                .to_string(),
        });
    }
    if is_empty(&draft.solution) {
        missing.push(CaseMissingField {
            field: "solution".to_string(),
            label: "解决方案".to_string(),
            question: "请补充已执行或建议执行的解决方案。".to_string(),
        });
    }
    missing
}

pub fn default_assistant_question(missing_fields: &[CaseMissingField]) -> Option<String> {
    match missing_fields {
        [] => None,
        [field] => Some(field.question.clone()),
        fields => {
            let labels = fields
                .iter()
                .map(|field| field.label.as_str())
                .collect::<Vec<_>>()
                .join("、");
            Some(format!("还缺少 {labels}。请按自然语言补充这些信息。"))
        }
    }
}

fn normalize_session(session: &mut CaseImportSession) {
    session.source_text = session.source_text.trim().to_string();
    session.filename = clean_optional_text(session.filename.take());
    normalize_import_draft(&mut session.structured_case);
    session.missing_fields = compute_missing_fields(&session.structured_case);
    session.ready_to_confirm = session.missing_fields.is_empty();
    if !session.ready_to_confirm {
        session.assistant_question = clean_optional_text(session.assistant_question.take())
            .or_else(|| default_assistant_question(&session.missing_fields));
        session.status = CaseImportStatus::NeedsInput;
    } else if session.status != CaseImportStatus::Saved {
        session.assistant_question = None;
        session.status = CaseImportStatus::Ready;
    }
    for message in &mut session.messages {
        message.content = clean_text(&message.content);
    }
    session
        .messages
        .retain(|message| !message.content.trim().is_empty());
}

fn validate_session(session: &CaseImportSession) -> anyhow::Result<()> {
    if session.schema_version != 1 {
        anyhow::bail!("unsupported case import schema version");
    }
    if !valid_draft_id(&session.draft_id) {
        anyhow::bail!("invalid case import draft id");
    }
    if session.source_text.trim().is_empty() {
        anyhow::bail!("case import source text must not be empty");
    }
    if session.status == CaseImportStatus::Saved && session.confirmed_case_id.is_none() {
        anyhow::bail!("saved case import must have confirmed case id");
    }
    Ok(())
}

fn valid_draft_id(draft_id: &str) -> bool {
    draft_id.starts_with("caseimp_")
        && draft_id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'_' || value == b'-')
}

fn is_empty(value: &Option<String>) -> bool {
    value.as_deref().map(str::trim).unwrap_or("").is_empty()
}

fn clean_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn clean_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|value| clean_text(&value))
        .filter(|value| !value.is_empty())
}

fn truncate(value: String, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value
    } else {
        value.chars().take(max_chars).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_required_missing_fields() {
        let draft = CaseImportDraft {
            title: Some("case".to_string()),
            symptom: Some("latency".to_string()),
            ..CaseImportDraft::default()
        };

        let missing = compute_missing_fields(&draft);

        assert_eq!(
            missing
                .into_iter()
                .map(|field| field.field)
                .collect::<Vec<_>>(),
            vec!["rootCause", "solution"]
        );
    }
}
