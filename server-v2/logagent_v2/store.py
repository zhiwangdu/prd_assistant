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
DEFAULT_FETCH_REFRESH_POLICY: JsonObject = {
    "mode": "manual_only",
    "automaticRefresh": False,
    "tokenRefreshSupported": False,
}
CASE_VECTOR_DIMS = 64
UNSET = object()
RUN_TERMINAL_STATUSES = {"succeeded", "failed"}
REMOTE_RUN_TERMINAL_STATUSES = {"SUCCEEDED", "FAILED"}


def now_iso() -> str:
    return datetime.now(UTC).replace(microsecond=0).isoformat()


def encode_json(value: Any) -> str:
    return json.dumps(value, ensure_ascii=True, separators=(",", ":"))


def decode_json(value: str | None, default: Any = None) -> Any:
    if value is None:
        return default
    return json.loads(value)


def artifact_support_specs(value: Any) -> list[JsonObject]:
    if not isinstance(value, dict):
        return []
    action_id = value.get("actionId")
    if not isinstance(action_id, str) or not action_id:
        action_id = None
    specs: list[JsonObject] = []
    seen: set[str] = set()

    def add(artifact_id: Any, role: str, logical_path: Any) -> None:
        if not isinstance(artifact_id, str) or not artifact_id:
            return
        if artifact_id in seen:
            return
        if not isinstance(logical_path, str) or not logical_path:
            logical_path = default_tool_support_path(action_id, role)
        seen.add(artifact_id)
        specs.append(
            {
                "artifact_id": artifact_id,
                "role": role,
                "logical_path": logical_path,
                "action_id": action_id,
            }
        )

    add(value.get("stdoutArtifactId"), "stdout", value.get("stdoutPath"))
    add(value.get("stderrArtifactId"), "stderr", value.get("stderrPath"))
    add(value.get("bodyArtifactId"), "response_body", value.get("bodyArtifactPath"))
    response = value.get("response")
    if isinstance(response, dict):
        add(
            response.get("bodyArtifactId"),
            "response_body",
            response.get("bodyArtifactPath"),
        )
    add(value.get("manifestArtifactId"), "manifest", "manifest.json")
    add(value.get("grepArtifactId"), "grep_results", "grep_results.json")

    artifact_ids = value.get("artifactIds")
    if isinstance(artifact_ids, dict):
        add_artifact_id_map(specs, seen, action_id, artifact_ids, value.get("artifactPaths"))
    else:
        artifacts = value.get("artifacts")
        if isinstance(artifacts, dict):
            add_artifact_id_map(specs, seen, action_id, artifacts, value.get("artifactPaths"))
    return specs


def add_artifact_id_map(
    specs: list[JsonObject],
    seen: set[str],
    action_id: str | None,
    artifact_ids: JsonObject,
    artifact_paths: Any,
) -> None:
    paths = artifact_paths if isinstance(artifact_paths, dict) else {}
    for role, artifact_id in artifact_ids.items():
        if not isinstance(role, str):
            continue
        if not isinstance(artifact_id, str) or not artifact_id or artifact_id in seen:
            continue
        path = support_path_for_role(action_id, role, paths)
        seen.add(artifact_id)
        specs.append(
            {
                "artifact_id": artifact_id,
                "role": role,
                "logical_path": path,
                "action_id": action_id,
            }
        )


def support_path_for_role(
    action_id: str | None,
    role: str,
    artifact_paths: JsonObject,
) -> str:
    role_path_fields = {
        "stdout": "stdoutPath",
        "stderr": "stderrPath",
        "top": "topTextPath",
        "tree": "treeTextPath",
        "raw": "rawTextPath",
        "svg": "svgPath",
        "body": "bodyArtifactPath",
        "response_body": "bodyArtifactPath",
        "collected_file": "collectedFilePath",
    }
    path_field = role_path_fields.get(role, f"{role}Path")
    path = artifact_paths.get(path_field)
    if isinstance(path, str) and path:
        return path
    return default_tool_support_path(action_id, role)


def default_tool_support_path(action_id: str | None, role: str) -> str:
    if not action_id:
        return role
    filenames = {
        "stdout": "stdout.txt",
        "stderr": "stderr.txt",
        "top": "top.txt",
        "tree": "tree.txt",
        "raw": "raw.txt",
        "svg": "graph.svg",
        "body": "response_body.bin",
        "response_body": "response_body.bin",
    }
    return f"tool_results/{action_id}/{filenames.get(role, f'{role}.bin')}"


def session_status_from_run_status(status: str) -> str:
    if status == "queued":
        return "ready"
    return status


def ensure_run_not_terminal(run: JsonObject, action: str) -> None:
    status = run.get("status")
    if status in RUN_TERMINAL_STATUSES:
        raise ValueError(
            f"terminal run {run.get('id')} with status {status} cannot be {action}"
        )


