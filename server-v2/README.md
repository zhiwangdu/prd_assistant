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
- Store-level terminal-state protection for analysis, tool, and remote runs:
  once a run reaches succeeded/failed, later worker retries or manual state
  writes cannot overwrite the terminal result.
- Workspace, Run, TimelineEvent, Evidence, Artifact, Upload, Action, and Job
  schema foundations.
- Workspace update and soft-delete lifecycle; deleted Workspaces are hidden from
  history lists but existing runs and artifacts remain readable by id.
- Session-first HTTP aliases under `/api/v2/sessions`: V2 maps `sessionId`
  to the Workspace id, maps `taskIds` to Run ids, persists Rust-style Session
  fields (`title`, `sourceUrl`, `instanceId`, `nodeId`, `systemContextIds`,
  `skillIds`, `analysisMode`, and language), exposes Session uploads,
  restartable upload sessions, JSON upload attachment, pre-run upload detach,
  task creation/listing, and workspace-level timeline events, maps queued tasks
  to Session `ready`, and rejects Session deletion while any task is
  unfinished. Session task APIs return Rust-style TaskSummary fields including
  the persisted `analysisMode` while retaining raw V2 Run records under `runs`
  for diagnostics. Session and task `systemContextIds` / `skillIds` are
  trimmed, empty entries are ignored, duplicates are collapsed, and invalid ids
  return HTTP 400 before being persisted.
- Task-scoped HTTP aliases under `/api/v2/tasks`: `POST /api/v2/tasks`
  accepts the Rust/V1-style `sessionId`, `uploadId` / `uploadIds`, `question`,
  `sourceUrl`, metadata, mode/language, context, and skill fields, validates
  uploads within the target Session, resolves `clusterId` to a V2 Metadata
  instance when needed, updates the Session snapshot, and creates a Run.
  `taskId` maps to the underlying Run id for
  list/read/timeline/evidence/artifacts/analysis/result and user-message
  resume; list/read responses expose TaskSummary-compatible top-level fields
  and retain the raw V2 Run under `run` for diagnostics. Rust/V1-style
  task result/artifact aliases reject non-analysis tool runs; tool-run
  artifacts and results remain available through `/api/v2/tools/runs/...`.
  Rust/V1-style approval decisions use
  `/api/v2/tasks/:task_id/actions/:action_id/decision`.
- Native Agent import compatibility through `native_agent.server_api: "v2"`:
  Chrome Extension still calls local `/imports`; Native Agent creates or reuses
  a V2 `ws_...` Session and uploads through Session-scoped V2 APIs.
- Single, batch, and restartable chunked upload foundations backed by SQLite
  upload sessions and local temp files.
- Initial evidence pipeline for uploaded text files and supported archives.
- V1-style node log package preprocessing for
  `<packageId>_<instanceId>_<nodeId>_<yyyy_MM_dd_HH_mm_ss_micros>_logs.tar.gz`
  uploads.
- Materialized `tool_inputs/index.json` generation for node-package generic
  `log_text` JSONL, node-package tsdb InfluxQL query lines, generic file-level
  InfluxQL/Flux query lines, and enabled storage analyzer file or directory
  inputs such as `.tssp`,
  `.tssp.init`, `.tsm`, `.tsi`, TSI/mergeset trees, and `_series` trees from
  direct uploads or supported archives.
- `manifest.json` and `grep_results.json` artifact generation; manifests keep
  existing V2 upload/file fields and also expose Rust/V1-compatible
  upload/package summaries, `size` / `uploadId` aliases, node package
  metadata, log group counts, gzip compression counts, ignored path samples,
  and file-level compression metadata.
- Read-only MCP endpoint with V1-shaped tool catalog, Metadata, Case Memory,
  Skill registry, and Domain Adapter resources/tools. `resources/list`
  advertises both static collection resources and dynamic per-Skill /
  per-Metadata snapshot resources under `logagent://...` and
  `logagent-v2://...`. Collection `resources/read` responses for Metadata
  instances, recent Cases, Skills, and Domain Adapters use the Rust/V1-style
  `schemaVersion=1` envelope. Read-only and task MCP handlers accept single JSON-RPC
  requests and JSON-RPC batch arrays; both also support `ping` and empty
  `prompts/list`.
- Task MCP endpoint with summary/evidence/artifact_index/manifest/grep,
  analysis_package, case_context, tool_results, Agent audit resources, and
  `logagent.search_logs` follow-up search plus `logagent.get_log_slice`; log
  slice artifacts expose V1-style `sourcePath` and `lines[].line` aliases.
- Tool Plugin registry exposed through `/api/v2/tools`, readonly MCP tool
  catalog, manual tool-run APIs, and task MCP `logagent.run_domain_tool`.
  Configured subprocess tools with `{input_file}` accept explicit
  `inputFile`/`inputFiles` workspace paths, otherwise consume matching
  materialized `tool_inputs` before execution, then fall back to manifest file
  patterns, initial grep keyword matches, or raw upload artifacts for storage
  analyzers. Enabled storage analyzer materialized inputs point to safe artifact
  files or directory bundles extracted from uploads and archives. Generic JSON,
  Flux analyzer metrics/topQueries/parseErrors, and InfluxQL analyzer
  report/compare stdout are normalized into `summary/findings`; InfluxQL
  CompareReport `removed_fingerprints` / `changed_fingerprints` values of
  `null` are treated as empty arrays so new fingerprints and rule deltas still
  produce evidence findings. Task MCP
  responses preserve the V2 nested
  `result/artifact/evidence` payload and also expose Rust/V1-compatible
  top-level `artifactPath`, `summary`, and `evidenceRefs` fields, plus
  finding-level `finalEvidenceRefs` when the tool produced findings. During
  result normalization, `findings[].file` values that point at the current
  input artifact are mapped back to stable workspace-relative `inputFile`
  logical paths, while raw stdout/stderr remain available as support artifacts.
  During analysis, V2 now runs matching input-based configured subprocess tools after
  initial manifest/grep evidence and before the first Agent provider request;
  these pre-run tool summaries are included in `analysis_package.json`,
  `agent_request.json`, and the prompt as `preRunToolResults`. Standalone
  Manual `tool_run` executions for configured tools now write a Rust/V1-style
  aggregate result artifact when multiple inputs are selected, preserving
  `inputFiles`, per-input `results[]`, and each single-input logical
  `artifactPath` while keeping per-input evidence artifacts available.
- V1 built-in tool migration for metadata catalog tools,
  `logagent.preprocess_log_package`, `logagent.fetch`, `pprof_analyzer`, and
  default-off `logagent.huawei_cloud_package_sync`; metadata field filters use
  the Rust/V1 trim-and-reject-empty-array-entry semantics, and tag-field tools
  reject the `field` parameter.
- Fetch endpoint foundation with SQLite endpoint storage, HTTP API management,
  DevTools bash cURL import, default-off allowlist execution, task MCP
  `logagent.fetch`, runtime `endpointId`/`fetchId` parameters with URL
  variables, temporary headers and body override, response body artifacts, and
  `fetch_result` final evidence refs.
- Waiting-state action foundation for task MCP `logagent.request_user_input`
  and `logagent.request_approval`, exposed through run analysis summaries for
  WebUI recovery; user supplements answer pending user-input actions and recent
  messages/action decisions are included in the next Agent request context.
  User message submission requires `waiting_for_user`, supports optional
  `questionId` validation, and de-duplicates retries with `idempotencyKey`.
  Approval decisions require `waiting_for_approval`, target a pending approval
  action, may carry an approved `input` override, and also de-duplicate retries
  with `idempotencyKey`.
  Calls also persist a V1-compatible `mcp_waiting_request.json` background
  artifact and return `artifactPath`, `runtimeStatus`, and `evidenceRefs`;
  `request_approval` accepts the V1 shape with only `reason` and defaults
  missing `actionType` to `manual_approval`.
  Approved `collect_environment` actions can either record V1-compatible mock
  `environment_evidence` background artifacts or queue Remote Executor
  collection targets before resuming the analysis run. The legacy single-target
  shape accepts `executorId` plus exactly one whitelisted `commandId` or
  `fileId`; if exactly one enabled executor exists, the Agent may omit
  `executorId` and provide only `commandId` or `fileId`. The approval payload
  may also carry target fields at the top level or inside `environmentInput` /
  `remoteInput` for provider-normalized actions. In multi-executor setups,
  V2 can resolve approved `target` / `executor` / `node` / `host` hints and
  `template` / `command` / `file` hints to exactly one enabled executor and
  one enabled command/file template; no match or ambiguous matches are rejected
  as `REMOTE_REJECTED` without starting SSH/SCP. The batch shape accepts
  `targets[]`, each with an executor or unique executor hint and one
  command/file template or unique template hint. Batch
  collection waits for every remote run to finish before writing one aggregate
  evidence artifact with `COLLECTED`,
  `PARTIALLY_COLLECTED`, or `REMOTE_FAILED`. Completed remote `result`,
  `stdout`, `stderr`, and collected file support artifacts are copied into the
  analysis workspace artifact registry and linked from the environment evidence
  payload.
- Final answer schema normalization and evidence ref validation before a run
  can be marked `succeeded`; the run question is persisted as
  `session_text_input.json` and can be cited as
  `session_text_input.json#question`, recalled Cases can be cited through
  `case_context.json#cases/<index>`, and V1 legacy grep aliases are normalized
  to canonical `grep_results.json#matches/<index>` refs before validation.
- Final result persistence as `result.json` and `result.md` artifacts, with
  HTTP and task MCP read surfaces, plus provider-backed run alias generation
  with deterministic fallback for history/UI display.
- Run artifact HTTP aggregation: `GET /api/v2/runs/<run_id>/artifacts`
  preserves raw V2 run/upload/evidence lists and also returns Rust/V1-style
  aggregate fields for manifest, grep results, Session text input,
  metadata/system/case context, analysis package, Agent audit artifacts, MCP
  calls, optional Claude MCP config/session artifacts, and tool results.
  Support files that are not themselves evidence, such as configured subprocess
  stdout/stderr, Fetch response bodies, pprof top/tree/raw/SVG output files,
  tool-run result artifacts not already listed as evidence, and remote
  environment command result/stdout/stderr files, are returned under
  `supportArtifacts` and are also included in the task MCP `artifact_index`
  with `source="support"`.
