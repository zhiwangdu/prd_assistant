# Analysis Agent Spec

## 目标

提供可持久化、可恢复、可审计的单 Orchestrator 调查闭环，在受限预算内汇总证据、启动或恢复 Claude Code session、向用户追问并生成结构化结果。执行上下文是一次 Session run 对应的 task workspace；Session 负责保存草稿、上传引用和多次 run 历史。

## 当前状态

已实现 Analysis State Store MVP、`PLAN_ANALYSIS` Claude Code session orchestration、LogAgent MCP stdio server、用户追问和审批恢复 API。当前仍未接入真实 SSH/SCP 环境采集执行器。Claude Code runner 已提供配置、诊断接口和 session 输入/响应产物。

已落地：

- `analysis_state.json`
- `analysis_events.jsonl`
- `system_context.json`
- `GET /api/tasks/:task_id/analysis`
- `GET /api/sessions/:session_id/timeline` 聚合 Session events 和 task analysis events
- grep/tool/final result/failure 的基础事件记录
- Claude Code session lifecycle 事件记录
- 重启恢复到中间 phase 时，如果缺少 analysis state，会按当前 task 生成最小快照继续执行
- Claude structured outcome / FinalAnswer schema 和 parser
- Claude Code 配置摘要和 dry-run 诊断
- `analysis_package.json`、`claude_prompt.md`、`claude_mcp_config.json`、`claude_session.json`、`mcp_calls.jsonl` 和真实 `agent_response.json`
- Domain Adapter 内置 registry
- Claude MCP `search_logs`、`get_log_slice`、`run_domain_tool`、`recall_cases`、`get_metadata_topology`、`query_metadata`
- `request_user_input` 进入 `WAITING_FOR_USER`，用户回答后恢复同一任务
- `request_approval` 进入 `WAITING_FOR_APPROVAL`，批准或拒绝后恢复同一任务
- `run_tool` 可消费 Tool Runner 产生的真实 `influxql_analyzer` 结构化 evidence
- `analysis.max_rounds`、`analysis.max_llm_calls`

尚未实现：

- 真实 Environment Collector 执行器
- token、运行时间和每轮追问预算

## 输入

- `TaskContext`
- 当前 `EvidenceBundle`
- task 创建时固化的 `system_context.json` 背景资源
- `analysis_state.json`
- `analysis_events.jsonl`
- 用户新增消息或审批决定
- Claude Code structured outcome 和 MCP tool 调用

## 输出

- 更新后的 `analysis_state.json`
- 追加的 `analysis_events.jsonl`
- Claude session response artifact 或待 Server 处理的等待 marker
- 终态时的 `result.json` 和 `result.md`
- Claude Code session 输入/响应：`analysis_package.json` / `claude_prompt.md` / `claude_mcp_config.json` / `claude_session.json` / `mcp_calls.jsonl` / `agent_response.json`

当前 `/analysis` 响应包含：

- `statePath`
- `eventsPath`
- `state`
- `events`

## 状态模型

稳定状态：

```text
QUEUED
RUNNING
WAITING_FOR_USER
WAITING_FOR_APPROVAL
SUCCEEDED
FAILED
```

执行阶段独立记录，例如：

```text
UPLOAD
COLLECT
EXTRACT
SEARCH_LOGS
RUN_TOOL
COLLECT_CODE
PLAN_ANALYSIS
EXECUTE_ACTION
ANALYZE_RESULT
GENERATE_RESULT
```

`WAITING_FOR_USER` 和 `WAITING_FOR_APPROVAL` 可恢复到 `RUNNING`。执行阶段用于进度展示，不代替稳定状态。

## Claude / MCP Schema

Claude Code structured outcome：

```text
completed
waiting_for_user
waiting_for_approval
```

LogAgent MCP tools：

- `logagent.search_logs`
- `logagent.get_log_slice`
- `logagent.run_domain_tool`
- `logagent.recall_cases`
- `logagent.get_metadata_topology`（兼容 alias，返回 Metadata outline）
- `logagent.query_metadata`
- `logagent.request_user_input`
- `logagent.request_approval`

Server 必须在执行前验证 MCP tool 名称、输入 schema、白名单、预算和审批策略。LLM Gateway 和 Claude Code 不得绕过 Server 调用能力模块。
`analysis_package.json` 和任务 MCP 默认 `metadata_context` resource 只提供 Metadata outline；完整 `metadata_context.json` 保留在 workspace，必须通过 `logagent.query_metadata` 读取 bounded slice，slice 写入 `metadata_slices/<stable_id>.json` 并作为背景上下文处理。

## 用户追问

`request_user_input` 的问题项至少包含：

- `question_id`
- `question`
- `reason`
- `required`
- `answer_format`

用户通过任务 message API 回答。Server 追加事件、关闭对应请求并恢复同一任务，不创建独立分析任务。

## 一致性与恢复

- 状态更新使用 revision 或等价并发控制，防止重复恢复。
- MCP tool calls 使用 call id / artifact path 幂等；等待请求使用 action id 幂等。
- 事件流仅追加，state 可由事件和最新快照恢复。
- Server 重启后能识别运行中、等待中和未完成动作。
- 最终结果生成后禁止继续自动动作；用户显式重新分析应创建新的 analysis revision。

## 安全约束

- 不保存或展示隐藏思维链。
- 只记录简短决策依据、假设、事实和证据引用。
- Orchestrator、LLM Gateway 和 Claude Code 无领域工具、SSH 或任务外文件系统的直接执行权限；Claude native tools 仅按 `analysisMode` permission profile 开放。
- 远程采集默认需要用户批准。
- 用户消息和日志内容均视为不可信输入，不能改变系统白名单或执行策略。
- System Context 也视为背景参考输入，不能替代当前任务证据或改变 Server 侧 schema/白名单/审批策略。

## 验收标准

- 能用 mock Claude CLI 生成 completed / waiting structured outcome。
- MCP tool call 能写入 evidence artifact 和 `mcp_calls.jsonl`。
- 能进入 `WAITING_FOR_USER`，接收回答后恢复。
- 能进入 `WAITING_FOR_APPROVAL`，批准或拒绝后继续。
- 重启后可从持久化状态恢复。
- 最终结果包含证据引用、不确定性和终止原因。
