# LogAgent V2 Server Spec

## Goal

V2 is a clean-room small-team implementation of LogAgent. It favors a simple
single-machine deployment over distributed infrastructure:

- Python + FastAPI for the API.
- LangGraph state graph around the Agent runtime, with separate provider,
  tool-execution, validation, and result nodes.
- SQLite WAL for durable state.
- Local filesystem artifacts for large evidence.
- DB-backed jobs instead of Redis.
- Static WebUI build hosting from local filesystem.

V2 does not need to be compatible with the current Rust Server API or artifact
layout. The stable product goal remains evidence-backed diagnosis with an
auditable agent boundary.

## Product Model

- `Session`: product-facing diagnosis container. In this slice it is backed by
  `Workspace`; `sessionId` equals the Workspace id, while Rust-style Session
  fields are persisted on the Workspace row.
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
- Session-first API aliases for create/list/read/update/delete, uploads,
  restartable upload sessions, task creation/listing, and full Session
  timeline. `title`, `sourceUrl`, `instanceId`, `nodeId`, `systemContextIds`,
  `skillIds`, `analysisMode`, and language are persisted. `taskId` equals the
  underlying Run id, `activeTaskId` is the newest Run, queued Runs map to
  Session `ready`, and Session deletion is rejected while any Run is not
  `succeeded` or `failed`.
  Session uploadIds are stored as an attachment set: direct uploads auto-attach,
  JSON attach can reattach existing Workspace uploads, and detach is allowed
  only before any task run exists. Session task create/list responses return
  Rust-style TaskSummary objects with `taskId`, `taskKind`, `sessionId`,
  `analysisMode`, `analysisLanguage`, `status`, `phase`, and `url`, while
  retaining raw Run records under `runs`.
- Native Agent V2 target support: browser imports still enter the local Native
  Agent `/imports` endpoint, and `native_agent.server_api=v2` maps them to
  `POST /api/v2/sessions` plus Session-scoped upload APIs.
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
  InfluxQL query lines, generic file-level InfluxQL query lines, Flux query
  lines, and enabled storage analyzer file or directory inputs such as `.tssp`,
  `.tssp.init`, `.tsm`, `.tsi`, TSI/mergeset trees, and `_series` trees from
  direct uploads or supported archives. Generated entries are compatible with
  the V1 `ToolInputEntry` shape and include V2 artifact ids for local
  execution.
- `manifest.json` and `grep_results.json` artifact generation.
- Agent runtime that executes the run lifecycle through a LangGraph state graph
  with `collect_initial_evidence`, `prepare_agent_request`,
  `call_agent_provider`, `execute_tool_calls`, `validate_final_answer`, and
  `finalize_result` nodes. The graph records initial question evidence as
  `session_text_input.json`, consumes the initial evidence pipeline, and either
  returns a deterministic stub summary or calls a bounded OpenAI-compatible or
  local binary provider loop for advertised Server-owned tools and an
  evidence-validated JSON final answer. After manifest/grep creation and before
  the first provider request, V2 also runs matching input-based configured
  subprocess tools that do not need runtime params through a V1-style automatic
  `run_tool` phase, persists their `tool_result` evidence, and injects their
  finding refs into `analysis_package.json` and `agent_request.json`.
  Provider-requested
  `logagent.request_user_input` / `logagent.request_approval` calls pause the
  run in the matching waiting state instead of writing a final result. Each
  round persists `agent_request.json`, `agent_response.json`,
  `analysis_state.json`, and provider tool-call `mcp_calls.jsonl` audit
  artifacts before the run reaches a terminal state. `analysis_state.json`
  includes `graphRuntime.engine=langgraph`, the graph name, and node list.
  Follow-up evidence refs
  returned by tool observations are added to the next round's
  `allowedEvidenceRefs`. After successful final-answer validation, non-stub
  providers receive a separate `run_alias` JSON prompt; valid aliases are
  persisted for history/UI display, while stub mode or alias failures fall back
  to a deterministic alias derived from the final summary or question.
- `analysis_package.json` generation after initial evidence collection, exposed
  as task MCP resource for Agent loop context. The package includes Session
  title, source URL, Metadata binding, System Context ids, Skill ids, and
  attached upload ids in its `workspace` section, plus bounded `analysisState`
  resume context with `finalizeRequested`.
- Timeline events for workspace, upload, run, and evidence lifecycle.
- Artifact download.
- Evidence and artifact listing for a run, including uploaded input artifacts,
  evidence artifact outputs, and Rust/V1-style aggregate fields for manifest,
  grep results, Session text input, metadata/system/case context, analysis
  package, Agent audit artifacts, optional Claude MCP config/session artifacts,
  MCP calls, and tool results.
- Run analysis summary endpoint combining run metadata, timeline, evidence,
  artifacts, analysis resources, final result, and run alias for WebUI
  inspection.
- Read-only MCP endpoint with `initialize`, `resources/list`, `resources/read`,
  `tools/list`, and tools/resources for V1-shaped tool catalog, Metadata,
  Case Memory, Skill registry, and Domain Adapter summaries. `resources/list`
  advertises static collection resources plus dynamic per-Skill and
  per-Metadata snapshot resources for both `logagent://...` and
  `logagent-v2://...` URI schemes. Collection `resources/read` responses for
  Metadata instances, recent Cases, Skills, and Domain Adapters include
  `schemaVersion=1`. Read-only and task MCP handlers accept single JSON-RPC
  requests and JSON-RPC batch arrays; both also support `ping` and empty
  `prompts/list`.
- Task MCP endpoint with `summary`, `artifact_index`, `evidence`, `manifest`,
  `grep_results`, `system_context`, `metadata_context`, `analysis_package`,
  `analysis_state`, `agent_request`, `agent_response`, `claude_mcp_config`,
  `claude_session`, `case_context`, `tool_results`, `mcp_calls`, `result`, and
  `result_markdown` resources.
- Task MCP `logagent.search_logs`, which accepts V1-compatible optional
  `maxMatches` clamped to 1..200, creates follow-up `log_search` evidence, and
  returns stable `log_searches/<search_id>.json#matches/<index>` refs. The
  response preserves the V2 nested `search` object and also exposes
  Rust-compatible top-level `artifactPath`, `totalMatches`, `keywordCounts`,
  `unmatchedKeywords`, `matches`, `evidenceRefs`, and `note` fields.
- Task MCP `logagent.get_log_slice`, which reads bounded context from a current
  Workspace text path and persists `log_slice` evidence. It accepts the V2
  center-line form `lineNumber` plus optional `before`/`after`, and the
  V1-compatible range form `startLine`/`endLine`; the two forms must not be
  mixed in one call. The response preserves the V2 nested `slice` object and
  also exposes Rust-compatible top-level `artifactPath`, `evidenceRefs`, and
  `lines` fields.
