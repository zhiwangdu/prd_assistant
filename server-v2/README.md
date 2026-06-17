# LogAgent V2 Server

`server-v2` is a clean-room Python implementation branch for the small-team
LogAgent redesign. It does not preserve the Rust Server API surface. The first
slice provides the durable foundation for the V2 product model:

- FastAPI HTTP service.
- Static WebUI hosting from `webui/out` with SPA fallback for non-API routes.
- SQLite WAL storage under one local data directory.
- Local artifact storage for uploads and future evidence files.
- DB-backed job queue for restartable background runs, with startup recovery
  for interrupted analysis and remote command jobs.
- Workspace, Run, TimelineEvent, Evidence, Artifact, Upload, Action, and Job
  schema foundations.
- Workspace update and soft-delete lifecycle; deleted Workspaces are hidden from
  history lists but existing runs and artifacts remain readable by id.
- Single, batch, and restartable chunked upload foundations backed by SQLite
  upload sessions and local temp files.
- Initial evidence pipeline for uploaded text files and supported archives.
- V1-style node log package preprocessing for
  `<packageId>_<instanceId>_<nodeId>_<timestamp>_logs.tar.gz` uploads.
- Materialized `tool_inputs/index.json` generation for node-package tsdb
  InfluxQL query lines, generic file-level InfluxQL/Flux query lines, and
  enabled storage analyzer file or directory inputs such as `.tssp`,
  `.tssp.init`, `.tsm`, `.tsi`, TSI/mergeset trees, and `_series` trees from
  direct uploads or supported archives.
- `manifest.json` and `grep_results.json` artifact generation.
- Read-only MCP endpoint with V1-shaped tool catalog, Metadata, Case Memory,
  Skill registry, and Domain Adapter resources/tools. `resources/list`
  advertises both static collection resources and dynamic per-Skill /
  per-Metadata snapshot resources under `logagent://...` and
  `logagent-v2://...`. Read-only and task MCP handlers accept single JSON-RPC
  requests and JSON-RPC batch arrays; both also support `ping` and empty
  `prompts/list`.
- Task MCP endpoint with summary/evidence/artifact_index/manifest/grep,
  analysis_package, case_context, tool_results, Agent audit resources, and
  `logagent.search_logs` follow-up search plus `logagent.get_log_slice`.
- Tool Plugin registry exposed through `/api/v2/tools`, readonly MCP tool
  catalog, manual tool-run APIs, and task MCP `logagent.run_domain_tool`.
  Configured subprocess tools with `{input_file}` accept explicit
  `inputFile`/`inputFiles` workspace paths, otherwise consume matching
  materialized `tool_inputs` before execution, then fall back to manifest file
  patterns, initial grep keyword matches, or raw upload artifacts for storage
  analyzers. Enabled storage analyzer materialized inputs point to safe artifact
  files or directory bundles extracted from uploads and archives. Generic JSON
  stdout and InfluxQL analyzer report/compare stdout are normalized into
  `summary/findings`. Task MCP responses preserve the V2 nested
  `result/artifact/evidence` payload and also expose Rust/V1-compatible
  top-level `artifactPath`, `summary`, and `evidenceRefs` fields, plus
  finding-level `finalEvidenceRefs` when the tool produced findings.
- V1 built-in tool migration for metadata catalog tools,
  `logagent.preprocess_log_package`, `logagent.fetch`, `pprof_analyzer`, and
  default-off `logagent.huawei_cloud_package_sync`.
- Fetch endpoint foundation with SQLite endpoint storage, HTTP API management,
  DevTools bash cURL import, default-off allowlist execution, task MCP
  `logagent.fetch`, runtime `endpointId`/`fetchId` parameters with URL
  variables, temporary headers and body override, response body artifacts, and
  `fetch_result` final evidence refs.
- Waiting-state action foundation for task MCP `logagent.request_user_input`
  and `logagent.request_approval`, exposed through run analysis summaries for
  WebUI recovery; user supplements answer pending user-input actions and recent
  messages/action decisions are included in the next Agent request context.
  Calls also persist a V1-compatible `mcp_waiting_request.json` background
  artifact and return `artifactPath`, `runtimeStatus`, and `evidenceRefs`;
  `request_approval` accepts the V1 shape with only `reason` and defaults
  missing `actionType` to `manual_approval`.
  Approved `collect_environment` actions can either record V1-compatible mock
  `environment_evidence` background artifacts or, when given a Remote Executor
  `executorId` and whitelisted `commandId`, queue a remote command and record
  the completed command output as background environment evidence before
  resuming the analysis run.
