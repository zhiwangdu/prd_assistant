# Development Progress

Last updated: 2026-06-08

## Status Summary

LogAgent MVP has a working upload-to-bounded-multi-round-analysis loop and a documented path toward user questions, approvals, and richer evidence modules.

Current runnable loop:

```text
Chrome Extension or WEBUI
  -> Native Agent or Server upload API
  -> persisted QUEUED task and raw snapshot
  -> bounded background extraction / manifest
  -> simple grep evidence
  -> optional rule-based Tool Runner evidence
  -> analysis_state.json / analysis_events.jsonl audit snapshot
  -> PLAN_ANALYSIS bounded multi-round LLM action/final-answer loop
  -> optional action-driven search_logs or run_tool, with repeated fingerprint protection
  -> final answer or budget-limited low-confidence result
  -> persisted result and WEBUI display
```

## Implemented

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
  - `GET /api/metadata/instances/:instance_id`
  - `GET /api/metadata/clusters/:cluster_id`
  - `GET /api/metadata/clusters/:cluster_id/nodes`
  - `POST /api/metadata/snapshots/fetch`
  - `POST /api/metadata/imports`
  - `POST /api/metadata/imports/fetch`
  - `GET /api/metadata/imports/:import_id/preview`
  - `POST /api/metadata/imports/:import_id/confirm`
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
- Persists `final_answer` decisions directly as `result.json` / `result.md`.
- Stops repeated action fingerprints and exhausted analysis budgets with a low-confidence final result instead of an infinite loop.
- Rejects artifact reads before success with `409` and the current task status.
- Runs one LLM result generation phase after grep and persists `result.json` / `result.md`.
- `GENERATE_RESULT` now reads `tool_results/*/result.json` and passes Tool Runner summary/findings into LLM Gateway as citeable evidence.
- Persists Analysis State Store MVP files, `analysis_state.json` and `analysis_events.jsonl`, and serves them through `GET /api/tasks/:task_id/analysis`.

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
- Added `examples/server-tools.yaml` with `LOGAGENT_TOOL_FLUX_QUERY_ANALYZER` and `LOGAGENT_TOOL_INFLUXQL_ANALYZER` templates for real tool smoke tests.
- Local Tool Runner smoke on port 50998 used `examples/server-tools.yaml` with both tool env vars pointed at `/bin/echo`; a batch `.flux` + `.sql` task `task_1780845768676_3` reached `SUCCEEDED` and returned OK tool results for both configured analyzers.

### WEBUI

- React + Vite + TypeScript + Tailwind CSS app under `webui/`.
- Uses shadcn/ui composition primitives and React Flow.
- `npm run build` writes `webui/out`.
- Served by Server at `/` from `webui/out`.
- Supports:
  - health check
  - fixed top-bar API Key input
  - one or more file uploads
  - chunked upload for large files
  - task creation with `uploadIds`
  - artifact display
  - Server-backed recent task list and task detail polling
  - separate upload and task execution progress
  - persisted task recovery after page refresh
  - failed phase/message display and historical artifact selection
  - Tool Runner result display
  - user question input and structured LLM result display
  - grep evidence reference navigation
  - Metadata query
  - Metadata YAML/JSON import preview and confirmation
  - Metadata openGemini `/getdata` URL fetch preview
  - Metadata cluster view for `PtView` partition state and `Databases` schema/RP/shard summary
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
- Supports `llm.model_env` for environment-provided model names while retaining static `llm.model` compatibility.
- Accepts pure JSON, whole-response JSON Markdown fences, and natural-language responses containing exactly one top-level JSON object.
- Builds a bounded prompt from question, manifest summary, and indexed grep matches.
- Adds bounded Tool Runner summary/findings to the prompt after grep evidence; stdout/stderr raw output is not sent.
- Validates result schema, confidence, and task-local grep evidence references.
- Validates task-local Tool Runner finding evidence references.
- Provides ActionDecision / FinalAnswer dual-mode schema and parser for the multi-round action loop.
- `PLAN_ANALYSIS` now calls the dual-mode action decision entrypoint until final answer, budget exhaustion, or repeated fingerprint termination.
- ActionDecision currently accepts `search_logs`, `run_tool`, and `final_answer`; unopened actions such as environment collection are rejected.
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

- 48 Rust tests pass.
- Upload Store tests cover persistence/reload, interrupted progress reconciliation, strict chunk offsets, completion size, and corrupt JSON.
- Upload API tests cover single and batch multipart upload flush-before-persist behavior.
- Task API rejects `UPLOADING` records until completion.
- Metadata context tests cover node/instance/cluster derivation, conflict rejection, workspace persistence, artifacts, prompt inclusion, and rerun preservation.
- Isolated HTTP smoke on port 50997 created a task with only `nodeId`, derived its instance/cluster IDs, reached `SUCCEEDED`, and returned the immutable Metadata artifact without `rawSnapshot`.
- Isolated HTTP restart smoke on port 50996 uploaded 6/12 bytes, restarted the Server, resumed from persisted offset 6, completed at 12 bytes, and created a task that reached `SUCCEEDED`.
- Task Store reload, corruption failure, reverse chronological listing, terminal-state protection, and interrupted task recovery.
- Executor recovery tests resume directly from `SEARCH_LOGS` and `GENERATE_RESULT`; Action/Evidence serialization and safe relative artifact paths are covered.
- Tool Runner, LLM, and Analysis State tests cover config validation, analysis budget defaults, `max_input_files`, rule-based multi-input selection, stable action ids, fake tool execution, JSON stdout summary/findings parsing, non-JSON fallback, timeout evidence, idempotent reuse, dispatcher `RUN_TOOL`, multi-round `PLAN_ANALYSIS`, repeated fingerprint termination, artifacts API `toolResults`, `/analysis` API, LLM prompt inclusion of tool findings, ActionDecision / FinalAnswer parsing, bare final-result JSON and nested final-answer wrapper normalization, and tool finding evidence ref validation.
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

## Planned Next

1. Configure and smoke-test real compiled tools through Tool Runner:
   - `flux_query_analyzer`
   - `influxql_analyzer`
2. Implement Code Evidence:
   - map product/version to branch/tag/ref
   - prepare read-only worktree/cache
   - collect code file/line evidence
3. Implement Analysis Agent state/events and extend LLM Gateway to structured action/final-answer decisions.
4. Implement user questions, approvals, budgets, idempotency, and restart recovery.
5. Implement Environment Collector with SSH/SCP whitelists and approval.
6. Implement Case Store save and recall from manually confirmed final results.

## Documentation Verification

For the Analysis Agent architecture update:

- Reviewed all component README/SPEC documents.
- Updated root architecture, original `plan.md`, interfaces, Server, WebUI, config, security, testing, deployment, evidence providers, Case Store, roadmap, and `AGENTS.md`.
- No application code or runtime configuration was changed, so Rust and WebUI build checks were not required.
- Added and syntax-reviewed the root Mermaid architecture and investigation-loop diagrams.

## Maintenance Rule

Every completed file change must update this progress document when it changes project status, behavior, APIs, module scope, verification, or next-step priorities.

When changing a component, also update that component's `README.md` and `SPEC.md`.
