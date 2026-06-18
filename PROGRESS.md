# Development Progress

Last updated: 2026-06-18

## 2026-06-18 V2 Manifest Source URL and Tool Input Generator Parity

- Added Rust/V1-compatible `sourceUrl` to V2 `manifest.json` artifacts for both
  Log Analysis runs and manual `logagent.preprocess_log_package` tool runs.
- Aligned `tool_inputs/index.json` `generatedBy` with Rust/V1
  `log_package_preprocessor` instead of the V2-internal materializer name.
- Extended regression coverage for Session manifest source URL, manual
  preprocess manifest source URL, and persisted tool input index generator.
- Updated V2 Server and Log Analyzer docs.
- Verification passed: focused sourceUrl/tool-input pytest selection,
  `cd server-v2 && PYTHONPATH=. uv run --extra dev pytest` with 158 passed and
  1 Starlette warning, `cd server-v2 && PYTHONPATH=. uv run --extra dev ruff
  check logagent_v2 tests`, and `git diff --check`.

## 2026-06-18 V2 Log Slice Artifact Alias Parity

- Added Rust/V1-compatible `sourcePath` and `lines[].line` aliases directly to
  V2 `log_slices/*.json` artifacts.
- Preserved the original selector as `requestedPath` when a legacy bare filename
  or archive-member selector resolves to a canonical `extracted/...` path.
- Extended task MCP `logagent.get_log_slice` regression coverage for the
  persisted slice payload aliases.
- Updated V2 Server and Log Analyzer docs.
- Verification passed: focused log-slice pytest selection,
  `cd server-v2 && uv run --extra dev ruff check logagent_v2 tests`, full
  `cd server-v2 && PYTHONPATH=. uv run --extra dev pytest` with 158 passed and
  1 Starlette warning, and `git diff --check`.

## 2026-06-18 V2 Grep Match Limit Default Parity

- Aligned V2 `max_grep_matches` / `LOGAGENT_V2_MAX_GREP_MATCHES` default with
  Rust/V1 `log_analyzer.max_matches=200`.
- Extended Settings regression coverage to assert both the default and the
  environment override behavior.
- Updated V2 Server, Log Analyzer, and Config docs.
- Verification passed: focused Settings pytest selection,
  `cd server-v2 && uv run --extra dev ruff check logagent_v2 tests`, full
  `cd server-v2 && PYTHONPATH=. uv run --extra dev pytest` with 158 passed and
  1 Starlette warning, and `git diff --check`.

## 2026-06-18 V2 Configured Grep Keyword Parity

- Added `Settings.grep_keywords` and `LOGAGENT_V2_GREP_KEYWORDS` for
  configuration-driven initial grep keywords.
- Changed V2 initial `grep_results.json` generation to use configured keywords
  instead of automatically tokenizing the user question, aligning the default
  behavior with Rust/V1 `log_analyzer.keywords`.
- The V2 default keyword set is now
  `error,exception,timeout,fail,failed,panic,fatal,refused,denied,verify`.
- Updated Tool Runner fallback regression coverage to configure `select`
  explicitly when it relies on initial grep keyword fallback.
- Updated root, V2 Server, Log Analyzer, and Config docs.
- Verification passed: focused grep keyword pytest selection,
  `cd server-v2 && uv run --extra dev ruff check logagent_v2 tests`, full
  `cd server-v2 && PYTHONPATH=. uv run --extra dev pytest` with 158 passed and
  1 Starlette warning, and `git diff --check`.

## 2026-06-18 V2 Grep Match Alias Parity

- Aligned V2 initial `grep_results.json` and follow-up `log_searches` match
  records with Rust/V1 by writing `file`, `line`, and `evidenceRef` aliases
  alongside existing `path`, `lineNumber`, and `ref` fields.
- Kept task MCP `logagent.search_logs` response behavior unchanged; wrappers
  now expose aliases that are already present in the underlying artifacts.
- Added regression coverage against the persisted grep artifact for the alias
  fields.
- Updated V2 Server and Log Analyzer docs.
- Verification passed: focused grep/path pytest selection,
  `cd server-v2 && uv run --extra dev ruff check logagent_v2 tests`, full
  `cd server-v2 && PYTHONPATH=. uv run --extra dev pytest` with 157 passed and
  1 Starlette warning, and `git diff --check`.

## 2026-06-18 V2 Multi-Upload Manifest Path Parity

- Aligned ordinary V2 upload logical paths with Rust/V1 per-upload extracted
  directory semantics.
- Plain text uploads now appear as `extracted/<uploadDir>/<filename>` and
  ordinary archive members appear as `extracted/<uploadDir>/<member>` in
  manifest, grep results, log slices, generic query tool inputs, and Tool
  Runner fallback selectors.
- Repeated upload directory names now receive stable `_2`, `_3`, ... suffixes
  in upload order, preventing duplicate `app.log` paths from becoming
  ambiguous across multiple uploads.
- Legacy bare filename and original archive-member selectors still resolve
  when they match exactly one current Workspace text file, while returned
  results expose the canonical `extracted/<uploadDir>/...` path.
- Added regression coverage for duplicate plain uploads and precise
  `logagent.get_log_slice` selection against the second upload.
- Updated root, V2 Server, and Log Analyzer docs.
- Verification passed: focused path/selector pytest selection,
  `cd server-v2 && uv run --extra dev ruff check logagent_v2 tests`, full
  `cd server-v2 && PYTHONPATH=. uv run --extra dev pytest` with 157 passed and
  1 Starlette warning, and `git diff --check`.

## 2026-06-18 V2 Manifest Upload Summary Parity

- Aligned V2 manifest artifacts with Rust/V1 node-package summary shape while
  preserving existing V2 fields.
- Manifest top-level data now includes V1 aliases such as `taskId`,
  `uploadId`, `uploadIds`, `source`, and `filename`; `uploads[]` now includes
  `size`, `rawPath`, `extractedDir`, package metadata, `nodeDir`, sorted
  `logGroups`, ignored file counts/samples, and warnings when present.
- Manifest file entries now expose V1 aliases and package metadata including
  `size`, `uploadId`, `instanceId`, `nodeId`, `packageTimestamp`,
  `compressed`, and `compression`; gzip compression is detected by magic bytes
  so rotated files without `.gz` suffix still get counted.
- Updated root, V2 Server, and Log Analyzer docs.
- Verification passed: focused node-package/preprocess/tool-input pytest
  selection, `cd server-v2 && uv run --extra dev ruff check logagent_v2
  tests`, full `cd server-v2 && PYTHONPATH=. uv run --extra dev pytest`
  with 156 passed and 1 Starlette warning, and `git diff --check`.

## 2026-06-18 V2 InfluxQL Query Extraction Parity

- Aligned V2 InfluxQL query extraction with Rust/V1 by recognizing JSON and
  `key=value` fields named `query`, `sql`, `stmt`, or `statement`.
- Added V1-compatible quoted key-value parsing, escaped quote handling, query
  cleanup, and `grant` / `revoke` statement detection.
- Extended materialized tool input coverage so generic InfluxQL input records
  include both JSON query lines and `stmt=...` log lines.
- Updated V2 Server and Log Analyzer docs.
- Verification passed: focused InfluxQL extraction/materialization pytest
  selection, `cd server-v2 && PYTHONPATH=. uv run --extra dev ruff check
  logagent_v2 tests`, and full `cd server-v2 && PYTHONPATH=. uv run --extra
  dev pytest` with 156 passed and 1 Starlette warning.

## 2026-06-18 V2 Node Package Filename Parity

- Aligned V2 node log package parsing with Rust/V1 filename semantics for
  `<packageId>_<instanceId>_<nodeId>_<yyyy_MM_dd_HH_mm_ss_micros>_logs.tar.gz`.
- The parser now validates package/instance/node ids with the V1 ASCII
  alphanumeric rule and validates timestamp segment widths before entering the
  dedicated node-package preprocessing path; `.tgz` remains accepted by V2.
- Added regression coverage for direct parsing plus full node-package
  extraction/manifest generation using a V1 timestamped filename.
- Updated root, V2 Server, and Log Analyzer docs.
- Verification passed: focused node-package pytest selection,
  `cd server-v2 && PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, and full `cd server-v2 && PYTHONPATH=. uv run --extra dev pytest`
  with 155 passed and 1 Starlette warning.

## 2026-06-18 V2 InfluxQL Tool Input Record Parity

- Aligned V2 node-package `influxql_analyzer` JSONL records with Rust/V1 by
  adding `line` and `logGroup` while keeping the existing additive
  `lineNumber` alias.
- Added regression coverage that reads the materialized analyzer JSONL artifact
  and validates the compatibility fields.
- Updated V2 Server and Log Analyzer SPEC docs.
- Verification passed: focused materialized tool input pytest selection,
  `cd server-v2 && PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, and full `cd server-v2 && PYTHONPATH=. uv run --extra dev pytest`
  with 154 passed and 1 Starlette warning.

## 2026-06-18 V2 Log Text Tool Input Parity

- Added V2 materialization for Rust/V1-style node-package `log_text_jsonl`
  inputs under `tool_inputs/log_text/<node>/<timestamp>/<logGroup>.jsonl`.
- The generated entries use `scope=log_group`, omit `toolIds`, preserve
  per-line node/package/source metadata, and remain explicit-use inputs rather
  than auto-selected configured analyzer inputs.
- Updated preprocess and configured Tool Runner regression coverage so
  `logagent.preprocess_log_package` reports the extra V1-compatible inputs
  while `influxql_analyzer` still consumes only matching `toolIds` entries.
- Updated root, V2 Server, Log Analyzer, and Tool Runner docs.
- Verification passed: focused node-package preprocess/tool-input pytest
  selection, `cd server-v2 && PYTHONPATH=. uv run --extra dev ruff check
  logagent_v2 tests`, and full `cd server-v2 && PYTHONPATH=. uv run --extra
  dev pytest` with 154 passed and 1 Starlette warning.

## 2026-06-18 V2 Readonly Case Recent Limit Parity

- Aligned V2 readonly MCP `logagent://cases/recent` with the Rust/V1 default
  of 20 recent enabled Cases.
- Extended the Case MCP limit regression to cover readonly resource reads in
  addition to `logagent.search_cases` and task MCP `logagent.recall_cases`.
- Updated V2 Server and Case Store README/SPEC docs.
- Verification passed: focused Case MCP/resource limit pytest selection,
  `cd server-v2 && PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, full `cd server-v2 && PYTHONPATH=. uv run --extra dev pytest` with
  154 passed and 1 Starlette warning, and `git diff --check`.

## 2026-06-18 V2 openGemini Remote Templates

- Added V2 built-in Remote Executor templates for openGemini process-name
  snapshots and common configuration, log, and data directory candidates.
- Kept the new templates as fixed read-only argv entries without shell pipes,
  redirects, or user-provided command parameters.
- Extended default remote template regression coverage to assert the new IDs
  and key argv/path values.
- Updated root, V2 Server, Environment Collector, Analysis Agent, Config, and
  deploy environment docs.
- Verification passed: focused default remote command template pytest selection,
  `cd server-v2 && PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, full `cd server-v2 && PYTHONPATH=. uv run --extra dev pytest` with
  154 passed and 1 Starlette warning, and `git diff --check`.

## 2026-06-18 V2 Case MCP Limit Parity

- Aligned V2 readonly MCP `logagent.search_cases` with the Rust/V1 readonly
  limit range of 1..50.
- Kept task MCP `logagent.recall_cases` bounded to 1..20 and added explicit
  integer validation before Store search.
- Added regression coverage for readonly tool schema, readonly search clamping,
  task recall clamping, and invalid limit rejection.
- Updated V2 Server and Case Store README/SPEC docs.
- Verification passed: `cd server-v2 && PYTHONPATH=. uv run --extra dev ruff
  check logagent_v2 tests`, focused Case MCP limit pytest selection, full
  `cd server-v2 && PYTHONPATH=. uv run --extra dev pytest` with 154 passed and
  1 Starlette warning.

## 2026-06-18 V2 Fetch Endpoint Schema Policy

- Added Fetch endpoint `schemaVersion=2` and persisted `refreshPolicy` storage.
- Existing endpoint rows migrate through new SQLite columns with default
  `manual_only` refresh policy; direct Store-created endpoints also default to
  schema v2.
- Fetch currently rejects automatic token refresh policy modes. Credential
  refresh remains an explicit endpoint PATCH or cURL re-import operation.
- Public endpoint summaries, task MCP endpoint summaries, and hydrated
  credential paths now preserve the manual-only refresh policy.
- Added regression coverage for schema v2 defaults, legacy-row migration,
  unsupported automatic refresh policy rejection, and API response visibility.
- Updated root, V2 Server, and Tool Runner README/SPEC docs.
- Verification passed: `cd server-v2 && PYTHONPATH=. uv run --extra dev ruff
  check logagent_v2 tests`, focused Fetch schema/PATCH pytest selection, full
  `cd server-v2 && PYTHONPATH=. uv run --extra dev pytest` with 153 passed and
  1 Starlette warning.

## 2026-06-18 V2 Environment Default Templates

- Expanded V2 default Remote Executor command templates beyond `smoke_ls_root`
  to include read-only `system_uname`, `uptime_load`, `disk_usage`,
  `memory_usage`, `process_overview`, and `network_listeners`.
- The templates remain fixed argv, non-shell, approval-gated remote commands;
  setting `LOGAGENT_V2_REMOTE_COMMANDS_JSON` still replaces the built-in list.
- Added regression coverage for the default template IDs, key argv values, and
  `parse_remote_commands_env(None)` fallback behavior.
- Updated root, V2 Server, Environment Collector, Analysis Agent, Config,
  Roadmap, and deploy environment docs.
- Verification passed: focused remote command template pytest selection,
  `PYTHONPATH=. uv run --extra dev ruff check logagent_v2 tests`, full
  `PYTHONPATH=. uv run --extra dev pytest` (152 passed, 1 warning), and
  `git diff --check`.

## 2026-06-18 V2 Agent Approval Budget

- Added `LOGAGENT_V2_AGENT_MAX_APPROVALS` / `agent_max_approvals` with default
  3 and the same non-positive clamp behavior as the other Agent budgets.
- V2 Agent runtime now counts persisted `request_approval` actions before each
  provider round; after the configured approval budget is reached, resumed
  analysis produces a `budgetLimited=true` low-confidence final answer and
  succeeds instead of asking the provider for another round.
- Settings summaries and Agent backend diagnostics now expose the approval
  budget alongside rounds, LLM calls, actions, repeated fingerprints, tokens,
  runtime seconds, and user prompt budgets.
- Added regression coverage for approval budget exhaustion after an approved
  waiting state resumes, plus Settings budget visibility.
- Updated root, V2 Server, Analysis Agent, and Config README/SPEC docs.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, focused budget/settings pytest selection, full `PYTHONPATH=. uv run
  --extra dev pytest` (151 passed, 1 warning), and `git diff --check`.

## 2026-06-18 V2 Code Evidence Orphan Worktree Scan

- Code Evidence search worktree preparation now scans the configured
  per-product `wt_*` cache directories for invalid or unregistered orphan
  worktrees.
- The scan is record-only in this slice: orphan details are written under
  `worktree.cleanup.orphanScan` and are not automatically deleted.
- Added regression coverage that creates a stale non-worktree cache directory
  and verifies the resulting orphan scan payload.
- Updated root and Code Evidence README/SPEC docs.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, focused Code Evidence pytest selection, full `PYTHONPATH=. uv run
  --extra dev pytest` (150 passed, 1 warning), and `git diff --check`.

## 2026-06-18 V2 Agent Token Runtime Prompt Budgets

- Added V2 Agent budget settings for cumulative provider usage tokens,
  per-invocation runtime seconds, and persisted user-input prompt count:
  `LOGAGENT_V2_AGENT_MAX_TOTAL_TOKENS`,
  `LOGAGENT_V2_AGENT_MAX_RUNTIME_SECONDS`, and
  `LOGAGENT_V2_AGENT_MAX_USER_PROMPTS`.
- Provider response usage is now recorded on `analysis_state.json` rounds as
  `tokenUsage`; when token, runtime, or user prompt budgets are exhausted,
  V2 reuses the existing guarded `budgetLimited=true` low-confidence final
  answer path and marks the run `succeeded`.
- Settings summaries and Agent backend diagnostics now expose all active
  budget values.
- Added regression coverage for token budget exhaustion, user prompt budget
  exhaustion after resume, and settings budget visibility.
- Updated root, V2 Server, Analysis Agent, and Config README/SPEC docs.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, focused budget/settings pytest selection, full `PYTHONPATH=. uv run
  --extra dev pytest` (150 passed, 1 warning), and `git diff --check`.

## 2026-06-18 V2 Environment Collector Hint Selection

- Approved V2 `collect_environment` inputs can now resolve multi-executor and
  multi-template targets from deterministic hints such as `target`, `executor`,
  `node`, `host`, `template`, `command`, and `file`.
- Hint selection only schedules SSH/SCP when exactly one enabled executor and
  exactly one enabled command/file template match; missing or ambiguous matches
  write `REMOTE_REJECTED` evidence and create no remote run.
- `analysis_package.environmentCollection` now advertises the
  `required_or_unique_hint` executor selection rule and a hinted approval input
  shape for Agent-generated approval requests.
- Added Environment Collector regression coverage for successful hinted target
  collection and ambiguous hint rejection.
- Updated root, V2 Server, Environment Collector, Analysis Agent, and Roadmap
  README/SPEC docs.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, focused `collect_environment` / `environment_collection` pytest
  selection, full `PYTHONPATH=. uv run --extra dev pytest` (148 passed, 1
  warning), and `git diff --check`.

## 2026-06-18 Analysis Agent Environment Batch Documentation

- Updated Analysis Agent README/SPEC status to reflect the already implemented
  V2 `collect_environment` batch remote target path.
- Clarified that the remaining gap is semantic auto-selection of
  executor/template across multiple executors and more built-in environment
  templates, not the batch execution/evidence mechanism itself.

## 2026-06-18 V2 Analysis Result Availability Status

- `/api/v2/runs/:run_id/result` now returns HTTP 409 with the current run
  status until a final answer/result artifact exists, instead of treating a
  queued run as a missing result.
- Added route regression coverage for queued analysis runs.
- Updated V2 Server and Interfaces specs.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check
  logagent_v2/api.py tests/test_store.py`, focused run-result route pytest
  selection, full `PYTHONPATH=. uv run --extra dev pytest` (146 passed, 1
  warning), and `git diff --check`.

## 2026-06-18 V2 Tool Run Result Compatibility

- `/api/v2/tools/runs/:run_id/result` now returns HTTP 409 with the current
  tool-run status until a result artifact exists, matching the Rust/V1
  "available after success" behavior.
- Successful result payloads now keep the V2 `run`, `artifact`, and `result`
  objects while adding Rust/V1-compatible top-level `runId`, `toolId`, and
  `resultPath` fields.
- Added route regression coverage for queued and succeeded manual metadata
  tool runs.
- Updated V2 Server and Tool Runner README/SPEC docs.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, focused tool-run result pytest selection, full `PYTHONPATH=. uv run
  --extra dev pytest` (145 passed, 1 warning), and `git diff --check`.

## 2026-06-18 V2 Settings Agent Budget Visibility

- `/api/v2/settings/llm` now returns the active Agent budgets:
  `maxRounds`, `maxLlmCalls`, `maxActions`, and
  `maxRepeatedActionFingerprints`.
- `/api/v2/settings/agent-backends` now includes the same budget summary on
  the `logagent_v2_agent` backend, and the backend diagnostic text reports all
  four budget values.
- Updated V2 Server README/SPEC docs.
- Verification passed: focused settings pytest selection, `PYTHONPATH=. uv run
  --extra dev ruff check logagent_v2/settings_api.py tests/test_store.py`, full
  `PYTHONPATH=. uv run --extra dev pytest` (144 passed, 1 warning), and
  `git diff --check`.

## 2026-06-18 V2 Agent Repeated Action Fingerprint Guard

- Added `LOGAGENT_V2_AGENT_MAX_REPEATED_ACTION_FINGERPRINTS` with default 1,
  matching the Rust/V1 repeated action fingerprint guardrail.
- V2 Agent provider tool calls now compute stable task MCP fingerprints from
  tool name and normalized arguments. If a requested fingerprint has already
  succeeded the configured number of times, V2 skips duplicate execution,
  returns a `budgetLimited=true` low-confidence final answer, records the round
  as `budget_limited`, and still finalizes the run as `succeeded`.
- Added regression coverage proving repeated provider requests only execute the
  first MCP tool call.
- Updated root, V2 Server, and Analysis Agent README/SPEC docs.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, focused repeated-action budget pytest selection, full `PYTHONPATH=.
  uv run --extra dev pytest` (144 passed, 1 warning), and `git diff --check`.

## 2026-06-18 V2 Configured Tool Params Schema Validation

- V2 configured subprocess tool params now validate a practical Rust/V1-style
  JSON Schema subset before execution: `type`, `enum`, `oneOf` / `anyOf`,
  string length, numeric min/max, array `items` / min/max items, and nested
  object `required` / `additionalProperties=false`.
- Added regression coverage for string-or-array `oneOf`, array item validation,
  numeric bounds, enum rejection, nested required fields, and nested unknown
  field rejection.
- Updated V2 Server and Tool Runner README/SPEC docs.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, focused configured tool registry pytest selection, full
  `PYTHONPATH=. uv run --extra dev pytest` (143 passed, 1 warning), and
  `git diff --check`.

## 2026-06-18 V2 Agent Budget-Limited Termination

- V2 Agent runtime now matches the Rust/V1 core budget defaults for provider
  rounds, LLM calls, and provider-directed actions: 4 / 4 / 6.
- Added `LOGAGENT_V2_AGENT_MAX_LLM_CALLS` and
  `LOGAGENT_V2_AGENT_MAX_ACTIONS` alongside
  `LOGAGENT_V2_AGENT_MAX_ROUNDS`; non-positive values are clamped to 1.
- Budget exhaustion no longer fails a run. The LangGraph `prepare_agent_request`
  node routes to an internal `budget_guard` response, validates a
  low-confidence final answer with `budgetLimited=true` and
  `terminationReason`, records the last analysis round as `budget_limited`, and
  finalizes the run as `succeeded`.
- Provider-directed task MCP calls are capped by the remaining action budget;
  excess requested tool calls are not executed.
- Updated root, V2 Server, and Analysis Agent README/SPEC docs.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, focused Agent budget/tool-loop pytest selection, full `PYTHONPATH=.
  uv run --extra dev pytest` (143 passed, 1 warning), and `git diff --check`.

## 2026-06-18 V2 Readonly MCP Tool Execution Boundary

- Readonly MCP now returns an explicit error when `tools/call` targets a tool
  catalog entry such as configured subprocess tools, preprocess, Fetch, Huawei
  package sync, or pprof. Discovery remains available through
  `logagent.list_tools` and `logagent://tools/catalog`.
- Added regression coverage to lock the readonly boundary for configured and
  manual built-in catalog tools while leaving readonly metadata tools on their
  existing path.
- Updated V2 Server and Tool Runner README/SPEC docs.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, focused readonly/tool catalog pytest selection, full `PYTHONPATH=.
  uv run --extra dev pytest` (141 passed, 1 warning), and `git diff --check`.

## 2026-06-18 V2 Code Evidence Diff Tool

- Added task MCP / Agent provider `logagent.diff_code` for configured local
  git repositories. It compares controlled base/target versions or refs with
  read-only `git diff --numstat` under configured `searchRoots`.
- Diff artifacts are persisted as `code_evidence/<action_id>.json` with
  base/target refs, commits, file-level added/deleted/binary summaries, and
  accurate `truncated` status plus final-answer refs
  `code_evidence/<action_id>.json#diffs/<index>`.
- Final answer validation and analysis package evidence summaries now accept
  Code Evidence `#diffs/<index>` refs in addition to existing `#matches/<index>`
  refs.
- Metadata-bound runs continue to enforce product/version context: diff target
  inherits the bound version when omitted, while diff base may point at another
  configured version/ref for regression comparison.
- Updated root, V2 Server, and Code Evidence README/SPEC docs.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, focused Code Evidence pytest selection, full `PYTHONPATH=. uv run
  --extra dev pytest` (141 passed, 1 warning), and `git diff --check`.

## 2026-06-18 V2 Code Evidence Worktree LRU

- Added `LOGAGENT_V2_CODE_WORKTREE_MAX_PER_REPO` / `code_worktree_max_per_repo`
  for V2 Code Evidence detached worktree cache retention; default is 5 and
  non-positive values are clamped to 1.
- `logagent.search_code` now touches the selected detached worktree as the
  usage marker and prunes least-recently-used same-product `wt_*` worktrees
  after each search while preserving the current worktree.
- Code Evidence artifacts now include `worktree.maxPerRepo` and
  `worktree.cleanup` audit metadata with removed path/name summaries and
  remaining counts.
- Added regression coverage for env parsing/defaults and LRU pruning across
  three tagged commits with a two-worktree limit.
- Updated root, V2 Server, Code Evidence, and Config README/SPEC docs.
- Verification passed: focused Code Evidence pytest selection and
  `PYTHONPATH=. uv run --extra dev ruff check logagent_v2 tests`, full
  `PYTHONPATH=. uv run --extra dev pytest` (140 passed, 1 warning), and
  `git diff --check`.

## 2026-06-18 V2 Environment Collector Target Inference

- `analysis_package.json` now includes `environmentCollection` with enabled
  Remote Executors, enabled command templates, enabled file templates, and the
  current executor selection rule for `collect_environment` approvals.
- Approved `collect_environment` side effects now merge target fields from
  `payload.input`, payload top-level fields, and `environmentInput` /
  `remoteInput` so provider-normalized actions can carry remote targets without
  relying on decision-time WebUI input.
- Single-target approvals that provide only `commandId` or `fileId` now infer
  `executorId` when exactly one Remote Executor is enabled; multi-executor
  semantic selection remains explicit/future work.
- Added regression coverage for action-payload target inference and analysis
  package environment candidate exposure.
- Updated root, V2 Server, and Environment Collector README/SPEC docs.
- Verification passed: focused Environment Collector pytest selection and
  `PYTHONPATH=. uv run --extra dev ruff check logagent_v2 tests`, full
  `PYTHONPATH=. uv run --extra dev pytest` (139 passed, 1 warning), and
  `git diff --check`.

## 2026-06-18 V2 Tool Finding Path Normalization

- V2 configured subprocess Tool Runner results now normalize
  `findings[].file` when the tool reports the current input artifact via a
  local absolute path.
- Exact input artifact paths are replaced with the stable `inputFile`
  workspace-relative logical path; child paths under directory inputs become
  `<inputFile>/<relative-child>`.
- Raw stdout/stderr remain stored as support artifacts for audit, while final
  evidence findings avoid leaking local artifact/tmp paths.
- Added regression coverage for storage analyzer file and directory inputs
  that emit absolute finding file paths.
- Updated root, V2 Server, Tool Runner, and Testing README/SPEC docs.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, focused storage/tool-runner pytest selection, full
  `PYTHONPATH=. uv run --extra dev pytest` (137 passed, 1 warning), and
  `git diff --check`.

## 2026-06-18 V2 Flux Analyzer Stdout Parser

- Added a dedicated V2 Tool Runner parser for `flux_query_analyzer` stdout.
  Tool-provided `summary/findings` remain authoritative when present.
- When Flux stdout only contains `metrics`, `topQueries`, and `parseErrors`,
  V2 now derives a `flux query stats` summary and structured findings for
  parse errors, Top Flux templates, p95 latency severity, and new templates.
- Added regression coverage for the fallback parser path without generic
  `summary/findings`.
- Updated root, V2 Server, Tool Runner, and Testing README/SPEC docs.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, focused Tool Runner stdout parser pytest selection, full
  `PYTHONPATH=. uv run --extra dev pytest` (137 passed, 1 warning),
  `./scripts/smoke-flux-query-analyzer.sh`, and `git diff --check`.

## 2026-06-18 V2 Code Evidence Worktree Cache

- Added optional `LOGAGENT_V2_CODE_WORKTREE_ROOT` for Code Evidence detached
  worktree cache; when unset, V2 uses `data_dir/code_worktrees`.
- `logagent.search_code` now resolves the configured ref to a commit, creates
  or reuses a detached `git worktree` under the safe cache root, and runs
  read-only `git grep` inside that worktree instead of the administrator source
  repo worktree.
- Code Evidence artifacts and MCP responses now include `repo.repoPath` plus
  `worktree` metadata (`mode`, `root`, `path`, `commit`, `reused`) so evidence
  consumers can audit where code was read from.
- Added regression coverage for explicit/default worktree roots, relative-root
  rejection, detached worktree creation, and dirty source worktree isolation.
- Updated root, V2 Server, and Code Evidence README/SPEC docs.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, focused Code Evidence pytest selection, full `PYTHONPATH=. uv run
  --extra dev pytest` (136 passed, 1 warning), and `git diff --check`.

## 2026-06-18 WebUI V2 Environment Batch Approval

- V2 Analyze `collect_environment` approval card now supports building a
  batch target list from enabled Remote Executors, command templates, and file
  templates.
- When the batch list is non-empty, the approval decision submits
  `input.targets[]`; when it is empty, the UI keeps the existing single-target
  or MOCK approval behavior.
- Existing action payloads that already contain `targets[]` / `remoteTargets[]`
  are restored into the approval card when loading a waiting run.
- Updated WebUI README/SPEC for the single-target plus batch-target approval
  behavior.
- Verification passed: `npm run lint`, `npm run typecheck`, and
  `npm run build` in `webui/`.

## 2026-06-18 V2 Environment Collector Batch Targets

- Extended approved V2 `collect_environment` actions with a structured
  `targets[]` / `remoteTargets[]` batch input. Each target must use an enabled
  Remote Executor and exactly one whitelisted `commandId` or `fileId`.
- Batch targets use independent idempotency keys
  `environment:<action_id>:<index>` and wait for every remote run to reach a
  terminal state before writing one aggregate `environment_evidence` artifact.
  Aggregate status is `COLLECTED`, `PARTIALLY_COLLECTED`, or `REMOTE_FAILED`.
- Batch support artifacts use logical paths under
  `environment_evidence/<action_id>/targets/<index>/...`; single-target paths
  remain unchanged.
- Remote job-level failures now also attempt environment evidence aggregation,
  so failed batch targets do not leave the analysis run permanently waiting.
- Added regression coverage for mixed command/file batch collection and partial
  batch failure handling.
- Verification passed: focused `collect_environment` pytest selection,
  `PYTHONPATH=. uv run --extra dev ruff check logagent_v2 tests`, full
  `PYTHONPATH=. uv run --extra dev pytest` (135 passed, 1 warning), and
  `git diff --check`.

## 2026-06-18 V2 Fetch Endpoint Patch Regression

- Added API-level regression coverage for `PATCH
  /api/v2/fetch/endpoints/:endpoint_id` on a sensitive Fetch endpoint.
- The test verifies partial update behavior across method, URL, headers, body,
  enabled state, and `followRedirects`; public/API storage remains redacted
  while server-side hydration uses the refreshed encrypted credential set.
- Updated V2 Server and Tool Runner docs to specify that endpoint PATCH merges
  against the hydrated endpoint and refreshes or removes the credential set.
- Verification passed: focused Fetch endpoint PATCH pytest selection,
  `PYTHONPATH=. uv run --extra dev ruff check logagent_v2 tests`, full
  `PYTHONPATH=. uv run --extra dev pytest` (133 passed, 1 warning), and
  `git diff --check`.

## 2026-06-18 V2 Source-built Analyzer V1 Env Alias Coverage

- Added regression coverage that V2 source-built analyzer auto-registration
  accepts Rust/V1 executable env aliases:
  `LOGAGENT_TOOL_FLUX_QUERY_ANALYZER`,
  `LOGAGENT_TOOL_INFLUXQL_ANALYZER`,
  `LOGAGENT_TOOL_OPENGEMINI_STORAGE_ANALYZER`, and
  `LOGAGENT_TOOL_INFLUXDB_STORAGE_ANALYZER`.
- Updated V2 Server, Tool Runner, and Deploy docs to state that V2-specific
  `LOGAGENT_V2_TOOL_*_ANALYZER` names take precedence, while V1 aliases remain
  supported for migration.
- Verification passed: focused `parse_tools_env` pytest selection,
  `PYTHONPATH=. uv run --extra dev ruff check logagent_v2 tests`, full
  `PYTHONPATH=. uv run --extra dev pytest` (132 passed, 1 warning), and
  `git diff --check`.

## 2026-06-18 V2 Memory Vector Recall Documentation

- Reconciled Memory / Case Store docs with the current V2 implementation:
  local hash-vector recall is already implemented in SQLite through
  `vector_json`, merged with FTS/BM25 and keyword fallback.
- Updated Case Store and Memory README/SPEC files to distinguish implemented
  dependency-light local vector recall from future external embedding providers
  and optional sqlite-vec/pgvector backends.
- Narrowed the root pending item to external embedding/vector-index
  enhancement and a more formal analysis evidence bundle.
- Verification passed: `git diff --check`.

## 2026-06-18 V2 Fetch cURL Import Dialects

- Extended the V2 Fetch cURL importer with safe common flags:
  `--url` / `--url=...`, `--user-agent` / `-A`, and `--referer` / `-e`.
- `--url` now sets the endpoint URL while preserving the existing multiple-URL
  rejection, and User-Agent/Referer flags are normalized into ordinary headers
  that continue through Server header validation.
- Added regression coverage for inline long/short flag forms and the second URL
  rejection path.
- Updated root, V2 Server, and Tool Runner docs; the root Fetch pending item now
  only tracks token refresh policy and endpoint schema migration.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, focused Fetch importer pytest selection, full `PYTHONPATH=. uv run
  --extra dev pytest` (131 passed, 1 warning), and `git diff --check`.

## 2026-06-18 V2 Claude Runtime Session Failure Audit

- V2 now writes a fresh `claude_session.json` runtime artifact for every
  Claude Code provider response that carries response metadata, even when the
  response failed and does not include a Claude session id.
- The runtime session audit now records `runtimeStatus`, `providerStatus`,
  linked `agent_response` artifact id, and any provider `error` /
  `validation` status, so failed Claude Code runs replace the initial
  `contract_ready` session artifact with an executable audit state.
- Added regression coverage for a non-zero-exit Claude Code provider inside
  `AgentRuntime`, including the latest task MCP `claude_session` resource.
- Updated root, V2 Server, and Agent Backend docs, and removed the stale root
  pending item for Claude usage/cost, failure classification, resume, and mode
  permission coverage.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, focused Claude Code provider pytest selection, full
  `PYTHONPATH=. uv run --extra dev pytest` (131 passed, 1 warning), and
  `git diff --check`.

## 2026-06-18 V2 InfluxQL Compare Smoke Coverage

- Extended `scripts/smoke-influxql-analyzer.sh` so the real source-built
  `influxql-analyzer` smoke now covers both normal Report mode and
  CompareReport mode with `-input-a` / `-input-b`.
- The compare smoke asserts `statement_delta`, added fingerprints,
  `large_limit` rule deltas, and A/B-side stderr progress output from the real
  CLI.
- Updated root and Tool Runner docs to move InfluxQL compare-mode smoke from
  pending work into the current verified capability set.
- Verification passed: `bash -n scripts/smoke-influxql-analyzer.sh`,
  `./scripts/smoke-influxql-analyzer.sh`, and `git diff --check`.

## 2026-06-18 V2 Source-built Analyzer Smoke Verification

- Verified all four source-built analyzer smoke scripts against real local
  `target/tools` binaries and initialized `third_party/` submodules:
  `scripts/smoke-flux-query-analyzer.sh`,
  `scripts/smoke-influxql-analyzer.sh`,
  `scripts/smoke-opengemini-storage-analyzer.sh`, and
  `scripts/smoke-influxdb-storage-analyzer.sh`.
- Updated Analysis Agent and Tool Runner docs to remove the stale note that
  Flux real smoke was not yet connected, and to document the current four-script
  smoke coverage.
- Verification passed: the four smoke scripts above and `git diff --check`.

## 2026-06-18 V2 Code Evidence Task Context Guard

- Tightened task MCP `logagent.search_code` so runs bound to a Metadata
  `instanceId` inherit that instance's `product` / `version` and reject
  mismatched `product`, `version`, or explicit `gitRef` values.
