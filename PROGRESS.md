# Development Progress

Last updated: 2026-06-11

## Status Summary

LogAgent MVP has a working upload-to-bounded-multi-round-analysis loop and a documented path toward user questions, approvals, and richer evidence modules.

Current runnable loop:

```text
Chrome Extension, WEBUI upload, or WEBUI question-only Session
  -> optional Native Agent or Server upload API
  -> persisted QUEUED task and raw/text snapshot
  -> bounded background extraction / manifest
  -> simple grep evidence
  -> optional rule-based Tool Runner evidence
  -> analysis_state.json / analysis_events.jsonl audit snapshot
  -> PLAN_ANALYSIS bounded multi-round LLM action/final-answer loop
  -> optional action-driven search_logs or run_tool, with repeated fingerprint protection
  -> optional ask_user / approval wait and resume
  -> final answer or budget-limited low-confidence result
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

## Implemented

### Metadata Topology Explorer

- WebUI Metadata Topology no longer builds a full React Flow graph by default. It now derives a topology index and starts with an abnormal-first PT overview table.
- The overview groups by DataNode / Database / PT and shows ShardGroup, Shard, IndexGroup, Index, diagnostic count, owner and time range summaries.
- Focused Graph renders React Flow only after a user selects a specific PT, and it blocks graph rendering when the selected scope would exceed 600 graph elements.
- The right-side details panel now supports both aggregate PT details and individual graph entity fields/relations.
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

### System Context

- Added Server `SystemContextStore`, persisted under `storage.data_dir/system_context/resources`, for versioned Prompt Pack, Architecture Doc, Runbook, Glossary, Tool Capability and Knowledge Note resources.
- Added protected System Context API: list/create/get/patch resources, add/patch/activate versions, and prompt preview.
- Metadata is now included under System Context through a read-only `metadata_instance` adapter while existing `/api/metadata/*` APIs and topology models remain unchanged.
- `AnalysisSessionRecord` now persists `systemContextIds`; Log Analysis task creation resolves explicit Session selections, enabled matching resources and Metadata adapter summaries into `workspaces/<task_id>/system_context.json`.
- `TaskRecord` and artifacts responses now expose `systemContextPath` / `systemContext`; Session timeline records `system_context_recorded`.
- LLM Gateway now injects System Context as background reference for both final-result generation and action decisions; System Context cannot be used as final result evidence refs.
- WebUI now has a top-level `System Context` page with resource library, version management, Prompt preview, Architecture Mermaid source preview and embedded Metadata tab; Log Analysis Session draft can select System Context resources and displays the frozen snapshot after a run.
- Verification: `cargo fmt --check`, `cargo check`, `cargo test`, `cd webui && npm run lint`, `cd webui && npm run typecheck`, and `cd webui && npm run build` pass.

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
- Uses shadcn/ui composition primitives and React Flow.
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
