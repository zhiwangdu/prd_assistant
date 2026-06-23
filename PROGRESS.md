# Development Progress

Last updated: 2026-06-23

Historical main-branch progress was archived to
`docs/archive/PROGRESS-history-main-2026-06-22.md`.

## Current Branch

- Branch: `rewrite/local-toolhub-rust`
- Base: `origin/main`
- Product direction: LocalToolHub local Tool/MCP Workbench
- Runtime target: Rust single binary + WebUI static files + local tools dir + local data dir

## 2026-06-23 批量 InfluxQL 日志分析内置 tool + Skill

目标：把「上传日志 -> 解包/预处理 -> influxql analyzer 分析」做成一个可发现、可批量的一键工具，并配一个内置 Skill 作为 runbook。现状该流程隐式存在但埋在 `influxql_analyzer`（configured，默认 disabled，`max_input_files: 3`）里，无批量入口。

- 新增内置 tool `logagent.batch_influxql_analysis`（`server/src/services/tools.rs`）：
  - `descriptors()`/`get_descriptor()`/`validate_tool_run_request()`/`run_tool_task()` 四处接线；`batch_influxql_analysis_descriptor(config)` 的 `enabled`/`runnable` 跟随 `influxql_analyzer` 是否配置+启用（pprof 模式），未启用时 catalog 中灰显。
  - `run_batch_influxql_analysis_task`：`prepare_pipeline_run` + `extract_task` 解包预处理（复用 `log_analyzer` 已有的 influxql JSONL 物化），读 `Manifest.tool_inputs_path` 的 `ToolInputIndex` 筛出 `tool_ids` 含 `influxql_analyzer` 的输入，对每个输入用 `tool_runner.execute`（action.input=`{tool: influxql_analyzer, inputFile}`，复用 configured tool 的 path/args + `{input_file}` 替换）跑分析，聚合 `findings[]`。200 输入安全上限（超限只警告）。结果 `result.json`：`preprocessSummary`/`analyzedInputs`/`failedCount`/`findings[]`/`warnings[]`/`status`(OK/PARTIAL/FAILED)。`max_files: 100`，`accepted_suffixes: .tar.gz/.tgz/.tar`。
  - 无 WebUI/MCP 改动：tool 经 `descriptors()` 自动出现在 `/api/tools`、MCP `tools/list`、WebUI Tools「Analyzers」分组（tag `log`）。
- 新增 managed Skill `skills/influxql-batch-analysis/`（`SKILL.md` + `logagent.json` + `references/batch-result.md`）：流程 runbook + 结果 schema；`toolIds` 含 batch tool / `influxql_analyzer` / `preprocess_log_package`；`skills.roots: ["skills"]` 自动加载。
- 单测（`tools::tests`）：descriptor 在 influxql 缺失/禁用/启用下的 `enabled`/`runnable` 门控、`descriptors()`/`get_descriptor()` 列出该 tool、`validate_batch_influxql_params` 接受对象/拒绝非对象。（需要真实 binary 的端到端跑由 smoke/手测覆盖。）
- 文档：`SPEC.md` 工具示例列表加 `logagent.batch_influxql_analysis`；本条 PROGRESS。
- 验证：`cargo fmt --check`、`cargo test -p logagent-server`（94 通过，含 5 个新测试）。
- 端到端已跑通（Go 1.26.4 构建 `target/tools/influxql-analyzer`，临时配置 `influxql_analyzer.enabled: true`）：
  - `GET /api/tools` 列出 `logagent.batch_influxql_analysis`（`enabled`/`runnable`=true）；`GET /api/skills` 列出 `influxql-batch-analysis`（toolIds 含 batch tool）；MCP `tools/list` 含该 tool（共 7 个）。
  - 上传 2 个含 InfluxQL query 的 `_logs.tar.gz`（node1/node2）跑 batch tool → `status: OK`，`influxqlInputs: 2`、`nodes: 2`、`findings[]` 2 条，每条带 `nodeId`/`packageTimestamp` + analyzer 规则（large_limit/has_wildcard/meta_query/no_time_filter）；server 日志无 error/panic。
  - 上传无 query 的包 → tool `status: FAILED`、`influxqlInputs: 0`、warning 正确。
  - 发现并补全 Skill「Input expectations」：preprocessor 要求包名 `<pkgid>_<inst>_<node>_<YYYY>_<MM>_<DD>_<HH>_<MM>_<SS>_<micros>_logs.tar.gz`、tar 内日志须在 `var/chroot/gemini/log/{tsdb,stream}` 或 `home/Ruby/log` 下、query 行须为 JSON 对象（`query`/`sql`/`stmt`/`statement`）或 `query="..."`。

