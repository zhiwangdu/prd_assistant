# Interfaces Spec

## 目标

定义 Server、Analysis Orchestrator、Claude Code Session Runner、LogAgent MCP、LLM Gateway、Domain Adapter 和证据模块之间的稳定契约。

## 当前状态

Server 已实现第一版 `TaskContext`、Action、Evidence 和 `EvidenceProvider` 契约，以及持久化 phase 驱动的 Executor dispatcher。Tool Runner 已实现第一个 Evidence Provider。Fetch endpoint 已接入同一 `tool_run` / `tool_results` 产物面，并新增 `tool_results/<action_id>/result.json#response` 作为受控最终证据引用。Code Evidence 已通过任务 MCP 写入 `code_evidence/<action_id>.json#matches/<index>` 最终证据引用。Huawei package sync 已接入同一 `tool_run` / `tool_results` 产物面，首版只支持受保护 Tools API 手动运行，不新增最终答案 evidence ref 类型。Claude Code 配置摘要、dry-run 诊断、MCP/session 契约产物和 Domain Adapter 内置 registry 已实现。只读 HTTP MCP 已实现为独立受保护接口：`POST /api/mcp/readonly`。它面向个人本地 Claude Code 读取共享知识，与任务 stdio MCP 分离，不绑定 task，不读取 workspace，不执行 action，包括不执行 Fetch endpoint 和 Huawei package sync。

## 公共产物

```text
manifest.json
grep_results.json
metadata_context.json
metadata_slices/*.json
tool_inputs/index.json
tool_inputs/**/*.jsonl
tool_results/*.json
tool_results/*/response_body.bin
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

Log Analysis 公开历史入口是 Session。Session 保存草稿、`analysisMode`、语言、upload 引用、task run 列表、active task 和 timeline；task workspace 仍是每次执行的不可变快照。

Task schema 现在包含 `taskKind` 和可选 `sessionId`：

- `log_analysis`：完整上传、解压、grep、Tool Runner、Analysis Orchestrator、Claude Code session result 流程。
- `log_analysis` 必须绑定 `sessionId`。
- `tool_run`：手动工具运行，复用上传、TaskStore、workspace 和 `RUN_TOOL` phase，不绑定 Session，成功后通过 `/api/tools/runs/:task_id/result` 暴露工具结果。内置 `logagent.preprocess_log_package` 使用同一任务类型生成预处理摘要和 `tool_inputs`；内置 `logagent.fetch` 也使用 `tool_run`，但无需上传文件，结果写入 `tool_results/<action_id>/result.json` 和 `response_body.bin`；内置 `logagent.huawei_cloud_package_sync` 使用一个上传文件，结果写入 `tool_results/<action_id>/result.json`。

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

- `logagent.search_logs`，支持 `keywords` 和 V1 兼容可选 `maxMatches`，`maxMatches` 按 1..200 裁剪；响应保留 V2 `search` 对象，并补齐 Rust/V1 顶层 `artifactPath`、`totalMatches`、`keywordCounts`、`unmatchedKeywords`、`matches`、`matches[].index`、`evidenceRefs` 和 `note`
- `logagent.get_log_slice`，支持 `path + lineNumber + before?/after?` 和 V1 兼容的 `path + startLine/endLine`；两种形态不能混用；响应保留 V2 `slice` 对象，并补齐 Rust/V1 顶层 `artifactPath`、`evidenceRefs` 和 `lines`；slice artifact 的 `startLine` / `endLine` 保留请求范围，`lines[]` 只包含实际存在的行
- `logagent.run_domain_tool`，`tools/list` schema 同时广告 V2 `toolId` 和 Rust/V1 `tool + inputFile` 调用形态；保留 V2 `result/artifact/evidence` 响应，并补齐 Rust/V1 顶层 `artifactPath`、`summary` 和 `evidenceRefs`；多输入工具额外返回 `artifactPaths`，有 findings 时返回最终答案可引用的 `finalEvidenceRefs`
- `logagent.list_fetch_endpoints`
- `logagent.fetch`
- `logagent.recall_cases`
- `logagent.get_metadata_topology`
- `logagent.query_metadata`
- `logagent.get_metadata_field_types`
- `logagent.get_metadata_tag_fields`
- `logagent.request_user_input`，保留 V2 `action`，同时写入并返回 Rust/V1 `mcp_waiting_request.json`、`runtimeStatus` 和 `evidenceRefs`
- `logagent.request_approval`，保留 V2 `action`，同时写入并返回 Rust/V1 `mcp_waiting_request.json`、`runtimeStatus` 和 `evidenceRefs`；可只传 V1 必填的 `reason`，缺省 `actionType` 为 `manual_approval`

只读 HTTP MCP resources 支持：

- `logagent://skills`
- `logagent://skills/{skill_id}`
- `logagent://metadata/instances`
- `logagent://metadata/instances/{instance_id}/snapshot`
- `logagent://cases/recent`
- `logagent://tools/catalog`
- `logagent://domain-adapters`

