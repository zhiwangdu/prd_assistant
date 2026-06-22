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

## Next Steps

- ✅ WebUI navigation pivot to Tools-first（阶段 1 完成）。
- Split or hide Agent/Analyze-only UI paths behind optional workflow mode.（OperationsView 已从导航降级，视图待阶段 5 删除）
- Consolidate HTTP APIs around tools, runs, artifacts, metadata, fetch, executors, MCP and settings.
- Keep old session/task analysis code only as a migration source until replaced.
- Add a local-toolhub config example and deployment smoke.

## Verification

- `git diff --check`
- stale wording scan over owned docs; remaining hits are explicit non-goal,
  optional automation or migration-source wording
- `cd webui && npm run lint`
- `cd webui && npm run typecheck`
- `cd webui && npm run build`
- docs-only status review
