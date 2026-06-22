# Development Progress

Last updated: 2026-06-22

Historical main-branch progress was archived to
`docs/archive/PROGRESS-history-main-2026-06-22.md`.

## Current Branch

- Branch: `rewrite/local-toolhub-rust`
- Base: `origin/main`
- Product direction: Local Tool/MCP Workbench
- Runtime target: Rust single binary + WebUI static files + local tools dir + local data dir

## 2026-06-22 Documentation Pivot

- Reframed LogAgent from a Claude Code-backed analysis workbench into a local tools and MCP workbench.
- Updated root README/SPEC and AGENTS instructions to make Tools, MCP, artifacts, Metadata, Fetch, Executors and local deployment the primary product surface.
- Rewrote Server docs to guide slimming the existing Rust server instead of restoring the old V1 analysis architecture.
- Rewrote WebUI docs to make Tools/Runs/Metadata/Fetch/Executors/MCP/Settings the target navigation.
- Rewrote deploy and testing docs around single-machine Rust runtime and deterministic tool/MCP testing.
- Rewrote all owned `docs/modules/*` README/SPEC files so Analysis Agent, LLM Gateway and Agent Backends are optional automation/client integration rather than core runtime dependencies.
- Updated Chrome Extension and Native Agent docs as optional file import bridges.

## 2026-06-22 WebUI Tools-first 导航（阶段 1）

- 重排 WebUI 导航为 Tools-first：`Tools | Runs | Metadata | Fetch | Executors | MCP | Cases | SystemContext | Settings`，默认进入 Tools。
- 移除 header 的 LLM debug 开关（`/api/debug/llm`）；LLM 面向后续随服务端 fat 代码删除。
- 接入已有孤儿视图 `ExecutorsView`、`metadata/MetadataDashboard`；`ToolsView` 收敛为只渲染 tool plugins。
- 新增最小视图：`RunsView`（消费 `/api/tools/runs`，轮询非终态 run）、`McpView`（stdio 配置示例 + `/api/mcp/readonly` 的 tools/resources 只读预览）；`FetchView` 提升为顶层 nav。
- 降级 `OperationsView`（Analyze）：从导航移除，视图文件保留待阶段 5 删除。
- `appCopy` 精简：移除 LLM 文案与 Analyze/Memory nav 文案，补齐 Tools-first nav 文案；`analysisCopy` 保留供 `OperationsView`。
- 偏差：保留 SystemContext 为第 9 个导航项（核心 keeper，视图已存在），webui/README 的 8 项建议扩展为 9 项。
- 验证：`npm run lint`、`npm run typecheck`、`npm run build` 通过；构建产物 380KB → 329KB（OperationsView 不再打包）。

## 2026-06-22 HTTP API 收敛（阶段 2）

- 新增 `GET /api/runs`、`GET /api/runs/:run_id`、`GET /api/runs/:run_id/result`、`GET /api/runs/:run_id/artifacts`（`http/runs.rs`），统一 run history，支持 `?kind=` 与 `?limit=`。
- 新增 `GET /api/artifacts/*artifact_id`（`http/artifacts.rs`）：按 `<runId>/<relativePath>` 逻辑路径下载，`safe_join` 拒绝穿越，未知 runId 返回 404。
- 新增 `POST /api/mcp` 作为 HTTP JSON-RPC 入口（复用 `mcp_readonly::readonly_mcp`），与 `/api/mcp/readonly` 并存。
- `/api/tools/runs*` 保留为兼容别名；旧 `/api/sessions*`、`/api/tasks*`、`/api/debug/llm` 仅作迁移兼容，不新增能力。
- 已知缺口：`/api/runs` 暂只聚合 `task_store`（tool/remote_command/log_analysis）；FetchStore 的 fetch run 仍走 `/api/fetch/runs`，后续再合并。
- 验证：`cargo fmt --all --check`、`cargo check`、`cargo test --all`（172 通过，+2 新增）全绿。

## 2026-06-22 服务端解耦 ToolRun 路径（阶段 3）

