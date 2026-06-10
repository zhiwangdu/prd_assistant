use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use rusqlite::{params, Connection, OptionalExtension};
use tracing::warn;

use crate::stores::case_store::{
    query_tokens, score_case, searchable_text, CaseRecord, CaseSearchHit,
};

#[derive(Clone)]
pub struct MemoryStore {
    db_path: PathBuf,
    conn: Arc<Mutex<Connection>>,
    fts_enabled: bool,
}

impl std::fmt::Debug for MemoryStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MemoryStore")
            .field("db_path", &self.db_path)
            .field("fts_enabled", &self.fts_enabled)
            .finish()
    }
}

impl MemoryStore {
    pub fn load(db_path: PathBuf) -> anyhow::Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(&db_path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        create_schema(&conn)?;
        let fts_enabled = create_fts_schema(&conn).unwrap_or_else(|err| {
            warn!(
                path = %db_path.display(),
                error = %err,
                "memory FTS index unavailable; falling back to token overlap search"
            );
            false
        });
        Ok(Self {
            db_path,
            conn: Arc::new(Mutex::new(conn)),
            fts_enabled,
        })
    }

    pub fn upsert_case(&self, record: &CaseRecord) -> anyhow::Result<()> {
        let mut conn = self.lock_conn()?;
        let tx = conn.transaction()?;
        upsert_case_tx(&tx, record, self.fts_enabled)?;
        tx.commit()?;
        Ok(())
    }