- Added `taskContext` to Code Evidence artifacts and evidence payloads, keeping
  the existing final-answer refs `code_evidence/<action_id>.json#matches/<index>`.
- Updated root, V2 Server, and Code Evidence README/SPEC docs.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, focused Code Evidence pytest selection, full `PYTHONPATH=. uv run
  --extra dev pytest` (130 passed, 1 warning), and `git diff --check`.

## 2026-06-18 V2 Fetch Task MCP Response Parity

- Extended task MCP `logagent.fetch` responses with Rust/V1-compatible top-level
  `artifactPath`, `statusCode`, `httpOk`, `bodyPreview`, and `evidenceRefs`
  fields while preserving the V2 `result`, `artifact`, and `evidence` objects.
- Kept deterministic task MCP Fetch action ids and the Rust/V1
  `schemaVersion=3` result envelope from the earlier Fetch parity slices.
- Updated root, V2 Server, and Tool Runner README/SPEC docs.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, focused Fetch pytest selection, full `PYTHONPATH=. uv run --extra
  dev pytest` (130 passed, 1 warning), and `git diff --check`.

## 2026-06-18 V2 Log Slice Stable Evidence Refs

- Aligned V2 task MCP `logagent.get_log_slice` with Rust/V1 stable logical
  artifact behavior by deriving `log_slices/slice_<digest>.json#lines` from the
  requested path and resolved line range.
- Repeated same-parameter log slice calls now return the same top-level
  `artifactPath`, nested `slice.ref`, and `evidenceRefs` while preserving the V2
  DB artifact/evidence records.
- Updated root, V2 Server, and Analysis Agent README/SPEC docs.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, focused log-slice pytest selection, full `PYTHONPATH=. uv run --extra
  dev pytest` (130 passed, 1 warning), and `git diff --check`.

## 2026-06-18 V2 Fetch Task MCP Stable Action IDs

- Aligned V2 task MCP `logagent.fetch` with Rust/V1 behavior by deriving a
  deterministic `act_fetch_<digest>` action id from normalized Fetch params.
- Repeated same-parameter task MCP Fetch calls now return the same logical
  `tool_results/<action_id>/result.json#response` evidence ref, while protected
  API/manual Fetch tool runs still create a fresh action id per execution.
- Updated root, V2 Server, and Tool Runner README/SPEC docs.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, focused Fetch pytest selection, full `PYTHONPATH=. uv run --extra
  dev pytest` (130 passed, 1 warning), and `git diff --check`.

## 2026-06-18 V2 Fetch Tool Result Field Parity

- Extended V2 `logagent.fetch` results to use the Rust/V1
  `schemaVersion=3` tool result envelope: `exitCode=null`, `command=[]`,
  `inputFile=null`, empty `stdoutPath` / `stderrPath`, `findings=[]`, and
  `evidenceRefs=["tool_results/<action_id>/result.json#response"]`.
- Kept existing V2 Fetch fields, including `toolId`, redacted request/response
  metadata, response-body artifact id/path, `evidenceRef`, and the final
  `fetch_result` evidence record.
- Updated root, V2 Server, and Tool Runner README/SPEC docs.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, focused Fetch pytest selection, full `PYTHONPATH=. uv run --extra
  dev pytest` (130 passed, 1 warning), and `git diff --check`.

## 2026-06-18 V2 Pprof Tool Result Field Parity

- Extended V2 `pprof_analyzer` manual tool results with Rust/V1-compatible
  `error`, `durationMs`, and `createdAt` fields while keeping existing
  artifact id mappings and `artifactPaths`.
- Added regression coverage to the fake-Go pprof run so the compatibility
  fields stay present with parsed profile metadata and top rows.
- Updated V2 Server and Tool Runner README/SPEC docs.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check
  logagent_v2 tests`, focused pprof/tool registry pytest selection, full
  `PYTHONPATH=. uv run --extra dev pytest` (130 passed, 1 warning), and
  `git diff --check`.

## 2026-06-18 V2 Metadata Tool Result Field Parity

- Extended V2 manual metadata built-in tool results with Rust/V1-compatible
  `params`, `result`, `durationMs`, and `createdAt` fields while preserving the
  existing V2 `value` field.
- `logagent.get_metadata_snapshot` manual runs now expose the V1
  `{ "snapshot": ... }` result wrapper in addition to the V2 expanded value.
- Updated V2 Server and Tool Runner README/SPEC docs.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check
  logagent_v2 tests`, focused metadata/tool registry pytest selection, full
  `PYTHONPATH=. uv run --extra dev pytest` (130 passed, 1 warning), and
  `git diff --check`.

## 2026-06-18 V2 Preprocess Tool Result Field Parity

- Extended V2 `logagent.preprocess_log_package` manual tool results with
  Rust/V1-compatible `manifestPath`, `grepResultsPath`, `toolInputsPath`,
  `toolInputs`, `durationMs`, and `createdAt` fields.
- Kept existing V2 artifact metadata by also exposing manifest/grep artifact
  ids and relative artifact paths in the same result.
- Updated V2 Server and Tool Runner README/SPEC docs.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check
  logagent_v2 tests`, focused preprocess/tool registry pytest selection,
  full `PYTHONPATH=. uv run --extra dev pytest` (130 passed, 1 warning), and
  `git diff --check`.

## 2026-06-18 V2 WebUI Remote File Approval Selection

- Extended the V2 Analyze `collect_environment` approval card to load
  `/api/v2/executor-file-templates` alongside Remote Executors and command
  templates.
- The approval card now lets users choose the remote target type explicitly:
  command targets submit `executorId + commandId`, file targets submit
  `executorId + fileId`, and blank selections preserve the compatible MOCK
  evidence path.
- Updated WebUI and Environment Collector README/SPEC docs to describe the
  command/file mutually exclusive approval input.
- Verification passed: `cd webui && npm run lint`, `cd webui && npm run
  typecheck`, `cd webui && npm run build`, and `git diff --check`.

## 2026-06-18 V2 Source-built Analyzer Catalog Status

- Added `sourceBuiltAnalyzers` to the shared V2 tool catalog returned by
  `/api/v2/tools`, readonly MCP tool catalog resources, and
  `logagent.list_tools`.
- The new catalog field reports the fixed Flux, InfluxQL, openGemini storage,
  and InfluxDB storage analyzer IDs with registered/enabled/runnable/status
  state, so deployments can confirm whether source-built submodule analyzers
  were recognized by the current V2 process.
- This is an observability/catalog parity field only; tool execution still uses
  the existing configured subprocess descriptors, task MCP validation, and
  manual tool-run APIs.
- Updated server-v2 and Tool Runner README/SPEC docs.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, focused tool catalog pytest selection, and `git diff --check`.

## 2026-06-18 V2 Environment SCP File Collection

- Added V2 approved `collect_environment` file collection through Remote
  Executor `operation=file_collection`.
- `LOGAGENT_V2_REMOTE_FILES_JSON` now defines whitelisted remote file
  templates, `LOGAGENT_V2_REMOTE_SCP_COMMAND` controls the absolute SCP binary,
  and `LOGAGENT_V2_REMOTE_FILE_MAX_BYTES` provides the default collected-file
  size cap.
- Approved actions now accept `executorId` plus exactly one `commandId` or
  `fileId`; command targets keep the existing SSH path, while file targets run
  fixed-argv SCP, persist `remote_file/{result.json,stdout.txt,stderr.txt}`,
  and register `environment_evidence/<action_id>/collected_file.bin` as a
  background support artifact.
- Added regression coverage for safe remote file template parsing and fake-SCP
  approved environment collection, including artifact index visibility and
  analysis-run resume.
- Updated server-v2, Environment Collector, Config, Security, Analysis Agent,
  Roadmap, and root docs. Full multi-node Environment Collector planning,
  batch file collection, WebUI file-template selection, and Agent automatic
  executor/template selection remain future work.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, focused `tests/test_store.py` remote environment collection pytest
  selection, and `git diff --check`.

## 2026-06-18 V2 Code Evidence Read-only MVP

- Added V2 Code Evidence configuration via `LOGAGENT_V2_CODE_REPOS_JSON`,
  supporting configured local git repos, default refs, version-to-ref maps, and
  safe relative search roots.
- Added task MCP / Agent provider `logagent.search_code` for configured repos.
  It resolves refs to commits with `git rev-parse`, searches with read-only
  `git grep <commit>`, persists `code_evidence/<action_id>.json`, and returns
  final-answer refs as `code_evidence/<action_id>.json#matches/<index>`.
- Final answer validation and analysis packages now accept current-run Code
  Evidence match refs, include them in allowed evidence refs, and expose the
  latest `code_evidence` task MCP resource.
- Added regression coverage for Code Evidence config validation, task MCP
  search idempotency/resource reads, provider tool advertisement, and final ref
  validation.
- Updated Code Evidence, Config, Analysis Agent, Interfaces, Security,
  Roadmap, server-v2, and root docs. Full worktree/cache, version diff,
  symbol-level parsing, and fix mode code edits remain future work.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, focused Code Evidence pytest selection, and `git diff --check`.

## 2026-06-18 V2 Deployment Script Regression Tests

- Added `server-v2/tests/test_deploy_scripts.py` with lightweight subprocess
  coverage for V2 local/deploy control scripts.
- Covered `scripts/v2-local.sh --help`, invalid
  `LOGAGENT_V2_STARTUP_TIMEOUT_SECONDS`, `deploy/logagent-v2ctl.sh` default
  pid-file scoping, and fast failure when `start` is used before the V2 runtime
  virtualenv is installed.
- Updated Deployment README/SPEC to record the script behavior now protected by
  regression tests.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, `PYTHONPATH=. uv run --extra dev pytest
  tests/test_deploy_scripts.py`, and `git diff --check`.

## 2026-06-18 V2 Agent Provider Error Classification

- V2 Agent provider failures now include stable `error.classification` and
  `error.retryable` fields while keeping existing `stage`, `type`, and
  `message` fields compatible.
- OpenAI-compatible HTTP failures also expose `error.httpStatus` and
  distinguish authentication failures, rate limits, input-too-large responses,
  Provider timeouts, server errors, and generic client errors; allowlisted
  provider request headers remain preserved for failure correlation.
- Binary and Claude Code local provider failures now classify configuration,
  timeout, transport, process, output-size, decode, and parse stages.
- Added fake-provider regression coverage for 401, 429, 413, 500, and 400
  responses, plus binary configuration/non-zero-exit and Claude Code non-zero
  exit paths.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, focused OpenAI-compatible HTTP error classification and local
  provider failure pytest selection, related provider pytest selection, and
  `git diff --check`.

## 2026-06-18 V2 Tools Catalog Envelope Parity

- V2 now uses a shared `tool_catalog()` payload for HTTP `/api/v2/tools`,
  readonly MCP `logagent://tools/catalog` / `logagent-v2://tools/catalog`, and
  readonly `logagent.list_tools`.
- `/api/v2/tools` remains WebUI-compatible by preserving the `tools` field and
  now also returns `schemaVersion` plus V1-compatible `configuredTools`
  summaries with configured args, timeout, match rules, and `maxInputFiles`.
- Added regression coverage for the shared catalog payload and updated
  server-v2 and Tool Runner docs.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, focused tool catalog pytest selection, and `git diff --check`.

## 2026-06-18 V2 Agent Provider Audit Metadata

- V2 OpenAI-compatible Agent provider now promotes stable audit metadata into
  `agent_response.json`: provider request id from allowlisted response headers,
  provider response id, response model, finish reason, usage, system
  fingerprint, and allowlisted response-header metadata.
- HTTP error responses also preserve allowlisted provider request headers so
  provider-side failures can be correlated without storing request headers or
  API keys.
- Updated server-v2, LLM Gateway, Agent Backend, and root docs; also corrected
  the root README's stale note about approved remote environment collection,
  which can now use Remote Executor evidence.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, focused OpenAI-compatible Agent provider pytest, provider runtime
  pytest selection, and `git diff --check`.

## 2026-06-18 V2 Analysis Package Artifact Index

- `analysis_package.json` now includes a bounded `artifactIndex` outline with
  current run upload, evidence, and support artifact paths, sources, roles,
  sizes, content types, and artifact ids.
- This makes remote environment command output support artifacts discoverable
  from the next Agent request package without inlining artifact contents.
- Added regression coverage for normal analysis packages and environment
  evidence packages.
- Updated server-v2 and Analysis Agent README/SPEC docs.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, focused workspace/environment analysis-package pytest, related
  `analysis_package or artifact_index or collect_environment` pytest
  selection, and `git diff --check`.

## 2026-06-18 V2 Environment Remote Output Artifacts

- V2 `collect_environment` remote execution now copies the completed remote
  command `result`, `stdout`, and `stderr` files into the current analysis
  workspace artifact registry.
- `environment_evidence/<action_id>/result.json` now links those files through
  `artifactIds` / `artifactPaths`, so `GET /api/v2/runs/:run_id/artifacts` and
  task MCP `artifact_index` expose them as background support artifacts.
- Added fake-ssh regression coverage for environment evidence support artifact
  registration and artifact-index visibility.
- Updated server-v2 and Environment Collector README/SPEC docs.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, focused
  `tests/test_store.py::StoreTests::test_approved_collect_environment_can_use_remote_executor`,
  environment/remote pytest selection, and `git diff --check`.

## 2026-06-18 WebUI V2 Case Import Detail Restore

- V2 Memory import history selection now calls
  `GET /api/v2/cases/imports/:import_id` before restoring a draft, so edits and
  confirmation start from the authoritative current import record rather than a
  possibly stale list snapshot.
- Restored import details are upserted back into the recent import history,
  keeping status, validation errors, and messages aligned after selection.
- Updated WebUI README/SPEC docs.
- Verification passed: `npm run lint`, `npm run typecheck`, and
  `npm run build`.

## 2026-06-18 WebUI V2 Case Import Draft Save

- V2 Memory Workbench now exposes the V2 Case import patch route through a
  `Save draft` action, calling `PATCH /api/v2/cases/imports/:import_id` for
  unconfirmed imports.
- Saving a draft refreshes the active structured draft, validation errors, and
  recent import history entry without confirming it into Case Memory.
- Confirmed imports disable draft save/confirm controls, matching the backend's
  immutable confirmed import behavior.
- Updated WebUI README/SPEC docs.
- Verification passed: `npm run lint`, `npm run typecheck`, and
  `npm run build`.

## 2026-06-18 WebUI V2 Manual Tool InputFiles

- V2 Tools Workbench manual `tool_run` creation now honors descriptor-supported
  `params.inputFiles` as explicit Workspace inputs, matching the V2 Tool Runner
  backend path for reusing `extracted/...` or `tool_inputs/...` entries.
- The WebUI no longer blocks configured command tool runs solely because no new
  upload was selected when the Params JSON already provides valid
  `inputFiles`; tools that do not advertise `inputFiles` keep the previous
  upload-count validation.
- Updated WebUI and Tool Runner README/SPEC docs.
- Verification passed: `npm run lint`, `npm run typecheck`, and
  `npm run build`.

## 2026-06-18 WebUI V2 Case Import History

- V2 Memory Workbench now calls `GET /api/v2/cases/imports` while refreshing
  Case Memory, showing the recent import drafts alongside the Case search/edit
  workflow.
- Preview, supplement-message, and confirm flows update the in-memory import
  history immediately, so the latest draft/status is visible without another
  refresh.
- Selecting a history item restores the structured draft, validation errors,
  message history, status, and import metadata into the active V2 import editor.
- Updated WebUI README/SPEC docs for the import history and draft restore
  behavior.
- Verification passed: `npm run lint`, `npm run typecheck`, and
  `npm run build`.

## 2026-06-18 V2 Environment Approval Remote Target Selection

- `POST /api/v2/actions/:action_id/decisions` now accepts an optional
  approved decision `input` object. For approvals, V2 persists that input back
  into the action payload, records it in the decision result/timeline event, and
  then runs approval side effects from the approved payload.
- `collect_environment` approvals can now be completed from WebUI even when the
  Agent did not pre-fill a remote target: the V2 Analyze approval panel loads
  enabled Remote Executors and whitelisted command templates, submits the chosen
  `executorId` / `commandId` as decision input, and still allows no-target
  approval for the compatible MOCK evidence path.
- Added API regression coverage that verifies decision-time
  `collect_environment` input is persisted, queues exactly one Remote Executor
  job, keeps the analysis run waiting for environment collection, and remains
  idempotent on repeated submissions.
- Updated server-v2, WebUI, and Environment Collector README/SPEC docs.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, focused approval/environment pytest, full
  `PYTHONPATH=. uv run --extra dev pytest`, `npm run lint`,
  `npm run typecheck`, `npm run build`, and `git diff --check`.

## 2026-06-18 WebUI V2 System Context Version Editing

- Added WebUI coverage for V2's V1-compatible
  `PATCH /api/v2/system-context/resources/:context_id/versions/:version_id`
  route.
- The System Context resource editor can now load an existing version into the
  version form, update content type, content, summary, and prompt policy, then
  save the patch without creating a new revision.
- Existing append-version and activate-version flows remain available from the
  same panel.
- Updated WebUI and System Context README/SPEC docs.
- Verification passed: `npm run lint`, `npm run typecheck`, and
  `npm run build`.

## 2026-06-18 WebUI V2 System Context Compatibility Resources

- V2 System Context Workbench now calls `/api/v2/system-context/resources` in
  addition to Skills and Metadata APIs, showing managed System Context
  resources alongside read-only `meta_<instanceId>` Metadata adapters.
- Added WebUI controls to create V1-compatible resources, edit resource
  summary fields, append new content versions, activate older versions, and
  select resources/adapters for `/api/v2/system-context/preview`.
- The preview panel now renders the backend-selected resources and prompt while
  keeping V2 analysis runs on the existing Skill-backed System Context path.
- Updated WebUI and System Context README/SPEC docs.
- Verification passed: `npm run lint`, `npm run typecheck`, and
  `npm run build`.

## 2026-06-18 V2 Local Build And Service Script

- Added `scripts/v2-local.sh` for local V2 build/start/stop/restart/status/logs
  without copying the runtime `deploy/` template.
- The helper defaults to `server-v2/.venv`, `/tmp/logagent-v2-local`, port
  `50993`, and `target/tools`; `start` reuses an existing virtualenv/WebUI build
  for fast restarts, while `--with-tools` and `--only-tool <name>` explicitly
  rebuild source-referenced analyzer submodules.
- Documented the split between local V2 development controls and runtime V2
  deploy controls, and added V2 Remote Executor environment examples to
  `deploy/.env.example`.
- Verification passed: `bash -n scripts/v2-local.sh`,
  `scripts/v2-local.sh --help`, and `git diff --check`.

## 2026-06-18 V2 Remote Executor File Downloads

- Added protected `GET /api/v2/executor-runs/:run_id/files/:file_name` for V2
  Remote Executor result file downloads. The endpoint only accepts `result`,
  `stdout`, or `stderr`, resolves the path from persisted run output, and
  rejects missing files or paths outside `LOGAGENT_V2_DATA_DIR`.
- V2 Executors Workbench now exposes download buttons for remote run
  `result.json`, `stdout.txt`, and `stderr.txt` while keeping the existing
  preview/path display.
- Added regression coverage for successful result/stdout/stderr downloads and
  invalid file names.
- Updated server-v2, WebUI, and Environment Collector README/SPEC docs.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, focused Remote Executor file download pytest, full
  `PYTHONPATH=. uv run --extra dev pytest`, `npm run lint`,
  `npm run typecheck`, and `npm run build`.

## 2026-06-18 WebUI V2 Fetch Standalone Runs

- V2 Fetch Workbench now supports standalone Fetch `tool_run` creation through
  `/api/v2/fetch/endpoints/:endpoint_id/runs`, with an optional existing
  Workspace id or backend-created isolated Workspace.
- The page now lists `/api/v2/fetch/runs`, filters by endpoint/workspace,
  selects historical Fetch tool runs, polls non-terminal selected runs, and
  reads `/api/v2/tools/runs/:run_id/result` plus
  `/api/v2/tools/runs/:run_id/artifacts` on success.
- Standalone Fetch tool runs reuse the same authenticated result/body/support
  artifact download UI as run-scoped Fetch execution.
- Updated WebUI README/SPEC docs.
- Verification passed: `npm run lint`, `npm run typecheck`, and
  `npm run build`.

## 2026-06-18 WebUI V2 Fetch Artifact Downloads

- V2 Fetch Workbench result panels now expose both the Fetch `result.json`
  artifact and the bounded `response_body.bin` artifact when a run-scoped Fetch
  call completes.
- Result and response body downloads use the shared authenticated
  `/api/v2/artifacts/:artifact_id` flow instead of unauthenticated links.
- Updated WebUI README/SPEC docs.
- Verification passed: `npm run lint`, `npm run typecheck`, and
  `npm run build`.

## 2026-06-18 WebUI V2 Manual Tool Run Artifacts

- V2 Tools Workbench now polls the selected non-terminal manual `tool_run`
  every second and refreshes its status/phase/result panel from the V2 run API.
- Manual `tool_run` selection now reads
  `/api/v2/tools/runs/:run_id/artifacts`, displays uploads, evidence artifacts,
  and V2 `supportArtifacts`, and downloads each artifact through the
  authenticated `/api/v2/artifacts/:artifact_id` endpoint.
- Added typed V2 API support for manual tool-run artifact listing.
- Updated WebUI README/SPEC docs.
- Verification passed: `npm run lint`, `npm run typecheck`, and
  `npm run build`.

## 2026-06-18 WebUI V2 Manual Tool Runs

- V2 Tools Workbench now supports the Server V2 manual `tool_run` API in
  addition to run-scoped task MCP execution.
- Users can reuse an existing Workspace id or leave it blank for the WebUI to
  create a dedicated tool-run Workspace, upload files through V2 Workspace
  upload APIs, queue `/api/v2/tools/:tool_id/runs`, refresh
  `/api/v2/tools/runs`, and read `/api/v2/tools/runs/:run_id/result`.
- Added typed V2 API helpers for tool-run create/list/get/result calls and
  wired the manual run history/result panel into `V2ToolsBridge`.
- Updated WebUI README/SPEC docs.
- Verification passed: `npm run lint`, `npm run typecheck`, and
  `npm run build`.

## 2026-06-18 V2 Tool Support Artifact Index

- V2 run artifact aggregation now exposes non-evidence tool support files under
  `supportArtifacts`, including configured subprocess stdout/stderr, Fetch
  response bodies, and pprof top/tree/raw/stderr/SVG outputs.
- Task MCP `artifact_index` now includes those support files with
  Rust/V1-style logical `tool_results/<action_id>/...` paths and
  `source="support"`, while keeping them `finalAllowed=false`.
- V2 Analyze artifact list now counts, displays, and downloads
  `supportArtifacts` through the same authenticated artifact endpoint used for
  uploads and evidence artifacts.
- Updated V2 Server, Tool Runner, and WebUI README/SPEC docs.
- Verification passed: focused configured-tool and pprof regressions,
  `PYTHONPATH=. uv run --extra dev pytest tests/test_store.py -k
  "task_mcp_runs_configured_tool_by_id or pprof_tool_result_includes_v1_artifact_paths"`
  (`2 passed`), `PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, `PYTHONPATH=. uv run --extra dev pytest` (`120 passed`, with the
  existing Starlette/httpx deprecation warning), plus WebUI `npm run lint`,
  `npm run typecheck`, and `npm run build`.

## 2026-06-18 V2 Tool Runner Numeric Field Parser Parity

- V2 Tool Runner generic stdout JSON parser now matches Rust/V1 string-field
  coercion for analyzer outputs: JSON number values in summary/message/status/
  path-like fields are normalized to strings, while booleans remain ignored for
  those fields.
- Added regression coverage for numeric `summary`, finding `description`,
  `status`, and `path` fields while retaining line-number parsing.
- Updated V2 Server and Tool Runner README/SPEC docs.
- Verification passed: focused Tool Runner parser regressions,
  `PYTHONPATH=. uv run --extra dev ruff check logagent_v2 tests`, and
  `PYTHONPATH=. uv run --extra dev pytest` (`120 passed`, with the existing
  Starlette/httpx deprecation warning).

## 2026-06-18 V2 Tool Runner Stdout/Stderr Artifacts

- V2 configured subprocess Tool Runner now persists bounded stdout and stderr
  as first-class artifacts for every run, including success, non-zero exit,
  timeout, and spawn-error paths.
- Tool results and evidence payloads now expose `stdoutArtifactId` and
  `stderrArtifactId` while keeping Rust/V1-compatible logical
  `tool_results/<action_id>/stdout.txt` and `stderr.txt` paths plus existing
  bounded previews.
- Task MCP `tool_results` resources now surface the stdout/stderr artifact IDs
  alongside the result artifact, so Agent/UI clients can fetch exact output
  evidence through the V2 artifact API.
- Updated V2 Server and Tool Runner README/SPEC docs.
- Verification passed: configured tool stdout/spawn-error/timeout focused
  regression tests, `PYTHONPATH=. uv run --extra dev ruff check logagent_v2
  tests`, and `PYTHONPATH=. uv run --extra dev pytest` (`119 passed`, with the
  existing Starlette/httpx deprecation warning).

## 2026-06-18 V2 Agent Search Logs Schema

- V2 OpenAI-compatible and binary Agent provider prompts now advertise
  `logagent.search_logs.maxMatches` with the same 1..200 bounds as task MCP,
  matching the Rust/V1-compatible execution contract.
- Added regression coverage for provider `availableTools` exposing
  `maxMatches` alongside the existing task MCP search test.
- Updated V2 Server and Analysis Agent README/SPEC docs.
- Verification passed: focused search/provider regression,
  `PYTHONPATH=. uv run --extra dev ruff check logagent_v2 tests`, and
  `PYTHONPATH=. uv run --extra dev pytest` (`119 passed`, with the existing
  Starlette/httpx deprecation warning).

## 2026-06-18 V2 Agent Log Slice Schema

- V2 OpenAI-compatible and binary Agent provider prompts now advertise
  `logagent.get_log_slice` with both center-line `lineNumber` and Rust/V1
  compatible `startLine` / `endLine` range forms, matching task MCP
  `tools/list` and execution behavior.
- Added regression coverage for provider `availableTools` exposing the range
  shape alongside the existing task MCP range execution test.
- Updated V2 Server and Analysis Agent README/SPEC docs.
- Verification passed: focused log-slice/provider regressions,
  `PYTHONPATH=. uv run --extra dev ruff check logagent_v2 tests`, and
  `PYTHONPATH=. uv run --extra dev pytest` (`119 passed`, with the existing
  Starlette/httpx deprecation warning).

## 2026-06-18 V2 Agent Domain Tool Schema

- V2 OpenAI-compatible and binary Agent provider prompts now advertise
  `logagent.run_domain_tool` with the same schema as task MCP `tools/list`:
  either V2 `toolId` or Rust/V1-compatible `tool + inputFile`.
- Provider-visible configured tool enums now exclude manual-only tools such as
  `pprof_analyzer`, matching task MCP behavior and preventing providers from
  requesting tool runs that only the protected Tools API should start.
- Added regression coverage for provider `availableTools` schema parity and
  manual-only exclusion.
- Updated V2 Server and Tool Runner README/SPEC docs.
- Verification passed: focused run-domain-tool/provider regressions,
  `PYTHONPATH=. uv run --extra dev ruff check logagent_v2 tests`, and
  `PYTHONPATH=. uv run --extra dev pytest` (`119 passed`, with the existing
  Starlette/httpx deprecation warning).

## 2026-06-18 V2 Session Analysis Mode

- V2 Session alias APIs now persist and return `analysisMode` on create, read,
  list, and PATCH instead of forcing Session-created Workspaces to `diagnose`.
- Session task creation/listing now inherits the current Session
  `analysisMode`, so Claude Code permission profile selection stays aligned
  for `diagnose`, `code_investigation`, and `fix` runs.
- Added regression coverage for creating a Session in `code_investigation`,
  PATCHing it to `fix`, and verifying the resulting TaskSummary mode.
- Updated V2 Server and Interfaces README/SPEC docs.
- Verification passed: focused Session alias regression,
  `PYTHONPATH=. uv run --extra dev ruff check logagent_v2 tests`, and
  `PYTHONPATH=. uv run --extra dev pytest` (`119 passed`, with the existing
  Starlette/httpx deprecation warning).

## 2026-06-18 V2 Claude Permission Profiles

- V2 Claude Code provider now selects Rust/V1-style permission profiles from
  the Workspace analysis mode: `diagnose` is MCP-only, `code_investigation`
  enables Read/Grep/Bash without edits, and `fix` enables Read/Grep/Bash/Edit/
  Write with `acceptEdits`.
- `LOGAGENT_V2_CLAUDE_CODE_PERMISSION_PROFILES_JSON` can override individual
  mode profiles, while the older flat `LOGAGENT_V2_CLAUDE_CODE_*` permission
  variables remain a compatibility override for the `diagnose` profile.
- `agent_request.json`, `agent_response.json`, and runtime
  `claude_session.json` now record `analysisMode`, `permissionProfile`, and
  `nativeToolPolicy`; Settings summaries also expose the profile set.
- Added regression coverage for code-investigation CLI argv selection and
  profile env parsing.
- Verification passed: focused Claude/settings regressions,
  `PYTHONPATH=. uv run --extra dev ruff check logagent_v2 tests`, and
  `PYTHONPATH=. uv run --extra dev pytest` (`119 passed`, with the existing
  Starlette/httpx deprecation warning).

## 2026-06-18 V2 Claude Session Runtime Artifact

- V2 now writes a fresh `claude_session.json` runtime artifact after Claude
  Code provider responses that include session metadata, replacing the latest
  task MCP `claude_session` resource from the initial `contract_ready` artifact
  to a runtime session record.
- The runtime session artifact records `claudeSessionId`, `resumedSessionId`,
  usage/cost, prompt delivery, attempt, and the linked `agent_response`
  artifact id.
- Added regression coverage for final-answer and resumed Claude Code paths.
- Updated V2 Server and Agent Backends README/SPEC docs.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check
  logagent_v2 tests`, focused Claude Code provider regressions, and
  `PYTHONPATH=. uv run --extra dev pytest` (`119 passed`, with the existing
  Starlette/httpx deprecation warning).

## 2026-06-18 V2 Claude Usage And Cost Audit

- V2 Claude Code provider now preserves Claude envelope `usage` and
  `total_cost_usd` / `totalCostUsd` in `agent_response.json` as
  `response.usage` and `response.cost.usd`, aligning the Python V2 audit
  surface with the Rust/V1 Claude runner.
- Added regression coverage to the Claude Code provider final-answer path.
- Updated V2 Server and Agent Backends README/SPEC docs.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check
  logagent_v2 tests`, focused Claude Code provider regressions, and
  `PYTHONPATH=. uv run --extra dev pytest` (`119 passed`, with the existing
  Starlette/httpx deprecation warning).

## 2026-06-18 V2 Claude Code Session Resume

- V2 `LOGAGENT_V2_AGENT_PROVIDER=claude_code` now resumes Claude Code sessions
  after waiting states. AgentRuntime reads the latest `agent_response.json`
  for `response.sessionId`, injects it into the next provider request as
  `resumeSessionId`, and the Claude Code CLI provider appends
  `--resume <session_id>`.
- `agent_response.json` records `response.resumedSessionId` on resumed Claude
  Code calls, so audit artifacts show both the new session id and the session
  being resumed without exposing API keys or executable paths.
- Updated V2 Server and Agent Backends README/SPEC docs.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check
  logagent_v2 tests`, focused Claude Code provider regressions, and
  `PYTHONPATH=. uv run --extra dev pytest` (`119 passed`, with the existing
  Starlette/httpx deprecation warning).

## 2026-06-18 V2 Claude Code CLI Provider

- Added `LOGAGENT_V2_AGENT_PROVIDER=claude_code` to the Python V2 Agent runtime.
- V2 now materializes per-run `claude_prompt.md` and
  `claude_mcp_config.json` under `data_dir/tmp/claude_sessions/<run_id>/`,
  launches the configured Claude Code CLI with
  `--print --output-format json --json-schema --mcp-config
  claude_mcp_config.json --strict-mcp-config`, and injects
  `LOGAGENT_V2_API_KEY` only through the child process environment.
- Claude Code stdout parsing now accepts native Claude envelopes using
  `structured_output`, `structuredOutput`, or `result`. Completed outcomes
  require `finalAnswer`; `waiting_for_user` and `waiting_for_approval` are
  bridged into the existing `logagent.request_user_input` and
  `logagent.request_approval` task MCP tools.
- Added Claude Code provider configuration for
  `LOGAGENT_V2_CLAUDE_CODE_PATH` / `LOGAGENT_CLAUDE_CODE_PATH`, max output
  bytes, permission mode, native tools, allowed tools, and disallowed tools.
  Settings diagnostics validate the CLI path without launching a task session
  and never return local executable paths or API keys.
- `claude_code` run aliases use the local summary/question fallback instead of
  starting a second Claude Code session.
- Updated V2 Server, Config, and Agent Backends README/SPEC docs.
- Verification passed: `PYTHONPATH=. uv run --extra dev ruff check
  logagent_v2 tests`, focused provider/settings regressions, and
  `PYTHONPATH=. uv run --extra dev pytest` (`119 passed`, with the existing
  Starlette/httpx deprecation warning).

## 2026-06-18 V2 Automatic Tool Runner Phase

- V2 analysis now runs matching input-based configured subprocess tools after
  initial manifest/grep evidence and before the first Agent provider request,
  matching the Rust/V1 rule-based `RUN_TOOL` phase.
- Auto-run Tool Runner results are persisted as `tool_result` evidence and
  injected into `analysis_package.json`, `agent_request.json`, and the prompt
  as `preRunToolResults` with finding-level `finalEvidenceRefs`.
- Task MCP `logagent.run_domain_tool` now reuses an existing result for the
  same `toolId + actionId` within one run, avoiding duplicate artifacts when
  Agent retries or users inspect a tool result after automatic execution.
- Manual-only and built-in tools such as `pprof_analyzer`, Fetch, preprocess,
  metadata, and Huawei sync remain explicit and are not triggered by the
  automatic phase.
- Configured tools that need runtime `{params.name}` values or required params
  are skipped by the automatic phase and remain available through explicit
  Agent/user tool calls.
- Updated V2 Server and Tool Runner README/SPEC docs.
- Verification passed: focused Tool Runner/Agent regressions,
  `PYTHONPATH=. uv run --extra dev ruff check logagent_v2 tests`,
  `PYTHONPATH=. UV_PROJECT_ENVIRONMENT=<tmp> uv run --extra dev pytest`
  (`116 passed`, with the existing Starlette/httpx deprecation warning), and
  `git diff --check`.

## 2026-06-18 WebUI V2 Dev Proxy

- WebUI Vite development proxy now defaults `/api` and `/health` to the Python
  V2 server at `http://127.0.0.1:50993`, matching the default V2 route cutover.
- Added `VITE_LOGAGENT_API_TARGET` as an override for Rust V1 or alternate
  backend ports during local development.
- Updated WebUI README/SPEC for the default V2 dev backend and override
  behavior.
- Verification passed: `npm run lint`, `npm run typecheck`,
  `npm run build`, and `git diff --check`.

## 2026-06-18 Native Agent V2 Target

- Native Agent added `native_agent.server_api` with default `v1` and V2 mode
  for `server-v2` Session-scoped upload APIs.
- Chrome Extension and server-v2 docs now state that browser import remains
  stable at Native Agent `/imports`; no extension code change is needed for V2.
- V2 imports now create or reuse the active `ws_...` Session before upload,
  use `/api/v2/sessions/:session_id/uploads` for small files, use
  `/api/v2/sessions/:session_id/uploads/init` plus
  `/api/v2/uploads/:upload_session_id/chunks|complete` for chunked files, and
  return the completed `upl_...` upload id.
- Added `examples/native-agent-v2-50993.yaml`; existing Native Agent examples
  now explicitly declare `server_api: "v1"`.
- Updated Native Agent and root README/SPEC docs for V1/V2 import behavior.
- Verification passed: `cargo fmt --check`,
  `cargo check -p logagent-native-agent`,
  `cargo test -p logagent-native-agent`, `git diff --check`, and an isolated
  V2 HTTP smoke on temporary ports `51033`/`17329` that imported
  `testing/fixtures/downloads/sample.log` through `/imports` and confirmed the
  resulting `upl_...` was attached to the created `ws_...` Session.