- Final answer schema normalization and evidence ref validation before a run
  can be marked `succeeded`; the run question is persisted as
  `session_text_input.json` and can be cited as
  `session_text_input.json#question`, and recalled Cases can be cited through
  `case_context.json#cases/<index>`.
- Final result persistence as `result.json` and `result.md` artifacts, with
  HTTP and task MCP read surfaces, plus deterministic fallback run alias
  persistence for history/UI display.
- Metadata foundation with JSON/YAML/openGemini content import, allowlisted URL
  fetch, SQLite snapshot storage, saved raw snapshot refresh,
  preview/confirm drafts, field/tag type queries, per-run `metadata_context`
  auto-selection, HTTP API, readonly MCP tools, and task MCP
  `logagent.get_metadata_topology` / `logagent.query_metadata` bounded slices.
- Case Memory foundation with manual cases, succeeded-run case confirmation,
  text/JSON import drafts, follow-up import messages, SQLite FTS5/BM25 plus local vector recall,
  edit/disable API, readonly MCP search, and task MCP
  V1-compatible `logagent.recall_cases`.
- Skill-backed System Context foundation with filesystem Skill registry,
  Markdown import, explicit or auto-matched Workspace skill selection,
  `system_context` run snapshot, readonly/task MCP reference reading, and
  `skills.zip` export.
- Legacy System Context resource compatibility APIs backed by SQLite for
  prompt packs, architecture docs, runbooks, glossaries, tool capability notes,
  knowledge notes, diagnostic-skill records, version activation, and prompt
  preview. Metadata instances are exposed as read-only `metadata_instance`
  adapter resources in the compatibility list/preview surface.
- `tools.zip` export for enabled configured subprocess tools, with packaged
  executables, shell wrappers, examples, and a manifest.
- Agent runtime with default stub final answer plus optional bounded
  OpenAI-compatible or local binary provider/tool loop for evidence-validated
  JSON final answers and per-round request/response/state audit artifacts.
- Settings and diagnostics endpoints for the V2 Agent provider, backend dry-run
  summary, built-in Domain Adapters, and process-local LLM response-content
  debug logging.
- Remote Executor foundation with SQLite-managed executors, environment-driven
  whitelisted SSH command templates, DB-backed remote command jobs, and
  stdout/stderr/result files under the V2 data directory.

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

Public endpoints:

```http
GET /
GET /health
```

## Runtime Deploy

For the runtime-style deploy template, copy or use `deploy/` with a configured
`.env`, then install and control V2 separately from the Rust server:

```bash
cd deploy
./rebuild-v2-install.sh
./logagent-v2ctl.sh start
./logagent-v2ctl.sh status
./logagent-v2ctl.sh stop
```

