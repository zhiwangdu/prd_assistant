use std::{collections::HashSet, fs, path::PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::{
    domain::models::{AnalysisResult, TaskRecord},
    stores::memory_store::MemoryStore,
};

#[derive(Debug, Clone)]
pub struct CaseStore {
    legacy_dir: PathBuf,
    memory: MemoryStore,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseRecord {
    pub schema_version: u32,
    pub case_id: String,
    pub source_type: CaseSourceType,
    pub task_id: Option<String>,
    pub product: Option<String>,
    pub version: Option<String>,
    pub environment: Option<String>,
    pub instance_id: Option<String>,
    pub node_id: Option<String>,
    pub title: String,
    pub symptom: String,
    pub root_cause: String,
    pub solution: String,
    pub evidence_refs: Vec<String>,
    pub source_result_path: Option<String>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseSourceType {
    Task,
    Manual,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseSearchHit {
    #[serde(flatten)]
    pub record: CaseRecord,
    pub score: f64,
}

#[derive(Debug, Clone)]
pub struct NewCase {
    pub case_id: String,
    pub task: TaskRecord,
    pub result: AnalysisResult,
    pub source_result_path: String,
    pub product: Option<String>,
    pub version: Option<String>,
    pub environment: Option<String>,
    pub title: Option<String>,
    pub symptom: Option<String>,
    pub root_cause: Option<String>,
    pub solution: Option<String>,
    pub evidence_refs: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct ManualCase {
    pub case_id: String,
    pub product: Option<String>,
    pub version: Option<String>,
    pub environment: Option<String>,
    pub instance_id: Option<String>,
    pub node_id: Option<String>,
    pub title: String,
    pub symptom: String,
    pub root_cause: String,
    pub solution: String,
    pub evidence_refs: Vec<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Default)]
pub struct CaseUpdate {
    pub product: Option<String>,
    pub version: Option<String>,
    pub environment: Option<String>,
    pub instance_id: Option<String>,
    pub node_id: Option<String>,
    pub title: Option<String>,
    pub symptom: Option<String>,
    pub root_cause: Option<String>,
    pub solution: Option<String>,
    pub evidence_refs: Option<Vec<String>>,
    pub enabled: Option<bool>,
}

impl CaseStore {
    #[cfg(test)]
    pub fn load(dir: PathBuf) -> anyhow::Result<Self> {
        let memory_db_path = dir.join("memory.sqlite");
        Self::load_with_memory(dir, memory_db_path)
    }

    pub fn load_with_memory(dir: PathBuf, memory_db_path: PathBuf) -> anyhow::Result<Self> {
        fs::create_dir_all(&dir)?;
        let memory = MemoryStore::load(memory_db_path)?;
        let mut seen = HashSet::new();
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let raw = fs::read_to_string(&path)?;
            let record: CaseRecord = serde_json::from_str(&raw)
                .map_err(|err| anyhow::anyhow!("invalid case record {}: {err}", path.display()))?;
            validate_case(&record)
                .map_err(|err| anyhow::anyhow!("invalid case record {}: {err}", path.display()))?;
            if !seen.insert(record.case_id.clone()) {
                anyhow::bail!("duplicate case record in {}", path.display());
            }
            memory.upsert_case(&record)?;
        }
        Ok(Self {
            legacy_dir: dir,
            memory,
        })
    }

    pub async fn create_or_get_for_task(&self, input: NewCase) -> anyhow::Result<CaseRecord> {
        if let Some(existing) = self.memory.find_task_case(&input.task.task_id)? {
            return Ok(existing);
        }
        let now = Utc::now();
        let mut record = CaseRecord {
            schema_version: 2,
            case_id: input.case_id,
            source_type: CaseSourceType::Task,
            task_id: Some(input.task.task_id.clone()),
            product: input.product,
            version: input.version,
            environment: input.environment,
            instance_id: input.task.instance_id.clone(),
            node_id: input.task.node_id.clone(),
            title: input
                .title
                .unwrap_or_else(|| default_title(&input.task, &input.result)),
            symptom: input
                .symptom
                .unwrap_or_else(|| join_or_summary(&input.result.symptoms, &input.result.summary)),
            root_cause: input.root_cause.unwrap_or_else(|| {
                input
                    .result
                    .likely_root_causes
                    .first()
                    .map(|cause| cause.cause.clone())
                    .unwrap_or_else(|| "当前证据不足以确认根因".to_string())
            }),
            solution: input.solution.unwrap_or_else(|| {
                join_or_summary(&input.result.fix_suggestions, &input.result.summary)
            }),
            evidence_refs: input
                .evidence_refs
                .unwrap_or_else(|| result_evidence_refs(&input.result)),
            source_result_path: Some(input.source_result_path),
            enabled: true,
            created_at: now,
            updated_at: now,
        };
        normalize_case_record(&mut record);
        validate_case(&record)?;
        self.memory.upsert_case(&record)?;
        self.persist_legacy(&record)?;
        Ok(record)
    }

    pub async fn create_manual(&self, input: ManualCase) -> anyhow::Result<CaseRecord> {
        if self.memory.get_case(&input.case_id)?.is_some() {
            anyhow::bail!("duplicate case id");
        }
        let now = Utc::now();
        let mut record = CaseRecord {
            schema_version: 2,
            case_id: input.case_id,
            source_type: CaseSourceType::Manual,
            task_id: None,
            product: input.product,
            version: input.version,
            environment: input.environment,
            instance_id: input.instance_id,
            node_id: input.node_id,
            title: input.title,
            symptom: input.symptom,
            root_cause: input.root_cause,
            solution: input.solution,
            evidence_refs: input.evidence_refs,
            source_result_path: None,
            enabled: input.enabled,
            created_at: now,
            updated_at: now,
        };
        normalize_case_record(&mut record);
        validate_case(&record)?;
        self.memory.upsert_case(&record)?;
        self.persist_legacy(&record)?;
        Ok(record)
    }

    pub async fn get(&self, case_id: &str) -> Option<CaseRecord> {
        self.memory.get_case(case_id).unwrap_or_else(|err| {
            warn!(case_id, error = %err, "failed to read case from memory");
            None
        })
    }

    pub async fn search(
        &self,
        query: Option<&str>,
        limit: usize,
        include_disabled: bool,
    ) -> Vec<CaseSearchHit> {
        self.memory
            .search_cases(query, limit, include_disabled)
            .unwrap_or_else(|err| {
                warn!(error = %err, "failed to search cases from memory");
                Vec::new()
            })
    }

    pub async fn update(&self, case_id: &str, update: CaseUpdate) -> anyhow::Result<CaseRecord> {
        let mut record = self
            .memory
            .get_case(case_id)?
            .ok_or_else(|| anyhow::anyhow!("unknown case {case_id}"))?;
        if let Some(product) = update.product {
            record.product = Some(product);
        }
        if let Some(version) = update.version {
            record.version = Some(version);
        }
        if let Some(environment) = update.environment {
            record.environment = Some(environment);
        }
        if let Some(instance_id) = update.instance_id {
            record.instance_id = Some(instance_id);
        }
        if let Some(node_id) = update.node_id {
            record.node_id = Some(node_id);
        }
        if let Some(title) = update.title {
            record.title = title;
        }
        if let Some(symptom) = update.symptom {
            record.symptom = symptom;
        }
        if let Some(root_cause) = update.root_cause {
            record.root_cause = root_cause;
        }
        if let Some(solution) = update.solution {
            record.solution = solution;
        }
        if let Some(evidence_refs) = update.evidence_refs {
            record.evidence_refs = evidence_refs;
        }
        if let Some(enabled) = update.enabled {
            record.enabled = enabled;
        }
        record.updated_at = Utc::now();
        normalize_case_record(&mut record);
        validate_case(&record)?;
        self.memory.upsert_case(&record)?;
        self.persist_legacy(&record)?;
        Ok(record)
    }

    fn persist_legacy(&self, record: &CaseRecord) -> anyhow::Result<()> {
        let path = self.legacy_dir.join(format!("{}.json", record.case_id));
        let temp = self
            .legacy_dir
            .join(format!(".{}.json.tmp", record.case_id));
        fs::write(&temp, serde_json::to_vec_pretty(record)?)?;
        fs::rename(&temp, &path)?;
        Ok(())
    }
}

fn validate_case(record: &CaseRecord) -> anyhow::Result<()> {
    if record.schema_version != 2 {
        anyhow::bail!("unsupported case schema version");
    }
    if !record.case_id.starts_with("case_") {
        anyhow::bail!("invalid case id");
    }
    match record.source_type {
        CaseSourceType::Task => {
            let task_id = record.task_id.as_deref().unwrap_or("");
            if !task_id.starts_with("task_") {
                anyhow::bail!("invalid task id");
            }
            if record
                .source_result_path
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .is_empty()
            {
                anyhow::bail!("task case source result path must not be empty");
            }
        }
        CaseSourceType::Manual => {
            if record.task_id.is_some() {
                anyhow::bail!("manual case must not have task id");
            }
            if record.source_result_path.is_some() {
                anyhow::bail!("manual case must not have source result path");
            }
        }
    }
    if record.title.trim().is_empty() {
        anyhow::bail!("case title must not be empty");
    }
    if record.symptom.trim().is_empty() {
        anyhow::bail!("case symptom must not be empty");
    }
    if record.root_cause.trim().is_empty() {
        anyhow::bail!("case root cause must not be empty");
    }
    if record.solution.trim().is_empty() {
        anyhow::bail!("case solution must not be empty");
    }
    Ok(())
}

fn normalize_case_record(record: &mut CaseRecord) {
    record.title = truncate(clean_text(&record.title), 160);
    record.symptom = truncate(clean_text(&record.symptom), 4_000);
    record.root_cause = truncate(clean_text(&record.root_cause), 4_000);
    record.solution = truncate(clean_text(&record.solution), 4_000);
    record.product = clean_optional_text(record.product.take());
    record.version = clean_optional_text(record.version.take());
    record.environment = clean_optional_text(record.environment.take());
    record.instance_id = clean_optional_text(record.instance_id.take());
    record.node_id = clean_optional_text(record.node_id.take());
    record.evidence_refs = record
        .evidence_refs
        .iter()
        .map(|value| clean_text(value))
        .filter(|value| !value.is_empty())
        .take(64)
        .collect();
}

fn default_title(task: &TaskRecord, result: &AnalysisResult) -> String {
    let summary = result.summary.trim();
    if !summary.is_empty() {
        truncate(summary.to_string(), 120)
    } else {
        truncate(task.question.clone(), 120)
    }
}

fn join_or_summary(items: &[String], summary: &str) -> String {
    if items.is_empty() {
        summary.to_string()
    } else {
        items.join("\n")
    }
}

fn result_evidence_refs(result: &AnalysisResult) -> Vec<String> {
    let mut refs = Vec::new();
    for cause in &result.likely_root_causes {
        for reference in &cause.evidence_refs {
            if !refs.iter().any(|value| value == reference) {
                refs.push(reference.clone());
            }
        }
    }
    refs
}

pub(crate) fn score_case(record: &CaseRecord, query_tokens: &[String]) -> f64 {
    let text = searchable_text(record);
    let mut hits = 0usize;
    for token in query_tokens {
        if text.contains(token) {
            hits += 1;
        }
    }
    hits as f64 / query_tokens.len().max(1) as f64
}

pub(crate) fn searchable_text(record: &CaseRecord) -> String {
    [
        record.title.as_str(),
        record.symptom.as_str(),
        record.root_cause.as_str(),
        record.solution.as_str(),
        record.product.as_deref().unwrap_or(""),
        record.version.as_deref().unwrap_or(""),
        record.environment.as_deref().unwrap_or(""),
        record.instance_id.as_deref().unwrap_or(""),
        record.node_id.as_deref().unwrap_or(""),
        &record.evidence_refs.join("\n"),
    ]
    .join("\n")
    .to_lowercase()
}

pub(crate) fn query_tokens(query: &str) -> Vec<String> {
    query
        .split(|value: char| value.is_whitespace() || value == ',' || value == ';')
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty())
        .take(16)
        .collect()
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
    use crate::domain::models::{Confidence, TaskSource, TaskStatus};

    #[tokio::test]
    async fn creates_searches_and_updates_case() {
        let dir = std::env::temp_dir().join(format!("logagent-case-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = CaseStore::load(dir.clone()).unwrap();
        let now = Utc::now();
        let task = TaskRecord {
            schema_version: 4,
            task_id: "task_case_test".to_string(),
            alias: None,
            session_id: Some("sess_test".to_string()),
            task_kind: crate::domain::models::TaskKind::LogAnalysis,
            source: TaskSource::Upload,
            upload_ids: vec!["upl_1".to_string()],
            inputs: vec![],
            source_url: None,
            tool_id: None,
            tool_params: serde_json::Value::Null,
            tool_result_path: None,
            instance_id: Some("i-1".to_string()),
            cluster_id: Some("c-1".to_string()),
            node_id: None,
            question: "slow query".to_string(),
            status: TaskStatus::Succeeded,
            phase: None,
            attempts: 1,
            error: None,
            manifest_path: None,
            grep_results_path: None,
            metadata_context_path: None,
            system_context_path: None,
            result_json_path: Some("result.json".to_string()),
            result_markdown_path: Some("result.md".to_string()),
            created_at: now,
            updated_at: now,
        };
        let result = AnalysisResult {
            schema_version: 1,
            summary: "slow query without time filter".to_string(),
            symptoms: vec!["query latency increased".to_string()],
            likely_root_causes: vec![crate::domain::models::RootCause {
                cause: "missing time filter".to_string(),
                evidence_refs: vec!["tool_results/act/result.json#findings/0".to_string()],
            }],
            next_checks: vec![],
            fix_suggestions: vec!["add time range".to_string()],
            missing_information: vec![],
            confidence: Confidence::High,
        };
        let created = store
            .create_or_get_for_task(NewCase {
                case_id: "case_test".to_string(),
                task,
                result,
                source_result_path: "result.json".to_string(),
                product: Some("opengemini".to_string()),
                version: Some("1.3.0".to_string()),
                environment: None,
                title: None,
                symptom: None,
                root_cause: None,
                solution: None,
                evidence_refs: None,
            })
            .await
            .unwrap();
        assert_eq!(created.schema_version, 2);
        assert_eq!(created.source_type, CaseSourceType::Task);
        assert_eq!(created.task_id.as_deref(), Some("task_case_test"));
        assert_eq!(created.source_result_path.as_deref(), Some("result.json"));
        assert_eq!(created.root_cause, "missing time filter");
        let hits = store.search(Some("time filter"), 5, false).await;
        assert_eq!(hits.len(), 1);
        assert!(hits[0].score > 0.0);

        let disabled = store
            .update(
                "case_test",
                CaseUpdate {
                    enabled: Some(false),
                    ..CaseUpdate::default()
                },
            )
            .await
            .unwrap();
        assert!(!disabled.enabled);
        assert!(store.search(Some("time filter"), 5, false).await.is_empty());
        assert_eq!(store.search(Some("time filter"), 5, true).await.len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn creates_manual_case_without_task_source() {
        let dir =
            std::env::temp_dir().join(format!("logagent-case-store-manual-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = CaseStore::load(dir.clone()).unwrap();
        let created = store
            .create_manual(ManualCase {
                case_id: "case_manual_test".to_string(),
                product: Some("opengemini".to_string()),
                version: Some("1.3.0".to_string()),
                environment: Some("prod".to_string()),
                instance_id: Some("inst-1".to_string()),
                node_id: Some("node-1".to_string()),
                title: "manual latency case".to_string(),
                symptom: "write latency increased".to_string(),
                root_cause: "wal disk saturation".to_string(),
                solution: "move shards and expand disk".to_string(),
                evidence_refs: vec!["INC-123".to_string()],
                enabled: true,
            })
            .await
            .unwrap();
        assert_eq!(created.schema_version, 2);
        assert_eq!(created.source_type, CaseSourceType::Manual);
        assert!(created.task_id.is_none());
        assert!(created.source_result_path.is_none());

        let hits = store.search(Some("inst-1 wal"), 5, false).await;
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].record.case_id, "case_manual_test");
        let loaded = CaseStore::load(dir.clone()).unwrap();
        assert!(loaded.get("case_manual_test").await.is_some());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn imports_legacy_json_cases_into_memory_idempotently() {
        let root = std::env::temp_dir().join(format!(
            "logagent-case-store-migration-{}",
            std::process::id()
        ));
        let legacy_dir = root.join("cases");
        let memory_db = root.join("memory/memory.sqlite");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&legacy_dir).unwrap();
        let now = Utc::now();
        let record = CaseRecord {
            schema_version: 2,
            case_id: "case_legacy_test".to_string(),
            source_type: CaseSourceType::Manual,
            task_id: None,
            product: Some("opengemini".to_string()),
            version: Some("1.3.0".to_string()),
            environment: Some("prod".to_string()),
            instance_id: Some("inst-legacy".to_string()),
            node_id: None,
            title: "legacy timeout case".to_string(),
            symptom: "query timeout".to_string(),
            root_cause: "coordinator overloaded".to_string(),
            solution: "rebalance partitions".to_string(),
            evidence_refs: vec!["INC-legacy".to_string()],
            source_result_path: None,
            enabled: true,
            created_at: now,
            updated_at: now,
        };
        std::fs::write(
            legacy_dir.join("case_legacy_test.json"),
            serde_json::to_vec_pretty(&record).unwrap(),
        )
        .unwrap();

        let store = CaseStore::load_with_memory(legacy_dir.clone(), memory_db.clone()).unwrap();
        assert_eq!(
            store
                .search(Some("coordinator timeout"), 10, false)
                .await
                .len(),
            1
        );
        assert!(legacy_dir.join("case_legacy_test.json").exists());

        let reloaded = CaseStore::load_with_memory(legacy_dir.clone(), memory_db).unwrap();
        let hits = reloaded
            .search(Some("coordinator timeout"), 10, false)
            .await;
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].record.case_id, "case_legacy_test");
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn fts_search_handles_hyphenated_queries() {
        let dir =
            std::env::temp_dir().join(format!("logagent-case-store-fts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = CaseStore::load(dir.clone()).unwrap();
        store
            .create_manual(ManualCase {
                case_id: "case_fts_test".to_string(),
                product: Some("opengemini".to_string()),
                version: None,
                environment: None,
                instance_id: None,
                node_id: None,
                title: "WAL disk saturation".to_string(),
                symptom: "writes are slow".to_string(),
                root_cause: "wal disk saturation".to_string(),
                solution: "move shards and expand disk".to_string(),
                evidence_refs: vec![],
                enabled: true,
            })
            .await
            .unwrap();

        assert_eq!(
            score_case(
                &store.get("case_fts_test").await.unwrap(),
                &query_tokens("wal-disk")
            ),
            0.0
        );
        let hits = store.search(Some("wal-disk"), 5, false).await;
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].record.case_id, "case_fts_test");
        assert!(hits[0].score > 0.0);
        let _ = std::fs::remove_dir_all(dir);
    }
}
