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
- V1-style node log package preprocessing for
  `<packageId>_<instanceId>_<nodeId>_<timestamp>_logs.tar.gz` uploads.
- `manifest.json` and `grep_results.json` artifact generation.
- Read-only MCP discovery placeholder.
- Task MCP endpoint with summary/evidence/manifest/grep resources and
  `logagent.search_logs` follow-up search plus `logagent.get_log_slice`.
- Minimal configured Tool Runner exposed through `/api/v2/tools` and task MCP
  `logagent.run_domain_tool`.
- Waiting-state action foundation for task MCP `logagent.request_user_input`
  and `logagent.request_approval`.
- Final answer schema normalization and evidence ref validation before a run
  can be marked `succeeded`.
- Metadata foundation with JSON/YAML/openGemini content import, SQLite snapshot
  storage, field/tag type queries, HTTP API, and readonly/task MCP tools.
- Case Memory foundation with manual cases, succeeded-run case confirmation,
  keyword recall, edit/disable API, and readonly/task MCP search.
- Skill-backed System Context foundation with filesystem Skill registry,
  Markdown import, explicit Workspace skill selection, `system_context` run
  snapshot, and readonly/task MCP reference reading.
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
GET  /api/v2/metadata/instances
GET  /api/v2/metadata/instances/:instance_id
GET  /api/v2/metadata/instances/:instance_id/snapshot
DELETE /api/v2/metadata/instances/:instance_id
POST /api/v2/metadata/imports
POST /api/v2/metadata/field-types
POST /api/v2/metadata/tag-fields
POST /api/v2/cases
POST /api/v2/runs/:run_id/case
GET  /api/v2/cases
GET  /api/v2/cases/:case_id
PATCH /api/v2/cases/:case_id
GET  /api/v2/skills
GET  /api/v2/skills/:skill_id
POST /api/v2/skills/imports
POST /api/v2/skills/preview
POST /api/v2/mcp/readonly
POST /api/v2/mcp/task/:run_id
```

## Verification

```bash
python3 -m compileall logagent_v2
PYTHONPATH=. python3 -m unittest discover tests
```

This V2 slice intentionally does not yet migrate V1 analyzer materialized tool
inputs, rich Tool Runner input matching, Metadata preview/confirm and URL fetch,
skills.zip export, richer Skill auto-matching, Case import drafts,
FTS/embedding recall, WebUI, or full LangGraph model loop.

## Initial Evidence Pipeline

When a run starts, V2 now reads all uploads attached to the Workspace and:

- accepts plain `.log`, `.txt`, `.out`, `.err`, `.trace`, `.json`, `.jsonl`,
  `.yaml`, `.yml`, `.conf`, and `.cfg` files;
- scans `.zip`, `.tar`, `.tar.gz`, and `.tgz` packages without writing archive
  members to arbitrary filesystem paths;
- recognizes openGemini-style node log packages named
  `<packageId>_<instanceId>_<nodeId>_<timestamp>_logs.tar.gz`;
- rejects absolute paths, `..` path traversal, and unsafe archive entries;
- skips symlinks and non-file archive members;
- writes bounded `manifest.json` and `grep_results.json` artifacts;
- records `manifest` and `log_search` evidence items; and
- lets the current stub Agent final answer reference
  `grep_results.json#matches/<index>` when matches exist.

Node log packages are classified by archive path components, so wrapper
directories and `./` entries are tolerated. Files under
`var/chroot/gemini/log/tsdb`, `var/chroot/gemini/log/stream`, and
`home/Ruby/log` become:

```text
extracted/<nodeId>/<timestamp>/tsdb/<relative-file>
extracted/<nodeId>/<timestamp>/stream/<relative-file>
extracted/<nodeId>/<timestamp>/agent/<relative-file>
```

Rotated files are accepted by directory membership rather than filename suffix,
and gzip content is decoded by magic bytes before grep indexing. A node package
with no supported log directory fails instead of producing an empty manifest.

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
- `tools/call logagent.request_user_input`
- `tools/call logagent.request_approval`

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