`rebuild-v2-install.sh` creates `$LOGAGENT_V2_VENV_DIR`, installs `server-v2`,
initializes SQLite, builds and syncs `webui/out`, and restarts V2 only when it
was already running. Runtime defaults are `$LOGAGENT_APP_DIR/server-v2/.venv`,
`$LOGAGENT_APP_DIR/data-v2`, `$LOGAGENT_APP_DIR/webui/out`, and port `50993`.
Use `--with-tools` to also build source-referenced analyzer submodules into
`$LOGAGENT_APP_DIR/bin/tools`, or `--tools-only --only-tool <name>` for a fast
tool-only rebuild. The rebuild script loads `$HOME/.cargo/env` when present so
rustup-managed `cargo` is available for Flux analyzer builds in non-interactive
SSH shells. `logagent-v2ctl.sh start` and `restart` wait for `/health` until
`LOGAGENT_V2_STARTUP_TIMEOUT_SECONDS` expires, and clean stale pid files when
startup fails. The control script is pid-file scoped by default so separate
runtime directories do not control each other's V2 processes.

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
| `LOGAGENT_V2_TOOL_INFLUXQL_ANALYZER` | unset | Default configured InfluxQL analyzer executable |
| `LOGAGENT_V2_TOOL_FLUX_QUERY_ANALYZER` | unset | Default configured Flux analyzer executable |
| `LOGAGENT_V2_TOOL_OPENGEMINI_STORAGE_ANALYZER` | unset | Default configured openGemini storage analyzer executable |
| `LOGAGENT_V2_TOOL_INFLUXDB_STORAGE_ANALYZER` | unset | Default configured InfluxDB storage analyzer executable |
| `LOGAGENT_V2_PPROF_ENABLED` | auto when Go command set | Enable V1-style configured `pprof_analyzer` adapter |
| `LOGAGENT_V2_PPROF_GO_COMMAND` | `LOGAGENT_TOOL_PPROF_GO` or `go` | Go executable for `go tool pprof` |
| `LOGAGENT_V2_FETCH_ENABLED` | `0` | Enable configured Fetch endpoint execution |
| `LOGAGENT_V2_FETCH_ALLOWED_HOSTS` | unset | Comma-separated exact host or host:port allowlist |
| `LOGAGENT_V2_FETCH_TIMEOUT_SECONDS` | `20` | Per-request Fetch timeout |
| `LOGAGENT_V2_FETCH_MAX_REQUEST_BYTES` | `1048576` | Maximum Fetch request body bytes |
| `LOGAGENT_V2_FETCH_MAX_RESPONSE_BYTES` | `1048576` | Maximum stored Fetch response preview bytes |
| `LOGAGENT_V2_FETCH_MAX_REDIRECTS` | `5` | Maximum manually revalidated Fetch redirects |
| `LOGAGENT_V2_FETCH_SECRET_KEY` | unset | Fernet 32-byte base64 key for encrypted Fetch credential sets |
| `LOGAGENT_V2_AGENT_PROVIDER` | `stub` | `stub`, `openai_compatible`, or `binary` final-answer provider |
| `LOGAGENT_V2_AGENT_BASE_URL` | unset | OpenAI-compatible base URL, e.g. `https://api.openai.com/v1` |
| `LOGAGENT_V2_AGENT_MODEL` | unset | Model name for the OpenAI-compatible provider |
| `LOGAGENT_V2_AGENT_API_KEY` | unset | Bearer token for the OpenAI-compatible provider |
| `LOGAGENT_V2_AGENT_BINARY_PATH` | unset | Absolute executable path for the local binary Agent provider |
| `LOGAGENT_V2_AGENT_BINARY_MAX_OUTPUT_BYTES` | `1048576` | Maximum stdout bytes accepted from the binary Agent provider |
| `LOGAGENT_V2_AGENT_TIMEOUT_SECONDS` | `60` | Provider request timeout |
| `LOGAGENT_V2_AGENT_MAX_ROUNDS` | `3` | Maximum provider/tool-loop rounds per run |
| `LOGAGENT_V2_AGENT_MAX_OUTPUT_TOKENS` | `2048` | Maximum provider output tokens for V2 Agent calls |
| `LOGAGENT_V2_REMOTE_EXECUTION_ENABLED` | `1` | Enable V2 Remote Executor APIs and jobs |
| `LOGAGENT_V2_REMOTE_SSH_COMMAND` | `ssh` | SSH executable used by Remote Executor jobs |
| `LOGAGENT_V2_REMOTE_CONNECT_TIMEOUT_SECONDS` | `10` | SSH connect timeout option |
| `LOGAGENT_V2_REMOTE_COMMAND_TIMEOUT_SECONDS` | `30` | Default remote command timeout |
| `LOGAGENT_V2_REMOTE_MAX_OUTPUT_BYTES` | `1048576` | Maximum stored stdout/stderr bytes per stream |
| `LOGAGENT_V2_REMOTE_HOST_KEY_POLICY` | `accept-new` | `strict`, `accept-new`, or `off` host-key behavior |
| `LOGAGENT_V2_REMOTE_COMMANDS_JSON` | default smoke | JSON array of whitelisted remote command templates |
| `LOGAGENT_V2_WEBUI_DIR` | repo `webui/out` | Static WebUI build directory served by `GET /` |
| `LOGAGENT_V2_HUAWEI_PACKAGE_SYNC_ENABLED` | `0` | Enable Huawei OBS + GaussDB package sync |
| `LOGAGENT_V2_HUAWEI_OBS_ENDPOINT` | unset | Huawei OBS endpoint |
| `LOGAGENT_V2_HUAWEI_OBS_BUCKET` | unset | Huawei OBS bucket |
| `LOGAGENT_V2_HUAWEI_OBS_OBJECT_PREFIX` | unset | Default object key prefix |
| `LOGAGENT_V2_HUAWEI_OBS_ACCESS_KEY` | unset | Huawei OBS access key |
| `LOGAGENT_V2_HUAWEI_OBS_SECRET_KEY` | unset | Huawei OBS secret key |
| `LOGAGENT_V2_HUAWEI_GAUSSDB_DSN` | unset | GaussDB/PostgreSQL DSN for package sync |

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