- Metadata foundation with JSON/YAML/CSV/openGemini content import, allowlisted
  URL fetch, SQLite snapshot storage, saved raw snapshot refresh,
  preview/confirm drafts, field/tag type queries, per-run `metadata_context`
  auto-selection, explicit Session `instanceId` / `nodeId` binding, HTTP API,
  readonly MCP tools, and task MCP `logagent.get_metadata_topology` /
  `logagent.query_metadata` bounded slices. Field type labels follow the
  Rust/V1 openGemini mapping and preserve unknown extension codes as
  `Type <code>`.
- Case Memory foundation with manual cases, succeeded-run case confirmation,
  text/JSON import drafts, follow-up import messages, SQLite FTS5/BM25 plus local vector recall,
  edit/disable API, legacy JSON import/writeback, readonly MCP search, and task MCP
  V1-compatible `logagent.recall_cases`.
- Skill-backed System Context foundation with filesystem Skill registry,
  Markdown import, explicit or auto-matched Workspace skill selection,
  explicit Session `systemContextIds` materialized from legacy System Context
  resources, `system_context` run snapshot, readonly/task MCP reference
  reading, preview ID normalization, and `skills.zip` export.
- Code Evidence MVP for configured local git repositories: `logagent.search_code`
  resolves administrator-defined repo/ref/search roots to a commit, creates or
  reuses a detached worktree cache, prunes least-recently-used cached
  worktrees per product, runs read-only `git grep` inside that worktree, writes
  `code_evidence/<action_id>.json`, and exposes final-answer refs as
  `code_evidence/<action_id>.json#matches/<index>`. `logagent.diff_code`
  compares configured base/target versions or refs with read-only
  `git diff --numstat`, writes file-level changed-file evidence, and exposes
  refs as `code_evidence/<action_id>.json#diffs/<index>`. Runs bound to a
  Metadata instance inherit and enforce that instance's product/version before
  resolving refs, while diff base may point at another configured version/ref.
- Legacy System Context resource compatibility APIs backed by SQLite for
  prompt packs, architecture docs, runbooks, glossaries, tool capability notes,
  knowledge notes, diagnostic-skill records, version activation, and prompt
  preview. Metadata instances are exposed as read-only `metadata_instance`
  adapter resources in the compatibility list/preview surface.
- `tools.zip` export for enabled configured subprocess tools, with packaged
  executables, shell wrappers, examples, and a manifest.
- `/api/v2/tools`, readonly MCP `logagent://tools/catalog` /
  `logagent-v2://tools/catalog`, and readonly `logagent.list_tools` now share
  the same catalog envelope: `schemaVersion`, complete `tools` descriptors,
  and V1-compatible `configuredTools` summaries with configured args, timeout,
  match rules, and `maxInputFiles`, plus `sourceBuiltAnalyzers` status for the
  Flux/InfluxQL/openGemini/InfluxDB source-built analyzer IDs.
- Agent runtime executed through a real LangGraph state graph with separate
  nodes for initial evidence collection, provider request preparation,
  provider calls, tool execution, final-answer validation, and final result
  persistence. The graph uses the default stub final answer or an optional
  bounded OpenAI-compatible / local binary provider-tool loop for
  evidence-validated JSON final answers, provider-requested waiting/approval
  pauses, and per-round request/response/state plus MCP-call audit artifacts.
  The initial evidence node includes a V1-style automatic `run_tool` phase for
  matching configured tools; repeated task MCP calls for the same tool action
  reuse the existing result evidence instead of duplicating artifacts.
  OpenAI-compatible provider responses now promote provider request id,
  response id, response model, finish reason, selected audit headers, usage,
  and system fingerprint into stable `agent_response.json` `response` fields,
  while keeping the bounded raw body preview for troubleshooting. Provider
  HTTP failures keep `type=HTTPError` for compatibility and add stable
  `error.classification`, `error.retryable`, and `error.httpStatus` fields for
  authentication failures, rate limits, input-too-large responses, server
  errors, provider timeouts, and generic client errors. Binary and Claude Code
  local provider failures use the same `error.classification` /
  `error.retryable` shape for configuration, timeout, transport, process,
  output-size, decode, and parse errors.
- Settings and diagnostics endpoints for the V2 Agent provider, backend dry-run
  summary, built-in Domain Adapters, and process-local LLM response-content
  debug logging.
- Remote Executor foundation with SQLite-managed executors, environment-driven
  whitelisted SSH command templates, whitelisted SCP file templates, DB-backed
  remote command/file jobs, built-in system, openGemini, Cassandra, and RocksDB
  read-only command templates, and result/support files under the V2 data
  directory.

## Local Run

For the fastest local V2 loop from the repository root:

```bash
./scripts/v2-local.sh build
./scripts/v2-local.sh start
./scripts/v2-local.sh status
./scripts/v2-local.sh smoke-tools
./scripts/v2-local.sh stop
```

The helper creates or reuses `server-v2/.venv`, installs `server-v2` in
editable mode, initializes SQLite under `/tmp/logagent-v2-local` by default,
uses port `50993`, and only rebuilds source-referenced analyzers when
`--with-tools` or `--only-tool <name>` is supplied. The `--only-tool` value can
be a short build name such as `flux` or the V2 catalog ID such as
`flux_query_analyzer`. `status` queries the protected `/api/v2/tools` catalog
with the configured API key and prints the `sourceBuiltAnalyzers` registration,
command existence, executable, and reason fields when the server is reachable.
`smoke-tools` runs the aggregate
source-built analyzer smoke script and accepts the same `--only-tool <name>`
selector. `start` waits for `/health`; `--foreground` keeps the FastAPI server
attached for debugging.

Manual startup remains available:

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
tool-only rebuild. Single-tool rebuild accepts both short names
`influxql|flux|opengemini|influxdb` and V2 catalog IDs
`influxql_analyzer|flux_query_analyzer|opengemini_storage_analyzer|influxdb_storage_analyzer`.
After building source-referenced analyzers, run
`scripts/smoke-source-built-analyzers.sh` from the repository root to smoke all
four binaries, or pass `--only <name>` for one analyzer.
When explicit analyzer env vars are unset, V2 auto-registers the standard
analyzer filenames found under `$LOGAGENT_APP_DIR/bin/tools` or
`$LOGAGENT_V2_TOOLS_DIR`. The rebuild script loads `$HOME/.cargo/env` when
present so rustup-managed `cargo` is available for Flux analyzer builds in
non-interactive SSH shells. Deploy regression coverage verifies `--help`,
missing `LOGAGENT_SRC_DIR` validation, pid-file-scoped controls, installed
runtime checks, and `--tools-only --only-tool <name>` canonical delegation to
`scripts/build-tools.sh` without creating a V2 virtualenv or syncing WebUI.
`logagent-v2ctl.sh help`, `--help`, and `-h` print usage and exit successfully;
unknown commands still fail. `logagent-v2ctl.sh start` and `restart` export
`LOGAGENT_V2_APP_DIR`, wait for `/health` until
`LOGAGENT_V2_STARTUP_TIMEOUT_SECONDS` expires, and clean stale pid files when
startup fails. The control script is pid-file scoped by default so separate
runtime directories do not control each other's V2 processes. `status` also
prints the authenticated `sourceBuiltAnalyzers` summary so runtime deployments
can confirm whether the four analyzer submodule binaries are registered,
present, executable, unavailable, or runnable by the current V2 process.
`logagent-v2ctl.sh smoke-tools [--only-tool <name>]` delegates to
`$LOGAGENT_SRC_DIR/scripts/smoke-source-built-analyzers.sh` using the same short
names and V2 catalog IDs as `rebuild-v2-install.sh --only-tool`, so deployed
runtimes can smoke one or all source-built analyzers after rebuilding them.

## Configuration

Environment variables:

| Variable | Default | Purpose |
|---|---:|---|
| `LOGAGENT_V2_DATA_DIR` | `/tmp/logagent-v2` | SQLite, artifacts, and temp data |
| `LOGAGENT_V2_API_KEY` | `dev-token` | Bearer token for protected APIs |
| `LOGAGENT_V2_HOST` | `127.0.0.1` | Server bind host |
| `LOGAGENT_V2_PORT` | `50993` | Server bind port |
| `LOGAGENT_V2_APP_DIR` | unset | Runtime root used to auto-discover source-built analyzers under `bin/tools`; deploy control scripts export it |
| `LOGAGENT_V2_MAX_UPLOAD_BYTES` | `536870912` | Per-upload limit |
| `LOGAGENT_V2_MAX_CHUNK_BYTES` | `524288` | Per chunked-upload request limit, aligned with Rust/V1 defaults |
| `LOGAGENT_V2_MAX_ARCHIVE_FILES` | `2000` | Maximum files scanned per archive |
| `LOGAGENT_V2_MAX_ARCHIVE_BYTES` | `268435456` | Maximum aggregate extracted text bytes |
| `LOGAGENT_V2_MAX_TEXT_FILE_BYTES` | `16777216` | Maximum single text file size |
| `LOGAGENT_V2_MAX_GREP_MATCHES` | `200` | Maximum initial grep matches, aligned with Rust/V1 defaults |
| `LOGAGENT_V2_GREP_KEYWORDS` | `error,exception,timeout,fail,failed,panic,fatal,refused,denied,verify` | Comma-separated initial grep keywords, aligned with Rust/V1 defaults |
| `LOGAGENT_V2_MAX_CONCURRENT_JOBS` | `2` | Inline worker concurrency; non-positive values clamp to 1 |
| `LOGAGENT_V2_INLINE_WORKER` | `1` | Run worker inside API process |
| `LOGAGENT_V2_TOOLS_JSON` | unset | JSON array or object map of fixed whitelist tool descriptors; supports V2 `command` plus V1 `path` / `path_env`, camelCase or snake_case limits; configured IDs allow only ASCII letters, digits, `_`, and `-`; enabled commands may use `${ENV}` / `~` and must resolve to absolute paths |
| `LOGAGENT_V2_TOOLS_DIR` | unset | Optional directory for standard source-built analyzer binaries; falls back to `$LOGAGENT_V2_APP_DIR/bin/tools` or `$LOGAGENT_APP_DIR/bin/tools` |
| `LOGAGENT_V2_TOOL_INFLUXQL_ANALYZER` | `LOGAGENT_TOOL_INFLUXQL_ANALYZER` or unset | Default configured InfluxQL analyzer executable |
| `LOGAGENT_V2_TOOL_FLUX_QUERY_ANALYZER` | `LOGAGENT_TOOL_FLUX_QUERY_ANALYZER` or unset | Default configured Flux analyzer executable |
| `LOGAGENT_V2_TOOL_OPENGEMINI_STORAGE_ANALYZER` | `LOGAGENT_TOOL_OPENGEMINI_STORAGE_ANALYZER` or unset | Default configured openGemini storage analyzer executable |
| `LOGAGENT_V2_TOOL_INFLUXDB_STORAGE_ANALYZER` | `LOGAGENT_TOOL_INFLUXDB_STORAGE_ANALYZER` or unset | Default configured InfluxDB storage analyzer executable |
| `LOGAGENT_V2_PPROF_ENABLED` | auto when Go command set | Enable V1-style configured `pprof_analyzer` adapter |
| `LOGAGENT_V2_PPROF_GO_COMMAND` | `LOGAGENT_TOOL_PPROF_GO` or unset | Go executable for `go tool pprof`; required and absolute when pprof is enabled |
| `LOGAGENT_V2_FETCH_ENABLED` | `0` | Enable configured Fetch endpoint execution |
| `LOGAGENT_V2_FETCH_ALLOWED_HOSTS` | unset | Comma-separated exact `host`, `host:port`, or `http(s)://host[:port]` allowlist; required when Fetch is enabled |
| `LOGAGENT_V2_FETCH_TIMEOUT_SECONDS` | `20` | Per-request Fetch timeout; non-positive values clamp to 1 |
| `LOGAGENT_V2_FETCH_MAX_REQUEST_BYTES` | `1048576` | Maximum Fetch request body bytes; non-positive values clamp to 1 |
| `LOGAGENT_V2_FETCH_MAX_RESPONSE_BYTES` | `1048576` | Maximum stored Fetch response preview bytes; non-positive values clamp to 1 |
| `LOGAGENT_V2_FETCH_MAX_REDIRECTS` | `5` | Maximum manually revalidated Fetch redirects; negative values clamp to 0 |
| `LOGAGENT_V2_FETCH_SECRET_KEY` | unset | Fernet 32-byte base64 key; required when Fetch is enabled and used for encrypted credential sets |
| `LOGAGENT_V2_AGENT_PROVIDER` | `stub` | `stub`, `openai_compatible`, `binary`, or `claude_code`; invalid values fail settings loading |
| `LOGAGENT_V2_AGENT_BASE_URL` | unset | OpenAI-compatible base URL, required when that provider is selected |
| `LOGAGENT_V2_AGENT_MODEL` | unset | Model name, required for `openai_compatible` |
| `LOGAGENT_V2_AGENT_API_KEY` | unset | Bearer token, required for `openai_compatible` |
| `LOGAGENT_V2_AGENT_BINARY_PATH` | unset | Binary provider command path; required and absolute when `binary` is selected |
| `LOGAGENT_V2_AGENT_BINARY_MAX_OUTPUT_BYTES` | `1048576` | Maximum stdout bytes accepted from the binary Agent provider |
| `LOGAGENT_V2_CLAUDE_CODE_PATH` | `LOGAGENT_CLAUDE_CODE_PATH` or unset | Claude Code CLI path; required and absolute when `claude_code` is selected |
| `LOGAGENT_V2_CLAUDE_CODE_MAX_OUTPUT_BYTES` | `1048576` | Maximum stdout bytes accepted from Claude Code CLI |
| `LOGAGENT_V2_CLAUDE_CODE_PERMISSION_MODE` | `dontAsk` | Legacy flat override for the `diagnose` Claude Code permission profile |
| `LOGAGENT_V2_CLAUDE_CODE_TOOLS` | empty | Legacy flat `diagnose` native Claude Code tool list passed with `--tools`; empty disables built-in native tools |
| `LOGAGENT_V2_CLAUDE_CODE_ALLOWED_TOOLS` | `mcp__logagent__*` | Legacy flat `diagnose` allowed tools; `mcp__logagent__*` is auto-added |
| `LOGAGENT_V2_CLAUDE_CODE_DISALLOWED_TOOLS` | V1 diagnose defaults | Legacy flat `diagnose` disallowed tools |
| `LOGAGENT_V2_CLAUDE_CODE_PERMISSION_PROFILES_JSON` | unset | JSON object keyed by `diagnose`, `code_investigation`, or `fix` to override per-mode Claude Code permission profiles |
| `LOGAGENT_V2_AGENT_TIMEOUT_SECONDS` | `60` | Provider request timeout |
| `LOGAGENT_V2_AGENT_MAX_ROUNDS` | `4` | Maximum provider/tool-loop rounds per run |
| `LOGAGENT_V2_AGENT_MAX_LLM_CALLS` | `4` | Maximum provider calls per run before a budget-limited result |
| `LOGAGENT_V2_AGENT_MAX_ACTIONS` | `6` | Maximum provider-directed task MCP tool calls per run before a budget-limited result |
| `LOGAGENT_V2_AGENT_MAX_REPEATED_ACTION_FINGERPRINTS` | `1` | Maximum successful identical task MCP tool fingerprint count before a budget-limited result |
| `LOGAGENT_V2_AGENT_MAX_OUTPUT_TOKENS` | `2048` | Maximum provider output tokens for V2 Agent calls |
| `LOGAGENT_V2_AGENT_MAX_TOTAL_TOKENS` | `200000` | Maximum cumulative provider usage tokens before the next round becomes budget-limited |
| `LOGAGENT_V2_AGENT_MAX_RUNTIME_SECONDS` | `300` | Maximum Agent runtime seconds for one graph invocation before the next round becomes budget-limited |
| `LOGAGENT_V2_AGENT_MAX_USER_PROMPTS` | `3` | Maximum persisted `request_user_input` prompts per run before resumed analysis becomes budget-limited |
| `LOGAGENT_V2_AGENT_MAX_APPROVALS` | `3` | Maximum persisted `request_approval` prompts per run before resumed analysis becomes budget-limited |
| `LOGAGENT_V2_REMOTE_EXECUTION_ENABLED` | `1` | Enable V2 Remote Executor APIs and jobs |
| `LOGAGENT_V2_REMOTE_SSH_COMMAND` | `/usr/bin/ssh` | Absolute SSH executable used by Remote Executor jobs when remote execution is enabled |
| `LOGAGENT_V2_REMOTE_SCP_COMMAND` | `/usr/bin/scp` | Absolute SCP executable used by approved Environment Collector file pulls when remote execution is enabled |
| `LOGAGENT_V2_REMOTE_CONNECT_TIMEOUT_SECONDS` | `10` | SSH connect timeout option |
| `LOGAGENT_V2_REMOTE_COMMAND_TIMEOUT_SECONDS` | `30` | Default remote command timeout |
| `LOGAGENT_V2_REMOTE_MAX_OUTPUT_BYTES` | `1048576` | Maximum stored stdout/stderr bytes per stream |
| `LOGAGENT_V2_REMOTE_FILE_MAX_BYTES` | `16777216` | Default max bytes accepted for each approved remote file collection |
| `LOGAGENT_V2_REMOTE_HOST_KEY_POLICY` | `accept-new` | `strict`, `accept-new`, or `no` host-key behavior |
| `LOGAGENT_V2_REMOTE_COMMANDS_JSON` | built-in read-only templates | JSON array that replaces the default whitelisted remote command templates; IDs allow only ASCII letters, digits, `_`, and `-`; argv entries are trimmed, empty entries are dropped, and the final argv must be non-empty |
| `LOGAGENT_V2_REMOTE_FILES_JSON` | unset | JSON array of whitelisted remote file templates for approved `collect_environment` file pulls; each entry has safe `id`/`fileId`, absolute safe `remotePath`, optional timeout, and optional `maxBytes` |
| `LOGAGENT_V2_CODE_REPOS_JSON` | unset | JSON object or array of configured read-only code repositories for `logagent.search_code` and `logagent.diff_code`; each entry requires absolute `repoPath`, `defaultRef`, optional `versionRefs`, and relative `searchRoots` |
| `LOGAGENT_V2_CODE_WORKTREE_ROOT` | `data_dir/code_worktrees` | Absolute cache root for detached Code Evidence worktrees; V2 creates/reuses paths under this root and rejects relative values |
| `LOGAGENT_V2_CODE_WORKTREE_MAX_PER_REPO` | `5` | Maximum detached Code Evidence worktrees retained per product cache before least-recently-used cleanup removes older `wt_*` directories |
| `LOGAGENT_V2_WEBUI_DIR` | repo `webui/out` | Static WebUI build directory served by `GET /` |
| `LOGAGENT_V2_HUAWEI_PACKAGE_SYNC_ENABLED` | `0` | Enable Huawei OBS + GaussDB package sync |
| `LOGAGENT_V2_HUAWEI_OBS_ENDPOINT` | unset | Huawei OBS endpoint; when enabled must be `http/https` with no path/query/fragment |
| `LOGAGENT_V2_HUAWEI_OBS_BUCKET` | unset | Huawei OBS bucket; when enabled allows letters, digits, `.`, and `-` |
| `LOGAGENT_V2_HUAWEI_OBS_OBJECT_PREFIX` | unset | Default safe relative object key prefix |
| `LOGAGENT_V2_HUAWEI_OBS_ACCESS_KEY` | unset | Huawei OBS access key; required and trimmed when enabled |
| `LOGAGENT_V2_HUAWEI_OBS_SECRET_KEY` | unset | Huawei OBS secret key; required and trimmed when enabled |
| `LOGAGENT_V2_HUAWEI_OBS_SECURITY_TOKEN` | unset | Optional Huawei OBS security token; trimmed when present |
| `LOGAGENT_V2_HUAWEI_GAUSSDB_DSN` | unset | GaussDB/PostgreSQL DSN for package sync; required and trimmed when enabled |