- Tool Plugin registry. Configured subprocess tools are loaded from
  `LOGAGENT_V2_TOOLS_JSON`, explicit V2 analyzer executable environment
  variables, or standard source-built analyzer filenames auto-discovered under
  `LOGAGENT_V2_TOOLS_DIR` / `$LOGAGENT_V2_APP_DIR/bin/tools`, listed through
  `/api/v2/tools`, runnable through manual tool-run APIs, and exposed to task
  MCP `logagent.run_domain_tool`. Tools with `{input_file}`
  can use explicit `inputFile`/`inputFiles` workspace selectors, otherwise
  consume matching materialized tool inputs before execution, then fall back to
  manifest file patterns, initial grep keyword matches, or raw upload artifacts
  for storage analyzers. Enabled storage analyzer materialized inputs are safe
  artifact files or directory bundles extracted from direct uploads and
  archives. Generic JSON stdout and InfluxQL analyzer report/compare stdout are
  normalized into `summary/findings`. Task MCP responses retain the V2 nested
  `result/artifact/evidence` shape and add Rust/V1-compatible `artifactPath`,
  `summary`, and `evidenceRefs` top-level aliases; multi-input runs also return
  `artifactPaths`, and finding outputs expose `finalEvidenceRefs`. Repeated task
  MCP calls for an existing `toolId + actionId` reuse the current run's
  persisted result evidence to keep Agent retries idempotent.
- Source-built analyzer env vars or runtime `bin/tools` auto-discovery create
  default configured descriptors aligned with `examples/server-tools.yaml`:
  Flux and InfluxQL use the V1 query analyzer args with `timeoutSeconds=30`
  and `maxInputFiles=3`, openGemini storage uses the full TSSP/TSI/mergeset
  file-pattern set with `maxInputFiles=10`, and InfluxDB storage uses
  `timeoutSeconds=60`, `maxInputFiles=5`, and the V1 TSM/TSI patterns.
  JSON-configured commands and source-built analyzer paths expand environment
  variables and `~` during configuration loading; enabled tools must resolve
  to absolute commands before they enter the registry.
- V1 built-in tool migration for metadata catalog tools,
  `logagent.preprocess_log_package`, `logagent.fetch`, and default-off
  `logagent.huawei_cloud_package_sync`, plus the V1-style configured command
  adapter `pprof_analyzer`. Metadata field filters use the Rust/V1
  trim-and-reject-empty-array-entry semantics, and tag-field tools reject the
  `field` parameter.
- Huawei package sync descriptors match Rust/V1 by using
  `acceptedSuffixes=["*"]`; execution still requires exactly one completed
  upload and validated object-key / SQL params. Worker execution revalidates
  params and writes Rust/V1-style result fields (`tool`, `input`, `obs`,
  `gaussdb`, `sql`, `timings`, `warnings`, `credentialMetadata`, and logical
  `evidenceRefs`) while preserving V2 `obsPut`, `obsHead`, `gaussdbUpdate`,
  and `gaussdbQuery` fields.
- `pprof_analyzer` catalog metadata matches the Rust/V1 configured command
  shape (`source=configured`, `backend=command`) while remaining manual-only in
  V2. Tool-run results preserve V2 artifact ids and include Rust/V1-style
  `artifactPaths` for top/tree/raw/stderr/SVG outputs, plus parsed
  `profileType`, `total`, and top table rows.
- Fetch endpoint foundation. Endpoints are stored in SQLite, listed and managed
  through protected HTTP APIs, importable from DevTools bash cURL, exposed as a
  built-in `/api/v2/tools` descriptor, and executable through task MCP
  `logagent.fetch` only when enabled and allowlisted. Runtime calls accept
  `endpointId` or V1-compatible `fetchId`, URL template variables, temporary
  headers, and body override. Sensitive endpoint material is split into
  encrypted credential sets.
- Waiting-state foundation through task MCP `logagent.request_user_input` and
  `logagent.request_approval`; pending actions are persisted, exposed in run
  analysis summaries, user supplements mark pending user-input actions as
  answered, and user message/approval APIs requeue the run with bounded
  `interactionContext` in the next Agent request. User message submission
  requires `waiting_for_user`, validates optional `questionId`, and de-duplicates
  retry requests by `idempotencyKey`. Approval decisions require
  `waiting_for_approval`, target a pending approval action, and also
  de-duplicate retries by `idempotencyKey`. Calls also persist a
  V1-compatible `mcp_waiting_request.json` background artifact and return
  `artifactPath`, `runtimeStatus`, and `evidenceRefs`; `request_approval`
  accepts the V1 shape with only `reason` and defaults missing `actionType` to
  `manual_approval`. Approved
  `collect_environment` actions either record V1-compatible MOCK
  `environment_evidence` background artifacts or, when the action input targets
  an enabled Remote Executor and whitelisted command, queue a remote command and
  record the completed command output before resuming the analysis run.
- Final answer schema normalization and evidence ref validation. A run can only
  be marked `succeeded` after final refs point to current-run, final-allowed
  `session_text_input.json#question`, log search, log slice, Fetch response, or
  tool finding evidence; recalled Case context is accepted through
  `case_context.json#cases/<index>`.
- Final result persistence as `result.json` and `result.md` background
  artifacts, exposed through HTTP and task MCP resources.
- Metadata foundation with direct JSON/YAML/openGemini content import,
  allowlisted URL fetch, preview/confirm draft workflow, SQLite snapshot
  storage, saved raw snapshot refresh, field/tag type query APIs, explicit
  Session `instanceId` / `nodeId` binding for per-run `metadata_context`,
  auto-selection fallback, readonly MCP tools, and task MCP V1-compatible
  topology alias plus bounded background slices.
- Case Memory foundation with manual Case creation, succeeded-run Case
  confirmation, text/JSON import drafts, follow-up import messages, SQLite FTS5/BM25 recall,
  local hash-vector recall, edit/disable API, readonly MCP search, and task MCP
  V1-compatible Case recall background context.
