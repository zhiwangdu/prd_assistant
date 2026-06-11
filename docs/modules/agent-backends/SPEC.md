# Agent Backends Spec

## 目标

定义 LogAgent 与成熟 agent 后端之间的稳定边界。LogAgent 专注证据采集、领域诊断和 WebUI；Codex、Claude Code、OpenCode 等后端负责推理与代码上下文分析。

## 当前状态

已实现：

- `agent_backends` 配置解析。
- 默认 `internal_llm` 后端。
- 外部后端类型枚举：`codex_cli`、`claude_code_cli`、`opencode_cli`。
- 外部后端启用时要求绝对 `command_path` 或 `command_path_env`。
- Settings API 返回不含命令路径的配置摘要。
- Settings dry-run 诊断：`internal_llm` 返回 ready，外部后端检查命令路径存在但不执行。

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
```

配置规则：

- `default_backend` 必须存在且启用。
- `internal_llm` 不需要命令路径。
- 启用的外部 CLI 后端必须解析出绝对路径。
- 禁用的外部 CLI 后端不读取环境变量。
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

诊断响应使用 `{ok,result,error}`。第一阶段外部 CLI 诊断只做路径检查，不执行命令。

## 后续契约

后续运行时接入需要固化：

```text
analysis_package.json
agent_request.json
agent_response.json
```

`agent_response.json` 必须映射到现有 action/final answer schema，未知 action、自由命令、任意路径和任意 SSH 目标必须拒绝。

## 验收标准

- 未配置 `agent_backends` 时默认提供 enabled `internal_llm`。
- `default_backend` 不存在或被禁用时启动失败。
- 启用外部 CLI 后端但路径缺失、为空或非绝对路径时启动失败。
- Settings 能列出 agent backends，且不暴露命令路径。
- dry-run 诊断能返回成功或完整异常文本。
- README 和 SPEC 在后端类型、配置或响应结构变更时同步更新。
