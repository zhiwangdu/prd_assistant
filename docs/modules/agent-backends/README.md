# Claude Code Session Runner 方案

## 定位

LogAgent 不再维护自研通用 Agent 调查循环，也不再通过旧 adapter 协议接入后端。Log Analysis 的 `PLAN_ANALYSIS` 现在启动一次 Claude Code CLI session，并通过 LogAgent MCP server 暴露日志、Metadata、System Context、Tool Runner、Case 和审批能力。

职责边界：

- Claude Code 负责通用推理、代码上下文理解和按模式使用允许的 native tools。
- LogAgent 负责 evidence package、MCP resources/tools、工具白名单、等待态、审批、审计 artifact 和 Case 确认。
- MCP tools 的输出必须写入 task workspace artifact，并返回 canonical evidence refs。
- Case 保存仍由用户确认，Claude Code 和 MCP tool 都不能直接写入长期 Memory。

## 当前实现状态

已实现：

- `claude_code` 配置块，读取 `LOGAGENT_CLAUDE_CODE_PATH` 或显式 `command_path`。
- `mcp.enabled` / `mcp.transport=stdio` 配置。
- `logagent-server mcp --config <logagent.yaml> --task-id <task_id> --mode <diagnose|code_investigation|fix>` stdio 子命令。
- Claude Code runner 使用 `--print --output-format json --json-schema --mcp-config --strict-mcp-config`，通过 stdin 传入短启动 prompt，证据包由 Claude 通过 MCP `analysis_package` resource 读取。Task MCP resource 主 URI 为 `logagent://task/<run_id>/<resource>`，Python V2 保留 `logagent-v2://run/<run_id>/<resource>` alias。
- `analysis_package.json` 不再内联完整 Metadata；Claude 初始只看到 `metadataContextOutline`，任务 MCP `metadata_context` resource 和 `logagent.get_metadata_topology` 也返回 outline，细节通过 `logagent.query_metadata` 分页读取。
- 分模式 permission profile：默认 `diagnose` 禁用 native tools，`code_investigation` 允许 Read/Grep/受控 Bash，`fix` 预留 Edit/Write/Test 能力。所有 profile 都自动允许 `mcp__logagent__*`，否则 Claude Code 的 `dontAsk` 模式会拒绝任务 MCP tools。
- task 创建接受 `analysisMode`，默认来自 `claude_code.default_mode`。
- 新 workspace artifacts：
  - `analysis_package.json`
  - `claude_prompt.md`
  - `claude_mcp_config.json`
  - `claude_session.json`
  - `mcp_calls.jsonl`
  - `agent_response.json`，已重定义为 Claude Code session response。
- `agent_response.json` 记录 `runtimeStatus`、`claudeSessionId`、`analysisMode`、`permissionProfile`、`promptDelivery`、`structuredOutput`、usage/cost、MCP call path、native tool policy、duration、error 和 stdout preview。
- Settings API 继续使用 `/api/settings/agent-backends` 作为前端兼容入口，但返回的是单一 `claude_code` 后端摘要。
- Python V2 迁移路径已支持 `LOGAGENT_V2_AGENT_PROVIDER=claude_code`：V2 的 LangGraph runtime 在每个 provider round 生成同类 `claude_prompt.md` / `claude_mcp_config.json`，启动配置的 Claude Code CLI，通过 HTTP task MCP 读取 `analysis_package`，并把 `waiting_for_user` / `waiting_for_approval` structured output 转换为现有 task MCP 等待工具。
- Python V2 恢复等待任务时会从最新 `agent_response.json` 读取上一轮 `response.sessionId`，并在下一次 Claude Code CLI 调用追加 `--resume <session_id>`。
- Python V2 会把 Claude envelope 的 `usage` 和 `total_cost_usd` / `totalCostUsd` 保存到 `agent_response.json` 的 `response.usage` 和 `response.cost.usd`。
- Python V2 会在 Claude Code 响应后写入新的 `claude_session.json` runtime artifact，记录 `claudeSessionId`、`resumedSessionId`、usage/cost、prompt delivery 和对应 `agent_response` artifact id。
- Python V2 现在也按 Workspace `mode` 选择 Rust/V1 同名 permission profile：
  `diagnose` 禁用 native tools，`code_investigation` 允许 Read/Grep/Bash，
  `fix` 允许 Read/Grep/Bash/Edit/Write；每个 profile 都自动允许
  `mcp__logagent__*`。扁平 `LOGAGENT_V2_CLAUDE_CODE_*` 权限变量只作为
  `diagnose` profile 的兼容覆盖；多模式覆盖使用
  `LOGAGENT_V2_CLAUDE_CODE_PERMISSION_PROFILES_JSON`。
- Python V2 的 settings diagnostics 不启动真实 Claude session，只校验 `LOGAGENT_V2_CLAUDE_CODE_PATH` / `LOGAGENT_CLAUDE_CODE_PATH` 是否 absolute、regular、executable；真实调用结果仍由 `agent_request.json` / `agent_response.json` 审计。

## CLI 与 Agent SDK 取舍

当前生产路径继续使用 Claude Code CLI。CLI 已覆盖一次性 headless 运行、JSON schema 输出、MCP 配置、session resume 和 permission profile；Server 侧只需要把大 `analysis_package.json` 从启动 prompt 移到 MCP resource，即可避免 argv 和 stdin 大小问题。Claude Agent SDK 更适合作为后续 adapter PoC：当需要 SDK message streaming、SDK hooks、独立 session store 或长期服务化 agent runtime 时，再引入 TypeScript/Python sidecar，不作为本轮默认路径。

## 配置示例

```yaml
claude_code:
  command_path_env: "LOGAGENT_CLAUDE_CODE_PATH"
  default_mode: "diagnose"
  max_session_seconds: 600
  max_output_bytes: 1048576
  permission_profiles:
    diagnose:
      permission_mode: "dontAsk"
      tools: ""
      allowed_tools: ["mcp__logagent__*"]
      disallowed_tools: ["Bash", "Edit", "Write", "Read", "Grep"]
    code_investigation:
      permission_mode: "dontAsk"
      tools: "Read,Grep,Bash"
      allowed_tools: ["Read", "Grep", "Bash", "mcp__logagent__*"]
      disallowed_tools: ["Edit", "Write"]
    fix:
      permission_mode: "acceptEdits"
      tools: "Read,Grep,Bash,Edit,Write"
      allowed_tools: ["Read", "Grep", "Bash", "Edit", "Write", "mcp__logagent__*"]
      disallowed_tools: []

mcp:
  enabled: true
  transport: "stdio"
```

## Claude 输出

Claude Code 必须返回结构化 session outcome：

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

旧 JSON 动作循环不再由 `PLAN_ANALYSIS` 消费。日志搜索和工具运行应通过 LogAgent MCP tools 完成。

## MCP 能力

Resources：

- task summary / artifact index
- analysis package
- manifest / grep results
- metadata context outline
- system context
- case context
- tool results

Tools：

- `logagent.search_logs`
- `logagent.get_log_slice`
- `logagent.run_domain_tool`
- `logagent.recall_cases`
- `logagent.get_metadata_topology`（兼容 alias，返回 overview outline）
- `logagent.query_metadata`
- `logagent.request_user_input`
- `logagent.request_approval`

每个 MCP tool call 追加到 `mcp_calls.jsonl`。会产生证据的工具同时写入对应 workspace artifact。`logagent.query_metadata` 写入 `metadata_slices/<stable_id>.json`，该 slice 是背景上下文，不是最终 evidence ref。