- 探勘确认：ToolRun（RunTool 阶段）与 RemoteCommandRun（ExecuteRemoteCommand 阶段）本就通过 `task_store` 完成、早返回，不走 analysis_state；二者与 fat 模块的实际运行时耦合只有两处。
- 3.1 `pipeline/executor.rs` 错误处理：捕获 `task_kind`，仅 `LogAnalysis` 调用 `analysis_state::record_failure`；ToolRun/RemoteCommandRun 失败只经 `task_store.fail` 记录错误。
- 3.2 `sync_session_status` 对非 `LogAnalysis` 任务直接返回，ToolRun/RemoteCommandRun 路径不再静态调用 `session_store`（`sync_task_status` 本就 no-op，现显式跳过，为阶段 5 删除 session_store 铺路）。
- keeper 模块（http/tools、services/tools、services/tool_runner、services/fetch、http/runs、http/artifacts）本就不 import analysis_state/llm_gateway/agent_backend/session_store，grep 确认 0 命中。
- LogAnalysis 分支仍使用 analysis_state/llm/agent_backend（待阶段 5 删除），本阶段未改动。
- 验证：`cargo fmt --all --check`、`cargo check`、`cargo test --all`（172 通过）全绿。

## 2026-06-22 MCP 重设计为独立 stdio server（阶段 4）

- 新增 `server/src/mcp_server.rs`：面向外部客户端的独立 MCP server（无 `task_id` 依赖）。
  - `run_stdio(config)` stdio 入口；`handle_request`/`handle_http` 统一 JSON-RPC handler（单对象或批量）。
  - `tools/list` = `services::tools::descriptors` 过滤 runnable（与 `/api/tools` 同一 catalog）。
  - `tools/call` 同步运行目录工具：`build_tool_run_task` → `tasks.create` → `start_attempt` → `run_tool_task` → `succeed_tool_run`，产出 ToolRun 记录（进入 `/api/runs` 历史）；失败经 `tasks.fail` 记录。
  - `resources/list`+`resources/read` = skills / metadata-instances(+snapshots) / cases-recent / runs-recent / tools-catalog，无 domain-adapters、无 task-workspace artifacts。
  - 移除 agent-loop 耦合：无 `log_mcp_call` / `waiting_marker_tool` / `request_user_input` / `request_approval` / `analysis_state`。
- 抽取 `services::tools::build_tool_run_task` 共享 helper（HTTP `create_tool_run` 与 MCP `tools/call` 复用任务构造）；`http/tools.rs::create_tool_run` 改用之。
- `main.rs` 新增 `Command::McpServe`（→ `mcp-serve`，无参数）调 `mcp_server::run_stdio`；保留旧 `mcp --task-id --mode`（agent_backend 用，阶段 5 删除）。
- HTTP：`POST /api/mcp` → `mcp_server::http_mcp`（full，可运行工具）；`POST /api/mcp/readonly` 保留（WebUI 只读预览）。WebUI `McpView` stdio 配置示例更新为 `mcp-serve`。
- 已知依赖：`mcp-serve` 经 `AppState::new` 仍需 `LOGAGENT_CLAUDE_CODE_PATH` + LLM env（fat 配置强制），阶段 5 删除 claude_code/llm 配置块后解除。
- 验证：`cargo fmt --all --check`、`cargo check`、`cargo test --all`（173 通过，+1 `mcp_server` 单测）；stdio smoke：`mcp-serve` 的 `initialize`/`tools/list`/`resources/list` 正常，`tools/list` 为 runnable catalog，logs 走 stderr；旧 `mcp --task-id` 不回归（executor 测试仍绿）。

## 2026-06-22 删除 fat 代码（阶段 5）

