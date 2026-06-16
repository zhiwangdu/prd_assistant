# LogAgent V2 Server

`server-v2` is a clean-room Python implementation branch for the small-team
LogAgent redesign. It does not preserve the Rust Server API surface. The first
slice provides the durable foundation for the V2 product model:

- FastAPI HTTP service.
- SQLite WAL storage under one local data directory.
- Local artifact storage for uploads and future evidence files.
- DB-backed job queue for restartable background runs.
- Workspace, Run, TimelineEvent, Evidence, Artifact, Upload, Action, and Job
  schema foundations.
- Read-only MCP discovery placeholder.
- Stub agent runtime that exercises the lifecycle before LangGraph model
  reasoning and tool execution are wired in.

## Local Run

```bash
cd server-v2
python3 -m venv .venv
. .venv/bin/activate
pip install -e ".[dev]"
export LOGAGENT_V2_API_KEY=dev-token
python -m logagent_v2 init-db
python -m logagent_v2 server
```

Default URL:

```text
http://127.0.0.1:50993
```

Health is public:

```bash
curl http://127.0.0.1:50993/health
```

Protected APIs require:

```text
Authorization: Bearer <api-key>
```

## Configuration

Environment variables:

| Variable | Default | Purpose |
|---|---:|---|
| `LOGAGENT_V2_DATA_DIR` | `/tmp/logagent-v2` | SQLite, artifacts, and temp data |
| `LOGAGENT_V2_API_KEY` | `dev-token` | Bearer token for protected APIs |
| `LOGAGENT_V2_HOST` | `127.0.0.1` | Server bind host |
| `LOGAGENT_V2_PORT` | `50993` | Server bind port |
| `LOGAGENT_V2_MAX_UPLOAD_BYTES` | `536870912` | Per-upload limit |
| `LOGAGENT_V2_MAX_CONCURRENT_JOBS` | `2` | Inline worker concurrency |
| `LOGAGENT_V2_INLINE_WORKER` | `1` | Run worker inside API process |

## Current API

```http
GET  /health
POST /api/v2/workspaces
GET  /api/v2/workspaces
GET  /api/v2/workspaces/:workspace_id
POST /api/v2/workspaces/:workspace_id/uploads
POST /api/v2/workspaces/:workspace_id/runs
GET  /api/v2/runs/:run_id
GET  /api/v2/runs/:run_id/timeline
POST /api/v2/runs/:run_id/messages
POST /api/v2/actions/:action_id/decisions
GET  /api/v2/evidence/:evidence_id
GET  /api/v2/artifacts/:artifact_id
GET  /api/v2/tools
POST /api/v2/mcp/readonly
```

## Verification

```bash
python3 -m compileall logagent_v2
PYTHONPATH=. python3 -m unittest discover tests
```

This V2 slice intentionally does not yet migrate the V1 log analyzer, Tool
Runner, Metadata, Skills, Memory, or full LangGraph model loop.

