# Agent Backends Spec

## 目标

定义 LogAgent 与成熟 agent 后端之间的稳定边界。LogAgent 专注证据采集、领域诊断和 WebUI；Codex、Claude Code、OpenCode 等后端负责推理与代码上下文分析。

## 当前状态

已实现：

- `agent_backends` 配置解析。
- 默认 `internal_llm` 后端。
- 外部后端类型枚举：`claude_agent_sdk`、`codex_cli`、`claude_code_cli`、`opencode_cli`。
- 外部后端启用时要求绝对 `command_path` 或 `command_path_env`。
- Settings API 返回不含命令路径的配置摘要。
- Settings dry-run 诊断：`internal_llm` 返回 ready，外部后端检查命令路径存在但不执行。
- Log Analysis run 在 `PLAN_ANALYSIS` 前写出 `analysis_package.json`、`agent_request.json` 和 `agent_response.json`；当前 `agent_response.json` 为 `not_invoked` 占位，真实 Claude Agent SDK adapter 尚未接管主路径。

## 配置

```yaml
agent_backends:
  default_backend: "internal_llm"
  backends:
    internal_llm:
      type: "internal_llm"
      enabled: true
    codex_cli:
      type: "codex_cli"
      enabled: false
      command_path_env: "LOGAGENT_AGENT_CODEX_PATH"
      timeout_seconds: 120
      max_input_bytes: 262144
      max_output_bytes: 1048576
    claude_agent_sdk:
      type: "claude_agent_sdk"
      enabled: false
      command_path_env: "LOGAGENT_AGENT_CLAUDE_SDK_PATH"
```

配置规则：

- `default_backend` 必须存在且启用。
- `internal_llm` 不需要命令路径。
- 启用的外部后端必须解析出绝对路径。
- 禁用的外部后端不读取环境变量。
- 路径、超时和输入输出大小不能由用户消息覆盖。

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

诊断响应使用 `{ok,result,error}`。第一阶段外部 adapter 诊断只做路径检查，不执行命令。

## 契约产物

每次 Log Analysis run 在 task workspace 中刷新：

```text
analysis_package.json
agent_request.json
agent_response.json
```

`analysis_package.json` 包含任务、用户问题、manifest、grep、Metadata、System Context、Case、Tool results 和 analysis state 摘要。`agent_request.json` 声明默认后端、输入包、允许输出 schema 和 Server 执行策略。当前 `agent_response.json` 使用 `not_invoked` 占位；后续真实后端输出必须映射到现有 action/final answer schema，未知 action、自由命令、任意路径和任意 SSH 目标必须拒绝。

## 验收标准

- 未配置 `agent_backends` 时默认提供 enabled `internal_llm`。
- `default_backend` 不存在或被禁用时启动失败。
- 启用外部后端但路径缺失、为空或非绝对路径时启动失败。
- Settings 能列出 agent backends，且不暴露命令路径。
- dry-run 诊断能返回成功或完整异常文本。
- 成功日志分析任务的 artifacts API 能返回 `analysisPackage`、`agentRequest` 和 `agentResponse`。
- README 和 SPEC 在后端类型、配置或响应结构变更时同步更新。
