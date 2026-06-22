# Optional Diagnostic Workflows

旧 Analysis Agent 降级为可选自动化 workflow。默认产品是工具工作台；workflow 只能编排已有工具，不能成为必需后端。

## 允许范围

- 一键运行一组工具。
- 根据模板生成检查清单。
- 汇总已有 artifacts。
- 调用可选 LLM 生成报告草稿。

## 禁止范围

- 自主执行未授权工具。
- 绕过 Tool Runner、Fetch、Executor allowlist。
- 把 Claude Code 或模型调用作为默认路径。