Python V2 同时保留 `logagent-v2://...` URI alias；`resources/list` 会动态广告当前已导入的 Skill 和 Metadata snapshot 资源，resource 内容中的 `uri` 回显调用方请求的 URI。

Python V2 的只读 MCP 和 task MCP handler 均接受单个 JSON-RPC request 或 JSON-RPC batch array；batch array 按输入顺序返回响应数组。二者都支持 V1 的 `ping` 和空 `prompts/list`。

只读 HTTP MCP tools 支持：

- `logagent.search_cases`
- `logagent.get_case`
- `logagent.list_skills`
- `logagent.get_skill`
- `logagent.get_skill_reference`
- `logagent.preview_system_context`
- `logagent.list_metadata_instances`
- `logagent.get_metadata_snapshot`
- `logagent.get_metadata_field_types`
- `logagent.get_metadata_tag_fields`
- `logagent.list_tools`
- `logagent.list_domain_adapters`

Readonly `logagent.preview_system_context` accepts `skillIds`, `product`,
`version`, `environment`, and `instanceId`, and returns combined `resources`,
split `skillResources` / `systemResources`, plus a prompt preview without
writing task artifacts.
Readonly `logagent.get_skill` returns the indexed skill both at top level and
inside the Rust/V1-compatible `skill` wrapper.
Readonly and manual-tool `logagent.get_metadata_snapshot` responses preserve
the V2 top-level snapshot fields and add the Rust/V1-compatible `snapshot`
wrapper.
Task `logagent.recall_cases` responses return Rust/V1-compatible
`artifactPath`, `caseCount`, and per-case `evidenceRefs`, and persist the
logical `case_recall/recall_<stable_id>.json` path on background
`case_context` evidence.

只读 HTTP MCP 的工具目录资源和 `logagent.list_tools` 可以展示 `logagent.fetch` descriptor，便于个人 Claude Code 理解团队 Server 有哪些受控能力；`tools/call logagent.fetch` 必须返回不支持，不能执行 HTTP 请求。

第一版 Rust/JSON/MCP 契约要求：

- MCP tool name 使用 `logagent.*` 前缀。
- risk 使用稳定大写枚举。
- Evidence artifact 使用 workspace 相对路径，拒绝绝对路径和 `..`。
- Provider 返回的 artifact 在持久化前必须通过路径校验。

## Claude Code Session 契约

已暴露 Settings 摘要、dry-run 诊断和 task workspace session 输入/响应产物。当前 `agent_response.json` 由 Claude Code runner 调用后写入，运行时必须遵守：