## System Context Resources

V2's primary run-time System Context remains Skill-backed: Workspace
`skillIds`, auto-matched Skills, and Metadata context are materialized into the
per-run `system_context` snapshot. For V1 management parity, V2 also exposes
legacy System Context resource APIs under `/api/v2/system-context/*`.

The compatibility store persists resource records and versions in SQLite,
supports draft/active/archived version states, and renders a prompt preview for
selected or default-included resources. User-created compatibility resources are
management/preview inputs only; new analysis runs continue to use the
Skill-backed System Context path unless later product work explicitly maps
those resources into Skills. Metadata instances appear as read-only
`meta_<instanceId>` adapter resources and can be included in preview requests.

## Settings And Diagnostics

V2 exposes Settings diagnostics under `/api/v2/settings/*`. The LLM section is
mapped to the V2 Agent provider configuration: `stub` is local,
`openai_compatible` calls the configured OpenAI-compatible `/models` and
`/chat/completions` endpoints, and `binary` validates the configured executable
then runs the same final-answer parse path through a local process. Responses
never include API keys or the configured binary path.

`/api/v2/settings/agent-backends` describes the in-process V2 Agent runtime
instead of the Rust Server's Claude Code CLI backend. The diagnostic endpoint is
a dry-run configuration check; for `binary` it checks that
`LOGAGENT_V2_AGENT_BINARY_PATH` is absolute, regular, and executable. The Domain
Adapter endpoint returns the built-in `opengemini_influxdb` active adapter plus
`cassandra` and `rocksdb` skeleton adapters. The same adapter summaries are also
available through readonly MCP `logagent-v2://domain-adapters` and
`logagent.list_domain_adapters`.

`/api/v2/debug/llm` toggles process-local response-content logging for provider
debugging. It only logs model response content to stderr and does not log
prompts, headers, or API keys. The setting resets when the process restarts.

## Remote Executors

V2 Remote Executor APIs live under `/api/v2/executors`,
`/api/v2/executor-command-templates`, and `/api/v2/executor-runs`. Executors are
stored in SQLite with host, port, SSH user, tags, notes, enabled state, and
timestamps. Deleting an executor disables it instead of removing historical run
records.

Command templates are loaded from `LOGAGENT_V2_REMOTE_COMMANDS_JSON`; if unset,
V2 exposes the low-risk `smoke_ls_root` template. Template descriptors match
the Rust/V1 behavior: `enabled` also reflects global remote execution state,
and `timeoutSeconds` is always filled with the template override or default
remote command timeout. Runs are DB-backed jobs. The worker invokes the
configured SSH executable with fixed argv:

```text
ssh -o BatchMode=yes -o ConnectTimeout=<seconds> -o StrictHostKeyChecking=<policy> -p <port> <user>@<host> <template argv...>
```

The API never accepts free-form shell commands. Results are written under:

```text
data_dir/
  remote_runs/
    <run_id>/
      remote_command/
        result.json
        stdout.txt
        stderr.txt
```

Non-zero exit, timeout, and start failures are recorded in `result.json`; the
remote run itself reaches `SUCCEEDED` when the controlled execution completed
and result files were persisted. System errors before result persistence mark
the run `FAILED`.

## Verification

```bash
python3 -m compileall logagent_v2
PYTHONPATH=. python3 -m unittest discover tests
```

This V2 slice migrates V1 configured analyzer execution, metadata/preprocess/
fetch/pprof/Huawei built-ins, storage analyzer materialized inputs, and raw
upload fallback. Full LangGraph planning and full WebUI cutover remain separate
product steps.

## Job Recovery

On startup, V2 scans DB-backed jobs left in `running` state by a prior process.
Interrupted non-terminal `run_analysis` jobs reset their Run to `queued`, append
`run.recovered`, and become immediately acquirable. Interrupted remote command
jobs reset their remote run to `QUEUED`. If the associated Run or remote run is
already terminal or waiting for user/approval, the stale job is marked
`succeeded` instead of rerunning.

## Agent Runtime

By default V2 uses `LOGAGENT_V2_AGENT_PROVIDER=stub`, which produces the
deterministic low-confidence evidence summary used by the foundation tests.
`LOGAGENT_V2_AGENT_PROVIDER=openai_compatible` sends a compact prompt with the
Workspace question, manifest counts, initial grep preview, allowed evidence
refs, recent user messages/action results from resumed runs, available
Server-owned tools, and prior tool observations to
`<LOGAGENT_V2_AGENT_BASE_URL>/chat/completions`. `LOGAGENT_V2_AGENT_PROVIDER=binary`
uses `LOGAGENT_V2_AGENT_BINARY_PATH` and invokes the executable without a shell
as fixed argv:

```text
<binary_path> run <prompt>
```

The binary provider stdout must be UTF-8 JSON containing the same final-answer
object accepted from OpenAI-compatible content. Non-zero exit, timeout,
oversized stdout, invalid UTF-8, and parse/schema failures are persisted in
`agent_response.json`.

The provider may return a `tool_calls` object for tools advertised in the
prompt: log search/slice, Metadata, Case Memory, Skill references, Fetch
catalog, configured domain tools, and Fetch execution when Fetch is enabled. V2
validates the requested tool name against the advertised set, executes through
the existing task MCP call path, feeds the observations into the next round, and
stops after `LOGAGENT_V2_AGENT_MAX_ROUNDS`. The provider must eventually return
one JSON final-answer object; V2 normalizes it and rejects unsupported or
non-current evidence refs before marking the run `succeeded`.

This is a bounded provider-directed tool loop. Waiting/approval tools are not
advertised to the provider. Full LangGraph planning remains future work, but
resumed runs include a bounded `interactionContext` with recent user messages,
answered/approved/rejected actions, pending actions, and a
finalize-with-current-evidence directive when the user requests it.

Every run writes `analysis_package.json` after initial evidence collection. The
package is a bounded Agent context bundle: Workspace/run metadata, task MCP
resource URIs, manifest outline, grep match preview, analyzer tool input
outline, system/metadata context outlines, and the current allowed evidence
refs, including `session_text_input.json#question`. Task MCP exposes it as
`logagent://task/<run_id>/analysis_package` and retains the
`logagent-v2://run/<run_id>/analysis_package` alias.

Every Agent round also writes background-only audit artifacts:
`agent_request.json`, `agent_response.json`, `analysis_state.json`, and
`mcp_calls.jsonl`. The request artifact stores the provider/stub payload
without Authorization headers, the response artifact stores provider output or
structured failure details plus final-answer validation status, and the state
artifact records the latest round status. Successful task MCP `resources/read`
and `tools/call` requests append JSONL records with call id, arguments, status,
result summary, and evidence/background refs. Task MCP exposes them as
`agent_request`, `agent_response`, `analysis_state`, and `mcp_calls` resources.
Task MCP also exposes V1-compatible aggregate resources: `artifact_index`
lists current run uploads and evidence artifacts by stable logical path,
`tool_results` aggregates `tool_result` and `fetch_result` artifacts, and
`case_context` returns the latest background Case recall/search context. The
artifact index includes the persisted run question at `session_text_input.json`.

Successful runs also write `result.json` and `result.md`, then persist a short
deterministic alias derived from the final summary or question. `GET
/api/v2/runs/<run_id>/result` returns the stored final answer plus artifact and
evidence metadata, while task MCP exposes `result` and `result_markdown`. The
alias is stored on the Run record for history/UI display; it is not model
evidence and does not affect final-answer validation.

## Uploads

V2 supports three upload paths:

- `POST /api/v2/workspaces/<workspace_id>/uploads` for one multipart file.
- `POST /api/v2/workspaces/<workspace_id>/uploads/batch` for multiple
  multipart files under one Workspace.
- `POST /api/v2/workspaces/<workspace_id>/uploads/init`, followed by
  `POST /api/v2/uploads/<session_id>/chunks?offset=<bytes>` and
  `POST /api/v2/uploads/<session_id>/complete`, for restartable chunked upload.

Chunked uploads persist session state in SQLite and temporary bytes under
`data_dir/tmp/upload_sessions`. Completion validates received size, converts the
temp file into a regular artifact, creates an Upload row, and marks the session
completed.

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
- `resources/read` for `summary`, `artifact_index`, `evidence`, `manifest`,
  `grep_results`, `system_context`, `metadata_context`, `analysis_package`,
  `analysis_state`, `agent_request`, `agent_response`, `case_context`,
  `tool_results`, `mcp_calls`, `result`, and `result_markdown`
- `tools/list`
- `tools/call logagent.search_logs` with V1-compatible optional `maxMatches`
  clamped to 1..200; responses keep the nested V2 `search` object and expose
  Rust-compatible top-level `artifactPath`, `totalMatches`, `keywordCounts`,
  `unmatchedKeywords`, `matches`, `evidenceRefs`, and `note`