    pub fn get_case(&self, case_id: &str) -> anyhow::Result<Option<CaseRecord>> {
        let conn = self.lock_conn()?;
        let raw = conn
            .query_row(
                "SELECT record_json FROM memory_items WHERE memory_type = 'case' AND memory_id = ?1",
                params![case_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        raw.map(|value| parse_case_json(case_id, &value))
            .transpose()
    }

    pub fn find_task_case(&self, task_id: &str) -> anyhow::Result<Option<CaseRecord>> {
        let conn = self.lock_conn()?;
        let raw = conn
            .query_row(
                "SELECT record_json FROM memory_items
                 WHERE memory_type = 'case' AND source_id = ?1
                 ORDER BY created_at ASC
                 LIMIT 1",
                params![task_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        raw.map(|value| parse_case_json(task_id, &value))
            .transpose()
    }

    pub fn search_cases(
        &self,
        query: Option<&str>,
        limit: usize,
        include_disabled: bool,
    ) -> anyhow::Result<Vec<CaseSearchHit>> {
        let query_text = query.unwrap_or("").trim();
        let tokens = query_tokens(query_text);
        let fts_scores = if tokens.is_empty() || !self.fts_enabled {
            HashMap::new()
        } else {
            self.fts_scores(&tokens).unwrap_or_else(|err| {
                warn!(error = %err, "memory FTS query failed; falling back to token overlap search");
                HashMap::new()
            })
        };
        let records = self.list_case_records(include_disabled)?;
        let mut hits = records
            .into_iter()
            .filter_map(|record| {
                let token_score = if tokens.is_empty() {
                    1.0
                } else {
                    score_case(&record, &tokens)
                };
                let fts_score = fts_scores.get(&record.case_id).copied().unwrap_or(0.0);
                let score = token_score + fts_score;
                if score > 0.0 {
                    Some(CaseSearchHit { record, score })
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
        Ok(hits)
    }

    fn list_case_records(&self, include_disabled: bool) -> anyhow::Result<Vec<CaseRecord>> {
        let conn = self.lock_conn()?;
        let sql = if include_disabled {
            "SELECT memory_id, record_json FROM memory_items
             WHERE memory_type = 'case' AND status = 'active'
             ORDER BY created_at DESC"
        } else {
            "SELECT memory_id, record_json FROM memory_items
             WHERE memory_type = 'case' AND status = 'active' AND enabled = 1
             ORDER BY created_at DESC"
        };
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut records = Vec::new();
        for row in rows {
            let (id, raw) = row?;
            records.push(parse_case_json(&id, &raw)?);
        }
        Ok(records)
    }

    fn fts_scores(&self, tokens: &[String]) -> anyhow::Result<HashMap<String, f64>> {
        let Some(query) = fts_query(tokens) else {
            return Ok(HashMap::new());
        };
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT memory_chunks_fts.item_id, bm25(memory_chunks_fts) AS rank
             FROM memory_chunks_fts
             JOIN memory_items i ON i.memory_id = memory_chunks_fts.item_id
             WHERE memory_chunks_fts MATCH ?1
               AND i.memory_type = 'case'
               AND i.status = 'active'",
        )?;
        let rows = stmt.query_map(params![query], |row| {
            let id: String = row.get(0)?;
            let rank: f64 = row.get(1)?;
            Ok((id, 1.0 / (1.0 + rank.abs())))
        })?;
        let mut scores = HashMap::new();
        for row in rows {
            let (id, score) = row?;
            scores
                .entry(id)
                .and_modify(|existing| {
                    if score > *existing {
                        *existing = score;
                    }
                })
                .or_insert(score);
        }
        Ok(scores)
    }

    fn lock_conn(&self) -> anyhow::Result<std::sync::MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|err| anyhow::anyhow!("memory sqlite lock poisoned: {err}"))
    }
}

fn create_schema(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS memory_items (
            memory_id TEXT PRIMARY KEY,
            memory_type TEXT NOT NULL,
            status TEXT NOT NULL,
            enabled INTEGER NOT NULL,
            source_id TEXT,
            record_json TEXT NOT NULL,
            searchable_text TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_memory_items_type_status_enabled
            ON memory_items(memory_type, status, enabled);
        CREATE INDEX IF NOT EXISTS idx_memory_items_source_id
            ON memory_items(memory_type, source_id);
        CREATE TABLE IF NOT EXISTS memory_chunks (
            chunk_id TEXT PRIMARY KEY,
            item_id TEXT NOT NULL,
            chunk_index INTEGER NOT NULL,
            content TEXT NOT NULL,
            FOREIGN KEY(item_id) REFERENCES memory_items(memory_id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_memory_chunks_item_id
            ON memory_chunks(item_id);
        "#,
    )?;
    Ok(())
}

fn create_fts_schema(conn: &Connection) -> anyhow::Result<bool> {
    conn.execute_batch(
        r#"
        CREATE VIRTUAL TABLE IF NOT EXISTS memory_chunks_fts
        USING fts5(item_id UNINDEXED, chunk_id UNINDEXED, content, tokenize = 'unicode61');
        "#,
    )?;
    Ok(true)
}

fn upsert_case_tx(
    tx: &rusqlite::Transaction<'_>,
    record: &CaseRecord,
    fts_enabled: bool,
) -> anyhow::Result<()> {
    let record_json = serde_json::to_string(record)?;
    let text = searchable_text(record);
    let source_id = record.task_id.as_deref();
    tx.execute(
        "INSERT INTO memory_items (
             memory_id, memory_type, status, enabled, source_id, record_json, searchable_text, created_at, updated_at
         ) VALUES (?1, 'case', 'active', ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(memory_id) DO UPDATE SET
             memory_type = excluded.memory_type,
             status = excluded.status,
             enabled = excluded.enabled,
             source_id = excluded.source_id,
             record_json = excluded.record_json,
             searchable_text = excluded.searchable_text,
             created_at = excluded.created_at,
             updated_at = excluded.updated_at",
        params![
            record.case_id,
            record.enabled as i64,
            source_id,
            record_json,
            text,
            record.created_at.to_rfc3339(),
            record.updated_at.to_rfc3339(),
        ],
    )?;
    tx.execute(
        "DELETE FROM memory_chunks WHERE item_id = ?1",
        params![record.case_id],
    )?;
    if fts_enabled {
        tx.execute(
            "DELETE FROM memory_chunks_fts WHERE item_id = ?1",
            params![record.case_id],
        )?;
    }
    let chunk_id = format!("{}:case:0", record.case_id);
    tx.execute(
        "INSERT INTO memory_chunks (chunk_id, item_id, chunk_index, content)
         VALUES (?1, ?2, 0, ?3)",
        params![chunk_id, record.case_id, text],
    )?;
    if fts_enabled {
        tx.execute(
            "INSERT INTO memory_chunks_fts (item_id, chunk_id, content)
             VALUES (?1, ?2, ?3)",
            params![record.case_id, chunk_id, text],
        )?;
    }
    Ok(())
}

fn parse_case_json(id: &str, raw: &str) -> anyhow::Result<CaseRecord> {
    serde_json::from_str(raw)
        .map_err(|err| anyhow::anyhow!("invalid memory case record {id}: {err}"))
}

fn fts_query(tokens: &[String]) -> Option<String> {
    let parts = tokens
        .iter()
        .flat_map(|token| split_fts_terms(token))
        .map(|term| format!("{term}*"))
        .collect::<Vec<_>>();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" OR "))
    }
}

fn split_fts_terms(token: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut current = String::new();
    for ch in token.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            current.push(ch.to_ascii_lowercase());
        } else if !current.is_empty() {
            terms.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        terms.push(current);
    }
    terms
}