## 2026-06-18 WebUI V2 Product Naming

- Default V2 pages now use Workbench/Console titles instead of user-visible
  Bridge labels, matching the V2 default-route cutover.
- Source filenames and exported component names keep the existing
  `V2*Bridge.tsx` names for now to avoid unrelated rename churn.
- Updated WebUI README/SPEC for the product naming rule.

## 2026-06-18 WebUI V2 Default Cutover

- The React App default routes now render V2 Analyze, Memory, System Context,
  Metadata, Tools, Fetch, Executors, and Settings surfaces directly instead of
  rendering legacy Rust-compatible panels below V2 bridge panels.
- The global LLM debug toggle now reads and writes `/api/v2/debug/llm`, keeping
  the default WebUI on V2 APIs.
- Updated root, WebUI, and V2 Server docs/specs to mark the default WebUI V2
  cutover as implemented and move remaining V2 work to product polish and
  real-domain fixtures.

## 2026-06-18 WebUI V2 Runtime Detail Parity

- V2 Executors bridge now surfaces executor last-check status/message,
  command template descriptions and timeouts, remote run attempts,
  executor/command IDs, timestamps, SSH argv preview, and persisted result
  timing details.
- V2 Memory bridge now exposes Case Memory search backend details with total,
  FTS, and vector scores in both search results and selected Case details.
- Updated WebUI README/SPEC for the richer V2 executor and memory runtime
  inspection surfaces.

## 2026-06-18 WebUI V2 Fetch Run Overrides

- V2 Fetch bridge now accepts run-scoped override JSON when executing
  `/api/v2/runs/:run_id/fetch/:endpoint_id`.
- The bridge forwards `variables`, header overrides, and body overrides to the
  V2 backend, matching the task MCP `logagent.fetch` parameter path.
- Updated WebUI V2 API typings and WebUI README/SPEC for Fetch override
  execution.

## 2026-06-18 WebUI V2 Tool Descriptor Parity

- V2 Tools bridge now exposes the descriptor fields needed to verify V1 tool
  migration: tags, editable/exportable/manualOnly flags, file-count bounds,
  accepted suffixes, output views, match rules, params template, and params
  schema.
- Selecting a V2 tool now pre-fills the task MCP Params JSON from
  `paramsTemplate`, so built-in tools and source-built analyzer tools can be
  invoked without guessing their parameter shape.
- Updated WebUI V2 API typings and WebUI README/SPEC for the richer tool
  descriptor display.

## 2026-06-18 WebUI V2 Analyze Runtime Resources

- V2 Analyze bridge now expands `/api/v2/runs/:run_id/analysis.resources`
  into a runtime resources panel instead of showing only a resource count.
- The panel summarizes `analysis_state.json` LangGraph runtime metadata,
  Agent request/response audit details, Claude MCP/session contract artifacts,
  and `mcp_calls.jsonl` call count / latest call.
- Updated WebUI README/SPEC for the V2 runtime resource inspection surface.

## 2026-06-18 V2 Readonly MCP Resource Envelopes

- V2 readonly MCP `resources/read` now returns Rust/V1-style
  `schemaVersion=1` envelopes for collection resources:
  `metadata/instances`, `cases/recent`, `skills`, and `domain-adapters`.
- Added regression coverage for both `logagent://...` and
  `logagent-v2://...` resource aliases where applicable.
- Updated V2 Server README/SPEC for the readonly collection resource envelope.

## 2026-06-18 WebUI V2 Agent Graph Runtime Display

- V2 Settings bridge now renders the Agent backend `graphRuntime` summary from
  `/api/v2/settings/agent-backends`, including LangGraph engine, graph name,
  and concrete runtime node list.
- Agent backend dry-run diagnostic results also display their returned
  `graphRuntime` before the raw JSON payload, so UI users can verify the active
  V2 Agent graph without inspecting artifacts manually.
- Updated WebUI V2 API typings and WebUI README/SPEC for the LangGraph runtime
  display.

## 2026-06-18 V2 LangGraph Planner Node Split

- V2 `AgentRuntime.run_analysis` now executes through a real LangGraph
  `StateGraph` named `logagent_v2_analysis` with separate
  `collect_initial_evidence`, `prepare_agent_request`, `call_agent_provider`,
  `execute_tool_calls`, `validate_final_answer`, and `finalize_result` nodes.
- Provider tool-call responses now route through the `execute_tool_calls` node,
  non-waiting observations loop back to `prepare_agent_request`, normal
  answers route through `validate_final_answer`, and waiting/approval tool
  calls end the current graph invocation in the matching waiting state.
- Existing provider/tool-loop behavior, waiting states, audit artifacts,
  evidence validation, and result persistence remain Server-owned and bounded;
  `analysis_state.json`, Settings Agent backend summary, and dry-run
  diagnostics now report the expanded graph node list.
- Added regression coverage that completed stub runs and Settings diagnostics
  expose the shared `AGENT_GRAPH_NODES` list through task MCP and backend
  metadata.

## 2026-06-18 V2 Provider-Backed Run Alias

- Successful V2 analysis runs now mirror Rust/V1 task naming more closely:
  OpenAI-compatible and local binary Agent providers receive a separate
  `run_alias` JSON prompt after final-answer validation.
- Alias generation is non-blocking for task success. Stub mode, provider
  errors, non-JSON output, or invalid/generic aliases fall back to the existing
  deterministic summary/question alias.
- Added regression coverage for OpenAI-compatible and binary provider alias
  generation while preserving existing Agent request/response audit artifacts.
- Updated V2 Server README/SPEC for the alias behavior.
- Verification passed: focused provider alias regressions, V2 ruff, V2 full
  pytest, compileall, and `git diff --check`.

## 2026-06-18 V2 Runtime Tool Auto-Discovery

- V2 now auto-registers source-built analyzer tools from
  `LOGAGENT_V2_TOOLS_DIR`, `$LOGAGENT_V2_APP_DIR/bin/tools`, or
  `$LOGAGENT_APP_DIR/bin/tools` when explicit `LOGAGENT_V2_TOOL_*` variables
  are unset.
- `deploy/logagent-v2ctl.sh` now exports `LOGAGENT_V2_APP_DIR` before starting
  the Python server, so `rebuild-v2-install.sh --with-tools` plus
  `logagent-v2ctl.sh start` is enough for the standard analyzer binaries to
  appear in `/api/v2/tools`.
- Updated V2 Server and Deploy docs/specs, plus `.env.example`, for the
  source-built analyzer auto-discovery behavior.
- Verification passed: focused source-built tool config regressions, V2 ruff,
  V2 full pytest, compileall, shell syntax checks, and `git diff --check`.

## 2026-06-18 V2 LLM Debug Route Parity Coverage

- Added route-level regression coverage for `GET/PUT /api/v2/debug/llm`, so
  V2 keeps the Rust/V1 process-local LLM response logging toggle behavior
  locked at the HTTP boundary.
- The test verifies both API responses and the underlying process-local flag,
  then resets the flag to avoid cross-test leakage.

## 2026-06-18 V2 Claude Runtime Contract Artifacts

- V2 analysis runs now automatically write Rust/V1-style Claude runtime
  contract artifacts: `claude_prompt.md`, `claude_mcp_config.json`, and
  `claude_session.json`.
- The generated MCP config points at `/api/v2/mcp/task/:run_id` and uses
  `Bearer ${LOGAGENT_V2_API_KEY}` as an environment placeholder, so the real
  API key is not persisted in artifacts.
- `/api/v2/runs/:run_id/artifacts` now exposes `claudePromptPath` and
  `claudePrompt` alongside the existing Claude MCP config/session aggregate
  fields.
- Added regression coverage that these artifacts are created automatically for
  a stub run and are included in the artifact aggregate response.

## 2026-06-17 V2 Analysis Package Runtime Resource Index

- V2 `analysis_package.resources` now includes the optional Rust/V1 Claude
  runtime compatibility resources `claude_mcp_config` and `claude_session`,
  matching the task MCP `resources/list` surface.
- Added regression coverage so future package index changes cannot hide those
  resources from Agents that discover task resources through
  `analysis_package.json`.

## 2026-06-17 V2 Analysis Package Resume State

- V2 `analysis_package.json` now includes a bounded `analysisState` section
  with recent user messages, action results, pending actions, and
  `finalizeRequested`.
- This aligns V2 with the Rust/V1 Claude prompt contract where
  `analysis_package.analysisState.finalizeRequested=true` tells the Agent not
  to request more user input and to answer from current evidence.
- Added regression coverage that `resumeMode=finalize` appears both in the
  provider prompt policy and in the task MCP `analysis_package` resource.

## 2026-06-17 V2 Provider Tool MCP Audit

- V2 Agent provider loop now writes successful provider-executed task tool calls
  to the shared `mcp_calls.jsonl` audit artifact, matching the task MCP
  `tools/call` visibility surface instead of only embedding observations in
  `agent_response.json`.
- Waiting tool calls from the provider loop are also audited, including the
  `mcp_waiting_request.json#request` evidence ref, before the run pauses in
  `waiting_for_user` or `waiting_for_approval`.
- Added regressions for provider `logagent.search_logs` and
  `logagent.request_user_input` calls appearing in the task MCP `mcp_calls`
  resource.

## 2026-06-17 V2 Provider Waiting Tool Loop

- V2 OpenAI-compatible and binary Agent provider prompts now advertise
  `logagent.request_user_input` and `logagent.request_approval` during normal
  analysis, so real provider loops can enter the same waiting states exposed by
  task MCP.
- When a provider requests a waiting/approval tool, V2 now records the
  provider response as `paused`, persists `analysis_state` with the matching
  `waiting_for_user` or `waiting_for_approval` status, keeps the pending action,
  and ends the current job without writing a final result.
- Resume with `resumeMode=finalize` now removes waiting/approval tools from the
  next provider prompt through `resumePolicy.finalizeWithCurrentEvidence`, so
  the resumed run must answer with current evidence instead of asking again.
- Added regressions for provider-requested user-input pause, normal provider
  tool availability, and finalize prompt suppression of waiting tools.

## 2026-06-17 V2 WebUI Waiting Idempotency

- V2 Analyze bridge now sends pending action `questionId` when answering
  `WAITING_FOR_USER` runs, so the frontend targets the same question contract
  enforced by `POST /api/v2/runs/:run_id/messages`.
- User-message and approval/rejection requests now include stable
  `idempotencyKey` values derived from run/action/intent/content, preventing
  duplicate resume or approval events on repeated clicks or network retries.
- Updated WebUI docs/specs for the V2 waiting-state bridge behavior.
- Verification passed: `cd webui && npm run lint`,
  `cd webui && npm run typecheck`, `cd webui && npm run build`, and
  `git diff --check`.

## 2026-06-17 V2 Huawei Package Sync Result Parity

- V2 `logagent.huawei_cloud_package_sync` worker execution now revalidates and
  normalizes tool params before running, matching the HTTP route behavior during
  recovered or internally-created tool jobs.
- Huawei package sync result JSON now includes Rust/V1-style `tool`, `input`,
  `obs`, `gaussdb`, `sql`, `timings`, `warnings`, `credentialMetadata`,
  logical `evidenceRefs`, and `createdAt` fields while retaining the existing
  V2 `obsPut`, `obsHead`, `gaussdbUpdate`, and `gaussdbQuery` fields.
- GaussDB DSN metadata is summarized without writing the password to the
  result artifact; truncated query output records a warning.
- Added a regression that monkeypatches OBS/GaussDB calls, verifies call order,
  default object-key generation, V1 result fields, secret redaction, and
  background-only evidence.
