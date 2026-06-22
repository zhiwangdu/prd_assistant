# Development Progress

Last updated: 2026-06-22

Historical append-only entries were archived to
[`docs/archive/PROGRESS-history-2026-06-22.md`](./docs/archive/PROGRESS-history-2026-06-22.md).
Keep this file focused on current state, recent changes and near-term next
steps.

## Current State

- Active Server implementation: `server-v2/` Python/FastAPI.
- Persistence: SQLite WAL, local upload/workspace/artifact directories and
  DB-backed jobs.
- WebUI: React/Vite/Tailwind static build served by V2 Server.
- Agent Provider Runtime: `stub` default, optional `openai_compatible`,
  `binary` and `claude_code`.
- Execution boundaries: Server owns Tool Runner, Fetch, Metadata, System
  Context, Case Memory, Code Evidence and Remote Executor.
- Old Rust `server/` crate is not part of the V2 branch runtime. Rust checks
  apply only when changing remaining Rust components.

## Recent Changes

### 2026-06-22 Review Debt Remediation

- Confirmed review findings around large V2 Server files, WebUI helper
  duplication, testing docs/fixture drift and oversized `PROGRESS.md`.
- Added shared WebUI helpers:
  - `webui/src/errors.ts` for `errorMessage`.
  - `webui/src/polling.ts` for interval setup/cleanup.
  - `webui/src/native-agent.ts` for the local Native Agent endpoint.
- Removed 14 duplicated WebUI `errorMessage` helpers and centralized visible
  interval setup through the polling helper.
- Updated `Store` to reuse a guarded SQLite connection per Store instance,
  close it during FastAPI lifespan shutdown and record schema version metadata
  through `PRAGMA user_version` plus `schema_migrations`.
- Added focused Store maintenance tests for connection reuse and schema version
  recording.
- Rewrote `testing/README.md` and `testing/SPEC.md` so V2 Server checks use
  ruff/pytest instead of obsolete cargo requirements.
- Added minimal landed fixtures for `redis_timeout`, `influxql_slow_query` and
  `environment_disk_full`.
- Archived the former append-only `PROGRESS.md` history and reset the root
  progress file to current-state tracking.
- Verification passed:
  - `git diff --check`
  - `server-v2/.venv/bin/python -m ruff check server-v2/logagent_v2 server-v2/tests`
  - `server-v2/.venv/bin/python -m pytest -q server-v2/tests/test_store_maintenance.py`
  - `server-v2/.venv/bin/python -m pytest -q server-v2/tests`
  - `cd webui && npm run lint`
  - `cd webui && npm run typecheck`
  - `cd webui && npm run build`

## Known Debt

- `server-v2/logagent_v2/api.py` still needs APIRouter-based splitting by
  domain. Keep `/api/v2` as canonical and register `/api/*` compatibility
  aliases only where needed.
- `server-v2/logagent_v2/store.py` still mixes schema DDL, migrations, DAO,
  queue and recovery logic. Connection lifecycle and schema version metadata
  are improved, but DAO/module separation remains.
- `server-v2/tests/test_store.py` is still oversized and includes substantial
  compatibility coverage. New tests should go into focused files by domain.
- Some WebUI bridge components remain large. Continue extracting pure helpers,
  data adapters and small presentational components without changing the V2
  UX surface.

## Next Steps

- Split V2 API into domain routers: workspaces/sessions, uploads, runs/tasks,
  tools/fetch, executors, metadata, system context, memory, settings and MCP.
- Split Store into schema/migrations plus focused repositories while preserving
  the current JSON contracts.
- Add a fixture runner that consumes `testing/fixtures/*/expected*.json`.
- Move V1 compatibility tests out of `test_store.py` into focused compatibility
  files, then reduce duplicate setup.