## 2026-06-23 WebUI Tools 目录页重设计（搜索/筛选/分组）

目标：Tools 页 catalog 列表信息杂乱、且工具增长到几十个后「依次排开」不可用。结合工具市场/命令面板的业界实践重做左侧 catalog 卡片，右侧 detail+run 面板不变。

- `ToolsView.tsx` `ToolPluginsView`：左侧 catalog 卡片改为可搜索、可筛选、按类别分组的紧凑列表。
  - 新增状态 `query` / `sourceFilter`(all|built_in|configured) / `runnableOnly`；用 `useMemo` 派生 `filtered`（按 displayName/toolId/description/tags 过滤）与 `groups`。
  - 派生功能类别 `categoryOf`（Analyzers/Metadata/Fetch/Sync/Other，由 tags+toolId+backend 推导，避开冗余 tag）；无搜索时按 `CATEGORY_ORDER` 分组带计数，空组隐藏；搜索时切扁平 `Results (N)`（按 displayName 排序）。
  - 紧凑 `ToolRow`：状态点（绿=enabled&runnable、琥珀=enabled 非 runnable、灰=disabled）+ 名称 + 来源标签；选中高亮。去掉列表里冗余的 `toolId · backend`、双 badge、描述、tags 行（这些已在右侧详情面板）。
  - 头部计数 `toolCount(shown,total)`；空状态 `noTools`/`noMatches`。左列 340px→380px。
  - 右侧 detail/run 面板、`runTool`/`refreshTools`/`refreshRuns`/`selectRun`/轮询 全部不变；`ToolDescriptor` 类型与 `/api/tools` 响应不变（纯前端，无 server 改动）。
- `i18n.ts` `toolsCopy`（中英）：新增 `searchPlaceholder`/`filterAll`/`filterBuiltIn`/`filterConfigured`/`runnableOnly`/`groupAnalyzers`/`groupMetadata`/`groupFetch`/`groupSync`/`groupOther`/`noMatches`/`resultsLabel(n)`/`toolCount(shown,total)`；删除随之 dead 的 `enabledBadge`。
- 文档：`webui/SPEC.md` `### Tools` 补搜索/筛选/分组要求；`webui/README.md` Tools bullet 更新；本条 PROGRESS。
- 验证：`npm run lint` / `typecheck` / `build` 全绿（bundle 325.53 KB）。

## 2026-06-23 WebUI 顶层导航改英文 + Runs 收纳为 Tools 子项

- 顶层导航标签页改为纯英文展示，不再随语言切换中英双语（页面内部文案仍随语言切换）。导航顺序调整为 `Tools → Skills → MCP → Metadata → Fetch → Executors → Cases → Settings`。
- Runs 不再作为独立顶层标签页，改为 Tools 的子项「Runs History」（缩进虚框小标签，点击仍渲染原 `RunsView`）。`App.tsx` 导航数据改为带 `children` 的 `NavItem[]`，用 `Fragment` 渲染父项 + 缩进子项；`navItems` 提为模块级常量（不再依赖 `copy`）。
- `i18n.ts`：删除 `appCopy` 中随之 dead 的 `navTools`/`navRuns`/`navMetadata`/`navFetch`/`navExecutors`/`navMcp`/`navCases`/`navSkills`/`navSettings` 与本就未使用的 `apiKeyRequired`。
- 同步更新 `webui/README.md`（导航顺序图 + 页面职责）、`webui/SPEC.md`（页面要求节重排为 Tools/Runs History/Skills/MCP/Metadata/Fetch/Executors/Cases/Settings，补 Cases，注明顶层英文-only 与 Runs 子项）。
- 验证：`npm run lint` / `npm run typecheck` / `npm run build` 全绿（bundle 322.26 KB）。