- Skill-backed System Context foundation with filesystem Skill registry,
  Markdown import, explicit or auto-matched Workspace skill selection, per-run
  `system_context` artifact, explicit Session `systemContextIds` materialized
  from legacy System Context resources, readonly MCP Skill tools, and task MCP
  reference artifacts.
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
- WebUI V2 cutover: the default React routes render V2 Analyze, Memory,
  System Context, Metadata, Tools, Fetch, Executors, and Settings surfaces
  directly against `/api/v2/*` instead of rendering the legacy Rust-compatible
  panels alongside V2 bridge panels.

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
POST /api/v2/sessions
GET  /api/v2/sessions
GET  /api/v2/sessions/:session_id
PATCH /api/v2/sessions/:session_id
DELETE /api/v2/sessions/:session_id
GET  /api/v2/workspaces/:workspace_id/uploads
GET  /api/v2/workspaces/:workspace_id/upload-sessions
GET  /api/v2/workspaces/:workspace_id/runs
GET  /api/v2/sessions/:session_id/uploads
GET  /api/v2/sessions/:session_id/upload-sessions
GET  /api/v2/sessions/:session_id/tasks
GET  /api/v2/sessions/:session_id/timeline
POST /api/v2/workspaces/:workspace_id/uploads
POST /api/v2/workspaces/:workspace_id/uploads/batch
POST /api/v2/workspaces/:workspace_id/uploads/init
POST /api/v2/sessions/:session_id/uploads
POST /api/v2/sessions/:session_id/uploads/batch
POST /api/v2/sessions/:session_id/uploads/init
DELETE /api/v2/sessions/:session_id/uploads/:upload_id
GET  /api/v2/uploads/:session_id
POST /api/v2/uploads/:session_id/chunks?offset=<bytes>
POST /api/v2/uploads/:session_id/complete
POST /api/v2/workspaces/:workspace_id/runs
POST /api/v2/sessions/:session_id/tasks
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
GET  /api/v2/metadata/clusters/:cluster_id
GET  /api/v2/metadata/clusters/:cluster_id/nodes
GET  /api/v2/metadata/imports
GET  /api/v2/metadata/imports/:import_id
POST /api/v2/metadata/imports/preview
POST /api/v2/metadata/imports/fetch/preview
POST /api/v2/metadata/imports/:import_id/confirm
POST /api/v2/metadata/snapshots/fetch
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
PATCH /api/v2/cases/imports/:import_id
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
POST /api/v2/fetch/endpoints/:endpoint_id/runs
GET  /api/v2/fetch/runs
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

Native Agent V2 mode uses `POST /api/v2/sessions/:session_id/uploads` for small
imports and `POST /api/v2/sessions/:session_id/uploads/init` plus the upload
session chunk/complete endpoints for larger imports. It returns the final
`upl_...` id after chunk completion, while the temporary upload session keeps
its `ups_...` id.

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
  -> agent_request.json / agent_response.json / analysis_state.json / mcp_calls.jsonl audit
  -> stub, OpenAI-compatible, or binary JSON final answer
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
  -> MCP logagent.run_domain_tool { toolId|tool, inputFile?, params? }
  -> optional paramsSchema validation
  -> optional explicit or materialized tool input selection
  -> materialized tool workspace with manifest/grep/tool_inputs view
  -> expanded fixed absolute command + fixed args with V1 placeholder substitution
  -> tool_result artifact/evidence
```

The model cannot submit executable paths, shell snippets, dynamic argv, or
environment overrides.

`LOGAGENT_V2_TOOLS_JSON` accepts either a descriptor array or a Rust/V1-style
object keyed by tool id. Descriptors may use V2 `command`, V1 `path`, or V1
`path_env` / `pathEnv`, and may use camelCase or snake_case limit fields such
as `timeoutSeconds` / `timeout_seconds`, `maxOutputBytes` / `max_output_bytes`,
and `maxInputFiles` / `max_input_files`. V2 expands `${ENV}` / `$ENV` variables
and `~` in configured command paths and in source-built analyzer executable
environment variables. When explicit source-built analyzer env vars are unset,
V2 auto-discovers the standard analyzer filenames from `LOGAGENT_V2_TOOLS_DIR`,
`$LOGAGENT_V2_APP_DIR/bin/tools`, or `$LOGAGENT_APP_DIR/bin/tools`. If an
enabled configured tool does not resolve to an absolute command path, settings
loading fails before the descriptor is exposed through HTTP, readonly MCP, or
task MCP surfaces. Disabled descriptors may retain unresolved or relative
commands because they are not runnable or exported.
User-configured tool IDs follow the Rust/V1 `tools.<name>` safe pattern:
non-empty ASCII letters, digits, `_`, and `-` only. Built-in `logagent.*` tools
are fixed server capabilities and are not loaded from `LOGAGENT_V2_TOOLS_JSON`.
Configured `match.filePatterns` and `match.keywords` are normalized to
lowercase during settings loading, so HTTP/MCP descriptors expose the same
catalog shape as Rust/V1.

The Tool Plugin registry is the single catalog source for `/api/v2/tools`,
readonly MCP `logagent.list_tools`, manual tool-run validation, and task MCP
configured tool execution. Task MCP `logagent.run_domain_tool` only exposes
configured subprocess tools. Its `tools/list` descriptor input schema must
advertise both the V2 `toolId` call shape and the Rust/V1 `tool + inputFile`
call shape with `anyOf`. The OpenAI-compatible and binary Agent provider
`availableTools` prompt must advertise the same schema and configured-tool enum,
and must exclude manual-only tools such as `pprof_analyzer`. Built-ins use
dedicated task MCP tools where available, or the protected manual Tools API.
The migrated built-ins are:

- metadata catalog tools: instance list, snapshot, field types, tag fields,
  using Rust/V1 `backend=builtin`, `read-only` / `manual-run` tags, and
  params templates with `retentionPolicy` where supported;
- `logagent.preprocess_log_package`, whose descriptor advertises rotated log
  normalization and `outputViews=["summary","nodes","log_groups","tool_inputs","warnings"]`;
  results include V1-style `nodes` aggregation plus the V2 `nodePackages`
  detail list;
- `logagent.fetch`, whose catalog descriptor keeps the Rust/V1 manual-run
  shape: `readOnly=false`, `paramsTemplate.fetchId`, `body=null`, and
  `outputViews=["summary","request","response","body_artifact"]` while runtime
  calls still accept either `endpointId` or `fetchId`;
- `pprof_analyzer`;
- `logagent.huawei_cloud_package_sync`, disabled until Huawei OBS/GaussDB
  environment variables are configured and pass startup validation. Its catalog
  descriptor uses the Rust/V1 display name `Huawei OBS + GaussDB Package Sync`,
  the `huawei-cloud` tag, and
  `outputViews=["summary","obs","gaussdb","json"]`. V2 validates OBS endpoint
  scheme/shape, bucket characters, safe object prefix, required OBS keys, and
  required GaussDB DSN when package sync is enabled. Results include the V1
  `input`, `obs`, `gaussdb`, `sql`, `timings`, `warnings`, credential metadata,
  and logical `tool_results/<action_id>/result.json` evidence reference.
Regression coverage must lock the V1 built-in tool names across task MCP,
readonly MCP, and the manual Tools catalog, so future refactors cannot drop a
migrated built-in from one surface accidentally.

Manual tool runs create `kind=tool_run` rows in `runs` and `tool_run` jobs in
the DB-backed queue. They accept `workspaceId`, optional `uploadIds`, and
validated `params`; results are stored as V2 artifacts/evidence and exposed
through `/api/v2/tools/runs/:run_id/result`. Configured tools with
`{input_file}` may pass reserved `params.inputFiles` to select existing
Workspace inputs without re-uploading files.
When uploads are attached, V2 validates both the upload count and each uploaded
filename against the selected tool descriptor's `acceptedSuffixes`. Descriptor
values may be suffixes such as `.tar.gz`, glob-style patterns such as `*.log`,
or `*` for unrestricted single-upload built-ins.

Readonly MCP `logagent://tools/catalog`, retained `logagent-v2://tools/catalog`,
and `logagent.list_tools` expose the same catalog payload shape used by the
Rust server: `schemaVersion`, complete `tools` descriptors, and
`configuredTools` summaries containing configured args, timeout, match rules,
and `maxInputFiles`. This readonly surface is catalog-only and cannot execute
configured or built-in tools. Static readonly resources support both
`logagent://...` and `logagent-v2://...` URIs, and dynamic skill/metadata
snapshot reads accept the same aliasing.

