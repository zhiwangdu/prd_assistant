from __future__ import annotations

import json
import math
import re
import sqlite3
from contextlib import contextmanager
from datetime import UTC, datetime, timedelta
from hashlib import sha256
from pathlib import Path
from typing import Any, Iterator

from .ids import new_id


JsonObject = dict[str, Any]
CASE_VECTOR_DIMS = 64


def now_iso() -> str:
    return datetime.now(UTC).replace(microsecond=0).isoformat()


def encode_json(value: Any) -> str:
    return json.dumps(value, ensure_ascii=True, separators=(",", ":"))


def decode_json(value: str | None, default: Any = None) -> Any:
    if value is None:
        return default
    return json.loads(value)


def case_tokens(value: str) -> set[str]:
    return {
        token.lower()
        for token in re.findall(r"[A-Za-z0-9_\u4e00-\u9fff]{2,}", value)
        if token.strip()
    }


def case_searchable_text(record: JsonObject) -> str:
    values = [
        record.get("title"),
        record.get("symptom"),
        record.get("rootCause"),
        record.get("solution"),
        record.get("product"),
        record.get("version"),
        record.get("environment"),
        record.get("instanceId"),
        record.get("nodeId"),
        " ".join(record.get("evidenceRefs", [])),
    ]
    return "\n".join(str(value) for value in values if value)


def keyword_score(tokens: set[str], text: str) -> int:
    if not tokens:
        return 0
    lowered = text.lower()
    return sum(1 for token in tokens if token in lowered)


def case_vector_tokens(value: str) -> list[str]:
    tokens = list(case_tokens(value))
    grams = []
    for token in tokens:
        if len(token) <= 3:
            grams.append(token)
            continue
        grams.extend(token[index : index + 3] for index in range(0, len(token) - 2))
    return tokens + grams


def case_vector(value: str) -> list[float]:
    vector = [0.0 for _ in range(CASE_VECTOR_DIMS)]
    for token in case_vector_tokens(value):
        digest = sha256(token.encode("utf-8")).digest()
        index = int.from_bytes(digest[:4], "big") % CASE_VECTOR_DIMS
        sign = 1.0 if digest[4] % 2 == 0 else -1.0
        vector[index] += sign
    norm = math.sqrt(sum(item * item for item in vector))
    if norm == 0:
        return vector
    return [round(item / norm, 6) for item in vector]


def vector_similarity(left: list[float], right: list[float]) -> float:
    if not left or not right or len(left) != len(right):
        return 0.0
    return sum(a * b for a, b in zip(left, right))


def quote_fts_token(token: str) -> str:
    return '"' + token.replace('"', '""') + '"'


