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
- Task MCP endpoint with summary/evidence/manifest/grep resources and
  `logagent.search_logs` follow-up search plus `logagent.get_log_slice`.
- Minimal configured Tool Runner exposed through `/api/v2/tools` and task MCP
  `logagent.run_domain_tool`.
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
| `LOGAGENT_V2_TOOLS_JSON` | unset | JSON array of fixed whitelist tool descriptors |

Tool descriptor example:

```json
[
  {
    "id": "mock_tool",
    "displayName": "Mock Tool",
    "command": "/usr/bin/mock-tool",
    "args": ["--json"],
    "enabled": true,
    "timeoutSeconds": 30,
    "maxOutputBytes": 1048576
  }
]
```

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
POST /api/v2/mcp/task/:run_id
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

## MCP

The V2 task MCP endpoint is available at:

```http
POST /api/v2/mcp/task/:run_id
```

It currently supports:

- `initialize`
- `resources/list`
- `resources/read` for `summary`, `evidence`, `manifest`, and `grep_results`
- `tools/list`
- `tools/call logagent.search_logs`
- `tools/call logagent.get_log_slice`
- `tools/call logagent.run_domain_tool`

Follow-up searches write a `log_search` evidence row and return stable refs:

```text
log_searches/<search_id>.json#matches/<index>
```

Log slices write a `log_slice` evidence row and return:

```text
log_slices/<slice_id>.json#lines
```

Configured tools can only be invoked by `toolId`; the model cannot provide an
executable path, shell command, or argv. Tool stdout is parsed as JSON when
possible and persisted as `tool_result` evidence.
