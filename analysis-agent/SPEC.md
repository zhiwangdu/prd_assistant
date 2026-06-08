# Analysis Agent Spec

## 目标

提供可持久化、可恢复、可审计的单 Agent 调查闭环，在受限预算内自主请求证据、向用户追问并生成结构化结果。

## 当前状态

已实现 Analysis State Store MVP 和 `PLAN_ANALYSIS` 单轮 action loop。当前仍未启用完整 LLM 多轮调查循环。

已落地：

- `analysis_state.json`
- `analysis_events.jsonl`
- `GET /api/tasks/:task_id/analysis`
- grep/tool/final result/failure 的基础事件记录
- model decision 事件记录
- 重启恢复到中间 phase 时，如果缺少 analysis state，会按当前 task 生成最小快照继续执行
- LLM Gateway ActionDecision / FinalAnswer 双模式 schema 和 parser
- 单轮消费 `search_logs`、`run_tool` 或 `final_answer`

尚未实现：

- 多轮 action loop
- `WAITING_FOR_USER`
- `WAITING_FOR_APPROVAL`
- 预算终止和重复 action 防护

## 输入

- `TaskContext`
- 当前 `EvidenceBundle`
- `analysis_state.json`
- `analysis_events.jsonl`
- 用户新增消息或审批决定
- LLM Gateway 的结构化决策

## 输出

- 更新后的 `analysis_state.json`
- 追加的 `analysis_events.jsonl`
- 一个待 Server 处理的结构化 action
- 终态时的 `result.json` 和 `result.md`

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

## Action Schema

动作枚举：

```text
search_logs
run_tool
collect_code_evidence
collect_environment
ask_user
final_answer
```

每个动作必须包含：

- `action_id`
- `type`
- `reason`
- `evidence_refs`
- `input`
- `risk`
- `fingerprint`

Server 必须在执行前验证动作类型、输入 schema、白名单、预算和审批策略。LLM Gateway 不得绕过 Server 调用能力模块。

## 用户追问

`ask_user` 的问题项至少包含：

- `question_id`
- `question`
- `reason`
- `required`
- `answer_format`

用户通过任务 message API 回答。Server 追加事件、关闭对应请求并恢复同一任务，不创建独立分析任务。

## 一致性与恢复

- 状态更新使用 revision 或等价并发控制，防止重复恢复。
- action 使用 `action_id` 和 fingerprint 幂等。
- 事件流仅追加，state 可由事件和最新快照恢复。
- Server 重启后能识别运行中、等待中和未完成动作。
- 最终结果生成后禁止继续自动动作；用户显式重新分析应创建新的 analysis revision。

## 安全约束

- 不保存或展示隐藏思维链。
- 只记录简短决策依据、假设、事实和证据引用。
- Agent 无文件系统、shell、网络或 SSH 的直接执行权限。
- 远程采集默认需要用户批准。
- 用户消息和日志内容均视为不可信输入，不能改变系统白名单或执行策略。

## 验收标准

- 能在两轮以上的 stub 决策中执行动作并合并新证据。
- 能进入 `WAITING_FOR_USER`，接收回答后恢复。
- 能进入 `WAITING_FOR_APPROVAL`，批准或拒绝后继续。
- 重复动作和预算耗尽能确定终止。
- 重启后可从持久化状态恢复。
- 最终结果包含证据引用、不确定性和终止原因。