Configured tools may declare `paramsSchema`. V2 validates a conservative object
schema subset: required fields, `additionalProperties=false`, primitive
`type`, arrays, and `enum`. Validated params are recorded in `tool_result`
artifacts/evidence and can be substituted into configured argv with
`{params.<name>}` placeholders. For `{input_file}` tools, V2 augments the
descriptor with reserved `inputFiles` but never substitutes it into argv unless
the configured args explicitly contain `{input_file}`. Params affect the stable
action id so different parameter sets do not reuse one result path. Configured
subprocess actions run with `cwd` set to a per-action materialized workspace
under `data_dir/tmp/tool_workspaces/<workspace_id>/<run_id>/<action_id>/`.
Before execution V2 copies the current run's `manifest.json`,
`grep_results.json`, and, when present, `tool_inputs/index.json` into that
workspace. It expands the Rust/V1 command placeholders `{workspace}`,
`{manifest_path}`, `{grep_results_path}`, `{action_id}`, `{input_file}`, and
`{params.<name>}`; unsupported placeholder-like tokens fail before subprocess
execution. Configured
tool descriptors must retain the Rust/V1 catalog semantics:
`source=configured`, `backend=command`, `readOnly=false`, `editable=true`,
`exportable=enabled`, `minFiles=1`, and `acceptedSuffixes` copied from
`match.filePatterns`. Their `paramsSchema` must expose Rust/V1 read-only
`configuredArgs` and `match` entries, and V2 additionally mirrors those entries
under `properties` so schema-oriented clients can render them alongside
reserved `inputFiles` and any configured custom params.

Configured subprocess `result.json` must retain the Rust/V1 `ToolRunRecord`
record shape: `schemaVersion=2`, `tool`, `actionId`, `status`, `exitCode`,
`durationMs`, `command`, `inputFile`, `stdoutPath`, `stderrPath`, `summary`,
`findings`, and `error`. V2 may include additive fields such as `toolId`,
`displayName`, `params`, `argv`, `stdoutPreview`, `stderrPreview`,
`parsedStdout`, `stdoutArtifactId`, and `stderrArtifactId`. The logical
`stdoutPath` / `stderrPath` must keep the Rust/V1
`tool_results/<action_id>/stdout.txt` and `stderr.txt` shape, while the artifact
IDs reference the actual V2 persisted stdout/stderr files. Non-zero exits,
timeout, and subprocess spawn failures must be persisted as `FAILED` /
`TIMED_OUT` tool results rather than surfacing as a missing-result MCP failure.

Tool stdout is parsed as JSON when possible. Generic JSON output supports
`summary` / `message` / `title`, `findings` / `issues` / `diagnostics`, and
finding fields `severity` / `level` / `status`, `file` / `path` / `filename`,
`line` / `lineNumber` / `startLine`, and `message` / `summary` /
`description` / `detail` / `title` / `cause`.

InfluxQL analyzer report stdout is specially adapted. Summary includes
`total_records`, `records_in_window`, `total_statements`, `parse_error_count`,
and `special_rules`. Findings include special rule hits, parse errors,
realtime classification, and notable fingerprints. Report detection follows
Rust/V1 key presence semantics: `total_records`, `total_statements`, and
`fingerprints` keys are enough to enter the specialized parser, even when
`fingerprints` is not an array. InfluxQL compare report stdout is also adapted:
`statement_delta`, `qps_delta`, `batch_a`, and `batch_b` go into summary, while
new/removed/changed fingerprints and `rule_deltas` become findings.

`GET /api/v2/exports/tools.zip` exports enabled configured subprocess tools
from `LOGAGENT_V2_TOOLS_JSON` and the enabled `pprof_analyzer` Go executable.
The pprof adapter is disabled by default unless `LOGAGENT_V2_PPROF_GO_COMMAND`
or `LOGAGENT_TOOL_PPROF_GO` is configured, or `LOGAGENT_V2_PPROF_ENABLED=1` is
used with an absolute Go command path. Its descriptor must expose V1 top-level
`sampleIndex`, `nodeCount`, and `generateSvg` params schema entries plus the V2
`properties` mirror. `sampleIndex` is trimmed and must contain only letters,
digits, `_`, or `-`; `generateSvg` must be a JSON boolean; `nodeCount` is
clamped to 1..200. The subprocess argv must match Rust/V1: top/tree/svg pass
`-nodecount=<nodeCount>`, and top/tree/raw/svg all pass `-symbolize=none`.
Built-in tools are not packaged. The
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

When a tool arg contains `{input_file}`, V2 first honors explicit selectors
from task MCP top-level `inputFile`, `params.inputFiles`, or manual tool-run
`params.inputFiles`. Selectors are workspace-relative only and resolve to
current Workspace text paths, their `extracted/...` virtual paths, or
`tool_inputs/...` entries from the current run's latest `tool_input_index`.
Without explicit selectors, V2 selects entries from that latest
`tool_input_index` evidence whose `toolIds` contain the requested tool id. The
placeholder is replaced with the resolved artifact path. Each input creates a
stable action id derived from tool id plus virtual input path, and the evidence
ref prefix becomes:

```text
tool_results/<tool_id>_<input_hash>/result.json#findings/
```

If no explicit selector is provided and no materialized input matches, V2 falls
back to current-run text files.
Manifest paths matching `match.filePatterns` are selected first. If capacity
remains, initial `grep_results.json` matches whose line text contains one of
`match.keywords` select additional files. Fallback files are materialized as
run-local `logagent.v2.tool_input.text_file.v1` artifacts and exposed to tools
as virtual `extracted/<manifest path>` inputs. Selection is de-duplicated,
bounded by `maxInputFiles`, and preserves materialized-input priority.
Multi-input MCP responses keep `result/evidence` for the primary execution and
add `results[]` and `evidenceItems[]`.

