# System Context

System Context 管理 prompt pack、runbook、glossary、tool capability 和 metadata adapter 等背景资源。它服务于用户和外部 MCP client，不是 Agent 必需上下文。

## 职责

- CRUD 背景资源。
- 版本管理。
- 预览组合结果。
- 向 MCP 暴露只读资源。
