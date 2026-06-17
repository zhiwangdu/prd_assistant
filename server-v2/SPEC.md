# LogAgent V2 Server Spec

## Goal

V2 is a clean-room small-team implementation of LogAgent. It favors a simple
single-machine deployment over distributed infrastructure:

- Python + FastAPI for the API.
- LangGraph-oriented Agent runtime.
- SQLite WAL for durable state.
- Local filesystem artifacts for large evidence.
- DB-backed jobs instead of Redis.
- Static WebUI build hosting from local filesystem.

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
- `UploadSession`: restartable chunked upload state and temp-file pointer.
- `Action`: Agent-requested operation that may require approval.
- `Job`: persistent background work item.

## Current Implementation

Implemented in this slice:

- FastAPI app and public `GET /health`.
- Public `GET /` static WebUI hosting from `webui/out`; non-API SPA routes
  return `index.html`, static assets are served directly, and unknown `/api/*`
  paths still return 404.
- Deploy template controls through `deploy/rebuild-v2-install.sh` and
  `deploy/logagent-v2ctl.sh` for runtime virtualenv install, SQLite
  initialization, WebUI static sync, start, stop, restart, status, and logs.
- Bearer auth for `/api/v2/*`.
- SQLite schema creation with WAL.
- Workspace creation/list/read/update and soft-delete lifecycle; deleted
  Workspaces are omitted from history lists while existing run/upload/artifact
  rows remain readable by id.
- Workspace-scoped upload, upload session, and run listing plus global run
  listing for WebUI history views.
- Single multipart upload, batch multipart upload, and restartable chunked
  upload storage as local artifacts.
- Run creation and queued `run_analysis` job.
- Inline DB-backed worker.
- Startup recovery for interrupted DB-backed jobs: non-terminal analysis and
  remote command jobs are requeued immediately, while stale jobs for terminal or
  waiting runs are completed without rerun.
- Initial evidence pipeline for uploaded text files and supported archives.
- Node log package preprocessing for
  `<packageId>_<instanceId>_<nodeId>_<timestamp>_logs.tar.gz`; supported log
  directories are classified into stable `extracted/<node>/<timestamp>/<group>`
  paths, and gzip-rotated files are decoded by magic bytes.
- Materialized `tool_inputs/index.json` generation for node package tsdb
  InfluxQL query lines, generic file-level InfluxQL query lines, and Flux query
  lines. Generated entries are compatible with the V1 `ToolInputEntry` shape
  and include V2 artifact ids for local execution.
- `manifest.json` and `grep_results.json` artifact generation.
- Agent runtime that records initial question evidence, consumes the initial
  evidence pipeline, and either returns a deterministic stub summary or calls a
  bounded OpenAI-compatible provider loop for advertised Server-owned tools and
  an evidence-validated JSON final answer. Each round persists
  `agent_request.json`, `agent_response.json`, and `analysis_state.json` audit
  artifacts before the run reaches a terminal state. Successful analysis runs
  persist a deterministic fallback alias derived from the final summary or
  question for history/UI display.
- `analysis_package.json` generation after initial evidence collection, exposed
  as task MCP resource for Agent loop context.
- Timeline events for workspace, upload, run, and evidence lifecycle.
- Artifact download.
- Evidence and artifact listing for a run, including uploaded input artifacts
  and evidence artifact outputs.
- Run analysis summary endpoint combining run metadata, timeline, evidence,
  artifacts, analysis resources, final result, and run alias for WebUI
  inspection.
- Read-only MCP endpoint with `initialize`, `resources/list`, `resources/read`,
  `tools/list`, and tools/resources for tool catalog, Metadata, Case Memory,
  Skill registry, and Domain Adapter summaries.
- Task MCP endpoint with `summary`, `evidence`, `manifest`, `grep_results`,
  `system_context`, `metadata_context`, `analysis_package`, `analysis_state`,
  `agent_request`, `agent_response`, `result`, and `result_markdown`
  resources.
- Task MCP `logagent.search_logs`, which creates follow-up `log_search`
  evidence and stable `log_searches/<search_id>.json#matches/<index>` refs.
- Task MCP `logagent.get_log_slice`, which reads bounded context from a current
  Workspace text path and persists `log_slice` evidence.
- Tool Plugin registry. Configured subprocess tools are loaded from
  `LOGAGENT_V2_TOOLS_JSON` or the V2 analyzer executable environment variables,
  listed through `/api/v2/tools`, runnable through manual tool-run APIs, and
  exposed to task MCP `logagent.run_domain_tool`. Tools with `{input_file}`
  consume matching materialized tool inputs before execution, then fall back to
  manifest file patterns, initial grep keyword matches, or raw upload artifacts
  for storage analyzers. Generic JSON stdout and InfluxQL analyzer
  report/compare stdout are normalized into `summary/findings`.