Storage analyzers (`opengemini_storage_analyzer` and
`influxdb_storage_analyzer`) first consume materialized storage inputs when
enabled. V2 safely extracts direct upload files, archive member files, and
archive directory bundles such as TSI/mergeset and `_series` trees into
artifact-backed `tool_inputs/storage/` or `tool_inputs/storage_dirs/` paths;
when none match, raw upload artifact fallback still lets uploaded
TSSP/TSI/TSM/_series payloads pass directly to source-built analyzer binaries.

## Fetch Endpoints

V2 Fetch endpoints are stored in SQLite table `fetch_endpoints` with name,
method, redacted URL, redacted headers, optional redacted body material, enabled
flag, `followRedirects`, and timestamps. Sensitive request material is stored
separately in `fetch_credential_sets` as encrypted JSON using
`LOGAGENT_V2_FETCH_SECRET_KEY`. The public API returns redacted endpoint
previews; raw request material is only hydrated inside the server-side executor.

Endpoints can be created directly or imported from DevTools bash cURL commands:

```text
POST /api/v2/fetch/imports/preview
POST /api/v2/fetch/imports
```

The cURL importer supports request method, headers, body, cookies,
`--compressed`, `--head`, and `--location`, and accepts a leading `$` shell
prompt from copied terminal commands. `--location` sets `followRedirects=true`;
otherwise imported and directly created endpoints default to no redirect
following. It rejects unsupported flags such as form uploads, proxy, cert, file,
or resolver options rather than widening the network or filesystem boundary.
Import previews redact sensitive query, header, and JSON/form body fields and
return detected sensitive field locations.

When Fetch execution is enabled, settings loading requires
`LOGAGENT_V2_FETCH_SECRET_KEY` to be a valid Fernet 32-byte base64 key. If a
URL query parameter, header, or body field name looks like a token, secret,
password, API key, session, Authorization, or Cookie, creating or updating the
endpoint uses that key to encrypt the sensitive values before the endpoint row
is stored.

Fetch execution is disabled by default. To execute endpoints, set:

```text
LOGAGENT_V2_FETCH_ENABLED=1
LOGAGENT_V2_FETCH_ALLOWED_HOSTS=127.0.0.1,example.internal:8080
LOGAGENT_V2_FETCH_MAX_REQUEST_BYTES=1048576
LOGAGENT_V2_FETCH_MAX_REDIRECTS=5
LOGAGENT_V2_FETCH_SECRET_KEY=<fernet-32-byte-base64-key>
```

Only `http` and `https` URLs are supported. `LOGAGENT_V2_FETCH_ALLOWED_HOSTS`
must be non-empty when Fetch is enabled. Allowlist entries support exact
`host`, `host:port`, or scheme-specific `http(s)://host[:port]` forms.
URL-form entries pin both scheme and port, using the default port when omitted.
Fetch timeout, request-byte cap, and response-byte cap use a minimum value of
1; maximum redirects uses a minimum value of 0.
Controlled headers such as `Host`,
`Content-Length`, `Transfer-Encoding`, and `Connection` are rejected when
endpoints are saved. Sensitive headers, query parameters, and JSON/form body fields
containing token/secret/password/api key style names are redacted from API, MCP,
and artifact previews.

Redirects are not followed unless the endpoint has `followRedirects=true`. When
enabled, redirects are followed manually up to
`LOGAGENT_V2_FETCH_MAX_REDIRECTS`. Every redirect target is revalidated with the
same scheme/host allowlist before the next request is sent. Sensitive headers
such as Authorization, Cookie, and X-Api-Key are stripped when a redirect
crosses origin. Each response artifact records `finalUrl`, `redirectCount`, and
redacted redirect hops.

Task MCP exposes:

```text
logagent.list_fetch_endpoints
logagent.fetch { endpointId | fetchId, variables?, headers?, body? }
```

`logagent.list_fetch_endpoints` must fail with
`fetch is disabled by configuration` when Fetch execution is disabled. When
enabled, it returns the Rust/V1-compatible envelope with `schemaVersion=1`,
enabled endpoint summaries, `fetchId`, `urlTemplate`, `credentialVersion`, and
`finalEvidenceAllowed=false`.

`logagent.fetch` writes a `fetch_result` artifact/evidence item. Network errors
produce a failed Fetch result rather than crashing the run. HTTP 4xx/5xx
responses are stored as responses.

Runtime `variables` must be a string map whose keys are ASCII
`[A-Za-z0-9_]`; they replace `{name}` placeholders in the endpoint URL before
allowlist validation, and unresolved `{...}` placeholders fail the run.
Runtime `headers` must be a string map and are merged over saved endpoint
headers for that single request; controlled headers are rejected. Runtime
`body`, when present, overrides the saved endpoint body for that request. Saved
endpoint bodies and runtime body overrides must be rejected before HTTP
execution when their UTF-8 byte size exceeds
`LOGAGENT_V2_FETCH_MAX_REQUEST_BYTES`.
`GET /api/v2/fetch/runs` lists persisted Fetch tool runs without executing a
request and supports `endpointId`, `fetchId`, V1-style `fetch_id`,
`workspaceId`, and `limit` filters. `POST
/api/v2/fetch/endpoints/:endpoint_id/runs` queues a Fetch `tool_run`, validates
the endpoint and runtime params, reuses a provided `workspaceId`, or creates an
isolated workspace when no workspace is provided.
Result artifacts include redacted request metadata, top-level `httpOk`,
`statusCode`, `redirectCount`, `finalUrl`, `truncated`, `credentialVersion`,
and a separate bounded response body artifact referenced by both logical
`tool_results/<action_id>/response_body.bin` and actual V2 artifact id/path
fields.

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
`GET /api/v2/metadata/clusters/:cluster_id` and `/nodes` derive V1-style
cluster detail and node-list views from persisted V2 snapshots.
`POST /api/v2/metadata/snapshots/fetch` fetches, parses, and normalizes a
remote snapshot without creating an import draft or mutating
`metadata_instances`.

URL fetch import uses the same default-off Fetch boundary. It requires
`LOGAGENT_V2_FETCH_ENABLED=1`, an exact host or host:port match in
`LOGAGENT_V2_FETCH_ALLOWED_HOSTS`, and the shared Fetch timeout and response-size
limits. V2 uses GET only, rejects redirects, redacts sensitive query parameters
in draft `sourceUrl`, and then runs the fetched content through the same
normalization and preview/confirm path. The Fetch request-size limit primarily
applies to configured endpoint execution because metadata URL fetch uses GET.

