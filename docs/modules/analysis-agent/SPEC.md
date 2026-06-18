# Analysis Agent Spec

## 目标

提供可持久化、可恢复、可审计的单 Orchestrator 调查闭环，在受限预算内汇总证据、启动或恢复 Claude Code session、向用户追问并生成结构化结果。执行上下文是一次 Session run 对应的 task workspace；Session 负责保存草稿、上传引用和多次 run 历史。

## 当前状态

已实现 Analysis State Store MVP、`PLAN_ANALYSIS` Claude Code session orchestration、LogAgent MCP stdio server、用户追问和审批恢复 API。V2 同时提供 run-scoped `/api/v2/runs/:run_id/messages` / `/api/v2/actions/:action_id/decisions` 和 task-scoped `/api/v2/tasks/:task_id/messages` / `/api/v2/tasks/:task_id/actions/:action_id/decision` 入口，后者用于承接 Rust/V1 的 taskId 语义并校验 action 属于对应 task。`collect_environment` 批准后已可接入 Remote Executor 白名单命令、通过 V2 白名单 file template 拉取单个有大小上限的 SCP 文件，或通过审批输入中的 `targets[]` / `remoteTargets[]` 批量采集多个远程目标；多 executor / 多模板场景已支持基于 `target` / `executor` / `node` / `host` 和 `template` / `command` / `file` hint 的确定性唯一匹配，匹配不到或有歧义时写入 `REMOTE_REJECTED` 并拒绝执行 SSH/SCP。V2 已内置通用只读环境模板和 openGemini 基础只读模板，更多 Cassandra/RocksDB 产品专用模板仍未实现。Claude Code runner 已提供配置、诊断接口和 session 输入/响应产物。

已落地：

- `analysis_state.json`
- `analysis_events.jsonl`
- `system_context.json`
- `GET /api/v2/tasks/:task_id/analysis` 和 `GET /api/v2/runs/:run_id/analysis`
- `GET /api/sessions/:session_id/timeline` 聚合 Session events 和 task analysis events
- grep/tool/final result/failure 的基础事件记录
- Claude Code session lifecycle 事件记录
- 重启恢复到中间 phase 时，如果缺少 analysis state，会按当前 task 生成最小快照继续执行
- Claude structured outcome / FinalAnswer schema 和 parser
- Claude Code 配置摘要和 dry-run 诊断
- `analysis_package.json`、`claude_prompt.md`、`claude_mcp_config.json`、`claude_session.json`、`mcp_calls.jsonl` 和真实 `agent_response.json`
- `analysis_package.json` 包含 bounded artifact index outline，列出当前 run
  已知 artifact 的 path/source/role/size/contentType，供 Agent 发现
  support artifacts 后再通过任务 MCP `artifact_index` 或具体资源读取细节
- task `analysisLanguage` 进入 `analysis_state.json`、`analysis_package.json` 和 Claude Code startup prompt，约束自然语言输出使用 `zh-CN` 或 `en-US`
- Domain Adapter 内置 registry
- Claude MCP `search_logs`、`get_log_slice`、`run_domain_tool`、`recall_cases`、`get_metadata_topology`、`query_metadata`、`get_metadata_field_types`、`get_metadata_tag_fields`
- `request_user_input` 进入 `WAITING_FOR_USER`，用户回答后恢复同一任务
- `request_approval` 进入 `WAITING_FOR_APPROVAL`，批准或拒绝后恢复同一任务
- `request_user_input` / `request_approval` 均写入 V1 兼容的
  `mcp_waiting_request.json`，响应保留 V2 `action` 并补齐
  `artifactPath`、`runtimeStatus` 和 `evidenceRefs`；`request_approval` 可只传
  V1 必填的 `reason`，缺省 `actionType` 为 `manual_approval`
- `run_tool` 可消费 Tool Runner 产生的真实 `influxql_analyzer` 结构化 evidence
- Rust/V1 `analysis.max_rounds`、`analysis.max_llm_calls`
- V2 `LOGAGENT_V2_AGENT_MAX_ROUNDS`、`LOGAGENT_V2_AGENT_MAX_LLM_CALLS`、
  `LOGAGENT_V2_AGENT_MAX_ACTIONS`、
  `LOGAGENT_V2_AGENT_MAX_REPEATED_ACTION_FINGERPRINTS`、
  `LOGAGENT_V2_AGENT_MAX_TOTAL_TOKENS`、
  `LOGAGENT_V2_AGENT_MAX_RUNTIME_SECONDS` 和
  `LOGAGENT_V2_AGENT_MAX_USER_PROMPTS`、
  `LOGAGENT_V2_AGENT_MAX_APPROVALS`，默认分别为 4、4、6、1、200000、300、3、3；
  预算或重复 tool fingerprint 耗尽时写入 `budgetLimited=true` 低置信度最终
  结果，任务进入 `SUCCEEDED` 而不是 `FAILED`

尚未实现：

- 更多 Cassandra/RocksDB 产品专用环境模板

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
- `analysis_package.json.task.analysisLanguage`，用于审计本轮 Claude Code 自然语言输出偏好

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

