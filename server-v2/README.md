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
- Initial evidence pipeline for uploaded text files and supported archives.
- `manifest.json` and `grep_results.json` artifact generation.
- Read-only MCP discovery placeholder.
- Stub agent runtime that exercises the lifecycle before LangGraph model
  reasoning and tool execution are wired in. The stub now summarizes real
  initial grep evidence when uploads are present.

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
| `LOGAGENT_V2_MAX_ARCHIVE_FILES` | `2000` | Maximum files scanned per archive |
| `LOGAGENT_V2_MAX_ARCHIVE_BYTES` | `268435456` | Maximum aggregate extracted text bytes |
| `LOGAGENT_V2_MAX_TEXT_FILE_BYTES` | `16777216` | Maximum single text file size |
| `LOGAGENT_V2_MAX_GREP_MATCHES` | `500` | Maximum initial grep matches |
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
GET  /api/v2/runs/:run_id/evidence
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

## Initial Evidence Pipeline

When a run starts, V2 now reads all uploads attached to the Workspace and:

- accepts plain `.log`, `.txt`, `.out`, `.err`, `.trace`, `.json`, `.jsonl`,
  `.yaml`, `.yml`, `.conf`, and `.cfg` files;
- scans `.zip`, `.tar`, `.tar.gz`, and `.tgz` packages without writing archive
  members to arbitrary filesystem paths;
- rejects absolute paths, `..` path traversal, and unsafe archive entries;
- skips symlinks and non-file archive members;
- writes bounded `manifest.json` and `grep_results.json` artifacts;
- records `manifest` and `log_search` evidence items; and
- lets the current stub Agent final answer reference
  `grep_results.json#matches/<index>` when matches exist.