Task MCP and readonly MCP expose `logagent.get_metadata_field_types` and
`logagent.get_metadata_tag_fields`. Task calls write Rust/V1-compatible
background slices to `metadata_slices/field_types_<stable_id>.json` and
`metadata_slices/tag_fields_<stable_id>.json`, return `artifactPath`,
`backgroundRef`, `evidenceRefs`, `finalEvidenceAllowed=false`, and keep both
the V2 top-level `fields` shape and the Rust/V1 `result` wrapper. Readonly MCP
returns the same `result` wrapper without writing task artifacts. The field
filter schema uses the Rust/V1-compatible `oneOf` form: either one string or a
non-empty string array. Field filters are trimmed; a blank string is treated as
omitted, array entries must be non-empty after trim, and
`logagent.get_metadata_tag_fields` rejects `field`.

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
type lookups, and tag-field lookups. Task MCP exposes the same catalog tools
plus V1-compatible `logagent.get_metadata_topology` and
`logagent.query_metadata`. `get_metadata_topology` returns the current run
outline with section counts and query hints. `query_metadata` reads bounded
`overview`, `nodes`, `databases`, `retention_policies`, `measurements`,
`fields`, `shard_groups`, `shards`, `index_groups`, `indexes`, and
`partition_views` slices from the run-selected snapshots using
section-specific filters and `limit`/`cursor`. Task MCP Metadata calls write
results as `metadata_slice` evidence with `final_allowed=false`; Metadata is
background context and cannot be cited by final answers as root-cause evidence.

Run startup writes `metadata_context.json` as background evidence and exposes it
through task MCP resource `logagent://task/<run_id>/metadata_context`, with
`logagent-v2://run/<run_id>/metadata_context` retained as an alias. The
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
assistant question when required fields are still missing. `PATCH
/api/v2/cases/imports/:import_id` applies manual draft corrections and
recomputes validation errors without mutating `cases`; confirmed imports reject
further patch attempts. `POST /api/v2/cases/imports/:import_id/confirm` may
provide field overrides; only a complete confirmed draft creates a `manual`
Case and updates the FTS index. Re-confirming an already confirmed import
returns the existing Case.

Search is dependency-light and local: V2 maintains a SQLite FTS5 table beside
`cases` and ranks query matches with `bm25`. The indexed text covers `title`,
`symptom`, `rootCause`, `solution`, product/version/environment, instance/node,
and evidence refs. V2 also stores a normalized hash vector derived from tokens
and character trigrams; query search merges FTS hits with vector recall and can
return vector-only hits when exact tokens do not match. If FTS5 is unavailable,
V2 falls back to token-overlap scoring plus vector recall. Disabled cases are
excluded by default and can be included with `includeDisabled=true`.

Readonly MCP exposes `logagent.search_cases` and `logagent.get_case`. Task MCP
exposes the same tools plus V1-compatible `logagent.recall_cases`, which only
returns enabled Cases. Task MCP Case calls write results as `case_context`
evidence with `final_allowed=false`. Historical Cases are background
references; final answers still need current-task evidence refs.

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
the current registry. Readonly `preview_system_context` accepts `skillIds`,
`product`, `version`, `environment`, and `instanceId`, returning combined
`resources`, separated `skillResources` / `systemResources`, and a `prompt`
preview without writing a run. Task MCP exposes the same tools, but
`logagent.get_skill_reference` is constrained to Skills and references captured
in the current run's `system_context` snapshot and persists `skill_reference`
evidence with `final_allowed=false`. Task responses must include the
Rust/V1-compatible background artifact envelope: stable `artifactPath`,
`backgroundRef`, `canonicalRef`, `evidenceRefs`, `skillRevision`, reference
metadata, `truncated`, and `finalEvidenceAllowed=false`.

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
  local stub model; `binary` returns the configured or reserved local model;
  `openai_compatible` calls the configured `/models` endpoint.
- `POST /api/v2/settings/llm/chat` sends one bounded test message. `stub`
  returns a deterministic acknowledgment; `binary` invokes the configured local
  executable and parses stdout as a final answer; `openai_compatible` calls the
  configured `/chat/completions` endpoint with the V2 max output token limit.
- `GET /api/v2/settings/agent-backends` summarizes the in-process V2 Agent
  runtime as `logagent_v2_agent` and returns `graphRuntime` metadata for the
  LangGraph engine, graph name, and node list used by analysis runs.
- `POST /api/v2/settings/agent-backends/:backend_id/test` performs a dry-run
  configuration diagnostic only. It must not execute shell commands. For
  `binary`, it validates that the configured path is absolute, regular, and
  executable. It returns the same `graphRuntime` metadata.
- `GET /api/v2/settings/domain-adapters` returns the built-in adapter registry:
  `opengemini_influxdb` is active, while `cassandra` and `rocksdb` are
  skeleton adapters.

Readonly MCP must expose the same Domain Adapter summaries through
`logagent-v2://domain-adapters` and `logagent.list_domain_adapters`.

`GET/PUT /api/v2/debug/llm` controls process-local model response-content
logging. It is off by default, resets on restart, and may only log response
content to stderr; prompts, headers, and API keys must never be logged. V2 must
keep route-level regression coverage for both reading and updating this flag,
matching the Rust/V1 debug API capability.

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
- Command template descriptors must match the Rust/V1 behavior: `enabled`
  combines global remote execution state with template state, and
  `timeoutSeconds` is always the template override or default remote command
  timeout.
- Command template IDs must match the Rust/V1 safe pattern: non-empty ASCII
  letters, digits, `_`, and `-` only.
- Command template argv is normalized like Rust/V1: every item is trimmed,
  empty items are dropped, and the final argv must still be non-empty.
- Creating a run validates that remote execution is enabled, the executor is
  enabled, and the command template exists and is enabled.
- The worker constructs a fixed SSH argv using the configured SSH executable,
  batch mode, connect timeout, host key policy, port, `user@host`, and the
  template argv. The API never accepts free-form shell input.
- `LOGAGENT_V2_REMOTE_SSH_COMMAND` defaults to `/usr/bin/ssh`, expands
  environment variables and `~`, and must resolve to an absolute path when
  remote execution is enabled. If remote execution is disabled, a relative
  command may remain in configuration but cannot be executed.
- `LOGAGENT_V2_REMOTE_HOST_KEY_POLICY` is normalized to lower-case at startup
  and must be one of `accept-new`, `strict`, or `no`, matching the Rust/V1
  `remote_execution.host_key_policy` validation. Unknown values fail settings
  loading instead of falling back to a default.
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
`likelyRootCauses[].evidenceRefs`, normalizes current-run Case id aliases such
as `历史案例 case_<id>` to canonical `case_context.json#cases/<index>`, then
verifies every ref against evidence rows visible to the current run. Most refs
must resolve to `final_allowed=true` evidence; Case context refs resolve against
the current run's `case_context` artifact.

