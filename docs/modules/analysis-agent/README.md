# Analysis Agent 方案

## 定位

Analysis Agent 已收敛为 Analysis Orchestrator。用户可见的 Log Analysis 历史入口是 Session；每次点击分析会从 Session 当前输入创建一个新的 task workspace 快照，Orchestrator 在该 task 内汇总证据、领域上下文、预算和历史事件，然后启动或恢复 Claude Code session。

新的职责边界：

- Analysis Orchestrator 管理状态、证据包、Claude MCP 配置、预算、恢复和终止条件。
- Claude Code Session Runner 调用 `claude` CLI，并按 `analysisMode` 应用 permission profile。
- LogAgent MCP server 暴露日志、Metadata、System Context、Tool Runner、Case recall、用户追问和审批能力。
- LLM Gateway 保留为 Case import、alias 和非 Agent Loop 辅助结构化任务能力。
- Domain Adapter 提供 openGemini/InfluxDB、Cassandra、RocksDB 等领域证据摘要。
- Server 是唯一领域动作执行者，负责权限、白名单、审批、持久化和模块调度。

MVP 保持单 Orchestrator、任务级上下文，不实现 Multi-Agent 或用户级长期记忆，也不复制成熟 agent 产品的通用能力。

## 调查循环

```text
用户问题 + 当前证据 + 历史事件
  -> Domain Adapter 生成领域摘要
  -> 写入 analysis_package.json / claude_prompt.md / claude_mcp_config.json
  -> Claude Code 通过 MCP resources/tools 获取证据
  -> MCP tools 写入新证据、等待 marker 和审计事件
  -> Claude Code 返回 completed / waiting outcome
  -> Server 校验最终 evidence refs 或进入等待态
```

初始日志提取仍作为可恢复 pipeline 前置步骤。Claude Code 后续可以通过 MCP tools 请求更精确的日志搜索、日志切片、工具分析、Case recall 或审批。

## 当前实现状态

已实现 Analysis State Store MVP，并启用 `PLAN_ANALYSIS` Claude Code session orchestration。用户追问和审批恢复 API 已启用；真实 SSH/SCP 环境采集执行器尚未接入。

`PLAN_ANALYSIS` 已切换为直接调用 Claude Code CLI。未配置、调用失败、返回非法 structured outcome 或返回非法 evidence ref 时任务失败，不自动 fallback。

当前 Server 会在现有固定 pipeline 中持久化：

- `analysis_state.json`
- `analysis_events.jsonl`
- `system_context.json`
- `analysis_package.json`
- `claude_prompt.md`
- `claude_mcp_config.json`
- `claude_session.json`
- `mcp_calls.jsonl`
- `agent_response.json`

已记录的事件和状态包括：

- analysis 初始化。
- manifest 证据。
- grep evidence。
- Tool Runner action 和 tool evidence。
- Claude Code session lifecycle。
- MCP waiting request、user prompt、user message、approval request 和 approval decision。
- final result。
- failure 事件。

`GET /api/tasks/:task_id/analysis` 可读取当前 state 和事件流。`GET /api/sessions/:session_id/timeline` 会把 Session events 和该 Session 下 task 的 analysis events 合并为统一 evidence timeline。真实 `influxql_analyzer` 已可通过 Tool Runner 产生结构化 evidence；`flux_query_analyzer` 尚未接入真实 smoke 时可继续使用配置中的 mock/stub 工具替代，保证 action/event/evidence 链路稳定。

