# Roadmap

## Phase 0: Product Architecture Baseline

- 以 `docs/product-architecture-and-flows.md` 作为目标产品架构和使用流程入口。
- 明确当前产品是本地开发者门户 + 受控 runbook/job runner + MCP tool server。
- 将 `docs/architecture_review.md` 只保留为 pre-pivot 历史快照。

## Phase 1: Documentation Pivot

- 产品定位切换为 Local Tool/MCP Workbench。
- 文档去掉默认 Agent 后端叙事。

## Phase 2: Server Slimming

- 保留 tools/runs/artifacts/metadata/fetch/executors/mcp/settings。
- 弱化 sessions/tasks/analysis-agent。

## Phase 3: WebUI Tools-first

- 首屏改为 Tools/Runs。
- Analyze 降级为可选 workflow。

## Phase 4: Packaging

- Rust binary + webui/out + bin/tools + data。
- 本地安装脚本和 smoke。

## Phase 5: Optional Automation

- 在工具平台稳定后再增加可选 LLM/report/workflow。
