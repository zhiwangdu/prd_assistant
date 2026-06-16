from __future__ import annotations

import json
import sqlite3
from contextlib import contextmanager
from datetime import UTC, datetime, timedelta
from pathlib import Path
from typing import Any, Iterator

from .ids import new_id


JsonObject = dict[str, Any]


def now_iso() -> str:
    return datetime.now(UTC).replace(microsecond=0).isoformat()


def encode_json(value: Any) -> str:
    return json.dumps(value, ensure_ascii=True, separators=(",", ":"))


def decode_json(value: str | None, default: Any = None) -> Any:
    if value is None:
        return default
    return json.loads(value)


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

                CREATE INDEX IF NOT EXISTS idx_runs_workspace_id ON runs(workspace_id);
                CREATE INDEX IF NOT EXISTS idx_events_workspace_run
                  ON timeline_events(workspace_id, run_id, created_at);
                CREATE INDEX IF NOT EXISTS idx_jobs_sched
                  ON jobs(status, next_run_at, locked_until);
                """
            )

    def create_workspace(self, question: str, mode: str, language: str) -> JsonObject:
        workspace_id = new_id("ws")
        ts = now_iso()
        with self.connect() as conn:
            conn.execute(
                """
                INSERT INTO workspaces(id, question, mode, language, status, created_at, updated_at)
                VALUES (?, ?, ?, ?, ?, ?, ?)
                """,
                (workspace_id, question, mode, language, "active", ts, ts),
            )
            self._append_event_tx(
                conn,
                workspace_id,
                None,
                "workspace.created",
                {"question": question, "mode": mode, "language": language},
                ts,
            )
        return self.get_workspace(workspace_id)

    def list_workspaces(self) -> list[JsonObject]:
        with self.connect() as conn:
            rows = conn.execute(
                "SELECT * FROM workspaces ORDER BY created_at DESC, id DESC"
            ).fetchall()
        return [dict(row) for row in rows]

    def get_workspace(self, workspace_id: str) -> JsonObject:
        with self.connect() as conn:
            row = conn.execute("SELECT * FROM workspaces WHERE id = ?", (workspace_id,)).fetchone()
        if row is None:
            raise KeyError(f"unknown workspace {workspace_id}")
        return dict(row)

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
                ORDER BY created_at ASC, id ASC
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