Setting any `LOGAGENT_V2_TOOL_*_ANALYZER` variable, or its Rust/V1
`LOGAGENT_TOOL_*_ANALYZER` alias, auto-registers that source-built analyzer as
a configured subprocess tool. The V2-specific name takes precedence. If those
variables are unset, V2 auto-discovers the standard analyzer filenames from
`LOGAGENT_V2_TOOLS_DIR`, `$LOGAGENT_V2_APP_DIR/bin/tools`, or
`$LOGAGENT_APP_DIR/bin/tools`. Both paths use the same args, timeouts,
`maxInputFiles`, match patterns, and keywords as `examples/server-tools.yaml`.
The storage defaults preserve the wider V1 settings: openGemini storage uses
`maxInputFiles=10`; InfluxDB storage uses `timeoutSeconds=60` and
`maxInputFiles=5`. `LOGAGENT_V2_TOOLS_JSON` accepts
either a V2 descriptor array or a Rust/V1-style object keyed by tool id. Each
descriptor may use V2 `command`, V1 `path`, or V1 `path_env` / `pathEnv`, and
may use camelCase or snake_case limit fields such as `timeoutSeconds` /
`timeout_seconds` and `maxInputFiles` / `max_input_files`. Source-built analyzer
paths and configured tool paths expand environment variables and `~` during
configuration loading; enabled tools fail startup if the resolved command is
not absolute. User-configured tool IDs use the Rust/V1 `tools.<name>` safe
pattern: non-empty ASCII letters, digits, `_`, and `-` only. Built-in
`logagent.*` tools live outside that configured-tool namespace. Configured
`match.filePatterns` and `match.keywords` are normalized to lowercase at load
time, matching the Rust/V1 catalog behavior.

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
GET  /api/v2/tasks?workspaceId=<workspace_id>
POST /api/v2/tasks
GET  /api/v2/tasks/:task_id
GET  /api/v2/tasks/:task_id/timeline
GET  /api/v2/tasks/:task_id/evidence
GET  /api/v2/tasks/:task_id/artifacts
GET  /api/v2/tasks/:task_id/analysis
GET  /api/v2/tasks/:task_id/result
POST /api/v2/tasks/:task_id/messages
POST /api/v2/tasks/:task_id/actions/:action_id/decision
GET  /api/v2/evidence/:evidence_id
GET  /api/v2/artifacts/:artifact_id
GET  /api/v2/tools
GET  /api/v2/tools/:tool_id
POST /api/v2/tools/:tool_id/runs
GET  /api/v2/tools/runs
GET  /api/v2/tools/runs/:run_id
GET  /api/v2/tools/runs/:run_id/result
GET  /api/v2/tools/runs/:run_id/artifacts
GET  /api/tools
GET  /api/tools/:tool_id
POST /api/tools/:tool_id/runs
GET  /api/tools/runs
GET  /api/tools/runs/:run_id
GET  /api/tools/runs/:run_id/result
GET  /api/tools/runs/:run_id/artifacts
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
GET  /api/v2/executor-file-templates
GET  /api/v2/executor-runs
POST /api/v2/executor-runs
GET  /api/v2/executor-runs/:run_id
GET  /api/v2/executor-runs/:run_id/result
GET  /api/v2/executor-runs/:run_id/files/:file_name
GET  /api/executors
POST /api/executors
GET  /api/executors/:executor_id
PATCH /api/executors/:executor_id
DELETE /api/executors/:executor_id
GET  /api/executor-command-templates
GET  /api/executor-file-templates
GET  /api/executor-runs
POST /api/executor-runs
GET  /api/executor-runs/:run_id
GET  /api/executor-runs/:run_id/result
GET  /api/executor-runs/:run_id/files/:file_name
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
GET  /api/v2/metadata/imports/:import_id/preview
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
POST /api/v2/tasks/:task_id/case
GET  /api/v2/cases
GET  /api/v2/cases/imports
GET  /api/v2/cases/imports/:import_id
POST /api/v2/cases/imports
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
POST /api/fetch/imports/preview
GET  /api/fetch/endpoints
POST /api/fetch/endpoints
GET  /api/fetch/endpoints/:endpoint_id
PATCH /api/fetch/endpoints/:endpoint_id
DELETE /api/fetch/endpoints/:endpoint_id
POST /api/fetch/endpoints/:endpoint_id/runs
GET  /api/fetch/runs
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
Preview `contextIds` are trimmed, empty entries are ignored, duplicates are
collapsed, invalid ids return HTTP 400, and at most 32 explicit ids are
accepted.

## Settings And Diagnostics

V2 exposes Settings diagnostics under `/api/v2/settings/*`. The LLM section is
mapped to the V2 Agent provider configuration: `stub` is local,
`openai_compatible` calls the configured OpenAI-compatible `/models` and
`/chat/completions` endpoints, `binary` validates the configured executable
then runs the same final-answer parse path through a local process, and
`claude_code` validates the configured Claude Code CLI path without launching a
task session from the settings chat test. Responses never include API keys or
configured executable paths.

`/api/v2/settings/agent-backends` describes the in-process V2 Agent runtime
plus optional provider execution mode and the active Agent budgets
(`maxRounds`, `maxLlmCalls`, `maxActions`, and
`maxRepeatedActionFingerprints`). The diagnostic endpoint is a dry-run
configuration check; for `binary` it checks that
`LOGAGENT_V2_AGENT_BINARY_PATH` is absolute, regular, and executable, and for
`claude_code` it applies the same path checks to
`LOGAGENT_V2_CLAUDE_CODE_PATH` / `LOGAGENT_CLAUDE_CODE_PATH`. Both the summary
and diagnostic response include `graphRuntime` with the LangGraph engine, graph
name, and node list used by analysis runs. The Domain
Adapter endpoint returns the built-in `opengemini_influxdb` active adapter plus
`cassandra` and `rocksdb` skeleton adapters. The V2 registry mirrors the
Rust/V1 built-in adapter summaries, including `case_context`,
`storage_file_tool_results`, `pprof_analyzer`, and the V1 Cassandra/RocksDB
planned tool names. The same adapter summaries are also available through
readonly MCP `logagent-v2://domain-adapters` and `logagent.list_domain_adapters`.

`/api/v2/debug/llm` toggles process-local response-content logging for provider
debugging. It only logs model response content to stderr and does not log
prompts, headers, or API keys. The setting resets when the process restarts;
route-level regression coverage keeps the V1 `GET/PUT` debug toggle behavior
available in V2.

## Remote Executors

V2 Remote Executor APIs live under `/api/v2/executors`,
`/api/v2/executor-command-templates`, `/api/v2/executor-file-templates`, and
`/api/v2/executor-runs`. Rust/V1-style `/api/executors`,
`/api/executor-command-templates`, `/api/executor-file-templates`, and
`/api/executor-runs` aliases share the same handlers. Executors are stored in
SQLite with host, port, SSH user, tags, notes, enabled state, and timestamps.
Deleting an executor disables it instead of removing historical run records.
Executor run create/list/get
responses expose Rust/V1 TaskResponse-compatible summary fields including
`taskId`, `runId`, `url`, `taskKind=remote_command_run`, `sessionId=null`,
`analysisMode=diagnose`, `analysisLanguage=zh-CN`, `status`, `phase`, and
`createdAt`.

Command templates are loaded from `LOGAGENT_V2_REMOTE_COMMANDS_JSON`; if unset,
V2 exposes built-in read-only templates for `smoke_ls_root`, `system_uname`,
`uptime_load`, `disk_usage`, `memory_usage`, `process_overview`,
`network_listeners`, `opengemini_processes`, `opengemini_config_dirs`,
`opengemini_log_dirs`, `opengemini_data_dirs`, `cassandra_processes`,
`cassandra_config_dirs`, `cassandra_log_dirs`, `cassandra_data_dirs`,
`rocksdb_data_dirs`, `rocksdb_wal_dirs`, and `rocksdb_log_dirs`. The product
defaults use fixed process names and common directory candidates; they do not
allow shell pipes, redirects, glob expansion, or user-provided argv. Template
descriptors match the Rust/V1 behavior: `enabled` also reflects global remote
execution state, and
`timeoutSeconds` is always
filled with the template override or default remote command timeout. Template
IDs are validated with the Rust/V1 safe pattern: non-empty ASCII letters,
digits, `_`, and `-` only. Template argv is normalized like Rust/V1 by trimming
entries, dropping empty entries, and requiring at least one remaining argv item.
Runs are DB-backed jobs. The worker invokes the
configured SSH executable with fixed argv. `LOGAGENT_V2_REMOTE_SSH_COMMAND`
expands environment variables and `~`; when remote execution is enabled it must
resolve to an absolute path, matching the Rust/V1 `remote_execution.ssh_binary`
boundary. `LOGAGENT_V2_REMOTE_HOST_KEY_POLICY` is validated at startup and must
be `accept-new`, `strict`, or `no`:

```text
/usr/bin/ssh -o BatchMode=yes -o ConnectTimeout=<seconds> -o StrictHostKeyChecking=<policy> -p <port> <user>@<host> <template argv...>
```

The API never accepts free-form shell commands.

File templates are loaded from `LOGAGENT_V2_REMOTE_FILES_JSON` and are used by
approved `collect_environment` actions, not by the manual run creation API.
Each descriptor provides a safe `id` / `fileId`, display metadata, absolute
safe `remotePath`, optional timeout, and optional `maxBytes`. Remote paths must
be absolute, cannot contain `..`, `.`, `//`, backslashes, whitespace, shell
globs, or characters outside the safe path set. The worker invokes the
configured SCP executable with fixed argv and deletes an over-limit file after
download if it exceeds the template or global byte cap:

```text
/usr/bin/scp -B -o BatchMode=yes -o ConnectTimeout=<seconds> -o StrictHostKeyChecking=<policy> -P <port> <user>@<host>:<remotePath> <data_dir>/remote_runs/<run_id>/remote_file/<basename>
```

Command results are written under:

```text
data_dir/
  remote_runs/
    <run_id>/
      remote_command/
        result.json
        stdout.txt
        stderr.txt
```

File collection results are written under:

```text
data_dir/
  remote_runs/
    <run_id>/
      remote_file/
        result.json
        stdout.txt
        stderr.txt
        <basename>
```

Non-zero exit, timeout, and start failures are recorded in `result.json`; the
remote run itself reaches `SUCCEEDED` when the controlled execution completed
and result files were persisted. System errors before result persistence mark
the run `FAILED`. `GET /api/v2/executor-runs/:run_id/result` returns HTTP 409
with the current run status until a result is available, then returns the
Rust/V1-compatible wrapper fields `taskId`, `executorId`, `commandId`,
`resultPath`, and `result`. The protected
`GET /api/v2/executor-runs/:run_id/files/:file_name` endpoint downloads only
the persisted `result`, `stdout`, `stderr`, or collected file (`collected` /
`file`) logical names. The server resolves those logical names from the stored
run result and rejects paths outside `LOGAGENT_V2_DATA_DIR`.

## Verification

```bash
python3 -m compileall logagent_v2
PYTHONPATH=. python3 -m unittest discover tests
```

This V2 slice migrates V1 configured analyzer execution, metadata/preprocess/
fetch/pprof/Huawei built-ins, storage analyzer materialized inputs, raw upload
fallback, and the default WebUI routes for Analyze, Memory, System Context,
Metadata, Tools, Fetch, Executors, and Settings. Regression coverage now locks
the V1 built-in tool names and key task MCP input schemas across task MCP,
readonly MCP, and the manual Tools catalog, including legacy `tool/inputFile`,
`fetchId`, `startLine`/`endLine`, metadata field/tag, and waiting/approval
parameters. Successful task and readonly MCP `tools/call` responses include the
Rust/V1-compatible `isError=false` envelope flag. Readonly MCP still exposes the
catalog for discovery, but any `tools/call` targeting catalog
configured/manual built-in tools is rejected with an explicit readonly error.
The default stub provider remains a low-confidence evidence-summary fallback;
non-stub providers use the LangGraph provider/tool loop for model-driven
follow-up.