Accepted ref formats:

```text
session_text_input.json#question
grep_results.json#matches/<index>
log_searches/<search_id>.json#matches/<index>
log_slices/<slice_id>.json#lines
case_context.json#cases/<index>
tool_results/<tool_id>/result.json#findings/<index>
tool_results/<fetch_action_id>/result.json#response
```

The referenced artifact must exist and the match/finding index must be in
range. Background context such as `manifest.json`, `system_context.json`,
metadata slices, case context, and diagnostic skill references must stay
readable context and cannot be cited as final root-cause evidence.

## Agent Provider

`LOGAGENT_V2_AGENT_PROVIDER=stub` is the default and keeps local deterministic
behavior. `openai_compatible` posts a compact Chat Completions request to
`<LOGAGENT_V2_AGENT_BASE_URL>/chat/completions` with
`LOGAGENT_V2_AGENT_MODEL`, `LOGAGENT_V2_AGENT_API_KEY`, and
`LOGAGENT_V2_AGENT_TIMEOUT_SECONDS`. Environment-loaded settings fail fast if
the provider is unsupported, if `openai_compatible` is missing base URL, model,
or API key, if `binary` is missing a command path that resolves to an absolute
path, or if `claude_code` is missing
`LOGAGENT_V2_CLAUDE_CODE_PATH` / `LOGAGENT_CLAUDE_CODE_PATH` resolving to an
absolute path. The compact prompt used by non-Claude providers includes the Workspace
question/mode/language, manifest counts, a bounded initial grep preview,
allowed current-run evidence refs, recent user messages/action results from
resumed runs, available read-only tools, and prior tool observations.

`binary` invokes the absolute executable configured by
`LOGAGENT_V2_AGENT_BINARY_PATH` without a shell, using fixed argv:

```text
<binary_path> run <prompt>
```

The same compact prompt is passed as one argv item. stdout must be UTF-8 JSON
containing either a tool-call request object or the final-answer object.
`LOGAGENT_V2_AGENT_BINARY_MAX_OUTPUT_BYTES` bounds stdout. Runtime diagnostics
still report non-regular or non-executable paths, start failures, timeout,
non-zero exit, oversized stdout, invalid UTF-8, invalid JSON, and schema or
evidence-ref failures as provider failures.

`claude_code` invokes the configured Claude Code CLI without a shell. The
provider writes `claude_prompt.md` and `claude_mcp_config.json` into
`data_dir/tmp/claude_sessions/<run_id>/`, sets `LOGAGENT_V2_API_KEY` only in
the child process environment, and uses fixed argv:

```text
<claude_code_path> --print --output-format json --json-schema <schema>
  --mcp-config claude_mcp_config.json --strict-mcp-config
  --permission-mode <mode> --tools <tools>
  [--allowedTools <csv>] [--disallowedTools <csv>]
```

The stdin prompt is the short Claude startup instruction and must tell Claude
to begin with MCP `resources/list` and then read the task `analysis_package`
resource. Full log text, full Metadata topology, and large tool context must
stay in task MCP resources/artifacts rather than argv or stdin.
The CLI permission policy must be selected from the current Workspace
`mode`/analysis mode. Defaults match Rust/V1:
`diagnose` uses `dontAsk`, `tools=""`, allows only `mcp__logagent__*`, and
disallows `Bash/Edit/Write/Read/Grep`; `code_investigation` uses `dontAsk`,
`tools="Read,Grep,Bash"`, allows `Read/Grep/Bash/mcp__logagent__*`, and
disallows `Edit/Write`; `fix` uses `acceptEdits`,
`tools="Read,Grep,Bash,Edit,Write"`, allows those native tools plus
`mcp__logagent__*`, and has no default disallowed tools. Flat
`LOGAGENT_V2_CLAUDE_CODE_PERMISSION_MODE` / `TOOLS` / `ALLOWED_TOOLS` /
`DISALLOWED_TOOLS` remain a compatibility override for the `diagnose` profile;
`LOGAGENT_V2_CLAUDE_CODE_PERMISSION_PROFILES_JSON` can override any profile
by mode key and V2 must still auto-add `mcp__logagent__*` to `allowedTools`.
`agent_request.json`, `agent_response.json`, and runtime `claude_session.json`
must record `analysisMode`, `permissionProfile`, and `nativeToolPolicy`.
`LOGAGENT_V2_CLAUDE_CODE_MAX_OUTPUT_BYTES` bounds stdout. Claude stdout may be
a native Claude envelope whose `structured_output`, `structuredOutput`, or
`result` contains a structured outcome. V2 accepts `runtimeStatus=completed`
/ `succeeded` / `final_answer` with `finalAnswer`, `waiting_for_user` with
`pendingPrompt`, and `waiting_for_approval` with `pendingApproval`. Waiting
outcomes are converted into the existing `logagent.request_user_input` and
`logagent.request_approval` task MCP tool calls before normal pause handling.
If the previous Claude response stored `response.sessionId`, resumed runs pass
that value as `--resume <session_id>` on the next Claude Code CLI invocation and
record `response.resumedSessionId` in the next `agent_response` audit artifact.
Claude envelope `usage` and `total_cost_usd` / `totalCostUsd` must be preserved
under `response.usage` and `response.cost.usd`. When the response contains
session metadata, V2 must write a fresh `claude_session.json` runtime artifact
with the latest `claudeSessionId`, optional `resumedSessionId`, usage/cost,
prompt delivery, and linked `agent_response` artifact id.

The provider may return a `tool_calls` object requesting a tool advertised in
the prompt. Advertised tools include log search/slice, Metadata, Case Memory,
Skill references, Fetch catalog, configured domain tools when present, and
Fetch execution when Fetch is enabled. `logagent.search_logs` must advertise
the V1-compatible optional `maxMatches` cap, and `logagent.get_log_slice` must
advertise the same center-line or V1-compatible `startLine`/`endLine` range
schema as task MCP. Configured domain tools must use the same `toolId` or
V1-compatible `tool + inputFile` schema exposed by task MCP `tools/list`, and
manual-only tools are not advertised to the provider. Waiting/approval tools
are advertised unless the run is resuming with `resumeMode=finalize`. V2
validates the tool name and arguments as JSON objects, executes the
Server-owned task MCP tool, records the resulting evidence/artifacts through
the existing tool implementation, records the call in `mcp_calls.jsonl`, and
feeds ordinary structured observations into the next provider round. If a provider calls
`logagent.request_user_input` or
`logagent.request_approval`, the tool creates the pending action, writes the
V1-compatible waiting marker, moves the run to `waiting_for_user` or
`waiting_for_approval`, persists the provider response validation as
`paused`, writes the waiting call to `mcp_calls.jsonl`, and stops the current
job without writing `result.json`. The loop is bounded by
`LOGAGENT_V2_AGENT_MAX_ROUNDS` with default 3. Evidence refs returned by
ordinary tool observations, including
`evidenceRefs`, `finalEvidenceRefs`, match `ref`, and `evidenceRef` fields, are
deduplicated into the next provider request and prompt `allowedEvidenceRefs` so
the provider can cite follow-up evidence without violating final-answer
validation.