- Server 生成 `analysis_package.json`、短启动 `claude_prompt.md` 和 `claude_mcp_config.json`。
- Claude CLI argv/stdin 不能承载完整 `analysis_package.json`；Claude Code 通过 MCP resources/tools 获取证据和请求领域能力。
- Task MCP resource 主 URI 为 `logagent://task/<run_id>/<resource>`，Python V2 保留 `logagent-v2://run/<run_id>/<resource>` alias，并在 content 中回显调用方请求的 URI。
- `analysis_package.json` 和任务 MCP 默认 `metadata_context` resource 不能承载完整 Metadata payload；只暴露 `metadataContextOutline`，细节通过 `logagent.query_metadata` 写入 `metadata_slices/<stable_id>.json` 背景 slice。`logagent.get_metadata_field_types` / `logagent.get_metadata_tag_fields` 在 task MCP 中写入 `metadata_slices/field_types_<stable_id>.json` / `metadata_slices/tag_fields_<stable_id>.json`，响应同时提供 V2 顶层 `fields` 和 Rust/V1 `result` 包装。
- `mcp_calls.jsonl` 记录成功的任务 MCP `resources/read` 和 `tools/call` 调用，包含 call id、arguments、status、result 和 evidence/background refs；Python V2 通过 `mcp_calls` task resource 与 run analysis resources 暴露解析后的调用列表。
- Python V2 持久化当前 run 用户问题为 `session_text_input.json`，并把 `session_text_input.json#question` 放入 `analysis_package` / Agent provider allowed refs；最终答案校验必须确认该 ref 来自当前 run 的 final-allowed `user_question` artifact。
- Python V2 最终答案校验兼容 V1 Case refs：`case_context.json#cases/<index>` 会按当前 run 的 `case_context` artifact 校验，模型输出的 `case_<id>` 或 `历史案例 case_<id>` 会规范化为 canonical Case ref。
- Python V2 最终答案校验兼容 V1 grep ref aliases：`matches/<index>`、
  `matches/<start>-<end>`、`#<start>-#<end>`，以及能命中初始
  `grep_results.json` 行号的裸行号 / 行号范围，都会规范化为 canonical
  `grep_results.json#matches/<index>` refs 后再做当前 run artifact 校验。
- Python V2 任务 MCP 兼容 V1 的 `artifact_index`、`case_context` 和 `tool_results` 资源；`artifact_index` 从 V2 Store 枚举当前 run 上传和 evidence artifacts，`case_context` 返回最新 Case background context，`tool_results` 聚合 `tool_result` / `fetch_result` artifact 并保持 `tool_results/<action_id>/result.json` canonical path。
- `GET /api/v2/runs/<run_id>/result` 在 final answer/result artifact 生成前必须返回 HTTP 409 和当前 run status；成功后返回 finalAnswer、result artifact、Markdown artifact 和对应 evidence metadata。
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
- Task MCP `logagent.get_skill_reference` 必须返回稳定 background artifact envelope，包括 `artifactPath`、`backgroundRef`、`canonicalRef`、`evidenceRefs`、`skillRevision`、reference metadata、`truncated` 和 `finalEvidenceAllowed=false`。
- 只读 HTTP MCP 不能执行 Fetch endpoint；任务 MCP 才能调用 `logagent.list_fetch_endpoints` 和 `logagent.fetch`。
- `logagent.list_fetch_endpoints` 在 Fetch 关闭时必须失败；开启时必须返回 Rust/V1 `schemaVersion=1`、enabled endpoint summaries 和 `finalEvidenceAllowed=false`。
- Fetch response 的最终证据引用格式只接受 `tool_results/<action_id>/result.json#response`，且必须校验该 action 属于当前任务并且 `tool=logagent.fetch`。
- Code Evidence 的最终证据引用格式只接受 `code_evidence/<action_id>.json#matches/<index>`，且必须校验该 evidence 属于当前任务、`final_allowed=true`、payload path 匹配并且 match index 存在。
- Log Analysis 历史必须以 `/api/sessions` 为主入口；每次重新分析创建新的 task run。
- README 和 SPEC 在接口、状态或 action 变更时同步更新。