当前 Claude Code runner 已接入 `PLAN_ANALYSIS`。`logagent.search_logs` 会写入稳定 `log_searches/logsearch_*.json` 并返回命中行正文、关键词计数和未命中关键词，不覆盖初始 `grep_results.json`；Claude prompt 要求检查 `matches[].text`，不能只按 `totalMatches` 推断异常类型或技术栈。`logagent.get_log_slice` 会写入日志切片，支持 `lineNumber` 加 `before/after` 的中心行形式，也支持 V1 兼容的 `startLine/endLine` range 形式。`logagent.run_domain_tool` 会走白名单 Tool Runner 通道，`logagent.query_metadata` 会写入分页 Metadata 背景 slice，`logagent.request_user_input` 会进入 `WAITING_FOR_USER`，`logagent.request_approval` 会进入 `WAITING_FOR_APPROVAL`，`completed` outcome 会直接持久化结果。用户在 `WAITING_FOR_USER` 可通过 `resumeMode=finalize` 表示没有更多补充信息；Orchestrator 会在下一轮 `analysis_package.json` 写入 `analysisState.finalizeRequested=true`，要求 Claude 基于当前证据直接完成。Orchestrator 会把当前证据刷新到 `analysis_package.json`，写入短启动 `claude_prompt.md` 和 `claude_mcp_config.json`，随后调用 Claude Code CLI；证据包由 Claude 通过 MCP `analysis_package` resource 读取，其中 Metadata 只包含 outline/counts。Runner 会写入 `claude_session.json`、`mcp_calls.jsonl` 和 `agent_response.json`。`agent_response.json` 包含 `runtimeStatus`、`promptDelivery`、`structuredOutput`、usage/cost、耗时和错误。Task `analysisLanguage` 会写入 `analysis_package.json` 并进入启动 prompt，约束 Claude 的 finalAnswer、追问和审批原因等自然语言字段使用 `zh-CN` 或 `en-US`；证据引用、JSON key、路径、工具名和产品名保持原值。

## 上下文产物

每个 task workspace 持久化：

```text
analysis_state.json
analysis_events.jsonl
system_context.json
result.json
result.md
```

`analysis_state.json` 至少包含：

- schema 版本和当前 revision
- 当前 task 状态与执行阶段
- 用户问题和已补充消息
- 已确认事实、候选假设和未解决信息缺口
- task 创建时固化的 System Context 背景资源引用
- task 创建时固化的 `analysisLanguage`
- 证据引用索引
- 待执行、待审批和待用户回答的请求
- 已完成动作的 fingerprint
- 轮数、模型调用数、动作数、token 和运行时间预算

`analysis_events.jsonl` 是仅追加的审计事件流，记录用户消息、模型决策摘要、动作执行结果、审批和状态变化。不得保存模型隐藏思维链；只保存简短、可审计的决策依据和证据引用。

## Claude / MCP 协议

Claude Code structured outcome 支持：

- `completed` + `finalAnswer`
- `waiting_for_user` + `pendingPrompt`
- `waiting_for_approval` + `pendingApproval`

Claude 通过 MCP tools 请求领域能力：

- `logagent.search_logs`
- `logagent.get_log_slice`
- `logagent.run_domain_tool`
- `logagent.recall_cases`
- `logagent.get_metadata_topology`（兼容 alias，返回 outline）
- `logagent.query_metadata`
- `logagent.request_user_input`
- `logagent.request_approval`

模型不能提供任意命令、任意文件路径、任意仓库 URL 或任意 SSH 地址。MCP tool input 必须由 Server 按对应 schema 校验并映射到配置白名单。

## 自动执行与审批

MVP 默认自动执行：

- task workspace 内的日志搜索和日志切片
- 白名单 Tool Runner 调用
- 已配置仓库和 ref 上的只读代码检索
- Case Store 只读召回

默认需要用户批准：

- SSH/SCP 环境采集
- 可能扩大远程采集范围的动作
- 配置明确标记为 `approval_required` 的动作

`logagent.request_user_input` 进入 `WAITING_FOR_USER`，问题必须说明所需信息、原因、是否必填和可接受格式。用户回答作为同一任务的新事件继续分析。

## 预算与终止

当前已实现配置：

- 最大分析轮数
- 最大 LLM 调用次数
- 最大动作数
- 同一动作 fingerprint 的最大重复次数

后续配置：

- 最大输入和输出 token
- 总运行时间
- 每轮最多追问数

等待用户或审批的时间不计入运行时间预算。达到预算、动作重复、用户拒绝或证据仍不足时，Agent 必须输出带不确定性、缺失信息和已尝试动作的最终结果，不能无限循环。

## 最终结果

`result.json` 至少包含：

- `summary`
- `symptoms`
- `confirmed_facts`
- `hypotheses`
- `likely_root_causes`
- `evidence`
- `missing_information`
- `actions_taken`
- `next_checks`
- `fix_suggestions`
- `confidence`
- `termination_reason`

所有结论必须引用任务内证据。历史 Case 只能作为参考，不能替代当前任务证据。System Context 同样只作为背景参考，用于帮助模型理解产品架构、Runbook、工具能力和通用约束。