def ensure_remote_run_not_terminal(run: JsonObject, action: str) -> None:
    status = run.get("status")
    if status in REMOTE_RUN_TERMINAL_STATUSES:
        run_id = run.get("taskId") or run.get("id")
        raise ValueError(
            f"terminal remote run {run_id} with status {status} cannot be {action}"
        )


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
                  title TEXT,
                  question TEXT NOT NULL,
                  source_url TEXT,
                  instance_id TEXT,
                  node_id TEXT,
                  mode TEXT NOT NULL,
                  language TEXT NOT NULL,
                  status TEXT NOT NULL,
                  session_status TEXT NOT NULL DEFAULT 'draft',
                  system_context_ids_json TEXT NOT NULL DEFAULT '[]',
                  skill_ids_json TEXT NOT NULL DEFAULT '[]',
                  upload_ids_json TEXT NOT NULL DEFAULT '[]',
                  created_at TEXT NOT NULL,
                  updated_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS runs (
                  id TEXT PRIMARY KEY,
                  workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
                  kind TEXT NOT NULL DEFAULT 'analysis',
                  status TEXT NOT NULL,
                  phase TEXT NOT NULL,
                  budget_json TEXT NOT NULL,
                  tool_id TEXT,
                  tool_params_json TEXT NOT NULL DEFAULT '{}',
                  tool_upload_ids_json TEXT NOT NULL DEFAULT '[]',
                  tool_result_artifact_id TEXT REFERENCES artifacts(id),
                  final_answer_json TEXT,
                  alias TEXT,
                  error_json TEXT,
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
                  messages_json TEXT NOT NULL DEFAULT '[]',
                  case_id TEXT,
                  created_at TEXT NOT NULL,
                  updated_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS fetch_endpoints (
                  id TEXT PRIMARY KEY,
                  schema_version INTEGER NOT NULL DEFAULT 2,
                  name TEXT NOT NULL,
                  method TEXT NOT NULL,
                  url TEXT NOT NULL,
                  headers_json TEXT NOT NULL,
                  body TEXT,
                  enabled INTEGER NOT NULL,
                  follow_redirects INTEGER NOT NULL DEFAULT 0,
                  refresh_policy_json TEXT NOT NULL DEFAULT '{"mode":"manual_only","automaticRefresh":false,"tokenRefreshSupported":false}',
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

                CREATE TABLE IF NOT EXISTS remote_executors (
                  id TEXT PRIMARY KEY,
                  name TEXT NOT NULL,
                  host TEXT NOT NULL,
                  port INTEGER NOT NULL,
                  user TEXT NOT NULL,
                  tags_json TEXT NOT NULL,
                  enabled INTEGER NOT NULL,
                  notes TEXT,
                  last_check_json TEXT,
                  created_at TEXT NOT NULL,
                  updated_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS remote_runs (
                  id TEXT PRIMARY KEY,
                  executor_id TEXT NOT NULL REFERENCES remote_executors(id),
                  command_id TEXT NOT NULL,
                  operation TEXT NOT NULL DEFAULT 'command',
                  input_json TEXT NOT NULL DEFAULT '{}',
                  idempotency_key TEXT UNIQUE,
                  status TEXT NOT NULL,
                  phase TEXT,
                  alias TEXT,
                  attempts INTEGER NOT NULL,
                  result_json TEXT,
                  error_json TEXT,
                  created_at TEXT NOT NULL,
                  updated_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS system_context_resources (
                  id TEXT PRIMARY KEY,
                  record_json TEXT NOT NULL,
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
                CREATE INDEX IF NOT EXISTS idx_remote_runs_executor
                  ON remote_runs(executor_id, created_at);
                CREATE INDEX IF NOT EXISTS idx_remote_runs_idempotency
                  ON remote_runs(idempotency_key);
                CREATE INDEX IF NOT EXISTS idx_system_context_resources_updated
                  ON system_context_resources(updated_at);
                """
            )
            self._ensure_column_tx(
                conn, "workspaces", "title", "TEXT"
            )
            self._ensure_column_tx(
                conn, "workspaces", "source_url", "TEXT"
            )
            self._ensure_column_tx(
                conn, "workspaces", "instance_id", "TEXT"
            )
            self._ensure_column_tx(
                conn, "workspaces", "node_id", "TEXT"
            )
            self._ensure_column_tx(
                conn, "workspaces", "session_status", "TEXT NOT NULL DEFAULT 'draft'"
            )
            self._ensure_column_tx(
                conn,
                "workspaces",
                "system_context_ids_json",
                "TEXT NOT NULL DEFAULT '[]'",
            )
            self._ensure_column_tx(
                conn, "workspaces", "skill_ids_json", "TEXT NOT NULL DEFAULT '[]'"
            )
            self._ensure_column_tx(
                conn, "workspaces", "upload_ids_json", "TEXT NOT NULL DEFAULT '[]'"
            )
            self._ensure_column_tx(conn, "runs", "kind", "TEXT NOT NULL DEFAULT 'analysis'")
            self._ensure_column_tx(conn, "runs", "tool_id", "TEXT")
            self._ensure_column_tx(conn, "runs", "tool_params_json", "TEXT NOT NULL DEFAULT '{}'")
            self._ensure_column_tx(
                conn, "runs", "tool_upload_ids_json", "TEXT NOT NULL DEFAULT '[]'"
            )
            self._ensure_column_tx(conn, "runs", "tool_result_artifact_id", "TEXT")
            self._ensure_column_tx(conn, "runs", "alias", "TEXT")
            self._ensure_column_tx(conn, "runs", "error_json", "TEXT")
            self._ensure_column_tx(conn, "metadata_imports", "source_url", "TEXT")
            self._ensure_column_tx(
                conn, "fetch_endpoints", "follow_redirects", "INTEGER NOT NULL DEFAULT 0"
            )
            self._ensure_column_tx(
                conn, "fetch_endpoints", "schema_version", "INTEGER NOT NULL DEFAULT 2"
            )
            self._ensure_column_tx(
                conn,
                "fetch_endpoints",
                "refresh_policy_json",
                """TEXT NOT NULL DEFAULT '{"mode":"manual_only","automaticRefresh":false,"tokenRefreshSupported":false}'""",
            )
            self._ensure_column_tx(conn, "cases", "vector_json", "TEXT NOT NULL DEFAULT '[]'")
            self._ensure_column_tx(
                conn, "case_imports", "messages_json", "TEXT NOT NULL DEFAULT '[]'"
            )
            self._ensure_column_tx(
                conn, "remote_runs", "operation", "TEXT NOT NULL DEFAULT 'command'"
            )
            self._ensure_column_tx(
                conn, "remote_runs", "input_json", "TEXT NOT NULL DEFAULT '{}'"
            )
            self._ensure_case_fts_tx(conn)
            self._backfill_case_vectors_tx(conn)
            self._backfill_workspace_upload_ids_tx(conn)

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

    def _backfill_workspace_upload_ids_tx(self, conn: sqlite3.Connection) -> None:
        rows = conn.execute("SELECT id, upload_ids_json FROM workspaces").fetchall()
        for row in rows:
            if decode_json(row["upload_ids_json"], []):
                continue
            uploads = conn.execute(
                """
                SELECT id FROM uploads
                WHERE workspace_id = ?
                ORDER BY created_at ASC, id ASC
                """,
                (row["id"],),
            ).fetchall()
            upload_ids = [upload["id"] for upload in uploads]
            if not upload_ids:
                continue
            conn.execute(
                """
                UPDATE workspaces
                SET upload_ids_json = ?,
                    session_status = CASE
                      WHEN session_status = 'draft' THEN 'ready'
                      ELSE session_status
                    END
                WHERE id = ?
                """,
                (encode_json(upload_ids), row["id"]),
            )

    def _workspace_from_row(self, row: sqlite3.Row) -> JsonObject:
        item = dict(row)
        if not item.get("title"):
            item["title"] = str(item.get("question") or "")[:120] or "Untitled session"
        item["sourceUrl"] = item.pop("source_url", None)
        item["instanceId"] = item.pop("instance_id", None)
        item["nodeId"] = item.pop("node_id", None)
        item["sessionStatus"] = item.pop("session_status", "draft")
        item["systemContextIds"] = decode_json(item.pop("system_context_ids_json", None), [])
        item["skillIds"] = decode_json(item.pop("skill_ids_json", None), [])
        item["uploadIds"] = decode_json(item.pop("upload_ids_json", None), [])
        return item

    def create_workspace(
        self,
        question: str,
        mode: str,
        language: str,
        skill_ids: list[str] | None = None,
        title: str | None = None,
        source_url: str | None = None,
        instance_id: str | None = None,
        node_id: str | None = None,
        system_context_ids: list[str] | None = None,
        session_status: str = "draft",
    ) -> JsonObject:
        workspace_id = new_id("ws")
        ts = now_iso()
        skill_ids = skill_ids or []
        system_context_ids = system_context_ids or []
        title = title or question[:120] or "Untitled session"
        with self.connect() as conn:
            conn.execute(
                """
                INSERT INTO workspaces(
                  id, title, question, source_url, instance_id, node_id, mode, language, status,
                  session_status, system_context_ids_json, skill_ids_json, upload_ids_json,
                  created_at, updated_at
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    workspace_id,
                    title,
                    question,
                    source_url,
                    instance_id,
                    node_id,
                    mode,
                    language,
                    "active",
                    session_status,
                    encode_json(system_context_ids),
                    encode_json(skill_ids),
                    encode_json([]),
                    ts,
                    ts,
                ),
            )
            self._append_event_tx(
                conn,
                workspace_id,
                None,
                "workspace.created",
                {
                    "title": title,
                    "question": question,
                    "sourceUrl": source_url,
                    "instanceId": instance_id,
                    "nodeId": node_id,
                    "mode": mode,
                    "language": language,
                    "sessionStatus": session_status,
                    "systemContextIds": system_context_ids,
                    "skillIds": skill_ids,
                },
                ts,
            )
        return self.get_workspace(workspace_id)

    def list_workspaces(self, include_deleted: bool = False) -> list[JsonObject]:
        with self.connect() as conn:
            if include_deleted:
                rows = conn.execute(
                    "SELECT * FROM workspaces ORDER BY created_at DESC, id DESC"
                ).fetchall()
            else:
                rows = conn.execute(
                    """
                    SELECT * FROM workspaces
                    WHERE status != 'deleted'
                    ORDER BY created_at DESC, id DESC
                    """
                ).fetchall()
        return [self._workspace_from_row(row) for row in rows]

    def get_workspace(self, workspace_id: str) -> JsonObject:
        with self.connect() as conn:
            row = conn.execute("SELECT * FROM workspaces WHERE id = ?", (workspace_id,)).fetchone()
        if row is None:
            raise KeyError(f"unknown workspace {workspace_id}")
        return self._workspace_from_row(row)

    def update_workspace(
        self,
        workspace_id: str,
        question: str | None = None,
        mode: str | None = None,
        language: str | None = None,
        skill_ids: list[str] | None = None,
        title: str | None = None,
        source_url: str | None = None,
        instance_id: str | None = None,
        node_id: str | None = None,
        system_context_ids: list[str] | None = None,
        session_status: str | None = None,
    ) -> JsonObject:
        current = self.get_workspace(workspace_id)
        if current["status"] == "deleted":
            raise ValueError("workspace is deleted")
        next_title = title if title is not None else current["title"]
        next_question = question if question is not None else current["question"]
        next_source_url = current["sourceUrl"] if source_url is UNSET else source_url
        next_instance_id = current["instanceId"] if instance_id is UNSET else instance_id
        next_node_id = current["nodeId"] if node_id is UNSET else node_id
        next_mode = mode if mode is not None else current["mode"]
        next_language = language if language is not None else current["language"]
        next_session_status = (
            session_status if session_status is not None else current["sessionStatus"]
        )
        next_system_context_ids = (
            system_context_ids
            if system_context_ids is not None
            else current["systemContextIds"]
        )
        next_skill_ids = skill_ids if skill_ids is not None else current["skillIds"]
        ts = now_iso()
        with self.connect() as conn:
            conn.execute(
                """
                UPDATE workspaces
                SET title = ?,
                    question = ?,
                    source_url = ?,
                    instance_id = ?,
                    node_id = ?,
                    mode = ?,
                    language = ?,
                    session_status = ?,
                    system_context_ids_json = ?,
                    skill_ids_json = ?,
                    updated_at = ?
                WHERE id = ?
                """,
                (
                    next_title,
                    next_question,
                    next_source_url,
                    next_instance_id,
                    next_node_id,
                    next_mode,
                    next_language,
                    next_session_status,
                    encode_json(next_system_context_ids),
                    encode_json(next_skill_ids),
                    ts,
                    workspace_id,
                ),
            )
            self._append_event_tx(
                conn,
                workspace_id,
                None,
                "workspace.updated",
                {
                    "title": next_title,
                    "question": next_question,
                    "sourceUrl": next_source_url,
                    "instanceId": next_instance_id,
                    "nodeId": next_node_id,
                    "mode": next_mode,
                    "language": next_language,
                    "sessionStatus": next_session_status,
                    "systemContextIds": next_system_context_ids,
                    "skillIds": next_skill_ids,
                },
                ts,
            )
        return self.get_workspace(workspace_id)

    def delete_workspace(self, workspace_id: str) -> JsonObject:
        current = self.get_workspace(workspace_id)
        if current["status"] == "deleted":
            return current
        ts = now_iso()
        with self.connect() as conn:
            conn.execute(
                """
                UPDATE workspaces
                SET status = 'deleted',
                    updated_at = ?
                WHERE id = ?
                """,
                (ts, workspace_id),
            )
            self._append_event_tx(
                conn,
                workspace_id,
                None,
                "workspace.deleted",
                {"workspaceId": workspace_id},
                ts,
            )
        return self.get_workspace(workspace_id)

    def list_uploads(self, workspace_id: str) -> list[JsonObject]:
        workspace = self.get_workspace(workspace_id)
        upload_ids = workspace.get("uploadIds", [])
        if not upload_ids:
            return []
        placeholders = ",".join("?" for _ in upload_ids)
        with self.connect() as conn:
            rows = conn.execute(
                f"""
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
                  AND uploads.id IN ({placeholders})
                ORDER BY uploads.created_at ASC, uploads.id ASC
                """,
                (workspace_id, *upload_ids),
            ).fetchall()
        by_id = {row["id"]: dict(row) for row in rows}
        return [by_id[upload_id] for upload_id in upload_ids if upload_id in by_id]

    def attach_uploads(self, workspace_id: str, upload_ids: list[str]) -> JsonObject:
        workspace = self.get_workspace(workspace_id)
        unique_upload_ids = []
        for upload_id in upload_ids:
            upload_id = upload_id.strip()
            if upload_id and upload_id not in unique_upload_ids:
                unique_upload_ids.append(upload_id)
        if not unique_upload_ids:
            raise ValueError("missing uploadIds")

        ts = now_iso()
        with self.connect() as conn:
            attached_upload_ids = list(workspace.get("uploadIds", []))
            for upload_id in unique_upload_ids:
                upload = conn.execute(
                    "SELECT id, workspace_id FROM uploads WHERE id = ?",
                    (upload_id,),
                ).fetchone()
                if upload is None:
                    raise KeyError(f"unknown upload {upload_id}")
                if upload["workspace_id"] != workspace_id:
                    raise ValueError(f"upload {upload_id} does not belong to workspace {workspace_id}")
                if upload_id not in attached_upload_ids:
                    attached_upload_ids.append(upload_id)
            conn.execute(
                """
                UPDATE workspaces
                SET upload_ids_json = ?,
                    session_status = CASE
                      WHEN session_status = 'draft' THEN 'ready'
                      ELSE session_status
                    END,
                    updated_at = ?
                WHERE id = ?
                """,
                (encode_json(attached_upload_ids), ts, workspace_id),
            )
            for upload_id in unique_upload_ids:
                self._append_event_tx(
                    conn,
                    workspace_id,
                    None,
                    "upload.attached",
                    {"uploadId": upload_id},
                    ts,
                )
        return self.get_workspace(workspace_id)

    def detach_upload(self, workspace_id: str, upload_id: str) -> JsonObject:
        workspace = self.get_workspace(workspace_id)
        attached_upload_ids = list(workspace.get("uploadIds", []))
        if upload_id not in attached_upload_ids:
            return workspace
        runs = self.list_runs(workspace_id)
        if runs:
            raise ValueError("cannot detach uploads after a task run has been created")
        ts = now_iso()
        attached_upload_ids = [value for value in attached_upload_ids if value != upload_id]
        next_status = "draft" if not attached_upload_ids else workspace.get("sessionStatus", "ready")
        with self.connect() as conn:
            conn.execute(
                """
                UPDATE workspaces
                SET upload_ids_json = ?,
                    session_status = ?,
                    updated_at = ?
                WHERE id = ?
                """,
                (encode_json(attached_upload_ids), next_status, ts, workspace_id),
            )
            self._append_event_tx(
                conn,
                workspace_id,
                None,
                "upload.detached",
                {"uploadId": upload_id},
                ts,
            )
        return self.get_workspace(workspace_id)

    def list_run_artifacts(self, run_id: str) -> JsonObject:
        run = self.get_run(run_id)
        uploads = self.list_uploads(run["workspace_id"])
        with self.connect() as conn:
            rows = conn.execute(
                """
                SELECT
                  evidence_items.id AS evidence_id,
                  evidence_items.kind AS evidence_kind,
                  evidence_items.summary AS evidence_summary,
                  evidence_items.final_allowed AS final_allowed,
                  evidence_items.payload_json AS evidence_payload_json,
                  evidence_items.created_at AS evidence_created_at,
                  artifacts.id AS artifact_id,
                  artifacts.relative_path AS relative_path,
                  artifacts.sha256 AS sha256,
                  artifacts.size_bytes AS size_bytes,
                  artifacts.content_type AS content_type,
                  artifacts.schema_name AS schema_name,
                  artifacts.preview_json AS preview_json,
                  artifacts.created_at AS artifact_created_at
                FROM evidence_items
                JOIN artifacts ON artifacts.id = evidence_items.artifact_id
                WHERE evidence_items.workspace_id = ?
                  AND (evidence_items.run_id = ? OR evidence_items.run_id IS NULL)
                ORDER BY evidence_items.created_at ASC, evidence_items.rowid ASC
                """,
                (run["workspace_id"], run_id),
            ).fetchall()
        evidence_artifacts = []
        for row in rows:
            item = dict(row)
            item["final_allowed"] = bool(item["final_allowed"])
            item["evidence_payload"] = decode_json(item.pop("evidence_payload_json"), {})
            item["preview"] = decode_json(item.pop("preview_json"), {})
            evidence_artifacts.append(item)
        upload_artifacts = [
            {
                "upload_id": upload["id"],
                "filename": upload["filename"],
                "artifact_id": upload["artifact_id"],
                "relative_path": upload["artifact_relative_path"],
                "sha256": upload["artifact_sha256"],
                "size_bytes": upload["artifact_size_bytes"],
                "content_type": upload["artifact_content_type"],
                "created_at": upload["created_at"],
            }
            for upload in uploads
        ]
        support_artifacts = self._run_support_artifacts(
            run,
            evidence_artifacts,
            {item["artifact_id"] for item in upload_artifacts}
            | {item["artifact_id"] for item in evidence_artifacts},
        )
        return {
            "run": run,
            "uploads": upload_artifacts,
            "evidenceArtifacts": evidence_artifacts,
            "supportArtifacts": support_artifacts,
        }

    def _run_support_artifacts(
        self,
        run: JsonObject,
        evidence_artifacts: list[JsonObject],
        seen_artifact_ids: set[str],
    ) -> list[JsonObject]:
        specs: list[JsonObject] = []
        for item in evidence_artifacts:
            for spec in artifact_support_specs(item.get("evidence_payload")):
                spec["source_evidence_id"] = item["evidence_id"]
                spec["source_evidence_kind"] = item["evidence_kind"]
                specs.append(spec)
        if run.get("kind") == "tool_run":
            for spec in artifact_support_specs(run.get("finalAnswer")):
                spec["source_evidence_id"] = None
                spec["source_evidence_kind"] = "tool_run_result"
                specs.append(spec)

        support_artifacts: list[JsonObject] = []
        if not specs:
            return support_artifacts
        with self.connect() as conn:
            for spec in specs:
                artifact_id = spec.get("artifact_id")
                if artifact_id in seen_artifact_ids:
                    continue
                row = conn.execute(
                    """
                    SELECT id, relative_path, sha256, size_bytes, content_type,
                           schema_name, preview_json, created_at
                    FROM artifacts
                    WHERE id = ? AND workspace_id = ?
                    """,
                    (artifact_id, run["workspace_id"]),
                ).fetchone()
                if row is None:
                    continue
                seen_artifact_ids.add(artifact_id)
                item = dict(row)
                support_artifacts.append(
                    {
                        "artifact_id": item["id"],
                        "logical_path": spec.get("logical_path") or item["relative_path"],
                        "relative_path": item["relative_path"],
                        "sha256": item["sha256"],
                        "size_bytes": item["size_bytes"],
                        "content_type": item["content_type"],
                        "schema_name": item["schema_name"],
                        "preview": decode_json(item.get("preview_json"), {}),
                        "created_at": item["created_at"],
                        "role": spec.get("role"),
                        "action_id": spec.get("action_id"),
                        "source_evidence_id": spec.get("source_evidence_id"),
                        "source_evidence_kind": spec.get("source_evidence_kind"),
                    }
                )
        return support_artifacts

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

    def upsert_system_context_resource(self, record: JsonObject) -> JsonObject:
        context_id = record.get("contextId")
        if not isinstance(context_id, str) or not context_id:
            raise ValueError("system context record requires contextId")
        created_at = str(record.get("createdAt") or now_iso())
        updated_at = str(record.get("updatedAt") or now_iso())
        with self.connect() as conn:
            conn.execute(
                """
                INSERT INTO system_context_resources(id, record_json, created_at, updated_at)
                VALUES (?, ?, ?, ?)
                ON CONFLICT(id) DO UPDATE SET
                  record_json = excluded.record_json,
                  updated_at = excluded.updated_at
                """,
                (context_id, encode_json(record), created_at, updated_at),
            )
        return self.get_system_context_resource(context_id)

    def list_system_context_resources(self) -> list[JsonObject]:
        with self.connect() as conn:
            rows = conn.execute(
                """
                SELECT record_json FROM system_context_resources
                ORDER BY updated_at DESC, id ASC
                """
            ).fetchall()
        return [decode_json(row["record_json"], {}) for row in rows]

    def get_system_context_resource(self, context_id: str) -> JsonObject:
        with self.connect() as conn:
            row = conn.execute(
                "SELECT record_json FROM system_context_resources WHERE id = ?",
                (context_id,),
            ).fetchone()
        if row is None:
            raise KeyError(f"unknown context {context_id}")
        return decode_json(row["record_json"], {})

    def create_run(self, workspace_id: str) -> JsonObject:
        workspace = self.get_workspace(workspace_id)
        if workspace["status"] == "deleted":
            raise ValueError("workspace is deleted")
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
            conn.execute(
                """
                UPDATE workspaces
                SET session_status = 'ready',
                    updated_at = ?
                WHERE id = ?
                """,
                (ts, workspace_id),
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
                    """
                    SELECT * FROM runs
                    WHERE kind = 'analysis'
                    ORDER BY created_at DESC, rowid DESC
                    """
                ).fetchall()
            else:
                self.get_workspace(workspace_id)
                rows = conn.execute(
                    """
                    SELECT * FROM runs
                    WHERE workspace_id = ? AND kind = 'analysis'
                    ORDER BY created_at DESC, rowid DESC
                    """,
                    (workspace_id,),
                ).fetchall()
        result = []
        for row in rows:
            result.append(self._run_from_row(row))
        return result

    def create_tool_run(
        self,
        workspace_id: str,
        tool_id: str,
        params: JsonObject,
        upload_ids: list[str] | None = None,
    ) -> JsonObject:
        workspace = self.get_workspace(workspace_id)
        if workspace["status"] == "deleted":
            raise ValueError("workspace is deleted")
        normalized_upload_ids = upload_ids or []
        for upload_id in normalized_upload_ids:
            upload = self.get_upload_with_artifact(upload_id)
            if upload["workspace_id"] != workspace_id:
                raise ValueError(f"upload {upload_id} does not belong to workspace {workspace_id}")
        run_id = new_id("trun")
        job_id = new_id("job")
        ts = now_iso()
        budget = {"toolCalls": 1}
        with self.connect() as conn:
            conn.execute(
                """
                INSERT INTO runs(
                  id, workspace_id, kind, status, phase, budget_json, tool_id,
                  tool_params_json, tool_upload_ids_json, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    run_id,
                    workspace_id,
                    "tool_run",
                    "queued",
                    "queued",
                    encode_json(budget),
                    tool_id,
                    encode_json(params),
                    encode_json(normalized_upload_ids),
                    ts,
                    ts,
                ),
            )
            self._append_event_tx(
                conn,
                workspace_id,
                run_id,
                "tool_run.queued",
                {"toolId": tool_id, "uploadIds": normalized_upload_ids},
                ts,
            )
            self._enqueue_tool_run_tx(conn, job_id, workspace_id, run_id, ts)
        return self.get_run(run_id)

    def list_tool_runs(
        self,
        tool_id: str | None = None,
        workspace_id: str | None = None,
        limit: int = 50,
    ) -> list[JsonObject]:
        limit = max(1, min(limit, 200))
        clauses = ["kind = 'tool_run'"]
        params: list[object] = []
        if tool_id:
            clauses.append("tool_id = ?")
            params.append(tool_id)
        if workspace_id:
            self.get_workspace(workspace_id)
            clauses.append("workspace_id = ?")
            params.append(workspace_id)
        params.append(limit)
        query = (
            "SELECT * FROM runs WHERE "
            + " AND ".join(clauses)
            + " ORDER BY created_at DESC, rowid DESC LIMIT ?"
        )
        with self.connect() as conn:
            rows = conn.execute(query, tuple(params)).fetchall()
        return [self._run_from_row(row) for row in rows]

    def mark_tool_run_running(self, run_id: str) -> JsonObject:
        run = self.get_run(run_id)
        if run.get("kind") != "tool_run":
            raise ValueError(f"run {run_id} is not a tool run")
        ensure_run_not_terminal(run, "marked running")
        ts = now_iso()
        with self.connect() as conn:
            conn.execute(
                """
                UPDATE runs
                SET status = 'running', phase = 'tool_run', updated_at = ?
                WHERE id = ?
                """,
                (ts, run_id),
            )
            self._append_event_tx(
                conn,
                run["workspace_id"],
                run_id,
                "tool_run.running",
                {"toolId": run.get("toolId")},
                ts,
            )
        return self.get_run(run_id)

    def complete_tool_run(
        self,
        run_id: str,
        result_artifact_id: str,
        result: JsonObject,
    ) -> JsonObject:
        run = self.get_run(run_id)
        if run.get("kind") != "tool_run":
            raise ValueError(f"run {run_id} is not a tool run")
        ensure_run_not_terminal(run, "completed")
        ts = now_iso()
        with self.connect() as conn:
            conn.execute(
                """
                UPDATE runs
                SET status = 'succeeded', phase = 'finish', tool_result_artifact_id = ?,
                    final_answer_json = ?, error_json = NULL, updated_at = ?
                WHERE id = ?
                """,
                (result_artifact_id, encode_json(result), ts, run_id),
            )
            self._append_event_tx(
                conn,
                run["workspace_id"],
                run_id,
                "tool_run.succeeded",
                {"toolId": run.get("toolId"), "artifactId": result_artifact_id},
                ts,
            )
        return self.get_run(run_id)

    def fail_tool_run(self, run_id: str, message: str) -> JsonObject:
        run = self.get_run(run_id)
        if run.get("kind") != "tool_run":
            raise ValueError(f"run {run_id} is not a tool run")
        ensure_run_not_terminal(run, "failed")
        ts = now_iso()
        error = {"message": message[:2000]}
        with self.connect() as conn:
            conn.execute(
                """
                UPDATE runs
                SET status = 'failed', phase = 'failed', error_json = ?, updated_at = ?
                WHERE id = ?
                """,
                (encode_json(error), ts, run_id),
            )
            self._append_event_tx(
                conn,
                run["workspace_id"],
                run_id,
                "tool_run.failed",
                {"toolId": run.get("toolId"), "error": message[:2000]},
                ts,
            )
        return self.get_run(run_id)

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
        return self._run_from_row(row)

    def _run_from_row(self, row: sqlite3.Row) -> JsonObject:
        result = dict(row)
        result["budget"] = decode_json(result.pop("budget_json"), {})
        result["kind"] = result.get("kind") or "analysis"
        result["toolId"] = result.pop("tool_id", None)
        result["toolParams"] = decode_json(result.pop("tool_params_json", None), {})
        result["toolUploadIds"] = decode_json(result.pop("tool_upload_ids_json", None), [])
        result["toolResultArtifactId"] = result.pop("tool_result_artifact_id", None)
        result["finalAnswer"] = decode_json(result.pop("final_answer_json"), None)
        result["error"] = decode_json(result.pop("error_json", None), None)
        return result

    def update_run_status(
        self,
        run_id: str,
        status: str,
        phase: str,
        final_answer: JsonObject | None = None,
        alias: str | None = None,
    ) -> JsonObject:
        ts = now_iso()
        with self.connect() as conn:
            row = conn.execute(
                "SELECT id, workspace_id, status FROM runs WHERE id = ?",
                (run_id,),
            ).fetchone()
            if row is None:
                raise KeyError(f"unknown run {run_id}")
            ensure_run_not_terminal(dict(row), "updated")
            workspace_id = row["workspace_id"]
            encoded_final_answer = encode_json(final_answer) if final_answer is not None else None
            if alias is None:
                conn.execute(
                    """
                    UPDATE runs
                    SET status = ?, phase = ?, final_answer_json = ?, updated_at = ?
                    WHERE id = ?
                    """,
                    (status, phase, encoded_final_answer, ts, run_id),
                )
            else:
                conn.execute(
                    """
                    UPDATE runs
                    SET status = ?, phase = ?, final_answer_json = ?, alias = ?, updated_at = ?
                    WHERE id = ?
                    """,
                    (status, phase, encoded_final_answer, alias, ts, run_id),
                )
            conn.execute(
                """
                UPDATE workspaces
                SET session_status = ?,
                    updated_at = ?
                WHERE id = ?
                """,
                (session_status_from_run_status(status), ts, workspace_id),
            )
            self._append_event_tx(
                conn,
                workspace_id,
                run_id,
                f"run.{status}",
                {"phase": phase, "alias": alias} if alias is not None else {"phase": phase},
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
            row = conn.execute(
                "SELECT upload_ids_json FROM workspaces WHERE id = ?",
                (workspace_id,),
            ).fetchone()
            attached_upload_ids = decode_json(row["upload_ids_json"], []) if row else []
            if upload_id not in attached_upload_ids:
                attached_upload_ids.append(upload_id)
            conn.execute(
                """
                UPDATE workspaces
                SET upload_ids_json = ?,
                    session_status = CASE
                      WHEN session_status = 'draft' THEN 'ready'
                      ELSE session_status
                    END,
                    updated_at = ?
                WHERE id = ?
                """,
                (encode_json(attached_upload_ids), ts, workspace_id),
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

    def get_upload_with_artifact(self, upload_id: str) -> JsonObject:
        with self.connect() as conn:
            row = conn.execute(
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
                WHERE uploads.id = ?
                """,
                (upload_id,),
            ).fetchone()
        if row is None:
            raise KeyError(f"unknown upload {upload_id}")
        return dict(row)

    def list_uploads_by_ids(self, workspace_id: str, upload_ids: list[str]) -> list[JsonObject]:
        if not upload_ids:
            return self.list_uploads(workspace_id)
        result = []
        for upload_id in upload_ids:
            upload = self.get_upload_with_artifact(upload_id)
            if upload["workspace_id"] != workspace_id:
                raise ValueError(f"upload {upload_id} does not belong to workspace {workspace_id}")
            result.append(upload)
        return result

    def create_remote_executor(self, record: JsonObject) -> JsonObject:
        executor_id = new_id("executor")
        ts = now_iso()
        with self.connect() as conn:
            conn.execute(
                """
                INSERT INTO remote_executors(
                  id, name, host, port, user, tags_json, enabled, notes,
                  last_check_json, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    executor_id,
                    record["name"],
                    record["host"],
                    int(record["port"]),
                    record["user"],
                    encode_json(record.get("tags", [])),
                    1 if record.get("enabled", True) else 0,
                    record.get("notes"),
                    encode_json(record.get("lastCheck")) if record.get("lastCheck") else None,
                    ts,
                    ts,
                ),
            )
        return self.get_remote_executor(executor_id)

    def list_remote_executors(self) -> list[JsonObject]:
        with self.connect() as conn:
            rows = conn.execute(
                "SELECT * FROM remote_executors ORDER BY created_at DESC, id DESC"
            ).fetchall()
        return [self._remote_executor_from_row(row) for row in rows]

    def get_remote_executor(self, executor_id: str) -> JsonObject:
        with self.connect() as conn:
            row = conn.execute(
                "SELECT * FROM remote_executors WHERE id = ?", (executor_id,)
            ).fetchone()
        if row is None:
            raise KeyError(f"unknown executor {executor_id}")
        return self._remote_executor_from_row(row)

    def update_remote_executor(self, executor_id: str, updates: JsonObject) -> JsonObject:
        current = self.get_remote_executor(executor_id)
        merged = {**current, **updates}
        ts = now_iso()
        with self.connect() as conn:
            conn.execute(
                """
                UPDATE remote_executors
                SET name = ?, host = ?, port = ?, user = ?, tags_json = ?,
                    enabled = ?, notes = ?, updated_at = ?
                WHERE id = ?
                """,
                (
                    merged["name"],
                    merged["host"],
                    int(merged["port"]),
                    merged["user"],
                    encode_json(merged.get("tags", [])),
                    1 if merged.get("enabled", True) else 0,
                    merged.get("notes"),
                    ts,
                    executor_id,
                ),
            )
        return self.get_remote_executor(executor_id)

    def disable_remote_executor(self, executor_id: str) -> JsonObject:
        return self.update_remote_executor(executor_id, {"enabled": False})

    def create_remote_run(
        self,
        executor_id: str,
        command_id: str,
        alias: str,
        idempotency_key: str | None = None,
        operation: str = "command",
        input_payload: JsonObject | None = None,
    ) -> JsonObject:
        if idempotency_key:
            existing = self.find_remote_run_by_idempotency_key(idempotency_key)
            if existing is not None:
                return existing
        self.get_remote_executor(executor_id)
        normalized_operation = operation.strip() or "command"
        run_id = new_id("rrun")
        job_id = new_id("job")
        ts = now_iso()
        with self.connect() as conn:
            conn.execute(
                """
                INSERT INTO remote_runs(
                  id, executor_id, command_id, operation, input_json, idempotency_key,
                  status, phase, alias, attempts, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    run_id,
                    executor_id,
                    command_id,
                    normalized_operation,
                    encode_json(input_payload or {}),
                    idempotency_key,
                    "QUEUED",
                    "QUEUED",
                    alias,
                    0,
                    ts,
                    ts,
                ),
            )
            self._enqueue_remote_run_tx(conn, job_id, run_id, ts)
        return self.get_remote_run(run_id)

    def list_remote_runs(
        self, executor_id: str | None = None, limit: int = 50
    ) -> list[JsonObject]:
        with self.connect() as conn:
            if executor_id:
                rows = conn.execute(
                    """
                    SELECT * FROM remote_runs
                    WHERE executor_id = ?
                    ORDER BY created_at DESC, rowid DESC
                    LIMIT ?
                    """,
                    (executor_id, limit),
                ).fetchall()
            else:
                rows = conn.execute(
                    """
                    SELECT * FROM remote_runs
                    ORDER BY created_at DESC, rowid DESC
                    LIMIT ?
                    """,
                    (limit,),
                ).fetchall()
        return [self._remote_run_from_row(row) for row in rows]

    def get_remote_run(self, run_id: str) -> JsonObject:
        with self.connect() as conn:
            row = conn.execute("SELECT * FROM remote_runs WHERE id = ?", (run_id,)).fetchone()
        if row is None:
            raise KeyError(f"unknown remote run {run_id}")
        return self._remote_run_from_row(row)

    def find_remote_run_by_idempotency_key(self, idempotency_key: str) -> JsonObject | None:
        with self.connect() as conn:
            row = conn.execute(
                "SELECT * FROM remote_runs WHERE idempotency_key = ?", (idempotency_key,)
            ).fetchone()
        return self._remote_run_from_row(row) if row is not None else None

    def mark_remote_run_running(self, run_id: str, phase: str) -> JsonObject:
        ts = now_iso()
        with self.connect() as conn:
            row = conn.execute(
                "SELECT id, status, attempts FROM remote_runs WHERE id = ?", (run_id,)
            ).fetchone()
            if row is None:
                raise KeyError(f"unknown remote run {run_id}")
            ensure_remote_run_not_terminal(dict(row), "marked running")
            conn.execute(
                """
                UPDATE remote_runs
                SET status = 'RUNNING', phase = ?, attempts = ?, updated_at = ?
                WHERE id = ?
                """,
                (phase, int(row["attempts"]) + 1, ts, run_id),
            )
        return self.get_remote_run(run_id)

    def complete_remote_run(self, run_id: str, result: JsonObject) -> JsonObject:
        remote_run = self.get_remote_run(run_id)
        ensure_remote_run_not_terminal(remote_run, "completed")
        ts = now_iso()
        with self.connect() as conn:
            conn.execute(
                """
                UPDATE remote_runs
                SET status = 'SUCCEEDED', phase = 'FINISHED', result_json = ?,
                    error_json = NULL, updated_at = ?
                WHERE id = ?
                """,
                (encode_json(result), ts, run_id),
            )
        return self.get_remote_run(run_id)

    def fail_remote_run(self, run_id: str, phase: str, message: str) -> JsonObject:
        remote_run = self.get_remote_run(run_id)
        ensure_remote_run_not_terminal(remote_run, "failed")
        ts = now_iso()
        error = {"phase": phase, "message": message[:2000]}
        with self.connect() as conn:
            conn.execute(
                """
                UPDATE remote_runs
                SET status = 'FAILED', phase = ?, error_json = ?, updated_at = ?
                WHERE id = ?
                """,
                (phase, encode_json(error), ts, run_id),
            )
        return self.get_remote_run(run_id)

    def _remote_executor_from_row(self, row: sqlite3.Row) -> JsonObject:
        item = dict(row)
        return {
            "executorId": item["id"],
            "name": item["name"],
            "host": item["host"],
            "port": item["port"],
            "user": item["user"],
            "tags": decode_json(item["tags_json"], []),
            "enabled": bool(item["enabled"]),
            "notes": item.get("notes"),
            "lastCheck": decode_json(item.get("last_check_json"), None),
            "createdAt": item["created_at"],
            "updatedAt": item["updated_at"],
        }

    def _remote_run_from_row(self, row: sqlite3.Row) -> JsonObject:
        item = dict(row)
        return {
            "taskId": item["id"],
            "alias": item.get("alias"),
            "taskKind": "remote_command_run",
            "status": item["status"],
            "phase": item.get("phase"),
            "attempts": item["attempts"],
            "remoteExecutorId": item["executor_id"],
            "remoteCommandId": item["command_id"],
            "operation": item.get("operation") or "command",
            "input": decode_json(item.get("input_json"), {}),
            "idempotencyKey": item.get("idempotency_key"),
            "result": decode_json(item.get("result_json"), None),
            "error": decode_json(item.get("error_json"), None),
            "createdAt": item["created_at"],
            "updatedAt": item["updated_at"],
        }

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

    def list_actions(self, run_id: str) -> list[JsonObject]:
        self.get_run(run_id)
        with self.connect() as conn:
            rows = conn.execute(
                """
                SELECT * FROM actions
                WHERE run_id = ?
                ORDER BY created_at ASC, id ASC
                """,
                (run_id,),
            ).fetchall()
        actions = []
        for row in rows:
            item = dict(row)
            item["payload"] = decode_json(item.pop("payload_json"), {})
            item["result"] = decode_json(item.pop("result_json"), None)
            actions.append(item)
        return actions

    def decide_action(
        self,
        action_id: str,
        decision: str,
        reason: str | None,
        idempotency_key: str | None = None,
        input_override: JsonObject | None = None,
    ) -> JsonObject:
        action = self.get_action(action_id)
        run = self.get_run(action["run_id"])
        ts = now_iso()
        payload = dict(action.get("payload") or {})
        result = {
            "decision": decision,
            "reason": reason,
            "idempotencyKey": idempotency_key,
        }
        event_payload = {
            "actionId": action_id,
            "decision": decision,
            "reason": reason,
            "idempotencyKey": idempotency_key,
        }
        if input_override is not None:
            payload["input"] = input_override
            result["input"] = input_override
            event_payload["input"] = input_override
        with self.connect() as conn:
            if input_override is not None:
                conn.execute(
                    """
                    UPDATE actions
                    SET status = ?, payload_json = ?, result_json = ?, updated_at = ?
                    WHERE id = ?
                    """,
                    (decision, encode_json(payload), encode_json(result), ts, action_id),
                )
            else:
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
                event_payload,
                ts,
            )
        return self.get_action(action_id)

    def answer_user_input_actions(
        self,
        run_id: str,
        message: str,
        resume_mode: str,
        question_id: str | None = None,
    ) -> list[JsonObject]:
        run = self.get_run(run_id)
        ts = now_iso()
        result = {"message": message, "resumeMode": resume_mode, "questionId": question_id}
        with self.connect() as conn:
            rows = conn.execute(
                """
                SELECT id, payload_json FROM actions
                WHERE run_id = ? AND kind = 'user_input' AND status = 'pending'
                ORDER BY created_at ASC, id ASC
                """,
                (run_id,),
            ).fetchall()
            action_ids = [
                row["id"]
                for row in rows
                if question_id is None
                or row["id"] == question_id
                or decode_json(row["payload_json"], {}).get("questionId") == question_id
            ]
            for action_id in action_ids:
                conn.execute(
                    """
                    UPDATE actions
                    SET status = 'answered', result_json = ?, updated_at = ?
                    WHERE id = ?
                    """,
                    (encode_json(result), ts, action_id),
                )
            if action_ids:
                self._append_event_tx(
                    conn,
                    run["workspace_id"],
                    run_id,
                    "action.user_input.answered",
                    {
                        "actionIds": action_ids,
                        "questionId": question_id,
                        "resumeMode": resume_mode,
                    },
                    ts,
                )
        return [self.get_action(action_id) for action_id in action_ids]

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

    def list_workspace_timeline(self, workspace_id: str) -> list[JsonObject]:
        self.get_workspace(workspace_id)
        with self.connect() as conn:
            rows = conn.execute(
                """
                SELECT * FROM timeline_events
                WHERE workspace_id = ?
                ORDER BY created_at ASC, id ASC
                """,
                (workspace_id,),
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
        messages: list[JsonObject] | None = None,
    ) -> JsonObject:
        import_id = new_id("caseimp")
        ts = now_iso()
        with self.connect() as conn:
            conn.execute(
                """
                INSERT INTO case_imports(
                  import_id, status, filename, source_text, draft_json,
                  validation_errors_json, messages_json, case_id, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    import_id,
                    "previewed",
                    filename,
                    source_text,
                    encode_json(draft),
                    encode_json(validation_errors),
                    encode_json(messages or []),
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
        messages: list[JsonObject] | None = None,
        source_text: str | None = None,
    ) -> JsonObject:
        current = self.get_case_import(import_id)
        ts = now_iso()
        with self.connect() as conn:
            cursor = conn.execute(
                """
                UPDATE case_imports
                SET status = ?,
                    source_text = ?,
                    draft_json = ?,
                    validation_errors_json = ?,
                    messages_json = ?,
                    case_id = ?,
                    updated_at = ?
                WHERE import_id = ?
                """,
                (
                    status,
                    source_text if source_text is not None else current["sourceText"],
                    encode_json(draft if draft is not None else current["draft"]),
                    encode_json(
                        validation_errors
                        if validation_errors is not None
                        else current["validationErrors"]
                    ),
                    encode_json(messages if messages is not None else current["messages"]),
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
        item["messages"] = decode_json(item.pop("messages_json"), [])
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
        follow_redirects: bool = False,
        schema_version: int = 2,
        refresh_policy: JsonObject | None = None,
    ) -> JsonObject:
        endpoint_id = new_id("fetch")
        ts = now_iso()
        with self.connect() as conn:
            conn.execute(
                """
                INSERT INTO fetch_endpoints(
                  id, schema_version, name, method, url, headers_json, body, enabled,
                  follow_redirects, refresh_policy_json, created_at, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    endpoint_id,
                    schema_version,
                    name,
                    method,
                    url,
                    encode_json(headers),
                    body,
                    1 if enabled else 0,
                    1 if follow_redirects else 0,
                    encode_json(refresh_policy or DEFAULT_FETCH_REFRESH_POLICY),
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
                    enabled = ?, follow_redirects = ?, schema_version = ?,
                    refresh_policy_json = ?, updated_at = ?
                WHERE id = ?
                """,
                (
                    merged["name"],
                    merged["method"],
                    merged["url"],
                    encode_json(merged.get("headers", {})),
                    merged.get("body"),
                    1 if merged.get("enabled", True) else 0,
                    1 if merged.get("followRedirects", False) else 0,
                    int(merged.get("schemaVersion") or 2),
                    encode_json(merged.get("refreshPolicy") or DEFAULT_FETCH_REFRESH_POLICY),
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
        item["schemaVersion"] = int(item.pop("schema_version", 2) or 2)
        item["headers"] = decode_json(item.pop("headers_json"), {})
        item["enabled"] = bool(item["enabled"])
        item["followRedirects"] = bool(item.pop("follow_redirects", 0))
        item["refreshPolicy"] = decode_json(
            item.pop("refresh_policy_json", None),
            dict(DEFAULT_FETCH_REFRESH_POLICY),
        )
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

    def recover_interrupted_jobs(self) -> JsonObject:
        ts = now_iso()
        summary = {
            "runAnalysisRequeued": 0,
            "runAnalysisCompleted": 0,
            "toolRunsRequeued": 0,
            "toolRunsCompleted": 0,
            "remoteRunsRequeued": 0,
            "remoteRunsCompleted": 0,
            "failedJobs": 0,
        }
        with self.connect() as conn:
            rows = conn.execute(
                """
                SELECT * FROM jobs
                WHERE status = 'running'
                ORDER BY updated_at ASC, id ASC
                """
            ).fetchall()
            for row in rows:
                payload = decode_json(row["payload_json"], {})
                if row["kind"] == "run_analysis":
                    self._recover_run_analysis_job_tx(conn, row, payload, ts, summary)
                elif row["kind"] == "tool_run":
                    self._recover_tool_run_job_tx(conn, row, payload, ts, summary)
                elif row["kind"] == "remote_command_run":
                    self._recover_remote_run_job_tx(conn, row, payload, ts, summary)
                else:
                    self._mark_job_failed_tx(
                        conn,
                        row["id"],
                        "unknown interrupted job kind",
                        ts,
                    )
                    summary["failedJobs"] += 1
        return summary

    def _recover_run_analysis_job_tx(
        self,
        conn: sqlite3.Connection,
        row: sqlite3.Row,
        payload: JsonObject,
        ts: str,
        summary: JsonObject,
    ) -> None:
        run_id = payload.get("run_id")
        if not isinstance(run_id, str):
            self._mark_job_failed_tx(conn, row["id"], "run_analysis job missing run_id", ts)
            summary["failedJobs"] += 1
            return
        run = conn.execute(
            "SELECT id, workspace_id, status FROM runs WHERE id = ?",
            (run_id,),
        ).fetchone()
        if run is None:
            self._mark_job_failed_tx(conn, row["id"], f"unknown run {run_id}", ts)
            summary["failedJobs"] += 1
            return
        if run["status"] in {"succeeded", "failed", "waiting_for_user", "waiting_for_approval"}:
            self._mark_job_succeeded_tx(conn, row["id"], ts)
            summary["runAnalysisCompleted"] += 1
            return
        conn.execute(
            """
            UPDATE runs
            SET status = 'queued', phase = 'queued', final_answer_json = NULL, updated_at = ?
            WHERE id = ?
            """,
            (ts, run_id),
        )
        self._mark_job_queued_tx(conn, row["id"], ts)
        self._append_event_tx(
            conn,
            run["workspace_id"],
            run_id,
            "run.recovered",
            {"jobId": row["id"], "previousStatus": run["status"]},
            ts,
        )
        summary["runAnalysisRequeued"] += 1

    def _recover_tool_run_job_tx(
        self,
        conn: sqlite3.Connection,
        row: sqlite3.Row,
        payload: JsonObject,
        ts: str,
        summary: JsonObject,
    ) -> None:
        run_id = payload.get("run_id")
        if not isinstance(run_id, str):
            self._mark_job_failed_tx(conn, row["id"], "tool_run job missing run_id", ts)
            summary["failedJobs"] += 1
            return
        run = conn.execute(
            "SELECT id, workspace_id, status, kind FROM runs WHERE id = ?",
            (run_id,),
        ).fetchone()
        if run is None:
            self._mark_job_failed_tx(conn, row["id"], f"unknown tool run {run_id}", ts)
            summary["failedJobs"] += 1
            return
        if run["kind"] != "tool_run":
            self._mark_job_failed_tx(conn, row["id"], f"run {run_id} is not a tool run", ts)
            summary["failedJobs"] += 1
            return
        if run["status"] in {"succeeded", "failed"}:
            self._mark_job_succeeded_tx(conn, row["id"], ts)
            summary["toolRunsCompleted"] += 1
            return
        conn.execute(
            """
            UPDATE runs
            SET status = 'queued', phase = 'queued', updated_at = ?
            WHERE id = ?
            """,
            (ts, run_id),
        )
        self._mark_job_queued_tx(conn, row["id"], ts)
        self._append_event_tx(
            conn,
            run["workspace_id"],
            run_id,
            "tool_run.recovered",
            {"jobId": row["id"], "previousStatus": run["status"]},
            ts,
        )
        summary["toolRunsRequeued"] += 1

    def _recover_remote_run_job_tx(
        self,
        conn: sqlite3.Connection,
        row: sqlite3.Row,
        payload: JsonObject,
        ts: str,
        summary: JsonObject,
    ) -> None:
        run_id = payload.get("run_id")
        if not isinstance(run_id, str):
            self._mark_job_failed_tx(conn, row["id"], "remote job missing run_id", ts)
            summary["failedJobs"] += 1
            return
        remote_run = conn.execute(
            "SELECT id, status FROM remote_runs WHERE id = ?",
            (run_id,),
        ).fetchone()
        if remote_run is None:
            self._mark_job_failed_tx(conn, row["id"], f"unknown remote run {run_id}", ts)
            summary["failedJobs"] += 1
            return
        if remote_run["status"] in {"SUCCEEDED", "FAILED"}:
            self._mark_job_succeeded_tx(conn, row["id"], ts)
            summary["remoteRunsCompleted"] += 1
            return
        conn.execute(
            """
            UPDATE remote_runs
            SET status = 'QUEUED', phase = 'QUEUED', updated_at = ?
            WHERE id = ?
            """,
            (ts, run_id),
        )
        self._mark_job_queued_tx(conn, row["id"], ts)
        summary["remoteRunsRequeued"] += 1

    def _mark_job_queued_tx(self, conn: sqlite3.Connection, job_id: str, ts: str) -> None:
        conn.execute(
            """
            UPDATE jobs
            SET status = 'queued', locked_by = NULL, locked_until = NULL,
                next_run_at = ?, updated_at = ?
            WHERE id = ?
            """,
            (ts, ts, job_id),
        )

    def _mark_job_succeeded_tx(self, conn: sqlite3.Connection, job_id: str, ts: str) -> None:
        conn.execute(
            """
            UPDATE jobs
            SET status = 'succeeded', locked_by = NULL, locked_until = NULL, updated_at = ?
            WHERE id = ?
            """,
            (ts, job_id),
        )

    def _mark_job_failed_tx(
        self, conn: sqlite3.Connection, job_id: str, error: str, ts: str
    ) -> None:
        conn.execute(
            """
            UPDATE jobs
            SET status = 'failed', locked_by = NULL, locked_until = NULL,
                last_error = ?, updated_at = ?
            WHERE id = ?
            """,
            (error[:2000], ts, job_id),
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

    def _enqueue_remote_run_tx(
        self,
        conn: sqlite3.Connection,
        job_id: str,
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
                "remote_command_run",
                "queued",
                encode_json({"run_id": run_id}),
                0,
                1,
                ts,
                ts,
                ts,
            ),
        )

    def _enqueue_tool_run_tx(
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
                "tool_run",
                "queued",
                encode_json({"workspace_id": workspace_id, "run_id": run_id}),
                0,
                2,
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