`WAITING_FOR_USER` 和 `WAITING_FOR_APPROVAL` 可恢复到 `RUNNING`。`WAITING_FOR_USER` 支持 `resumeMode=finalize`，表示用户没有更多补充信息，下一轮必须直接生成最终结果。执行阶段用于进度展示，不代替稳定状态。

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
- `logagent.search_code`（存在配置仓库时）
- `logagent.run_domain_tool`
- `logagent.recall_cases`
- `logagent.get_metadata_topology`（兼容 alias，返回 Metadata outline）
- `logagent.query_metadata`
- `logagent.get_metadata_field_types`
- `logagent.get_metadata_tag_fields`
- `logagent.request_user_input`
- `logagent.request_approval`

Server 必须在执行前验证 MCP tool 名称、输入 schema、白名单、预算和审批策略。LLM Gateway 和 Claude Code 不得绕过 Server 调用能力模块。
`logagent.search_logs` 后续检索支持 V1 兼容的可选 `maxMatches`，按 1..200 裁剪；task MCP `tools/list` 和 OpenAI-compatible / binary provider prompt 都必须广告该可选参数。每次调用必须写入独立 `log_searches/logsearch_*.json`，返回 `matches[].text`、`keywordCounts`、`unmatchedKeywords` 和稳定 `log_searches/...#matches/<index>` refs，不得覆盖初始 `grep_results.json`。响应必须保留 V2 `search` 对象，同时补齐 Rust/V1 顶层 `artifactPath`、`totalMatches`、`keywordCounts`、`unmatchedKeywords`、`matches`、`matches[].index`、`evidenceRefs` 和 `note`。Claude Code prompt 必须要求模型基于命中行正文判断异常，不能只把 `totalMatches` 当作具体异常类型、技术栈或根因证据。
`logagent.get_log_slice` 必须支持两种输入形态：V2 中心行形态 `path` + `lineNumber` + 可选 `before/after`，以及 V1 兼容 range 形态 `path` + `startLine/endLine`。task MCP `tools/list` 和 OpenAI-compatible / binary provider prompt 都必须广告这两种形态。两种形态不能在同一调用中混用，range 形态要求 `endLine >= startLine` 且 `endLine - startLine <= 500`。响应必须保留 V2 `slice` 对象，同时补齐 Rust/V1 顶层 `artifactPath`、`evidenceRefs` 和 `lines`；同一 path 和请求 line range 必须产生稳定 `log_slices/slice_<digest>.json#lines` 引用，artifact 的 `startLine` / `endLine` 保留请求范围，`lines[]` 只包含实际存在的行。
`logagent.search_code` 仅在配置本地代码仓时广告。调用必须限制到管理员配置的 product、version/ref 和 search roots，执行只读 `git rev-parse` / `git grep <commit>`，写入 `code_evidence/<action_id>.json`，并返回最终答案可引用的 `code_evidence/<action_id>.json#matches/<index>` refs。
`analysis_package.json` 和任务 MCP 默认 `metadata_context` resource 只提供 Metadata outline；完整 `metadata_context.json` 保留在 workspace，必须通过 `logagent.query_metadata` 读取 bounded slice，slice 写入 `metadata_slices/<stable_id>.json` 并作为背景上下文处理。需要从全局已导入 Metadata 精确查询 field/tag 类型时，使用 `logagent.get_metadata_field_types` / `logagent.get_metadata_tag_fields`；task MCP 会写入 `metadata_slices/field_types_<stable_id>.json` / `metadata_slices/tag_fields_<stable_id>.json`，这些 slice 同样是背景上下文。
Claude Code startup prompt 必须依据 task `analysisLanguage` 约束自然语言字段：`zh-CN` 优先简体中文，`en-US` 使用英文；协议名、JSON key、路径、工具名、产品名和 evidence refs 不翻译。

## 用户追问

`request_user_input` 的问题项至少包含：

- `question_id`
- `question`
- `reason`
- `required`
- `answer_format`

用户通过任务 message API 回答。Server 追加事件、关闭对应请求并恢复同一任务，不创建独立分析任务。
当 message API 收到 `resumeMode=finalize` 时，Server 仍追加用户消息并关闭 pending prompt，但还必须在 `analysis_package.json` 中设置 `analysisState.finalizeRequested=true`，并通过 prompt 禁止 Claude 再次返回 `waiting_for_user`。

`request_approval` 兼容 Rust/V1 schema：`reason` 是唯一必填字段，`actionType` 可选；缺省时按 `manual_approval` 处理。审批请求同样写入 `mcp_waiting_request.json`，用于 MCP call 审计和旧提示中的等待 marker 读取。

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
- task `analysisLanguage=zh-CN/en-US` 时，`claude_prompt.md` 包含对应语言约束。
- 能进入 `WAITING_FOR_USER`，接收回答后恢复。
- 能进入 `WAITING_FOR_APPROVAL`，批准或拒绝后继续。
- 重启后可从持久化状态恢复。
- 最终结果包含证据引用、不确定性和终止原因。
