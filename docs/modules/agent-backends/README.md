# Agent Backends 方案

## 定位

Agent Backends 是 LogAgent 接入成熟 agent 产品的适配层。LogAgent 不再把自研通用 agent loop 作为核心差异化方向，而是把日志、元数据、工具结果、测试流水线、环境采集和 Case 召回整理成诊断证据包，再交给可插拔后端完成推理和代码上下文分析。

当前配置支持的后端类型：

- `claude_agent_sdk`：Log Analysis 唯一默认运行后端，可直接调用 Claude Code CLI `claude` 二进制，也可调用本地自定义 adapter 命令封装 Claude Agent SDK。
- `codex_cli`：预留 Codex CLI 适配。
- `claude_code_cli`：预留 Claude Code CLI 适配。
- `opencode_cli`：预留 OpenCode CLI 适配。

## 当前实现状态

已实现配置解析、只读摘要、Settings dry-run 诊断接口和 task workspace 后端输入/响应产物。启用后端时必须配置绝对命令路径或路径环境变量；诊断只检查命令路径存在，不实际调用 CLI 或 SDK adapter。

已实现接口：

```http
GET /api/settings/agent-backends
POST /api/settings/agent-backends/:backend_id/test
```

`claude_agent_sdk` 是默认启用后端。未配置命令路径、Claude CLI 或 adapter 调用失败、返回非法 action 或返回非法 evidence ref 时，Log Analysis task 失败，不自动 fallback。

## 配置示例

```yaml
agent_backends:
  default_backend: "claude_agent_sdk"
  backends:
    claude_agent_sdk:
      type: "claude_agent_sdk"
      enabled: true
      command_path_env: "LOGAGENT_AGENT_CLAUDE_SDK_PATH"
    codex_cli:
      type: "codex_cli"
      enabled: false
      command_path_env: "LOGAGENT_AGENT_CODEX_PATH"
      timeout_seconds: 120
      max_input_bytes: 262144
      max_output_bytes: 1048576
```

## 执行边界

- Server 仍然是唯一执行边界，负责任务状态、证据持久化、工具白名单、审批和幂等。
- 成熟 agent 后端不能直接修改 LogAgent 状态，不能绕过 Server 执行 shell、SSH、工具或文件访问。
- `claude_agent_sdk` 后端只能消费 Server 生成的 `analysis_package.json` / `agent_request.json`，并通过 stdout 返回结构化 action 或 final answer；Server 写入真实 `agent_response.json`。
- 首版不开放 Claude 内置 Bash/Read/Grep/Write/Edit。后续如需后端工具访问，只通过 LogAgent MCP/adapter 暴露受控只读能力；实际工具执行仍回到 Server Action Executor。
- 当命令文件名为 `claude` 或 `claude.exe` 时，Server 在 task workspace 中直接执行 Claude Code CLI：`<command_path> --print --output-format json --json-schema <AgentDecision schema> --tools "" --no-session-persistence <prompt>`。因此 `LOGAGENT_AGENT_CLAUDE_SDK_PATH` 通常应设置为 `which claude` 的绝对路径。
- 当命令文件名不是 `claude` / `claude.exe` 时，Server 保留自定义 adapter 协议：`<command_path> run --request agent_request.json --package analysis_package.json`。

## 后续计划

1. 把 Settings dry-run 诊断升级为真实版本探测和最小消息测试。
2. 增加 usage/cost/provider request id 的稳定审计字段。
3. 通过受控 MCP/adapter 逐步暴露只读证据能力。