- **Wave 1（HTTP 分析面）**：删除 `http/sessions.rs`、`http/tasks.rs`、`http/debug.rs`、`http/settings.rs` 及其路由与 mod 声明；移除 `/api/sessions*`、`/api/tasks*`、`/api/debug/llm`、`/api/settings/{llm,agent-backends,domain-adapters}*`。`pprof` 测试中遗留的 `/api/tasks` 断言已移除。
- **Wave 2（执行路径 + fat 模块 + 数据模型，Level 2 purge）**：
  - 删除 fat 模块（~8.8k 行）：`services/{llm_gateway,agent_backend,agent_contracts,domain_adapters}`、`stores/{analysis_state,session_store}`、旧 `mcp.rs`（task-bound MCP，被 `mcp_server.rs` 取代）。
  - 精简 `pipeline/executor.rs`：只保留 ToolRun + RemoteCommandRun 单阶段执行（无 agent loop、无 analysis_state）；`pipeline/mod.rs` 保留 extract/search/prepare（`logagent.preprocess_log_package` 工具依赖），删 generate/persist/render LLM 辅助。
  - 精简 `domain/models.rs`：purge `TaskKind::LogAnalysis`、`TaskStatus::Waiting*`、LogAnalysis-only `TaskPhase` 变体、`AnalysisMode`、`AnalysisLanguage`、`AnalysisSession*` 类型、`AnalysisResult`/`RootCause`/`Confidence`、`TaskRecord.analysis_mode/language`、`CreateTaskRequest`、`TaskListResponse`、`TaskArtifactsResponse`；`default_task_kind`→`ToolRun`。保留 `SystemContextScope::LogAnalysis` 变体（on-disk 兼容，仅删 match arm）。
  - 精简 `support/config.rs`：删 `llm`/`claude_code`/`analysis`/`embedding` 配置块 + 结构 + resolver + 默认值；新增 `ServerSettings.max_input_chars`（keeper 文本上限，从 llm 配置迁入）。`examples/logagent.yaml` 同步。
  - `app.rs`：删 `sessions/llm/agent_backends/domain_adapters` 字段。`main.rs`：删 `Command::Mcp`+`McpArgs`（保留 `mcp-serve`）。
  - `http/cases.rs`：case import 改为 manual-first（无 LLM 抽取）；删 `confirm_task_case` + task→case helper + `/api/tasks/:task_id/case` 路由。`http/mcp_readonly.rs`：删 `list_domain_adapters` 工具/资源。
  - `write_json_atomic` 移到 `support/fs_utils`（fetch + huawei_package_sync 共享，原经 agent_contracts）。
  - 删 `task_store` 的 `succeed`/`advance_phase`/`wait_for_user`/`wait_for_approval`/`resume_waiting`（dead after LogAnalysis 删除）。
  - WebUI：删 `OperationsView.tsx`（孤儿）；`i18n.ts` 删 `analysisCopy`+5 helper；`SettingsView.tsx` 精简为 external-MCP/exports 卡片（LLM/agent-backends/domain-adapters 面板删除）。
- 验证：`cargo fmt --check`、`cargo check`、`cargo test -p logagent-server`（91 通过）；`npm run lint`/`typecheck`/`build` 全绿（bundle 329→318.89 KB）。Smoke：server 无 `LOGAGENT_CLAUDE_CODE_PATH`/LLM env 即可启动，`/health` ok、`/api/tools` 7 工具、`POST /api/mcp` tools/list 5、`mcp-serve` stdio 正常。
- 残留：`services/metadata.rs` 中 ~35 dead-code 警告（retired analysis-agent 的 metadata-context-outline 子系统，与 keeper metadata 端点交织），留作后续 focused 清理（Wave C）。`SystemContextScope::LogAnalysis` 变体保留（on-disk 兼容）。

## Next Steps

- ✅ WebUI navigation pivot to Tools-first（阶段 1 完成）。
- ✅ OperationsView/analysisCopy 删除（阶段 5 Wave 2 完成）。
- ✅ Consolidate HTTP APIs around tools, runs, artifacts, metadata, fetch, executors, MCP and settings.（阶段 2 完成；fetch run 合并待后续）
- 清理 `services/metadata.rs` 的 metadata-context-outline dead code（Wave C）。
- Add a local-toolhub config example and deployment smoke.

## Verification

- `git diff --check`
- stale wording scan over owned docs; remaining hits are explicit non-goal,
  optional automation or migration-source wording
- `cd webui && npm run lint`
- `cd webui && npm run typecheck`
- `cd webui && npm run build`
- docs-only status review
