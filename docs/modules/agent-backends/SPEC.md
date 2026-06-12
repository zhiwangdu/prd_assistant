# Claude Code Session Runner Spec

## 目标

定义 LogAgent 与 Claude Code 的稳定边界。LogAgent 是领域诊断增强层和证据工作台；Claude Code 是通用推理、代码理解和受控 native tool 执行入口。

## 当前状态

已实现：

- `claude_code` 配置解析。
- `mcp` 配置解析。
- `analysisMode=diagnose|code_investigation|fix` task 字段。
- `logagent-server mcp` stdio 子命令。
- `PLAN_ANALYSIS` 生成 `analysis_package.json`、`claude_prompt.md` 和 `claude_mcp_config.json`。
- Claude Code runner 调用 `claude --print --output-format json --json-schema ... --mcp-config claude_mcp_config.json --strict-mcp-config`，通过 stdin 传入短启动 prompt，并要求 Claude 通过 MCP `analysis_package` resource 读取完整证据包。
- `agent_response.json` 作为 Claude Code session response。
- `claude_session.json` 持久化 session id、mode、permission profile、prompt delivery 和 response artifact。
- `mcp_calls.jsonl` 记录 MCP resource/tool 调用。
- 等待用户和等待审批仍复用 `WAITING_FOR_USER` / `WAITING_FOR_APPROVAL` 状态。

## 配置

```yaml
claude_code:
  command_path_env: "LOGAGENT_CLAUDE_CODE_PATH"
  default_mode: "diagnose"
  max_session_seconds: 120
  max_output_bytes: 1048576

mcp:
  enabled: true
  transport: "stdio"
```

规则：

- `claude_code.command_path` 或 `command_path_env` 必须解析为绝对路径。
- `default_mode` 必须有 permission profile。
- `mcp.transport` 当前只支持 `stdio`。
- 旧 `agent_backends` 配置不再作为运行入口。

## Artifacts

```text
analysis_package.json
claude_prompt.md
claude_mcp_config.json
claude_session.json
mcp_calls.jsonl
agent_response.json
```

`agent_response.json` 字段：

- `runtimeStatus`
- `claudeSessionId`
- `analysisMode`
- `permissionProfile`
- `promptDelivery`
- `structuredOutput`
- `usage`
- `cost`
- `mcpCallsPath`
- `nativeToolPolicy`
- `durationMs`
- `error`
- `rawStdoutPreview`

## Structured Output

允许三类 outcome：

- `completed` + `finalAnswer`
- `waiting_for_user` + `pendingPrompt`
- `waiting_for_approval` + `pendingApproval`

最终答案 evidence refs 仍由 Server 校验。System Context 不能作为 final evidence。

## MCP

MCP resources/read 和 tools/call 只能访问当前 task workspace 内的安全 artifact。`analysis_package` resource 映射到 workspace 内的 `analysis_package.json`，用于承载可能过大的证据上下文。`get_log_slice` 只允许 `raw/` 或 `extracted/` 下的 workspace-relative path，禁止绝对路径和 `..`。

MCP tools：

- `logagent.search_logs`：重建 `grep_results.json`。
- `logagent.get_log_slice`：写入 `log_slices/<id>.json`。
- `logagent.run_domain_tool`：复用 Tool Runner 白名单。
- `logagent.recall_cases`：只返回 active/enabled Case。
- `logagent.get_metadata_topology`：读取 `metadata_context.json`。
- `logagent.request_user_input`：写入等待 marker，由 Executor 进入 `WAITING_FOR_USER`。
- `logagent.request_approval`：写入等待 marker，由 Executor 进入 `WAITING_FOR_APPROVAL`。

## 验收标准

- 未配置 Claude Code command path 时启动失败。
- `PLAN_ANALYSIS` artifact API 能返回 `analysisPackage`、`claudeMcpConfig`、`claudeSession`、`agentResponse` 和 `mcpCalls`。
- 大 `analysis_package.json` 不能进入 Claude CLI argv 或启动 stdin；CLI stdin 只包含短 prompt 和 MCP resource 读取指令。
- Claude Code 非零退出、超时、stdout 非 JSON、非法 structured output 或非法 evidence ref 会写入失败 `agent_response.json` 并使 task 失败。
- `request_user_input` / `request_approval` 能持久化等待状态并由现有恢复 API 继续任务。
