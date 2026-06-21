# Analysis Orchestrator

## 定位

旧 Analysis Agent 设计已收敛为 V2 Analysis Orchestrator。用户可见入口是 Log Analysis Session；每次点击 Analyze 都会从 Session 当前问题、上传、Metadata/System Context 选择和历史消息创建一个新的 run/task workspace 快照。

Orchestrator 负责：

- 汇总当前 run 的证据、背景上下文和预算。
- 生成 `analysis_package.json` 和 provider prompt。
- 调用当前 `LOGAGENT_V2_AGENT_PROVIDER` 对应 provider。
- 执行 provider 请求的受控 task MCP tools。
- 进入并恢复 `WAITING_FOR_USER` / `WAITING_FOR_APPROVAL`。
- 校验最终答案 schema 和 evidence refs。
- 持久化 state、events、artifacts 和最终结果。

Orchestrator 不负责：

- 直接执行任意 shell、SSH、SCP 或外部工具。
- 绕过 Tool Runner/Remote Executor/Code Evidence/Metadata 的白名单。
- 保存模型隐藏思维链。
- 把历史 Case 或 System Context 当作当前任务根因证据。

## 当前运行循环

```text
Session 问题和上传
  -> 创建 run/task workspace 快照
  -> 解压、manifest、初始 grep、工具预处理
  -> 固化 system_context.json、metadata_context.json、case_context.json
  -> 生成 analysis_package.json 和 provider prompt
  -> Agent provider 返回 completed / waiting_for_user / waiting_for_approval
  -> task MCP tools 写入新证据和审计事件
  -> Server 校验最终 result 或进入等待态
```

`LOGAGENT_V2_AGENT_PROVIDER` 默认是 `stub`。启用 `openai_compatible`、`binary` 或 `claude_code` 时，Orchestrator 仍使用同一套证据、预算、等待恢复和结果校验规则。

## 当前实现

已落地：

- `analysis_state.json` 和 `analysis_events.jsonl`。
- `GET /api/v2/tasks/:task_id/analysis`、`GET /api/v2/runs/:run_id/analysis`。
- `GET /api/v2/sessions/:session_id/timeline` 聚合 Session events 和 run analysis events。
- Session-first run 创建、问题-only run、上传附加、同一 Session 多次 run。
- `analysis_package.json` bounded artifact index，供 provider 发现支持性 artifact。
- `system_context.json`、Metadata outline、Case context 和 Domain Adapter 背景注入。
- task `analysisLanguage=zh-CN|en-US`，约束自然语言输出语言。
- task MCP tools：日志搜索、日志切片、领域工具、Case recall、Metadata slice、Skill reference、Code Evidence、Fetch、用户追问和审批请求。
- `WAITING_FOR_USER` message 恢复，并支持 `resumeMode=finalize`。
- `WAITING_FOR_APPROVAL` decision 恢复。
- Remote Executor 白名单命令、白名单 SCP file template、approved `targets[]` 批量远程采集。
- 预算边界：轮次、provider 调用、动作数、重复 action fingerprint、累计 token、单次运行时长、用户追问次数和审批次数。
- 预算耗尽时生成 `budgetLimited=true` 的低置信度结果，并以 `SUCCEEDED` 结束。
- 最终结果 evidence ref 校验和 Markdown/JSON result artifact。

Claude Code provider 启用时会额外生成 `claude_prompt.md`、`claude_mcp_config.json`、`claude_session.json` 和 `mcp_calls.jsonl`；这些是 provider-specific artifact，不代表 V2 默认依赖 Claude Code。

## 关键产物

每个 run/task workspace 持久化：

```text
session_text_input.json
manifest.json
grep_results.json
metadata_context.json
system_context.json
case_context.json
analysis_package.json
analysis_state.json
analysis_events.jsonl
agent_request.json
agent_response.json
result.json
result.md
```

按需产生：

```text
log_searches/*.json
log_slices/*.json
tool_results/*/result.json
metadata_slices/*.json
skill_references/*.json
code_evidence/*.json
environment_evidence/*.json
fetch_results/*.json
```

## 自动执行与审批

默认可自动执行：

- task workspace 内日志搜索和日志切片。
- 已启用且 runnable 的白名单 Tool Runner 工具。
- 已配置本地仓库和 ref 的只读代码检索。
- Case Store 只读召回。
- Metadata bounded slice 查询。

默认需要审批：

- SSH/SCP 远程环境采集。
- 扩大远程目标或文件采集范围的动作。
- 配置显式标记为 `approval_required` 的工具或执行机动作。

## 最终结果要求

`result.json` 必须包含 summary、symptoms、likelyRootCauses、nextChecks、fixSuggestions、missingInformation、confidence 和 evidence refs。证据引用必须来自当前 run 的可引用 artifact，例如：

- `session_text_input.json#question`
- `grep_results.json#matches/<index>`
- `log_searches/<id>.json#matches/<index>`
- `log_slices/<id>.json#lines`
- `tool_results/<action_id>/result.json#findings/<index>`
- `code_evidence/<action_id>.json#matches/<index>`
- `case_context.json#cases/<index>`，仅作为历史参考

System Context、Diagnostic Skill reference 和 Metadata slice 只能作为背景参考，不能作为根因证据。
