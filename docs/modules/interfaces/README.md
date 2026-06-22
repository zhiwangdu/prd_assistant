# Interfaces

LogAgent 对外接口包括 WebUI HTTP API、MCP JSON-RPC、Native Agent import API 和 artifact download API。

## 原则

- WebUI 和 MCP 共享工具 registry。
- Artifact download 必须鉴权。
- 新能力优先设计 tool/run/artifact 语义。
- 旧 task/session 语义只作迁移兼容。