## 2026-06-23 清理所有 Rust warning（Wave C dead-code 清理）

目标：`cargo check --all-targets` 零 warning。

- **metadata.rs dead-code 清理（Wave C）**：删除 retired analysis-agent 的 metadata-context-outline 子系统（~850 行）：`MetadataSection` enum + impl、`MetadataSliceQuery`/`MetadataSliceResult`/`MetadataCounts` 结构、`metadata_context_outline`/`metadata_slice_query_from_value`/`query_metadata_context`、以及 `section_outline`/`metadata_counts`/`optional_*_filter`/`validate_metadata_query_filters`/`metadata_query_filters`/`metadata_items_for_section`/`metadata_*_items`/`*_matches`/`shard_ids_for_group`/`pt_owner_filters_match` 等全部 helper。保留 keeper metadata 端点（`get_metadata_field_types`/`get_metadata_tag_fields` 等）和它们依赖的 `measurement_name_matches`/`databases` 视图函数。误删的 `fetch_metadata_content`（async fn，被 import 预览使用）已恢复。同步删除 3 个只测试已删函数的 test（`metadata_outline_*`/`metadata_query_filters_*`/`metadata_query_rejects_*`）及仅被它们使用的 `metadata_context_fixture`。移除随之 unused 的 `serde_json::{json, Value}` import（文件改用 `serde_json::` 全限定）。
- **config.rs**：删除从未读取的 `AppConfig.config_path` 字段（及 11 处 test 构造赋值）和 `McpSettings.transport` 字段（值恒为 "stdio"，`resolve_mcp` 仍校验输入 transport；`rejects_unknown_mcp_transport` 测试不变）。
- **log_analyzer.rs**：`read_log_slice` 仅被一个 test 使用，改用 `#[cfg(test)]` 限定为 test-only（非测试构建不再编译，消除 "never used" warning）。
- **skill_registry.rs**：移除 unused import `SystemContextBundle`。
- **tool_runner.rs 测试**：`action()`/`Fixture::context()`/`EvidenceProvider` import 仅被 3 个 `#[cfg(unix)]` async 测试使用，加 `#[cfg(unix)]` 守卫，消除 Windows `--tests` 下的 dead-code warning。
- 验证：`cargo fmt --check`、`cargo check --all-targets`（零 code warning，唯一 warning 是环境级 `~/.cargo/config` deprecation）、`cargo test -p logagent-server`（89 通过，原 92 删 3 dead test）；Windows 交叉编译 `cargo check --tests --target x86_64-pc-windows-gnu` 同样零 code warning。

## 2026-06-23 WebUI 拆分 System Context 集合页

- 移除 WebUI 顶层 "系统上下文 / System Context" 集合标签页（`SystemContextView`，内部用 Tabs 聚合 Skills + Metadata，其中 Metadata 与已有顶层 Metadata 标签页重复）。
- 把 Skills 拆为独立顶层导航项：新增 `webui/src/SkillsView.tsx`（从 `SystemContextView` 提取 Skills 列表/详情/导入，去掉 Tabs 包装与 Metadata 子页）；`App.tsx` 导航 `system-context` → `skills`，渲染 `SkillsView`；`i18n.ts` `navSystemContext` → `navSkills`（zh "技能" / en "Skills"）。
- 删除 `webui/src/SystemContextView.tsx`。
- 导航收敛为 `Tools | Runs | Metadata | Fetch | Executors | MCP | Cases | Skills | Settings`。
- 后端 `system_context_store` / `/api/system-context/*` 资源 store 与本变更无关（`SystemContextView` 本就未调用该 API），保留不动。
- 文档同步：`webui/README.md`、`webui/SPEC.md`、根 `README.md`、根 `SPEC.md`、`CLAUDE.md`、`docs/modules/README.md`。
- 验证：`npm run lint` / `typecheck` / `build` 全绿（bundle 322.27 KB）。

## 2026-06-23 Server 跨平台 (Linux/Windows) 与全工具 catalog