- `tools/call logagent.get_log_slice` with either `lineNumber` plus
  `before`/`after`, or V1-compatible `startLine`/`endLine`; responses keep the
  nested V2 `slice` object and expose Rust-compatible top-level `artifactPath`,
  `evidenceRefs`, and `lines`
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

Configured tools can be invoked by V2 `toolId` or by the V1-compatible
`tool` field; the model cannot provide an executable path, shell command, or
argv. Task MCP `logagent.run_domain_tool` only exposes configured subprocess
tools, and its `tools/list` input schema advertises both V2 `toolId` and
Rust/V1 `tool + inputFile` forms with `anyOf`. Built-ins are available through their dedicated task MCP tools or the
protected manual Tools API according to each descriptor's `runnable` policy.
Tool stdout is parsed as JSON when possible and persisted as `tool_result`
evidence. Generic JSON output can use
`summary`, `message`, or `title`, plus `findings`, `issues`, or `diagnostics`.
InfluxQL analyzer report JSON is adapted into a compact summary and findings
for special rules, parse errors, realtime classification, fingerprints, compare
fingerprint deltas, and rule deltas.

Configured tools may declare `paramsSchema`. Task MCP `logagent.run_domain_tool`
then accepts `params`, validates a conservative object-schema subset
(`required`, `additionalProperties`, primitive `type`, and `enum`), and replaces
`{params.name}` placeholders in configured argv. Commands and argv templates
still come only from Server configuration. For tools with `{input_file}`, V2
adds a reserved `params.inputFiles` array to the descriptor; task MCP also
accepts V1-style top-level `inputFile` and maps it to that same selector.

If a configured tool argument contains `{input_file}`, explicit selectors are
resolved first. They can reference current Workspace text paths, their
`extracted/...` virtual paths, or `tool_inputs/...` entries from the current
run's latest `tool_input_index`. Without explicit selectors, V2 reads that
latest `tool_input_index` evidence and selects entries whose `toolIds` include
the requested tool. The placeholder is replaced with the local artifact path
for that input. Each selected input gets a stable action id derived from the
tool id and virtual input path, so final refs use:

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

Manual tool runs use:

```http
POST /api/v2/tools/:tool_id/runs
```

with `workspaceId`, optional `uploadIds`, and `params`. They create
`kind=tool_run` Run rows and DB-backed `tool_run` jobs, so startup recovery and
artifact/evidence tracking use the same SQLite foundation as analysis runs. V2
configured subprocess descriptors use the Rust/V1 command shape:
`source=configured`, `backend=command`, `readOnly=false`, `editable=true`,
`exportable=enabled`, `minFiles=1`, and `acceptedSuffixes` copied from
`match.filePatterns`. V2 currently includes manual built-ins for metadata tools,
`logagent.preprocess_log_package`, `logagent.fetch`, and default-off
`logagent.huawei_cloud_package_sync`, plus the V1-style configured command
adapter `pprof_analyzer`.
Huawei package sync matches the Rust/V1 catalog behavior by accepting any
single completed upload (`acceptedSuffixes=["*"]`) and validating only the
single-upload count plus structured SQL/object-key params.
`pprof_analyzer` catalog metadata uses the Rust/V1 configured command shape
(`source=configured`, `backend=command`) while remaining manual-only in V2.
Its result JSON includes parsed `profileType`, `total`, top rows, V2 artifact
id mappings, and Rust/V1-style `artifactPaths` for
`tool_results/<action_id>/{top.txt,tree.txt,raw.txt,stderr.txt,graph.svg}`.

Readonly MCP exposes the same tool registry as `logagent://tools/catalog`,
the retained `logagent-v2://tools/catalog` alias, and `logagent.list_tools`.
All return
`schemaVersion`, full `tools` descriptors, and a V1-compatible
`configuredTools` summary with configured args, timeout, match rules, and
`maxInputFiles`. The readonly endpoint never runs tools.