## Job Recovery

On startup, V2 scans DB-backed jobs left in `running` state by a prior process.
Interrupted non-terminal `run_analysis` jobs reset their Run to `queued`, append
`run.recovered`, and become immediately acquirable. Interrupted remote command
jobs reset their remote run to `QUEUED`. If the associated Run or remote run is
already terminal or waiting for user/approval, the stale job is marked
`succeeded` instead of rerunning.

## Agent Runtime

By default V2 uses `LOGAGENT_V2_AGENT_PROVIDER=stub`, which produces the
deterministic low-confidence evidence summary used by the foundation tests. It
does not run additional model-driven tool calls, so its final answer now
describes that limitation without implying that the V2 tool loop is absent.
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

`LOGAGENT_V2_AGENT_PROVIDER=claude_code` keeps the same LangGraph run lifecycle
but executes each provider round by launching the configured Claude Code CLI.
V2 writes a per-run working directory under `data_dir/tmp/claude_sessions/`,
materializes `claude_prompt.md` and `claude_mcp_config.json`, injects
`LOGAGENT_V2_API_KEY` only into the child process environment, and invokes:

```text
<claude_code_path> --print --output-format json --json-schema <schema> \
  --mcp-config claude_mcp_config.json --strict-mcp-config \
  --permission-mode <mode> --tools <tools> [--allowedTools ...] [--disallowedTools ...]
```

The startup stdin is the short Claude prompt that instructs Claude to read task
context through the HTTP task MCP `analysis_package` resource. Large log,
Metadata, and tool context are not placed in argv or stdin. Claude Code stdout
must be a Claude JSON envelope whose `structured_output` / `structuredOutput`
or `result` contains `runtimeStatus=completed` with `finalAnswer`,
`waiting_for_user` with `pendingPrompt`, or `waiting_for_approval` with
`pendingApproval`. Waiting states are bridged into the existing
`logagent.request_user_input` and `logagent.request_approval` task MCP tools.
When a waiting run resumes and the previous Claude response recorded
`response.sessionId`, V2 adds `--resume <session_id>` to the next Claude Code
CLI invocation and records `response.resumedSessionId` in `agent_response.json`.
Claude envelope `usage` and `total_cost_usd` / `totalCostUsd` are preserved as
`response.usage` and `response.cost.usd`. OpenAI-compatible Chat Completions
responses preserve `response.providerRequestId`,
`response.providerResponseId`, `response.responseModel`,
`response.finishReason`, `response.usage`, and an allowlisted
`response.providerRequestHeaders` map. After each Claude Code provider response,
V2 also writes a fresh `claude_session.json` runtime artifact with
runtime/provider status, optional `claudeSessionId` / `resumedSessionId`,
usage/cost, prompt delivery, error/validation status, and the linked
`agent_response` artifact id; failed responses without a session id still
replace the initial contract artifact as the latest session audit.

The CLI permission policy is selected from the Workspace `mode` using the V1
profile names: `diagnose` disables native tools and disallows
`Bash/Edit/Write/Read/Grep`, `code_investigation` allows `Read,Grep,Bash` and
disallows edits, and `fix` uses `acceptEdits` with `Read,Grep,Bash,Edit,Write`.
Every profile automatically includes `mcp__logagent__*` in `allowedTools`.
Legacy flat Claude Code env vars override the `diagnose` profile; use
`LOGAGENT_V2_CLAUDE_CODE_PERMISSION_PROFILES_JSON` for per-mode overrides.
`agent_request.json`, `agent_response.json`, and runtime `claude_session.json`
record `analysisMode`, `permissionProfile`, and `nativeToolPolicy`.

The run lifecycle is executed by a LangGraph state graph with
`collect_initial_evidence`, `prepare_agent_request`, `call_agent_provider`,
`execute_tool_calls`, `validate_final_answer`, and `finalize_result` nodes. The
latest `analysis_state.json` records `graphRuntime.engine=langgraph` and the
node list so runtime artifacts prove which orchestration graph executed the
run.

The provider may return a `tool_calls` object for tools advertised in the
prompt: log search/slice, Metadata, Case Memory, Skill references, Code
Evidence when code repos are configured, Fetch catalog, configured domain
tools, and Fetch execution when Fetch is enabled. V2
validates the requested tool name against the advertised set, executes through
the existing task MCP call path, feeds the observations into the next round, and
enforces V1-style round, LLM-call, and action budgets. Budget exhaustion writes
a validated low-confidence final answer with `budgetLimited=true` instead of
failing the run. Follow-up evidence refs returned by tools, such as
`log_searches/...#matches/<index>` or tool
`finalEvidenceRefs`, are merged into the next round's `allowedEvidenceRefs` so
the provider can legally cite evidence it requested. The provider must
eventually return one JSON final-answer object; V2 normalizes it and rejects
unsupported or non-current evidence refs before marking the run `succeeded`.
Provider-visible `logagent.get_log_slice` uses the same center-line or
V1-compatible `startLine`/`endLine` range schema as task MCP, and
provider-visible `logagent.run_domain_tool` uses the same `toolId` or
V1-compatible `tool + inputFile` schema while excluding manual-only tools.
Provider-visible `logagent.search_logs` also exposes the V1-compatible
`maxMatches` cap.

Provider-directed tool use is bounded by `LOGAGENT_V2_AGENT_MAX_ROUNDS`,
`LOGAGENT_V2_AGENT_MAX_LLM_CALLS`, `LOGAGENT_V2_AGENT_MAX_ACTIONS`,
`LOGAGENT_V2_AGENT_MAX_REPEATED_ACTION_FINGERPRINTS`,
`LOGAGENT_V2_AGENT_MAX_TOTAL_TOKENS`,
`LOGAGENT_V2_AGENT_MAX_RUNTIME_SECONDS`,
`LOGAGENT_V2_AGENT_MAX_USER_PROMPTS`, and
`LOGAGENT_V2_AGENT_MAX_APPROVALS`. These limits are implemented as explicit
graph transitions: provider tool-call responses route
through `execute_tool_calls`, normal answers route through
`validate_final_answer`, waiting/approval tools end the current graph invocation
in a waiting state, and non-waiting tool observations loop back to
`prepare_agent_request`. If a budget is exhausted before the next provider
call, `prepare_agent_request` routes to an internal `budget_guard` response
that cites current evidence, records `analysis_state.json` status
`budget_limited`, and finalizes successfully. If the provider asks for a task
MCP tool fingerprint that has already succeeded the configured number of times,
the current round is also finalized as `budget_limited` without executing the
duplicate call. Provider usage is recorded on each round as `tokenUsage` when
the backend returns OpenAI/Claude-style usage fields. Resumed runs include a bounded `interactionContext` with recent
user messages, answered/approved/rejected actions, pending actions, and a
finalize-with-current-evidence directive when the user requests it.

Every run writes `analysis_package.json` after initial evidence collection. The
package is a bounded Agent context bundle: Workspace/run metadata, task MCP
resource URIs, manifest outline, grep match preview, analyzer tool input
outline, bounded artifact index outline, enabled Environment Collector executor
and template candidates, system/metadata context outlines,
bounded resume `analysisState`
(recent user messages, action results, pending actions, and
`finalizeRequested`), and the current allowed evidence refs, including
`session_text_input.json#question`. The resource index includes optional
Rust/V1 Claude runtime compatibility resources `claude_mcp_config` and
`claude_session` alongside Agent audit resources. Task MCP exposes it as
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
Each run also writes Rust/V1 Claude runtime contract artifacts:
`claude_prompt.md`, `claude_mcp_config.json`, and `claude_session.json`. The
MCP config points at the V2 task HTTP MCP endpoint and uses
`${LOGAGENT_V2_API_KEY}` as an Authorization placeholder, so the real API key
is not written to artifacts. When `LOGAGENT_V2_AGENT_PROVIDER=claude_code`,
the same contract files are also materialized into the temporary Claude session
directory used by the real CLI invocation. Once Claude Code returns session
metadata, the latest `claude_session` task MCP resource points to the runtime
session artifact instead of the initial `contract_ready` artifact.
Task MCP also exposes V1-compatible aggregate resources: `artifact_index`
lists current run uploads and evidence artifacts by stable logical path,
`tool_results` aggregates `tool_result` and `fetch_result` artifacts, and
`case_context` returns the latest background Case recall/search context. The
artifact index includes the persisted run question at `session_text_input.json`.
The HTTP artifacts endpoint returns the same compatibility payloads in one
response for WebUI and Rust/V1 migration callers: `manifestPath`/`manifest`,
`grepResultsPath`/`grepResults`, `textInputPath`/`textInput`,
`metadataContextPath`/`metadataContext`, `systemContextPath`/`systemContext`,
`caseContextPath`/`caseContext`, `analysisPackagePath`/`analysisPackage`,
`agentResponsePath`/`agentResponse`, `analysisStatePath`/`analysisState`,
`claudePromptPath`/`claudePrompt`, `claudeMcpConfigPath`/`claudeMcpConfig`,
`claudeSessionPath`/`claudeSession`, `mcpCallsPath`/`mcpCalls`, and
`toolResults`.

Successful runs also write `result.json` and `result.md`, then persist a short
alias. OpenAI-compatible and binary Agent providers receive a separate
`run_alias` JSON prompt after the final answer is validated; stub and
`claude_code` modes, plus any alias provider failure, fall back to the final
summary or question. `GET
/api/v2/runs/<run_id>/result` returns the stored final answer plus artifact and
evidence metadata after success; before a final answer exists it returns HTTP
409 with the current run status. Task MCP exposes `result` and
`result_markdown`. The
alias is stored on the Run record for history/UI display; it is not model
evidence and does not affect final-answer validation.

## Uploads

V2 supports three upload paths:

- `POST /api/v2/workspaces/<workspace_id>/uploads` for one multipart `file`.
  The request may also include a text `filename` field; V2 uses it as the
  stored filename after the same basename and character filtering as Rust/V1.
  Basenames that resolve to `.` or `..` are rejected with HTTP 400.
- `POST /api/v2/workspaces/<workspace_id>/uploads/batch` for multiple
  multipart files under one Workspace; repeated file parts may be named
  `file` or `files`, matching Rust/V1 batch upload clients. Each stored batch
  filename uses the same basename and character filtering as single uploads.
