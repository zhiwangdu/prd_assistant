# LogAgent V2 Server Spec

## Goal

V2 is a clean-room small-team implementation of LogAgent. It favors a simple
single-machine deployment over distributed infrastructure:

- Python + FastAPI for the API.
- LangGraph-oriented Agent runtime.
- SQLite WAL for durable state.
- Local filesystem artifacts for large evidence.
- DB-backed jobs instead of Redis.

V2 does not need to be compatible with the current Rust Server API or artifact
layout. The stable product goal remains evidence-backed diagnosis with an
auditable agent boundary.

## Product Model

- `Workspace`: top-level diagnosis container.
- `Run`: one Agent execution inside a Workspace.
- `TimelineEvent`: append-only product event stream.
- `Evidence`: typed fact or background item.
- `Artifact`: large file tracked by DB metadata and content hash.
- `Upload`: user-provided file attached to a Workspace.
- `Action`: Agent-requested operation that may require approval.
- `Job`: persistent background work item.

## Current Implementation

Implemented in this slice:

- FastAPI app and public `GET /health`.
- Bearer auth for `/api/v2/*`.
- SQLite schema creation with WAL.
- Workspace creation/list/read.
- Upload storage as local artifacts.
- Run creation and queued `run_analysis` job.
- Inline DB-backed worker.
- Initial evidence pipeline for uploaded text files and supported archives.
- `manifest.json` and `grep_results.json` artifact generation.
- Stub Agent runtime that records initial question evidence, consumes the
  initial evidence pipeline, and returns a low-confidence evidence summary.
- Timeline events for workspace, upload, run, and evidence lifecycle.
- Artifact download.
- Evidence listing for a run.
- Read-only MCP placeholder with `initialize`, `resources/list`,
  `resources/read`, `tools/list`, and `tools/call logagent.list_tools`.
- Task MCP endpoint with `summary`, `evidence`, `manifest`, and `grep_results`
  resources.
- Task MCP `logagent.search_logs`, which creates follow-up `log_search`
  evidence and stable `log_searches/<search_id>.json#matches/<index>` refs.

Not yet implemented:

- V1-compatible node log package preprocessing and log slicing.
- LangGraph provider integration.
- Additional task MCP tools such as `get_log_slice`, `run_domain_tool`,
  Metadata, Case recall, user prompt, and approval.
- Tool Runner execution.
- Metadata import/query.
- Skill-backed System Context.
- Case Memory.
- WebUI V2 cutover.

## API

Protected endpoints use:

```text
Authorization: Bearer <api-key>
```

Current V2 endpoints:

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

## Storage

Default data layout:

```text
/tmp/logagent-v2/
  logagent.sqlite
  artifacts/
    <workspace_id>/
      <artifact_file_id>/
        <filename>
  tmp/
```

SQLite tables:

- `workspaces`
- `runs`
- `timeline_events`
- `artifacts`
- `uploads`
- `evidence_items`
- `actions`
- `jobs`

The database stores state and bounded previews. Large payloads live in artifact
files and are referenced by `relative_path`, `sha256`, and size.

## Initial Evidence Pipeline

Run execution currently performs:

```text
Workspace uploads
  -> safe archive scan / text file collection
  -> manifest.json artifact
  -> bounded keyword grep
  -> grep_results.json artifact
  -> manifest and log_search evidence
  -> low-confidence stub final answer
```

Supported archive formats are `.zip`, `.tar`, `.tar.gz`, and `.tgz`. Archive
members are never extracted by path. V2 normalizes member names and rejects
absolute paths, `..` traversal, empty paths, and other unsafe names. Non-file
members and symlinks are skipped. Text files are bounded by
`LOGAGENT_V2_MAX_TEXT_FILE_BYTES`, aggregate scanned bytes by
`LOGAGENT_V2_MAX_ARCHIVE_BYTES`, and initial matches by
`LOGAGENT_V2_MAX_GREP_MATCHES`.

Initial grep refs use:

```text
grep_results.json#matches/<index>
```

These refs are current-task evidence. Manifest evidence is background and not
final evidence.

Follow-up task MCP searches use:

```text
log_searches/<search_id>.json#matches/<index>
```

Each follow-up search persists a `log_search` evidence item and a JSON artifact.

## Security

- API key is read from `LOGAGENT_V2_API_KEY`.
- Artifact paths are resolved relative to `data_dir` and rejected if they
  escape it.
- Upload filenames are basename-normalized and character-filtered.
- Archive entries are scanned in memory and rejected if they contain absolute
  paths or traversal components.
- Agent runtime cannot execute tools directly in this slice.
- Future tools must execute only through configured whitelist descriptors.

## Acceptance

The current slice is accepted when:

- `python3 -m compileall logagent_v2` passes.
- `PYTHONPATH=. python3 -m unittest discover tests` passes.
- A Workspace can be created, an upload stored, a Run queued, and the inline
  worker can complete the stub Agent result.