- Verification passed: focused Huawei result regression,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (112 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Approval Decision Idempotency

- `POST /api/v2/actions/:action_id/decisions` now rejects non-
  `waiting_for_approval` runs with 409 and requires the target action to be a
  pending approval action.
- Approval decisions now persist optional `idempotencyKey` in the action result
  and decision timeline event. Repeated submissions with the same key return
  the original event without recording another decision or enqueueing another
  job.
- Added API regression coverage for non-waiting rejection, successful approval
  requeue, and duplicate idempotency handling.
- Verification passed: focused approval decision idempotency regressions, ruff
  for `server-v2/logagent_v2` and `server-v2/tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (111 passed, 1 warning),
  compileall for `server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 User Message Resume Idempotency

- `POST /api/v2/runs/:run_id/messages` now matches Rust waiting semantics more
  closely: it rejects non-`waiting_for_user` runs with 409, validates optional
  `questionId` against pending `user_input` actions, and supports
  `idempotencyKey` retry de-duplication.
- Task MCP `logagent.request_user_input` now accepts optional `questionId` and
  stores it in the pending action payload. User messages with a matching
  `questionId` answer only that pending action.
- Agent resume context now carries user-message `questionId` and
  `idempotencyKey` alongside message text and `resumeMode`.
- Added API regression coverage for non-waiting rejection, unknown question
  rejection, successful answer/requeue, and duplicate idempotency handling.
- Verification passed: focused user-message resume idempotency regressions,
  ruff for `server-v2/logagent_v2` and `server-v2/tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (110 passed, 1 warning),
  compileall for `server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Agent Follow-up Evidence Refs

- V2 Agent provider requests now merge evidence refs discovered in prior tool
  observations into the next round's `allowedEvidenceRefs`.
- The extractor covers `evidenceRefs`, `finalEvidenceRefs`, nested match
  `ref`, and `evidenceRef` fields, with stable de-duplication after the initial
  Session question and grep refs.
- Added regression coverage that an OpenAI-compatible provider can request
  `logagent.search_logs`, receive a `log_searches/...#matches/0` ref, and see
  that ref in the second-round prompt's `allowedEvidenceRefs`.
- Verification passed: focused Agent follow-up evidence regression, ruff for
  `server-v2/logagent_v2` and `server-v2/tests`,
  `cd server-v2 && .venv/bin/python -m pytest`, compileall for
  `server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Claude Runtime Artifact Compatibility

- V2 task MCP now advertises optional Rust/V1 Claude runtime resources
  `claude_mcp_config` and `claude_session` on both `logagent://task/...` and
  `logagent-v2://run/...` URI schemes.
- `GET /api/v2/runs/:run_id/artifacts` now includes
  `claudeMcpConfigPath` / `claudeMcpConfig` and `claudeSessionPath` /
  `claudeSession` when matching evidence artifacts exist; current Python V2
  Agent runs still do not generate Claude runtime artifacts.
- Added regression coverage by writing simulated Claude config/session evidence
  artifacts, reading one through task MCP, and checking the HTTP aggregate
  response.
- Verification passed: focused Claude runtime artifact compatibility
  regression, ruff for `server-v2/logagent_v2` and `server-v2/tests`,
  `cd server-v2 && .venv/bin/python -m pytest`, compileall for
  `server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Run Artifact Aggregate Response

- `GET /api/v2/runs/:run_id/artifacts` now keeps the raw V2 `run`, `uploads`,
  and `evidenceArtifacts` lists while adding Rust/V1-style aggregate fields.
- The response includes `taskId`, `artifactIndex`, parsed manifest and grep
  results, Session text input, metadata/system/case context, analysis package,
  Agent response/state artifacts, MCP call audit entries, and tool results.
- Added regression coverage for the aggregate response on a completed
  Session-backed run with text input, manifest, grep, analysis package, Agent
  audit artifacts, and recorded MCP calls.
- Verification passed: focused run artifact aggregate regression,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest`, compileall for
  `server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Session System Context Materialization

- V2 run `system_context.json` now materializes explicit Session
  `systemContextIds` from legacy System Context resources alongside
  Skill-backed Diagnostic Skills.
- The artifact keeps existing Skill `resources`, adds `skillResources`,
  `systemResources`, and the rendered legacy System Context prompt.
- `analysis_package.systemContext` now includes `systemResourceCount` and
  bounded `systemResources` summaries so the Agent can see selected runbooks,
  architecture docs, glossaries, and similar legacy context.
- Added regression coverage for a Session-bound runbook appearing in both task
  MCP `system_context` and `analysis_package`.
- Verification passed: focused Session System Context materialization
  regression, `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (109 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Session Metadata Binding Propagation

- V2 Metadata context generation now honors explicit Session `instanceId` /
  `nodeId` binding before auto-selecting metadata instances from the question.
- Bound metadata resources are marked with `selectionReason=session_binding`,
  and `metadata_context.json` records `boundInstanceId` and `boundNodeId` in
  its selection block.
- `analysis_package.json` now carries Session title, source URL, instance/node
  binding, System Context ids, Skill ids, and attached upload ids in the
  `workspace` section so the Agent receives the product-level Session context.
- Added regression coverage where the question matches one metadata instance
  but the Session binding selects another.
- Verification passed: focused Metadata context binding regression,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (108 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Session Task Summaries

- V2 Session task APIs now return product-level TaskSummary fields instead of
  only exposing raw Run rows.
- `POST /api/v2/sessions/:session_id/tasks` returns top-level `taskId`,
  `runId`, `taskKind=log_analysis`, `sessionId`, `analysisMode`,
  `analysisLanguage`, upper-case task `status`, `phase`, `url`, and the summary
  under `task`, while retaining the raw V2 Run under `run`.
- `GET /api/v2/sessions/:session_id/tasks` returns summary objects in `tasks`
  and keeps raw V2 Run records in `runs` for diagnostics and V2-native clients.
- Verification passed: focused Session alias task summary regression,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (108 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Session Upload Attachments

- V2 Workspaces now persist Session `uploadIds` as an attachment set instead
  of deriving Session uploads from every Upload row under the Workspace.
- Direct Workspace/Session uploads and completed chunked uploads auto-attach
  the new upload id; existing databases are backfilled from current Workspace
  uploads during SQLite initialization.
- `POST /api/v2/sessions/:session_id/uploads` now supports both one multipart
  `file` direct upload and JSON `{"uploadIds":[...]}` attachment.
- Added `DELETE /api/v2/sessions/:session_id/uploads/:upload_id` to detach an
  upload before any task run exists. Detached uploads and artifacts remain
  stored, but no longer appear in Session `uploadIds`, Session upload lists, or
  initial analysis evidence.
- Verification passed: focused Session alias upload attachment regression,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (108 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Session Field Persistence

- V2 Workspaces now persist Rust-style Session fields used by the Session-first
  API: `title`, `sourceUrl`, `instanceId`, `nodeId`, `systemContextIds`,
  `skillIds`, `analysisLanguage`, and draft/ready Session status.
- Existing SQLite databases are migrated in place with additive Workspace
  columns, and old records derive `title` from `question` when the title column
  is empty.
- Session PATCH now supports updating persisted title/question/source/metadata
  binding/context fields, clearing source/instance/node fields with JSON null,
  and manually setting Session status to `draft` or `ready`.
- Session status now follows the Rust product model more closely: uploads move
  a draft Session to `ready`, queued Runs show Session `ready`, and run status
  changes update the backing Workspace timestamp/status.
- Verification passed: focused Workspace/Session alias regressions,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (108 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Session Alias API

- Added Session-first HTTP aliases under `/api/v2/sessions` while keeping the
  V2 internal Workspace/Run model unchanged.
- `sessionId` maps to the Workspace id, `taskId` maps to the Run id, and
  Session records expose `uploadIds`, `taskIds`, `activeTaskId`,
  `analysisLanguage`, status, counts, and the backing Workspace payload.
- Added aliases for Session CRUD, Session uploads, batch uploads, restartable
  upload init/list, task creation/listing, and workspace-level Session timeline
  events.
- Session deletion now rejects unfinished tasks on the alias surface, matching
  the Rust/WebUI expectation that active analysis sessions cannot be deleted.
- Added API regression coverage for create/update/upload/task/timeline/delete
  Session alias behavior.
- Verification passed: focused Session alias API regression,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (108 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Built-in Tool Surface Coverage

- Added regression coverage that locks V1 built-in tool names across V2 task
  MCP, readonly MCP, and the manual Tools catalog.
- The coverage includes task tools for log search/slice, domain tool execution,
  user input, approval, metadata topology/query/catalog, case recall, skills,
  system context preview, and Fetch; readonly tools for tool/domain/metadata/
  case/skill catalogs; and manual catalog tools for preprocess, pprof,
  metadata, Fetch, and Huawei package sync.
- Updated V2 Server docs/spec.
- Verification passed: focused built-in tool surface coverage,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (107 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Tool Config V1 Compatibility

- `LOGAGENT_V2_TOOLS_JSON` now accepts both descriptor arrays and Rust/V1-style
  object maps keyed by tool id.
- Configured tool descriptors now accept V2 `command` plus V1 `path`,
  `path_env` / `pathEnv`, and camelCase or snake_case limit fields such as
  `timeoutSeconds` / `timeout_seconds`, `maxOutputBytes` / `max_output_bytes`,
  and `maxInputFiles` / `max_input_files`.
- Disabled descriptors do not read `path_env`; enabled descriptors still fail
  startup when their resolved command is missing or not absolute.
- Added regression coverage for map-shaped tool config, `path_env`,
  snake_case fields, disabled `path_env`, and missing enabled `path_env`.
- Updated V2 Server and Tool Runner docs/specs.
- Verification passed: focused `parse_tools_env` regressions,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (106 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Configured Tool Result Parity

- V2 configured subprocess `result.json` now uses the Rust/V1
  `ToolRunRecord` shape with `schemaVersion=2`, `tool`, `status`,
  `durationMs`, `command`, `stdoutPath`, `stderrPath`, and `error`, while
  preserving additive V2 fields such as `toolId`, `argv`, `stdoutPreview`,
  `stderrPreview`, and `parsedStdout`.
- Non-zero exits now fall back to `tool <id> exited with non-zero status`,
  spawn failures return a persisted `FAILED` tool result, and timeout records
  use the V1-style `TIMED_OUT` status and summary.
- Added regression coverage for non-zero result shape, subprocess spawn
  failure records, and timeout records.
- Updated V2 Server and Tool Runner docs/specs.
- Verification passed: focused configured-tool result regressions,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (104 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Configured Tool Workspace Parity

- V2 configured subprocess tools now run with `cwd` set to a per-action
  materialized tool workspace under `data_dir/tmp/tool_workspaces/...`.
- The workspace view copies the current run's `manifest.json`,
  `grep_results.json`, and optional `tool_inputs/index.json`, then expands
  Rust/V1 placeholders `{workspace}`, `{manifest_path}`, `{grep_results_path}`,
  `{action_id}`, `{input_file}`, and `{params.name}`.
- Unsupported placeholder-like tokens such as `{unknown}` now fail before
  subprocess execution.
- Added regression coverage for workspace/cwd placeholder execution and
  unknown placeholder rejection.
- Updated V2 Server and Tool Runner docs/specs.
- Verification passed: focused configured-tool workspace regressions,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (101 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Pprof Command Arg Parity

- V2 `pprof_analyzer` now builds `go tool pprof` argv like Rust/V1:
  top/tree/svg pass `-nodecount=<nodeCount>`, and top/tree/raw/svg all pass
  `-symbolize=none`.
- The pprof regression fake Go executable now verifies these flags for top,
  tree, raw, and SVG calls, including that raw does not receive nodecount.
- Updated V2 Server and Tool Runner docs/specs.
- Verification passed: focused pprof result regression,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (99 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Pprof Param Type Parity

- V2 pprof parameter validation now matches Rust/V1 serde semantics more
  closely: `sampleIndex` must be a string, `null` is rejected, and
  `generateSvg` must be a JSON boolean instead of any truthy value.
- This prevents `"false"` from enabling SVG generation and keeps direct
  DB-backed tool-run execution on the same validation path as the API.
- Added regression coverage for invalid `sampleIndex: null` and
  `generateSvg: "false"` inputs.
- Updated V2 Server and Tool Runner docs/specs.
- Verification passed: focused pprof validation regressions,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (99 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 InfluxQL Report Detection Parity

- V2 Tool Runner stdout parsing now recognizes InfluxQL analyzer Report JSON
  with the Rust/V1 key-presence rule: `total_records`, `total_statements`, and
  `fingerprints` keys are sufficient to enter the specialized parser.
- Non-array `fingerprints` values still skip fingerprint findings, but no
  longer cause the report to fall back to generic JSON summary parsing.
- Added regression coverage for `fingerprints: null` report payloads.
- Updated V2 Server and Tool Runner docs/specs.
- Verification passed: focused InfluxQL stdout parser regressions,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (99 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Pprof Tool Params Parity

- V2 `pprof_analyzer` descriptor now exposes Rust/V1 top-level
  `paramsSchema.sampleIndex`, `paramsSchema.nodeCount`, and
  `paramsSchema.generateSvg` entries while preserving the V2
  `paramsSchema.properties` mirror.
- Pprof parameter normalization now matches Rust/V1: `sampleIndex` is trimmed,
  rejects empty values and characters outside letters, digits, `_`, and `-`,
  and `nodeCount` still clamps to 1..200.
- Direct pprof tool-run execution reuses the same normalization so DB-backed
  runs cannot bypass the API validation path.
- Added regression coverage for descriptor shape and invalid sample indexes.
- Updated V2 Server and Tool Runner docs/specs.
- Verification passed: focused pprof descriptor/result regressions,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (98 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Configured Tool Descriptor Parity

- V2 configured subprocess tool descriptors now expose Rust/V1 read-only
  `paramsSchema.configuredArgs` and `paramsSchema.match` entries, including
  configured argv templates, match file patterns, and keywords.
- The same read-only entries are mirrored under `paramsSchema.properties` so V2
  schema-oriented clients can render them alongside reserved `inputFiles` and
  custom params without changing execution or validation behavior.
- Added regression coverage for `/api/v2/tools` descriptors, readonly MCP tool
  catalog output, and custom-params configured tools.
- Updated V2 Server and Tool Runner docs/specs.
- Verification passed: focused configured-tool descriptor regressions,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (98 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Metadata Descriptor Catalog Parity

- V2 Tools catalog metadata built-ins now match the Rust/V1 descriptor shape:
  `backend=builtin`, tags include `read-only` and `manual-run`, and display
  names/descriptions use the V1 wording.
- `logagent.get_metadata_field_types` now advertises the V1 params template
  with `retentionPolicy` and `field=[]`; `logagent.get_metadata_tag_fields`
  advertises `retentionPolicy` without `field`.
- Execution behavior is unchanged; this aligns the catalog surface consumed by
  WebUI, readonly MCP, and tool listing clients.
- Added regression coverage for metadata descriptor fields and templates.
- Updated V2 Server, Metadata, and Tool Runner docs/specs.
- Verification passed: focused tool registry regression,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (98 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Preprocess Result Parity

- V2 `logagent.preprocess_log_package` now exposes Rust/V1-style preprocess
  catalog metadata for rotated log normalization and
  `outputViews=["summary", "nodes", "log_groups", "tool_inputs", "warnings"]`.
- Manual preprocess results now include a V1-style `nodes` aggregation with
  package count, instance IDs, timestamps, per-log-group file counts,
  compressed count placeholders, ignored file count, and warnings, while
  retaining existing V2 `nodePackages`, `logGroups`, and artifact IDs.
- Added regression coverage for preprocess descriptor fields and node-package
  result aggregation.
- Updated V2 Server and Tool Runner docs/specs.
- Verification passed: focused preprocess/tool registry regressions,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (98 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Fetch Catalog Descriptor Parity

- V2 `logagent.fetch` catalog metadata now matches the Rust/V1 manual-run
  descriptor shape while keeping the V2 runtime-compatible `endpointId`
  parameter path.
- The descriptor now uses the browser DevTools cURL description, includes the
  `manual-run` tag, marks `readOnly=false`, exposes a V1-style
  `paramsTemplate` with `fetchId` and `body=null`, and advertises
  `outputViews=["summary", "request", "response", "body_artifact"]`.
- Runtime validation now reports that either `endpointId` or `fetchId` is
  required.
- Added regression coverage for Fetch descriptor fields and missing ID
  validation.
- Updated V2 Server and Tool Runner docs/specs.
- Verification passed: focused Fetch/tool registry regressions,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (98 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Huawei Descriptor Catalog Parity

- V2 `logagent.huawei_cloud_package_sync` catalog metadata now matches the
  Rust/V1 descriptor beyond upload suffixes: display name is
  `Huawei OBS + GaussDB Package Sync`, tags include `huawei-cloud`, and
  `outputViews` is `["summary", "obs", "gaussdb", "json"]`.
- Execution behavior is unchanged: the tool remains default-off and still
  requires exactly one completed upload plus validated object-key / SQL params.
- Added regression coverage for the catalog descriptor fields.
- Updated V2 Server and Tool Runner docs/specs.
- Verification passed: focused tool registry regression,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (98 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Metadata Field Param Parity

- V2 metadata field filters now match Rust/V1 across Tools API, readonly MCP,
  and task MCP paths.
- `logagent.get_metadata_field_types` trims string `field` values, treats a
  blank string as omitted, and rejects array entries that are not non-empty
  strings after trim.
- `logagent.get_metadata_tag_fields` now rejects the unsupported `field`
  parameter on direct MCP calls instead of silently applying it.
- Added regression coverage for Tools API parameter normalization, invalid
  field arrays, readonly MCP tag-field rejection, and Metadata query filtering.
- Updated V2 Server and Metadata docs/specs.
- Verification passed: focused tool registry and metadata MCP regressions,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (98 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Remote Command Argv Normalization

- V2 Remote Executor command templates now normalize `argv` while loading
  `LOGAGENT_V2_REMOTE_COMMANDS_JSON`, matching Rust/V1
  `remote_execution.commands.<command_id>.argv`.
- Each argv entry is trimmed, empty entries are dropped, and the normalized
  argv must still contain at least one item before it can enter API
  descriptors, queued jobs, or SSH execution.
- Added regression coverage for trimmed argv preservation and all-empty argv
  rejection.
- Updated V2 Server, Environment Collector, and Configuration docs/specs.
- Verification passed: focused remote command argv normalization regressions,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (98 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Numeric Settings Clamp Parity

- V2 now clamps non-positive `LOGAGENT_V2_MAX_CONCURRENT_JOBS` to 1, matching
  the Rust/V1 fail-safe worker concurrency boundary.
- V2 Fetch runtime limits now clamp timeout, request bytes, and response bytes
  to at least 1, while redirect count continues to clamp negative values to 0.
- Added regression coverage for environment-loaded non-positive job and Fetch
  numeric settings.
- Updated V2 Server and Configuration docs/specs.
- Verification passed: focused numeric settings clamp regression,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (97 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 pprof Analyzer Config Boundary

- V2 `Settings.from_env` now treats `pprof_analyzer` like a Rust/V1 configured
  command: it is disabled by default unless `LOGAGENT_V2_PPROF_GO_COMMAND` or
  `LOGAGENT_TOOL_PPROF_GO` is configured, or `LOGAGENT_V2_PPROF_ENABLED=1` is
  paired with a configured command.
- When pprof is enabled, the Go command expands environment variables and `~`
  and must resolve to an absolute path during settings loading.
- Runtime/export checks still decide whether the absolute path exists and is
  packageable, preserving existing skipped/export diagnostics.
- Added regression coverage for default-disabled pprof, missing enabled
  command, relative enabled command rejection, disabled relative tolerance, and
  env-expanded absolute commands.
- Updated V2 Server, Tool Runner, and Configuration docs/specs.
- Verification passed: focused pprof config regressions,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (96 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Agent Provider Config Validation

- V2 now validates environment-loaded Agent provider settings during
  `Settings.from_env`, matching Rust/V1's fail-fast provider boundary.
- `LOGAGENT_V2_AGENT_PROVIDER` is normalized and restricted to `stub`,
  `openai_compatible`, or `binary`.
- `openai_compatible` now requires non-empty base URL, model, and API key at
  startup; `binary` requires `LOGAGENT_V2_AGENT_BINARY_PATH` and the path must
  resolve to an absolute path. Runtime diagnostics still report file existence
  and executable-bit problems.
- Added regression coverage for unsupported provider values, missing OpenAI
  fields, missing/relative binary path, and valid normalized settings.
- Updated V2 Server and Configuration docs/specs.
- Verification passed: focused Agent provider config regressions,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (95 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Huawei Package Sync Config Validation

- V2 now parses Huawei package sync settings through a validation layer instead
  of passing raw environment variables directly into the built-in tool
  descriptor.
- When `LOGAGENT_V2_HUAWEI_PACKAGE_SYNC_ENABLED=1`, settings loading validates
  OBS endpoint scheme and shape, OBS bucket characters, safe object prefix,
  required trimmed OBS access/secret keys, optional trimmed security token, and
  required trimmed `LOGAGENT_V2_HUAWEI_GAUSSDB_DSN`.
- The built-in `logagent.huawei_cloud_package_sync` descriptor only becomes
  runnable after that startup validation has succeeded.
- Added regression coverage for missing endpoint, endpoint path rejection,
  bucket character rejection, trimming/normalization, timeout clamping, and
  runnable descriptor state.
- Updated V2 Server, Tool Runner, and Configuration docs/specs.
- Verification passed: focused Huawei package sync config regressions,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (94 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Fetch Secret Key Startup Validation

- V2 now validates `LOGAGENT_V2_FETCH_SECRET_KEY` during settings loading when
  Fetch is enabled, matching Rust/V1's enabled-Fetch configuration boundary.
- The key is trimmed, must be base64/URL-safe-base64 text, and must decode to
  exactly 32 bytes before Fetch endpoints or runs can be exposed as runnable.
- Fetch-disabled settings still do not require the key; sensitive endpoint
  writes continue to use the same key for credential encryption.
- Added regression coverage for missing, invalid-base64, wrong-length, and
  valid Fetch secret keys.
- Updated V2 Server, Tool Runner, and Configuration docs/specs.
- Verification passed: focused Fetch secret-key config regressions,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (93 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Fetch Allowlist Config Parity

- V2 now validates `LOGAGENT_V2_FETCH_ALLOWED_HOSTS` during settings loading:
  when Fetch is enabled the allowlist must be non-empty.
- Fetch allowlist entries now match Rust/V1 forms: exact `host`,
  `host:port`, or scheme-specific `http(s)://host[:port]`. URL-form entries
  pin both scheme and port, using the default port when omitted.
- Runtime allowlist checks now understand the normalized scheme-specific form
  while preserving existing exact host and host:port matching for direct
  `Settings(...)` construction.
- Added regression coverage for empty enabled allowlists and scheme-specific
  allowlist matching.
- Updated V2 Server, Tool Runner, and Configuration docs/specs.
- Verification passed: focused Fetch allowlist config regressions,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (92 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Tool Match Normalization

- V2 now normalizes `LOGAGENT_V2_TOOLS_JSON.match.filePatterns` and
  `match.keywords` to lowercase while loading configured subprocess tools,
  matching Rust/V1 `tools.<name>.match` catalog behavior.
- HTTP/MCP Tool Plugin descriptors now expose normalized match metadata, while
  execution keeps its existing case-insensitive file and keyword matching.
- Added regression coverage for uppercase configured match values.
- Updated V2 Server, Tool Runner, and Configuration docs/specs.
- Verification passed: focused configured tool match normalization regressions,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (90 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Configured Tool ID Validation

- V2 now validates `LOGAGENT_V2_TOOLS_JSON.id` while loading configured
  subprocess tools, matching Rust/V1 `tools.<name>` validation.
- Accepted configured tool IDs are non-empty ASCII letters, digits, `_`, and
  `-` only. Invalid IDs such as empty values, spaces, slashes, or dots now fail
  configuration parsing before entering the Tool Plugin registry, MCP catalog,
  manual tool runs, exports, or artifact path derivation.
- Built-in `logagent.*` tools remain fixed server capabilities outside the
  user-configured tool namespace.
- Added regression coverage for invalid configured tool IDs.
- Updated V2 Server, Tool Runner, and Configuration docs/specs.
- Verification passed: focused configured tool ID regressions,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (89 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Remote Command ID Validation

- V2 Remote Executor command templates now validate `commandId` while loading
  `LOGAGENT_V2_REMOTE_COMMANDS_JSON`, matching Rust/V1
  `remote_execution.commands.<command_id>`.
- Accepted IDs are non-empty ASCII letters, digits, `_`, and `-` only. Inputs
  such as empty IDs, spaces, slashes, or dots now fail configuration parsing
  before they can enter API descriptors, queued jobs, or artifact paths.
- Added regression coverage for valid and invalid command template IDs.
- Updated V2 Server, Environment Collector, and Configuration docs/specs.
- Verification passed: focused remote command template ID regressions,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (88 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Remote Host Key Policy Validation

- V2 Remote Executor now validates `LOGAGENT_V2_REMOTE_HOST_KEY_POLICY` during
  settings loading. Accepted values are exactly `accept-new`, `strict`, and
  `no`, matching Rust/V1 `remote_execution.host_key_policy`.
- Unknown values such as `off` now fail startup instead of silently falling back
  to `accept-new`. The execution-layer `StrictHostKeyChecking` mapper also
  rejects unknown values defensively.
- Added regression coverage for valid policy normalization and invalid policy
  rejection at both settings and execution mapping layers.
- Updated V2 Server and Environment Collector docs/specs.
- Verification passed: focused remote host-key policy regressions,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (87 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Remote SSH Command Boundary

- V2 Remote Executor now defaults `LOGAGENT_V2_REMOTE_SSH_COMMAND` to
  `/usr/bin/ssh` instead of PATH-resolved `ssh`.
- The configured SSH command expands environment variables and `~`; when remote
  execution is enabled it must resolve to an absolute path, matching the
  Rust/V1 `remote_execution.ssh_binary` boundary. Disabled remote execution may
  keep a relative value because it cannot be executed.
- Added Settings regression coverage for enabled relative command rejection and
  disabled relative command tolerance.
- Updated V2 Server and Environment Collector docs/specs.
- Verification passed: focused remote SSH command config regressions,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (85 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Fetch Redirect Policy Parity

- V2 Fetch endpoints now persist `followRedirects`, with SQLite migration for
  existing `fetch_endpoints` rows.
- Fetch execution no longer follows redirects by default. Imported cURL
  commands with `--location` or endpoints created/updated with
  `followRedirects=true` opt into bounded manual redirects with per-hop
  allowlist validation and sensitive-header stripping across origins.
- Fetch responses now treat only HTTP 2xx as `httpOk`, matching the Rust/V1
  result semantics for non-followed 3xx responses.
- Added regression coverage for default no-follow redirect behavior,
  opt-in redirect execution, blocked redirect allowlist validation, and
  `--location` import persistence.
- Updated V2 Server and Tool Runner docs/specs.
- Verification passed: focused Fetch redirect/import regressions,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (83 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Fetch cURL Prompt Import

- V2 Fetch cURL import now accepts copied bash commands with a leading `$`
  shell prompt, matching Rust/V1 import tolerance for terminal copy/paste.
- The importer still rejects unsupported flags and non-bash cURL forms; this
  change only strips the prompt before the existing parser runs.
- Added regression coverage in the Fetch cURL import test.
- Updated V2 Server and Tool Runner docs/specs.
- Verification passed: focused Fetch cURL import regression,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (83 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Tool Command Path Validation

- V2 now expands environment variables and `~` in `LOGAGENT_V2_TOOLS_JSON`
  command paths and `LOGAGENT_V2_TOOL_*_ANALYZER` source-built analyzer
  shortcuts during configuration loading.
- Enabled configured tools must resolve to absolute command paths before they
  enter the Tool Plugin registry, so HTTP catalogs, readonly MCP, tools.zip,
  manual tool runs, and task MCP no longer see a tool as runnable when it would
  later fail only at execution-time path validation.
- Disabled JSON tool descriptors may still keep relative commands; they remain
  non-runnable and non-exportable.
- Added regression coverage for command expansion, enabled relative command
  rejection, and source-built analyzer relative path rejection.
- Updated V2 Server and Tool Runner docs/specs.
- Verification passed: focused tool command config regressions,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (83 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Source-Built Analyzer Defaults

- Aligned V2's environment-variable analyzer auto-registration with the Rust/V1
  `examples/server-tools.yaml` defaults.
- `LOGAGENT_V2_TOOL_*_ANALYZER` now creates descriptors with the same args,
  timeouts, `maxInputFiles`, match patterns, and keywords: Flux/InfluxQL keep
  30s and 3 inputs, openGemini storage uses the full TSSP/TSI/mergeset pattern
  set and 10 inputs, and InfluxDB storage uses 60s and 5 inputs.
- Added regression coverage for env-based source-built analyzer defaults.
- Updated V2 Server, Tool Runner, and Deployment docs/specs.
- Verification passed: focused source-built analyzer config regression,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (80 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Manual Tool Upload Suffix Validation

- Manual V2 tool-run creation now validates attached upload filenames against
  each tool descriptor's `acceptedSuffixes` in addition to the existing upload
  count and params validation.
- The validation supports suffix descriptors such as `.tar.gz`, glob patterns
  such as `*.log`, and `*` for unrestricted single-upload built-ins; explicit
  `params.inputFiles` can still reuse existing workspace inputs without new
  uploads.
- Added regression coverage for API rejection of invalid
  `logagent.preprocess_log_package` uploads and for the preprocess tool's
  node-package execution path that materializes `influxql_analyzer` inputs.
- Updated V2 Server and Tool Runner docs/specs.
- Verification passed: focused preprocess/manual tool upload regressions,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (79 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Direct Fetch Tool Run API

- Added `POST /api/v2/fetch/endpoints/:endpoint_id/runs` to queue a Fetch
  `tool_run` directly from a saved endpoint, matching the Rust/V1 Fetch run
  entrypoint while preserving V2's workspace-backed run model.
- The endpoint validates Fetch configuration, endpoint state, and runtime
  params; it reuses a provided `workspaceId` or creates an isolated workspace
  when none is provided, then queues the standard DB-backed tool-run job.
- Updated V2 Server and Tool Runner docs/specs.
- Verification passed: focused Fetch endpoint run API regression,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (77 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Fetch Run History API

- Added `GET /api/v2/fetch/runs` as the V2 Fetch run-history endpoint for
  persisted `toolId=logagent.fetch` tool runs.
- The endpoint is read-only, returns the current Fetch enabled flag, and
  supports `endpointId`, `fetchId`, V1-style `fetch_id`, `workspaceId`, and
  `limit` filters without executing network requests.
- Updated V2 Server and Tool Runner docs/specs.
- Verification passed: focused Fetch run-history API regression,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (76 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Deploy Tool Build Shell Parity

- Updated `deploy/rebuild-v2-install.sh` to load `$HOME/.cargo/env` when
  present, matching the Rust rebuild script behavior for non-interactive SSH
  shells.
- This makes `--with-tools` / `--tools-only` able to find rustup-managed
  `cargo` when rebuilding the Flux analyzer through `scripts/build-tools.sh`.
- Updated Deploy and V2 Server docs/specs.
- Verification passed: `bash -n deploy/rebuild-v2-install.sh
  deploy/logagent-v2ctl.sh deploy/rebuild-install.sh deploy/logagentctl.sh
  scripts/build-tools.sh scripts/configure-tool-submodules.sh` and
  `git diff --check`.

## 2026-06-17 V2 Metadata Field Filter Schema Compatibility

- Aligned V2 Metadata field filter schemas with Rust/V1 across Tools catalog,
  readonly MCP, and task MCP: `field` now uses `oneOf` for a single string or a
  non-empty string array.
- Shared the schema between metadata MCP descriptors and built-in tool catalog
  descriptors to prevent future drift.
- Updated V2 Server and Metadata docs/specs.
- Verification passed: focused Metadata descriptor regressions,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (75 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Configured Tool Descriptor Compatibility

- Aligned regular V2 configured subprocess tool descriptors with the Rust/V1
  command catalog shape: `backend=command`, `readOnly=false`, `editable=true`,
  `exportable=enabled`, `minFiles=1`, and `acceptedSuffixes` copied directly
  from configured `match.filePatterns`.
- Preserved V2 params schema validation and controlled `params.inputFiles`
  selection while restoring the V1 manual-run upload-count contract.
- Updated V2 Server and Tool Runner docs/specs.
- Verification passed: focused configured tool descriptor/manual validation
  regressions, `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (75 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Case Import Draft Patch Compatibility

- Added `PATCH /api/v2/cases/imports/:import_id` to match the Rust/V1 Case
  import draft correction flow before confirmation.
- Added V2 Case Memory helper logic that updates unconfirmed drafts, normalizes
  editable fields, recomputes `validationErrors`, persists the draft in SQLite,
  and rejects edits after an import has been confirmed.
- Updated V2 Server and Case Store docs/specs.
- Verification passed: focused Case import draft patch regressions,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (75 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Remote Command Template Descriptor Compatibility

- Aligned V2 Remote Executor command template descriptors with Rust/V1:
  `enabled` now combines global remote execution state with template state, and
  `timeoutSeconds` is always populated from the template override or default
  remote command timeout.
- Kept remote command execution and queue behavior unchanged.
- Updated V2 Server and Environment Collector docs/specs.
- Verification passed: focused Remote Executor descriptor regressions,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (74 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Metadata Cluster and Snapshot Fetch APIs

- Added V2 Metadata cluster detail and cluster node routes derived from
  persisted instance snapshots: `/api/v2/metadata/clusters/:cluster_id` and
  `/api/v2/metadata/clusters/:cluster_id/nodes`.
- Added `/api/v2/metadata/snapshots/fetch` for direct remote snapshot fetch and
  normalization without creating an import draft or persisting an instance.
- Reused the existing V2 Fetch allowlist/redaction boundary for snapshot URL
  reads and kept snapshot normalization shared with import preview/confirm.
- Updated V2 Server and Metadata docs/specs.
- Verification passed: focused Metadata cluster/snapshot-fetch API regressions,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (73 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 pprof Catalog and Export Compatibility

- Aligned V2 `pprof_analyzer` catalog metadata with the Rust/V1 configured
  command shape: `source=configured`, `backend=command`, editable/exportable
  when runnable, and default `nodeCount=50`.
- Kept pprof manual-only for V2 tool runs so it does not appear in task MCP
  `logagent.run_domain_tool` configured subprocess enums.
- Extended `tools.zip` export to package the enabled pprof Go executable,
  wrapper, config example, and `tools-manifest.json` entry.
- Updated V2 Server and Tool Runner docs/specs.
- Verification passed: focused pprof/catalog/export/task-MCP regressions,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (72 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Domain Tool MCP Schema Compatibility

- Aligned V2 task MCP `logagent.run_domain_tool` descriptor with the
  Rust/V1 migration contract by advertising both `toolId` and `tool + inputFile`
  call shapes through `tools/list` `anyOf`.
- Kept the existing V2 `toolId` protocol and legacy `tool/inputFile` execution
  path unchanged; this change makes the callable schema match the already
  supported behavior.
- Updated V2 Server, Interfaces, and Tool Runner docs/specs.
- Verification passed: focused task MCP descriptor regression,
  `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (71 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Huawei Package Sync Descriptor

- Aligned V2 `logagent.huawei_cloud_package_sync` catalog descriptor with the
  Rust/V1 built-in tool shape by advertising `acceptedSuffixes=["*"]`.
- Execution still requires exactly one completed upload and validated
  `objectKey` / `updateSql` / `querySql` params; this change fixes catalog/WebUI
  behavior for arbitrary package filenames.
- Updated V2 Server and Tool Runner docs/specs.
- Verification passed: focused tool registry regression, `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (71 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 pprof Tool Result Paths

- Aligned V2 built-in `pprof_analyzer` manual tool results with the Rust/V1
  artifact path contract while preserving V2 artifact id mappings.
- pprof result JSON now includes parsed `profileType`, `total`, top table
  rows, `artifactIds`, and `artifactPaths` for logical
  `tool_results/<action_id>/{top.txt,tree.txt,raw.txt,stderr.txt,graph.svg}`.
- pprof status now matches the Rust/V1 behavior: top/tree/raw must all succeed
  for the run result to be `OK`; SVG remains optional.
- Added regression coverage with a fake `go tool pprof` executable so the
  contract does not depend on a local real profile.
- Updated V2 Server and Tool Runner docs/specs.
- Verification passed: focused pprof tool-run regression, `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (71 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Deploy Health Wait

- Aligned V2 deploy service control with the Rust server's quick start/stop
  behavior. `deploy/logagent-v2ctl.sh start` now waits for the configured
  health URL to succeed before returning.
- Startup now detects early process exit, enforces
  `LOGAGENT_V2_STARTUP_TIMEOUT_SECONDS`, removes stale pid files on failure,
  and returns a non-zero status when V2 is not ready.
- V2 service status/stop/restart are pid-file scoped by default to avoid
  controlling another runtime directory's V2 process; global process discovery
  remains opt-in via `LOGAGENT_V2_DISCOVER_PROCESS=1`.
- Added `LOGAGENT_V2_STARTUP_TIMEOUT_SECONDS` to deploy `.env.example` and
  updated V2 Server and Deployment docs/specs.
- Verification passed: `bash -n deploy/logagent-v2ctl.sh`,
  `bash -n deploy/rebuild-v2-install.sh`, isolated V2 status smoke,
  invalid timeout smoke, and `git diff --check`.

## 2026-06-17 V2 Skill Reference MCP Envelope

- Aligned V2 task MCP `logagent.get_skill_reference` with the Rust/V1
  background artifact envelope while preserving the existing content payload.
- Skill reference responses now use stable
  `skill_references/skill_ref_<hash>.json` logical paths and return
  `artifactPath`, `backgroundRef`, `canonicalRef`, `evidenceRefs`,
  `skillRevision`, reference metadata, `truncated`, and
  `finalEvidenceAllowed=false`.
- Persisted `skill_reference` evidence now records the same logical path and
  background ref for later resource aggregation.
- Updated V2 Server and Interfaces docs/specs.
- Verification passed: focused Skill reference MCP regression, `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (70 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Fetch Endpoint MCP Envelope

- Aligned V2 task MCP `logagent.list_fetch_endpoints` with the Rust/V1 Fetch
  contract. When Fetch execution is disabled it now returns a JSON-RPC error
  with `fetch is disabled by configuration`.
- When Fetch is enabled, the endpoint listing now includes `schemaVersion=1`,
  V1-compatible endpoint fields (`fetchId`, `urlTemplate`,
  `credentialVersion`) and `finalEvidenceAllowed=false` while keeping V2
  redacted endpoint preview fields.
- Updated V2 Server, Tool Runner, and Interfaces docs/specs.
- Verification passed: focused Fetch MCP regression, `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (70 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Task MCP Protocol Methods

- Added V1-compatible task MCP `ping` and `prompts/list` methods.
  `ping` returns an empty object and `prompts/list` returns an empty prompt
  list, matching the Rust task MCP stdio server behavior.
- Updated V2 Server and Interfaces docs/specs.
- Verification passed: focused task MCP protocol regression, `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (70 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Readonly MCP Protocol Methods

- Added V1-compatible readonly MCP `ping` and `prompts/list` methods.
  `ping` returns an empty object and `prompts/list` returns an empty prompt
  list, matching the Rust readonly MCP behavior.
- Updated V2 Server and Interfaces docs/specs.
- Verification passed: focused readonly MCP protocol regression, `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (70 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 MCP JSON-RPC Batch Support

- Added JSON-RPC batch array support to V2 readonly and task MCP handlers,
  matching the Rust/V1 readonly MCP behavior while preserving single-request
  handling.
- Updated FastAPI MCP endpoints to accept arbitrary JSON payloads instead of
  dict-only bodies so HTTP batch arrays are not rejected before reaching the
  handler.
- Added focused regression coverage for readonly and task MCP batch responses.
- Updated V2 Server and Interfaces docs/specs.
- Verification passed: focused MCP batch regression, `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (70 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Readonly MCP Dynamic Resources

- Aligned V2 readonly MCP `resources/list` with Rust/V1 discovery behavior:
  dynamic Skill resources and Metadata snapshot resources are now advertised in
  addition to static collection resources.
- Dynamic readonly resources are listed under both `logagent://...` and
  `logagent-v2://...` schemes, matching the existing dual-scheme
  `resources/read` support.
- Updated V2 Server and Interfaces docs/specs.
- Verification passed: focused readonly dynamic-resource regression, `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (70 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Waiting Tool Marker Compatibility

- Aligned V2 task MCP `logagent.request_user_input` and
  `logagent.request_approval` with the Rust/V1 waiting marker envelope while
  preserving V2 pending `actions`.
- Waiting tool calls now persist a background `mcp_waiting_request.json`
  artifact and return `artifactPath`, `runtimeStatus`, and
  `mcp_waiting_request.json#request` in `evidenceRefs`.
- `logagent.request_approval` now accepts the V1 shape where only `reason` is
  required, defaulting a missing `actionType` to `manual_approval`; explicit
  `evidenceRefs` are validated as a string array.
- Updated V2 Server, Interfaces, and Analysis Agent docs/specs.
- Verification passed: focused waiting-tool regression, `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (70 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Task MCP Tool Result Aliases

- Aligned V2 task MCP `logagent.run_domain_tool` with the Rust/V1 response
  envelope while preserving the V2 nested payload. Tool calls now return
  top-level `artifactPath`, `artifactPaths`, `summary`, and `evidenceRefs`
  based on logical `tool_results/<action_id>/result.json` paths.
- Added `finalEvidenceRefs` for finding-producing configured tools so Agent
  responses can cite `tool_results/<action_id>/result.json#findings/<index>`
  directly.
- Updated V2 Server, Tool Runner, and Interfaces docs/specs.
- Verification passed: focused Tool Runner MCP regression, `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (70 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Case Recall MCP Evidence Refs

- Aligned V2 task MCP `logagent.recall_cases` with Rust/V1 response fields by
  returning `artifactPath`, `caseCount`, and per-case `evidenceRefs`.
- Recall Case background evidence now stores the logical
  `case_recall/recall_<stable_id>.json` path and `backgroundRef` in its
  evidence payload.
- Updated Interfaces and Case Store README/SPEC docs.
- Added regression coverage for recall evidence refs and persisted logical path.
- Verification passed: `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (70 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Metadata Snapshot Envelope

- Aligned V2 readonly MCP and manual tool-run `logagent.get_metadata_snapshot`
  responses with the Rust/V1 `snapshot` envelope while preserving V2 top-level
  snapshot fields.
- Updated Interfaces and Metadata README/SPEC docs.
- Added regression coverage for readonly MCP and manual tool-run snapshot
  wrappers.
- Verification passed: `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (70 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Readonly MCP Skill Envelope

- Aligned V2 readonly MCP `logagent.get_skill` with the Rust/V1 response
  envelope by returning the indexed skill both at the top level and inside a
  `skill` wrapper.
- Updated Interfaces and Skills README/SPEC docs.
- Added regression coverage for the wrapped and preserved top-level fields.
- Verification passed: `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (70 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Task MCP Resource URI Aliases

- Added V1-compatible task MCP resource URI support for
  `logagent://task/<run_id>/<resource>` while retaining existing
  `logagent-v2://run/<run_id>/<resource>` URIs.
- `resources/list` now advertises both URI schemes for all task resources.
- `resources/read` now normalizes both task URI schemes, persists the same MCP
  call audit entry, and echoes the caller-provided URI in returned content.
- Updated V2 server, Interfaces, and Agent Backends README/SPEC docs.
- Added regression coverage for V1 `analysis_package` reads and dual URI
  advertisement.
- Verification passed: `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (70 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Readonly MCP System Context Preview

- Aligned V2 readonly MCP `logagent.preview_system_context` with the Rust/V1
  argument surface by accepting `skillIds`, `product`, `version`,
  `environment`, and `instanceId`.
- The readonly preview now combines legacy Skill preview resources with V2
  System Context and Metadata adapter resources, returning combined
  `resources`, split `skillResources` / `systemResources`, and a `prompt`
  preview without writing a task artifact.
- Updated V2 server, Interfaces, and System Context README/SPEC docs.
- Added regression coverage for readonly MCP schema exposure and
  product/instance-driven System Context + Metadata adapter preview.
- Verification passed: `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest` (70 passed, 1 warning),
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Readonly MCP Resource URI Aliases

- Added V1-compatible readonly MCP resource URI support for `logagent://...`
  while retaining existing `logagent-v2://...` URIs.
- `resources/list` now advertises both URI schemes for tools catalog, metadata
  instances, recent cases, skills, and domain adapters.
- `resources/read` now normalizes V1 URIs for static resources and dynamic
  `skills/<skill_id>` / `metadata/instances/<instance_id>/snapshot` reads while
  echoing the caller-provided URI in the MCP content.
- Updated V2 server, Interfaces, and Metadata README/SPEC docs.
- Added regression coverage for V1 tools catalog, metadata instance list,
  metadata snapshot, and domain adapter resource reads.
- Verification passed: `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest`,
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Metadata Field Tool MCP Compatibility

- Aligned V2 task MCP `logagent.get_metadata_field_types` and
  `logagent.get_metadata_tag_fields` with Rust/V1 response envelopes.
- Task field/tag queries now write stable background slices under
  `metadata_slices/field_types_<stable_id>.json` and
  `metadata_slices/tag_fields_<stable_id>.json`, and return `artifactPath`,
  `backgroundRef`, `evidenceRefs`, `finalEvidenceAllowed=false`, and a
  Rust/V1 `result` wrapper while preserving V2 top-level `fields`.
- Readonly MCP field/tag queries now also expose the `result` wrapper without
  writing task artifacts.
- Field/tag query results now include `defaultRetentionPolicyUsed` when
  resolving an omitted RP through the DB default RP.
- Updated V2 server, Interfaces, Analysis Agent, and Metadata README/SPEC docs.
- Added regression coverage for readonly wrapping, task field/tag artifact
  paths, background refs, evidence refs, and default RP reporting.
- Verification passed: `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest`,
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Task MCP Log Result Aliases

- Added Rust/V1-compatible top-level response fields to V2 task MCP
  `logagent.search_logs`: `artifactPath`, `totalMatches`, `keywordCounts`,
  `unmatchedKeywords`, `matches`, `evidenceRefs`, and `note`, while preserving
  the existing nested V2 `search` object.
- Added Rust/V1-compatible top-level response fields to V2 task MCP
  `logagent.get_log_slice`: `artifactPath`, `evidenceRefs`, and `lines`, while
  preserving the existing nested V2 `slice` object.
- Follow-up log search results now include `unmatchedKeywords` in the canonical
  search payload.
- Updated V2 server, Interfaces, Analysis Agent, and Log Analyzer README/SPEC
  docs.
- Added regression coverage for the top-level MCP response aliases.
- Verification passed: `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest`,
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Task MCP Log Slice Range Params

- Added V1-compatible `startLine` / `endLine` input support to V2 task MCP
  `logagent.get_log_slice` while preserving the existing `lineNumber` plus
  `before` / `after` center-line form.
- `logagent.get_log_slice` now rejects mixed center-line and range parameters,
  requires ordered ranges, and enforces the Rust-compatible
  `endLine - startLine <= 500` span limit.
- Range slices preserve Rust-like EOF behavior: out-of-file ranges return an
  empty `lines` array instead of clamping to the last line.
- Updated V2 server, Analysis Agent, and Interfaces README/SPEC docs.
- Added regression coverage for successful `startLine` / `endLine` slicing and
  mixed-parameter rejection.
- Verification passed: `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest`,
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Task MCP Search Max Matches

- Added V1-compatible optional `maxMatches` to V2 task MCP
  `logagent.search_logs`.
- Search calls now validate `maxMatches` as an integer and clamp it to 1..200
  before creating the follow-up `log_searches/<search_id>.json` artifact.
- Updated V2 server, Analysis Agent, and Interfaces README/SPEC docs.
- Added regression coverage that verifies `maxMatches=1` truncates a follow-up
  search result to one match.
- Verification passed: `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest`,
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Fetch Request Size Limit

- Added `LOGAGENT_V2_FETCH_MAX_REQUEST_BYTES` with a 1MiB default to match the
  Rust Fetch `max_request_bytes` safety boundary.
- V2 Fetch now rejects saved endpoint bodies and runtime body overrides before
  HTTP execution when their UTF-8 byte length exceeds the configured limit.
- Updated V2 server, Config, Deployment, deploy README/SPEC docs, and
  `deploy/.env.example` with the request-size boundary.
- Added regression coverage that verifies an oversized runtime body override
  returns a task MCP error and does not create Fetch evidence.
- Verification passed: `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest`,
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Fetch Runtime Overrides

- Added V1-compatible runtime params to V2 `logagent.fetch`: `endpointId` or
  `fetchId`, string-map `variables`, temporary string-map `headers`, and string
  `body` override.
- Fetch URL variables now replace `{name}` placeholders before HTTP allowlist
  validation; unresolved placeholders and controlled runtime headers are
  rejected.
- Fetch result artifacts now use schema v2 and include Rust-style top-level
  `httpOk`, `statusCode`, `redirectCount`, `finalUrl`, `truncated`,
  `credentialVersion`, and response body artifact references while preserving
  existing response preview fields.
- Each Fetch run now stores the bounded raw response body as a separate V2
  artifact and exposes both logical
  `tool_results/<action_id>/response_body.bin` and actual artifact id/path.
- Updated `server-v2` and Tool Runner README/SPEC docs and added regression
  coverage for `fetchId`, URL variables, temporary headers, body override, and
  response body artifact persistence.
- Verification passed: `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest`,
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Storage Analyzer Directory Inputs

- Added V2 directory artifact support for tool inputs without changing the
  SQLite artifact schema.
- Storage analyzer materialization now groups archive TSI/mergeset and
  `_series` trees into safe `tool_inputs/storage_dirs/...` directory artifacts
  when the matching analyzer is enabled.
- `logagent.run_domain_tool` can pass artifact-backed directories through the
  existing `{input_file}` argument path, so source-built storage analyzers can
  receive directory inputs.
- Added regression coverage for InfluxDB `_series` tar.gz bundles and
  openGemini TSI zip bundles.
- Verification passed: `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest`,
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Storage Analyzer Materialized Inputs

- Extended V2 `tool_inputs/index.json` generation beyond InfluxQL/Flux text
  query inputs to include file-level storage analyzer inputs.
- Direct uploads and supported archives now produce safe
  `tool_inputs/storage/...` entries for `.tssp`, `.tssp.init`, `.tsm`, `.tsi`,
  and `_series` files when the matching storage analyzer is enabled, with
  archive members persisted as bounded artifacts.
- `logagent.run_domain_tool` and manual configured tools now automatically
  prefer these storage materialized inputs before falling back to raw upload
  artifact matching.
- Updated Tool Runner and `server-v2` README/SPEC docs to describe file-level
  storage analyzer input materialization.
- Verification passed: `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest`,
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Case Context Final Evidence Refs

- Added V2 final-answer validation support for
  `case_context.json#cases/<index>` refs backed by the current run's
  `case_context` artifact.
- Added V1-compatible Case id alias normalization: model refs such as
  `case_<id>` or `历史案例 case_<id>` are converted to the matching
  `case_context.json#cases/<index>` canonical ref.
- Kept `case_context` evidence background-only while allowing only the
  canonical Case item refs as final evidence.
- Updated `analysis_package.json` final-evidence policy to include
  `case_context.json#cases/<index>`.
- Verification passed: `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest`,
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Session Text Input Evidence

- Added V2 `session_text_input.json` persistence for each analysis run's
  Workspace question, stored as final-allowed `user_question` evidence.
- `analysis_package.json` and Agent provider requests now include
  `session_text_input.json#question` in allowed evidence refs, before bounded
  log match refs.
- Final-answer validation now accepts `session_text_input.json#question` only
  when it resolves to the current run's final-allowed question artifact.
- `artifact_index` now naturally includes `session_text_input.json` through the
  evidence artifact index.
- Verification passed: `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest`,
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Task MCP Aggregate Resources

- Added V1-compatible V2 task MCP resources `artifact_index`, `case_context`,
  and `tool_results`.
- `artifact_index` now enumerates current run upload artifacts and evidence
  artifacts from the V2 Store, using stable logical paths such as
  `manifest.json`, `grep_results.json`, `mcp_calls.jsonl`, and
  `tool_results/<action_id>/result.json`.
- `case_context` returns the latest Case search/recall background artifact, or
  an empty background context when no Case tool has run.
- `tool_results` aggregates parsed `tool_result` and `fetch_result` artifacts
  for task MCP consumers while preserving canonical tool-result paths.
- Exposed these resources through `analysis_package.json` resource indexes and
  `GET /api/v2/runs/:run_id/analysis` resources.
- Verification passed: `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest`,
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 MCP Calls Audit Resource

- Added V2 `mcp_calls.jsonl` audit persistence for successful task MCP
  `resources/read` and `tools/call` requests.
- Each call record now includes schema version, generated call id, timestamp,
  tool/resource name, arguments, status, result payload, and extracted
  evidence/background refs.
- Exposed parsed MCP call history through task MCP
  `logagent-v2://run/<run_id>/mcp_calls`, `analysis_package.json` resource
  index, and `GET /api/v2/runs/:run_id/analysis` resources.
- Kept MCP call audits background-only (`final_allowed=false`) so they support
  WebUI/session inspection without becoming root-cause evidence.
- Added regression coverage for resource-read/tool-call auditing, evidence ref
  extraction, task MCP `mcp_calls` resource reads, and run analysis exposure.
- Verification passed: `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest`,
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Task MCP Compatibility Aliases

- Added V2 task MCP compatibility tools for Rust V1 names:
  `logagent.recall_cases`, `logagent.get_metadata_topology`, and
  `logagent.query_metadata`.
- `logagent.recall_cases` now recalls enabled Cases through the V2 Case Memory
  store and persists background-only `case_context` evidence.
- `logagent.get_metadata_topology` returns a run-scoped Metadata outline with
  section counts and query hints; `logagent.query_metadata` reads bounded
  section/filter/cursor slices from the run-selected SQLite metadata snapshots.
- Query Metadata slices persist background-only `metadata_slice` evidence with
  logical `metadata_slices/slice_<hash>.json#items` refs and remain excluded
  from final root-cause evidence refs.
- Agent provider prompts now advertise the task MCP alias tool surface, while
  readonly MCP remains global/catalog-only for Case and Metadata.
- Added regression coverage for task MCP tools/list, Metadata topology/query
  aliases, invalid Metadata filters, Case recall alias, and Agent available
  tool advertising.
- Verification passed: `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest`,
  `python3 -m compileall -q server-v2/logagent_v2`, and `git diff --check`.

## 2026-06-17 V2 Readonly Tool Catalog Shape

- Updated V2 readonly MCP `logagent-v2://tools/catalog` and
  `logagent.list_tools` to return the Rust-compatible catalog payload shape:
  `schemaVersion`, complete `tools` descriptors, and `configuredTools`
  summaries.
- `configuredTools` now includes configured args, timeout, `maxInputFiles`, and
  match rules for each configured subprocess tool while the readonly MCP surface
  remains catalog-only and cannot execute tools.
- Replaced the placeholder tools catalog resource description with the concrete
  configured/built-in catalog description.
- Added regression coverage for resource read and tool call parity.
- Verification passed: `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest`, `python3 -m compileall -q server-v2/logagent_v2`,
  and `git diff --check`.

## 2026-06-17 V2 Explicit Tool Inputs

- Added V2 configured Tool Runner support for explicit current-Workspace input
  selection through `params.inputFiles`.
- Task MCP `logagent.run_domain_tool` now accepts both V2 `toolId` and
  V1-compatible `tool` plus top-level `inputFile`, mapping legacy calls into
  the same safe `inputFiles` selector.
- Explicit tool inputs are workspace-relative only and resolve to known
  manifest text paths, `extracted/...` virtual paths, `tool_inputs/...` entries,
  or storage-upload artifacts where applicable; arbitrary local paths remain
  rejected.
- Configured tool descriptors now expose reserved `inputFiles` in
  `paramsSchema`/`paramsTemplate` when the tool args contain `{input_file}`.
- Added regression coverage for legacy task MCP `tool/inputFile` calls and
  manual tool runs using `params.inputFiles` without re-uploading.
- Verification passed: `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest`, `python3 -m compileall -q server-v2/logagent_v2`,
  and `git diff --check`.

## 2026-06-17 V2 Binary Agent Provider

- Added V2 `LOGAGENT_V2_AGENT_PROVIDER=binary` support for the Agent runtime,
  using fixed argv `<binary_path> run <prompt>` without shell expansion.
- Added `LOGAGENT_V2_AGENT_BINARY_PATH` and
  `LOGAGENT_V2_AGENT_BINARY_MAX_OUTPUT_BYTES` settings, with runtime and
  Settings dry-run validation for absolute, regular, executable provider
  paths.
- Extended Settings diagnostics so model listing, chat smoke tests, backend
  summaries, and backend dry-run checks work for the local binary provider
  without returning the configured binary path or secrets.
- Persisted binary provider request/response audit artifacts with bounded
  stdout/stderr previews, final-answer parsing, and validation status.
- Added regression coverage for binary provider analysis execution and Settings
  diagnostics using mock executable providers.
- Verification passed: `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest`, `python3 -m compileall -q server-v2/logagent_v2`,
  and `git diff --check`.

## 2026-06-17 V2 Remote Environment Evidence

- Extended V2 `collect_environment` approval handling to use Remote Executor
  when the approved action input includes an enabled `executorId` and
  whitelisted `commandId`.
- Remote environment collection now queues a `remote_command_run` with
  idempotency key `environment:<action_id>`, writes background-only
  `environment_evidence` after the remote command completes, and requeues the
  original analysis run with the new evidence.
- Invalid remote targets now write `REMOTE_REJECTED` background evidence instead
  of leaving the approved action half-applied.
- Kept the V1-compatible `MOCK` evidence path when no remote target is supplied.
- Added fake-ssh regression coverage for the approved remote environment
  collection flow and preserved the existing mock evidence coverage.
- Verification passed: `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest`, `python3 -m compileall -q server-v2/logagent_v2`,
  and `git diff --check`.

## 2026-06-17 V2 Legacy System Context Resources

- Added V2 `/api/v2/system-context/*` compatibility APIs for V1-style System
  Context resources: create/list/read/update, version create/update/activate,
  and prompt preview.
- Persisted compatibility resources in SQLite through the
  `system_context_resources` table while keeping Metadata instances exposed as
  read-only `meta_<instanceId>` adapter resources in list/preview responses.
- Kept V2 run-time System Context Skill-backed; compatibility resources are
  management/preview inputs in this slice and are not automatically injected
  into new analysis runs.
- Fixed the FastAPI protected-route auth dependency annotation so `/api/v2/*`
  routes no longer interpret the local auth alias as a required `_` query
  parameter.
- Added regression coverage for compatibility resource version activation,
  Metadata adapter preview, and HTTP route smoke coverage.
- Verification passed: `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest`, `python3 -m compileall -q server-v2/logagent_v2`,
  and `git diff --check`.

## 2026-06-17 V2 Run Alias Persistence

- Added deterministic fallback alias generation for successful V2 analysis
  runs, matching the Rust server behavior where completed runs have a short
  history/UI display title.
- Added `runs.alias` schema/migration support and persisted the alias atomically
  with the succeeded run status and final answer.
- Added regression coverage for alias normalization/fallback and successful
  run alias persistence/timeline payloads.
- Updated server-v2 README/SPEC to document the alias behavior. The V2 alias
  path currently uses the fallback summary/question rules and does not call a
  model-specific alias prompt.
- Verification passed: `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest`, `python3 -m compileall -q server-v2/logagent_v2`,
  and `git diff --check`.

## 2026-06-17 V2 Approved Environment Evidence

- Added V2 `collect_environment` approval handling parity with the Rust server:
  approving a pending action with `actionType=collect_environment` now writes a
  MOCK `environment_evidence` artifact and evidence row.
- Exposed the latest `environment_evidence` through run analysis resources and
  task MCP `logagent-v2://run/<run_id>/environment_evidence`.
- Included approved environment evidence as background context in
  `analysis_package.json` and the next Agent provider prompt while keeping it
  excluded from final evidence refs.
- Updated server-v2 and Environment Collector docs to distinguish the completed
  mock approval evidence path from the still-planned real SSH/SCP collector.
- Verification passed: `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest`, `python3 -m compileall -q server-v2/logagent_v2`,
  and `git diff --check`.

## 2026-06-17 V2 Tool Plugin Migration

- Added a V2 Tool Plugin registry shared by `/api/v2/tools`, readonly MCP tool
  catalog, manual tool runs, and task MCP configured tool execution.
- Added DB-backed V2 `tool_run` runs/jobs plus APIs for creating, listing, and
  reading tool run results/artifacts.
- Migrated V1 built-ins into V2 descriptors and execution paths: metadata
  tools, `logagent.preprocess_log_package`, `logagent.fetch`, `pprof_analyzer`,
  and default-off `logagent.huawei_cloud_package_sync`.
- Added V2 source-built analyzer environment variables and raw-upload fallback
  for openGemini/InfluxDB storage analyzers.
- Extended `deploy/rebuild-v2-install.sh` with `--with-tools`,
  `--tools-only`, and `--only-tool` for fast analyzer rebuilds.
- Added focused V2 regression coverage for the unified tool registry and
  DB-backed manual metadata tool runs.
- Verification passed: `cd server-v2 && .venv/bin/python -m ruff check logagent_v2 tests`,
  `cd server-v2 && .venv/bin/python -m pytest`, `python3 -m compileall -q server-v2/logagent_v2`,
  `bash -n deploy/rebuild-v2-install.sh deploy/logagent-v2ctl.sh`, and `git diff --check`.

## 2026-06-17 V2 Deploy Quick Controls

- Added `deploy/logagent-v2ctl.sh` for V2 start, stop, restart, status, and
  log tailing with the same `.env` loading pattern as the Rust deploy controls.
- Added `deploy/rebuild-v2-install.sh` to create the runtime virtualenv,
  install `server-v2`, initialize SQLite, build/sync WebUI static files, and
  restart V2 only when it was already running.
- Extended deploy environment examples plus Deployment and server-v2 docs for
  V2 runtime defaults: `server-v2/.venv`, `data-v2`, `webui/out`, and port
  `50993`.
- Verification passed: `bash -n deploy/logagent-v2ctl.sh deploy/rebuild-v2-install.sh deploy/logagentctl.sh deploy/rebuild-install.sh`,
  `deploy/rebuild-v2-install.sh --help`, `deploy/logagent-v2ctl.sh` usage
  exit-code check, and `git diff --check`.

## 2026-06-17 V2 Status Documentation Cleanup

- Updated server-v2 docs to reflect the implemented readonly MCP resources and
  tools instead of calling it a placeholder.
- Collapsed stale WebUI bridge "not yet" items into the remaining full WebUI V2
  cutover work.
- Verification passed: `git diff --check`.

## 2026-06-17 WebUI V2 Timeline Event Kind

- Fixed the V2 Analyze timeline event label to use the Python V2 backend
  `kind` field while retaining compatibility with legacy `event_type`.
- Updated V2 timeline TypeScript typing, WebUI docs, and `PROGRESS.md`.
- Verification passed: `cd webui && npm run lint`, `cd webui && npm run typecheck`,
  `cd webui && npm run build`, and `git diff --check`.

## 2026-06-17 WebUI V2 Analyze Case Save

- Added a V2 Analyze Case save panel for succeeded runs with final answers.
- Prefills title, symptom, root cause, solution, and evidence refs from the V2
  final answer, allows editing, and calls `POST /api/v2/runs/:run_id/case`.
- Added `confirmV2RunCase` to `webui/src/v2-api.ts`.
- Updated WebUI docs and `PROGRESS.md`.
- Verification passed: `cd webui && npm run lint`, `cd webui && npm run typecheck`,
  `cd webui && npm run build`, and `git diff --check`.

## 2026-06-17 V2 Startup Job Recovery

- Added `Store.recover_interrupted_jobs()` for DB-backed jobs left in `running`
  by a prior process.
- Requeues non-terminal V2 analysis runs immediately, appends `run.recovered`,
  and resets interrupted remote command runs to `QUEUED`.
- Marks stale jobs for succeeded/failed/waiting runs as completed instead of
  rerunning them.
- Wired recovery into FastAPI startup before the inline worker begins polling.
- Added regression coverage for interrupted analysis and remote command job
  recovery.
- Updated server-v2 docs and `PROGRESS.md`.
- Verification passed: `python3 -m compileall logagent_v2`,
  `PYTHONPATH=. python3 -m unittest discover tests`, and `git diff --check`.

## 2026-06-17 V2 Static WebUI Hosting

- Added V2 `GET /` static WebUI hosting from `webui/out`, configurable through
  `LOGAGENT_V2_WEBUI_DIR`.
- Added SPA fallback for non-API routes while preserving 404 behavior for
  unknown `/api/*` paths and missing static assets.
- Added regression coverage for root index serving, asset serving, SPA fallback,
  missing asset 404, and missing API route 404.
- Updated server-v2 docs and `PROGRESS.md`.
- Verification passed: `python3 -m compileall logagent_v2`,
  `PYTHONPATH=. python3 -m unittest discover tests`, and `git diff --check`.

## 2026-06-17 V2 Agent Resume Context

- Added bounded V2 Agent `interactionContext` built from recent user messages,
  answered/approved/rejected actions, and remaining pending actions.
- Updated `POST /api/v2/runs/:run_id/messages` to mark pending `user_input`
  actions as `answered` before requeueing waiting runs.
- Updated stub resume behavior so finalize-with-current-evidence clears the
  no-log missing-information blocker and records the latest user message in the
  final answer.
- Updated V2 API typings plus server-v2/WebUI docs.
- Verification passed: `python3 -m compileall logagent_v2`,
  `PYTHONPATH=. python3 -m unittest discover tests`, `cd webui && npm run lint`,
  `cd webui && npm run typecheck`, `cd webui && npm run build`, and
  `git diff --check`.

## 2026-06-17 V2 Analyze Waiting Actions

- Added V2 run analysis `actions` and `pendingActions` so the WebUI can render waiting user-input and approval states.
- Added V2 Analyze bridge controls for user supplements, finalize-with-current-evidence messages, and approve/reject action decisions.
- Added regression coverage for pending actions in `GET /api/v2/runs/:run_id/analysis`.
- Updated server-v2, WebUI docs, and `PROGRESS.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, `cd webui && npm run lint`, `cd webui && npm run typecheck`, `cd webui && npm run build`, and `git diff --check`.

## 2026-06-17 V2 Workspace Management

- Added V2 Workspace update and soft-delete APIs: `PATCH /api/v2/workspaces/:workspace_id` and `DELETE /api/v2/workspaces/:workspace_id`.
- Updated Workspace listing to hide `deleted` Workspaces while preserving existing uploads, runs, evidence, and artifacts by id; creating a new run on a deleted Workspace is rejected.
- Updated the V2 Analyze bridge to load selected Workspace question/mode, save selected Workspace edits, soft-delete selected Workspace history items, and save edits before running the selected Workspace.
- Updated server-v2, WebUI docs, and `PROGRESS.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, `cd webui && npm run lint`, `cd webui && npm run typecheck`, `cd webui && npm run build`, and `git diff --check`.

## 2026-06-17 V2 Metadata Instance Refresh

- Added `POST /api/v2/metadata/instances/:instance_id/refresh` to rebuild a V2 Metadata snapshot from the raw JSON already saved in SQLite.
- Added regression coverage that corrupts a normalized snapshot, refreshes from stored raw openGemini metadata, and verifies the node count is restored.
- Added a V2 Metadata bridge `Refresh raw` action for each imported instance and refreshed the displayed snapshot after success.
- Updated server-v2, WebUI, Metadata docs, and `PROGRESS.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, `cd webui && npm run lint`, `cd webui && npm run typecheck`, `cd webui && npm run build`, and `git diff --check`.

## 2026-06-17 V2 Case Import Messages

- Added V2 Case import follow-up messages through `POST /api/v2/cases/imports/:import_id/messages`, closing the Rust Memory multi-turn import gap.
- Persisted Case import message history in SQLite and re-parsed drafts after user supplements while keeping confirm blocked until required fields are complete.
- Updated the V2 Memory bridge to submit missing-field supplements, show import messages, and refresh the editable draft.
- Updated server-v2, WebUI, Case Store docs, and `PROGRESS.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, `cd webui && npm run lint`, `cd webui && npm run typecheck`, `cd webui && npm run build`, and `git diff --check`.

## 2026-06-17 WebUI V2 Executors Bridge

- Added a V2 Executors bridge panel at the top of the existing Tools / Executors page while preserving the Rust-compatible Executors flow below it.
- Extended `webui/src/v2-api.ts` with Python V2 executor CRUD, command template listing, remote run creation/list/detail, and result helpers.
- Added `webui/src/V2ExecutorsBridge.tsx` to manage V2 executors, select whitelisted command templates, create and poll V2 remote command runs, and inspect stdout/stderr/result previews and paths.
- Updated `webui/README.md`, `webui/SPEC.md`, and `PROGRESS.md`.
- Verification passed: `cd webui && npm run lint`, `cd webui && npm run typecheck`, `cd webui && npm run build`, and `git diff --check`.

## 2026-06-17 V2 Remote Executor Foundation

- Added V2 Remote Executor backend APIs for executor CRUD, whitelisted command template listing, remote command run creation/list/detail, and result reads.
- Extended V2 config with `LOGAGENT_V2_REMOTE_*` settings, default `smoke_ls_root` template, SQLite `remote_executors` / `remote_runs` tables, and DB-backed `remote_command_run` jobs.
- Added `server-v2/logagent_v2/remote_execution.py` to execute configured SSH argv with batch mode, connect timeout, host-key policy, bounded stdout/stderr capture, and persisted `result.json` / `stdout.txt` / `stderr.txt`.
- Updated `server-v2/README.md`, `server-v2/SPEC.md`, Environment Collector docs, and `PROGRESS.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, and `git diff --check`.

## 2026-06-17 WebUI V2 Settings Bridge

- Added a V2 Settings bridge panel at the top of the existing Settings page while preserving the Rust-compatible Settings diagnostics below it.
- Extended `webui/src/v2-api.ts` with Python V2 Settings, Agent backend diagnostics, Domain Adapter summaries, and response-content debug helpers.
- Added `webui/src/V2SettingsBridge.tsx` to inspect V2 Agent provider settings, run model list/message tests, dry-run the V2 Agent backend, toggle V2 LLM debug logging, display V2 Domain Adapters, copy readonly MCP config, and download V2 `skills.zip` / `tools.zip`.
- Updated `webui/README.md`, `webui/SPEC.md`, and `PROGRESS.md`.
- Verification passed: `cd webui && npm run lint`, `cd webui && npm run typecheck`, `cd webui && npm run build`, and `git diff --check`.

## 2026-06-17 V2 Settings Diagnostics

- Added V2 Settings and diagnostics endpoints under `/api/v2/settings/*` plus `/api/v2/debug/llm`.
- Added `server-v2/logagent_v2/settings_api.py` for V2 Agent provider summaries, model list/chat connectivity tests, in-process Agent backend dry-run diagnostics, and built-in Domain Adapter summaries.
- Added `LOGAGENT_V2_AGENT_MAX_OUTPUT_TOKENS` and wired it into OpenAI-compatible Agent provider requests.
- Added response-content-only provider debug logging and exposed Domain Adapter summaries through readonly MCP `logagent-v2://domain-adapters` and `logagent.list_domain_adapters`.
- Updated `server-v2/README.md`, `server-v2/SPEC.md`, and `PROGRESS.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, and `git diff --check`.

## 2026-06-17 WebUI V2 Metadata Bridge

- Added a V2 Metadata bridge panel under the V2 System Context bridge while preserving the Rust-compatible Metadata Dashboard.
- Extended `webui/src/v2-api.ts` with Python V2 Metadata import preview/fetch preview/confirm/direct import, import listing, instance delete, and snapshot helpers.
- Added `webui/src/V2MetadataBridge.tsx` to preview/confirm/direct-import JSON/YAML/openGemini metadata from content or URL, list import drafts, manage V2 instances, delete instances, and inspect snapshot JSON.
- Updated `webui/README.md`, `webui/SPEC.md`, and `PROGRESS.md`.
- Verification passed: `cd webui && npm run lint`, `cd webui && npm run typecheck`, `cd webui && npm run build`, and `git diff --check`.

## 2026-06-17 WebUI V2 Fetch Bridge

- Added a V2 Fetch bridge panel at the top of the existing Tools / Fetch page while preserving the Rust-compatible Fetch endpoint and tool-run flow below it.
- Extended `webui/src/v2-api.ts` with Python V2 Fetch endpoint listing, cURL preview/import, endpoint update/delete, and run-scoped fetch execution helpers.
- Added `webui/src/V2FetchBridge.tsx` to preview/import cURL commands, manage V2 Fetch endpoints, show redacted sensitive-field previews, run a Fetch endpoint inside a V2 run, and display result/evidence/artifact output.
- Updated `webui/README.md`, `webui/SPEC.md`, and `PROGRESS.md`.
- Verification passed: `cd webui && npm run lint`, `cd webui && npm run typecheck`, `cd webui && npm run build`, and `git diff --check`.

## 2026-06-17 WebUI V2 System Context Bridge

- Added a V2 System Context bridge panel at the top of the existing System Context page while preserving the Rust-compatible Skills and Metadata tabs below it.
- Extended `webui/src/v2-api.ts` with Python V2 Skills list/detail/import/preview, Metadata instance listing, and `skills.zip` download helpers.
- Added `webui/src/V2SystemContextBridge.tsx` to inspect V2 Diagnostic Skills, import Markdown Skills, preview explicit System Context resources, download `/api/v2/exports/skills.zip`, and summarize V2 Metadata instances.
- Updated `webui/README.md`, `webui/SPEC.md`, and `PROGRESS.md`.
- Verification passed: `cd webui && npm run lint`, `cd webui && npm run typecheck`, `cd webui && npm run build`, and `git diff --check`.

## 2026-06-17 WebUI V2 Tools Bridge

- Added a V2 Tools bridge panel at the top of the existing Tool plugins page while preserving the Rust-compatible standalone Tool Runner flow below it.
- Extended `webui/src/v2-api.ts` with Python V2 tool catalog, task MCP tool call, and `tools.zip` download helpers.
- Added `webui/src/V2ToolsBridge.tsx` to list V2 tools, inspect match/params schema metadata, download `/api/v2/exports/tools.zip`, and execute run-scoped V2 tools through `/api/v2/mcp/task/:run_id`.
- Updated `webui/README.md`, `webui/SPEC.md`, and `PROGRESS.md`.
- Verification passed: `cd webui && npm run lint`, `cd webui && npm run typecheck`, `cd webui && npm run build`, and `git diff --check`.

## 2026-06-17 WebUI V2 Memory Bridge

- Added a V2 Memory bridge panel at the top of the existing Memory page while preserving the Rust-compatible Case import/search/edit flow below it.
- Extended `webui/src/v2-api.ts` with Python V2 Case Memory search, import preview, import confirm, and case update helpers.
- Added `webui/src/V2MemoryBridge.tsx` to search V2 Cases, preview text/file imports, edit structured drafts, confirm Cases, edit selected Case details, and enable/disable Cases.
- Updated `webui/README.md`, `webui/SPEC.md`, and `PROGRESS.md`.
- Verification passed: `cd webui && npm run lint`, `cd webui && npm run typecheck`, `cd webui && npm run build`, and `git diff --check`.

## 2026-06-17 WebUI V2 Analyze Bridge

- Added a V2 Analyze bridge panel at the top of the existing Analyze page while preserving the Rust-compatible Session-first flow below it.
- Added `webui/src/v2-api.ts` for Python V2 Workspace, upload, chunked upload, run, analysis, and artifact download requests.
- Added `webui/src/V2AnalyzeBridge.tsx` to create/select V2 Workspaces, upload files, create Runs, poll `/api/v2/runs/:run_id/analysis`, show run/timeline/evidence/resource/artifact counts, render final answers, and download artifacts with Authorization headers.
- Updated `webui/README.md`, `webui/SPEC.md`, and `PROGRESS.md`.
- Verification passed: `cd webui && npm run lint`, `cd webui && npm run typecheck`, `cd webui && npm run build`, and `git diff --check`.

## 2026-06-17 V2 Run Analysis Summary

- Added `GET /api/v2/runs/:run_id/analysis` as the V2 counterpart to Rust Server task analysis reads.
- Added `get_run_analysis`, which combines run/workspace metadata, timeline, evidence, artifact listing, analysis state, analysis package, Agent request/response, System/Metadata context, and final result when present.
- Missing optional analysis resources return `null` so queued, running, failed, and succeeded runs can share the same inspection surface.
- Added regression coverage for analysis summary contents after a succeeded stub run.
- Updated `server-v2/README.md`, `server-v2/SPEC.md`, and `PROGRESS.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, and `git diff --check`.

## 2026-06-17 V2 Run Artifact Listing

- Added `Store.list_run_artifacts` to enumerate both Workspace upload artifacts and run evidence artifacts for a run.
- Added `GET /api/v2/runs/:run_id/artifacts` as the V2 counterpart to Rust Server task artifact listing.
- Artifact listing includes evidence kind, summary, final evidence flag, payload, artifact metadata, and uploaded input artifact metadata.
- Added regression coverage for uploaded input artifact listing and generated artifact kinds such as manifest, log search, analysis package, and result.
- Updated `server-v2/README.md`, `server-v2/SPEC.md`, and `PROGRESS.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, and `git diff --check`.

## 2026-06-17 V2 Run and Upload Listing

- Added Store and HTTP read surfaces for workspace uploads, upload sessions, and runs.
- Added global `GET /api/v2/runs` with optional `workspaceId` filter to support WebUI history/task list views.
- Added `GET /api/v2/uploads/:session_id` for chunked upload session recovery/status reads.
- Added regression coverage for run listing and upload session listing.
- Updated `server-v2/README.md`, `server-v2/SPEC.md`, and `PROGRESS.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, and `git diff --check`.

## 2026-06-17 V2 Upload Sessions

- Added SQLite-backed `upload_sessions` for restartable chunked uploads.
- Added V2 batch upload endpoint `POST /api/v2/workspaces/:workspace_id/uploads/batch`.
- Added V2 chunked upload endpoints: init under a Workspace, offset-checked chunk append, and complete to convert temp bytes into a normal artifact/upload.
- Added streaming `write_artifact_file` so completed chunked uploads do not need to be reloaded into memory.
- Chunked upload temp files live under `data_dir/tmp/upload_sessions`; completion validates received size and records `upload_id` / `artifact_id` on the session.
- Added regression coverage for batch upload persistence, chunk session progress, stream-to-artifact completion, stored bytes, and completed session metadata.
- Updated `server-v2/README.md`, `server-v2/SPEC.md`, and `PROGRESS.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, and `git diff --check`.

## 2026-06-17 V2 Result Artifacts

- Added final result persistence for succeeded V2 runs as `result.json` and `result.md` artifacts.
- `result.json` stores schema version, run id, creation time, and the validated final answer; `result.md` renders the same answer for human review.
- Added background evidence kinds `result` and `result_markdown` with `final_allowed=false`.
- Added `GET /api/v2/runs/:run_id/result`, returning the stored final answer plus result artifact/evidence metadata.
- Task MCP now lists and reads `result` and `result_markdown`; Markdown is returned with `text/markdown`.
- `analysis_package.json` resource index now advertises result resources for post-run review.
- Added regression coverage for result evidence, MCP JSON/Markdown resources, Markdown content, and result helper artifact metadata.
- Updated `server-v2/README.md`, `server-v2/SPEC.md`, and `PROGRESS.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, and `git diff --check`.

## 2026-06-17 V2 Agent Context Tool Catalog

- Expanded the provider-directed Agent tool loop from log search/slice to the advertised task MCP tool catalog.
- Agent prompts now advertise Metadata, Case Memory, Skill reference, Fetch catalog, configured domain tool, and enabled Fetch execution tools in addition to log search/slice.
- AgentRuntime now validates provider tool calls against the same advertised tool set before executing through `call_task_tool`.
- Fetch execution is only advertised when `LOGAGENT_V2_FETCH_ENABLED=1`; waiting/approval tools are not advertised to the provider.
- Added regression coverage that verifies the advertised prompt tool names exclude waiting/approval, that disabled Fetch execution is hidden, and that a provider can call `logagent.search_cases` before returning a final answer.
- Updated `server-v2/README.md`, `server-v2/SPEC.md`, and `PROGRESS.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, and `git diff --check`.

## 2026-06-17 V2 Agent Read-only Tool Loop

- Added `LOGAGENT_V2_AGENT_MAX_ROUNDS` with default 3 for bounded provider/tool-loop execution.
- OpenAI-compatible Agent prompts now include available read-only tools and prior tool observations.
- AgentRuntime now accepts provider-returned `tool_calls` for `logagent.search_logs` and `logagent.get_log_slice`, executes those Server-owned task MCP tools, and feeds structured observations into the next provider round.
- Agent response audit artifacts now persist tool calls and tool observations; `analysis_state.json` records multiple rounds with `tool_calls_executed` and final completion statuses.
- Final answers can now cite follow-up `log_searches/<id>.json#matches/<index>` refs produced during the same run.
- Unsupported tool names, non-object arguments, provider failures, invalid final refs, and max-round exhaustion fail the run with audit artifacts retained.
- Added regression coverage for provider-directed follow-up log search followed by a final answer citing the generated evidence ref.
- Updated `server-v2/README.md`, `server-v2/SPEC.md`, and `PROGRESS.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, and `git diff --check`.

## 2026-06-17 V2 Agent Round Audit

- Added V2 Agent round audit artifacts: `agent_request.json`, `agent_response.json`, and `analysis_state.json`.
- AgentRuntime now persists the provider/stub request before execution, response/validation details after execution, and a latest state snapshot for both success and failure paths.
- OpenAI-compatible provider execution now returns a structured envelope with sanitized request metadata, HTTP/body previews, parsed final answer, or structured failure details; Authorization headers are not stored.
- Task MCP now lists and reads `analysis_state`, `agent_request`, and `agent_response` resources.
- `analysis_package.json` resource index now includes the new Agent audit resources for loop context.
- Evidence listing now uses SQLite insertion order as the tie-breaker inside equal timestamps so latest artifact resources are stable when multiple audit snapshots are written in the same second.
- Added regression coverage for stub audit resources, OpenAI-compatible provider audit resources, and invalid evidence-ref failure retaining audit artifacts.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`.

## 2026-06-17 V2 Analysis Package Resource

- Added V2 `analysis_package.json` generation after initial evidence collection.
- The package captures bounded Agent context: Workspace/run metadata, task MCP resource URIs, manifest outline, grep preview, tool input outline, System Context outline, Metadata Context outline, allowed evidence refs, and final evidence policy.
- `analysis_package` is persisted as background evidence with `final_allowed=false`.
- Task MCP now lists and reads `logagent-v2://run/:run_id/analysis_package`.
- Added regression coverage for package evidence, MCP resource reading, allowed refs, manifest outline, resource index, and empty System Context outline.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, and `git diff --check`.

## 2026-06-17 V2 Agent Provider Bridge

- Added V2 Agent provider settings: `LOGAGENT_V2_AGENT_PROVIDER`, `LOGAGENT_V2_AGENT_BASE_URL`, `LOGAGENT_V2_AGENT_MODEL`, `LOGAGENT_V2_AGENT_API_KEY`, and `LOGAGENT_V2_AGENT_TIMEOUT_SECONDS`.
- Added an OpenAI-compatible single-round Chat Completions provider that sends the Workspace question, manifest counts, bounded grep preview, and allowed current-run evidence refs.
- AgentRuntime now uses the configured provider when `agent_provider=openai_compatible`; default `stub` behavior remains unchanged.
- Provider output must be one JSON final-answer object and still passes existing normalization and evidence-ref validation before the run succeeds.
- Added regression coverage with a mock OpenAI-compatible provider, including Authorization header, model payload, prompt evidence refs, and model-produced final answer.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, and `git diff --check`.

## 2026-06-17 V2 Case Vector Recall

- Added local hash-vector recall for V2 Case Memory without PostgreSQL, Redis, pgvector, or external embedding services.
- `cases` now stores derived `vector_json`; store initialization backfills vectors for existing Case rows.
- Case create/update refreshes both FTS and vector data from the same searchable text.
- `Store.search_cases` now merges SQLite FTS5/BM25 results with vector recall and can return vector-only hits when exact tokens do not match.
- Search results expose `searchBackend=hybrid` or `vector` plus `vectorScore` where applicable.
- Added regression coverage for hybrid FTS/vector results, vector-only approximate recall, and vector refresh after Case edits.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, and `git diff --check`.

## 2026-06-17 V2 Tool Params Schema

- Added V2 `ToolDefinition.params_schema` from configured `paramsSchema`.
- `/api/v2/tools` now exposes each configured tool's own params schema instead of a generic tool-id-only schema.
- Task MCP `logagent.run_domain_tool` now accepts optional `params`, validates a conservative object-schema subset, and rejects unknown or invalid parameters before running the tool.
- Configured argv templates now support `{params.name}` substitution alongside `{input_file}` and `{action_id}`; command paths and argv templates remain Server-owned.
- Tool result artifacts and evidence payloads record validated params, and params contribute to action id hashing so distinct parameter sets do not collide.
- Added regression coverage for params schema exposure, argv substitution, result/evidence params persistence, and unknown-param rejection.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, and `git diff --check`.

## 2026-06-17 V2 Generic Query Tool Inputs

- Extended V2 `tool_inputs/index.json` materialization beyond node-package tsdb InfluxQL.
- Generic text files now produce file-scoped `influxql_analyzer` JSONL inputs when they contain JSON or raw InfluxQL query lines.
- Logs now produce file-scoped `flux_query_analyzer` JSONL inputs when they contain JSON Flux fields or raw `from(...) |> ...` Flux pipelines.
- Tool input index generation now uses `generatedBy=logagent_v2_tool_input_materializer`.
- Existing Tool Runner selection continues to prioritize materialized `tool_inputs` by `toolIds`, so `influxql_analyzer` and `flux_query_analyzer` consume their matching JSONL artifacts before fallback file matching.
- Added regression coverage for generic InfluxQL and Flux materialization plus task MCP tool execution against both generated inputs.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, and `git diff --check`.

## 2026-06-17 V2 Fetch Credential Sets

- Added `cryptography` dependency and `LOGAGENT_V2_FETCH_SECRET_KEY` for Fernet-encrypted Fetch credential sets.
- Added SQLite-backed `fetch_credential_sets` with one encrypted credential set per sensitive Fetch endpoint.
- Fetch endpoint create/import/update now stores only redacted URL/header/body material in `fetch_endpoints` when sensitive query, header, or body fields are detected.
- Sensitive endpoint writes require a valid secret key before the endpoint row is created or updated; non-sensitive endpoints continue to work without a key.
- Fetch execution hydrates the full request material from the encrypted credential set, while API, MCP, and result artifacts continue to expose redacted values.
- Added regression coverage for missing-key rejection, encrypted storage, credential hydration, execution with restored secrets, and redacted request artifacts.
- Updated `server-v2/README.md`, `server-v2/SPEC.md`, and `server-v2/pyproject.toml`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, and `git diff --check`.

## 2026-06-17 V2 Case Import Drafts

- Added SQLite-backed V2 `case_imports` drafts for Memory import preview/confirm flow.
- Added deterministic Case draft parsing for JSON fields and plain text sections such as `Title`, `Symptom`, `Root Cause`, `Solution`, `Product`, `Instance ID`, and `Evidence Refs`.
- Added protected APIs: `GET /api/v2/cases/imports`, `GET /api/v2/cases/imports/:import_id`, `POST /api/v2/cases/imports/preview`, and `POST /api/v2/cases/imports/:import_id/confirm`.
- Preview stores the source text, parsed draft, and validation errors without writing to `cases`; confirm may apply overrides and only then creates a manual Case through the existing FTS-indexed path.
- Confirming an already confirmed import returns the existing Case instead of creating duplicates.
- Added regression coverage for preview parsing, confirm/search, idempotent confirm, incomplete draft rejection, and override completion.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, and `git diff --check`.

## 2026-06-17 V2 Metadata Task Context

- Added V2 run-start `metadata_context.json` artifact generation.
- Metadata context auto-selects up to three imported instances from the Workspace question/mode; a single imported instance is included as `default_single`, while multiple instances require question matches on instance, remark, node, database, measurement, or field terms.
- The context stores bounded topology/schema outlines only; full snapshots and field/tag details remain available through existing Metadata MCP tools.
- Task MCP now exposes `logagent-v2://run/:run_id/metadata_context` through `resources/list` and `resources/read`.
- Metadata context evidence is background-only with `final_allowed=false`.
- Added regression coverage for auto-selection, MCP resource exposure, outline contents, and final evidence exclusion.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, and `git diff --check`.

## 2026-06-17 V2 Fetch cURL Import

- Added V2 DevTools bash cURL import helpers for Fetch endpoints.
- Added protected APIs `POST /api/v2/fetch/imports/preview` and `POST /api/v2/fetch/imports`.
- Supported cURL flags are limited to method, headers, data/body, cookie, compressed, HEAD, and location; unsupported flags such as form upload are rejected.
- Import previews redact sensitive query, header, and JSON/form body fields and report detected sensitive fields.
- V2 Fetch endpoint methods now accept `HEAD`; controlled headers now include `Transfer-Encoding`.
- Added regression coverage for cURL parsing, redaction, sensitive field detection, direct endpoint creation data, HEAD import, unsupported flag rejection, and controlled header rejection.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, and `git diff --check`.

## 2026-06-17 V2 Case FTS Recall

- Added a local SQLite FTS5 `cases_fts` index for Case Memory, synchronized on create/update and backfilled during store initialization.
- `Store.search_cases` now prefers FTS5 `MATCH` with `bm25` ranking when a query is present, while retaining token-overlap fallback if FTS5 is unavailable.
- Search results include `searchBackend` (`fts5`, `keyword`, or `recent`) and a score.
- Added regression coverage for FTS-backed ranking and index updates after case edits.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, and `git diff --check`.

## 2026-06-17 V2 Skill Auto Matching

- Added V2 automatic Skill selection when a Workspace has no explicit `skillIds`.
- Skill matching now considers `keywords`, `products`, `toolIds`, `domainAdapters`, Skill id, name, display name, description, Workspace question, and mode.
- `includeByDefault` Skills are still included; matched resources record `selectionReason` (`explicit`, `default`, or `auto`) and `matchScore` in `system_context.json`.
- Imported Skill default manifests now include an empty `keywords` list.
- Added regression coverage for question-driven Skill selection and non-matching Skill exclusion.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, and `git diff --check`.

## 2026-06-17 V2 Fetch Redirect Revalidation

- Added `LOGAGENT_V2_FETCH_MAX_REDIRECTS` with default 5 for V2 Fetch execution.
- Fetch now follows redirects manually and revalidates every redirect target against the same `LOGAGENT_V2_FETCH_ALLOWED_HOSTS` allowlist before sending the next request.
- Cross-origin redirects strip sensitive headers such as Authorization, Cookie, X-Api-Key, and X-Auth-Token.
- Fetch response artifacts now include redacted `finalUrl`, `redirectCount`, and redirect hop summaries.
- Added regression coverage for allowlisted redirect success and disallowed redirect failure.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, and `git diff --check`.

## 2026-06-17 V2 InfluxQL Tool Output Adapter

- Added V2 Tool Runner stdout normalization for InfluxQL analyzer report and compare-report JSON.
- Generic JSON stdout parsing now accepts `summary`/`message`/`title`, `findings`/`issues`/`diagnostics`, string findings, and alternate finding fields such as `level`, `path`, `lineNumber`, and `description`.
- InfluxQL report output now produces summaries for record/window/statement/parse-error counts plus special rule counts, and findings for special rules, parse errors, realtime classification, and notable fingerprints.
- InfluxQL compare output now produces summaries for statement/QPS deltas and batch stats, plus findings for new/removed/changed fingerprints and rule deltas.
- Added regression coverage for InfluxQL report and compare-report parsing.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, and `git diff --check`.

## 2026-06-17 V2 Tool Runner Fallback Inputs

- Added V2 Tool Runner fallback input matching for configured tools whose args contain `{input_file}`.
- Materialized `tool_inputs/index.json` entries still take priority; when none match, V2 selects current-run text files by `match.filePatterns`, then supplements with initial `grep_results.json` line matches containing `match.keywords`.
- Fallback inputs are persisted as `logagent.v2.tool_input.text_file.v1` artifacts and exposed to tools as virtual `extracted/<manifest path>` inputs.
- Multi-input task MCP `logagent.run_domain_tool` responses now preserve the primary `result/evidence` fields and also return `results[]` and `evidenceItems[]`.
- Added regression coverage for pattern-first and grep-keyword fallback selection, multi-input execution, stable distinct action ids, and evidence creation.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, and `git diff --check`.

## 2026-06-17 V2 Tools Zip Export

- Added V2 `GET /api/v2/exports/tools.zip` for exporting enabled configured subprocess tools.
- Tool exports now include `README.md`, `tools-manifest.json`, packaged binaries under `bin/<toolId>/`, shell wrappers under `wrappers/`, and config examples under `config/examples/`; packaged binaries and wrappers carry executable zip mode metadata.
- Missing, relative, non-file, or non-executable configured tools are retained in the manifest with `skipped=true`; disabled tools and built-in tools are omitted.
- The export does not include API keys, Fetch endpoint credentials, runtime environment values, uploads, artifacts, or workspace data.
- Added regression coverage for executable packaging, skipped missing tools, config examples, manifest fields, and disabled-tool omission.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2`, `PYTHONPATH=. python3 -m unittest discover tests`, and `git diff --check`.

## 2026-06-17 V2 Skills Zip Export

- Added V2 `GET /api/v2/exports/skills.zip` for exporting the current filesystem Skill registry.
- Added `server-v2/logagent_v2/exports.py` to package each Skill directory's regular files, preserve relative paths, and write a root `manifest.json` with Skill revision and file metadata.
- Export path validation rejects absolute/parent paths, and symlinks or symlinked directories are skipped so exports cannot include files outside the Skill registry.
- Added regression coverage for regular Skill files, reference files, manifest generation, and symlink skipping.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2` and `PYTHONPATH=. python3 -m unittest discover tests`.

## 2026-06-17 V2 Metadata URL Fetch

- Added allowlisted Metadata URL fetch for preview and direct import, reusing `LOGAGENT_V2_FETCH_ENABLED`, `LOGAGENT_V2_FETCH_ALLOWED_HOSTS`, timeout, and response-size limits.
- Added `POST /api/v2/metadata/imports/fetch/preview` to GET a metadata URL, parse/normalize the response, and create a preview draft without mutating confirmed instances.
- Added `POST /api/v2/metadata/imports/fetch` as a direct fetch-and-confirm shortcut.
- Metadata import drafts now track a redacted `sourceUrl`; sensitive query parameters are not exposed in previews.
- Redirects are rejected in this V2 slice; HTTP errors and allowlist failures are returned as controlled errors.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2` and `PYTHONPATH=. python3 -m unittest discover tests`.

## 2026-06-17 V2 Metadata Preview Confirm

- Added SQLite-backed V2 `metadata_imports` drafts with `previewed` and `confirmed` statuses.
- Added Metadata preview flow that parses and normalizes JSON/YAML/openGemini content into a draft without mutating `metadata_instances`.
- Added Metadata confirm flow that upserts the draft snapshot into `metadata_instances` and marks the draft confirmed.
- Added protected APIs: `GET /api/v2/metadata/imports`, `GET /api/v2/metadata/imports/:import_id`, `POST /api/v2/metadata/imports/preview`, and `POST /api/v2/metadata/imports/:import_id/confirm`.
- Kept `POST /api/v2/metadata/imports` as the direct immediate import shortcut.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2` and `PYTHONPATH=. python3 -m unittest discover tests`.

## 2026-06-17 V2 Materialized Tool Inputs

- Added V2 `ToolDefinition` support for `maxInputFiles` and `match` metadata in `LOGAGENT_V2_TOOLS_JSON`.
- Added node-package `tsdb` InfluxQL query materialization during initial evidence indexing. V2 now writes `influxql_analyzer` JSONL artifacts and a `tool_inputs/index.json` artifact with V1-compatible `ToolInputEntry` fields plus V2 artifact ids.
- Manifest artifacts now include `toolInputsPath=tool_inputs/index.json` and `toolInputCount` when materialized inputs exist.
- Added `tool_input_index` background evidence so task MCP Tool Runner calls can find run-local materialized inputs without exposing them as final root-cause evidence.
- Configured tools whose args contain `{input_file}` now select matching materialized inputs by `toolIds`, substitute the local artifact path, and generate stable per-input action ids for `tool_results/<tool_id>_<input_hash>/result.json#findings/<index>` refs.
- Existing no-input configured tools keep the previous action id behavior and final ref shape.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2` and `PYTHONPATH=. python3 -m unittest discover tests`.

## 2026-06-17 V2 Fetch Endpoint Foundation

- Added V2 Fetch endpoint settings: default disabled execution, explicit host/host:port allowlist, request timeout, and bounded response preview size.
- Added SQLite-backed Fetch endpoint storage with create/list/read/update/delete helpers and redacted public endpoint previews.
- Added protected Fetch APIs: `GET/POST /api/v2/fetch/endpoints`, `GET/PATCH/DELETE /api/v2/fetch/endpoints/:endpoint_id`, and `POST /api/v2/runs/:run_id/fetch/:endpoint_id`.
- Added built-in Fetch catalog descriptor to `/api/v2/tools` and readonly MCP catalog output.
- Added task MCP tools `logagent.list_fetch_endpoints` and `logagent.fetch`; execution writes `fetch_result` artifacts/evidence and returns controlled refs in the form `tool_results/<fetch_action_id>/result.json#response`.
- Final-answer validation now accepts Fetch `#response` refs only when they point to current-run, final-allowed `fetch_result` evidence with a real response object.
- Readonly MCP remains non-executing for Fetch; `tools/call logagent.fetch` is rejected there.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2` and `PYTHONPATH=. python3 -m unittest discover tests`.

## 2026-06-17 V2 Skill-backed System Context Foundation

- Added V2 filesystem Skill registry under `LOGAGENT_V2_DATA_DIR/skills` with Codex-compatible `SKILL.md`, optional `logagent.json`, revision hashing, declared references, and safe reference path validation.
- Added Skill APIs: `GET /api/v2/skills`, `GET /api/v2/skills/:skill_id`, `POST /api/v2/skills/imports`, and side-effect-free `POST /api/v2/skills/preview`.
- Workspaces now persist explicit `skillIds`; run execution writes a `system_context` background artifact containing selected diagnostic Skills, bounded content, revisions, and reference indexes.
- Added readonly MCP and task MCP Skill tools: `logagent.list_skills`, `logagent.get_skill`, `logagent.get_skill_reference`, and `logagent.preview_system_context`.
- Task MCP Skill reference reads are constrained to the run snapshot and persist `skill_reference` background evidence with `final_allowed=false`.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2` and `PYTHONPATH=. python3 -m unittest discover tests`.

## 2026-06-17 V2 Case Memory Foundation

- Added SQLite-backed V2 Case Memory table and Case schema v2 records.
- Added manual Case creation, succeeded-run Case confirmation with duplicate prevention, keyword recall, get-by-id, edit, and enable/disable support.
- Added V2 Case APIs: `POST /api/v2/cases`, `POST /api/v2/runs/:run_id/case`, `GET /api/v2/cases`, `GET /api/v2/cases/:case_id`, and `PATCH /api/v2/cases/:case_id`.
- Added readonly MCP and task MCP Case tools: `logagent.search_cases` and `logagent.get_case`.
- Task MCP Case calls now persist `case_context` background evidence with `final_allowed=false`.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2` and `PYTHONPATH=. python3 -m unittest discover tests`.

## 2026-06-17 V2 Metadata Foundation

- Added SQLite-backed V2 `metadata_instances` storage for imported raw snapshots and normalized Metadata snapshots.
- Added direct Metadata import for JSON, YAML via PyYAML, and openGemini-style content. The openGemini path normalizes nodes, databases, retention policies, measurements, and field type labels.
- Added V2 Metadata APIs for instance list/detail/snapshot/delete plus field type and tag field queries.
- Added readonly MCP and task MCP Metadata tools: `logagent.list_metadata_instances`, `logagent.get_metadata_snapshot`, `logagent.get_metadata_field_types`, and `logagent.get_metadata_tag_fields`.
- Task MCP Metadata calls now persist `metadata_slice` background evidence with `final_allowed=false`.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2` and `PYTHONPATH=. python3 -m unittest discover tests`.

## 2026-06-17 V2 Node Log Package Preprocessor

- Added V2 preprocessing for node log packages named `<packageId>_<instanceId>_<nodeId>_<timestamp>_logs.tar.gz` or `.tgz`.
- Tar scanning now classifies wrapped `var/chroot/gemini/log/tsdb`, `var/chroot/gemini/log/stream`, and `home/Ruby/log` members into stable `extracted/<nodeId>/<timestamp>/<group>/...` paths.
- Node package rotated files are accepted by directory membership instead of suffix, and gzip content is decoded by magic bytes before manifest and grep indexing.
- Matching node packages with no supported log directories now fail clearly instead of producing an empty successful manifest.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2` and `PYTHONPATH=. python3 -m unittest discover tests`.

## 2026-06-17 V2 Final Evidence Validation

- Added V2 final answer normalization and validation before a run can be marked `succeeded`.
- Final answers now require a non-empty summary, normalized string arrays, structured likely root causes, and `confidence=low|medium|high`.
- Final evidence refs are validated against current-run `final_allowed` evidence artifacts. Supported refs are initial grep matches, follow-up log search matches, log slices, and configured tool findings.
- Background context such as `manifest.json` is explicitly rejected as final root-cause evidence.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2` and `PYTHONPATH=. python3 -m unittest discover tests`.

## 2026-06-17 V2 Waiting Actions

- Added persistent V2 action helpers for pending user input and approval requests.
- Added task MCP `logagent.request_user_input` and `logagent.request_approval`; calls persist actions and move the run into `waiting_for_user` or `waiting_for_approval`.
- Updated message and action decision APIs so a waiting run can be requeued into the SQLite job queue after user input or approve/reject.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2` and `PYTHONPATH=. python3 -m unittest discover tests`.

## 2026-06-17 V2 Minimal Tool Runner

- Added V2 configured Tool Runner foundation. Tools can be loaded from `LOGAGENT_V2_TOOLS_JSON`, listed through `/api/v2/tools`, and invoked through task MCP `logagent.run_domain_tool`.
- Tool execution is whitelist-only: models provide `toolId` only; executable path and argv come from Server configuration, run without shell, with timeout and bounded stdout/stderr.
- Tool results are written as JSON artifacts and `tool_result` evidence. JSON stdout with `summary` and `findings` is parsed into structured result fields.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2` and `PYTHONPATH=. python3 -m unittest discover tests`.

## 2026-06-17 V2 Task MCP Log Slice

- Added task MCP `logagent.get_log_slice` for bounded context around a current Workspace text path and line number.
- Log slices are persisted as `log_slice` evidence with refs in the form `log_slices/<slice_id>.json#lines`.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2` and `PYTHONPATH=. python3 -m unittest discover tests`.

## 2026-06-17 V2 Task MCP Log Search

- Added V2 task MCP endpoint `POST /api/v2/mcp/task/:run_id`.
- Task MCP now supports `initialize`, `resources/list`, `resources/read`, `tools/list`, and `tools/call`.
- Exposed run-scoped `summary`, `evidence`, `manifest`, and `grep_results` resources over MCP.
- Implemented `logagent.search_logs` follow-up search for current Workspace uploads. Each search creates a `log_search` evidence item and returns stable refs in the form `log_searches/<search_id>.json#matches/<index>`.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2` and `PYTHONPATH=. python3 -m unittest discover tests`.

## 2026-06-17 V2 Initial Evidence Pipeline

- Continued the `rewrite/logagent-v2` clean-room migration toward Rust Server feature parity.
- Added V2 initial evidence indexing for Workspace uploads. Runs now collect supported plain text logs and scan `.zip`, `.tar`, `.tar.gz`, and `.tgz` packages with bounded file counts, bytes, and grep matches.
- Added archive safety checks that reject absolute paths, `..` traversal, empty paths, and unsafe member names; symlinks and non-file archive members are skipped.
- V2 run execution now writes `manifest.json` and `grep_results.json` artifacts, records `manifest` and `log_search` evidence rows, exposes `GET /api/v2/runs/:run_id/evidence`, and lets the current stub final answer cite `grep_results.json#matches/<index>` when initial matches exist.
- Updated `server-v2/README.md` and `server-v2/SPEC.md`.
- Verification passed: `python3 -m compileall logagent_v2` and `PYTHONPATH=. python3 -m unittest discover tests`.

## 2026-06-16 LogAgent V2 Clean-room Scaffold

- Created the `rewrite/logagent-v2` branch from `main` to keep Rust V1 stable while evaluating a small-team Python Agent stack.
- Added `server-v2/` as a clean-room FastAPI/LangGraph-oriented implementation. The first slice uses SQLite WAL and local artifacts instead of PostgreSQL/Redis, matching the small-team deployment decision.
- Implemented the V2 foundation: settings, API key auth, SQLite schema, DB-backed job queue, local artifact writes, Workspace/Run/Upload/Evidence/Timeline models, inline worker, stub Agent runtime, artifact download, and read-only MCP discovery placeholder.
- Added `server-v2/README.md` and `server-v2/SPEC.md`, and updated root README/SPEC to document the V2 exception to the Rust-first rule.
- Verification passed: `python3 -m compileall logagent_v2` and `PYTHONPATH=. python3 -m unittest discover tests`.

## 2026-06-16 Stable MCP Log Search Evidence

- Investigated `sess_1781622184222_1` / `task_1781622204486_5`: the uploaded openGemini node packages were extracted successfully and the real analyzer found query issues, but Claude Code later issued broad MCP searches such as `memory`, `at `, and Java exception names. The previous `logagent.search_logs` response returned only counts and `grep_results.json#matches/*` refs, so the model misread benign `lib/memory/memory.go` and ordinary English/Go log lines as JVM/OOM/NPE evidence.
- Changed task MCP `logagent.search_logs` to write each subsequent search to stable `log_searches/logsearch_*.json` artifacts, return matched line text, `keywordCounts`, `unmatchedKeywords`, and `log_searches/...#matches/<index>` refs, and stop overwriting the initial `grep_results.json`.
- Extended Claude Code / LLM final evidence validation to accept stable log search refs and reject unknown log search artifacts or out-of-range matches. Updated the Claude startup prompt to require inspecting `matches[].text` and to forbid inferring exception types or technology stacks from `totalMatches` alone.
- Updated root, Server, Log Analyzer, Analysis Agent, and LLM Gateway docs/specs.
- Verification passed: `cargo fmt`; focused tests for MCP stable search artifacts, LLM log search evidence refs, and Claude evidence rejection.

## 2026-06-16 Claude Code Session Timeout

- Increased the default and recommended `claude_code.max_session_seconds` from 120 seconds to 600 seconds so `PLAN_ANALYSIS` has enough time for Claude Code MCP evidence reads and tool calls on larger log packages.
- Updated example configs, deploy sample config, Agent Backend docs, Config docs, and the active 50992 runtime config.
- Verification passed: `cargo fmt --check`, `cargo check`, and `cargo test -p logagent-server support::config -- --nocapture`.

## 2026-06-16 Analyze Start With Pending Uploads

- Fixed Analyze Session run creation when a user selected log files and clicked `Start analysis` without first clicking the separate upload button. WebUI now uploads pending files, attaches the completed upload IDs to the selected Session, and only then creates the Session task run.
- Root cause observed in `sess_1781620388058_1`: the latest task snapshot had `uploadIds=[]`, `manifest.files=0`, and `analysis_package.task.uploadIds=[]`, so Claude Code correctly treated it as a question-only task and asked for more data.
- Updated WebUI README/SPEC to document that `Start analysis` auto-attaches pending selected files before creating the run.
- Verification passed: `npm --prefix webui run lint`, `npm --prefix webui run typecheck`, `npm --prefix webui run build`, and the rebuilt static `webui/out` was synced to the running 50992 runtime directory for manual retest.

## 2026-06-16 Node Package Directory Entry Fix

- Fixed node log package extraction for tar archives that include `./` or other directory entries. The preprocessor now skips directory entries before path normalization and only classifies ordinary files, matching tarballs produced by `tar -czf package.tar.gz .`.
- Updated the node log package regression fixture to include an archive root directory entry, covering the real extraction failure observed during openGemini rotation upload testing.
- Tool Runner now treats matching materialized `tool_inputs` as exclusive automatic inputs; raw manifest/grep fallback is only used when no materialized input exists, preventing `influxql_analyzer` from being run against raw rotated `.log.gz` files after the preprocessor has generated JSONL.
- Documentation now states that top-level wrapper directories and directory entries are supported for node log packages.
- Verification covered the real openGemini rotation flow: deployment configs were set to `max-size=1m`, `max-num=8`, `max-age=7`, `compress-enabled=true`; tsbs load/query traffic generated rotated gzip logs; a node package with wrapper directory, `./` tar entry, active logs, and rotated gzip logs uploaded successfully; LogAgent extracted 16 files into node log groups, generated a 2,247-line `influxql_analyzer` JSONL input, and the real analyzer finished with `status=OK`.
- Local checks passed: `cargo fmt --check`, `cargo check`, `cargo test -p logagent-server services::log_analyzer -- --nocapture`, `cargo test -p logagent-server services::tool_runner -- --nocapture`, and `git diff --check`.

## 2026-06-16 Session Tarball Extraction Fix

- Fixed node log package preprocessing for Session tasks when uploaded `.tar.gz` archives contain a top-level wrapper directory. The path classifier now searches normalized archive components for `var/chroot/gemini/log/{tsdb,stream}` and `home/Ruby/log` instead of requiring those prefixes at archive root.
- Node log packages that match `<packageId>_<instanceId>_<nodeId>_<timestamp>_logs.tar.gz` but contain no supported log directories now fail EXTRACT with a clear error instead of producing an empty successful manifest.
- Added pipeline regression coverage for ordinary `.tar.gz` Session task extraction and wrapped node log package extraction, plus Log Analyzer coverage for unsupported-node-package failure.
- Updated root, Server, and Log Analyzer docs/specs.

## 2026-06-16 Analyze Session Create/Delete

- Fixed the Analyze “新建 Session” path by aligning newly created `AnalysisSessionRecord` values with the Session Store's supported schema version. The previous handler created `schemaVersion=2` while the store validator only accepted `schemaVersion=1`, causing `POST /api/sessions` to fail.
- Added protected `DELETE /api/sessions/:session_id`. It rejects running/waiting Sessions and Sessions with unfinished tasks, then removes only the Session JSON and Session timeline workspace.
- Preserved evidence by design: deleting a Session does not delete associated upload payloads, task records, task workspaces, result artifacts, Cases, or Memory entries.
- Added Analyze Session history delete controls with confirmation. Deleting the selected Session clears the draft/details, run list, timeline, artifacts and active task state before refreshing the Session list.
- Updated root, Server and WebUI docs/specs for Session deletion semantics and the create-session schema fix.

## 2026-06-16 Log Package Preprocessor

- Added Server-native preprocessing for node log packages named `<packageId>_<instanceId>_<nodeId>_<timestamp>_logs.tar.gz`. Matching uploads now expand to `extracted/<nodeId>/<timestamp>/{tsdb,stream,agent}/` and record package/node metadata in `manifest.json`.
- Log rotation is handled by directory membership, not filename suffix. All ordinary files under the three supported log directories are kept; gzip content is detected by magic bytes so rotated files with arbitrary names can be searched and sliced.
- Generated analyzer-ready materialized inputs under `tool_inputs/`, including a shared `tool_inputs/index.json`, generic `log_text` JSONL, and `influxql_analyzer` JSONL containing only lines where a query can be extracted.
- Tool Runner selection now prefers `tool_inputs/index.json` entries for the current toolId before falling back to manifest file patterns and grep keywords. Manual configured tool runs can explicitly use safe `tool_inputs/...` paths.
- Added built-in runnable catalog tool `logagent.preprocess_log_package` so the same preprocessing can be run from WebUI Tools as a `tool_run` task and viewed as a JSON result artifact.
- Updated root, Server, WebUI, Log Analyzer, Tool Runner, Interfaces, and Security docs/specs.
- Verification passed: `cargo fmt --check`, `cargo check`, focused Log Analyzer tests, focused pipeline preprocessing test, focused Tool Runner materialized-input test, `cargo test -- --test-threads=1`, `npm --prefix webui run lint`, `npm --prefix webui run typecheck`, `npm --prefix webui run build`, and `git diff --check`.

## 2026-06-16 Huawei OBS + GaussDB Package Sync Tool

- Added disabled-by-default `huawei_cloud.package_sync` Server config with OBS endpoint/bucket/object prefix, OBS credential env vars, optional security token env var, GaussDB host/port/database/user/password env var, `sslmode=disable`, and per-step timeout. Disabled configs do not read credential env vars; enabled configs fail startup on missing or empty secrets.
- Added built-in Tools catalog descriptor `logagent.huawei_cloud_package_sync` with `source=built_in`, `backend=huawei_cloud_package_sync`, `minFiles=maxFiles=1`, `exportable=false`, `editable=false`, and `runnable` tied to config.
- Added Huawei package sync execution service. Manual `tool_run` requires one completed upload, streams that raw snapshot file to Huawei OBS with signed PUT, executes user-provided GaussDB update SQL, performs OBS HEAD, executes GaussDB query SQL, and writes `tool_results/<action_id>/result.json`.
- Result artifacts include OBS PUT/HEAD status, object key/URL, GaussDB affected rows, bounded query rows, step timings, failed step/error, and credential environment variable names. They intentionally omit raw SQL and OBS/GaussDB secret values.
- Added object key validation, config validation tests, parameter validation tests, fake-client success result tests, and Tools API descriptor regression coverage.
- Updated root, Server, WebUI, Tool Runner, Interfaces, Security, Config docs/specs plus deploy and example configs for the disabled Huawei package sync section.
- Verification passed: `cargo fmt --check`, `cargo check -p logagent-server`, `cargo test -p logagent-server huawei -- --nocapture`, `cargo test -p logagent-server http::tools::tests::pprof_tool_run_reuses_uploads_tasks_and_result_api -- --nocapture`, `cargo test -p logagent-server -- --test-threads=1`, `npm --prefix webui run lint`, `npm --prefix webui run typecheck`, `npm --prefix webui run build`, and `git diff --check`.

## 2026-06-16 Fetch Endpoint MVP

- Added Server-managed Fetch endpoints behind a new `fetch` config section. Fetch is disabled by default; enabling it requires a 32-byte base64 secret key from `fetch.secret_key_env` and an explicit `fetch.allowed_hosts` HTTP/HTTPS allowlist.
- Added DevTools bash cURL import preview and endpoint CRUD APIs under `/api/fetch/*`. Import detects Authorization, Cookie, and token/api_key/secret/password/session-style query/body fields, stores sensitive values in an AES-256-GCM credential set under `storage.data_dir/fetch`, and returns only redacted previews.
- Added built-in runnable tool descriptor `logagent.fetch`, manual Fetch runs as `taskKind=tool_run`, bounded reqwest execution with request/response size limits, timeout, redirect revalidation, cross-host Authorization/Cookie stripping, controlled-header rejection, and response artifacts at `tool_results/<action_id>/result.json` plus `response_body.bin`.
- Added task MCP tools `logagent.list_fetch_endpoints` and `logagent.fetch`; the latter returns bounded response previews and final-answer evidence refs in the controlled format `tool_results/<action_id>/result.json#response`.
- Kept the personal read-only HTTP MCP boundary read-only: it may expose the `logagent.fetch` catalog descriptor through `logagent://tools/catalog` / `logagent.list_tools`, but `tools/call logagent.fetch` is rejected.
- Updated LLM final evidence validation so `#response` refs are accepted only for current-task actions whose result artifact is real and `tool=logagent.fetch`; unknown actions and non-Fetch actions are rejected.
- Added WebUI `Tools / Fetch` for cURL paste, redacted preview, save, enable/disable, delete, manual run, recent run polling, and response artifact viewing.
- Updated root, Server, WebUI, Tool Runner, Interfaces, Security, Config docs/specs, added `examples/server-fetch.yaml`, and synced this progress log.
- Verification passed: `cargo fmt --check`, `cargo check`, `cargo test -p logagent-server fetch -- --nocapture`, `cargo test -p logagent-server readonly_mcp_exposes_resources_and_tools_without_task_access -- --nocapture`, `cargo test -- --test-threads=1`, `npm --prefix webui run lint`, `npm --prefix webui run typecheck`, `npm --prefix webui run build`, and `git diff --check`.

## 2026-06-16 Submodule Remote Guard

- Fixed `scripts/configure-tool-submodules.sh` so custom source-built analyzer submodule URLs can no longer rewrite the parent repository `origin`.
- Root cause: when a `third_party/<tool>` directory existed but the submodule was not initialized, `git -C <tool> rev-parse --git-dir` discovered the parent repository, so `remote set-url origin <submodule-url>` targeted the parent checkout.
- The script now only updates an existing submodule `origin` when that directory is its own initialized Git worktree; otherwise it only writes `submodule.<path>.url` in the parent repo local config for future `git submodule update`.
- Updated deployment and Tool Runner docs/specs to state that submodule URL overrides must not mutate `.gitmodules` or the top-level `origin`.
- Verification passed: shell syntax checks for `scripts/configure-tool-submodules.sh` and `scripts/build-tools.sh`, `configure-tool-submodules.sh --help`, `build-tools.sh --help`, temp-repo regression for an uninitialized submodule directory preserving parent `origin`, temp-repo regression for an initialized submodule updating only the child `origin`, and `git diff --check`.

## 2026-06-16 Metadata Compact Schema Type Codes

- openGemini raw JSON import now accepts both `Measurement.Schema` shapes: compact `{ "field": <typeCode> }` values and object `{ "field": { "Typ": <typeCode>, "EndTime": ... } }` values.
- Schema normalization resolves each field by first parsing the compact direct type code, including numeric strings, then falling back to object aliases `Typ` / `Type` / `type` / `typ`; `EndTime` remains available for object fields.
- The openGemini type-code mapping is unchanged, so `logagent.get_metadata_field_types`, `logagent.get_metadata_tag_fields`, and WebUI Schemas continue to display the same `Unknown/Integer/Unsigned/Float/String/Boolean/Tag/Unknown` labels.
- Verification passed: `cargo fmt --check`, `cargo check`, `cargo test -p logagent-server services::metadata -- --test-threads=1`, and `cargo test -p logagent-server -- --test-threads=1`.

## 2026-06-16 Analyze Language Preference

- Added WebUI language configuration for the Analyze workflow. The default is Simplified Chinese, with an English switch in the top bar; the selected language is saved in browser localStorage.
- Analyze now localizes fixed UI labels, status/phase/confidence badges, common timeline event labels, waiting prompts, approval controls, result headings, Case confirmation controls, and artifact panel labels. Technical terms such as `Session`, `Case`, `Claude Code`, `MCP`, `Metadata`, `Tool Runner`, `grep`, `artifact`, `evidence ref`, JSON keys, paths, and product names remain unchanged where translation would reduce precision.
- Added Server `analysisLanguage` persistence to Sessions and log-analysis tasks. Existing persisted JSON defaults to `zh-CN`; new task runs snapshot the Session language into `TaskRecord`, `analysis_state.json`, task MCP summary, and `analysis_package.json`.
- Claude Code startup prompts now include response-language instructions: `zh-CN` asks for Simplified Chinese natural-language fields, while `en-US` asks for English. Protocol names, paths, JSON keys, tool names, product names, and evidence refs remain raw.
- Verification passed: `cargo fmt --check`, `cargo check`, `cargo test -- --test-threads=1`, `npm --prefix webui run lint`, `npm --prefix webui run typecheck`, `npm --prefix webui run build`, and `git diff --check`. Plain parallel `cargo test` exposed existing concurrency-sensitive fake CLI/tool timeout failures, so the single-thread run is the final test signal for this change.

## 2026-06-16 Source-built Domain Analyzer Tools

- Unified the four source-built analyzer submodules on Go 1.26: InfluxQL analyzer, Flux query analyzer, openGemini storage analyzer, and InfluxDB storage analyzer now have matching module/build/CI or developer documentation baselines. InfluxDB already had `go 1.26`; its Dockerfile and storage-analyzer README now match that baseline.
- Verification for the Go 1.26 bump passed for `go test ./...` in `third_party/influxql`, `scripts/build-tools.sh --only influxql`, `--only opengemini`, `--only influxdb`, Go module parsing for all updated modules, shell syntax checks, and `git diff --check`. A focused Flux Go package test (`go -C third_party/flux test ./stdlib/strings`) could not complete in this local environment because `libflux` pkg-config metadata (`flux.pc`) is not installed.
- Added `third_party/` submodules for source-referenced tools: `influxql` branch `influxql-analyzer` at `d76501446edb0520dddf88951c985dc62dc671de`, `flux` branch `feature/query-stats` at pushed commit `182ee355aff98074feb952ae92e6eb561bc72076`, `openGemini` branch `openGemini-tools` at pushed commit `9fb4d6feafcc9990504b3112182184dc41b1ea76`, and `influxdb` branch `influxdb-tools` at pushed commit `20ecb454036c40730236c174dee1067e3c6f5760`.
- Added `scripts/configure-tool-submodules.sh` and wired `scripts/build-tools.sh` to honor `LOGAGENT_SUBMODULE_BASE_URL` plus per-tool `LOGAGENT_SUBMODULE_*_URL` clone URL overrides before initializing analyzer submodules, so intranet deployments can use mirrored repositories without editing `.gitmodules`.
- `scripts/smoke-flux-query-analyzer.sh` now uses `scripts/build-tools.sh --only flux`, matching the other real-tool smoke scripts and inheriting the same submodule URL override behavior.
- Verification passed for the submodule override follow-up: shell syntax checks for the changed scripts, `configure-tool-submodules.sh --help`, `build-tools.sh --help`, temporary per-tool and `--base-url` overrides with local Git config/remote restoration, `scripts/build-tools.sh --only flux --output-dir target/tools`, full `scripts/build-tools.sh --output-dir target/tools`, `scripts/smoke-flux-query-analyzer.sh`, and `git diff --check`.
- Flux `query_stats` now supports LogAgent-friendly bounded stdout JSON via `--format json`, input/error caps, and real `--fail-on-parse-error`; stdout includes `summary/findings/topQueries/parseErrors`.
- openGemini now has `opengemini-storage-analyzer` for read-only TSSP and TSI mergeset inspection; InfluxDB 1.x now has `influxdb_storage_analyzer` for read-only TSM, TSI, and `_series` inspection.
- Added `scripts/build-tools.sh` to build all source-referenced tools into `target/tools/`, `$LOGAGENT_WORK_DIR/bin/tools/`, or deploy runtime `$LOGAGENT_APP_DIR/bin/tools/`; `scripts/build-server.sh` and `deploy/rebuild-install.sh` invoke it automatically.
- Server tool fixed `path` now supports `${ENV}` expansion, so deploy/runtime configs can point directly at `${LOGAGENT_APP_DIR}/bin/tools/...` without extra path env vars.
- Added dedicated example configs and smoke scripts for Flux, InfluxQL, openGemini storage, and InfluxDB storage analyzers; `scripts/smoke-product-loop.sh` now builds and uses the repo InfluxQL analyzer instead of `/usr/bin/influxql-analyzer`.
- Updated root, Server, Tool Runner, Domain Adapter, Config, Deployment, and Testing docs/specs for source-built analyzers, storage analyzer tool IDs, config paths, and verification flow.

## 2026-06-15 Metadata Tag Fields Tool

- Added built-in metadata tool `logagent.get_metadata_tag_fields` for Tools API, read-only HTTP MCP, and task stdio MCP.
- The tool requires `instanceId`, `database`, and `measurement`, accepts optional `retentionPolicy`, and intentionally does not support `field`; omitted RP uses the DB default RP exactly like `logagent.get_metadata_field_types`.
- Returned JSON reuses the field-types response contract but filters `fields` to Tag entries (`typ=6` / `typeLabel=Tag`), keeps `missingFields=[]`, and sets `finalEvidenceAllowed=false`.
- Manual no-upload Tools runs now persist `tool_run` results for the tag fields tool, and task MCP calls write background artifacts at `metadata_slices/tag_fields_<stable_id>.json` with audit entries in `mcp_calls.jsonl`.
- Updated Server, Metadata, Tool Runner, and root specs/docs to distinguish arbitrary field type lookup from tag-only field lookup.
- Verification passed: `cargo fmt --check`, `cargo check -p logagent-server`, focused metadata/task-MCP/read-only-MCP/Tools API tests, `cargo test -p logagent-server -- --test-threads=1` after rerunning one transient Remote Executor test, `cd webui && npm run lint`, `cd webui && npm run typecheck`, and `cd webui && npm run build`.

## 2026-06-15 Manual Tools JSON Template Run

- `/api/tools` descriptors now include `paramsTemplate`; WebUI Tools uses it to prefill editable JSON params for every runnable catalog tool.
- Configured command tools are now runnable from the Tools page when enabled. Manual runs prepare `extracted/`, `manifest.json`, and `grep_results.json`, then execute the configured whitelist args through the existing ToolRunner. Users can leave `inputFiles=[]` for match-rule selection or provide safe `extracted/...` paths.
- Built-in metadata tools remain read-only, non-editable, and non-exportable, but are now runnable from Tools without uploads. Results are persisted as `tool_run` artifacts and shown as JSON.
- `pprof_analyzer` keeps its specialized parsed top-table result display; other tool results use a generic JSON viewer.
- Verification passed: `cargo fmt --check`, `cargo check -p logagent-server`, focused `http::tools`, `http::exports`, and `http::mcp_readonly` tests, `cargo test -p logagent-server -- --test-threads=1`, `cd webui && npm run lint`, `cd webui && npm run typecheck`, `cd webui && npm run build`, `git diff --check`, and LAN auto deploy to `duzhiwang@192.168.31.128`.

## 2026-06-15 Built-in Tool Catalog Registration

- Registered the built-in metadata tools in the shared Tools catalog: `logagent.list_metadata_instances`, `logagent.get_metadata_snapshot`, and `logagent.get_metadata_field_types`.
- Extended Tool descriptors with `source`, `tags`, `readOnly`, `editable`, `exportable`, and `runnable`; configured tools use `source=configured`, while built-ins use `source=built_in`, are read-only, cannot be edited, and cannot be exported.
- `/api/tools`, read-only MCP `logagent://tools/catalog`, and `logagent.list_tools` now use the same descriptor view. The catalog only shows real configured tools plus built-ins; an unconfigured pprof template no longer appears as a fake configured tool.
- `tools.zip` remains limited to enabled configured executable tools, and tests assert built-in metadata tools are excluded from `tools-manifest.json`.
- WebUI Tools / Tool plugins now displays configured vs built-in tags, read-only status, exportability, and input schema; only `runnable=true` tools show the upload/run form.
- Updated Server, WebUI, and Tool Runner docs/specs for the catalog fields, built-in metadata tools, and export/run restrictions.
- Verification passed: `cargo fmt --check`, `cargo check -p logagent-server`, focused `http::tools`, `http::exports`, and `http::mcp_readonly` tests, `cargo test -p logagent-server -- --test-threads=1`, `cd webui && npm run lint`, `cd webui && npm run typecheck`, `cd webui && npm run build`, `git diff --check`, and local `/api/tools` sanity check with `examples/server-test.yaml`.

## 2026-06-15 LAN Auto Deploy

- Added `scripts/auto-deploy-lan.sh` and wired it into `scripts/build-all.sh`.
- On macOS, after local Server and WebUI builds complete, the helper pings `192.168.31.128`; when reachable it SSHes to `duzhiwang@192.168.31.128`, runs `git pull --ff-only` in the remote source tree, then runs the remote runtime `deploy/rebuild-install.sh` plus `logagentctl.sh start/status`.
- The helper can be disabled with `LOGAGENT_LAN_AUTO_DEPLOY=0` and supports overrides for remote address, SSH host, and runtime deploy directory.
- `deploy/rebuild-install.sh` and the LAN auto-deploy SSH payload now load `$HOME/.cargo/env` when present so rustup-installed cargo is available in non-interactive SSH shells.
- Runtime deploy scripts now best-effort load `$HOME/.bashrc` before deploy `.env`, matching the LAN server where LogAgent runtime environment variables live in `/home/duzhiwang/.bashrc`.
- Deployment docs/specs were updated with the LAN auto-deploy behavior.
- Verification passed: `bash -n scripts/auto-deploy-lan.sh`, `bash -n scripts/build-all.sh`, `LOGAGENT_LAN_AUTO_DEPLOY=0 scripts/auto-deploy-lan.sh`, and `git diff --check`.

## 2026-06-15 Metadata Raw Refresh And Delete

- Stopped local `logagent-server` processes, including the existing screen-managed local service, before changing the code.
- Added protected metadata instance maintenance APIs: `POST /api/metadata/instances/:instance_id/refresh` rebuilds the stored openGemini snapshot from its saved `rawSnapshot`, and `DELETE /api/metadata/instances/:instance_id` removes the instance plus its non-shared cluster/node records.
- Metadata import confirmation now treats `instanceId` as the overwrite boundary. Re-importing the same instance replaces the old snapshot and clears stale cluster/node records instead of leaving residual topology behind.
- System Context / Metadata WebUI now has a `Raw JSON 刷新` button and per-instance delete controls. Successful refresh/delete updates the imported list and right-side snapshot state.
- Updated Server, WebUI, Metadata module, and root specs/readmes for Raw JSON refresh, deletion, and duplicate InstanceID overwrite behavior.
- Verification passed: `cargo fmt --check`, `cargo check`, `cargo test -p logagent-server services::metadata -- --test-threads=1`, `cargo test -p logagent-server -- --test-threads=1`, `cd webui && npm run lint`, `cd webui && npm run typecheck`, `cd webui && npm run build`, and `git diff --check`.

## 2026-06-15 Metadata Schema Field Type Display Follow-up

- Fixed the Metadata Schemas type renderer to resolve field type codes from `typ`, `type`, `Typ`, or `Type` and coerce numeric strings before mapping labels.
- Updated the openGemini display mapping to match `FieldTypeName`: `0 Unknown`, `1 Integer`, `2 Unsigned`, `3 Float`, `4 String`, `5 Boolean`, `6 Tag`, and `7 Unknown`.
- Metadata JSON template import now accepts `Typ`, `Type`, and `type` aliases for field type codes so newly imported schemas do not persist empty field types.
- Verification passed: `cargo fmt --check`, `cargo check`, `cargo test -p logagent-server services::metadata -- --test-threads=1`, `cargo test -p logagent-server mcp -- --test-threads=1`, `cargo test -p logagent-server -- --test-threads=1`, `cd webui && npm run lint`, `cd webui && npm run typecheck`, `cd webui && npm run build`, and `git diff --check`.

## 2026-06-15 Metadata Field Type Lookup Tool

- Added built-in MCP tool `logagent.get_metadata_field_types` for task stdio MCP and read-only HTTP MCP.
- The tool requires `instanceId`, `database`, and `measurement`; `retentionPolicy` is optional and defaults to the DB default RP, while `field` may be omitted, a single string, or an array of field names.
- Returned field entries include raw `typ`, mapped `typeLabel`, and `endTime`; the openGemini mapping is `0 Unknown`, `1 Integer`, `2 Unsigned`, `3 Float`, `4 String`, `5 Boolean`, `6 Tag`, and `7 Unknown`.
- Task MCP lookups write `metadata_slices/field_types_<stable_id>.json`, audit the call in `mcp_calls.jsonl`, and remain background context only.
- Verification passed: `cargo fmt --check`, `cargo check`, focused metadata/MCP/read-only MCP tests, `cargo test -p logagent-server -- --test-threads=1`, and `git diff --check`.

## 2026-06-15 Metadata Schema Field Type Mapping Fix

- Fixed Metadata Schemas field type display by routing the table through a dedicated openGemini field type mapping helper.
- The WebUI mapping is now explicit for `Field_Type_Unknown=0`, `Field_Type_Int=1`, `Field_Type_UInt=2`, `Field_Type_Float=3`, `Field_Type_String=4`, `Field_Type_Boolean=5`, `Field_Type_Tag=6`, and `Field_Type_Last=7`.
- Updated WebUI and Metadata docs/specs with the exact enum-to-label mapping.
- Verification passed: `cd webui && npm run lint`, `cd webui && npm run typecheck`, and `cd webui && npm run build`.

## 2026-06-15 System Context Skill Import

- Added protected `POST /api/skills/imports` for Markdown Diagnostic Skill imports. The API writes `<skillId>/SKILL.md` and default `logagent.json` under the first configured `skills.roots`, reloads the Skill Registry snapshot, and returns the imported Skill detail.
- Skill Registry now uses a reloadable in-memory snapshot so Skills list/detail, Analyze Skill resolve, read-only MCP, and `skills.zip` use imported Skills without restarting Server. Reload failure rolls back the newly created import directory and keeps the old snapshot.
- System Context / Skills WebUI now has an Import control beside Refresh. Users can choose `.md/.markdown` files or paste Markdown, frontmatter can prefill name/description, and successful imports refresh the list, select the new Skill, and close the form.
- Updated Server, WebUI, Skills and System Context docs/specs with the import API, storage location, generated manifest defaults, v1 limitations, and UI behavior.
- Verification passed: `cargo fmt --check`, `cargo check`, `cargo test -p logagent-server services::skill_registry -- --test-threads=1`, `cargo test -- --test-threads=1`, `cd webui && npm run lint`, `cd webui && npm run typecheck`, `cd webui && npm run build`, and `git diff --check`.

## 2026-06-15 WAITING_FOR_USER Finalize Button Fix

- Fixed the Analyze task execution `WAITING_FOR_USER` finalize path: the parent callback now preserves the child component's `resumeMode: "finalize"` argument instead of dropping it and falling back to `continue`.
- The finalize button label now matches the product wording `没有更多信息，直接生成最终结果`, and waiting interaction buttons explicitly use non-submit button semantics.
- Verification passed: `cd webui && npm run lint`, `npm run typecheck`, and `npm run build`.

## Status Summary

LogAgent MVP is now framed as a diagnostic evidence workbench and Claude Code domain enhancement layer. The self-built analysis action loop is no longer the running analysis path; `PLAN_ANALYSIS` prepares LogAgent evidence/MCP artifacts, starts or resumes Claude Code, and persists Claude session outcomes, MCP calls, waiting state and final evidence.

Current runnable loop:

```text
Chrome Extension, WEBUI upload, or WEBUI question-only Session
  -> optional Native Agent or Server upload API
  -> persisted QUEUED task and raw/text snapshot
  -> bounded background extraction / manifest
  -> simple grep evidence
  -> optional rule-based Tool Runner evidence
  -> analysis_state.json / analysis_events.jsonl audit snapshot
  -> analysis_package.json / claude_prompt.md / claude_mcp_config.json
  -> Claude Code CLI --print --output-format json --json-schema --mcp-config with short stdin prompt
  -> Claude reads full evidence package through task MCP analysis_package resource
  -> LogAgent MCP resources/tools for logs, slices, tools, Metadata, Cases, prompts and approvals
  -> claude_session.json / mcp_calls.jsonl / agent_response.json
  -> completed final answer, WAITING_FOR_USER, WAITING_FOR_APPROVAL, or PLAN_ANALYSIS failure
  -> persisted result and WEBUI display
  -> optional human confirmation or LLM-assisted text import into local Memory-backed Case Store
```

Current manual Tools loop:

```text
WEBUI Tools
  -> upload pprof profile through existing upload API
  -> create persisted taskKind=tool_run task
  -> background RUN_TOOL execution
  -> go tool pprof top/tree/raw artifacts
  -> /api/tools/runs polling and result display
```

Current Remote Executor loop:

```text
WEBUI Tools / Executors
  -> create or edit ECS executor records in Server JSON store
  -> select a remote_execution command template
  -> create taskKind=remote_command_run
  -> background EXECUTE_REMOTE_COMMAND phase calls system ssh with BatchMode=yes
  -> remote_command/result.json, stdout.txt, stderr.txt
  -> /api/executor-runs polling and result display
```

## Implemented

### Remote Executor Framework

- Added `remote_execution` Server config with system `ssh` path, host key policy, connect/command timeouts, output cap, and command templates. Empty command config defaults to `smoke_ls_root`, which runs `ls -la /root`.
- Added WebUI-managed ECS executor records under `storage.data_dir/executors`, plus protected APIs for executor CRUD, command template discovery, remote run creation, run polling, and result reads.
- Added `taskKind=remote_command_run`, `source=remote_executor`, and `EXECUTE_REMOTE_COMMAND` phase. Remote command runs reuse TaskStore, workspaces, background executor recovery, and write `remote_command/{result.json,stdout.txt,stderr.txt}`.
- Added `Tools / Executors` WebUI subpage for executor create/edit/disable, template selection, SSH run creation, run history polling, stdout/stderr preview, and artifact paths. The UI does not expose free-form shell command input.
- Updated Server, WebUI, Environment Collector, Config, Testing, Deploy, root docs/specs, and example configs. Full Environment Collector SCP collection and Analysis Agent approval mapping remain follow-up work.
- Verification passed: `cargo fmt --check`, `cargo check`, `cargo test -- --test-threads=1` (native 1 + server 129 tests), `cd webui && npm run lint`, `cd webui && npm run typecheck`, `cd webui && npm run build`, and direct low-risk SSH smoke `ssh root@112.74.50.120 ls -la /root`.

### Architecture Review

- Added `docs/architecture_review.md` with a current real architecture diagram, module-by-module implemented capability inventory, README/SPEC/roadmap gap analysis, and the top 10 extension-blocking architecture issues.
- The review cites concrete code evidence by file and function, including the task executor, task/stdout MCP tools, final evidence validation, Metadata fetch/store behavior, Native Agent local boundary, Tool Runner/Tools split, and WebUI evidence navigation.
- Reorganized the path from the current state to a complete MVP loop into P0/P1/P2 tasks, marking which tasks are suitable for Codex Implement mode and which require human environment/configuration input.
- Added a suggested 2-week development rhythm focused first on evidence contract stability, action budget guardrails, failed artifact observability, tool platform cleanup, security hardening, Code Evidence, and Environment Collector.
- Verification: docs-only change; `git diff --check` passed.

### Claude CLI Prompt Delivery

- Changed Claude Code session runner prompt delivery from a full prompt argv argument to a short `claude_prompt.md` artifact piped through stdin.
- Stopped inlining `analysis_package.json` into the Claude startup prompt. Claude now reads the full task evidence package through the task-scoped stdio MCP `analysis_package` resource.
- Added `promptDelivery` metadata to `claude_session.json` and `agent_response.json`, recording `mode=stdin_file`, `promptPath=claude_prompt.md`, prompt byte size and `largeContextVia=mcp_resource`.
- Added task MCP `analysis_package` resource backed by workspace `analysis_package.json`; existing permission profiles remain unchanged, so `diagnose` still does not need native file `Read`.
- Kept Claude CLI as the production backend for this change. Claude Agent SDK remains a later adapter PoC option for SDK streaming/hooks/session runtime needs; it is not required to solve oversized prompt delivery.
- Verification: `cargo fmt --check`, `cargo check`, focused `cargo test -p logagent-server agent_backend -- --test-threads=1`, focused MCP resource test, `cargo test -p logagent-server -- --test-threads=1`, and workspace `cargo test -- --test-threads=1` pass.

### Dual Entry: WebUI Analyze + Read-only HTTP MCP

- Renamed the primary WebUI navigation entry from `Log Analysis` to `Analyze`; the underlying workflow remains Session-first and continues to use the Server machine's Claude Code, task-scoped stdio MCP config and Server local workspace.
- Added protected read-only HTTP MCP at `POST /api/mcp/readonly` for personal local Claude Code knowledge access. It supports `initialize`, `resources/list`, `resources/read`, `tools/list` and `tools/call`.
- Read-only MCP resources now cover Skills, Metadata instances/snapshots, recent Cases, Tools catalog and Domain Adapters. Read-only tools cover case search/get, skill list/get/reference, System Context preview, metadata list/snapshot, tool catalog and domain adapter list.
- The read-only MCP is deliberately separated from `logagent-server mcp --task-id ...`: it does not read task workspaces, start or resume Sessions, upload files, request approval, run Tool Runner, SSH/SCP, or mutate Case/Metadata/Skills/System Context.
- Added protected `GET /api/exports/skills.zip`; it packages all indexed Skill ordinary files, keeps relative Skill directory structure, writes `manifest.json`, and skips symlinks.
- Added protected `GET /api/exports/tools.zip`; it packages enabled executable tool binaries for the Server OS/arch, wrappers, sanitized config examples and `tools-manifest.json`; missing, non-file, non-executable or unreadable tools are marked skipped without failing the download.
- Added Settings `Personal Claude Code` block showing the read-only MCP URL, Authorization header hint, Claude Code HTTP MCP config example, and authenticated download buttons for Skills and Tools packages. It intentionally does not install, bootstrap or write local Claude Code configuration.
- Updated root, Server, WebUI, Skills, System Context, Tool Runner, Metadata, Case Store, Domain Adapter, Interfaces, Deployment and Security docs/specs.
- Verification: `cargo fmt --check`, `cargo check`, `cargo test -- --test-threads=1`, `cd webui && npm run lint`, `cd webui && npm run typecheck`, and `cd webui && npm run build` pass. A default parallel `cargo test -p logagent-server` run hit existing timeout-style failures, while the same server suite passed with `--test-threads=1`. Vite HTTP smoke returned `/`; in-app Browser smoke could not run because the `iab` browser was unavailable in this session.

### Skill-backed System Context

- Reworked System Context semantics from editable long-lived Prompt Pack/Runbook/Architecture resources into task-level Skill-backed background snapshots.
- Added Server `SkillRegistry` with `skills` config (`enabled`, `roots`, `max_skill_chars`, `max_reference_chars`), defaulting to repository `skills/`.
- Added Codex-compatible initial Skills: `opengemini-diagnosis`, `influxql-analysis`, and `pprof-diagnosis`, each with `SKILL.md`, optional `logagent.json`, and declared `references/`.
- Added protected Skills APIs: `GET /api/skills`, `GET /api/skills/:skill_id`, and `POST /api/skills/preview`.
- Added `AnalysisSessionRecord.skillIds`; `systemContextIds` remains deserializable for old Sessions but no longer injects old non-Metadata resources into new tasks.
- Log Analysis task creation now writes `system_context.json` schema v2 containing selected/auto-matched `diagnostic_skill` items plus Metadata adapter summaries.
- Added MCP `logagent.get_skill_reference`, restricted to references declared by Skills already snapshotted in the current task; reads write `skill_references/<stable_id>.json` background artifacts.
- Final result evidence validation now explicitly rejects `system_context.json`, `diagnostic_skill`, and `skill_references/*` as root-cause evidence refs.
- WebUI `System Context` page now has `Skills` and `Metadata` tabs; Log Analysis draft uses a Skill picker backed by `/api/skills` and persists `skillIds`.
- Updated root, Server, WebUI, System Context, Config, and new Skills docs, plus example configs.
- Verification: `cargo fmt --check`, `cargo check`, `cargo test -- --test-threads=1`, `cd webui && npm run lint`, `cd webui && npm run typecheck`, and `cd webui && npm run build` pass. Added focused Rust coverage for task Skill snapshots and MCP Skill reference artifacts. Local smoke with `examples/server-test.yaml` returned HTTP 200 for `/`, listed the three initial Skills from `/api/skills`, and previewed selected plus auto-matched Skills via `/api/skills/preview`.

### Server Observability

- Added default Server tracing when `RUST_LOG` is unset: `logagent_server=info,tower_http=info`, with logs written to stderr so MCP stdio stdout remains JSON-RPC only.
- Configured HTTP trace logging for request/response/failure summaries and changed `AppError` logging so 4xx/409 responses are warnings while 5xx responses are errors.
- Added lifecycle logs for uploads, Sessions, Tasks, Tool runs, Case imports, Metadata imports, System Context writes, Executor phase transitions, waiting-state resumes, Claude Code sessions, MCP calls, Tool Runner execution/reuse, and pprof analyzer runs.
- Added focused English comments around stderr logging for MCP safety, error-level mapping, persisted phase recovery, MCP waiting marker handoff, and Tool Runner idempotent artifact reuse.
- Updated Server README/SPEC with logging defaults, level policy, covered events, and sensitive-data exclusions.
- Verification: `cargo fmt --check`, `git diff --check`, `cargo check -p logagent-server`, and `cargo test -p logagent-server` pass.

### Claude Code MCP Session Runner

- Replaced the running `PLAN_ANALYSIS` action-loop path with Claude Code session orchestration. The executor now writes `analysis_package.json` and `claude_mcp_config.json`, invokes Claude Code, validates completed final evidence refs, and persists waiting user/approval requests from MCP markers.
- Added `claude_code` config, `mcp` config, `analysisMode=diagnose|code_investigation|fix`, mode-specific permission profiles, and the `logagent-server mcp --config <path> --task-id <task_id> --mode <mode>` stdio subcommand.
- Added LogAgent MCP resources and tools for task/artifact context, manifest/grep, metadata, system context, case context, tool results, log search, log slices, domain tool execution, case recall, metadata topology, user input requests and approval requests. MCP calls append to `mcp_calls.jsonl` and evidence-producing tools write workspace artifacts.
- Redefined `agent_response.json` as a Claude Code session response with `runtimeStatus`, `claudeSessionId`, `analysisMode`, `permissionProfile`, `structuredOutput`, usage/cost, MCP call path, native tool policy, duration, error and stdout preview. Added `claude_session.json` for resume metadata.
- Removed `agent_request.json` from the task artifacts API and WebUI artifact model. The old AgentDecision parser helpers are test-gated in LLM Gateway; normal runtime validation uses final-answer evidence validation directly.
- Updated sample/deploy configs to use `LOGAGENT_CLAUDE_CODE_PATH`, `claude_code`, and `mcp.transport=stdio`; updated root, Server, WebUI, deploy and module README/SPEC docs for the Claude Code MCP architecture.
- Verification: `cargo fmt --check`, `cargo check`, `cargo test`, `npm --prefix webui run lint`, `npm --prefix webui run typecheck`, and `npm --prefix webui run build` pass.

### Agent Contract Artifacts

- Added `claude_agent_sdk` as an Agent Backend type with `agent_sdk_adapter` execution mode; enabled external adapters still require an absolute configured command path, and Settings diagnostics only inspect the path without invoking the adapter.
- Log Analysis `PLAN_ANALYSIS` now refreshes `analysis_package.json`, `agent_request.json`, and `agent_response.json` in the task workspace before each internal decision call. The package freezes task input, manifest, grep, Metadata, System Context, Case context, Tool results, analysis state summary, and Server execution boundaries.
- `agent_response.json` is currently an explicit `not_invoked` placeholder: the production decision loop still uses `internal_llm`, and the contract artifacts prepare the next Claude Agent SDK adapter PoC without changing the running analysis path.
- Task artifacts API now returns optional `analysisPackage`, `agentRequest`, and `agentResponse`; WebUI successful task details show an Agent contract panel with backend, execution mode, runtime status, and artifact paths.
- Updated sample and deploy configs with disabled `claude_agent_sdk` entries and optional `LOGAGENT_AGENT_CLAUDE_SDK_PATH`.
- Verification: `cargo fmt --check`, `cargo check`, `cargo test`, `npm --prefix webui run lint`, `npm --prefix webui run typecheck`, and `npm --prefix webui run build` pass.

### Agent Backend And Domain Adapter Direction

- Reframed LogAgent as a diagnostic evidence workbench that can call mature agent backends instead of trying to replace Codex, Claude Code or OpenCode with a fully self-built general agent loop.
- Added Server `agent_backends` config with default `internal_llm` and reserved `claude_agent_sdk`, `codex_cli`, `claude_code_cli` and `opencode_cli` backend types.
- Added protected Settings APIs:
  - `GET /api/settings/agent-backends`
  - `POST /api/settings/agent-backends/:backend_id/test`
  - `GET /api/settings/domain-adapters`
- Agent backend diagnostics are first-stage dry-run checks: `internal_llm` returns ready; external adapters validate configured command paths but do not execute the adapter.
- Added an in-process Domain Adapter registry with active `opengemini_influxdb` and skeleton `cassandra` / `rocksdb` adapters.
- Extended WebUI Settings to show LLM diagnostics, Agent Backend summaries/dry-run results, and Domain Adapter status.
- Added Agent Backends and Domain Adapters module docs, and updated root, Server, WebUI, config, interfaces, security, analysis, LLM Gateway, Tool Runner, Metadata, Code Evidence, Environment Collector, System Context and Roadmap docs for the new direction.
- Verification: `cargo fmt --check`, `cargo check`, `cargo test` (106 server tests plus native-agent test pass), `cd webui && npm run lint`, `cd webui && npm run typecheck`, and `cd webui && npm run build` pass. Local Vite dev server started successfully, but in-app Browser visual smoke could not run because no browser instance was available.

### Settings LLM Diagnostics

- Added protected Server Settings APIs for LLM diagnostics: configuration summary, model list test and simple chat message test.
- OpenAI-compatible model list diagnostics call the provider `/models` endpoint; stub and binary providers report the configured model.
- Diagnostic model/chat calls return `{ok,result,error}` so WebUI can print provider HTTP, auth, rate-limit, network, timeout or decode exceptions directly.
- Added a WebUI `Settings` top navigation page with LLM configuration summary, model list fetch test, simple message send test and raw success/error output panels.
- Updated WebUI, Server and LLM Gateway docs/specs for the new Settings page and APIs.
- Verification: `cargo fmt --check`, `cargo check`, `cargo test`, `cd webui && npm run lint`, `cd webui && npm run typecheck`, `cd webui && npm run build`, local Vite HTTP smoke, and stub LLM Settings API smoke pass; in-app Browser was unavailable for visual verification.

### Metadata Page Usability Refinements

- Raw JSON now renders as a lazy expandable tree instead of stringifying and rendering the full snapshot on initial page load.
- Imported Instances can collapse into a narrow rail and expand back without losing selection.
- Metadata Explorer now combines the former topology and database detail entry points into `Node / DBPT / Shards` and `DB / RP / Shards / Indexes` views.
- Schemas now defaults to the first non-`_internal` database and its first retention policy, keeps RP options scoped to the selected DB, and renders field type codes as names.
- Nodes now display MetaNode status as `none` and map Data/SQL node status codes to `none`, `alive`, `leaving`, `left`, and `failed`.
- Verification: `cd webui && npm run lint`, `cd webui && npm run typecheck`, `cd webui && npm run build`, and local Vite HTTP smoke pass; in-app Browser was unavailable for visual verification.

### Sticky Table Headers

- Metadata reusable tables now use bounded local scrolling with sticky headers, so Nodes, Partitions, Explorer shard/index rows and Schemas field rows keep column meanings visible while scrolling.
- Tools pprof top table now uses the same sticky-header scrolling behavior.
- Updated WebUI and Metadata module docs/specs for the long-table browsing behavior.
- Verification: `cd webui && npm run lint`, `cd webui && npm run typecheck`, `cd webui && npm run build`, and local Vite HTTP smoke pass; in-app Browser was unavailable for visual verification.

### Metadata Cascading Views

- Removed the WebUI Metadata graph rendering path and the `@xyflow/react` dependency; Topology is now rendered as a cascade instead of a React Flow graph.
- Topology now expands as `Database -> DataNode -> DBPT -> Shards`, with each Shard row showing RP, ShardGroup, time range, Owners, IndexID and Index status information.
- Databases now expands as `Database -> RP -> ShardGroup/IndexGroup -> Shard/Index`, so large clusters no longer render every detail table at once.
- Schemas now requires Database, RP, Measurement or field filtering before results render, preventing all schemas from being laid out by default.
- Updated WebUI and Metadata module docs/specs for the cascading topology, database and schema views.
- Verification: `cd webui && npm run typecheck`, `cd webui && npm run lint`, `cd webui && npm run build`, and `git diff --check` pass.

### Metadata Topology Explorer

- WebUI Metadata Topology no longer builds a full graph by default. It now derives a topology index and starts with an abnormal-first PT overview table.
- The overview groups by DataNode / Database / PT and shows ShardGroup, Shard, IndexGroup, Index, diagnostic count, owner and time range summaries.
- The right-side details panel supports aggregate PT details.
- Updated WebUI and Metadata module docs/specs for the overview-first topology behavior.
- Verification: `cd webui && npm run typecheck`, `cd webui && npm run lint`, `cd webui && npm run build`, and `git diff --check` pass; in-app Browser was unavailable for visual verification.

### Metadata Import UX

- WebUI Metadata tab now exposes three explicit import paths in the management panel: realtime openGemini `/getdata` URL loading, `.json` file upload, and manual JSON text paste.
- Realtime loading keeps the existing readonly snapshot behavior and still requires a user-provided InstanceID for openGemini metadata.
- JSON file and manual JSON text imports call `/api/metadata/imports` with `templateType=json`, then require preview and confirmation before writing to the Metadata Store.
- Full Metadata JSON templates can be previewed without an InstanceID; raw openGemini JSON still requires InstanceID so the Server can normalize it into the instance-keyed topology model.
- Updated WebUI and Metadata module docs/specs for the three import modes.
- Verification: `cd webui && npm run lint`, `cd webui && npm run typecheck`, `cd webui && npm run build`, and `git diff --check` pass.

### Memory-backed Case Store

- Added Server `MemoryStore` backed by `storage.data_dir/memory/memory.sqlite` with `memory_items`, `memory_chunks`, and FTS5 `memory_chunks_fts`.
- Converted `CaseStore` into a compatibility facade over Memory while preserving existing `/api/cases*`, `CaseRecord`, `CaseSearchHit`, `case_context.json`, and `case_context.json#cases/<index>` evidence refs.
- Server startup now idempotently imports legacy `storage.data_dir/cases/*.json` by `caseId` into SQLite. Legacy JSON files remain untouched as migration/rollback source, and create/update still syncs JSON.
- Case recall now filters `memoryType=case`, active status, and enabled state, then merges SQLite FTS/BM25 scores with existing keyword-overlap scoring. If FTS is unavailable, recall falls back to keyword overlap.
- Added disabled embedding config shape (`embedding.enabled/provider/model/api_key_env/store`) for future vector retrieval without changing API contracts.
- WebUI now defaults to `Log Analysis` and top navigation order is `Log Analysis`, `Memory`, `System Context`, `Tools`; the old top-level Metadata path is removed and Metadata remains reachable inside System Context.
- The former Cases page is presented as `Memory` while keeping Case terminology inside confirmed fault-case forms.
- Added Memory module docs and updated Server/WebUI/Case Store docs for storage, migration, recall, and UI naming.
- Verification so far: focused `cargo test -p logagent-server stores::case_store -- --nocapture` and `cargo check -p logagent-server` pass.

### Deploy Template Refresh

- Updated `deploy/logagent.example.yaml` with the default disabled `embedding` block so runtime deployments match the current Server config shape.
- Added optional `LOGAGENT_EMBEDDING_API_KEY` documentation to `deploy/.env.example`.
- Added `deploy/install-deps.sh` to install common source rebuild dependencies on macOS/Homebrew and common Linux package managers; SQLite remains bundled with the Server binary and is not installed separately.
- Updated `deploy/logagentctl.sh` and `deploy/rebuild-install.sh` to pre-create expected runtime data directories, including `data/memory`, `data/cases`, and `data/case_imports`, without deleting existing data.
- Updated deploy README and deployment module docs to document `data/memory/memory.sqlite`, legacy Case JSON rollback files, and the current WebUI navigation.

### Legacy System Context (superseded by Skill-backed System Context)

- Added Server `SystemContextStore`, persisted under `storage.data_dir/system_context/resources`, for versioned Prompt Pack, Architecture Doc, Runbook, Glossary, Tool Capability and Knowledge Note resources.
- Added protected System Context API: list/create/get/patch resources, add/patch/activate versions, and prompt preview.
- Metadata is now included under System Context through a read-only `metadata_instance` adapter while existing `/api/metadata/*` APIs and topology models remain unchanged.
- `AnalysisSessionRecord` now persists `systemContextIds`; Log Analysis task creation resolves explicit Session selections, enabled matching resources and Metadata adapter summaries into `workspaces/<task_id>/system_context.json`.
- `TaskRecord` and artifacts responses now expose `systemContextPath` / `systemContext`; Session timeline records `system_context_recorded`.
- LLM Gateway now injects System Context as background reference for both final-result generation and action decisions; System Context cannot be used as final result evidence refs.
- WebUI now has a top-level `System Context` page with resource library, version management, Prompt preview, Architecture Mermaid source preview and embedded Metadata tab; Log Analysis Session draft can select System Context resources and displays the frozen snapshot after a run.
- Verification: `cargo fmt --check`, `cargo check`, `cargo test`, `cd webui && npm run lint`, `cd webui && npm run typecheck`, and `cd webui && npm run build` pass.

This behavior is retained only for legacy data/API compatibility. New tasks and the current WebUI use the Skill-backed System Context flow documented above.

### Runtime Deploy Template

- Added repository `deploy/` runtime template copied from `/home/duzhiwang/workspace/data/prd_assistant/deploy`: README, `.env.example`, `logagent.example.yaml`, `logagentctl.sh`, and `rebuild-install.sh`.
- Real runtime secrets/config remain excluded: `deploy/.env` and `deploy/logagent.yaml` are ignored.
- Deployment scripts now auto-load same-directory `.env`; `logagentctl.sh` starts Server detached so non-interactive script execution can keep the process alive.
- Verification: `bash -n deploy/logagentctl.sh`, `bash -n deploy/rebuild-install.sh`, and `git diff --check` pass.

### Session-first Log Analysis

- Log Analysis 公开入口从一次性 task 改为可恢复 Session。
- 新增 Server `AnalysisSessionStore`，持久化到 `storage.data_dir/sessions/<session_id>.json`，Session events 追加到 `storage.data_dir/session_workspaces/<session_id>/session_events.jsonl`。
- 新增受保护 Session API：`POST/GET /api/sessions`、`GET/PATCH /api/sessions/:session_id`、`POST /api/sessions/:session_id/uploads`、`DELETE /api/sessions/:session_id/uploads/:upload_id`、`POST /api/sessions/:session_id/tasks`、`GET /api/sessions/:session_id/timeline`。
- `TaskRecord` schema 增加 `sessionId`；`log_analysis` task 必须绑定 Session，`tool_run` 不绑定 Session。
- 每次从 Session 启动分析都会创建新的 `log_analysis` task workspace 快照，Session 记录 `taskIds`、`activeTaskId` 和状态。
- Task 状态进入 running、waiting、succeeded、failed 时会同步 Session status，并追加 timeline event。
- Task 创建时继续固化 `metadata_context.json` 和 `case_context.json`，同时向 Session timeline 记录 Metadata summary 和 Case recall count。
- WebUI `Log analysis` 页面改为 Session-first：左侧 Session history，右侧 Session draft editor、upload attach、Start analysis、runs panel 和 unified Evidence timeline；草稿字段 debounce PATCH 到 Server。
- WebUI 选择 Session 时 best-effort 调用本机 Native Agent `PUT /workspace/current` 设置活动 Session，失败只提示不阻断 WebUI 上传。
- Native Agent 新增 `native_agent.state_path`，默认 `~/.logagent/native-agent-state.json`，并提供 `GET/PUT/DELETE /workspace/current`。
- Native Agent `POST /imports` 上传后附加到活动 Session；没有活动 Session 时自动创建 `Native import <filename>` Session 并设为活动；返回 `{uploadId, sessionId, taskId:null, url}`。
- Chrome Extension 成功通知改为 `LogAgent session updated`。
- Verification: `cargo fmt --check`, `cargo check`, `cargo test`, `npm run lint`, `npm run typecheck`, and `npm run build` pass after implementation.

### Text-only Log Analysis

- Log Analysis Session 现在支持不上传日志直接分析，只填写 Session 问题即可点击 `Start analysis`。
- Server `POST /api/sessions/:session_id/tasks` 和兼容 `POST /api/tasks` 在绑定 `sessionId` 后允许 `uploadIds=[]`；有上传时仍校验上传存在且为 `COMPLETE`。
- Text-only task 持久化 `uploadIds=[]`、`inputs=[]`，创建空 `raw/` 和 `extracted/`，并生成 `session_text_input.json`、`manifest.json` / `grep_results.json`。其中 `manifest.uploads`、`manifest.files` 和 grep `matches` 为空。
- Session timeline 会记录 `text_input_recorded`，用于区分只来自问题文本的 run；`session_text_input.json#question` 可作为最终结果 evidence ref。
- WebUI `Log analysis` 的 `Start analysis` 不再依赖已附加上传，Session draft 中标明 uploads optional；成功 artifacts 会展示 Session text input 并支持该 evidence ref 跳转。
- Verification: focused Rust regressions, `cargo fmt --check`, `cargo check`, `cargo test`, `npm run lint`, `npm run typecheck`, and `npm run build` pass.

### Log Analysis Collapse UX

- WebUI `Log analysis` 的 `Session draft` 现在支持展开/收起；新建空 Session 默认展开，已有 run 的 Session 默认收起，点击 `Start analysis` 创建 run 后会自动收起。
- Session draft 收起态展示 title、question、source URL、Metadata 绑定、upload/run 数量和 session 状态摘要，避免运行中占用主要视野。
- Unified Evidence Timeline 现在支持展开/收起；运行中 run 默认展开，切换到历史终态 run 或当前 run 到达 `SUCCEEDED` / `FAILED` 后自动收起。
- Timeline 收起态只展示最终结果 summary、confidence、失败 phase/message，或运行中的当前状态和最近事件；用户仍可手动展开查看完整 timeline。
- Verification: `npm run lint`, `npm run typecheck`, and `npm run build` pass in `webui/`.

### Task Alias Naming

- 成功的 Log Analysis task 现在持久化 `alias` 字段；新写入的 Log Analysis task 使用 schemaVersion 7，tool_run 使用 schemaVersion 6，旧 task 缺少 alias 时仍可读取。
- Task alias 在最终结果写入后由 LLM Gateway 静默生成，输入为用户问题、最终结果、manifest 和 Metadata 摘要；命名调用不写入 `analysis_events.jsonl`，也不追加 Session timeline event。
- alias schema 错误会重试一次；Provider 或 schema 最终失败时 Server 使用最终 summary/question 生成短标题，避免命名失败影响 task 成功状态。
- WebUI Runs、timeline 收起态和 Case 确认区优先展示 alias；没有 alias 时用状态/时间回退，不再把裸 `task_...` 当主要显示名称。
- Verification: `cargo fmt --check`, `cargo check`, `cargo test`, `npm run lint`, `npm run typecheck`, and `npm run build` pass.

### Case Evidence Ref Normalization

- 修复线上 Session `sess_1781100427508_1` 中 `task_1781103906266_1` 的 `PLAN_ANALYSIS` 失败原因：模型把历史 Case 输出为 `历史案例 case_1781027802189_1`，旧校验无法映射该 evidence ref。
- LLM Gateway 现在在 Prompt 中给历史 Case 标注 `case_context.json#cases/<index>`，并把模型输出的 `case_<id>` 或“历史案例 case_<id>”规范化为当前 task `case_context.json` 中的 canonical ref。
- 最终结果允许引用 `case_context.json#cases/<index>`；未知 Case、缺失 case context 或越界 index 仍会拒绝。
- WebUI 现在支持点击 `case_context.json#cases/<index>` 跳转到对应 Case context 条目。
- Verification: `cargo fmt --check`, `cargo check`, `cargo test`, `npm run lint`, `npm run typecheck`, and `npm run build` pass; deployed to `/home/duzhiwang/workspace/data/prd_assistant`, rebuilt Server/WebUI, restarted with deployment env, and `logagentctl.sh status` returned `{"status":"ok"}`.

### WebUI Naming

- Renamed the top bar product title from `LogAgent Metadata Console` to `LogAgent Analysis Workbench`.
- Updated the subtitle to describe the broader WebUI scope: evidence, metadata, tools, and case workflow.
- Verification: `npm run lint`, `npm run typecheck`, and `npm run build` from `webui/` all pass.

### Repository Structure

- Root directory now keeps only runnable components and engineering support directories: `server/`, `native-agent/`, `chrome-extension/`, `webui/`, `examples/`, `scripts/`, and `testing/`.
- Former planning-only capability directories were moved under `docs/modules/`, including Log Analyzer, Tool Runner, Metadata, Analysis Agent, LLM Gateway, Case Store, Code Evidence, Environment Collector, Config, Interfaces, Security, Deployment, and Roadmap.
- Server remains a single Rust crate. Internal code is now organized by layer:
  - `http/` for routes and handlers.
  - `domain/` for shared DTOs and Action/Evidence contracts.
  - `stores/` for JSON-backed persistence.
  - `services/` for Log Analyzer, Tool Runner, Metadata, LLM Gateway, and Tools plugin implementations.
  - `pipeline/` for task phase handlers and the recoverable executor.
  - `support/` for config, auth, errors, IDs, and path safety.
- The refactor is structure-only: HTTP API paths, config keys, task/upload/case JSON schema, and workspace artifact paths remain unchanged.
- Verification: `cargo fmt --check`, `cargo check`, and `cargo test` all pass after the module move.

### Runtime Scripts

- Added `scripts/init-workdir.sh`, `scripts/build-server.sh`, `scripts/build-webui.sh`, `scripts/build-all.sh`, and `scripts/server-service.sh`.
- All new workdir scripts require `LOGAGENT_WORK_DIR`; if it is unset or empty they fail before writing runtime files.
- `init-workdir.sh` creates `bin/`, `config/`, `data/`, `logs/`, `run/`, and `webui/`, then writes `config/server.yaml` with `storage.data_dir` under the work directory.
- `build-server.sh` compiles the release Server and installs it to `$LOGAGENT_WORK_DIR/bin/logagent-server`.
- `build-webui.sh` runs the WebUI production build and syncs `webui/out` to `$LOGAGENT_WORK_DIR/webui/out`.
- `server-service.sh` manages start, stop, restart, status, and logs using `$LOGAGENT_WORK_DIR/run/logagent-server.pid` and `$LOGAGENT_WORK_DIR/logs/logagent-server.log`.
- Script validation: `bash -n` passes for the new scripts; every new entry script fails when `LOGAGENT_WORK_DIR` is missing; initialization smoke created the expected runtime tree; `build-server.sh` installed the release Server binary; `build-webui.sh` built and copied WebUI static output. `server-service.sh start` reached the Server startup step, but this Codex sandbox rejects local port binding (`PermissionError: [Errno 1] Operation not permitted` from a minimal Python socket bind), so live health-check startup cannot be completed in this environment.
- Verification: `cargo fmt --check`, `cargo check`, `cargo test`, `npm run lint`, `npm run typecheck`, and `npm run build` pass.

### Documentation Cleanup

- Removed obsolete root `plan.md`. Its early full-plan content duplicated current README/SPEC and `docs/modules/*` documents while retaining outdated module-directory and implementation assumptions.
- Root README now points readers to the maintained documentation set instead of the deleted historical draft.

### Case Store Import

- Reworked the top-level WebUI `Cases` page from direct manual entry to an LLM-assisted import workflow.
- Users can paste long Case text or upload UTF-8 text-like files (`.txt/.md/.log/.json/.yaml/.yml/.csv`); PDF/DOCX parsing is intentionally out of scope for this first pass.
- Server now persists Case import drafts under `storage.data_dir/case_imports/` and exposes `POST /api/cases/imports`, `GET/PATCH /api/cases/imports/:draft_id`, `POST /api/cases/imports/:draft_id/messages`, and `POST /api/cases/imports/:draft_id/confirm`.
- LLM Gateway now has a Case extraction call for stub, OpenAI-compatible, and binary providers. It returns `structuredCase`, `missingFields`, `assistantQuestion`, and `readyToConfirm`.
- Missing `title`, `symptom`, `rootCause`, or `solution` blocks confirmation; users can answer follow-up questions or edit the structured draft before saving.
- Confirmation still creates a normal `sourceType=manual` Case, so existing search, detail edit, enable/disable, and task recall behavior remain unchanged.
- Verification: `cargo test -p logagent-server`, `npm --prefix webui run typecheck`, `npm --prefix webui run lint`, and `npm --prefix webui run build` pass.

### Chrome Extension

- Manifest V3 extension exists under `chrome-extension/`.
- Watches Chrome download completion.
- Filters by configured URL prefixes and file suffixes.
- Shows a notification and calls Native Agent `POST /imports`.
- Options page supports `agentBaseUrl`, `urlPrefixes`, and `fileSuffixes`.

### Native Agent

- Rust/Axum local agent exists under `native-agent/`.
- Supports:
  - `GET /health`
  - `POST /imports`
- Validates:
  - allowed local directories
  - allowed suffixes
  - max upload size
- Uploads small files via multipart.
- Uploads large files via chunked upload.
- Calls Server task creation after upload.

### Server

- Rust/Axum server exists under `server/`.
- Supports:
  - `GET /health`
  - `POST /api/uploads`
  - `POST /api/uploads/batch`
  - `POST /api/uploads/init`
  - `POST /api/uploads/:upload_id/chunks?offset=<bytes>`
  - `POST /api/uploads/:upload_id/complete`
  - `POST /api/tasks`
  - `GET /api/tasks`
  - `GET /api/tasks/:task_id`
  - `GET /api/tasks/:task_id/artifacts`
  - `GET /api/tasks/:task_id/result`
  - `POST /api/tasks/:task_id/messages`
  - `POST /api/tasks/:task_id/actions/:action_id/decision`
  - `POST /api/tasks/:task_id/case`
  - `POST /api/cases`
  - `POST /api/cases/imports`
  - `GET /api/cases/imports/:draft_id`
  - `PATCH /api/cases/imports/:draft_id`
  - `POST /api/cases/imports/:draft_id/messages`
  - `POST /api/cases/imports/:draft_id/confirm`
  - `GET /api/cases`
  - `GET /api/cases/:case_id`
  - `PATCH /api/cases/:case_id`
  - `GET /api/metadata/instances`
  - `GET /api/metadata/instances/:instance_id`
  - `GET /api/metadata/instances/:instance_id/snapshot`
  - `GET /api/metadata/clusters/:cluster_id`
  - `GET /api/metadata/clusters/:cluster_id/nodes`
  - `POST /api/metadata/snapshots/fetch`
  - `POST /api/metadata/imports`
  - `POST /api/metadata/imports/fetch`
  - `GET /api/metadata/imports/:import_id/preview`
  - `POST /api/metadata/imports/:import_id/confirm`
  - `GET /api/tools`
  - `GET /api/tools/:tool_id`
  - `POST /api/tools/:tool_id/runs`
  - `GET /api/tools/runs`
  - `GET /api/tools/runs/:task_id`
  - `GET /api/tools/runs/:task_id/result`
  - `GET /api/tools/runs/:task_id/artifacts`
- Uses API Key middleware for protected APIs.
- Statically serves Vite output from `webui/out`.
- Creates Server-owned task IDs and workspaces.
- Persists each task as `storage.data_dir/tasks/<task_id>.json` with atomic replacement.
- Returns `202 Accepted` after raw snapshot creation and runs tasks in the background.
- Limits concurrent tasks with `server.max_concurrent_tasks` (default 2).
- Recovers `QUEUED` and interrupted `RUNNING` tasks after restart; successful and failed tasks remain terminal.
- Uses a phase-driven Executor dispatcher instead of a hard-coded linear executor.
- Preserves the interrupted phase across restart, increments attempts on resume, and reruns only that idempotent phase.
- Rejects stale phase advancement and inconsistent persisted `RUNNING`/`SUCCEEDED` state.
- Defines shared `TaskContext`, Action, EvidenceArtifact, and EvidenceProvider contracts for Tool Runner and later evidence modules.
- Runs optional rule-based Tool Runner actions during `RUN_TOOL` and exposes `toolResults` in artifacts.
- Runs `PLAN_ANALYSIS` after rule-based tools and consumes bounded multi-round LLM `action | final_answer` decisions.
- Executes `search_logs` action by rebuilding `grep_results.json` with model-provided keywords, then continues to the next analysis round.
- Executes LLM-selected `run_tool` action through the same whitelist Tool Runner channel, then continues to the next analysis round.
- Enters `WAITING_FOR_USER` for `ask_user`, persists `pendingUserPrompts`, accepts `POST /api/tasks/:task_id/messages`, records the user message, and resumes the same task from `PLAN_ANALYSIS`.
- Enters `WAITING_FOR_APPROVAL` for `collect_environment` / `REQUIRES_APPROVAL`, persists `pendingApprovals`, accepts approval or rejection through `POST /api/tasks/:task_id/actions/:action_id/decision`, and resumes the same task from `PLAN_ANALYSIS`.
- Approved environment collection currently writes mock `environment_evidence/<action_id>/result.json`; real SSH/SCP execution remains planned for Environment Collector.
- Successful tasks can now be manually confirmed into the local Case Store through `POST /api/tasks/:task_id/case`.
- Manual Cases can now be created through `POST /api/cases` without binding to a task.
- Case Store records now use schema v2 with `sourceType=task|manual`; task Cases require `taskId/sourceResultPath`, while manual Cases forbid those fields. Development-stage startup intentionally rejects old v1 Case JSON instead of migrating it.
- Case Store records are persisted as JSON under `storage.data_dir/cases`, loaded at startup, searchable through `GET /api/cases`, and can be updated or disabled through `PATCH /api/cases/:case_id`.
- Case search now indexes InstanceID, NodeID, and evidence refs in addition to title, symptom, root cause, solution, product, version, and environment.
- New tasks now recall up to 5 enabled Cases by question, persist `case_context.json`, expose `caseContext` in artifacts, and include historical Case references in the LLM prompt as non-authoritative context.
- Metadata now uses user-provided `instanceId` as the user-facing unique key. openGemini imports require an explicit InstanceID, preserve the raw openGemini `ClusterID` as `sourceClusterId`, expose an imported Instance list, and serve stored topology snapshots by InstanceID. Legacy cluster endpoints remain for compatibility.
- Metadata instances now support an optional `remark` display name. openGemini fetch/import requests accept it, the store persists it, WebUI shows it beside InstanceID, truncates it in the list, and includes it in Overview.
- Persists `final_answer` decisions directly as `result.json` / `result.md`.
- Stops repeated action fingerprints and exhausted analysis budgets with a low-confidence final result instead of an infinite loop.
- Rejects artifact reads before success with `409` and the current task status.
- Runs one LLM result generation phase after grep and persists `result.json` / `result.md`.
- `GENERATE_RESULT` now reads `tool_results/*/result.json` and passes Tool Runner summary/findings into LLM Gateway as citeable evidence.
- Persists Analysis State Store MVP files, `analysis_state.json` and `analysis_events.jsonl`, and serves them through `GET /api/tasks/:task_id/analysis`.
- Records LLM call lifecycle events for `PLAN_ANALYSIS` action decisions, including `llm_call_started`, `llm_call_completed`, `llm_call_schema_retry`, `llmcall_*` callId, attempt, model, and schema error details.
- Exposes `/api/debug/llm` for runtime LLM response-content logging control; the flag is in-memory, defaults off, and does not print prompts or API keys.
- `scripts/start-local.sh` background mode now disowns the server job after `nohup`, so local quick-start remains available after non-interactive zsh exits.
- Runtime deployment assets are now organized under `$LOGAGENT_APP_DIR/deploy`, including deploy steps, env sample, config sample, `logagentctl.sh`, and `rebuild-install.sh`. The scripts use `LOGAGENT_APP_DIR` and `LOGAGENT_SRC_DIR`, replace `$LOGAGENT_APP_DIR/bin/logagent-server`, sync WebUI static files, and restart only when the server was already running.
- Tools API now supports a separate `taskKind=tool_run` path for user-triggered tools. Tool runs reuse upload records, raw workspace snapshots, TaskStore, background executor, status polling, and `tool_results`, while `/api/tasks` remains scoped to log analysis tasks.
- `pprof_analyzer` is the first Tools plugin. It is configured through `tools.pprof_analyzer`, expects the path to a Go executable, runs `go tool pprof -top/-tree/-raw` with `PPROF_TMPDIR` inside the task workspace, optionally attempts SVG output, and writes a structured result with profile type, total, top functions, warnings, and artifact paths.
- Added `examples/server-pprof-tool.yaml` for pprof Tools smoke with `LOGAGENT_TOOL_PPROF_GO`.

### Upload And Workspace

- Server owns `upload_id`, `task_id`, and workspace location.
- Persists each upload as `storage.data_dir/uploads/<upload_id>.json` with atomic replacement.
- Restores completed and in-progress uploads after restart.
- Tracks `UPLOADING` / `COMPLETE`, expected size, received size, payload path, and timestamps.
- Flushes small-file and batch multipart payloads before persisting `COMPLETE` upload records.
- Enforces sequential chunk offsets and exact expected size at completion; incomplete uploads cannot create tasks.
- Reconciles an interrupted `UPLOADING` record from the payload file length on startup.
- Corrupt records, unsafe paths, missing payloads, and inconsistent completed sizes fail startup; orphan upload directories are only warned.
- Single task can now reference one upload or many uploads.
- Batch task workspace layout:

```text
workspaces/task_xxx/
  raw/
    upl_xxx/<filename>
  extracted/
    <package_name>/
  manifest.json
  grep_results.json
```

- Batch task manifest includes:
  - `uploadId` for backward compatibility
  - `uploadIds`
  - `uploads`
  - `files`

### Log Analyzer

- Implemented as Server internal Rust module.
- Supports:
  - `.log`
  - `.txt`
  - `.zip`
  - `.tar`
  - `.tar.gz`
  - `.tgz`
- `.tar.gz` / `.tgz` extraction falls back to plain `.tar` if gzip tar extraction fails.
- Archive extraction uses safe path joining to prevent workspace escape.
- Produces `manifest.json` and `grep_results.json`.

### Tool Runner

- Implemented as a Server internal Rust module.
- Reads `tools` whitelist configuration.
- Validates enabled tool paths are absolute.
- Supports `tools.<name>.path_env` for environment-provided tool paths; disabled tools do not read their env vars.
- Supports `tools.<name>.max_input_files` to bound automatic per-tool input selection; default is 1.
- Generates rule-based `run_tool` actions from manifest file patterns first, then grep keywords as fallback candidates.
- Uses stable action ids derived from tool name and input file hash, so one tool can process multiple files in the same task without result directory collisions.
- Executes configured tools through `tokio::process::Command` without shell string concatenation.
- Supports timeout, stdout/stderr capture, output truncation, non-zero exit recording, spawn failure recording, and idempotent result reuse.
- Parses JSON stdout into structured `summary` and `findings`; non-JSON stdout keeps the old fallback summary and does not fail the task.
- Adapts real `influxql-analyzer` Report stdout into LogAgent `summary` and structured findings, including `large_limit`, `no_time_filter`, `group_by_high_cardinality_risk`, `meta_query`, parse errors, realtime classification, and query fingerprint statistics.
- `ToolRunRecord` schema version 2 adds `findings[]` with optional `severity`, `file`, `line` and required `message`.
- Writes:

```text
tool_results/<action_id>/
  result.json
  stdout.txt
  stderr.txt
```

- `GET /api/tasks/:task_id/artifacts` returns `toolResults`.
- WebUI displays tool result status, exit code, duration, summary, structured findings, stdout path, and stderr path.
- Tool findings can be cited by final LLM results as `tool_results/<action_id>/result.json#findings/<index>`.
- Manual Tools runs now also write under `tool_results/<action_id>/`; `pprof_analyzer` results are exposed through `/api/tools/runs/:task_id/result` rather than the log-analysis artifact endpoint.
- Added `examples/server-tools.yaml` with `LOGAGENT_TOOL_FLUX_QUERY_ANALYZER` and `LOGAGENT_TOOL_INFLUXQL_ANALYZER` templates for real tool smoke tests.
- Added `examples/server-influxql-tool.yaml` for single-tool real InfluxQL smoke; it now uses `/usr/bin/influxql-analyzer` directly.
- Local Tool Runner smoke on port 50998 used `examples/server-tools.yaml` with both tool env vars pointed at `/bin/echo`; a batch `.flux` + `.sql` task `task_1780845768676_3` reached `SUCCEEDED` and returned OK tool results for both configured analyzers.
- Real InfluxQL Tool Runner smoke on port 50999 previously used a local analyzer path; current local setup exposes the analyzer as `/usr/bin/influxql-analyzer`, a symlink to `/home/duzhiwang/workspace/influxql/influxql-analyzer`. Analyzer docs and code live under `/home/duzhiwang/workspace/influxql`.
- The prior real InfluxQL smoke task `task_1780932701757_2` reached `SUCCEEDED` and artifacts contained `toolResults[0].findings` for `no_time_filter`, `large_limit`, `group_by_high_cardinality_risk`, `has_wildcard`, and `meta_query`.
- InfluxQL CompareReport stdout mapping now includes batch summaries, statement/QPS deltas, fingerprint count/QPS A->B deltas, rule count/QPS A->B deltas, rules, normalized query, and severity classification.
- Local search did not find `flux_query_analyzer` or `flux-query-analyzer`; real Flux smoke remains blocked until the binary is installed.

### WEBUI

- React + Vite + TypeScript + Tailwind CSS app under `webui/`.
- Uses shadcn/ui composition primitives.
- `npm run build` writes `webui/out`.
- Served by Server at `/` from `webui/out`.
- Supports:
  - health check
  - fixed top-bar API Key input
  - top-level Tools page
  - one or more file uploads
  - chunked upload for large files
  - task creation with `uploadIds`
  - artifact display
  - Server-backed recent task list and task detail polling
  - separate upload and task execution progress
  - persisted task recovery after page refresh
  - failed phase/message display and historical artifact selection
  - manual tool run status polling and result loading
  - Tool Runner result display
  - pprof analyzer upload, sample index/node count/SVG controls, top function table, and artifact path display
  - user question input and structured LLM result display
  - live Task execution loop summary from `/api/tasks/:task_id/analysis`
  - `WAITING_FOR_USER` prompt answer form
  - `WAITING_FOR_APPROVAL` action approval/rejection form
  - top-bar LLM debug switch backed by `/api/debug/llm`
  - successful task confirmation into Case Store with editable title/symptom/root cause/solution
  - Case Store schema v2 source display, keyword search, and disabling cases from the Log analysis view
  - top-level Cases page for Case Store search, manual entry, detail editing, evidence ref maintenance, and enable/disable state changes
  - grep evidence reference navigation
  - Metadata query
  - imported Metadata Instance list
  - stored Metadata snapshot loading by InstanceID
  - Metadata YAML/JSON import preview and confirmation
  - Metadata openGemini `/getdata` URL fetch preview with explicit InstanceID
  - Metadata Instance view for `PtView` partition state and `Databases` schema/RP/shard summary
  - Metadata Overview, Nodes, Partitions, Topology, Databases, Schemas, Diagnostics, and Raw JSON
  - complete Shard, IndexGroup, Index, and MstVersions logical/physical table views
  - topology follows DataNode -> Database/PT -> ShardGroup -> Shard -> IndexGroup -> Index
  - DataNode container lanes, topology filters, abnormal highlighting, missing-owner lanes, and entity detail panel

### Metadata

- Implemented as Server internal Rust module.
- Uses local JSON files under `storage.data_dir/metadata`.
- Supports:
  - instance lookup
  - cluster lookup
  - cluster node listing
  - JSON/YAML import preview
  - openGemini `/getdata` snapshot normalization
  - optional instance `remark` persistence and summary listing
  - openGemini `PtView` normalization into cluster `partitionViews`
  - openGemini `Databases` normalization into database/RP/measurement/shard summaries
  - server-side metadata URL fetch
  - import confirmation and persistence
- CSV import remains reserved but not implemented.
- Raw openGemini snapshots are preserved.
- Shard and Index owners are modeled as PT IDs and resolved through PtView to DataNodes.
- Task creation accepts optional instance, cluster, and node IDs, resolves related IDs, and rejects unknown or conflicting relationships.
- Persists an immutable normalized task snapshot as `metadata_context.json` without duplicating the raw Metadata snapshot.
- LLM prompts include bounded product, version, environment, node, database, and partition summaries.

### LLM Gateway

- Implemented as a Server-internal Rust module.
- Supports deterministic `stub` and OpenAI-compatible Chat Completions.
- Supports a reserved `binary` provider branch that invokes a configured absolute-path executable as `<binary_path> run <prompt>` through argv arrays, parses stdout with the same JSON/schema/evidence validation as the HTTP provider, and enforces timeout plus `binary_max_output_bytes`. Current verification uses a mock binary, so no real model executable is required in this environment.
- Supports `llm.model_env` for environment-provided model names while retaining static `llm.model` compatibility.
- Accepts pure JSON, whole-response JSON Markdown fences, and natural-language responses containing exactly one top-level JSON object.
- Builds a bounded prompt from question, manifest summary, and indexed grep matches.
- Adds bounded Tool Runner summary/findings to the prompt after grep evidence; stdout/stderr raw output is not sent.
- Validates result schema, confidence, and task-local grep evidence references.
- Validates task-local Tool Runner finding evidence references.
- Provides ActionDecision / FinalAnswer dual-mode schema and parser for the multi-round action loop.
- `PLAN_ANALYSIS` now calls the dual-mode action decision entrypoint until final answer, budget exhaustion, or repeated fingerprint termination.
- ActionDecision currently accepts `search_logs`, `run_tool`, `ask_user`, `collect_environment`, and `final_answer`; `collect_environment` requires approval and `collect_code_evidence` remains closed.
- If a real model returns a bare final-result JSON during `PLAN_ANALYSIS` without the outer `type` field, or returns common nested final-answer wrappers such as `final_answer.result.result`, `answer`, or `finalAnswer`, LLM Gateway wraps it as `final_answer` and still validates evidence refs.
- Action decision parsing/schema failures in `PLAN_ANALYSIS` now get one corrective retry with the latest schema error, so a first response missing top-level `type` no longer fails the task immediately.
- Normalizes traceable LLM evidence ref aliases, including raw log line ranges such as `12-14`, index ranges such as `#0-#7`, and `matches/<start>-<end>`, into canonical `grep_results.json#matches/<index>` refs.
- Normalizes real-model schema drift for string root causes with embedded evidence refs and single-string list fields.
- Retries final-result parsing/schema failures once with a corrective schema prompt and returns latest/previous parse errors if both attempts fail.
- Provider or schema failure in action decision moves the task to `FAILED / PLAN_ANALYSIS`; final result generation failures still move the task to `FAILED / GENERATE_RESULT`.

### Analysis Agent

- Analysis State Store MVP is implemented as a Server internal module.
- Current pipeline records analysis initialization, manifest evidence, grep evidence, Tool Runner action/evidence, model decision, final result, and failure events.
- Workspaces now include `analysis_state.json` and append-only `analysis_events.jsonl`.
- `GET /api/tasks/:task_id/analysis` returns the current state snapshot and event list.
- Multi-round Action Loop MVP is enabled through `PLAN_ANALYSIS` for `search_logs`, `run_tool`, and `final_answer`.
- User questions, approvals, token/runtime budgets, and richer state facts/hypotheses remain planned.

### Local startup

- Added `scripts/start-local.sh` for one-command local Server startup.
- Defaults to the real OpenAI-compatible configuration on port 50994 and supports `--stub` and `--foreground`.
- Builds the WebUI only when `webui/out/index.html` is missing, builds the Rust Server, writes PID/log files under `/tmp`, and waits for the health endpoint without exposing secrets.
- Shell syntax/help checks passed; a real-LLM foreground launch reached `http://127.0.0.1:50994/health` with a healthy response and an active listener.

### Documentation

- Root `README.md` and `SPEC.md` exist.
- Root `PROGRESS.md` records development status and must be updated after file changes.
- Every component has a `README.md` and `SPEC.md`.
- `AGENTS.md` records development conventions and current context.
- `AGENT.md` has been renamed to `AGENTS.md`.
- `metadata/` module has planning docs for instance, cluster, node, and template import management.
- Added independent `analysis-agent/` documentation for task-scoped context, multi-round investigation, user questions, action approvals, budgets, termination, and recovery.
- Repositioned `llm-agent/` as an LLM Gateway instead of the task orchestrator.
- Unified task planning around stable states plus execution phases:
  - `QUEUED`
  - `RUNNING`
  - `WAITING_FOR_USER`
  - `WAITING_FOR_APPROVAL`
  - `SUCCEEDED`
  - `FAILED`
- Defined Agent actions: `search_logs`, `run_tool`, `collect_code_evidence`, `collect_environment`, `ask_user`, and `final_answer`.
- Defined planned analysis APIs for state reads, user messages, and action approval decisions.
- Documented that safe read-only actions may run automatically while SSH/SCP collection requires approval by default.
- Documented that hidden chain-of-thought is not stored; only auditable rationale summaries and evidence references are persisted.
- Added Mermaid diagrams for the planned component architecture, execution trust boundary, Analysis Agent investigation loop, waiting states, and recovery path.

## Verified

Recent checks run successfully:

```bash
cargo fmt --check
cargo check
cargo test
npm run lint
npm run typecheck
npm run build
```

Task, upload, and LLM verification:

- 92 Rust tests pass.
- Upload Store tests cover persistence/reload, interrupted progress reconciliation, strict chunk offsets, completion size, and corrupt JSON.
- Upload API tests cover single and batch multipart upload flush-before-persist behavior.
- Task API rejects `UPLOADING` records until completion.
- Metadata context tests cover node/instance/cluster derivation, conflict rejection, workspace persistence, artifacts, prompt inclusion, and rerun preservation.
- Isolated HTTP smoke on port 50997 created a task with only `nodeId`, derived its instance/cluster IDs, reached `SUCCEEDED`, and returned the immutable Metadata artifact without `rawSnapshot`.
- Isolated HTTP restart smoke on port 50996 uploaded 6/12 bytes, restarted the Server, resumed from persisted offset 6, completed at 12 bytes, and created a task that reached `SUCCEEDED`.
- Task Store reload, corruption failure, reverse chronological listing, terminal-state protection, and interrupted task recovery.
- Executor recovery tests resume directly from `SEARCH_LOGS` and `GENERATE_RESULT`; Action/Evidence serialization and safe relative artifact paths are covered.
- Tool Runner, LLM, and Analysis State tests cover config validation, analysis budget defaults, `max_input_files`, rule-based multi-input selection, stable action ids, fake tool execution, JSON stdout summary/findings parsing, non-JSON fallback, timeout evidence, idempotent reuse, dispatcher `RUN_TOOL`, multi-round `PLAN_ANALYSIS`, repeated fingerprint termination, artifacts API `toolResults`, `/analysis` API, LLM prompt inclusion of tool findings, ActionDecision / FinalAnswer parsing, bare final-result JSON and nested final-answer wrapper normalization, and tool finding evidence ref validation.
- Case Store tests cover local JSON persistence, task final-result confirmation, keyword recall, duplicate task confirmation protection, and disabling cases from default recall.
- LLM Gateway tests cover Case context prompt inclusion and the task API test verifies recalled Case context appears in artifacts.
- Tool Runner tests cover enhanced InfluxQL CompareReport delta summary/findings.
- Pipeline rerun removes stale derived files and rebuilds evidence from raw snapshots.
- Task API covers `202`, list/detail, `404`, and artifacts `409`.
- Stub task execution reaches `SUCCEEDED`, writes result files, and serves the result API.
- Prompt truncation, Chat Completions parsing, Provider error classification, evidence refs, and evidence ref alias normalization are tested.
- Task API tests use per-process atomic temp roots so concurrent test cleanup cannot remove another task workspace.
- LLM model configuration tests cover static values, `model_env` precedence, and missing or empty environment values.
- Chat Completions parsing tests cover pure JSON, JSON code fences, natural-language wrappers around a single JSON object, and rejection of multiple JSON objects.
- LLM Gateway now normalizes real-model string root causes with embedded `evidenceRefs`, including `matches/<index>` and `matches/<start>-<end>` aliases, into canonical result objects.
- LLM Gateway now normalizes single-string list fields such as `missingInformation: "..."` into one-item arrays, matching the observed cluster metadata real-model response.
- LLM Gateway now retries final-result parsing/schema failures once with a corrective schema prompt and returns latest/previous parse errors when both attempts fail.
- LLM Gateway now normalizes observed real-model `PLAN_ANALYSIS` final-answer wrapper variants, including nested `result.result` and `action.decision.type=final_answer` with a result in `input`, into true `FinalAnswer` decisions while preserving strict final-result schema checks.
- LLM Gateway now applies the same bounded schema-correction pattern to action decisions that final-result generation already used; after two invalid action decision responses, the final error includes latest and previous parse reasons.
- LLM Gateway now supports a runtime debug switch for printing raw model response content to Server stderr when manually enabled.
- LLM Gateway now assigns `llmcall_*` callIds to real `PLAN_ANALYSIS` action-decision calls and propagates the callId into started/completed/schema retry events and final provider/schema errors.
- WebUI Task execution now shows live analysis loop revision, budget counters, recent events, LLM callId/attempt/schema retry details, model decisions, actions, artifacts, and evidence refs.
- WebUI top bar now includes an LLM debug switch that controls the Server-side response logging flag.
- WebUI Log analysis now shows a Case confirmation panel for successful tasks and a local Case search/disable panel.
- WebUI successful task artifacts now show task-local Case context captured at task creation.
- Added `scripts/smoke-product-loop.sh` for repeatable local product-loop smoke with real `/usr/bin/influxql-analyzer`, Case save, and Case recall.
- Upload API tests now use per-process atomic temp roots so concurrent cleanup cannot remove another upload payload.
- Real OpenAI-compatible smoke on port 50994 with clusterId `8343121086559132311` completed task `task_1780843631402_1` as `SUCCEEDED` after the LLM retry/error-detail change.
- LLM request failure is verified to persist `FAILED / GENERATE_RESULT`.
- Isolated HTTP smoke on port 50993 verified upload, `202 QUEUED`, polling to `SUCCEEDED`, persisted list/detail, `attempts=1`, and artifact reads.
- Isolated stub LLM HTTP smoke on port 50995 verified question persistence, `GENERATE_RESULT`, `result.json` / `result.md`, result API, and grep evidence references.
- Real OpenAI-compatible smoke on port 50994 reached the configured `deepseek-v4-flash` model and completed task `task_1780762062871_3` as `SUCCEEDED`.
- The successful real-model result persisted `result.json` / `result.md`, returned through the result API, and cited both task-local grep matches.
- Two preceding real-model attempts returned content that failed the strict result JSON parser, while an equivalent direct request and the third task returned valid JSON. This confirms the end-to-end protocol but leaves output-format stability, JSON response-format enforcement, and bounded schema retry as follow-up work.
- After evidence ref alias normalization, two real-model smoke tasks on port 50994 still failed earlier at strict JSON parsing (`LLM content is not valid result JSON`), so the exact `12-14` normalization path is covered by unit tests rather than real-model completion.

Recent HTTP smoke checks:

- `GET /health`
- `GET /`
- batch upload with `/api/uploads/batch`
- task creation with `uploadIds`
- artifact read with `/api/tasks/:task_id/artifacts`
- manifest paths include package-name prefixes for batch analysis
- metadata YAML import preview and confirm
- metadata instance and cluster query
- metadata `http://127.0.0.1:8091/getdata` parsing plan implemented for openGemini snapshot
- metadata server-side fetch from `http://127.0.0.1:8091/getdata`, confirm, cluster query, node query, and instance query
- metadata unit test covers openGemini `PtView` owner/status and `Databases` RP/schema/shard summary parsing
- live `127.0.0.1:8091/getdata` smoke test verified PT 0 -> DataNode 2, Shard/Index 1, and `testmst -> testmst_0000`
- isolated Tool Runner smoke on port 50998 configured `/bin/echo` as a fake tool, uploaded sample.log, completed a task as `SUCCEEDED`, and returned `toolResults[0].status=OK`

Current product-loop Case Store slice verification:

- `cargo fmt --check`
- `cargo check`
- `cargo test` (84 Rust tests pass)
- `npm run lint`
- `npm run typecheck`
- `npm run build`
- `scripts/smoke-product-loop.sh` passed with upload `upl_1780939967830_1`, task `task_1780939967836_2`, case `case_1780939968869_3`, and recall task `task_1780939968874_4`.

## 2026-06-11 Claude SDK Backend Runtime Switch

- Log Analysis `PLAN_ANALYSIS` no longer uses the self-built `internal_llm` agent loop. The executor now refreshes `analysis_package.json` and `agent_request.json`, invokes the configured `claude_agent_sdk` adapter, validates the returned `AgentDecision`, and writes a real `agent_response.json`.
- `agent_response.json` now records `runtimeStatus`, raw decision, `normalizedDecision`, usage/cost, duration, and errors. Adapter failures, non-JSON stdout, illegal actions, or invalid evidence refs fail the task without fallback.
- Config defaults now require `claude_agent_sdk`; missing `LOGAGENT_AGENT_CLAUDE_SDK_PATH` or a configured absolute `command_path` fails startup. `internal_llm` is no longer a supported agent backend type.
- Tool capability metadata is now included in `analysis_package.json` so Claude can request `run_tool` using Server-managed whitelist execution and canonical finding refs.
- WebUI now labels results as `Agent analysis` and shows a `Claude Code backend` panel with real backend status, usage/cost, duration, errors, and artifact paths instead of the old contract-only panel.
- Added mock Claude SDK adapter tests for final answers, `run_tool`, `ask_user`, invalid evidence refs, non-zero adapter exits, and executor multi-round search/budget behavior.
- Updated root, Server, WebUI, Agent Backends, Analysis Agent, LLM Gateway, Config, System Context, Interfaces, Roadmap, Tool Runner, and Testing docs to remove the old `internal_llm` default/runtime language.
- Verification for this slice passed: `cargo fmt --check`, `cargo check`, `cargo test` (native 1 + server 113 tests), `npm --prefix webui run lint`, `npm --prefix webui run typecheck`, and `npm --prefix webui run build`.

- Fixed direct Claude Code CLI execution for `claude_agent_sdk`: when `LOGAGENT_AGENT_CLAUDE_SDK_PATH` points to a `claude` / `claude.exe` binary, Server now invokes `claude --print --output-format json --json-schema ... --tools "" --no-session-persistence <prompt>` instead of the custom adapter-only `run --request ...` protocol. Non-`claude` command paths continue to use the custom LogAgent adapter protocol.
- Agent response parsing now accepts Claude CLI JSON envelopes by parsing the `result` field, records `total_cost_usd` as `cost.usd`, and still rejects failed CLI envelopes, invalid decisions, and invalid evidence refs without fallback.
- Added a mock direct `claude` CLI unit test to ensure `--request` is not passed to the native CLI and Claude envelope output normalizes to `final_answer`.
- Updated deploy templates so `claude_agent_sdk` is the default backend and `.env.example` documents `LOGAGENT_AGENT_CLAUDE_SDK_PATH` as the absolute `claude` CLI path. The local runtime `/home/duzhiwang/workspace/data/prd_assistant/deploy/logagent.yaml` was also updated with an explicit `agent_backends` block.
- Updated `deploy/logagentctl.sh` to load runtime `.env` with auto-export semantics so background Server processes receive `LOGAGENT_AGENT_CLAUDE_SDK_PATH` and other configured environment variables. The local runtime `logagentctl.sh` and `.env` were adjusted, the runtime server binary was rebuilt/installed, and `/health` returned `{"status":"ok"}` after a normal control-script start.
- Verification for the Claude CLI backend fix passed: `cargo fmt --check`, `cargo check`, `cargo test` (native 1 + server 114 tests), `bash -n deploy/logagentctl.sh deploy/rebuild-install.sh /home/duzhiwang/workspace/data/prd_assistant/deploy/logagentctl.sh`, and repeated runtime `/health` checks through `logagentctl.sh status`.

- Fixed Claude CLI structured output parsing after real `PLAN_ANALYSIS` returned a valid Claude envelope whose user-visible `result` was `"Done."` and whose actual schema-constrained decision was in `structured_output`. Agent Backend parsing now checks `structured_output` / `structuredOutput` before `result`, and failed parse responses include a truncated `rawStdoutPreview` plus the full error chain for future diagnosis.
- Verification for the structured output parser fix passed: `cargo fmt --check`, `cargo check`, `cargo test -p logagent-server services::agent_backend`, `cargo test` (native 1 + server 114 tests), runtime `rebuild-install.sh --server-only --no-restart`, and `logagentctl.sh status` returning `/health` `{"status":"ok"}`.

## 2026-06-12 Metadata On-Demand Loading

- `analysis_package.json` no longer embeds the full `metadataContext` payload. The Claude evidence package now writes `evidence.metadataContextOutline` with selected instance/cluster/node ids, product/version/environment, section counts, and `logagent.query_metadata` discovery info.
- The full `metadata_context.json` remains in the task workspace and continues to be returned by the successful task artifacts API for WebUI and compatibility.
- Task stdio MCP `resources/read metadata_context` and compatibility tool `logagent.get_metadata_topology` now return the outline instead of full metadata topology.
- Added task MCP tool `logagent.query_metadata` with `section`, `database`, `retentionPolicy`, `measurement`, `nodeId`, `ownerNodeId`, `ptId`, `shardId`, `indexId`, `limit`, and `cursor` arguments. It returns bounded `items`, `total`, `nextCursor`, `truncated`, `backgroundRef`, writes `metadata_slices/<stable_id>.json`, and records calls in `mcp_calls.jsonl`.
- Metadata slices remain background context and are not accepted as final evidence refs.
- Verification passed: `cargo fmt --check`, `cargo check`, `cargo test -p logagent-server metadata`, `cargo test -p logagent-server mcp`, and `cargo test -- --test-threads=1` (native 1 + server 128 tests).

## 2026-06-12 Claude MCP Permission Allowlist Fix

- Root cause for repeated "LogAgent MCP tools are being denied because permission mode is don't ask" prompts: the Server launched Claude Code in `dontAsk` mode with `--tools ""` and no `--allowedTools` entry for the task MCP server. LogAgent user approvals cannot change Claude CLI's tool allowlist, so answering the prompt did not unblock MCP calls.
- Every Claude Code permission profile now automatically includes `mcp__logagent__*` in `allowedTools`, while `diagnose` still keeps native built-in tools disabled through `--tools ""` and disallowed `Bash/Edit/Write/Read/Grep`.
- The Claude startup prompt now tells the model that LogAgent MCP read tools are pre-authorized and should be used directly; `request_approval` remains reserved for approval-gated actions such as remote environment collection.
- Verification passed: `cargo fmt --check`, `cargo check`, focused LogAgent Server config and Claude Code session tests, and `cargo test --quiet -- --test-threads=1` (native 1 + server 128 tests).

## 2026-06-12 Waiting Prompt Finalize Control

- Added a `resumeMode` field to task user messages. The default `continue` keeps the existing answer-and-resume behavior; `finalize` records that the user has no more information and wants a final result from current evidence.
- `analysis_package.json` now exposes `analysisState.finalizeRequested`, and the Claude startup prompt instructs Claude to return `completed` instead of asking the user again when that flag is true.
- WebUI `Task execution` now shows a secondary `没有更多信息，生成最终结果` button in `WAITING_FOR_USER`, sending the finalize mode with a bounded default message when the answer box is empty.
- Verification passed: `cargo fmt --check`, `cargo check --quiet`, focused `cargo test -p logagent-server task_message_resumes_waiting_for_user_task -- --test-threads=1`, `cargo test --quiet -- --test-threads=1` (native 1 + server 128 tests), `npm run lint`, `npm run typecheck`, and `npm run build`.

## Planned Next

1. Complete the current product loop around the existing upload, Metadata, Tool Runner, Analysis Agent, and WebUI flow:
   - stable task creation and polling
   - user question and approval interactions
   - evidence display and navigation
   - broader final result confirmation polish after the Case Store MVP
   - repeatable local smoke with `/usr/bin/influxql-analyzer`
2. Install/connect and smoke-test real `flux_query_analyzer`; run a real `influxql_analyzer` compare-mode smoke and tune delta mapping if needed.
3. Extend Case Store with embedding recall and a formal Analysis Agent evidence bundle after the product loop is stable.
4. Implement Code Evidence after the product loop is stable:
   - map product/version to branch/tag/ref
   - prepare read-only worktree/cache
   - collect code file/line evidence
5. Replace mock approved environment evidence with the real Environment Collector SSH/SCP executor after Code Evidence.

## Documentation Verification

For the Analysis Agent architecture update:

- Reviewed all component README/SPEC documents.
- Updated root architecture, interfaces, Server, WebUI, config, security, testing, deployment, evidence providers, Case Store, roadmap, and `AGENTS.md`.
- No application code or runtime configuration was changed, so Rust and WebUI build checks were not required.
- Added and syntax-reviewed the root Mermaid architecture and investigation-loop diagrams.

## Maintenance Rule

Every completed file change must update this progress document when it changes project status, behavior, APIs, module scope, verification, or next-step priorities.

When changing a component, also update that component's `README.md` and `SPEC.md`.