- `POST /api/v2/workspaces/<workspace_id>/uploads/init`, followed by
  `POST /api/v2/uploads/<session_id>/chunks?offset=<bytes>` and
  `POST /api/v2/uploads/<session_id>/complete`, for restartable chunked upload.

Session APIs expose the same stored uploads through an attachment set:
`POST /api/v2/sessions/<session_id>/uploads` accepts either one multipart
`file` for direct upload, with the same optional text `filename` override, or
JSON `{"uploadIds":[...]}` to attach existing Workspace uploads. JSON attach
uses Rust/V1 normalization: entries are trimmed, empty entries are ignored,
duplicates are collapsed, non-`upl_` ids return HTTP 400, and at least one
valid id is required. `DELETE /api/v2/sessions/<session_id>/uploads/<upload_id>`
detaches an upload only before any task run exists; the Upload row and artifact
remain stored. Native Agent V2 mode uses these Session-scoped upload APIs, so
browser imports do not require Chrome Extension code changes.

Chunked uploads persist session state in SQLite and temporary bytes under
`data_dir/tmp/upload_sessions`. Each chunk request is bounded by
`LOGAGENT_V2_MAX_CHUNK_BYTES`, total received bytes are bounded by
`LOGAGENT_V2_MAX_UPLOAD_BYTES`, init filenames are stored after Rust/V1-style
basename and character filtering, invalid `.` / `..` basenames are rejected,
and completion validates received size, converts the temp file into a regular
artifact, creates an Upload row, and marks the session completed.

## Initial Evidence Pipeline

When a run starts, V2 now reads all uploads attached to the Workspace and:

- accepts plain `.log`, `.txt`, `.out`, `.err`, `.trace`, `.json`, `.jsonl`,
  `.yaml`, `.yml`, `.conf`, and `.cfg` files;
- scans `.zip`, `.tar`, `.tar.gz`, and `.tgz` packages without writing archive
  members to arbitrary filesystem paths;
- assigns ordinary text uploads and ordinary archive members stable
  `extracted/<uploadDir>/...` logical paths, using `_2` suffixes for repeated
  upload directory names so multi-upload manifests and log slices are
  unambiguous; legacy bare filename or original archive-member selectors still
  resolve when they match exactly one current Workspace text file;
- recognizes openGemini-style node log packages named
  `<packageId>_<instanceId>_<nodeId>_<yyyy_MM_dd_HH_mm_ss_micros>_logs.tar.gz`;
- rejects absolute paths, `..` path traversal, and unsafe archive entries;
- skips symlinks and non-file archive members;
- writes bounded `manifest.json` and `grep_results.json` artifacts, including
  V1-style `sourceUrl`, manifest upload summaries for node packages, and grep
  match aliases (`file`, `line`, `evidenceRef`) alongside V2 fields;
- uses `LOGAGENT_V2_GREP_KEYWORDS` for initial grep, defaulting to the
  Rust/V1 keyword set instead of deriving keywords from the user question;
- writes `tool_inputs/index.json`, node-package `log_text` JSONL artifacts,
  `influxql_analyzer` JSONL artifacts, and `flux_query_analyzer` JSONL
  artifacts when logs contain supported query lines;
- exposes the manual `logagent.preprocess_log_package` result with V1-style
  `nodes` aggregation, `manifestPath` / `grepResultsPath` /
  `toolInputsPath`, `toolInputs`, timing metadata, V2 `nodePackages` details,
  and artifact id/path fields; `nodes[]` is aggregated from manifest upload
  summaries so `ignoredFileCount`, package warnings, and compressed log group
  counts match Rust/V1;
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
and gzip content is decoded by magic bytes before grep indexing. Manifest upload
summaries include `packageId`, `instanceId`, `nodeId`, `packageTimestamp`,
`nodeDir`, sorted `logGroups`, `ignoredFileCount`, and `ignoredPathSamples`;
manifest file entries include V1 aliases such as `size`, `uploadId`,
`instanceId`, `nodeId`, `packageTimestamp`, `compressed`, and `compression`.
The manifest always includes the V1-compatible `sourceUrl` field copied from
the Workspace, using `null` when the Workspace has no source URL.
A node package with no supported log directory fails instead of producing an
empty manifest.

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
  - `summary` includes Rust/V1-compatible top-level fields such as `taskId`,
    `sessionId`, `analysisMode`, `analysisLanguage`, `question`, `sourceUrl`,
    `nodeId`, and `uploadIds`, and also keeps the V2 nested `run` and
    `workspace` objects.
  - `metadata_context` returns the bounded
    `metadata_context_outline` resource, not the full
    `metadata_context.json` artifact; detailed metadata must be read through
    `logagent.query_metadata` or field/tag metadata tools.
- `tools/list`
- `tools/call logagent.search_logs` with V1-compatible optional `maxMatches`
  clamped to 1..200; responses keep the nested V2 `search` object and expose
  Rust-compatible top-level `artifactPath`, `totalMatches`, `keywordCounts`,
  `unmatchedKeywords`, `matches`, `evidenceRefs`, and `note`; top-level
  `matches[]` includes the Rust/V1-compatible `index` field
- `tools/call logagent.get_log_slice` with either `lineNumber` plus
  `before`/`after`, or V1-compatible `startLine`/`endLine`; responses keep the
  nested V2 `slice` object and expose Rust-compatible top-level `artifactPath`,
  `evidenceRefs`, and `lines`; logical slice paths use stable
  `log_slices/slice_<digest>.json#lines` refs. The persisted `startLine` and
  `endLine` keep the requested range, while `lines[]` only includes lines that
  exist in the file.
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
tools whose descriptor is currently `runnable=true`, and its `tools/list` input
schema advertises both V2 `toolId` and Rust/V1 `tool + inputFile` forms with
`anyOf`. The OpenAI-compatible and binary Agent provider prompt advertises the
same schema in `availableTools` and uses the same runnable configured-tool enum,
excluding manual-only tools such as `pprof_analyzer`. Built-ins are available
through their dedicated task MCP tools or the protected manual Tools API
according to each descriptor's `runnable` policy.
Tool stdout is parsed as JSON when possible and persisted as `tool_result`
evidence. Generic JSON output can use
`summary`, `message`, or `title`, plus `findings`, `issues`, or `diagnostics`.
For Rust/V1 parser compatibility, JSON number values in string-like fields are
normalized to strings while booleans are ignored for those fields.
InfluxQL analyzer report JSON is adapted into a compact summary and findings
for special rules, parse errors, realtime classification, fingerprints, compare
fingerprint deltas, and rule deltas. V2 uses the Rust/V1 report detection
rule: `total_records`, `total_statements`, and `fingerprints` keys are enough
to enter the specialized parser, even if `fingerprints` is not an array.
Flux analyzer stdout keeps tool-provided `summary/findings` when present. If a
version only returns `metrics`, `topQueries`, and `parseErrors`, V2 derives a
`flux query stats` summary plus parse-error, Top Flux template, p95 latency,
and new-template findings.

Configured tools may declare `paramsSchema`. Task MCP `logagent.run_domain_tool`
then accepts `params`, validates a conservative object-schema subset
(`required`, `additionalProperties`, primitive `type`, and `enum`), and replaces
`{params.name}` placeholders in configured argv. Commands and argv templates
still come only from Server configuration. For tools with `{input_file}`, V2
adds a reserved `params.inputFiles` array to the descriptor; task MCP also
accepts V1-style top-level `inputFile` and maps it to that same selector.
Each configured action runs with `cwd` set to a materialized V2 tool workspace
under `data_dir/tmp/tool_workspaces/...`. V2 copies the current run's
`manifest.json`, `grep_results.json`, and, when present, `tool_inputs/index.json`
into that workspace, then expands Rust/V1 placeholders: `{workspace}`,
`{manifest_path}`, `{grep_results_path}`, `{action_id}`, `{input_file}`, and
`{params.name}`. Unsupported placeholder-like tokens such as `{unknown}` fail
before subprocess execution.

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

The same selection path runs automatically during analysis after
`manifest.json` and `grep_results.json` are created. Only enabled configured
tools whose argv requires `{input_file}` and no runtime `{params.name}` values
are auto-run; manual-only built-ins such as `pprof_analyzer`, Fetch,
preprocess, metadata, and Huawei sync remain explicit tools. Auto results are
stored as `tool_result` evidence, added to
`analysis_package.allowedEvidenceRefs`, and shown to the first provider prompt
as `preRunToolResults`. Task MCP `logagent.run_domain_tool` reuses an existing
`toolId + actionId` result when present, so provider retries and user follow-up
calls do not duplicate the same result inside one run. Legacy Rust/V1
`{tool,inputFile}` calls use an `act_mcp_tool_<stable_digest>` action id, while
the V2 `toolId` protocol keeps the tool/input/params-derived action id.
Configured tool-derived action ids use the Rust/V1 `act_tool_<tool_id>` prefix;
input-file runs append a stable input hash.

Configured subprocess `result.json` uses the Rust/V1 `ToolRunRecord` shape:
`schemaVersion=2`, `tool`, `actionId`, `status`, `exitCode`, `durationMs`,
`command`, `inputFile`, `stdoutPath`, `stderrPath`, `summary`, `findings`, and
`error`. V2 also keeps `toolId`, `displayName`, `params`, `argv`,
`stdoutPreview`, `stderrPreview`, `parsedStdout`, `stdoutArtifactId`, and
`stderrArtifactId` for existing clients. The logical `stdoutPath` and
`stderrPath` point at Rust/V1-style `tool_results/<action_id>/stdout.txt` and
`stderr.txt`; the artifact IDs point at the actual V2 persisted evidence files.
Non-zero exits, timeouts, and spawn failures are recorded as `FAILED` or
`TIMED_OUT` tool results instead of failing the MCP call before an artifact is
written.

Manual tool runs use:

```http
POST /api/v2/tools/:tool_id/runs
```

with optional `workspaceId`, optional `uploadIds`, `params`, and a
Rust/V1-compatible ignored `idempotencyKey`. When `workspaceId` is omitted, V2
infers it from the first non-empty trimmed `uploadIds` entry, or creates a
short-lived `Manual tool run` workspace for zero-upload built-ins such as
metadata tools. They create `kind=tool_run` Run rows and DB-backed `tool_run`
jobs, so startup recovery and artifact/evidence tracking use the same SQLite
foundation as analysis runs. V2 create/list/get tool-run responses expose
Rust/V1 TaskSummary-compatible top-level fields including `taskId`,
`taskKind=tool_run`, `status`, `phase`, `toolId`, and `url`, while retaining
the raw V2 Run under `run` and list `rawRuns` for diagnostics.
Rust/V1-style `/api/tools...` aliases are also mounted for list, get, create,
list-runs, get-run, result, and artifacts. The legacy artifacts alias returns
the same result envelope as `/api/tools/runs/:run_id/result`, matching Rust/V1;
the `/api/v2/tools/runs/:run_id/artifacts` route keeps the V2 artifact-list
response.

