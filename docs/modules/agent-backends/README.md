# Agent Provider Runtime

## 定位

Agent Provider Runtime 定义 V2 Analysis Orchestrator 与模型/本地代理后端之间的边界。V2 不再把 Claude Code 作为固定后端；实际后端由 `LOGAGENT_V2_AGENT_PROVIDER` 选择，默认是 `stub`。

支持的 provider：

| Provider | 用途 | 关键配置 |
|----------|------|----------|
| `stub` | 本地开发、无模型环境、确定性低置信度结果 | 默认值，无额外依赖 |
| `openai_compatible` | OpenAI-compatible Chat Completions 服务 | `LOGAGENT_V2_LLM_BASE_URL`、`LOGAGENT_V2_LLM_API_KEY`、`LOGAGENT_V2_LLM_MODEL` |
| `binary` | 本机可执行 Agent provider PoC | `LOGAGENT_V2_AGENT_BINARY_PATH` |
| `claude_code` | 可选 Claude Code CLI + task MCP provider | `LOGAGENT_V2_CLAUDE_CODE_PATH` 或兼容的 `LOGAGENT_CLAUDE_CODE_PATH` |

职责边界：

- Analysis Orchestrator 负责 run 状态、预算、证据包、工具调用、等待态和最终 evidence ref 校验。
- Agent provider 只返回结构化 outcome：完成、追问用户、请求审批或失败。
- Tool Runner、Fetch、Code Evidence、Remote Executor、Metadata 和 Case recall 只能通过 Server/task MCP 受控执行。
- Claude Code provider 可按 `analysisMode` 使用 native tools，但领域能力仍必须走 LogAgent Server 边界。

## 当前实现

已实现：

- `LOGAGENT_V2_AGENT_PROVIDER=stub|openai_compatible|binary|claude_code`。
- 每轮 provider 调用写入 `agent_request.json` 和 `agent_response.json` 审计产物。
- `analysis_package.json` 作为 provider 背景包；Metadata 默认只暴露 outline/counts，细节通过 task MCP slice 读取。
- `analysisMode=diagnose|code_investigation|fix`，用于 Claude Code permission profile 和后续模式扩展。
- `WAITING_FOR_USER` / `WAITING_FOR_APPROVAL` 恢复路径；等待恢复后继续同一 run。
- OpenAI-compatible provider 保存 request id、response id、model、finish reason、usage、system fingerprint 和 allowlist response headers，不保存 API Key。
- Binary provider 固定调用 `<binary_path> run <prompt>`，不拼接 shell，不允许用户覆盖 path 或 argv。
- Claude Code provider 生成 `claude_prompt.md`、`claude_mcp_config.json`、`claude_session.json`，通过 HTTP task MCP 读取证据包，并在恢复等待态时使用上一轮 `sessionId`。
- Provider 失败统一写入 `error.classification` 和 `error.retryable`，覆盖配置、鉴权、限流、输入过大、超时、transport、进程退出、输出过大、decode 和 parse 阶段。
- Settings 诊断接口复用 `/api/v2/settings/agent-backends`，展示当前 provider 摘要；诊断不启动真实 Claude Code session。

## Claude Code Provider

`claude_code` 是可选 provider，不是 V2 默认依赖。启用后：

- CLI path 必须是绝对、常规且可执行文件。
- Server 使用 `--print --output-format json --json-schema --mcp-config --strict-mcp-config`，stdin 只放短启动 prompt。
- 大证据包不进入 argv/stdin；Claude 通过 task MCP `analysis_package` resource 读取。
- 所有 permission profile 自动允许 `mcp__logagent__*`，否则 `dontAsk` 模式会拒绝任务 MCP tools。
- `diagnose` 默认禁用 native tools；`code_investigation` 允许 Read/Grep/Bash；`fix` 预留 Edit/Write/Test 能力。

## Provider Outcome

Provider 必须返回结构化对象，最终被规范化为：

```json
{
  "runtimeStatus": "completed",
  "finalAnswer": {
    "summary": "...",
    "symptoms": [],
    "likelyRootCauses": [],
    "nextChecks": [],
    "fixSuggestions": [],
    "missingInformation": [],
    "confidence": "low"
  }
}
```

也可以返回：

- `runtimeStatus=waiting_for_user` + `pendingPrompt`
- `runtimeStatus=waiting_for_approval` + `pendingApproval`

最终答案中的 evidence refs 由 Server 校验。`system_context.json`、Diagnostic Skill reference 和 Metadata slice 只能作为背景，不能作为根因证据。

## 相关产物

通用产物：

- `analysis_package.json`
- `agent_request.json`
- `agent_response.json`
- `analysis_state.json`
- `analysis_events.jsonl`

Claude Code provider 额外产物：

- `claude_prompt.md`
- `claude_mcp_config.json`
- `claude_session.json`
- `mcp_calls.jsonl`

## 验证入口

- `GET /api/v2/settings/agent-backends`
- `POST /api/v2/settings/agent-backends/:backend_id/test`
- `GET /api/v2/tasks/:task_id/analysis`
- `GET /api/v2/runs/:run_id/analysis`

本地常用：

```bash
scripts/v2-local.sh status
scripts/v2-local.sh logs
```
