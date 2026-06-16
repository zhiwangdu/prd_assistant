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
- Materialized `tool_inputs/index.json` generation for node-package tsdb
  InfluxQL query lines plus generic file-level InfluxQL and Flux query lines,
  with analyzer JSONL input artifacts.
- `manifest.json` and `grep_results.json` artifact generation.
- Read-only MCP discovery placeholder.
- Task MCP endpoint with summary/evidence/manifest/grep resources and
  `logagent.search_logs` follow-up search plus `logagent.get_log_slice`.
- Minimal configured Tool Runner exposed through `/api/v2/tools` and task MCP
  `logagent.run_domain_tool`; tools with `{input_file}` consume matching
  materialized `tool_inputs` before execution, then fall back to manifest file
  patterns and initial grep keyword matches. Generic JSON stdout and
  InfluxQL analyzer report/compare stdout are normalized into
  `summary/findings`.
- Fetch endpoint foundation with SQLite endpoint storage, HTTP API management,
  DevTools bash cURL import, default-off allowlist execution, task MCP
  `logagent.fetch`, and `fetch_result` final evidence refs.
- Waiting-state action foundation for task MCP `logagent.request_user_input`
  and `logagent.request_approval`.
- Final answer schema normalization and evidence ref validation before a run
  can be marked `succeeded`.
- Metadata foundation with JSON/YAML/openGemini content import, allowlisted URL
  fetch, SQLite snapshot storage, preview/confirm drafts, field/tag type
  queries, per-run `metadata_context` auto-selection, HTTP API, and
  readonly/task MCP tools.
- Case Memory foundation with manual cases, succeeded-run case confirmation,
  text/JSON import drafts, SQLite FTS5/BM25 recall, edit/disable API, and
  readonly/task MCP search.
- Skill-backed System Context foundation with filesystem Skill registry,
  Markdown import, explicit or auto-matched Workspace skill selection,
  `system_context` run snapshot, readonly/task MCP reference reading, and
  `skills.zip` export.