configured subprocess descriptors use the Rust/V1 command shape:
`source=configured`, `backend=command`, `readOnly=false`, `editable=true`,
`exportable=enabled`, `minFiles=1`, and `acceptedSuffixes` copied from
`match.filePatterns`. Their `paramsSchema` also exposes Rust/V1 read-only
`configuredArgs` and `match` entries, with the same data mirrored under
`properties` for V2 clients that expect object-schema fields. Runtime params
for configured tools are validated against a practical JSON Schema subset:
`type`, `enum`, `oneOf` / `anyOf`, string length, numeric min/max, array
`items` / min/max items, and nested object `required` /
`additionalProperties=false`. V2 currently
includes manual built-ins for metadata tools, `logagent.preprocess_log_package`,
`logagent.fetch`, and default-off `logagent.huawei_cloud_package_sync`, plus the
V1-style configured command adapter `pprof_analyzer`.
Readonly MCP exposes these descriptors through `logagent.list_tools` and
`logagent://tools/catalog`, but it rejects direct `tools/call` execution for
catalog tools such as configured commands, preprocess, fetch, pprof, and Huawei
sync.
Metadata built-in descriptors use the Rust/V1 catalog shape with
`backend=builtin`, `read-only` / `manual-run` tags, and field/tag params
templates that include `retentionPolicy` where supported. Manual metadata
tool results preserve the V2 `value` field and also expose Rust/V1-compatible
`params`, `result`, `durationMs`, and `createdAt` fields. Their action ids use
the Rust/V1 `act_tool_metadata_<tool_id_sanitized>_<run_id>` prefix with
non-ASCII, dots, and other separators normalized to `_`.
Manual tool run creation validates both upload count and uploaded filenames
against the selected descriptor's `acceptedSuffixes`; `params.inputFiles` can
still select existing workspace inputs without attaching uploads.
`GET /api/v2/tools/runs/:run_id/result` returns HTTP 409 until the tool run
succeeds, with the current run status in the response detail. After success the
payload keeps the V2 `run/artifact/result` objects and also exposes
Rust/V1-compatible top-level `taskId`, `runId`, `toolId`, and `resultPath`
fields; `taskId` is the same value as `runId` in the V2 data model.
Huawei package sync matches the Rust/V1 catalog behavior by using the
`Huawei OBS + GaussDB Package Sync` display name, `huawei-cloud` tag,
`outputViews=["summary","obs","gaussdb","json"]`, accepting any single
completed upload (`acceptedSuffixes=["*"]`), and validating only the
single-upload count plus structured SQL/object-key params. Execution also
emits Rust/V1-style result fields (`tool`, `input`, `obs`, `gaussdb`, `sql`,
`timings`, `warnings`, `credentialMetadata`, and logical `evidenceRefs`) while
retaining V2 top-level `obsPut` / `gaussdbQuery` compatibility fields.
OBS object URLs use the Rust/V1 virtual-hosted bucket shape
`<scheme>://<bucket>.<endpoint-host>/<object-key>` and percent-encode object key
segments.
`credentialMetadata` keeps the V1 `gaussdbPasswordEnv` key as `null` because
V2 uses a single `LOGAGENT_V2_HUAWEI_GAUSSDB_DSN` environment value.
`pprof_analyzer` catalog metadata uses the Rust/V1 configured command shape
(`source=configured`, `backend=command`) while remaining manual-only in V2.
It is disabled by default unless `LOGAGENT_V2_PPROF_GO_COMMAND` or
`LOGAGENT_TOOL_PPROF_GO` is set, or `LOGAGENT_V2_PPROF_ENABLED=1` is used with
an absolute Go command path. Its `paramsSchema` exposes V1 top-level
`sampleIndex`, `nodeCount`, and `generateSvg` entries plus the V2 `properties`
mirror; `sampleIndex` is trimmed and must contain only letters, digits, `_`, or
`-`, while `generateSvg` must be a JSON boolean.
Its result JSON includes parsed `profileType`, `total`, top rows, `error`,
`durationMs`, `createdAt`, Rust/V1-style `artifacts` / `artifactPaths` for
`tool_results/<action_id>/{top.txt,tree.txt,raw.txt,stderr.txt,graph.svg}`, and
V2-only `artifactIds` for local artifact records.
The pprof action id follows Rust/V1 as `act_tool_pprof_analyzer_<run_id>`, so
manual run support artifact paths keep the same logical prefix.
The pprof subprocess argv matches Rust/V1: top/tree/svg use
`-nodecount=<nodeCount>`, and all pprof subcommands use `-symbolize=none`.

`GET /api/v2/tools`, readonly MCP `logagent://tools/catalog`, the retained
`logagent-v2://tools/catalog` alias, and `logagent.list_tools` expose the same
tool catalog envelope. All return `schemaVersion`, full `tools` descriptors,
and a V1-compatible `configuredTools` summary with configured args, timeout,
match rules, and `maxInputFiles`. They also return `sourceBuiltAnalyzers`,
which reports whether the four source-built analyzer IDs are registered,
enabled/runnable, disabled, missing, or unavailable because the configured
command is absent or not executable in this V2 process. The readonly MCP
endpoint never runs tools.

`GET /api/v2/exports/tools.zip` exports enabled configured subprocess tools
and the enabled `pprof_analyzer` Go executable.
The archive contains `README.md`, `tools-manifest.json`, executable files under
`bin/<toolId>/`, shell wrappers under `wrappers/`, and config examples under
`config/examples/`. Missing, relative, non-file, or non-executable tool
commands are kept in the manifest with `skipped=true`; disabled tools and
built-in tools without standalone executables are omitted. Generic subprocess
tool examples are `LOGAGENT_V2_TOOLS_JSON` snippets with absolute command path
placeholders; the `pprof_analyzer` example instead documents
`LOGAGENT_V2_PPROF_GO_COMMAND`, because V2 must invoke the packaged Go
executable as `go tool pprof` rather than as a raw subprocess tool. The
examples use absolute path placeholders because enabled V2 tool and pprof
configuration rejects relative executable paths. `tools-manifest.json` also
includes the same fixed `sourceBuiltAnalyzers` status list exposed by
`/api/v2/tools`, so exported runtime bundles can be audited for
`flux_query_analyzer`, `influxql_analyzer`, `opengemini_storage_analyzer`, and
`influxdb_storage_analyzer` registration or command availability without
starting the server. The export does not include API keys, endpoint
credentials, runtime environment values, uploads, artifacts, or workspace data.

Fetch endpoints are configured through the protected HTTP API or imported from
DevTools bash cURL commands using `POST /api/v2/fetch/imports/preview` and
`POST /api/v2/fetch/imports`. Supported cURL flags are limited to URL,
request method, headers, body, cookies, User-Agent, Referer, compression, HEAD,
and location; commands may include a leading `$` shell prompt from terminal
copy/paste. Rust/V1-style `/api/fetch...` aliases are mounted for import
preview, endpoint CRUD, endpoint run creation, and run listing; the V2-only
direct run-scoped execution endpoint remains `/api/v2/runs/:run_id/fetch/:endpoint_id`.
Execution is
disabled unless `LOGAGENT_V2_FETCH_ENABLED=1` and constrained to `http`/`https`
URLs whose host, host:port, or scheme-specific `http(s)://host[:port]` entry
matches `LOGAGENT_V2_FETCH_ALLOWED_HOSTS`. When Fetch is enabled the allowlist
must be non-empty; URL-form allowlist entries pin both scheme and port, using
the default port when omitted.
Endpoint records are migrated to `schemaVersion=2` and expose
`refreshPolicy.mode=manual_only`; V2 does not automatically refresh bearer
tokens, cookies, or API keys. Operators refresh credentials by updating the
endpoint or re-importing a cURL command, which rewrites the encrypted credential
set.
Fetch does not follow redirects by default; imported cURL commands with
`--location` or endpoints created with `followRedirects=true` opt into bounded
manual redirects.
Runtime calls accept either `endpointId` or the V1-compatible `fetchId`,
optional string `variables` that replace `{name}` placeholders in the endpoint
URL before allowlist validation, optional temporary string `headers`, and an
optional string `body` override. Controlled headers such as `Host` and
`Content-Length` are rejected for both saved endpoints and runtime overrides.
The `/api/v2/tools` catalog descriptor keeps the Rust/V1 manual-run shape:
`readOnly=false`, `paramsTemplate.fetchId`, `body=null`, and
`outputViews=["summary","request","response","body_artifact"]`.
Task MCP `logagent.list_fetch_endpoints` matches the Rust/V1 envelope with
`schemaVersion=1`, enabled endpoint summaries, and endpoint-level
`schemaVersion=2` / `refreshPolicy` fields, plus
`finalEvidenceAllowed=false`; when Fetch execution is disabled it returns a
JSON-RPC error instead of listing endpoints.
`GET /api/v2/fetch/runs` lists persisted Fetch tool runs, filtered by
`endpointId`, `fetchId`, V1-style `fetch_id`, or `workspaceId`, without
executing network requests. `POST /api/v2/fetch/endpoints/:endpoint_id/runs`
queues a Fetch `tool_run`; callers may provide `workspaceId`, otherwise V2
creates an isolated workspace for the run. This endpoint-run path updates the
endpoint summary with `lastRunId` and Rust/V1-compatible `lastRunTaskId`.
Task MCP `logagent.fetch` uses a deterministic Rust/V1-style
`act_fetch_<digest>` action id derived from normalized Fetch params, so repeated
same-parameter calls produce the same logical `result.json#response` evidence
ref. Its response keeps the V2 `result` / `artifact` / `evidence` objects and
also exposes the Rust/V1 top-level `artifactPath`, `statusCode`, `httpOk`,
`bodyPreview`, and `evidenceRefs` fields. Queued API/manual Fetch `tool_run`
executions use the Rust/V1 `act_fetch_<run_id>` action id.
Saved endpoint bodies and runtime body overrides are rejected before the HTTP
request when their UTF-8 byte size exceeds
`LOGAGENT_V2_FETCH_MAX_REQUEST_BYTES`.
Request URLs, sensitive headers, and sensitive JSON/form-style body preview
fields are redacted as `<redacted>` in API, MCP, and artifact previews.
Inside URL query strings and form-style body previews the marker is URL-encoded
as `%3Credacted%3E`.
Redirects are followed manually up to
`LOGAGENT_V2_FETCH_MAX_REDIRECTS`; every hop is revalidated against the same
allowlist, and sensitive headers are stripped when redirecting across origin.
Fetch stores bounded response previews as `fetch_result` evidence and stores
the bounded raw response body as a separate body artifact. Results include the
Rust/V1 `schemaVersion=3` tool result envelope with `exitCode=null`,
`command=[]`, `inputFile=null`, empty stdout/stderr logical paths,
`findings=[]`, and `evidenceRefs=["tool_results/<action_id>/result.json#response"]`.
They also include the logical V1-style
`tool_results/<action_id>/response_body.bin` path plus the actual V2 artifact id
and relative path.

