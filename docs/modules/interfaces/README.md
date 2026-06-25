# Interfaces

LocalToolHub 对外接口包括 WebUI HTTP API、MCP JSON-RPC、upload/import API 和 artifact download API。

## 原则

- WebUI 和 MCP 共享工具 registry。
- Artifact download 必须鉴权。
- 两模块能力统一通过 tool/run/artifact 语义呈现。
- MCP platform 工具 `logagent.runs.get/result` 只读 run 状态，不创建新的 run。
- 不暴露 Fetch、Metadata、Case、Skills、Executor 管理接口。