class Store:
    def __init__(self, sqlite_path: Path):
        self.sqlite_path = sqlite_path
        self.sqlite_path.parent.mkdir(parents=True, exist_ok=True)

    @contextmanager
    def connect(self) -> Iterator[sqlite3.Connection]:
        conn = sqlite3.connect(self.sqlite_path)
        conn.row_factory = sqlite3.Row
        conn.execute("PRAGMA foreign_keys = ON")
        conn.execute("PRAGMA journal_mode = WAL")
        try:
            yield conn
            conn.commit()
        except Exception:
            conn.rollback()
            raise
        finally:
            conn.close()

    def initialize(self) -> None:
        with self.connect() as conn:
            conn.executescript(
                """
                CREATE TABLE IF NOT EXISTS workspaces (
                  id TEXT PRIMARY KEY,
                  question TEXT NOT NULL,
                  mode TEXT NOT NULL,
                  language TEXT NOT NULL,
                  status TEXT NOT NULL,
                  skill_ids_json TEXT NOT NULL DEFAULT '[]',
                  created_at TEXT NOT NULL,
                  updated_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS runs (
                  id TEXT PRIMARY KEY,
                  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
                  status TEXT NOT NULL,
                  phase TEXT NOT NULL,
                  budget_json TEXT NOT NULL,
                  final_answer_json TEXT,
                  created_at TEXT NOT NULL,
                  updated_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS timeline_events (
                  id TEXT PRIMARY KEY,
                  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
                  run_id TEXT REFERENCES runs(id) ON DELETE CASCADE,
                  kind TEXT NOT NULL,
                  payload_json TEXT NOT NULL,
                  created_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS artifacts (
                  id TEXT PRIMARY KEY,
                  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
                  relative_path TEXT NOT NULL,
                  sha256 TEXT NOT NULL,
                  size_bytes INTEGER NOT NULL,
                  content_type TEXT NOT NULL,
                  schema_name TEXT,
                  preview_json TEXT NOT NULL,
                  created_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS uploads (
                  id TEXT PRIMARY KEY,
                  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
                  filename TEXT NOT NULL,
                  artifact_id TEXT NOT NULL REFERENCES artifacts(id),
                  created_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS upload_sessions (
                  id TEXT PRIMARY KEY,
                  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
                  filename TEXT NOT NULL,
                  content_type TEXT NOT NULL,
                  expected_size_bytes INTEGER,
                  received_bytes INTEGER NOT NULL,
                  temp_relative_path TEXT NOT NULL,
                  status TEXT NOT NULL,
                  upload_id TEXT REFERENCES uploads(id),
                  artifact_id TEXT REFERENCES artifacts(id),
                  created_at TEXT NOT NULL,
                  updated_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS evidence_items (
                  id TEXT PRIMARY KEY,
                  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
                  run_id TEXT REFERENCES runs(id) ON DELETE CASCADE,
                  kind TEXT NOT NULL,
                  final_allowed INTEGER NOT NULL,
                  summary TEXT NOT NULL,
                  artifact_id TEXT REFERENCES artifacts(id),
                  payload_json TEXT NOT NULL,
                  created_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS actions (
                  id TEXT PRIMARY KEY,
                  run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
                  kind TEXT NOT NULL,
                  status TEXT NOT NULL,
                  payload_json TEXT NOT NULL,
                  result_json TEXT,
                  created_at TEXT NOT NULL,
                  updated_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS jobs (
                  id TEXT PRIMARY KEY,
                  kind TEXT NOT NULL,
                  status TEXT NOT NULL,
                  payload_json TEXT NOT NULL,
                  locked_by TEXT,
                  locked_until TEXT,
                  attempts INTEGER NOT NULL,
                  max_attempts INTEGER NOT NULL,
                  next_run_at TEXT NOT NULL,
                  last_error TEXT,
                  created_at TEXT NOT NULL,
                  updated_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS metadata_instances (
                  instance_id TEXT PRIMARY KEY,
                  remark TEXT,
                  template_type TEXT NOT NULL,
                  snapshot_json TEXT NOT NULL,
                  raw_json TEXT NOT NULL,
                  created_at TEXT NOT NULL,
                  updated_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS metadata_imports (
                  import_id TEXT PRIMARY KEY,
                  instance_id TEXT NOT NULL,
                  remark TEXT,
                  template_type TEXT NOT NULL,
                  source_url TEXT,
                  status TEXT NOT NULL,
                  snapshot_json TEXT NOT NULL,
                  raw_json TEXT NOT NULL,
                  created_at TEXT NOT NULL,
                  updated_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS cases (
                  case_id TEXT PRIMARY KEY,
                  source_type TEXT NOT NULL,
                  task_id TEXT,
                  enabled INTEGER NOT NULL,
                  record_json TEXT NOT NULL,
                  searchable_text TEXT NOT NULL,
                  vector_json TEXT NOT NULL DEFAULT '[]',
                  created_at TEXT NOT NULL,
                  updated_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS case_imports (
                  import_id TEXT PRIMARY KEY,
                  status TEXT NOT NULL,
                  filename TEXT,
                  source_text TEXT NOT NULL,
                  draft_json TEXT NOT NULL,
                  validation_errors_json TEXT NOT NULL,
                  case_id TEXT,
                  created_at TEXT NOT NULL,
                  updated_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS fetch_endpoints (
                  id TEXT PRIMARY KEY,
                  name TEXT NOT NULL,
                  method TEXT NOT NULL,
                  url TEXT NOT NULL,
                  headers_json TEXT NOT NULL,
                  body TEXT,
                  enabled INTEGER NOT NULL,
                  created_at TEXT NOT NULL,
                  updated_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS fetch_credential_sets (
                  id TEXT PRIMARY KEY,
                  endpoint_id TEXT NOT NULL UNIQUE REFERENCES fetch_endpoints(id) ON DELETE CASCADE,
                  encrypted_json TEXT NOT NULL,
                  redacted_json TEXT NOT NULL,
                  created_at TEXT NOT NULL,
                  updated_at TEXT NOT NULL
                );

                CREATE INDEX IF NOT EXISTS idx_runs_workspace_id ON runs(workspace_id);
                CREATE INDEX IF NOT EXISTS idx_events_workspace_run
                  ON timeline_events(workspace_id, run_id, created_at);
                CREATE INDEX IF NOT EXISTS idx_jobs_sched
                  ON jobs(status, next_run_at, locked_until);
                CREATE INDEX IF NOT EXISTS idx_metadata_imports_status
                  ON metadata_imports(status, updated_at);
                CREATE INDEX IF NOT EXISTS idx_cases_task_id ON cases(task_id);
                CREATE INDEX IF NOT EXISTS idx_case_imports_status
                  ON case_imports(status, updated_at);
                CREATE INDEX IF NOT EXISTS idx_fetch_endpoints_enabled ON fetch_endpoints(enabled);
                CREATE INDEX IF NOT EXISTS idx_fetch_credentials_endpoint
                  ON fetch_credential_sets(endpoint_id);
                """
            )
            self._ensure_column_tx(
                conn, "workspaces", "skill_ids_json", "TEXT NOT NULL DEFAULT '[]'"
            )
            self._ensure_column_tx(conn, "metadata_imports", "source_url", "TEXT")
            self._ensure_column_tx(conn, "cases", "vector_json", "TEXT NOT NULL DEFAULT '[]'")
            self._ensure_case_fts_tx(conn)
            self._backfill_case_vectors_tx(conn)

    def _ensure_column_tx(
        self, conn: sqlite3.Connection, table: str, column: str, definition: str
    ) -> None:
        columns = {
            row["name"] for row in conn.execute(f"PRAGMA table_info({table})").fetchall()
        }
        if column not in columns:
            conn.execute(f"ALTER TABLE {table} ADD COLUMN {column} {definition}")

    def _ensure_case_fts_tx(self, conn: sqlite3.Connection) -> None:
        try:
            conn.execute(
                """
                CREATE VIRTUAL TABLE IF NOT EXISTS cases_fts
                USING fts5(case_id UNINDEXED, searchable_text, tokenize='unicode61')
                """
            )
            conn.execute(
                """
                INSERT INTO cases_fts(rowid, case_id, searchable_text)
                SELECT c.rowid, c.case_id, c.searchable_text
                FROM cases c
                LEFT JOIN cases_fts f ON f.rowid = c.rowid
                WHERE f.rowid IS NULL
                """
            )
        except sqlite3.OperationalError:
            return

    def _upsert_case_fts_tx(
        self, conn: sqlite3.Connection, case_id: str, searchable_text: str
    ) -> None:
        try:
            row = conn.execute(
                "SELECT rowid FROM cases WHERE case_id = ?", (case_id,)
            ).fetchone()
            if row is None:
                return
            conn.execute("DELETE FROM cases_fts WHERE rowid = ?", (row["rowid"],))
            conn.execute(
                """
                INSERT INTO cases_fts(rowid, case_id, searchable_text)
                VALUES (?, ?, ?)
                """,
                (row["rowid"], case_id, searchable_text),
            )
        except sqlite3.OperationalError:
            return

    def _backfill_case_vectors_tx(self, conn: sqlite3.Connection) -> None:
        rows = conn.execute(
            """
            SELECT case_id, searchable_text, vector_json
            FROM cases
            WHERE vector_json IS NULL OR vector_json = '[]'
            """
        ).fetchall()
        for row in rows:
            conn.execute(
                "UPDATE cases SET vector_json = ? WHERE case_id = ?",
                (encode_json(case_vector(row["searchable_text"])), row["case_id"]),
            )

    def _workspace_from_row(self, row: sqlite3.Row) -> JsonObject:
        item = dict(row)
        item["skillIds"] = decode_json(item.pop("skill_ids_json", None), [])
        return item

    def create_workspace(
        self, question: str, mode: str, language: str, skill_ids: list[str] | None = None
    ) -> JsonObject:
        workspace_id = new_id("ws")
        ts = now_iso()
        skill_ids = skill_ids or []
        with self.connect() as conn:
            conn.execute(
                """
                INSERT INTO workspaces(
                  id, question, mode, language, status, skill_ids_json, created_at, updated_at
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (workspace_id, question, mode, language, "active", encode_json(skill_ids), ts, ts),
            )
            self._append_event_tx(
                conn,
                workspace_id,
                None,
                "workspace.created",
                {
                    "question": question,
                    "mode": mode,
                    "language": language,
                    "skillIds": skill_ids,
                },
                ts,
            )
        return self.get_workspace(workspace_id)

    def list_workspaces(self) -> list[JsonObject]:
        with self.connect() as conn:
            rows = conn.execute(
                "SELECT * FROM workspaces ORDER BY created_at DESC, id DESC"
            ).fetchall()
        return [self._workspace_from_row(row) for row in rows]

    def get_workspace(self, workspace_id: str) -> JsonObject:
        with self.connect() as conn:
            row = conn.execute("SELECT * FROM workspaces WHERE id = ?", (workspace_id,)).fetchone()
        if row is None:
            raise KeyError(f"unknown workspace {workspace_id}")
        return self._workspace_from_row(row)

    def list_uploads(self, workspace_id: str) -> list[JsonObject]:
        with self.connect() as conn:
            rows = conn.execute(
                """
                SELECT
                  uploads.id AS id,
                  uploads.workspace_id AS workspace_id,
                  uploads.filename AS filename,
                  uploads.artifact_id AS artifact_id,
                  uploads.created_at AS created_at,
                  artifacts.relative_path AS artifact_relative_path,
                  artifacts.sha256 AS artifact_sha256,
                  artifacts.size_bytes AS artifact_size_bytes,
                  artifacts.content_type AS artifact_content_type
                FROM uploads
                JOIN artifacts ON artifacts.id = uploads.artifact_id
                WHERE uploads.workspace_id = ?
                ORDER BY uploads.created_at ASC, uploads.id ASC
                """,
                (workspace_id,),
            ).fetchall()
        return [dict(row) for row in rows]

    def list_upload_sessions(self, workspace_id: str | None = None) -> list[JsonObject]:
        with self.connect() as conn:
            if workspace_id is None:
                rows = conn.execute(
                    """
                    SELECT * FROM upload_sessions
                    ORDER BY created_at DESC, rowid DESC
                    """
                ).fetchall()
            else:
                self.get_workspace(workspace_id)
                rows = conn.execute(
                    """
                    SELECT * FROM upload_sessions
                    WHERE workspace_id = ?
                    ORDER BY created_at DESC, rowid DESC
                    """,
                    (workspace_id,),
                ).fetchall()
        return [dict(row) for row in rows]

    def create_run(self, workspace_id: str) -> JsonObject:
        workspace = self.get_workspace(workspace_id)
        run_id = new_id("run")
        job_id = new_id("job")
        ts = now_iso()
        budget = {"rounds": 0, "llmCalls": 0, "toolCalls": 0}
        with self.connect() as conn:
            conn.execute(
                """
                INSERT INTO runs(id, workspace_id, status, phase, budget_json, created_at, updated_at)
                VALUES (?, ?, ?, ?, ?, ?, ?)
                """,
                (run_id, workspace_id, "queued", "queued", encode_json(budget), ts, ts),
            )
            self._append_event_tx(
                conn,
                workspace_id,
                run_id,
                "run.queued",
                {"question": workspace["question"], "mode": workspace["mode"]},
                ts,
            )
            self._enqueue_run_tx(conn, job_id, workspace_id, run_id, ts)
        return self.get_run(run_id)

    def list_runs(self, workspace_id: str | None = None) -> list[JsonObject]:
        with self.connect() as conn:
            if workspace_id is None:
                rows = conn.execute(
                    "SELECT * FROM runs ORDER BY created_at DESC, rowid DESC"
                ).fetchall()
            else:
                self.get_workspace(workspace_id)
                rows = conn.execute(
                    """
                    SELECT * FROM runs
                    WHERE workspace_id = ?
                    ORDER BY created_at DESC, rowid DESC
                    """,
                    (workspace_id,),
                ).fetchall()
        result = []
        for row in rows:
            item = dict(row)
            item["budget"] = decode_json(item.pop("budget_json"), {})
            item["finalAnswer"] = decode_json(item.pop("final_answer_json"), None)
            result.append(item)
        return result

    def enqueue_run(self, run_id: str) -> JsonObject:
        run = self.get_run(run_id)
        job_id = new_id("job")
        ts = now_iso()
        with self.connect() as conn:
            self._enqueue_run_tx(conn, job_id, run["workspace_id"], run_id, ts)
            self._append_event_tx(conn, run["workspace_id"], run_id, "run.requeued", {}, ts)
        return {"jobId": job_id, "runId": run_id}

    def get_run(self, run_id: str) -> JsonObject:
        with self.connect() as conn:
            row = conn.execute("SELECT * FROM runs WHERE id = ?", (run_id,)).fetchone()
        if row is None:
            raise KeyError(f"unknown run {run_id}")
        result = dict(row)
        result["budget"] = decode_json(result.pop("budget_json"), {})
        result["finalAnswer"] = decode_json(result.pop("final_answer_json"), None)
        return result

    def update_run_status(
        self, run_id: str, status: str, phase: str, final_answer: JsonObject | None = None
    ) -> JsonObject:
        ts = now_iso()
        with self.connect() as conn:
            row = conn.execute("SELECT workspace_id FROM runs WHERE id = ?", (run_id,)).fetchone()
            if row is None:
                raise KeyError(f"unknown run {run_id}")
            conn.execute(
                """
                UPDATE runs
                SET status = ?, phase = ?, final_answer_json = ?, updated_at = ?
                WHERE id = ?
                """,
                (
                    status,
                    phase,
                    encode_json(final_answer) if final_answer is not None else None,
                    ts,
                    run_id,
                ),
            )
            self._append_event_tx(
                conn,
                row["workspace_id"],
                run_id,
                f"run.{status}",
                {"phase": phase},
                ts,
            )
        return self.get_run(run_id)

    def create_artifact(
        self,
        workspace_id: str,
        relative_path: str,
        sha256: str,
        size_bytes: int,
        content_type: str,
        schema_name: str | None,
        preview: JsonObject,
    ) -> JsonObject:
        artifact_id = new_id("art")
        ts = now_iso()
        with self.connect() as conn:
            conn.execute(
                """
                INSERT INTO artifacts(
                  id, workspace_id, relative_path, sha256, size_bytes, content_type,
                  schema_name, preview_json, created_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    artifact_id,
                    workspace_id,
                    relative_path,
                    sha256,
                    size_bytes,
                    content_type,
                    schema_name,
                    encode_json(preview),
                    ts,
                ),
            )
        return self.get_artifact(artifact_id)

    def get_artifact(self, artifact_id: str) -> JsonObject:
        with self.connect() as conn:
            row = conn.execute("SELECT * FROM artifacts WHERE id = ?", (artifact_id,)).fetchone()
        if row is None:
            raise KeyError(f"unknown artifact {artifact_id}")
        result = dict(row)
        result["preview"] = decode_json(result.pop("preview_json"), {})
        return result

    def create_upload(self, workspace_id: str, filename: str, artifact_id: str) -> JsonObject:
        upload_id = new_id("upl")
        ts = now_iso()
        with self.connect() as conn:
            conn.execute(
                """
                INSERT INTO uploads(id, workspace_id, filename, artifact_id, created_at)
                VALUES (?, ?, ?, ?, ?)
                """,
                (upload_id, workspace_id, filename, artifact_id, ts),
            )
            self._append_event_tx(
                conn,
                workspace_id,
                None,
                "upload.created",
                {"uploadId": upload_id, "artifactId": artifact_id, "filename": filename},
                ts,
            )
        return {
            "id": upload_id,
            "workspace_id": workspace_id,
            "filename": filename,
            "artifact_id": artifact_id,
            "created_at": ts,
        }

    def get_upload(self, upload_id: str) -> JsonObject:
        with self.connect() as conn:
            row = conn.execute("SELECT * FROM uploads WHERE id = ?", (upload_id,)).fetchone()
        if row is None:
            raise KeyError(f"unknown upload {upload_id}")
        return dict(row)

    def create_upload_session(
        self,
        session_id: str,
        workspace_id: str,
        filename: str,
        content_type: str,
        expected_size_bytes: int | None,
        temp_relative_path: str,
    ) -> JsonObject:
        self.get_workspace(workspace_id)
        ts = now_iso()
        with self.connect() as conn:
            conn.execute(
                """
                INSERT INTO upload_sessions(
                  id, workspace_id, filename, content_type, expected_size_bytes,
                  received_bytes, temp_relative_path, status, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    session_id,
                    workspace_id,
                    filename,
                    content_type,
                    expected_size_bytes,
                    0,
                    temp_relative_path,
                    "active",
                    ts,
                    ts,
                ),
            )
            self._append_event_tx(
                conn,
                workspace_id,
                None,
                "upload_session.created",
                {
                    "sessionId": session_id,
                    "filename": filename,
                    "expectedSizeBytes": expected_size_bytes,
                },
                ts,
            )
        return self.get_upload_session(session_id)

    def get_upload_session(self, session_id: str) -> JsonObject:
        with self.connect() as conn:
            row = conn.execute(
                "SELECT * FROM upload_sessions WHERE id = ?", (session_id,)
            ).fetchone()
        if row is None:
            raise KeyError(f"unknown upload session {session_id}")
        return dict(row)

    def update_upload_session_progress(
        self,
        session_id: str,
        received_bytes: int,
    ) -> JsonObject:
        ts = now_iso()
        with self.connect() as conn:
            row = conn.execute(
                "SELECT workspace_id FROM upload_sessions WHERE id = ?", (session_id,)
            ).fetchone()
            if row is None:
                raise KeyError(f"unknown upload session {session_id}")
            conn.execute(
                """
                UPDATE upload_sessions
                SET received_bytes = ?, updated_at = ?
                WHERE id = ?
                """,
                (received_bytes, ts, session_id),
            )
        return self.get_upload_session(session_id)

    def complete_upload_session(
        self,
        session_id: str,
        upload_id: str,
        artifact_id: str,
    ) -> JsonObject:
        ts = now_iso()
        with self.connect() as conn:
            row = conn.execute(
                "SELECT workspace_id, filename FROM upload_sessions WHERE id = ?",
                (session_id,),
            ).fetchone()
            if row is None:
                raise KeyError(f"unknown upload session {session_id}")
            conn.execute(
                """
                UPDATE upload_sessions
                SET status = ?, upload_id = ?, artifact_id = ?, updated_at = ?
                WHERE id = ?
                """,
                ("completed", upload_id, artifact_id, ts, session_id),
            )
            self._append_event_tx(
                conn,
                row["workspace_id"],
                None,
                "upload_session.completed",
                {
                    "sessionId": session_id,
                    "uploadId": upload_id,
                    "artifactId": artifact_id,
                    "filename": row["filename"],
                },
                ts,
            )
        return self.get_upload_session(session_id)

    def create_evidence(
        self,
        workspace_id: str,
        run_id: str | None,
        kind: str,
        final_allowed: bool,
        summary: str,
        payload: JsonObject,
        artifact_id: str | None = None,
    ) -> JsonObject:
        evidence_id = new_id("ev")
        ts = now_iso()
        with self.connect() as conn:
            conn.execute(
                """
                INSERT INTO evidence_items(
                  id, workspace_id, run_id, kind, final_allowed, summary, artifact_id,
                  payload_json, created_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    evidence_id,
                    workspace_id,
                    run_id,
                    kind,
                    1 if final_allowed else 0,
                    summary,
                    artifact_id,
                    encode_json(payload),
                    ts,
                ),
            )
            self._append_event_tx(
                conn,
                workspace_id,
                run_id,
                "evidence.created",
                {"evidenceId": evidence_id, "kind": kind, "summary": summary},
                ts,
            )
        return self.get_evidence(evidence_id)

    def create_action(
        self,
        run_id: str,
        kind: str,
        payload: JsonObject,
    ) -> JsonObject:
        run = self.get_run(run_id)
        action_id = new_id("act")
        ts = now_iso()
        with self.connect() as conn:
            conn.execute(
                """
                INSERT INTO actions(id, run_id, kind, status, payload_json, created_at, updated_at)
                VALUES (?, ?, ?, ?, ?, ?, ?)
                """,
                (action_id, run_id, kind, "pending", encode_json(payload), ts, ts),
            )
            self._append_event_tx(
                conn,
                run["workspace_id"],
                run_id,
                f"action.{kind}.pending",
                {"actionId": action_id, "payload": payload},
                ts,
            )
        return self.get_action(action_id)

    def get_action(self, action_id: str) -> JsonObject:
        with self.connect() as conn:
            row = conn.execute("SELECT * FROM actions WHERE id = ?", (action_id,)).fetchone()
        if row is None:
            raise KeyError(f"unknown action {action_id}")
        item = dict(row)
        item["payload"] = decode_json(item.pop("payload_json"), {})
        item["result"] = decode_json(item.pop("result_json"), None)
        return item

    def decide_action(
        self,
        action_id: str,
        decision: str,
        reason: str | None,
    ) -> JsonObject:
        action = self.get_action(action_id)
        run = self.get_run(action["run_id"])
        ts = now_iso()
        result = {"decision": decision, "reason": reason}
        with self.connect() as conn:
            conn.execute(
                """
                UPDATE actions
                SET status = ?, result_json = ?, updated_at = ?
                WHERE id = ?
                """,
                (decision, encode_json(result), ts, action_id),
            )
            self._append_event_tx(
                conn,
                run["workspace_id"],
                run["id"],
                f"action.{decision}",
                {"actionId": action_id, "reason": reason},
                ts,
            )
        return self.get_action(action_id)

    def get_evidence(self, evidence_id: str) -> JsonObject:
        with self.connect() as conn:
            row = conn.execute(
                "SELECT * FROM evidence_items WHERE id = ?", (evidence_id,)
            ).fetchone()
        if row is None:
            raise KeyError(f"unknown evidence {evidence_id}")
        result = dict(row)
        result["final_allowed"] = bool(result["final_allowed"])
        result["payload"] = decode_json(result.pop("payload_json"), {})
        return result

    def list_evidence(self, run_id: str) -> list[JsonObject]:
        run = self.get_run(run_id)
        with self.connect() as conn:
            rows = conn.execute(
                """
                SELECT * FROM evidence_items
                WHERE workspace_id = ? AND (run_id = ? OR run_id IS NULL)
                ORDER BY created_at ASC, rowid ASC
                """,
                (run["workspace_id"], run_id),
            ).fetchall()
        evidence = []
        for row in rows:
            item = dict(row)
            item["final_allowed"] = bool(item["final_allowed"])
            item["payload"] = decode_json(item.pop("payload_json"), {})
            evidence.append(item)
        return evidence

    def list_timeline(self, run_id: str) -> list[JsonObject]:
        run = self.get_run(run_id)
        with self.connect() as conn:
            rows = conn.execute(
                """
                SELECT * FROM timeline_events
                WHERE workspace_id = ? AND (run_id = ? OR run_id IS NULL)
                ORDER BY created_at ASC, id ASC
                """,
                (run["workspace_id"], run_id),
            ).fetchall()
        events = []
        for row in rows:
            item = dict(row)
            item["payload"] = decode_json(item.pop("payload_json"), {})
            events.append(item)
        return events

    def upsert_metadata_instance(
        self,
        instance_id: str,
        remark: str | None,
        template_type: str,
        snapshot: JsonObject,
        raw: JsonObject,
    ) -> JsonObject:
        existing = self.get_metadata_instance(instance_id, missing_ok=True)
        ts = now_iso()
        created_at = existing["created_at"] if existing else ts
        with self.connect() as conn:
            conn.execute(
                """
                INSERT INTO metadata_instances(
                  instance_id, remark, template_type, snapshot_json, raw_json,
                  created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(instance_id) DO UPDATE SET
                  remark = excluded.remark,
                  template_type = excluded.template_type,
                  snapshot_json = excluded.snapshot_json,
                  raw_json = excluded.raw_json,
                  updated_at = excluded.updated_at
                """,
                (
                    instance_id,
                    remark,
                    template_type,
                    encode_json(snapshot),
                    encode_json(raw),
                    created_at,
                    ts,
                ),
            )
        return self.get_metadata_instance(instance_id)

    def list_metadata_instances(self) -> list[JsonObject]:
        with self.connect() as conn:
            rows = conn.execute(
                """
                SELECT instance_id, remark, template_type, snapshot_json, created_at, updated_at
                FROM metadata_instances
                ORDER BY updated_at DESC, instance_id ASC
                """
            ).fetchall()
        instances = []
        for row in rows:
            item = dict(row)
            snapshot = decode_json(item.pop("snapshot_json"), {})
            instance = snapshot.get("instance", {})
            cluster = snapshot.get("cluster", {})
            item["instanceId"] = item.pop("instance_id")
            item["templateType"] = item.pop("template_type")
            item["product"] = instance.get("product")
            item["version"] = instance.get("version")
            item["environment"] = instance.get("environment")
            item["nodeCount"] = len(cluster.get("nodes", []))
            item["databaseCount"] = len(cluster.get("databases", []))
            instances.append(item)
        return instances

    def get_metadata_instance(
        self, instance_id: str, missing_ok: bool = False
    ) -> JsonObject | None:
        with self.connect() as conn:
            row = conn.execute(
                "SELECT * FROM metadata_instances WHERE instance_id = ?", (instance_id,)
            ).fetchone()
        if row is None:
            if missing_ok:
                return None
            raise KeyError(f"unknown metadata instance {instance_id}")
        item = dict(row)
        item["instanceId"] = item.pop("instance_id")
        item["templateType"] = item.pop("template_type")
        item["snapshot"] = decode_json(item.pop("snapshot_json"), {})
        item["raw"] = decode_json(item.pop("raw_json"), {})
        return item

    def get_metadata_snapshot(self, instance_id: str) -> JsonObject:
        item = self.get_metadata_instance(instance_id)
        assert item is not None
        return item["snapshot"]

    def delete_metadata_instance(self, instance_id: str) -> None:
        with self.connect() as conn:
            cursor = conn.execute(
                "DELETE FROM metadata_instances WHERE instance_id = ?", (instance_id,)
            )
            if cursor.rowcount == 0:
                raise KeyError(f"unknown metadata instance {instance_id}")

    def create_metadata_import(
        self,
        instance_id: str,
        remark: str | None,
        template_type: str,
        snapshot: JsonObject,
        raw: JsonObject,
        source_url: str | None = None,
    ) -> JsonObject:
        import_id = new_id("mdimp")
        ts = now_iso()
        with self.connect() as conn:
            conn.execute(
                """
                INSERT INTO metadata_imports(
                  import_id, instance_id, remark, template_type, source_url, status,
                  snapshot_json, raw_json, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    import_id,
                    instance_id,
                    remark,
                    template_type,
                    source_url,
                    "previewed",
                    encode_json(snapshot),
                    encode_json(raw),
                    ts,
                    ts,
                ),
            )
        return self.get_metadata_import(import_id)

    def list_metadata_imports(self, limit: int = 50) -> list[JsonObject]:
        limit = max(1, min(limit, 200))
        with self.connect() as conn:
            rows = conn.execute(
                """
                SELECT * FROM metadata_imports
                ORDER BY updated_at DESC, import_id ASC
                LIMIT ?
                """,
                (limit,),
            ).fetchall()
        return [self._metadata_import_from_row(row) for row in rows]

    def get_metadata_import(self, import_id: str) -> JsonObject:
        with self.connect() as conn:
            row = conn.execute(
                "SELECT * FROM metadata_imports WHERE import_id = ?", (import_id,)
            ).fetchone()
        if row is None:
            raise KeyError(f"unknown metadata import {import_id}")
        return self._metadata_import_from_row(row)

    def update_metadata_import_status(self, import_id: str, status: str) -> JsonObject:
        ts = now_iso()
        with self.connect() as conn:
            cursor = conn.execute(
                """
                UPDATE metadata_imports
                SET status = ?, updated_at = ?
                WHERE import_id = ?
                """,
                (status, ts, import_id),
            )
            if cursor.rowcount == 0:
                raise KeyError(f"unknown metadata import {import_id}")
        return self.get_metadata_import(import_id)

    def _metadata_import_from_row(self, row: sqlite3.Row) -> JsonObject:
        item = dict(row)
        item["importId"] = item.pop("import_id")
        item["instanceId"] = item.pop("instance_id")
        item["templateType"] = item.pop("template_type")
        item["sourceUrl"] = item.pop("source_url", None)
        item["snapshot"] = decode_json(item.pop("snapshot_json"), {})
        item["raw"] = decode_json(item.pop("raw_json"), {})
        item["createdAt"] = item.pop("created_at")
        item["updatedAt"] = item.pop("updated_at")
        return item

    def create_case_import(
        self,
        source_text: str,
        draft: JsonObject,
        validation_errors: list[str],
        filename: str | None = None,
    ) -> JsonObject:
        import_id = new_id("caseimp")
        ts = now_iso()
        with self.connect() as conn:
            conn.execute(
                """
                INSERT INTO case_imports(
                  import_id, status, filename, source_text, draft_json,
                  validation_errors_json, case_id, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    import_id,
                    "previewed",
                    filename,
                    source_text,
                    encode_json(draft),
                    encode_json(validation_errors),
                    None,
                    ts,
                    ts,
                ),
            )
        return self.get_case_import(import_id)

    def list_case_imports(self, limit: int = 50) -> list[JsonObject]:
        limit = max(1, min(limit, 200))
        with self.connect() as conn:
            rows = conn.execute(
                """
                SELECT * FROM case_imports
                ORDER BY updated_at DESC, import_id ASC
                LIMIT ?
                """,
                (limit,),
            ).fetchall()
        return [self._case_import_from_row(row) for row in rows]

    def get_case_import(self, import_id: str) -> JsonObject:
        with self.connect() as conn:
            row = conn.execute(
                "SELECT * FROM case_imports WHERE import_id = ?", (import_id,)
            ).fetchone()
        if row is None:
            raise KeyError(f"unknown case import {import_id}")
        return self._case_import_from_row(row)

    def update_case_import(
        self,
        import_id: str,
        status: str,
        draft: JsonObject | None = None,
        validation_errors: list[str] | None = None,
        case_id: str | None = None,
    ) -> JsonObject:
        current = self.get_case_import(import_id)
        ts = now_iso()
        with self.connect() as conn:
            cursor = conn.execute(
                """
                UPDATE case_imports
                SET status = ?,
                    draft_json = ?,
                    validation_errors_json = ?,
                    case_id = ?,
                    updated_at = ?
                WHERE import_id = ?
                """,
                (
                    status,
                    encode_json(draft if draft is not None else current["draft"]),
                    encode_json(
                        validation_errors
                        if validation_errors is not None
                        else current["validationErrors"]
                    ),
                    case_id if case_id is not None else current.get("caseId"),
                    ts,
                    import_id,
                ),
            )
            if cursor.rowcount == 0:
                raise KeyError(f"unknown case import {import_id}")
        return self.get_case_import(import_id)

    def _case_import_from_row(self, row: sqlite3.Row) -> JsonObject:
        item = dict(row)
        item["importId"] = item.pop("import_id")
        item["sourceText"] = item.pop("source_text")
        item["sourceSizeBytes"] = len(item["sourceText"].encode("utf-8"))
        item["draft"] = decode_json(item.pop("draft_json"), {})
        item["validationErrors"] = decode_json(item.pop("validation_errors_json"), [])
        item["caseId"] = item.pop("case_id", None)
        item["createdAt"] = item.pop("created_at")
        item["updatedAt"] = item.pop("updated_at")
        return item

    def create_case(self, record: JsonObject, searchable_text: str) -> JsonObject:
        case_id = record["caseId"]
        ts = record["createdAt"]
        with self.connect() as conn:
            conn.execute(
                """
                INSERT INTO cases(
                  case_id, source_type, task_id, enabled, record_json, searchable_text,
                  vector_json, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    case_id,
                    record["sourceType"],
                    record.get("taskId"),
                    1 if record.get("enabled", True) else 0,
                    encode_json(record),
                    searchable_text,
                    encode_json(case_vector(searchable_text)),
                    ts,
                    record["updatedAt"],
                ),
            )
            self._upsert_case_fts_tx(conn, case_id, searchable_text)
        return self.get_case(case_id)

    def get_case(self, case_id: str) -> JsonObject:
        with self.connect() as conn:
            row = conn.execute("SELECT * FROM cases WHERE case_id = ?", (case_id,)).fetchone()
        if row is None:
            raise KeyError(f"unknown case {case_id}")
        return self._case_from_row(row)

    def find_case_by_task(self, task_id: str) -> JsonObject | None:
        with self.connect() as conn:
            row = conn.execute(
                "SELECT * FROM cases WHERE source_type = 'task' AND task_id = ? LIMIT 1",
                (task_id,),
            ).fetchone()
        return self._case_from_row(row) if row else None

    def update_case(self, case_id: str, updates: JsonObject, searchable_text: str) -> JsonObject:
        current = self.get_case(case_id)
        record = dict(current)
        record.update({key: value for key, value in updates.items() if value is not None})
        record["updatedAt"] = now_iso()
        with self.connect() as conn:
            conn.execute(
                """
                UPDATE cases
                SET enabled = ?, record_json = ?, searchable_text = ?, vector_json = ?,
                    updated_at = ?
                WHERE case_id = ?
                """,
                (
                    1 if record.get("enabled", True) else 0,
                    encode_json(record),
                    searchable_text,
                    encode_json(case_vector(searchable_text)),
                    record["updatedAt"],
                    case_id,
                ),
            )
            self._upsert_case_fts_tx(conn, case_id, searchable_text)
        return self.get_case(case_id)

    def search_cases(
        self,
        query: str | None,
        limit: int,
        include_disabled: bool = False,
    ) -> list[JsonObject]:
        limit = max(1, min(limit, 50))
        tokens = case_tokens(query or "")
        if tokens:
            fts_results = self._search_cases_fts(query or "", limit, include_disabled)
            if fts_results is not None:
                return self._merge_case_vector_results(
                    fts_results,
                    self._search_cases_vector(query or "", limit, include_disabled),
                    limit,
                )
            vector_results = self._search_cases_vector(query or "", limit, include_disabled)
            if vector_results:
                return vector_results
        with self.connect() as conn:
            rows = conn.execute(
                """
                SELECT * FROM cases
                WHERE (? OR enabled = 1)
                ORDER BY updated_at DESC, case_id ASC
                """,
                (1 if include_disabled else 0,),
            ).fetchall()
        cases = [self._case_from_row(row) for row in rows]
        scored = []
        for record in cases:
            text = case_searchable_text(record)
            score = keyword_score(tokens, text)
            if tokens and score == 0:
                continue
            hit = dict(record)
            hit["score"] = score
            hit["searchBackend"] = "keyword" if tokens else "recent"
            scored.append(hit)
        scored.sort(key=lambda item: (item["score"], item["updatedAt"]), reverse=True)
        return scored[:limit]

    def _search_cases_fts(
        self,
        query: str,
        limit: int,
        include_disabled: bool,
    ) -> list[JsonObject] | None:
        tokens = case_tokens(query)
        if not tokens:
            return None
        fts_query = " OR ".join(quote_fts_token(token) for token in tokens)
        try:
            with self.connect() as conn:
                rows = conn.execute(
                    """
                    SELECT c.*, bm25(cases_fts) AS rank
                    FROM cases_fts
                    JOIN cases c ON c.rowid = cases_fts.rowid
                    WHERE cases_fts MATCH ? AND (? OR c.enabled = 1)
                    ORDER BY rank ASC, c.updated_at DESC, c.case_id ASC
                    LIMIT ?
                    """,
                    (fts_query, 1 if include_disabled else 0, limit),
                ).fetchall()
        except sqlite3.OperationalError:
            return None
        results = []
        for row in rows:
            hit = self._case_from_row(row)
            hit["score"] = round(float(row["rank"]) * -1.0, 6)
            hit["searchBackend"] = "fts5"
            results.append(hit)
        return results

    def _search_cases_vector(
        self,
        query: str,
        limit: int,
        include_disabled: bool,
    ) -> list[JsonObject]:
        query_vector = case_vector(query)
        if not any(query_vector):
            return []
        with self.connect() as conn:
            rows = conn.execute(
                """
                SELECT * FROM cases
                WHERE (? OR enabled = 1)
                ORDER BY updated_at DESC, case_id ASC
                """,
                (1 if include_disabled else 0,),
            ).fetchall()
        results = []
        for row in rows:
            vector = decode_json(row["vector_json"], [])
            if not isinstance(vector, list):
                continue
            score = vector_similarity(query_vector, [float(item) for item in vector])
            if score <= 0.05:
                continue
            hit = self._case_from_row(row)
            hit["score"] = round(score, 6)
            hit["vectorScore"] = round(score, 6)
            hit["searchBackend"] = "vector"
            results.append(hit)
        results.sort(key=lambda item: (item["score"], item["updatedAt"]), reverse=True)
        return results[:limit]

    def _merge_case_vector_results(
        self,
        fts_results: list[JsonObject],
        vector_results: list[JsonObject],
        limit: int,
    ) -> list[JsonObject]:
        merged: dict[str, JsonObject] = {}
        for item in fts_results:
            case_id = item["caseId"]
            hit = dict(item)
            hit["ftsScore"] = hit.get("score", 0)
            hit["vectorScore"] = 0.0
            hit["searchBackend"] = "hybrid"
            merged[case_id] = hit
        for item in vector_results:
            case_id = item["caseId"]
            if case_id in merged:
                merged[case_id]["vectorScore"] = item["vectorScore"]
                merged[case_id]["score"] = round(
                    float(merged[case_id].get("ftsScore", 0)) + item["vectorScore"],
                    6,
                )
            else:
                merged[case_id] = dict(item)
        results = list(merged.values())
        results.sort(key=lambda item: (item["score"], item["updatedAt"]), reverse=True)
        return results[:limit]

    def _case_from_row(self, row: sqlite3.Row) -> JsonObject:
        record = decode_json(row["record_json"], {})
        return record

    def create_fetch_endpoint(
        self,
        name: str,
        method: str,
        url: str,
        headers: JsonObject,
        body: str | None,
        enabled: bool,
    ) -> JsonObject:
        endpoint_id = new_id("fetch")
        ts = now_iso()
        with self.connect() as conn:
            conn.execute(
                """
                INSERT INTO fetch_endpoints(
                  id, name, method, url, headers_json, body, enabled, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    endpoint_id,
                    name,
                    method,
                    url,
                    encode_json(headers),
                    body,
                    1 if enabled else 0,
                    ts,
                    ts,
                ),
            )
        return self.get_fetch_endpoint(endpoint_id)

    def list_fetch_endpoints(self) -> list[JsonObject]:
        with self.connect() as conn:
            rows = conn.execute(
                "SELECT * FROM fetch_endpoints ORDER BY updated_at DESC, id ASC"
            ).fetchall()
        return [self._fetch_endpoint_from_row(row) for row in rows]

    def get_fetch_endpoint(self, endpoint_id: str) -> JsonObject:
        with self.connect() as conn:
            row = conn.execute(
                "SELECT * FROM fetch_endpoints WHERE id = ?", (endpoint_id,)
            ).fetchone()
        if row is None:
            raise KeyError(f"unknown fetch endpoint {endpoint_id}")
        return self._fetch_endpoint_from_row(row)

    def get_fetch_credential_set(self, endpoint_id: str) -> JsonObject | None:
        with self.connect() as conn:
            row = conn.execute(
                """
                SELECT * FROM fetch_credential_sets
                WHERE endpoint_id = ?
                """,
                (endpoint_id,),
            ).fetchone()
        return self._fetch_credential_set_from_row(row) if row else None

    def upsert_fetch_credential_set(
        self,
        endpoint_id: str,
        encrypted_json: str,
        redacted: JsonObject,
    ) -> JsonObject:
        existing = self.get_fetch_credential_set(endpoint_id)
        credential_id = existing["id"] if existing else new_id("fetchcred")
        created_at = existing["createdAt"] if existing else now_iso()
        ts = now_iso()
        with self.connect() as conn:
            conn.execute(
                """
                INSERT INTO fetch_credential_sets(
                  id, endpoint_id, encrypted_json, redacted_json, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?)
                ON CONFLICT(endpoint_id) DO UPDATE SET
                  encrypted_json = excluded.encrypted_json,
                  redacted_json = excluded.redacted_json,
                  updated_at = excluded.updated_at
                """,
                (
                    credential_id,
                    endpoint_id,
                    encrypted_json,
                    encode_json(redacted),
                    created_at,
                    ts,
                ),
            )
        credential = self.get_fetch_credential_set(endpoint_id)
        assert credential is not None
        return credential

    def delete_fetch_credential_set(self, endpoint_id: str) -> None:
        with self.connect() as conn:
            conn.execute("DELETE FROM fetch_credential_sets WHERE endpoint_id = ?", (endpoint_id,))

    def update_fetch_endpoint(self, endpoint_id: str, updates: JsonObject) -> JsonObject:
        current = self.get_fetch_endpoint(endpoint_id)
        merged = dict(current)
        for key, value in updates.items():
            if key == "body":
                merged[key] = value if isinstance(value, str) else None
            elif value is not None:
                merged[key] = value
        ts = now_iso()
        with self.connect() as conn:
            conn.execute(
                """
                UPDATE fetch_endpoints
                SET name = ?, method = ?, url = ?, headers_json = ?, body = ?,
                    enabled = ?, updated_at = ?
                WHERE id = ?
                """,
                (
                    merged["name"],
                    merged["method"],
                    merged["url"],
                    encode_json(merged.get("headers", {})),
                    merged.get("body"),
                    1 if merged.get("enabled", True) else 0,
                    ts,
                    endpoint_id,
                ),
            )
        return self.get_fetch_endpoint(endpoint_id)

    def delete_fetch_endpoint(self, endpoint_id: str) -> None:
        with self.connect() as conn:
            conn.execute("DELETE FROM fetch_credential_sets WHERE endpoint_id = ?", (endpoint_id,))
            cursor = conn.execute("DELETE FROM fetch_endpoints WHERE id = ?", (endpoint_id,))
            if cursor.rowcount == 0:
                raise KeyError(f"unknown fetch endpoint {endpoint_id}")

    def _fetch_endpoint_from_row(self, row: sqlite3.Row) -> JsonObject:
        item = dict(row)
        item["headers"] = decode_json(item.pop("headers_json"), {})
        item["enabled"] = bool(item["enabled"])
        item["createdAt"] = item.pop("created_at")
        item["updatedAt"] = item.pop("updated_at")
        return item

    def _fetch_credential_set_from_row(self, row: sqlite3.Row) -> JsonObject:
        item = dict(row)
        item["endpointId"] = item.pop("endpoint_id")
        item["encrypted"] = item.pop("encrypted_json")
        item["redacted"] = decode_json(item.pop("redacted_json"), {})
        item["createdAt"] = item.pop("created_at")
        item["updatedAt"] = item.pop("updated_at")
        return item

    def append_event(
        self, workspace_id: str, run_id: str | None, kind: str, payload: JsonObject
    ) -> JsonObject:
        ts = now_iso()
        with self.connect() as conn:
            return self._append_event_tx(conn, workspace_id, run_id, kind, payload, ts)

    def acquire_jobs(self, worker_id: str, limit: int, lock_seconds: int = 60) -> list[JsonObject]:
        ts = now_iso()
        locked_until = (
            datetime.now(UTC).replace(microsecond=0) + timedelta(seconds=lock_seconds)
        ).isoformat()
        acquired: list[JsonObject] = []
        with self.connect() as conn:
            conn.execute("BEGIN IMMEDIATE")
            rows = conn.execute(
                """
                SELECT * FROM jobs
                WHERE
                  next_run_at <= ?
                  AND (
                    status = 'queued'
                    OR (status = 'running' AND locked_until IS NOT NULL AND locked_until < ?)
                  )
                ORDER BY created_at ASC
                LIMIT ?
                """,
                (ts, ts, limit),
            ).fetchall()
            for row in rows:
                attempts = int(row["attempts"]) + 1
                conn.execute(
                    """
                    UPDATE jobs
                    SET status = 'running', locked_by = ?, locked_until = ?,
                        attempts = ?, updated_at = ?
                    WHERE id = ?
                    """,
                    (worker_id, locked_until, attempts, ts, row["id"]),
                )
                item = dict(row)
                item["attempts"] = attempts
                item["status"] = "running"
                item["locked_by"] = worker_id
                item["locked_until"] = locked_until
                item["payload"] = decode_json(item.pop("payload_json"), {})
                acquired.append(item)
        return acquired

    def complete_job(self, job_id: str) -> None:
        ts = now_iso()
        with self.connect() as conn:
            conn.execute(
                """
                UPDATE jobs
                SET status = 'succeeded', locked_by = NULL, locked_until = NULL, updated_at = ?
                WHERE id = ?
                """,
                (ts, job_id),
            )

    def fail_job(self, job: JsonObject, error: str) -> None:
        ts = now_iso()
        status = "failed" if int(job["attempts"]) >= int(job["max_attempts"]) else "queued"
        next_run_at = (
            datetime.now(UTC).replace(microsecond=0) + timedelta(seconds=5 * int(job["attempts"]))
        ).isoformat()
        with self.connect() as conn:
            conn.execute(
                """
                UPDATE jobs
                SET status = ?, locked_by = NULL, locked_until = NULL, next_run_at = ?,
                    last_error = ?, updated_at = ?
                WHERE id = ?
                """,
                (status, next_run_at, error[:2000], ts, job["id"]),
            )

    def _enqueue_run_tx(
        self,
        conn: sqlite3.Connection,
        job_id: str,
        workspace_id: str,
        run_id: str,
        ts: str,
    ) -> None:
        conn.execute(
            """
            INSERT INTO jobs(
              id, kind, status, payload_json, attempts, max_attempts, next_run_at,
              created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            (
                job_id,
                "run_analysis",
                "queued",
                encode_json({"workspace_id": workspace_id, "run_id": run_id}),
                0,
                3,
                ts,
                ts,
                ts,
            ),
        )

    def _append_event_tx(
        self,
        conn: sqlite3.Connection,
        workspace_id: str,
        run_id: str | None,
        kind: str,
        payload: JsonObject,
        ts: str,
    ) -> JsonObject:
        event_id = new_id("evt")
        conn.execute(
            """
            INSERT INTO timeline_events(id, workspace_id, run_id, kind, payload_json, created_at)
            VALUES (?, ?, ?, ?, ?, ?)
            """,
            (event_id, workspace_id, run_id, kind, encode_json(payload), ts),
        )
        return {
            "id": event_id,
            "workspace_id": workspace_id,
            "run_id": run_id,
            "kind": kind,
            "payload": payload,
            "created_at": ts,
        }