`GET /api/v2/exports/tools.zip` exports enabled configured subprocess tools
and the enabled `pprof_analyzer` Go executable.
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
Runtime calls accept either `endpointId` or the V1-compatible `fetchId`,
optional string `variables` that replace `{name}` placeholders in the endpoint
URL before allowlist validation, optional temporary string `headers`, and an
optional string `body` override. Controlled headers such as `Host` and
`Content-Length` are rejected for both saved endpoints and runtime overrides.
Task MCP `logagent.list_fetch_endpoints` matches the Rust/V1 envelope with
`schemaVersion=1`, enabled endpoint summaries, and
`finalEvidenceAllowed=false`; when Fetch execution is disabled it returns a
JSON-RPC error instead of listing endpoints.
`GET /api/v2/fetch/runs` lists persisted Fetch tool runs, filtered by
`endpointId`, `fetchId`, V1-style `fetch_id`, or `workspaceId`, without
executing network requests. `POST /api/v2/fetch/endpoints/:endpoint_id/runs`
queues a Fetch `tool_run`; callers may provide `workspaceId`, otherwise V2
creates an isolated workspace for the run.
Saved endpoint bodies and runtime body overrides are rejected before the HTTP
request when their UTF-8 byte size exceeds
`LOGAGENT_V2_FETCH_MAX_REQUEST_BYTES`.
Request URLs, sensitive headers, and sensitive JSON/form-style body preview
fields are redacted in API, MCP, and artifact previews. Redirects are followed
manually up to
`LOGAGENT_V2_FETCH_MAX_REDIRECTS`; every hop is revalidated against the same
allowlist, and sensitive headers are stripped when redirecting across origin.
Fetch stores bounded response previews as `fetch_result` evidence and stores
the bounded raw response body as a separate body artifact. Results include the
logical V1-style `tool_results/<action_id>/response_body.bin` path plus the
actual V2 artifact id and relative path.

Sensitive Fetch endpoint material is split into an encrypted credential set.
If a URL query parameter, header, or body field looks like a token, secret,
password, API key, session, Authorization, or Cookie, V2 stores only a redacted
endpoint definition in `fetch_endpoints` and encrypts the full request material
in `fetch_credential_sets` using `LOGAGENT_V2_FETCH_SECRET_KEY`. Creating or
updating a sensitive endpoint without a valid key is rejected before the
endpoint row is written. Execution hydrates the endpoint from the credential
set, while API, MCP, and result artifacts continue to show only redacted values.

`request_user_input` and `request_approval` persist pending `actions`, write
`mcp_waiting_request.json`, and move the run into `waiting_for_user` or
`waiting_for_approval`. The task MCP response includes the V2 `action` plus
Rust/V1 `artifactPath`, `runtimeStatus`, and `evidenceRefs`. Posting a message
to a waiting run marks pending user-input actions as `answered` and requeues
the run through the SQLite job queue. Approving/rejecting a pending action
records the decision and requeues approval-waiting runs. The next Agent request
carries recent user messages, action results, and remaining pending actions in
`interactionContext`. When an approved action payload has
`actionType=collect_environment`, V2 checks `input.executorId` and
`input.commandId`. If both target an enabled Remote Executor and whitelisted
command template, V2 queues a `remote_command_run`, keeps the analysis run
waiting while collection runs, then writes
`environment_evidence/<action_id>/result.json` with the remote status,
stdout/stderr previews, and result paths before requeueing the analysis run.
Invalid remote targets produce `REMOTE_REJECTED` background evidence instead of
leaving the approved action half-applied. If no remote target is supplied, V2
preserves the V1-compatible MOCK evidence path. Environment evidence is exposed
through `/analysis` resources and task MCP `environment_evidence`, included in
the next `analysis_package` and Agent prompt, and remains background-only
rather than a final evidence ref.

## Final Answers

Before V2 stores a `succeeded` run, final answers are normalized and validated.
The current required shape is:

- `summary`: non-empty string
- `symptoms`, `nextChecks`, `fixSuggestions`, `missingInformation`: string
  arrays
- `likelyRootCauses`: objects with non-empty `cause` and `evidenceRefs`
- `confidence`: `low`, `medium`, or `high`
- `evidenceRefs`: optional top-level string array

Only current-task evidence refs are accepted. Most refs must come from
`final_allowed=true` evidence; `case_context.json#cases/<index>` is the
canonical exception for recalled Case background context:

```text
session_text_input.json#question
grep_results.json#matches/<index>
log_searches/<search_id>.json#matches/<index>
log_slices/<slice_id>.json#lines
case_context.json#cases/<index>
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

`POST /api/v2/metadata/instances/<instance_id>/refresh` rebuilds the normalized
snapshot from the raw JSON already saved with the instance and upserts the
instance row again. It is useful after normalizer changes and does not perform
network fetches.
`GET /api/v2/metadata/clusters/<cluster_id>` and `/nodes` derive cluster detail
and node lists from persisted snapshots. `POST /api/v2/metadata/snapshots/fetch`
fetches and normalizes a remote snapshot without creating an import draft or
persisting an instance.

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

Task MCP field/tag queries persist Rust/V1-compatible background slices under
`metadata_slices/field_types_<stable_id>.json` and
`metadata_slices/tag_fields_<stable_id>.json`. Their responses keep the V2
top-level `fields` shape and also expose the Rust/V1 `result` wrapper plus
`artifactPath`, `backgroundRef`, `evidenceRefs`, and `finalEvidenceAllowed`.
Readonly MCP uses the same `result` wrapper without writing a task artifact.
The field filter schema uses the Rust/V1-compatible `oneOf` form: either one
string or a non-empty string array.

Task MCP also exposes V1-compatible run-scoped Metadata tools:

```text
logagent.get_metadata_topology
logagent.query_metadata
```

`logagent.get_metadata_topology` returns the current run Metadata outline with
section counts and query hints. `logagent.query_metadata` reads bounded
`overview`, `nodes`, `databases`, `retention_policies`, `measurements`,
`fields`, `shard_groups`, `shards`, `index_groups`, `indexes`, and
`partition_views` slices from the selected run Metadata snapshots using
section-specific filters plus `limit`/`cursor`. Task MCP Metadata calls persist
`metadata_slice` evidence as background context with `final_allowed=false`;
final answers cannot cite these slices as root-cause evidence.

When a run starts, V2 also writes `metadata_context.json` as background
evidence. If exactly one metadata instance exists, it is selected as
`default_single`; with multiple instances, V2 scores instance id, remark,
cluster, node, database, retention policy, measurement, and field names against
the Workspace question/mode and includes up to three matched outlines. The
outline is bounded to node/database/schema summaries and is exposed through task
MCP resource `logagent://task/<run_id>/metadata_context`; the
`logagent-v2://run/<run_id>/metadata_context` alias is retained. Full snapshots
and field details remain available through the Metadata MCP tools.

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
environment, instance/node, and evidence refs, plus a dependency-light local
hash-vector recall column stored in SQLite. Disabled cases are hidden unless
the caller sets `includeDisabled=true`.

Case import drafts support text or JSON capture before a Case is confirmed:

```text
POST /api/v2/cases/imports/preview
GET  /api/v2/cases/imports
GET  /api/v2/cases/imports/:import_id
POST /api/v2/cases/imports/:import_id/messages
PATCH /api/v2/cases/imports/:import_id
POST /api/v2/cases/imports/:import_id/confirm
```

Preview parses JSON Case fields or plain text sections such as `Title`,
`Symptom`, `Root Cause`, `Solution`, `Product`, `Instance ID`, and
`Evidence Refs`. Missing required fields are returned as `validationErrors`.
Follow-up messages are appended to the draft, persisted in SQLite, and combined
with the original source text for another parse pass. Patch updates an
unconfirmed draft with manual field corrections and recomputes
`validationErrors` without writing to `cases`. Confirm may provide overrides to
complete or edit the draft; only confirm writes to `cases` and updates the FTS
index, and it remains blocked until required fields are complete.

Case MCP tools:

```text
logagent.recall_cases
logagent.search_cases
logagent.get_case
```

Readonly MCP exposes `logagent.search_cases` and `logagent.get_case`.
`logagent.recall_cases` is task-MCP-only and keeps the Rust V1 name for enabled
Case recall. Task MCP Case calls persist `case_context` evidence as background
context with `final_allowed=false`. Historical cases are references for
investigation and do not replace current-task evidence.

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

Readonly `logagent.preview_system_context` accepts `skillIds`, `product`,
`version`, `environment`, and `instanceId`. It returns the combined `resources`
plus separated `skillResources`, `systemResources`, and a `prompt` preview; it
does not create a run or write `system_context.json`.

Task MCP `logagent.get_skill_reference` only reads references declared in the
run's `system_context` snapshot and persists a `skill_reference` background
artifact with `final_allowed=false`. Its response follows the Rust/V1 background
artifact shape with stable `artifactPath`, `backgroundRef`, `canonicalRef`,
`evidenceRefs`, `skillRevision`, reference metadata, `truncated`, and
`finalEvidenceAllowed=false`. Readonly MCP reads the current registry and does
not write workspace artifacts.

`GET /api/v2/exports/skills.zip` exports the current Skill registry as a zip
snapshot. It includes regular files under each Skill directory, preserves
relative paths, writes a root `manifest.json`, and skips symlinks so exports
cannot include files outside the registry.