- `tools.zip` export for enabled configured subprocess tools, with packaged
  executables, shell wrappers, examples, and a manifest.
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
| `LOGAGENT_V2_FETCH_ENABLED` | `0` | Enable configured Fetch endpoint execution |
| `LOGAGENT_V2_FETCH_ALLOWED_HOSTS` | unset | Comma-separated exact host or host:port allowlist |
| `LOGAGENT_V2_FETCH_TIMEOUT_SECONDS` | `20` | Per-request Fetch timeout |
| `LOGAGENT_V2_FETCH_MAX_RESPONSE_BYTES` | `1048576` | Maximum stored Fetch response preview bytes |
| `LOGAGENT_V2_FETCH_MAX_REDIRECTS` | `5` | Maximum manually revalidated Fetch redirects |
| `LOGAGENT_V2_FETCH_SECRET_KEY` | unset | Fernet 32-byte base64 key for encrypted Fetch credential sets |

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
    "maxOutputBytes": 1048576,
    "maxInputFiles": 1,
    "match": {
      "filePatterns": ["*.jsonl"],
      "keywords": ["query"]
    },
    "paramsSchema": {
      "type": "object",
      "properties": {
        "mode": {"type": "string", "enum": ["fast", "full"]}
      },
      "additionalProperties": false
    }
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
GET  /api/v2/exports/skills.zip
GET  /api/v2/exports/tools.zip
GET  /api/v2/metadata/instances
GET  /api/v2/metadata/instances/:instance_id
GET  /api/v2/metadata/instances/:instance_id/snapshot
DELETE /api/v2/metadata/instances/:instance_id
GET  /api/v2/metadata/imports
GET  /api/v2/metadata/imports/:import_id
POST /api/v2/metadata/imports/preview
POST /api/v2/metadata/imports/fetch/preview
POST /api/v2/metadata/imports/:import_id/confirm
POST /api/v2/metadata/imports/fetch
POST /api/v2/metadata/imports
POST /api/v2/metadata/field-types
POST /api/v2/metadata/tag-fields
POST /api/v2/cases
POST /api/v2/runs/:run_id/case
GET  /api/v2/cases
GET  /api/v2/cases/imports
GET  /api/v2/cases/imports/:import_id
POST /api/v2/cases/imports/preview
POST /api/v2/cases/imports/:import_id/confirm
GET  /api/v2/cases/:case_id
PATCH /api/v2/cases/:case_id
GET  /api/v2/skills
GET  /api/v2/skills/:skill_id
POST /api/v2/skills/imports
POST /api/v2/skills/preview
POST /api/v2/fetch/imports/preview
POST /api/v2/fetch/imports
GET  /api/v2/fetch/endpoints
POST /api/v2/fetch/endpoints
GET  /api/v2/fetch/endpoints/:endpoint_id
PATCH /api/v2/fetch/endpoints/:endpoint_id
DELETE /api/v2/fetch/endpoints/:endpoint_id
POST /api/v2/runs/:run_id/fetch/:endpoint_id
POST /api/v2/mcp/readonly
POST /api/v2/mcp/task/:run_id
```

## Verification

```bash
python3 -m compileall logagent_v2
PYTHONPATH=. python3 -m unittest discover tests
```

This V2 slice intentionally does not yet migrate V1 analyzer materialized tool
inputs beyond generic InfluxQL/Flux JSONL, embedding/vector recall, WebUI, or
full LangGraph model loop.

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
- writes `tool_inputs/index.json`, `influxql_analyzer` JSONL artifacts, and
  `flux_query_analyzer` JSONL artifacts when logs contain supported query
  lines;
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
- `resources/read` for `summary`, `evidence`, `manifest`, `grep_results`,
  `system_context`, and `metadata_context`
- `tools/list`
- `tools/call logagent.search_logs`
- `tools/call logagent.get_log_slice`
- `tools/call logagent.run_domain_tool`
- `tools/call logagent.list_fetch_endpoints`
- `tools/call logagent.fetch`
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
possible and persisted as `tool_result` evidence. Generic JSON output can use
`summary`, `message`, or `title`, plus `findings`, `issues`, or `diagnostics`.
InfluxQL analyzer report JSON is adapted into a compact summary and findings
for special rules, parse errors, realtime classification, fingerprints, compare
fingerprint deltas, and rule deltas.

Configured tools may declare `paramsSchema`. Task MCP `logagent.run_domain_tool`
then accepts `params`, validates a conservative object-schema subset
(`required`, `additionalProperties`, primitive `type`, and `enum`), and replaces
`{params.name}` placeholders in configured argv. Commands and argv templates
still come only from Server configuration.

If a configured tool argument contains `{input_file}`, V2 reads the current
run's latest `tool_input_index` evidence and selects entries whose `toolIds`
include the requested tool. The placeholder is replaced with the local artifact
path for that input. Each selected input gets a stable action id derived from
the tool id and virtual input path, so final refs use:

```text
tool_results/<tool_id>_<input_hash>/result.json#findings/<index>
```

If no materialized input matches, V2 falls back to current-run text files:
manifest paths matching the tool's `match.filePatterns` are selected first,
then initial `grep_results.json` matches whose line text contains one of the
tool's `match.keywords` supplement the selection. Fallback inputs are persisted
as run-local tool input artifacts, exposed to the tool as virtual
`extracted/<manifest path>` inputs, and bounded by `maxInputFiles`. Multi-input
MCP calls keep `result/evidence` as the primary run and additionally return
`results[]` and `evidenceItems[]`.

`GET /api/v2/exports/tools.zip` exports enabled configured subprocess tools.
The archive contains `README.md`, `tools-manifest.json`, executable files under
`bin/<toolId>/`, shell wrappers under `wrappers/`, and config examples under
`config/examples/`. Missing, relative, non-file, or non-executable tool
commands are kept in the manifest with `skipped=true`; disabled tools and
built-in tools are omitted. The export does not include API keys, endpoint
credentials, runtime environment values, uploads, artifacts, or workspace data.

Fetch endpoints are configured through the protected HTTP API or imported from
DevTools bash cURL commands using `POST /api/v2/fetch/imports/preview` and
`POST /api/v2/fetch/imports`. Supported cURL flags are limited to request
method, headers, body, cookies, compression, HEAD, and location. Execution is
disabled unless `LOGAGENT_V2_FETCH_ENABLED=1` and constrained to `http`/`https`
URLs whose host or host:port exactly matches `LOGAGENT_V2_FETCH_ALLOWED_HOSTS`.
Request URLs, sensitive headers, and sensitive JSON/form-style body preview
fields are redacted in API, MCP, and artifact previews. Redirects are followed
manually up to
`LOGAGENT_V2_FETCH_MAX_REDIRECTS`; every hop is revalidated against the same
allowlist, and sensitive headers are stripped when redirecting across origin.
Fetch stores bounded response previews as `fetch_result` evidence.

Sensitive Fetch endpoint material is split into an encrypted credential set.
If a URL query parameter, header, or body field looks like a token, secret,
password, API key, session, Authorization, or Cookie, V2 stores only a redacted
endpoint definition in `fetch_endpoints` and encrypts the full request material
in `fetch_credential_sets` using `LOGAGENT_V2_FETCH_SECRET_KEY`. Creating or
updating a sensitive endpoint without a valid key is rejected before the
endpoint row is written. Execution hydrates the endpoint from the credential
set, while API, MCP, and result artifacts continue to show only redacted values.

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
tool_results/<fetch_action_id>/result.json#response
```

