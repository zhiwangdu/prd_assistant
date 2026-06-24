# External Agent Clients Spec

## Contract

外部 Agent 通过 MCP 使用 LocalToolHub：

```text
MCP client -> LocalToolHub MCP -> shared tool/context services -> artifacts
```

## Requirements

- MCP tools/list 与 WebUI Tool Catalog 复用同一 descriptor/schema，但只暴露 enabled/runnable
  tools 和 platform tools。
- `mcp.enabled=false` 时 HTTP 和 stdio MCP 都不可用。
- tools/call 复用 Server allowlist、schema、timeout 和 artifact store。
- resources/read 只返回 bounded、脱敏内容。
- 不保存外部 Agent 隐藏思维链。
- 不要求安装 Claude Code 才能启动 Server。

## Acceptance

- 无 Claude/Codex 环境时 Server 和 WebUI 正常可用。
- MCP client 可列出工具和读取资源。
- 危险动作仍由 Server policy 拒绝或要求审批。
