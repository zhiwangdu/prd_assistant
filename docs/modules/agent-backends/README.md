# Agent Backends 方案

## 定位

Agent Backends 是 LogAgent 接入成熟 agent 产品的适配层。LogAgent 不再把自研通用 agent loop 作为核心差异化方向，而是把日志、元数据、工具结果、测试流水线、环境采集和 Case 召回整理成诊断证据包，再交给可插拔后端完成推理和代码上下文分析。

当前支持的后端类型：

- `internal_llm`：默认后端，复用现有 LLM Gateway 和结构化 action/result schema。
- `claude_agent_sdk`：首个目标 PoC 后端，通过本地 adapter 命令封装 Claude Agent SDK。
- `codex_cli`：预留 Codex CLI 适配。
- `claude_code_cli`：预留 Claude Code CLI 适配。
- `opencode_cli`：预留 OpenCode CLI 适配。

## 当前实现状态

第一阶段已实现配置解析、只读摘要、Settings dry-run 诊断接口和 task workspace 契约产物。外部后端启用时必须配置绝对命令路径或路径环境变量；诊断只检查命令路径存在，不实际调用 CLI 或 SDK adapter。

已实现接口：

```http
GET /api/settings/agent-backends
POST /api/settings/agent-backends/:backend_id/test
```

`internal_llm` 是默认启用后端，保证现有任务执行路径不变。

## 配置示例

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

## 执行边界

- Server 仍然是唯一执行边界，负责任务状态、证据持久化、工具白名单、审批和幂等。
- 成熟 agent 后端不能直接修改 LogAgent 状态，不能绕过 Server 执行 shell、SSH、工具或文件访问。
- 外部 agent 后续只能消费 Server 生成的 `analysis_package.json` / `agent_request.json`，返回结构化 `agent_response.json`。
- 当前 Log Analysis run 会写出三份契约产物，但 `agent_response.json` 标记为 `not_invoked`，主路径仍由 `internal_llm` 执行 `PLAN_ANALYSIS`。
- 第一阶段不运行真实 CLI，避免绑定具体产品的认证、交互协议和输出格式。

## 后续计划

1. 接入 Claude Agent SDK adapter，使其读取 `analysis_package.json` / `agent_request.json` 并写出实际 `agent_response.json`。
2. 将 adapter 输出映射为现有 action/final answer 契约。
3. 把 Settings dry-run 诊断升级为真实版本探测和最小消息测试。
