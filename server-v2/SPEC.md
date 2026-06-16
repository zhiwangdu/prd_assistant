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
- Node log package preprocessing for
  `<packageId>_<instanceId>_<nodeId>_<timestamp>_logs.tar.gz`; supported log
  directories are classified into stable `extracted/<node>/<timestamp>/<group>`
  paths, and gzip-rotated files are decoded by magic bytes.
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
- Task MCP `logagent.get_log_slice`, which reads bounded context from a current
  Workspace text path and persists `log_slice` evidence.
- Minimal configured Tool Runner. Tools are loaded from
  `LOGAGENT_V2_TOOLS_JSON`, listed through `/api/v2/tools`, and runnable through
  task MCP `logagent.run_domain_tool`.
- Waiting-state foundation through task MCP `logagent.request_user_input` and
  `logagent.request_approval`; pending actions are persisted and user
  message/approval APIs can requeue the run.
- Final answer schema normalization and evidence ref validation. A run can only
  be marked `succeeded` after final refs point to current-run, final-allowed
  log search, log slice, or tool finding evidence.
- Metadata foundation with direct JSON/YAML/openGemini content import, SQLite
  snapshot storage, field/tag type query APIs, readonly MCP tools, and task MCP
  background slices.

Not yet implemented:

- V1-compatible analyzer materialized `tool_inputs/index.json` generation.
- LangGraph provider integration.
- Rich Tool Runner input matching, per-tool params schema, Case
  recall, and full multi-round model reasoning after resume.
- Metadata preview/confirm flow, URL fetch, task context auto-selection, and
  WebUI cutover.
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
GET  /api/v2/metadata/instances
GET  /api/v2/metadata/instances/:instance_id
GET  /api/v2/metadata/instances/:instance_id/snapshot
DELETE /api/v2/metadata/instances/:instance_id
POST /api/v2/metadata/imports
POST /api/v2/metadata/field-types
POST /api/v2/metadata/tag-fields
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
- `metadata_instances`

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

Node log packages named
`<packageId>_<instanceId>_<nodeId>_<timestamp>_logs.tar.gz` or `.tgz` are a
special tar scan mode. Archive members can live below a wrapper directory; V2
searches normalized path components for supported log roots:

```text
var/chroot/gemini/log/tsdb
var/chroot/gemini/log/stream
home/Ruby/log
```

Files under those roots are accepted regardless of suffix, decoded as gzip when
the bytes start with gzip magic, and exposed in manifest/search paths as:

```text
extracted/<nodeId>/<timestamp>/tsdb/<relative-file>
extracted/<nodeId>/<timestamp>/stream/<relative-file>
extracted/<nodeId>/<timestamp>/agent/<relative-file>
```

Each manifest file entry records `originalPath`, `logGroup`, and `nodePackage`
metadata. A matching node package with no supported log directories is rejected
with a clear error so an empty manifest is not treated as a successful import.

Follow-up task MCP searches use:

```text
log_searches/<search_id>.json#matches/<index>
```

Each follow-up search persists a `log_search` evidence item and a JSON artifact.

Log slice refs use:

```text
log_slices/<slice_id>.json#lines
```

`logagent.get_log_slice` only reads paths that are available from the current
Workspace's uploaded text files or supported archive members.

Configured Tool Runner execution:

```text
LOGAGENT_V2_TOOLS_JSON
  -> /api/v2/tools descriptor
  -> MCP logagent.run_domain_tool { toolId }
  -> fixed absolute command + fixed args
  -> tool_result artifact/evidence
```

The model cannot submit executable paths, shell snippets, dynamic argv, or
environment overrides.

## Metadata

V2 stores imported Metadata snapshots in SQLite table `metadata_instances`.
Each row is keyed by `instance_id` and contains `template_type`, optional
`remark`, normalized `snapshot_json`, and original `raw_json`.

Current direct import request:

```json
{
  "instanceId": "prod-og-1",
  "templateType": "opengemini",
  "content": "{...}",
  "remark": "optional display name"
}
```

`templateType=json` normalizes generic `instance` / `cluster` / `nodes` /
`databases` content. `templateType=yaml` uses PyYAML. `templateType=opengemini`
normalizes `MetaNodes`, `DataNodes`, `SqlNodes`, `Databases`, retention
policies, measurements, and schema field types. Field type mapping follows the
existing openGemini labels:

```text
0 Unknown
1 Integer
2 Unsigned
3 Float
4 String
5 Boolean
6 Tag
7 Unknown
```

Readonly MCP resources/tools expose imported instance lists, snapshots, field
type lookups, and tag-field lookups. Task MCP exposes the same tools and writes
results as `metadata_slice` evidence with `final_allowed=false`; Metadata is
background context and cannot be cited by final answers as root-cause evidence.

## Final Answer Validation

Final answers must be JSON objects with a non-empty `summary`, string arrays for
`symptoms`, `nextChecks`, `fixSuggestions`, and `missingInformation`,
`likelyRootCauses[]` objects with non-empty `cause`, and `confidence` set to
`low`, `medium`, or `high`. Scalar strings for the simple array fields are
normalized to one-item arrays.

The validator collects top-level `evidenceRefs` and
`likelyRootCauses[].evidenceRefs`, then verifies every ref against evidence rows
visible to the current run where `final_allowed=true`.

Accepted ref formats:

```text
grep_results.json#matches/<index>
log_searches/<search_id>.json#matches/<index>
log_slices/<slice_id>.json#lines
tool_results/<tool_id>/result.json#findings/<index>
```

The referenced artifact must exist and the match/finding index must be in
range. Background context such as `manifest.json`, future system context,
metadata slices, and diagnostic skill references must stay readable context and
cannot be cited as final root-cause evidence.

## Waiting States

Task MCP can now request:

```text
logagent.request_user_input
logagent.request_approval
```

Both calls create an `actions` row and append timeline events. User input moves
the run to `waiting_for_user`; approval moves it to `waiting_for_approval`.
`POST /api/v2/runs/:run_id/messages` and
`POST /api/v2/actions/:action_id/decisions` requeue waiting runs into the
SQLite job queue. The current Agent runtime is still a stub, so resumed runs do
not yet perform true multi-round model reasoning.

## Security

- API key is read from `LOGAGENT_V2_API_KEY`.
- Artifact paths are resolved relative to `data_dir` and rejected if they
  escape it.
- Upload filenames are basename-normalized and character-filtered.
- Archive entries are scanned in memory and rejected if they contain absolute
  paths or traversal components.
- Tools execute only through configured whitelist descriptors, with absolute
  command paths, fixed args, timeout, and bounded stdout/stderr.
- Agent final answers are rejected before success if they cite missing,
  out-of-range, unsupported, or background-only refs.

## Acceptance

The current slice is accepted when:

- `python3 -m compileall logagent_v2` passes.
- `PYTHONPATH=. python3 -m unittest discover tests` passes.
- A Workspace can be created, an upload stored, a Run queued, and the inline
  worker can complete the stub Agent result.
