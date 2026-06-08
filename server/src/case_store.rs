use std::{collections::HashMap, fs, path::PathBuf, sync::Arc};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::models::{AnalysisResult, TaskRecord};

#[derive(Debug, Clone)]
pub struct CaseStore {
    dir: PathBuf,
    inner: Arc<RwLock<HashMap<String, CaseRecord>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseRecord {
    pub schema_version: u32,
    pub case_id: String,
    pub task_id: String,
    pub product: Option<String>,
    pub version: Option<String>,
    pub environment: Option<String>,
    pub instance_id: Option<String>,
    pub cluster_id: Option<String>,
    pub node_id: Option<String>,
    pub title: String,
    pub symptom: String,
    pub root_cause: String,
    pub solution: String,
    pub evidence_refs: Vec<String>,
    pub source_result_path: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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

#[derive(Debug, Clone, Default)]
pub struct CaseUpdate {
    pub title: Option<String>,
    pub symptom: Option<String>,
    pub root_cause: Option<String>,
    pub solution: Option<String>,
    pub evidence_refs: Option<Vec<String>>,
    pub enabled: Option<bool>,
}

impl CaseStore {
    pub fn load(dir: PathBuf) -> anyhow::Result<Self> {
        fs::create_dir_all(&dir)?;
        let mut cases = HashMap::new();
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
            if cases.insert(record.case_id.clone(), record).is_some() {
                anyhow::bail!("duplicate case record in {}", path.display());
            }
        }
        Ok(Self {
            dir,
            inner: Arc::new(RwLock::new(cases)),
        })
    }

    pub async fn create_or_get_for_task(&self, input: NewCase) -> anyhow::Result<CaseRecord> {
        let mut cases = self.inner.write().await;
        if let Some(existing) = cases
            .values()
            .find(|record| record.task_id == input.task.task_id)
            .cloned()
        {
            return Ok(existing);
        }
        let now = Utc::now();
        let mut record = CaseRecord {
            schema_version: 1,
            case_id: input.case_id,
            task_id: input.task.task_id.clone(),
            product: input.product,
            version: input.version,
            environment: input.environment,
            instance_id: input.task.instance_id.clone(),
            cluster_id: input.task.cluster_id.clone(),
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
            source_result_path: input.source_result_path,
            enabled: true,
            created_at: now,
            updated_at: now,
        };
        normalize_case_record(&mut record);
        validate_case(&record)?;
        self.persist(&record)?;
        cases.insert(record.case_id.clone(), record.clone());
        Ok(record)
    }

    pub async fn get(&self, case_id: &str) -> Option<CaseRecord> {
        self.inner.read().await.get(case_id).cloned()
    }

    pub async fn search(
        &self,
        query: Option<&str>,
        limit: usize,
        include_disabled: bool,
    ) -> Vec<CaseSearchHit> {
        let query_tokens = query_tokens(query.unwrap_or(""));
        let mut hits = self
            .inner
            .read()
            .await
            .values()
            .filter(|record| include_disabled || record.enabled)
            .filter_map(|record| {
                let score = if query_tokens.is_empty() {
                    1.0
                } else {
                    score_case(record, &query_tokens)
                };
                if score > 0.0 {
                    Some(CaseSearchHit {
                        record: record.clone(),
                        score,
                    })
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        hits.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| right.record.created_at.cmp(&left.record.created_at))
        });
        hits.truncate(limit.max(1));
        hits
    }

    pub async fn update(&self, case_id: &str, update: CaseUpdate) -> anyhow::Result<CaseRecord> {
        let mut cases = self.inner.write().await;
        let mut record = cases
            .get(case_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown case {case_id}"))?;
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
        self.persist(&record)?;
        cases.insert(case_id.to_string(), record.clone());
        Ok(record)
    }

    fn persist(&self, record: &CaseRecord) -> anyhow::Result<()> {
        let path = self.dir.join(format!("{}.json", record.case_id));
        let temp = self.dir.join(format!(".{}.json.tmp", record.case_id));
        fs::write(&temp, serde_json::to_vec_pretty(record)?)?;
        fs::rename(&temp, &path)?;
        Ok(())
    }
}

fn validate_case(record: &CaseRecord) -> anyhow::Result<()> {
    if !record.case_id.starts_with("case_") {
        anyhow::bail!("invalid case id");
    }
    if !record.task_id.starts_with("task_") {
        anyhow::bail!("invalid task id");
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

fn score_case(record: &CaseRecord, query_tokens: &[String]) -> f64 {
    let text = searchable_text(record);
    let mut hits = 0usize;
    for token in query_tokens {
        if text.contains(token) {
            hits += 1;
        }
    }
    hits as f64 / query_tokens.len().max(1) as f64
}

fn searchable_text(record: &CaseRecord) -> String {
    [
        record.title.as_str(),
        record.symptom.as_str(),
        record.root_cause.as_str(),
        record.solution.as_str(),
        record.product.as_deref().unwrap_or(""),
        record.version.as_deref().unwrap_or(""),
        record.environment.as_deref().unwrap_or(""),
    ]
    .join("\n")
    .to_lowercase()
}

fn query_tokens(query: &str) -> Vec<String> {
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
    use crate::models::{Confidence, TaskSource, TaskStatus};

    #[tokio::test]
    async fn creates_searches_and_updates_case() {
        let dir = std::env::temp_dir().join(format!("logagent-case-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = CaseStore::load(dir.clone()).unwrap();
        let now = Utc::now();
        let task = TaskRecord {
            schema_version: 4,
            task_id: "task_case_test".to_string(),
            source: TaskSource::Upload,
            upload_ids: vec!["upl_1".to_string()],
            inputs: vec![],
            source_url: None,
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
            result_json_path: Some("result.json".to_string()),
            result_markdown_path: Some("result.md".to_string()),
            created_at: now,
            updated_at: now,
        };
        let result = AnalysisResult {
            schema_version: 1,
            summary: "slow query without time filter".to_string(),
            symptoms: vec!["query latency increased".to_string()],
            likely_root_causes: vec![crate::models::RootCause {
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
}
