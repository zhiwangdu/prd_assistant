# Agent Backends Spec

## 目标

定义 LogAgent 与成熟 agent 后端之间的稳定边界。LogAgent 专注证据采集、领域诊断和 WebUI；Codex、Claude Code、OpenCode 等后端负责推理与代码上下文分析。

## 当前状态

已实现：

- `agent_backends` 配置解析。
- 默认 `claude_agent_sdk` 后端。
- 后端类型枚举：`claude_agent_sdk`、`codex_cli`、`claude_code_cli`、`opencode_cli`。
- 后端启用时要求绝对 `command_path` 或 `command_path_env`。
- Settings API 返回不含命令路径的配置摘要。
- Settings dry-run 诊断检查命令路径存在但不执行。
- Log Analysis run 在每轮 `PLAN_ANALYSIS` 前写出 `analysis_package.json` 和 `agent_request.json`，调用 `claude_agent_sdk` 后端后写出真实 `agent_response.json`。该后端可直接调用 Claude Code CLI `claude`，也可调用自定义 LogAgent adapter。

## 配置

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

配置规则：

- `default_backend` 必须存在且启用。
- 启用的后端必须解析出绝对路径。
- 禁用的后端不读取环境变量。
- 路径、超时和输入输出大小不能由用户消息覆盖。
- Log Analysis runtime 只执行 `claude_agent_sdk`；其它类型为后续预留，配置为默认后端会导致任务失败。

## API

```http
GET /api/settings/agent-backends
POST /api/settings/agent-backends/:backend_id/test
```

摘要响应包含：

- `defaultBackend`
- `backends[].id`
- `backends[].backendType`
- `backends[].enabled`
- `backends[].defaultBackend`
- `backends[].commandConfigured`
- `backends[].timeoutSeconds`
- `backends[].maxInputBytes`
- `backends[].maxOutputBytes`
- `backends[].executionMode`

诊断响应使用 `{ok,result,error}`。当前 adapter 诊断只做路径检查，不执行命令。

## 契约产物

每次 Log Analysis run 在 task workspace 中刷新：

```text
analysis_package.json
agent_request.json
agent_response.json
```

`analysis_package.json` 包含任务、用户问题、manifest、grep、Metadata、System Context、Case、Tool capability、Tool results 和 analysis state 摘要。`agent_request.json` 声明默认后端、输入包、允许输出 schema 和 Server 执行策略。`agent_response.json` 记录真实 Claude CLI 或 adapter 响应，包含 `runtimeStatus`、原始 decision、`normalizedDecision`、usage/cost、`durationMs` 和错误信息。

当命令文件名为 `claude` 或 `claude.exe` 时，`claude_agent_sdk` 使用 Claude Code CLI 直连协议：

```text
<command_path> --print --output-format json --json-schema <AgentDecision schema> --tools "" --no-session-persistence <prompt>
```

此时 `LOGAGENT_AGENT_CLAUDE_SDK_PATH` 应设置为 `which claude` 输出的绝对路径。Server 禁用 Claude 内置工具，并把 `analysis_package.json` / `agent_request.json` 嵌入 prompt；stdout 可是 Claude CLI 的 JSON envelope，Server 会解析其中的 `result` 为 `AgentDecision`。

当命令文件名不是 `claude` / `claude.exe` 时，`claude_agent_sdk` 使用自定义 adapter 协议：

```text
<command_path> run --request agent_request.json --package analysis_package.json
```

stdout 必须是纯 JSON，可直接是 `AgentDecision`，也可使用包含 `decision`、`normalizedDecision` 或 Claude CLI `result` 的 envelope。未知 action、自由命令、任意路径、任意 SSH 目标和非法 evidence ref 必须拒绝。

## 验收标准

- 未配置 `LOGAGENT_AGENT_CLAUDE_SDK_PATH` 且未显式配置 `claude_agent_sdk.command_path` 时 Server 启动失败。
- `default_backend` 不存在或被禁用时启动失败。
- 启用后端但路径缺失、为空或非绝对路径时启动失败。
- Settings 能列出 agent backends，且不暴露命令路径。
- dry-run 诊断能返回成功或完整异常文本。
- 成功日志分析任务的 artifacts API 能返回 `analysisPackage`、`agentRequest` 和 `agentResponse`。
- Claude CLI 或 adapter 非零退出、超时、stdout 非 JSON、非法 action 或非法 evidence ref 会写入失败 `agent_response.json` 并使任务失败，不自动 fallback。
- README 和 SPEC 在后端类型、配置或响应结构变更时同步更新。