Sensitive Fetch endpoint material is split into an encrypted credential set.
When Fetch execution is enabled, `LOGAGENT_V2_FETCH_SECRET_KEY` is validated at
settings load time as a Fernet 32-byte base64 key.
If a URL query parameter, header, or body field looks like a token, secret,
password, API key, session, Authorization, or Cookie, V2 stores only a redacted
endpoint definition in `fetch_endpoints` and encrypts the full request material
in `fetch_credential_sets` using `LOGAGENT_V2_FETCH_SECRET_KEY`. Creating or
updating a sensitive endpoint without a valid key is rejected before the
endpoint row is written. `PATCH /api/v2/fetch/endpoints/:endpoint_id` rewrites
the redacted endpoint row and refreshes the encrypted credential set from the
merged endpoint definition. Execution hydrates the endpoint from the credential
set, while API, MCP, and result artifacts continue to show only `<redacted>`
values; URL query strings and form-style body previews percent-encode that
marker.

`request_user_input` and `request_approval` persist pending `actions`, write
`mcp_waiting_request.json`, and move the run into `waiting_for_user` or
`waiting_for_approval`. The task MCP response includes the V2 `action` plus
Rust/V1 `artifactPath`, `runtimeStatus`, and `evidenceRefs`. Posting a message
to a waiting run marks pending user-input actions as `answered` and requeues
the run through the SQLite job queue. Message retries with the same
`idempotencyKey` return the original timeline event without answering actions
or enqueueing another job; optional `questionId` must match a pending
`user_input` action id or payload question id. Approving/rejecting requires a
`waiting_for_approval` run and pending approval action. Approval retries with
the same `idempotencyKey` return the original timeline event without recording
another decision or enqueueing another job. The next Agent request
carries recent user messages, action results, and remaining pending actions in
`interactionContext`. The OpenAI-compatible and binary Agent provider loop also
advertises these waiting tools during normal analysis; when a provider requests
  one, the current provider response is recorded as `paused`, `analysis_state`
  records the waiting status, the run keeps its waiting state, and no final
  result is written until the user resumes it. If the user resumes with
  `resumeMode=finalize`, the next provider prompt carries
  `resumePolicy.finalizeWithCurrentEvidence=true` and no longer advertises
  waiting/approval tools. The approval decision body may include an `input`
  object; for approved actions V2 writes it back to the action payload before
  executing approval side effects. When an approved action payload has
  `actionType=collect_environment`, V2 first merges `input`, top-level target
  fields, and `environmentInput` / `remoteInput`, then checks either
  `executorId` plus exactly one of `commandId` / `fileId`, or a batch
  `targets[]` array. If the merged single-target input names only `commandId`
  or `fileId` and there is exactly one enabled executor, V2 infers that
  executor. Each batch target must name an enabled executor, inherit one from
  the parent input, or be resolvable through the same single-executor rule; up
  to 20 targets are accepted. A valid command target queues a
  `remote_command_run` using the whitelisted command template. A valid file
  target queues the same DB-backed remote job with `operation=file_collection`
  and uses the whitelisted
  `LOGAGENT_V2_REMOTE_FILES_JSON` template plus `LOGAGENT_V2_REMOTE_SCP_COMMAND`
  to fetch one bounded file. The analysis run remains waiting while collection
  runs. Single-target collection writes
  `environment_evidence/<action_id>/result.json` when that remote run finishes;
  batch collection uses idempotency keys `environment:<action_id>:<index>` and
  writes one aggregate result only after all targets reach a terminal state.
  Aggregate status is `COLLECTED`, `PARTIALLY_COLLECTED`, or `REMOTE_FAILED`.
  Command collection exposes `remote_result.json`, `stdout.txt`, and
  `stderr.txt`; file collection also exposes `collected_file.bin`. Batch support
  artifacts use logical paths under
  `environment_evidence/<action_id>/targets/<index>/...` before requeueing the
  analysis run.
Invalid remote targets produce `REMOTE_REJECTED` background evidence instead of
leaving the approved action half-applied. If no remote target is supplied, V2
preserves the V1-compatible MOCK evidence path. Environment evidence is exposed
through `/analysis` resources and task MCP `environment_evidence`, included in
the next `analysis_package` and Agent prompt, and remains background-only
rather than a final evidence ref. The copied remote output files appear under
`supportArtifacts` and task MCP `artifact_index` with `source="support"`.

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
canonical exception for recalled Case background context. V1 legacy grep refs
are accepted and normalized before validation: `matches/<index>`,
`matches/<start>-<end>`, `#<start>-#<end>`, and bare line numbers or line
ranges that map to initial `grep_results.json` matches.

```text
session_text_input.json#question
grep_results.json#matches/<index>
log_searches/<search_id>.json#matches/<index>
log_slices/<slice_id>.json#lines
case_context.json#cases/<index>
tool_results/<tool_id>/result.json#findings/<index>
tool_results/<fetch_action_id>/result.json#response
code_evidence/<action_id>.json#matches/<index>
code_evidence/<action_id>.json#diffs/<index>
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
GET  /api/v2/metadata/imports/:import_id/preview
GET  /api/v2/metadata/imports/:import_id
POST /api/v2/metadata/imports/:import_id/confirm
```

Preview parses and normalizes content, stores a draft with status `previewed`,
and returns node/database counts without changing `metadata_instances`.
`GET /api/v2/metadata/imports/:import_id/preview` returns only the lightweight
preview summary, while `GET /api/v2/metadata/imports/:import_id` also includes
the normalized snapshot for inspection. Confirm upserts the normalized snapshot
and marks the draft `confirmed`.

`templateType=csv` is dependency-free and intended for small-team maintained
tables. A `section` column may be set to `instance`, `node`, `database`,
`retention_policy`, `measurement`, `field`, or `partition_view`; when omitted,
V2 infers common node/database/measurement/field rows from column names. Common
columns include `clusterId`, `product`, `version`, `environment`, `nodeId`,
`host`, `role`, `database`, `defaultRetentionPolicy`, `retentionPolicy`,
`measurement`, `field`, `typ`, and `endTime`. Field `typ` accepts openGemini
type codes or labels such as `tag`, `float`, `string`, and `boolean`.

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
`field` filters are trimmed; a blank string is treated as omitted, array entries
must be non-empty after trim, and `logagent.get_metadata_tag_fields` rejects
`field`.
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
the Workspace question/mode and includes up to three matched outlines. Task MCP
resource `logagent://task/<run_id>/metadata_context` returns the same bounded
`metadata_context_outline` used by `analysis_package.json`; the
`logagent-v2://run/<run_id>/metadata_context` alias is retained. The full
`metadata_context.json` artifact remains available through run artifact APIs for
WebUI/compatibility, while detailed snapshot and field data remain available
through Metadata MCP tools.

## Case Memory

V2 stores confirmed cases in SQLite table `cases` using Case schema v2, and
keeps Rust/V1-compatible JSON files under `LOGAGENT_V2_DATA_DIR/cases/` as a
local migration and rollback layer:

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
confirmed through `POST /api/v2/runs/:run_id/case` or the Rust/V1-style task
alias `POST /api/v2/tasks/:task_id/case`; repeated confirmation of the same run
returns the existing task case. Cases can be searched with keyword
queries, read by ID, edited, or disabled. Query search uses local SQLite
FTS5/BM25 over title, symptom, root cause, solution, product/version/
environment, instance/node, and evidence refs, plus a dependency-light local
hash-vector recall column stored in SQLite. Disabled cases are hidden unless
the caller sets `includeDisabled=true`.

On startup, V2 imports `cases/*.json` schema v2 files by `caseId` using
idempotent upsert semantics. Creating or editing a Case first updates SQLite,
FTS, and the local vector column, then atomically writes
`cases/<caseId>.json`.

Case import drafts support text or JSON capture before a Case is confirmed:

```text
POST /api/v2/cases/imports
POST /api/v2/cases/imports/preview
GET  /api/v2/cases/imports
GET  /api/v2/cases/imports/:import_id
POST /api/v2/cases/imports/:import_id/messages
PATCH /api/v2/cases/imports/:import_id
POST /api/v2/cases/imports/:import_id/confirm
```

`POST /api/v2/cases/imports` is the Rust/V1-style create endpoint. It accepts
JSON bodies with either V2 `content` or V1 `text`, multipart `text`/`content`
fields, or one UTF-8 text file field named `file`. Text-file imports allow
`.txt`, `.text`, `.md`, `.markdown`, `.log`, `.json`, `.yaml`, `.yml`, and
`.csv` filenames, or text/json/yaml content types. Optional JSON or multipart
filenames are normalized to a safe basename, and basenames resolving to `.` or
`..` are rejected with HTTP 400 before a draft is persisted. The endpoint
persists the same draft as preview, returns HTTP 201, and includes both V2
`import` and Rust/V1-style `draft` aliases.

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

Readonly MCP exposes `logagent://cases/recent`, `logagent.search_cases`, and
`logagent.get_case`. `logagent://cases/recent` returns the Rust/V1 default of
20 recent enabled Cases. `logagent.search_cases` keeps the Rust/V1 readonly
default of 5 and limit range of 1..50.
`logagent.recall_cases` is task-MCP-only, keeps the Rust V1 name for enabled
Case recall, defaults to 5, and clamps limit to 1..20. Task MCP Case calls
persist `case_context` evidence as background context with `final_allowed=false`.
Historical cases are references for investigation and do not replace current-task
evidence.

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
does not create a run or write `system_context.json`. Skill preview `skillIds`
are trimmed, empty entries are ignored, duplicates are collapsed, invalid ids
return HTTP 400 or a JSON-RPC error, and at most 32 explicit ids are accepted.

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
