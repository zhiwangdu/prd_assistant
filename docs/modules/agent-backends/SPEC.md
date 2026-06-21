# Agent Provider Runtime Spec

## 目标

定义 V2 Analysis Orchestrator 与可替换 Agent provider 的稳定契约。Provider 负责生成结构化分析 outcome；LogAgent Server 负责证据、工具执行、权限、预算、等待恢复、审计和最终结果校验。

## 支持范围

Provider 集合：

- `stub`：默认 provider，生成确定性低置信度结果，不访问外部模型。
- `openai_compatible`：调用 OpenAI-compatible `/chat/completions`。
- `binary`：调用管理员配置的本地可执行文件。
- `claude_code`：调用管理员配置的 Claude Code CLI，并通过 task MCP 读取证据和请求能力。

V2 必须在未设置 `LOGAGENT_V2_AGENT_PROVIDER` 时使用 `stub`。选择 `claude_code` 时才要求 Claude Code CLI 路径。

## 输入

每轮 provider call 的输入包含：

- run/task/session/workspace 摘要
- 用户问题、补充消息、等待恢复意图和预算
- bounded artifact index
- manifest、grep、tool、case、metadata outline、system context 和环境/代码背景摘要
- 可调用的 task MCP tools schema
- 目标结构化 outcome schema

输入写入 `agent_request.json`。敏感字段、API Key、Authorization header 和完整 provider headers 不得写入 artifact。

## 输出

Provider 输出规范化为以下 outcome：

- `completed`：包含 `finalAnswer`
- `waiting_for_user`：包含 `pendingPrompt`
- `waiting_for_approval`：包含 `pendingApproval`
- `failed`：包含稳定错误分类

输出写入 `agent_response.json`，至少包含：

- `provider`
- `runtimeStatus`
- `response` 或 `structuredOutput`
- `durationMs`
- `usage`（provider 可提供时）
- `cost`（provider 可提供时）
- `error.classification`
- `error.retryable`
- `validation`

最终答案仍必须经过 Server schema 和 evidence ref 校验后才能写入 `result.json` / `result.md`。

## Provider 规则

### stub

- 不访问网络和文件系统。
- 必须产生低置信度、可解释的最终结果。
- 用于开发、无模型部署和预算耗尽 fallback。

### openai_compatible

- 必须使用配置的 base URL、model、timeout 和 API Key。
- 请求 headers 和 API Key 不得进入 artifact。
- 响应审计字段必须保存到 `agent_response.json.response`：`providerRequestId`、`providerResponseId`、`responseModel`、`finishReason`、`usage`、`systemFingerprint` 和 allowlist response headers。
- HTTP 失败必须保留 status，并写入稳定分类：`authentication_failed`、`rate_limited`、`input_too_large`、`provider_timeout`、`provider_server_error`、`provider_client_error` 或 `transport_error`。

### binary

- `LOGAGENT_V2_AGENT_BINARY_PATH` 必须解析为绝对、常规且可执行文件。
- 固定 argv 为 `<binary_path> run <prompt>`。
- 不得拼接 shell，不得允许用户输入覆盖可执行路径或 argv。
- stdout 必须在大小上限内并包含可解析结构化 JSON。

### claude_code

- `LOGAGENT_V2_CLAUDE_CODE_PATH` 或兼容的 `LOGAGENT_CLAUDE_CODE_PATH` 必须解析为绝对、常规且可执行文件。
- CLI 只接收短启动 prompt；`analysis_package.json` 通过 task MCP resource 读取。
- 必须生成 `claude_prompt.md`、`claude_mcp_config.json`、`claude_session.json` 和 `mcp_calls.jsonl`。
- 恢复等待态时，如果上一轮响应提供 `sessionId`，下一轮必须传递 `--resume <session_id>`。
- permission profile 按 `analysisMode=diagnose|code_investigation|fix` 选择，并自动包含 `mcp__logagent__*`。
- Claude envelope 中的 usage/cost/session id 必须保存到审计产物。

## Artifact

所有 provider：

```text
analysis_package.json
agent_request.json
agent_response.json
analysis_state.json
analysis_events.jsonl
```

Claude Code provider 额外：

```text
claude_prompt.md
claude_mcp_config.json
claude_session.json
mcp_calls.jsonl
```

## 安全约束

- Provider 不能直接执行 Tool Runner、Fetch、Code Evidence、Remote Executor 或 Metadata 查询。
- Provider 不能读取任务 workspace 外任意路径；Claude native tools 仅在对应 permission profile 中开放。
- 用户消息、日志、System Context、Skill reference 和历史 Case 都视为不可信输入，不能改变 Server schema、白名单、预算或审批策略。
- 不保存模型隐藏思维链，只保存结构化 outcome、简短理由和证据引用。
- 失败必须可审计，不能静默 fallback 到另一个真实 provider。

## 验收标准

- 默认配置使用 `stub` 且无需 Claude Code。
- `LOGAGENT_V2_AGENT_PROVIDER` 只接受 `stub`、`openai_compatible`、`binary`、`claude_code`。
- OpenAI-compatible HTTP 失败写入稳定 `error.classification`、`retryable` 和 `httpStatus`。
- Binary provider 的配置错误、启动失败、非零退出、超时、输出过大和 JSON parse 错误均可区分。
- Claude Code provider 未配置 path 时只在选择该 provider 后失败。
- `agent_request.json` / `agent_response.json` 不泄露 API Key、Authorization header 或真实 binary path。
- `completed`、`waiting_for_user` 和 `waiting_for_approval` 均能通过 Orchestrator 恢复或终止。
- 最终答案非法 evidence ref 会被 Server 拒绝。
