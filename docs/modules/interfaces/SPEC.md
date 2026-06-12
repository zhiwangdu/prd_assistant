# Interfaces Spec

## 目标

定义 Server、Analysis Orchestrator、Claude Code Session Runner、LogAgent MCP、LLM Gateway、Domain Adapter 和证据模块之间的稳定契约。

## 当前状态

Server 已实现第一版 `TaskContext`、Action、Evidence 和 `EvidenceProvider` 契约，以及持久化 phase 驱动的 Executor dispatcher。Tool Runner 已实现第一个 Evidence Provider。Claude Code 配置摘要、dry-run 诊断、MCP/session 契约产物和 Domain Adapter 内置 registry 已实现。只读 HTTP MCP 已实现为独立受保护接口：`POST /api/mcp/readonly`。它面向个人本地 Claude Code 读取共享知识，与任务 stdio MCP 分离，不绑定 task，不读取 workspace，不执行 action。

## 公共产物

```text
manifest.json
grep_results.json
metadata_context.json
tool_results/*.json
code_evidence/*.json
environment_evidence/*.json
analysis_state.json
analysis_events.jsonl
result.json
result.md
analysis_package.json
claude_prompt.md
claude_mcp_config.json
claude_session.json
mcp_calls.jsonl
agent_response.json
domain_context.json
```

公共 JSON 必须包含 `schemaVersion`。证据引用使用 workspace 相对路径和稳定 selector，禁止把绝对敏感路径暴露给模型或 WebUI。

Log Analysis 公开历史入口是 Session。Session 保存草稿、upload 引用、task run 列表、active task 和 timeline；task workspace 仍是每次执行的不可变快照。

Task schema 现在包含 `taskKind` 和可选 `sessionId`：

- `log_analysis`：完整上传、解压、grep、Tool Runner、Analysis Orchestrator、Claude Code session result 流程。
- `log_analysis` 必须绑定 `sessionId`。
- `tool_run`：手动工具运行，复用上传、TaskStore、workspace 和 `RUN_TOOL` phase，不绑定 Session，成功后通过 `/api/tools/runs/:task_id/result` 暴露工具结果。

## 状态契约

- `QUEUED`：已持久化，尚未执行。
- `RUNNING`：正在执行基础处理或 Agent 轮次。
- `WAITING_FOR_USER`：存在未回答问题。
- `WAITING_FOR_APPROVAL`：存在待批准动作。
- `SUCCEEDED`：最终结果已持久化。
- `FAILED`：发生不可恢复的系统错误。

预算耗尽或证据不足通常生成低置信度结果并进入 `SUCCEEDED`，不是系统 `FAILED`。

## MCP / Outcome 契约

Claude Code structured outcome 支持：

- `completed`
- `waiting_for_user`
- `waiting_for_approval`

LogAgent MCP tools 支持：

- `logagent.search_logs`
- `logagent.get_log_slice`
- `logagent.run_domain_tool`
- `logagent.recall_cases`
- `logagent.get_metadata_topology`
- `logagent.request_user_input`
- `logagent.request_approval`

只读 HTTP MCP resources 支持：

- `logagent://skills`
- `logagent://skills/{skill_id}`
- `logagent://metadata/instances`
- `logagent://metadata/instances/{instance_id}/snapshot`
- `logagent://cases/recent`
- `logagent://tools/catalog`
- `logagent://domain-adapters`

只读 HTTP MCP tools 支持：

- `logagent.search_cases`
- `logagent.get_case`
- `logagent.list_skills`
- `logagent.get_skill`
- `logagent.get_skill_reference`
- `logagent.preview_system_context`
- `logagent.list_metadata_instances`
- `logagent.get_metadata_snapshot`
- `logagent.list_tools`
- `logagent.list_domain_adapters`

第一版 Rust/JSON/MCP 契约要求：

- MCP tool name 使用 `logagent.*` 前缀。
- risk 使用稳定大写枚举。
- Evidence artifact 使用 workspace 相对路径，拒绝绝对路径和 `..`。
- Provider 返回的 artifact 在持久化前必须通过路径校验。

## Claude Code Session 契约

已暴露 Settings 摘要、dry-run 诊断和 task workspace session 输入/响应产物。当前 `agent_response.json` 由 Claude Code runner 调用后写入，运行时必须遵守：

- Server 生成 `analysis_package.json`、短启动 `claude_prompt.md` 和 `claude_mcp_config.json`。
- Claude CLI argv/stdin 不能承载完整 `analysis_package.json`；Claude Code 通过 MCP resources/tools 获取证据和请求领域能力。
- `agent_response.json` 只能表达 completed / waiting outcome。
- Server 继续负责 MCP tool schema、白名单、预算、幂等、审批和 final evidence ref 校验。

## Rust 接口

优先定义：

- `AnalysisAgent`
- `ClaudeSessionRunner`
- `DomainAdapter`
- `LlmGateway`
- `LogAnalyzer`
- `ToolRunner`
- `CodeEvidenceProvider`
- `EnvironmentCollector`
- `MetadataStore`
- `CaseStore`
- `AnalysisStateStore`

## 验收标准

- MCP tool 无法绕过 Server 直接执行。
- 只读 HTTP MCP 无法创建/读取/恢复 Session，无法上传文件，无法读取 task workspace，无法运行 Tool Runner，无法修改 Case/Metadata/Skills/System Context。
- Claude Code 无法绕过 Server 直接执行领域能力。
- Domain Adapter 只能推荐证据组织和工具能力，不能放宽白名单或审批策略。
- 等待状态可通过 message 或 decision 恢复。
- 重复 action 不产生重复副作用。
- 公共 JSON schema 可版本化。
- `RUNNING` 任务重启后保留 phase，并从该 phase 幂等恢复。
- phase 推进带 expected phase 校验，陈旧执行器不能覆盖状态。
- `tool_run` 任务不能混入 `/api/tasks` 日志分析列表，必须通过 `/api/tools/runs` 查询。
- Log Analysis 历史必须以 `/api/sessions` 为主入口；每次重新分析创建新的 task run。
- README 和 SPEC 在接口、状态或 action 变更时同步更新。