`request_user_input` and `request_approval` persist pending `actions` and move
the run into `waiting_for_user` or `waiting_for_approval`. Posting a message to
a waiting run or approving/rejecting a pending action requeues the run through
the SQLite job queue.

## Final Answers

Before V2 stores a `succeeded` run, final answers are normalized and validated.
The current required shape is:

- `summary`: non-empty string
- `symptoms`, `nextChecks`, `fixSuggestions`, `missingInformation`: string
  arrays
- `likelyRootCauses`: objects with non-empty `cause` and `evidenceRefs`
- `confidence`: `low`, `medium`, or `high`
- `evidenceRefs`: optional top-level string array

Only current-task, final-allowed evidence refs are accepted:

```text
grep_results.json#matches/<index>
log_searches/<search_id>.json#matches/<index>
log_slices/<slice_id>.json#lines
tool_results/<tool_id>/result.json#findings/<index>
```

Background resources such as `manifest.json` are readable over task MCP but
cannot be used as final root-cause evidence.

## Metadata

V2 stores imported Metadata in SQLite table `metadata_instances`. The current
direct import endpoint accepts:

```json
{
  "instanceId": "prod-og-1",
  "templateType": "opengemini",
  "content": "{...}",
  "remark": "optional display name"
}
```

`templateType=json` and `templateType=opengemini` parse JSON content.
`templateType=yaml` uses PyYAML. The openGemini parser normalizes `MetaNodes`,
`DataNodes`, `SqlNodes`, `Databases`, retention policies, measurements, and
schema field type codes. Type labels follow the existing openGemini mapping:
`0 Unknown`, `1 Integer`, `2 Unsigned`, `3 Float`, `4 String`, `5 Boolean`,
`6 Tag`, and `7 Unknown`.

Readonly MCP and task MCP expose:

```text
logagent.list_metadata_instances
logagent.get_metadata_snapshot
logagent.get_metadata_field_types
logagent.get_metadata_tag_fields
```

Task MCP Metadata calls persist `metadata_slice` evidence as background context
with `final_allowed=false`; final answers cannot cite these slices as root-cause
evidence.

## Case Memory

V2 stores confirmed cases in SQLite table `cases` using Case schema v2:

```json
{
  "sourceType": "manual",
  "title": "Timeout during compaction",
  "symptom": "...",
  "rootCause": "...",
  "solution": "...",
  "evidenceRefs": []
}
```

Manual cases are created through `POST /api/v2/cases`. Succeeded runs can be
confirmed through `POST /api/v2/runs/:run_id/case`; repeated confirmation of the
same run returns the existing task case. Cases can be searched with keyword
overlap, read by ID, edited, or disabled. Disabled cases are hidden unless the
caller sets `includeDisabled=true`.

Readonly MCP and task MCP expose:

```text
logagent.search_cases
logagent.get_case
```

Task MCP Case calls persist `case_context` evidence as background context with
`final_allowed=false`. Historical cases are references for investigation and do
not replace current-task evidence.

## Skills And System Context

V2 stores diagnostic Skills under:

```text
<LOGAGENT_V2_DATA_DIR>/skills/<skillId>/
  SKILL.md
  logagent.json
  references/...
```

`POST /api/v2/skills/imports` creates a simple Markdown Skill with
`SKILL.md` frontmatter and default `logagent.json`. Workspaces can carry
explicit `skillIds`; each run writes a `system_context` artifact containing
selected diagnostic skill summaries, bounded `SKILL.md` content, revision, and
declared references.

Readonly MCP and task MCP expose:

```text
logagent.list_skills
logagent.get_skill
logagent.get_skill_reference
logagent.preview_system_context
```

Task MCP `logagent.get_skill_reference` only reads references declared in the
run's `system_context` snapshot and persists a `skill_reference` background
artifact with `final_allowed=false`. Readonly MCP reads the current registry and
does not write workspace artifacts.
