# Analysis Orchestrator Spec

## 目标

提供可持久化、可恢复、可审计的单 Orchestrator 调查闭环。Orchestrator 在有限预算内汇总证据、调用当前 Agent provider、执行受控 task MCP tools、处理用户追问/审批，并生成可校验的结构化结果。

## 输入

- Session question、draft、messages 和 attached uploads。
- run/task context、workspace path 和 API identity。
- `manifest.json`、`grep_results.json`、tool results、code evidence、environment evidence。
- `metadata_context.json` outline、`system_context.json`、`case_context.json`。
- 用户新增 message、`resumeMode=finalize` 或 approval decision。
- 当前 `LOGAGENT_V2_AGENT_PROVIDER` 的 provider response。

## 输出

- `analysis_state.json`
- `analysis_events.jsonl`
- `analysis_package.json`
- `agent_request.json`
- `agent_response.json`
- 等待态 marker：`mcp_waiting_request.json`
- `result.json`
- `result.md`
- provider-specific artifacts，例如 Claude Code 的 `claude_prompt.md`、`claude_mcp_config.json`、`claude_session.json`、`mcp_calls.jsonl`

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

`WAITING_FOR_USER` 和 `WAITING_FOR_APPROVAL` 必须能恢复到同一 run。执行阶段用于进度和恢复，不替代稳定状态。

## Provider 交互

Orchestrator 必须从统一 provider runtime 调用当前 provider：

- `stub`
- `openai_compatible`
- `binary`
- `claude_code`

Provider outcome 只允许：

- `completed` + `finalAnswer`
- `waiting_for_user` + `pendingPrompt`
- `waiting_for_approval` + `pendingApproval`
- classified failure

Provider 不能直接执行领域工具、SSH/SCP、Fetch 或代码检索；必须通过 Server/task MCP 请求。

## Task MCP Tools

可广告的工具由当前配置和 run context 决定：

- `logagent.search_logs`
- `logagent.get_log_slice`
- `logagent.run_domain_tool`
- `logagent.recall_cases`
- `logagent.get_metadata_topology`
- `logagent.query_metadata`
- `logagent.get_metadata_field_types`
- `logagent.get_metadata_tag_fields`
- `logagent.get_skill_reference`
- `logagent.search_code`
- `logagent.diff_code`
- `logagent.fetch`
- `logagent.request_user_input`
- `logagent.request_approval`

Server 必须在执行前校验工具名、输入 schema、白名单、预算、run ownership 和审批策略。每次 tool call 必须写入审计事件；产生证据的工具必须写入 workspace artifact 并返回 canonical refs。

日志工具要求：

- 初始 `grep_results.json` 不被后续搜索覆盖。
- `logagent.search_logs` 每次写入 `log_searches/logsearch_*.json`，返回 `matches[].text`、`keywordCounts`、`unmatchedKeywords` 和 `log_searches/...#matches/<index>`。
- `logagent.get_log_slice` 只允许 workspace-relative `raw/` 或 `extracted/` 路径，写入稳定 `log_slices/slice_<digest>.json#lines`。

Metadata 和 Skill 要求：

- `analysis_package.json` 只包含 Metadata outline/counts。
- 完整 Metadata 通过 bounded slice 读取，写入 `metadata_slices/*.json`。
- Skill reference 写入 `skill_references/*.json`。
- Metadata slice 和 Skill reference 均为背景，不允许作为最终根因 evidence ref。

## 用户追问

`request_user_input` 至少包含：

- `question_id`
- `question`
- `reason`
- `required`
- `answer_format`

用户通过 run/task message API 回答。`resumeMode=finalize` 表示用户没有更多补充信息；下一轮 `analysis_package.json` 必须包含 `analysisState.finalizeRequested=true`，并要求 provider 基于当前证据直接完成。

## 审批

`request_approval` 可只传 `reason`，缺省 `actionType=manual_approval`。审批 action 必须属于当前 run/task。拒绝审批时 Orchestrator 应继续生成解释性结果或转入可恢复状态，不能默默执行动作。

远程采集、SCP 文件拉取和扩大目标范围的动作默认需要审批。多 executor/template 场景必须通过 `target` / `executor` / `node` / `host` 与 `template` / `command` / `file` hint 唯一匹配；无匹配或歧义时拒绝执行。

## 预算与终止

默认预算：

- `LOGAGENT_V2_AGENT_MAX_ROUNDS=4`
- `LOGAGENT_V2_AGENT_MAX_LLM_CALLS=4`
- `LOGAGENT_V2_AGENT_MAX_ACTIONS=6`
- `LOGAGENT_V2_AGENT_MAX_REPEATED_ACTION_FINGERPRINTS=1`
- `LOGAGENT_V2_AGENT_MAX_TOTAL_TOKENS=200000`
- `LOGAGENT_V2_AGENT_MAX_RUNTIME_SECONDS=300`
- `LOGAGENT_V2_AGENT_MAX_USER_PROMPTS=3`
- `LOGAGENT_V2_AGENT_MAX_APPROVALS=3`

预算耗尽、重复 tool fingerprint 或证据不足时，Orchestrator 应生成带 `budgetLimited=true`、`terminationReason` 和不确定性的低置信度结果，并进入 `SUCCEEDED`。不可恢复系统错误才进入 `FAILED`。

## 安全约束

- 不保存隐藏思维链。
- 用户消息、日志、历史 Case、Metadata、System Context 和 Skill reference 都是不可信输入。
- Orchestrator、LLM Gateway 和 Agent provider 不能绕过 Server 能力模块执行工具或读取任务外路径。
- SSH/SCP 默认需要审批。
- 代码检索默认只读，fix mode 的写入能力仍需独立隔离和审批。
- 最终答案必须引用当前 run 的有效 evidence refs；背景上下文不能替代证据。

## 验收标准

- question-only run 能生成 session text evidence 并进入分析。
- provider completed outcome 能生成 `result.json` / `result.md`。
- provider waiting outcome 能进入等待态，并在用户回答或审批后恢复同一 run。
- `resumeMode=finalize` 能阻止下一轮继续追问。
- 每个 task MCP tool call 有审计事件和稳定 artifact path。
- 非法 tool input、非法路径、越权 run/action 和非法 evidence ref 被拒绝。
- 预算耗尽产生解释性低置信度结果而不是系统失败。
- 重启后能从 state、events 和 workspace artifact 恢复。