The provider must eventually return one JSON object matching the final answer
schema. V2 then runs the same normalization and evidence-ref validation used by
the stub. Invalid JSON, unsupported refs, provider HTTP errors, unsupported
tool requests, or max-round exhaustion fail the run. After a user message or
approval decision requeues the run, the next provider request includes recent
messages, action results, remaining pending actions, and `resumePolicy` in
`interactionContext`. When the latest user message has `resumeMode=finalize`,
`resumePolicy.finalizeWithCurrentEvidence=true`, waiting tools are removed from
the advertised tool list, and the provider must return a final answer based on
current evidence.

Each run also writes `analysis_package.json` with schema version 1. It contains
Workspace/run metadata, task MCP resource URIs, manifest and grep outlines,
bounded tool input summaries, system/metadata context outlines, bounded
`analysisState` resume context, allowed current-run evidence refs starting with
`session_text_input.json#question`, and final-evidence policy including
`case_context.json#cases/<index>`. Its resource index includes Agent audit
resources and optional Rust/V1 Claude runtime compatibility resources
`claude_mcp_config` and `claude_session`. It
intentionally omits full Skill content, full Metadata topology, and raw
uploaded text. Task MCP exposes it at
`logagent://task/<run_id>/analysis_package` and retains the
`logagent-v2://run/<run_id>/analysis_package` alias.

The Agent boundary is audited with schema version 1 artifacts. `agent_request`
captures the provider/stub, model, transport metadata, allowed evidence refs,
analysis package artifact id, and request payload without Authorization
headers. `agent_response` captures provider status, HTTP/body previews when
available, parsed final answer, normalized final answer, and validation status
or failure details. `analysis_state` captures the latest round status and links
the request and response artifact ids. `mcp_calls` captures successful task MCP
`resources/read` and `tools/call` requests, including tool calls executed by
the provider loop, as JSONL with call id, arguments, status, result, and
evidence/background refs. These evidence rows are background-only
(`final_allowed=false`) and exposed through task MCP resources.
Each run also writes Rust/V1 Claude runtime contract artifacts:
`claude_prompt.md`, `claude_mcp_config.json`, and `claude_session.json`.
`claude_mcp_config.json` points at the V2 task HTTP MCP endpoint and uses
`${LOGAGENT_V2_API_KEY}` as an Authorization placeholder, so the resolved API
key is never persisted. When `LOGAGENT_V2_AGENT_PROVIDER=claude_code`, the
same prompt/config are materialized into the temporary Claude session
directory and used by the CLI invocation. After a Claude Code provider response
with session metadata, the latest `claude_session` task MCP resource must return
the runtime session artifact instead of the initial `contract_ready` artifact.
Task MCP also exposes aggregate compatibility resources: `artifact_index`
enumerates current run upload and evidence artifacts with stable logical paths,
`tool_results` returns parsed `tool_result` and `fetch_result` artifacts under
the canonical `tool_results/<action_id>/result.json` shape, and `case_context`
returns the latest background-only Case search/recall context or an empty
context when no Case tool has run. `artifact_index` includes the persisted run
question at `session_text_input.json`.

`GET /api/v2/runs/<run_id>/artifacts` preserves the V2 `run`, `uploads`, and
`evidenceArtifacts` lists while adding a Rust/V1 migration aggregate response:
`taskId`, `artifactIndex`, `manifestPath`/`manifest`,
`grepResultsPath`/`grepResults`, `textInputPath`/`textInput`,
`metadataContextPath`/`metadataContext`, `systemContextPath`/`systemContext`,
`caseContextPath`/`caseContext`, `analysisPackagePath`/`analysisPackage`,
`agentResponsePath`/`agentResponse`, `analysisStatePath`/`analysisState`,
`claudeMcpConfigPath`/`claudeMcpConfig`,
`claudeSessionPath`/`claudeSession`, `mcpCallsPath`/`mcpCalls`, and
`toolResults`.

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
Both calls write `mcp_waiting_request.json` and return the V2 `action` plus
Rust/V1 `artifactPath`, `runtimeStatus`, and `evidenceRefs`.
`GET /api/v2/runs/:run_id/analysis` returns `actions` and `pendingActions` so
WebUI can render the same recovery controls as the Rust task detail page.
`POST /api/v2/runs/:run_id/messages` accepts only `waiting_for_user` runs,
returns 409 for other states, and optionally validates `questionId` against a
pending `user_input` action id or payload question id. Repeated submissions
with the same `idempotencyKey` return the original `user.message` timeline
event without re-answering actions or creating another job.
`POST /api/v2/actions/:action_id/decisions` accepts only
`waiting_for_approval` runs with a pending approval action. Repeated approval
submissions with the same `idempotencyKey` return the original decision event
without updating the action or creating another job.
`POST /api/v2/runs/:run_id/messages` and
`POST /api/v2/actions/:action_id/decisions` requeue waiting runs into the
SQLite job queue. User messages also mark pending matching `user_input`
actions as `answered`. The next Agent request includes a bounded
`interactionContext`
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
`logagent://task/<run_id>/environment_evidence`, with the
`logagent-v2://run/<run_id>/environment_evidence` alias retained. A bounded
outline is included in the next `analysis_package` and Agent prompt. The current runtime
still does not implement full LangGraph resume planning, SCP file collection,
or multi-node Environment Collector execution.

## Security

- API key is read from `LOGAGENT_V2_API_KEY`.
- Artifact paths are resolved relative to `data_dir` and rejected if they
  escape it.
- Upload filenames are basename-normalized and character-filtered.
- Archive entries are scanned in memory and rejected if they contain absolute
  paths or traversal components.
- Tools execute only through configured whitelist descriptors. Enabled tool
  commands are environment/user-expanded during settings loading and must
  resolve to absolute paths before registration; execution still uses fixed
  args, timeout, and bounded stdout/stderr.
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
  `server-v2`, initialize SQLite, sync WebUI static files, load
  `$HOME/.cargo/env` for source-built analyzer rebuilds when present, and
  preserve existing `data-v2`.
- `deploy/logagent-v2ctl.sh` can start, stop, restart, report status, and tail
  V2 logs using the same `.env` loading pattern as the Rust deploy controls.
