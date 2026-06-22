# External Agent Clients

本模块替代旧的 Claude Code Session Runner 叙事。LogAgent 不再默认把 Claude Code 当作 Server 后端；Claude Code、Codex、Cursor、OpenCode 等都只是外部 MCP client。

## 目标

- 提供 MCP endpoint、tools/resources 和配置示例。
- 说明外部 Agent 如何连接 LogAgent。
- 保证外部 Agent 不能绕过 Server 执行边界。

## 非目标

- Server 不启动或管理个人 Agent 进程。
- Server 不保存外部 Agent 的认证信息。
- Server 不依赖 Claude Code 才能运行工具。

## 开发约束

新增客户端适配只能是文档、MCP schema 或配置示例；不能把某个 Agent 变成默认后端依赖。