- V1 built-in tool migration for metadata catalog tools,
  `logagent.preprocess_log_package`, `logagent.fetch`, `pprof_analyzer`, and
  default-off `logagent.huawei_cloud_package_sync`.
- Fetch endpoint foundation. Endpoints are stored in SQLite, listed and managed
  through protected HTTP APIs, importable from DevTools bash cURL, exposed as a
  built-in `/api/v2/tools` descriptor, and executable through task MCP
  `logagent.fetch` only when enabled and allowlisted. Sensitive endpoint
  material is split into encrypted credential sets.
- Waiting-state foundation through task MCP `logagent.request_user_input` and
  `logagent.request_approval`; pending actions are persisted, exposed in run
  analysis summaries, user supplements mark pending user-input actions as
  answered, and user message/approval APIs requeue the run with bounded
  `interactionContext` in the next Agent request. Approved
  `collect_environment` actions either record V1-compatible MOCK
  `environment_evidence` background artifacts or, when the action input targets
  an enabled Remote Executor and whitelisted command, queue a remote command and
  record the completed command output before resuming the analysis run.
- Final answer schema normalization and evidence ref validation. A run can only
  be marked `succeeded` after final refs point to current-run, final-allowed
  log search, log slice, or tool finding evidence.
- Final result persistence as `result.json` and `result.md` background
  artifacts, exposed through HTTP and task MCP resources.
- Metadata foundation with direct JSON/YAML/openGemini content import,
  allowlisted URL fetch, preview/confirm draft workflow, SQLite snapshot
  storage, saved raw snapshot refresh, field/tag type query APIs, per-run `metadata_context`
  auto-selection, readonly MCP tools, and task MCP background slices.
- Case Memory foundation with manual Case creation, succeeded-run Case
  confirmation, text/JSON import drafts, follow-up import messages, SQLite FTS5/BM25 recall,
  local hash-vector recall, edit/disable API, readonly MCP search, and task MCP
  background case context.
- Skill-backed System Context foundation with filesystem Skill registry,
  Markdown import, explicit or auto-matched Workspace skill selection, per-run
  `system_context` artifact, readonly MCP Skill tools, and task MCP reference
  artifacts.
- Legacy System Context resource compatibility APIs backed by SQLite. V2 can
  create, list, read, update, version, activate, and preview prompt packs,
  architecture docs, runbooks, glossaries, tool capability notes, knowledge
  notes, and diagnostic-skill records. Metadata instances are exposed as
  read-only `metadata_instance` adapter resources in the same list/preview
  surface.
- `skills.zip` export for the current Skill registry, with regular files only,
  root manifest, and symlink skipping.
- `tools.zip` export for enabled configured subprocess tools, with packaged
  executable files, shell wrappers, config examples, and skip reasons for tools
  that cannot be packaged.
- V2 Settings and diagnostics endpoints for Agent provider summary, model list
  and chat connectivity tests, in-process Agent backend dry-run diagnostics,
  built-in Domain Adapter summaries, and process-local LLM response-content
  debug logging.
- Remote Executor foundation with SQLite-managed executor assets,
  environment-configured whitelisted SSH command templates, DB-backed
  `remote_command_run` jobs, controlled SSH argv construction, bounded
  stdout/stderr capture, and result files under `remote_runs/<run_id>/`.

Not yet implemented:

- Full LangGraph multi-round planning and product-grade resume policies beyond
  the current bounded `interactionContext` handoff.
- Additional analyzer materialized `tool_inputs/index.json` generation beyond
- Full WebUI V2 cutover that replaces the legacy Rust-compatible panels instead
  of running V2 bridge panels alongside them.

## API

Protected endpoints use:

```text
Authorization: Bearer <api-key>
```

Current V2 endpoints:

```http
GET  /
GET  /health
POST /api/v2/workspaces
GET  /api/v2/workspaces
GET  /api/v2/workspaces/:workspace_id
PATCH /api/v2/workspaces/:workspace_id
DELETE /api/v2/workspaces/:workspace_id
GET  /api/v2/workspaces/:workspace_id/uploads
GET  /api/v2/workspaces/:workspace_id/upload-sessions
GET  /api/v2/workspaces/:workspace_id/runs
POST /api/v2/workspaces/:workspace_id/uploads
POST /api/v2/workspaces/:workspace_id/uploads/batch
POST /api/v2/workspaces/:workspace_id/uploads/init
GET  /api/v2/uploads/:session_id
POST /api/v2/uploads/:session_id/chunks?offset=<bytes>
POST /api/v2/uploads/:session_id/complete
POST /api/v2/workspaces/:workspace_id/runs
GET  /api/v2/runs?workspaceId=<workspace_id>
GET  /api/v2/runs/:run_id
GET  /api/v2/runs/:run_id/timeline
GET  /api/v2/runs/:run_id/evidence
GET  /api/v2/runs/:run_id/artifacts
GET  /api/v2/runs/:run_id/analysis
GET  /api/v2/runs/:run_id/result
POST /api/v2/runs/:run_id/messages
POST /api/v2/actions/:action_id/decisions
GET  /api/v2/evidence/:evidence_id
GET  /api/v2/artifacts/:artifact_id
GET  /api/v2/tools
GET  /api/v2/tools/:tool_id
POST /api/v2/tools/:tool_id/runs
GET  /api/v2/tools/runs
GET  /api/v2/tools/runs/:run_id
GET  /api/v2/tools/runs/:run_id/result
GET  /api/v2/tools/runs/:run_id/artifacts
GET  /api/v2/debug/llm
PUT  /api/v2/debug/llm
GET  /api/v2/settings/llm
GET  /api/v2/settings/llm/models
POST /api/v2/settings/llm/chat
GET  /api/v2/settings/agent-backends
POST /api/v2/settings/agent-backends/:backend_id/test
GET  /api/v2/settings/domain-adapters
GET  /api/v2/executors
POST /api/v2/executors
GET  /api/v2/executors/:executor_id
PATCH /api/v2/executors/:executor_id
DELETE /api/v2/executors/:executor_id
GET  /api/v2/executor-command-templates
GET  /api/v2/executor-runs
POST /api/v2/executor-runs
GET  /api/v2/executor-runs/:run_id
GET  /api/v2/executor-runs/:run_id/result
GET  /api/v2/exports/skills.zip
GET  /api/v2/exports/tools.zip
GET  /api/v2/metadata/instances
GET  /api/v2/metadata/instances/:instance_id
GET  /api/v2/metadata/instances/:instance_id/snapshot
POST /api/v2/metadata/instances/:instance_id/refresh
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
POST /api/v2/cases/imports/:import_id/messages
POST /api/v2/cases/imports/:import_id/confirm
GET  /api/v2/cases/:case_id
PATCH /api/v2/cases/:case_id
GET  /api/v2/skills
GET  /api/v2/skills/:skill_id
POST /api/v2/skills/imports
POST /api/v2/skills/preview
GET  /api/v2/system-context/resources
POST /api/v2/system-context/resources
GET  /api/v2/system-context/resources/:context_id
PATCH /api/v2/system-context/resources/:context_id
POST /api/v2/system-context/resources/:context_id/versions
PATCH /api/v2/system-context/resources/:context_id/versions/:version_id
POST /api/v2/system-context/resources/:context_id/versions/:version_id/activate
POST /api/v2/system-context/preview
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

## System Context Compatibility

The canonical V2 run-time System Context is Skill-backed. Workspace `skillIds`,
auto-matched Skills, and Metadata context are materialized into a per-run
`system_context` artifact and exposed through task MCP.

The `/api/v2/system-context/*` endpoints provide the V1-style management
surface for internal tools that still model prompt packs, runbooks, glossaries,
and similar knowledge as System Context resources. These records are persisted
in SQLite, support draft/active/archived versions, and can be previewed with
task kind, product, version, environment, and metadata instance filters. They
are not automatically injected into new analysis runs in this slice;
productized run-time knowledge should continue to be represented as Skills.

## Storage

Default data layout:

```text
/tmp/logagent-v2/
  logagent.sqlite
  artifacts/
    <workspace_id>/
      <artifact_file_id>/
        <filename>
  remote_runs/
    <remote_run_id>/
      remote_command/
        result.json
        stdout.txt
        stderr.txt
  tmp/
    upload_sessions/
      <session_id>/
        <filename>
```

Runtime deploy defaults:

```text
$LOGAGENT_APP_DIR/
  server-v2/.venv/
  data-v2/
    logagent.sqlite
    artifacts/
    tmp/
  webui/out/
  logagent-v2.pid
  logagent-v2.log
```

SQLite tables:

- `workspaces`
- `runs`
  - `kind=analysis|tool_run`
  - tool-run columns: `tool_id`, `tool_params_json`, `tool_upload_ids_json`,
    `tool_result_artifact_id`, `error_json`
- `timeline_events`
- `artifacts`
- `uploads`
- `upload_sessions`
- `evidence_items`
- `actions`
- `jobs`
- `metadata_instances`
- `metadata_imports`
- `cases`
- `case_imports`
- `fetch_endpoints`
- `fetch_credential_sets`
- `remote_executors`
- `remote_runs`
- `system_context_resources`

The database stores state and bounded previews. Large payloads live in artifact
files and are referenced by `relative_path`, `sha256`, and size.

## Workspaces

V2 Workspaces are mutable analysis containers. `PATCH
/api/v2/workspaces/:workspace_id` updates the question, mode, language, and
explicit Skill ids for future runs. Existing run artifacts remain immutable
snapshots of what was executed.

`DELETE /api/v2/workspaces/:workspace_id` is a soft delete: it marks the
Workspace status as `deleted`, appends a timeline event, and hides the
Workspace from `GET /api/v2/workspaces`. It does not cascade-delete uploads,
runs, evidence, artifacts, or jobs. Creating a new run on a deleted Workspace is
rejected.

## Uploads

Single and batch upload endpoints create artifact rows directly from multipart
files and then attach Upload rows to the Workspace. Each file is bounded by
`LOGAGENT_V2_MAX_UPLOAD_BYTES`.

Chunked uploads use a durable `upload_sessions` row:

```text
init(filename, sizeBytes?) -> active session + temp_relative_path
chunk(offset, bytes) -> append only when offset == received_bytes
complete -> validate size, copy temp file to artifact store, create upload
```

Session state is stored in SQLite, while partial bytes live under
`tmp/upload_sessions/<session_id>/`. Completion marks the session `completed`
with the resulting `upload_id` and `artifact_id`; repeated complete calls can
return the completed session.

## Initial Evidence Pipeline

Run execution currently performs:

```text
Workspace uploads
  -> safe archive scan / text file collection
  -> optional analyzer JSONL tool_inputs materialization
  -> manifest.json artifact
  -> bounded keyword grep
  -> grep_results.json artifact
  -> manifest and log_search evidence
  -> analysis_package.json bounded Agent context
  -> agent_request.json / agent_response.json / analysis_state.json audit
  -> stub or OpenAI-compatible JSON final answer
  -> result.json and result.md artifacts
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
final evidence. Approved `environment_evidence` is also background-only and is
not accepted as a final evidence ref.

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

For node package `tsdb` logs, V2 extracts JSON lines with a string `query`,
`sql`, or `statement` field and raw lines that look like InfluxQL statements.
Those records are written to `influxql_analyzer` JSONL artifacts. The
corresponding `tool_inputs/index.json` artifact uses entries like:

```json
{
  "path": "tool_inputs/influxql_analyzer/<node>/<timestamp>.jsonl",
  "inputKind": "influxql_jsonl",
  "scope": "package",
  "toolIds": ["influxql_analyzer"],
  "nodeId": "node-a",
  "instanceId": "inst-a",
  "packageTimestamp": "20260617130000",
  "logGroup": "tsdb",
  "sourceFiles": ["extracted/node-a/20260617130000/tsdb/query.log"],
  "recordCount": 1,
  "artifactId": "artfile_...",
  "artifactRelativePath": "artifacts/..."
}
```

For non-node-package text files, V2 also extracts generic InfluxQL lines into
`tool_inputs/influxql_analyzer/workspace/<hash>.jsonl` with
`scope=file`. Flux scripts are detected from JSON fields such as `flux`,
`fluxQuery`, `query`, `script`, or `statement`, or raw lines that contain a
Flux `from(...) |> ...` pipeline. Flux inputs use
`inputKind=flux_query_jsonl` and `toolIds=["flux_query_analyzer"]`.

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
  -> optional manual POST /api/v2/tools/:tool_id/runs
  -> MCP logagent.run_domain_tool { toolId, params? }
  -> optional paramsSchema validation
  -> optional materialized tool input selection
  -> fixed absolute command + fixed args with {input_file}/{params.name} substitution
  -> tool_result artifact/evidence
```

The model cannot submit executable paths, shell snippets, dynamic argv, or
environment overrides.

The Tool Plugin registry is the single catalog source for `/api/v2/tools`,
readonly MCP `logagent.list_tools`, manual tool-run validation, and task MCP
configured tool execution. Task MCP `logagent.run_domain_tool` only exposes
configured subprocess tools. Built-ins use dedicated task MCP tools where
available, or the protected manual Tools API. The migrated built-ins are:

- metadata catalog tools: instance list, snapshot, field types, tag fields;
- `logagent.preprocess_log_package`;
- `logagent.fetch`;
- `pprof_analyzer`;
- `logagent.huawei_cloud_package_sync`, disabled until Huawei OBS/GaussDB
  environment variables are configured.

Manual tool runs create `kind=tool_run` rows in `runs` and `tool_run` jobs in
the DB-backed queue. They accept `workspaceId`, optional `uploadIds`, and
validated `params`; results are stored as V2 artifacts/evidence and exposed
through `/api/v2/tools/runs/:run_id/result`.

Configured tools may declare `paramsSchema`. V2 validates a conservative object
schema subset: required fields, `additionalProperties=false`, primitive
`type`, arrays, and `enum`. Validated params are recorded in `tool_result`
artifacts/evidence and can be substituted into configured argv with
`{params.<name>}` placeholders. Params affect the stable action id so different
parameter sets do not reuse one result path.

Tool stdout is parsed as JSON when possible. Generic JSON output supports
`summary` / `message` / `title`, `findings` / `issues` / `diagnostics`, and
finding fields `severity` / `level` / `status`, `file` / `path` / `filename`,
`line` / `lineNumber` / `startLine`, and `message` / `summary` /
`description` / `detail` / `title` / `cause`.

InfluxQL analyzer report stdout is specially adapted. Summary includes
`total_records`, `records_in_window`, `total_statements`, `parse_error_count`,
and `special_rules`. Findings include special rule hits, parse errors,
realtime classification, and notable fingerprints. InfluxQL compare report
stdout is also adapted: `statement_delta`, `qps_delta`, `batch_a`, and
`batch_b` go into summary, while new/removed/changed fingerprints and
`rule_deltas` become findings.

`GET /api/v2/exports/tools.zip` exports only enabled configured subprocess
tools from `LOGAGENT_V2_TOOLS_JSON`. Built-in tools are not packaged. The
archive contains:

```text
README.md
tools-manifest.json
bin/<toolId>/<executable>
wrappers/<toolId>.sh
config/examples/<toolId>.yaml
```

Executable commands must resolve to absolute, regular, executable files to be
packaged. Tools that cannot be packaged remain in `tools-manifest.json` with
`skipped=true` and `skipReason`; disabled tools are omitted. The export does
not include API keys, Fetch endpoint secrets, environment values, uploads,
artifacts, or workspace data.

When a tool arg contains `{input_file}`, V2 selects entries from the current
run's latest `tool_input_index` evidence whose `toolIds` contain the requested
tool id. The placeholder is replaced with the resolved artifact path. Each input
creates a stable action id derived from tool id plus virtual input path, and the
evidence ref prefix becomes:

```text
tool_results/<tool_id>_<input_hash>/result.json#findings/
```

If no materialized input matches, V2 falls back to current-run text files.
Manifest paths matching `match.filePatterns` are selected first. If capacity
remains, initial `grep_results.json` matches whose line text contains one of
`match.keywords` select additional files. Fallback files are materialized as
run-local `logagent.v2.tool_input.text_file.v1` artifacts and exposed to tools
as virtual `extracted/<manifest path>` inputs. Selection is de-duplicated,
bounded by `maxInputFiles`, and preserves materialized-input priority.
Multi-input MCP responses keep `result/evidence` for the primary execution and
add `results[]` and `evidenceItems[]`.

Storage analyzers (`opengemini_storage_analyzer` and
`influxdb_storage_analyzer`) use raw upload artifact fallback when no
materialized text input exists, so uploaded TSSP/TSI/TSM/_series payloads can be
passed directly to the source-built analyzer binaries.

## Fetch Endpoints

V2 Fetch endpoints are stored in SQLite table `fetch_endpoints` with name,
method, redacted URL, redacted headers, optional redacted body material, enabled
flag, and timestamps. Sensitive request material is stored separately in
`fetch_credential_sets` as encrypted JSON using `LOGAGENT_V2_FETCH_SECRET_KEY`.
The public API returns redacted endpoint previews; raw request material is only
hydrated inside the server-side executor.

Endpoints can be created directly or imported from DevTools bash cURL commands:

```text
POST /api/v2/fetch/imports/preview
POST /api/v2/fetch/imports
```

The cURL importer supports request method, headers, body, cookies,
`--compressed`, `--head`, and `--location`. It rejects unsupported flags such as
form uploads, proxy, cert, file, or resolver options rather than widening the
network or filesystem boundary. Import previews redact sensitive query,
header, and JSON/form body fields and return detected sensitive field
locations.

If a URL query parameter, header, or body field name looks like a token,
secret, password, API key, session, Authorization, or Cookie, creating or
updating the endpoint requires a valid Fernet 32-byte base64 key in
`LOGAGENT_V2_FETCH_SECRET_KEY`. Without that key, the write is rejected before
the endpoint row is stored.

Fetch execution is disabled by default. To execute endpoints, set:

```text
LOGAGENT_V2_FETCH_ENABLED=1
LOGAGENT_V2_FETCH_ALLOWED_HOSTS=127.0.0.1,example.internal:8080
LOGAGENT_V2_FETCH_MAX_REDIRECTS=5
LOGAGENT_V2_FETCH_SECRET_KEY=<fernet-32-byte-base64-key>
```

Only `http` and `https` URLs are supported. The requested host or host:port must
exactly match the allowlist. Controlled headers such as `Host`,
`Content-Length`, `Transfer-Encoding`, and `Connection` are rejected when
endpoints are saved. Sensitive headers, query parameters, and JSON/form body fields
containing token/secret/password/api key style names are redacted from API, MCP,
and artifact previews.

Redirects are followed manually up to `LOGAGENT_V2_FETCH_MAX_REDIRECTS`. Every
redirect target is revalidated with the same scheme/host allowlist before the
next request is sent. Sensitive headers such as Authorization, Cookie, and
X-Api-Key are stripped when a redirect crosses origin. Each response artifact
records `finalUrl`, `redirectCount`, and redacted redirect hops.

Task MCP exposes:

```text
logagent.list_fetch_endpoints
logagent.fetch { endpointId }
```

`logagent.fetch` writes a `fetch_result` artifact/evidence item. Network errors
produce a failed Fetch result rather than crashing the run. HTTP 4xx/5xx
responses are stored as responses.

Fetch response evidence refs use:

```text
tool_results/<fetch_action_id>/result.json#response
```

Final-answer validation accepts these refs only for current-run,
`final_allowed=true`, `kind=fetch_result` evidence whose artifact contains a
real `response` object. The readonly MCP endpoint may list the built-in Fetch
catalog descriptor, but it does not expose or run `logagent.fetch`.

## Metadata

V2 stores Metadata import drafts in SQLite table `metadata_imports` and
confirmed snapshots in `metadata_instances`. Instance rows are keyed by
`instance_id` and contain `template_type`, optional `remark`, normalized
`snapshot_json`, and original `raw_json`.

The product import flow is preview then confirm:

```text
POST /api/v2/metadata/imports/preview
POST /api/v2/metadata/imports/fetch/preview
GET  /api/v2/metadata/imports/:import_id
POST /api/v2/metadata/imports/:import_id/confirm
```

Preview parses and normalizes content into a draft with status `previewed` and
returns summary counts. It does not mutate `metadata_instances`. Confirm upserts
the draft snapshot into `metadata_instances` and marks the draft `confirmed`.
`POST /api/v2/metadata/imports` remains as a direct immediate import shortcut.

`POST /api/v2/metadata/instances/:instance_id/refresh` loads the instance's
stored raw JSON from SQLite, reruns the current normalizer with the same
template type and remark, and overwrites the normalized snapshot. It does not
fetch the original URL again; URL refresh remains an explicit fetch import.

URL fetch import uses the same default-off Fetch boundary. It requires
`LOGAGENT_V2_FETCH_ENABLED=1`, an exact host or host:port match in
`LOGAGENT_V2_FETCH_ALLOWED_HOSTS`, and the shared Fetch timeout/response-size
limits. V2 uses GET only, rejects redirects, redacts sensitive query parameters
in draft `sourceUrl`, and then runs the fetched content through the same
normalization and preview/confirm path.

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

Run startup writes `metadata_context.json` as background evidence and exposes it
through task MCP resource `logagent-v2://run/<run_id>/metadata_context`. The
context has schema version 1, a selection summary, and bounded
`metadata_instance` resources. If exactly one instance exists, V2 includes it as
`default_single`; if multiple exist, V2 scores instance id, remark, product,
environment, cluster, node, database, retention policy, measurement, and field
names against the Workspace question and mode. Selected resources include only
bounded topology/schema outlines; full snapshots remain behind
`logagent.get_metadata_snapshot` and detailed field/tag queries remain behind
their dedicated tools. The context artifact and all metadata slices use
`final_allowed=false`.

## Case Memory

V2 Case Memory stores confirmed Case schema v2 records in SQLite table `cases`.
Each row contains `source_type`, optional `task_id`, `enabled`, full
`record_json`, a denormalized `searchable_text` field for local keyword recall,
and a local hash-vector `vector_json` for dependency-light approximate recall.

Supported sources:

- `manual`: created through `POST /api/v2/cases`; requires `title`, `symptom`,
  `rootCause`, and `solution`.
- `task`: created through `POST /api/v2/runs/:run_id/case`; the run must be
  `succeeded` and have a final answer. Repeated confirmation of one run returns
  the existing task Case instead of creating duplicates.

Case import drafts live in `case_imports` and are created through
`POST /api/v2/cases/imports/preview`. Preview accepts JSON Case fields or plain
text sections such as `Title`, `Symptom`, `Root Cause`, `Solution`, `Product`,
`Version`, `Environment`, `Instance ID`, `Node ID`, and `Evidence Refs`. It
stores the source text, parsed draft, validation errors, and follow-up message
history without mutating `cases`. `POST
/api/v2/cases/imports/:import_id/messages` appends a user supplement, combines
all messages with the original source text, reparses the draft, and adds an
assistant question when required fields are still missing. `POST
/api/v2/cases/imports/:import_id/confirm` may provide field overrides; only a
complete confirmed draft creates a `manual` Case and updates the FTS index.
Re-confirming an already confirmed import returns the existing Case.

Search is dependency-light and local: V2 maintains a SQLite FTS5 table beside
`cases` and ranks query matches with `bm25`. The indexed text covers `title`,
`symptom`, `rootCause`, `solution`, product/version/environment, instance/node,
and evidence refs. V2 also stores a normalized hash vector derived from tokens
and character trigrams; query search merges FTS hits with vector recall and can
return vector-only hits when exact tokens do not match. If FTS5 is unavailable,
V2 falls back to token-overlap scoring plus vector recall. Disabled cases are
excluded by default and can be included with `includeDisabled=true`.

Readonly MCP exposes `logagent.search_cases` and `logagent.get_case`. Task MCP
exposes the same tools and writes results as `case_context` evidence with
`final_allowed=false`. Historical Cases are background references; final answers
still need current-task evidence refs.

## Skills And System Context

V2 Skills are Codex-compatible filesystem directories under
`LOGAGENT_V2_DATA_DIR/skills`. Each Skill requires `SKILL.md` with `name` and
`description` frontmatter; optional `logagent.json` defines display metadata,
`includeByDefault`, priority, and declared references.

The import API writes:

```text
skills/<skillId>/SKILL.md
skills/<skillId>/logagent.json
```

with a conservative default manifest. Workspaces can store explicit `skillIds`.
When a run starts, V2 writes `system_context.json` as a background artifact with
schema v2 resources:

```text
kind=diagnostic_skill
skillId
selectionReason
matchScore
revision
summary
content
references[]
```

If no explicit `skillIds` are set, V2 includes Skills whose manifest has
`includeByDefault=true` and auto-matches Skills whose `keywords`, `products`,
`toolIds`, `domainAdapters`, name, display name, Skill id, or description match
the Workspace question or mode. Each resource records `selectionReason` as
`explicit`, `default`, or `auto`, plus a numeric `matchScore`.

Readonly MCP exposes `logagent.list_skills`, `logagent.get_skill`,
`logagent.get_skill_reference`, and `logagent.preview_system_context` against
the current registry. Task MCP exposes the same tools, but
`logagent.get_skill_reference` is constrained to Skills and references captured
in the current run's `system_context` snapshot and persists `skill_reference`
evidence with `final_allowed=false`.

`GET /api/v2/exports/skills.zip` builds a current registry snapshot. The archive
contains each Skill directory's regular files under `<skillId>/...` plus a root
`manifest.json` with `schemaVersion`, `generatedAt`, Skill ids, display names,
revisions, source paths, and exported file metadata. Symlinks and symlinked
directories are skipped; archive paths must remain relative and cannot contain
`..`.

## Settings And Domain Adapters

V2 Settings APIs are equivalent product diagnostics for the clean-room runtime
rather than compatibility routes for the Rust Server:

- `GET /api/v2/settings/llm` returns the V2 Agent provider summary, configured
  model, timeout, input/output limits, and boolean configuration flags. It must
  not return API keys.
- `GET /api/v2/settings/llm/models` tests model listing. `stub` returns the
  local stub model; `openai_compatible` calls the configured `/models`
  endpoint.
- `POST /api/v2/settings/llm/chat` sends one bounded test message. `stub`
  returns a deterministic acknowledgment; `openai_compatible` calls the
  configured `/chat/completions` endpoint with the V2 max output token limit.
- `GET /api/v2/settings/agent-backends` summarizes the in-process V2 Agent
  runtime as `logagent_v2_agent`.
- `POST /api/v2/settings/agent-backends/:backend_id/test` performs a dry-run
  configuration diagnostic only. It must not execute shell commands.
- `GET /api/v2/settings/domain-adapters` returns the built-in adapter registry:
  `opengemini_influxdb` is active, while `cassandra` and `rocksdb` are
  skeleton adapters.

Readonly MCP must expose the same Domain Adapter summaries through
`logagent-v2://domain-adapters` and `logagent.list_domain_adapters`.

`GET/PUT /api/v2/debug/llm` controls process-local model response-content
logging. It is off by default, resets on restart, and may only log response
content to stderr; prompts, headers, and API keys must never be logged.

## Remote Executors

Remote Executors provide the V2 equivalent of the Rust Server's low-level
remote command smoke runner. They are not a full Environment Collector.

- Executors are stored in SQLite with `executorId`, name, host, port, SSH user,
  tags, notes, enabled state, and timestamps.
- `DELETE /api/v2/executors/:executor_id` disables an executor; it does not
  delete historical run records.
- Command templates are loaded from `LOGAGENT_V2_REMOTE_COMMANDS_JSON`. If
  unset, V2 exposes a default `smoke_ls_root` template with argv
  `["ls", "-la", "/root"]`.
- Creating a run validates that remote execution is enabled, the executor is
  enabled, and the command template exists and is enabled.
- The worker constructs a fixed SSH argv using the configured SSH executable,
  batch mode, connect timeout, host key policy, port, `user@host`, and the
  template argv. The API never accepts free-form shell input.
- stdout and stderr are capped by `LOGAGENT_V2_REMOTE_MAX_OUTPUT_BYTES`, stored
  as files, and previewed in `result.json`.
- Non-zero exit code, timeout, and SSH start failure are recorded in
  `result.status` as `FAILED` or `TIMED_OUT`. The remote run reaches
  `SUCCEEDED` when controlled execution completed and result files were
  persisted. System errors before result persistence mark the run `FAILED`.

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
range. Background context such as `manifest.json`, `system_context.json`,
metadata slices, case context, and diagnostic skill references must stay
readable context and cannot be cited as final root-cause evidence.

## Agent Provider

`LOGAGENT_V2_AGENT_PROVIDER=stub` is the default and keeps local deterministic
behavior. `openai_compatible` posts a compact Chat Completions request to
`<LOGAGENT_V2_AGENT_BASE_URL>/chat/completions` with
`LOGAGENT_V2_AGENT_MODEL`, optional `LOGAGENT_V2_AGENT_API_KEY`, and
`LOGAGENT_V2_AGENT_TIMEOUT_SECONDS`. The request includes the Workspace
question/mode/language, manifest counts, a bounded initial grep preview,
allowed current-run evidence refs, recent user messages/action results from
resumed runs, available read-only tools, and prior tool observations.

The provider may return a `tool_calls` object requesting a tool advertised in
the prompt. Advertised tools include log search/slice, Metadata, Case Memory,
Skill references, Fetch catalog, configured domain tools when present, and
Fetch execution when Fetch is enabled. Waiting/approval tools are not
advertised. V2 validates the tool name and arguments as JSON objects, executes
the Server-owned task MCP tool, records the resulting evidence/artifacts through
the existing tool implementation, and feeds the structured observation into the
next provider round. The loop is bounded by `LOGAGENT_V2_AGENT_MAX_ROUNDS` with
default 3.

The provider must eventually return one JSON object matching the final answer
schema. V2 then runs the same normalization and evidence-ref validation used by
the stub. Invalid JSON, unsupported refs, provider HTTP errors, unsupported
tool requests, or max-round exhaustion fail the run. Approval/user waiting tools
are still not advertised to the provider; full resume-aware LangGraph planning
remains future work.

Each run also writes `analysis_package.json` with schema version 1. It contains
Workspace/run metadata, task MCP resource URIs, manifest and grep outlines,
bounded tool input summaries, system/metadata context outlines, allowed
current-run evidence refs, and final-evidence policy. It intentionally omits
full Skill content, full Metadata topology, and raw uploaded text. Task MCP
exposes it at `logagent-v2://run/<run_id>/analysis_package`.

The Agent boundary is audited with schema version 1 artifacts. `agent_request`
captures the provider/stub, model, transport metadata, allowed evidence refs,
analysis package artifact id, and request payload without Authorization
headers. `agent_response` captures provider status, HTTP/body previews when
available, parsed final answer, normalized final answer, and validation status
or failure details. `analysis_state` captures the latest round status and links
the request and response artifact ids. These evidence rows are
background-only (`final_allowed=false`) and exposed through task MCP resources.

After final-answer validation succeeds, V2 writes `result.json` with schema
version 1 and `result.md` as a Markdown rendering of the same final answer.
Both are background evidence rows (`result` and `result_markdown`) and can be
read through `GET /api/v2/runs/<run_id>/result` or task MCP resources
`result` and `result_markdown`.

## Waiting States

Task MCP can now request:

```text
logagent.request_user_input
logagent.request_approval
```

Both calls create an `actions` row and append timeline events. User input moves
the run to `waiting_for_user`; approval moves it to `waiting_for_approval`.
`GET /api/v2/runs/:run_id/analysis` returns `actions` and `pendingActions` so
WebUI can render the same recovery controls as the Rust task detail page.
`POST /api/v2/runs/:run_id/messages` and
`POST /api/v2/actions/:action_id/decisions` requeue waiting runs into the
SQLite job queue. User messages also mark pending `user_input` actions as
`answered`. The next Agent request includes a bounded `interactionContext`
containing recent user messages, answered/approved/rejected actions, remaining
pending actions, and `resumeDirective=finalize_with_current_evidence` when the
user chooses finalization. If an approved action has
`actionType=collect_environment`, V2 checks the approved input for
`executorId` and `commandId`. If present and valid, it queues a
`remote_command_run` with idempotency key `environment:<action_id>`, keeps the
analysis run waiting during collection, and writes
`environment_evidence/<action_id>/result.json` with `status=COLLECTED` or
`REMOTE_FAILED`, the approved input, remote run id, remote result paths, and
bounded stdout/stderr previews. Invalid remote targets produce
`status=REMOTE_REJECTED` background evidence. When no remote target is supplied,
V2 records the V1-compatible `status=MOCK` artifact. The resource is available
from `GET /api/v2/runs/:run_id/analysis` and task MCP
`logagent-v2://run/<run_id>/environment_evidence`, and a bounded outline is
included in the next `analysis_package` and Agent prompt. The current runtime
still does not implement full LangGraph resume planning, SCP file collection,
or multi-node Environment Collector execution.

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
- Interrupted `running` jobs are recovered on startup without waiting for the
  previous lock timeout.
- `deploy/rebuild-v2-install.sh` can create the V2 virtualenv, install
  `server-v2`, initialize SQLite, sync WebUI static files, and preserve
  existing `data-v2`.
- `deploy/logagent-v2ctl.sh` can start, stop, restart, report status, and tail
  V2 logs using the same `.env` loading pattern as the Rust deploy controls.