Background resources such as `manifest.json` are readable over task MCP but
cannot be used as final root-cause evidence.

## Metadata

V2 stores Metadata import drafts in SQLite table `metadata_imports` and
confirmed Metadata in `metadata_instances`. The direct import endpoint still
imports immediately; the safer product flow is:

```text
POST /api/v2/metadata/imports/preview
POST /api/v2/metadata/imports/fetch/preview
GET  /api/v2/metadata/imports/:import_id
POST /api/v2/metadata/imports/:import_id/confirm
```

Preview parses and normalizes content, stores a draft with status `previewed`,
and returns node/database counts without changing `metadata_instances`. Confirm
upserts the normalized snapshot and marks the draft `confirmed`.

URL fetch uses the same default-off Fetch boundary:
`LOGAGENT_V2_FETCH_ENABLED=1`, exact host/host:port
`LOGAGENT_V2_FETCH_ALLOWED_HOSTS`, timeout, and response size limit. Only GET is
used, redirects are rejected in this V2 slice, and draft `sourceUrl` values are
stored/displayed with sensitive query parameters redacted.

Metadata import payloads accept:

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

When a run starts, V2 also writes `metadata_context.json` as background
evidence. If exactly one metadata instance exists, it is selected as
`default_single`; with multiple instances, V2 scores instance id, remark,
cluster, node, database, retention policy, measurement, and field names against
the Workspace question/mode and includes up to three matched outlines. The
outline is bounded to node/database/schema summaries and is exposed through task
MCP resource `logagent-v2://run/<run_id>/metadata_context`; full snapshots and
field details remain available through the Metadata MCP tools.

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
queries, read by ID, edited, or disabled. Query search uses local SQLite
FTS5/BM25 over title, symptom, root cause, solution, product/version/
environment, instance/node, and evidence refs, with token-overlap fallback if
FTS5 is unavailable. Disabled cases are hidden unless the caller sets
`includeDisabled=true`.

Case import drafts support text or JSON capture before a Case is confirmed:

```text
POST /api/v2/cases/imports/preview
GET  /api/v2/cases/imports
GET  /api/v2/cases/imports/:import_id
POST /api/v2/cases/imports/:import_id/confirm
```

Preview parses JSON Case fields or plain text sections such as `Title`,
`Symptom`, `Root Cause`, `Solution`, `Product`, `Instance ID`, and
`Evidence Refs`. Missing required fields are returned as `validationErrors`.
Confirm may provide overrides to complete or edit the draft; only confirm writes
to `cases` and updates the FTS index.

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
explicit `skillIds`; if none are set, V2 includes `includeByDefault` Skills and
auto-matches Skills whose `keywords`, `products`, `toolIds`, `domainAdapters`,
name, or description match the question. Each run writes a `system_context`
artifact containing selected diagnostic skill summaries, bounded `SKILL.md`
content, revision, match reason, match score, and declared references.

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

`GET /api/v2/exports/skills.zip` exports the current Skill registry as a zip
snapshot. It includes regular files under each Skill directory, preserves
relative paths, writes a root `manifest.json`, and skips symlinks so exports
cannot include files outside the registry.