目标：server 包括所有 tools，兼容 Windows 和 Linux 双平台。

- **非测试代码已跨平台**：审计确认 server/native-agent 非测试代码无未守护的 Unix-only API；`tokio::signal::ctrl_c`、`tokio::process::Command`、`std::env::temp_dir()` 均跨平台。`exports.rs::is_executable` 已有 `#[cfg(unix)]`/`#[cfg(not(unix))]` 双分支。
- **测试模块 Windows 可编译**：`http/tools.rs`、`http/executors.rs` 整个测试模块依赖 bash 假工具 + Unix 可执行权限，改为 `#[cfg(all(test, unix))]`；`services/tool_runner.rs` 把 `PermissionsExt` 从模块级 `use` 移入 `#[cfg(unix)] fn write_tool`，3 个 bash 异步测试加 `#[cfg(unix)]`，纯解析测试仍全平台运行。
- **ssh_binary 默认值跨平台**：`default_ssh_binary()` 改为 Linux `/usr/bin/ssh`、Windows `C:\Windows\System32\OpenSSH\ssh.exe`；`examples/logagent.yaml`、`examples/server-test.yaml`、`deploy/logagent.example.yaml` 移除硬编码 `/usr/bin/ssh`，改用平台默认 + 注释。
- **全工具 catalog**：`examples/logagent.yaml` 新增 `tools:` 段，声明 `pprof_analyzer` + 4 个 analyzer（`flux_query_analyzer` / `influxql_analyzer` / `opengemini_storage_analyzer` / `influxdb_storage_analyzer`），全部 `enabled: false` + `path_env`，使配置在两平台无需外部二进制即可加载，catalog 即包含全部 12 个工具（5 configured + 7 built-in）。
- **Windows 工具构建脚本**：新增 `scripts/build-tools.ps1`，对应 `build-tools.sh`，构建 Go/Rust analyzer 到 `bin/tools/*.exe`。
- 验证：`cargo fmt --check`、`cargo check`、`cargo test -p logagent-server`（92 通过）全绿；**Windows 交叉编译校验通过**——`cargo check --target x86_64-pc-windows-gnu -p logagent-server`（非测试）与 `cargo check --tests --target x86_64-pc-windows-gnu -p logagent-server`（测试）均 Finished（仅原有 dead-code 警告，无 `std::os::unix` 错误）；`logagent-native-agent` 同样通过 Windows 交叉编译。运行时校验：`examples/logagent.yaml` 加载成功，`/api/tools` 返回 12 个工具。

## 2026-06-23 LocalToolHub 命名与 MCP P1 修复

- 产品可见名称从 LogAgent Tool Workbench 收敛为 `LocalToolHub`；WebUI 标题、Settings/MCP 页面文案、MCP `serverInfo.name` 和根/组件文档已更新。
- 保留 `logagent-server` crate/binary、`LOGAGENT_*` 环境变量、`logagent.*` tool id 和 `logagent://` resource URI 作为兼容 namespace，避免打断已有配置和外部客户端。
- 修复 HTTP MCP 配置开关：`mcp.enabled=false` 时 `/api/mcp` 返回 JSON-RPC error；stdio `mcp-serve` 继续在启动时拒绝服务。
- WebUI `McpView` 和 `SettingsView` 从旧 `/api/mcp/readonly` 切换到 `/api/mcp`，页面展示真实 catalog MCP tools/resources。
- 新增 `mcp_server::tests::http_mcp_respects_disabled_config` 覆盖 HTTP MCP 禁用行为。

## 2026-06-23 WebUI 工具页 / MCP 页中英双语

- `i18n.ts` 新增 `toolsCopy` + `mcpCopy`（zh-CN / en-US），覆盖工具目录、工具详情、运行记录、运行状态、pprof 结果、MCP stdio/tools/resources 等全部可见文案。
- `ToolsView`（ToolPluginsView）和 `McpView` 接收 `language` prop，按语言切换文案；App.tsx 透传 `language`。FetchView 暂未国际化（独立页面，后续按需）。
- 验证：`npm run lint` / `typecheck` / `build` 全绿（bundle 318.89→322.66 KB）。

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
