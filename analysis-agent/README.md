# Analysis Agent 方案

## 定位

Analysis Agent 是 LogAgent 的任务级调查编排器。它负责持续维护问题上下文，执行多轮“观察证据 -> 更新假设 -> 识别缺口 -> 请求动作 -> 合并结果”的分析循环，直到形成最终结论、等待用户输入或达到预算。

Analysis Agent 与 LLM Gateway 分离：

- Analysis Agent 管理状态、轮次、动作、预算、恢复和终止条件。
- LLM Gateway 只负责模型适配、Prompt 组装、证据裁剪和结构化响应解析。
- Server 是唯一动作执行者，负责权限、白名单、审批、持久化和模块调度。

MVP 保持单 Agent、任务级上下文，不实现 Multi-Agent 或用户级长期记忆。

## 调查循环

```text
用户问题 + 当前证据 + 历史事件
  -> 生成本轮决策
  -> Server 校验结构化动作
  -> 自动执行安全只读动作，或等待用户/审批
  -> 写入新证据和审计事件
  -> 更新事实、假设和信息缺口
  -> 下一轮或 final_answer
```

初始日志提取不是固定的一次性前置流水线。Agent 可以在后续轮次继续请求更精确的日志搜索、工具分析、代码检索或环境采集。

## 当前实现状态

已实现 Analysis State Store MVP，并启用 `PLAN_ANALYSIS` 多轮 LLM action loop。用户追问和审批尚未启用。

当前 Server 会在现有固定 pipeline 中持久化：

- `analysis_state.json`
- `analysis_events.jsonl`

已记录的事件和状态包括：

- analysis 初始化。
- manifest 证据。
- grep evidence。
- Tool Runner action 和 tool evidence。
- model decision。
- final result。
- failure 事件。

`GET /api/tasks/:task_id/analysis` 可读取当前 state 和事件流。真实 `flux_query_analyzer` / `influxql_analyzer` 尚未完成时，Tool Runner 继续使用配置中的 mock/stub 工具替代，保证 action/event/evidence 链路先稳定。

LLM Gateway 已接入 `PLAN_ANALYSIS` 多轮决策。当前 `search_logs` 会按模型关键词重建 grep evidence 并进入下一轮，`run_tool` 会走白名单 Tool Runner 通道并进入下一轮，`final_answer` 会直接持久化结果。循环受 `analysis.max_rounds`、`analysis.max_llm_calls`、`analysis.max_actions` 和 `analysis.max_repeated_action_fingerprints` 控制；达到预算或重复 fingerprint 上限时会生成低置信度结果并正常终止。

## 上下文产物

每个 task workspace 持久化：

```text
analysis_state.json
analysis_events.jsonl
result.json
result.md
```

`analysis_state.json` 至少包含：

- schema 版本和当前 revision
- 当前 task 状态与执行阶段
- 用户问题和已补充消息
- 已确认事实、候选假设和未解决信息缺口
- 证据引用索引
- 待执行、待审批和待用户回答的请求
- 已完成动作的 fingerprint
- 轮数、模型调用数、动作数、token 和运行时间预算

`analysis_events.jsonl` 是仅追加的审计事件流，记录用户消息、模型决策摘要、动作执行结果、审批和状态变化。不得保存模型隐藏思维链；只保存简短、可审计的决策依据和证据引用。

## Action 协议

支持的动作类型：

- `search_logs`
- `run_tool`
- `collect_code_evidence`
- `collect_environment`
- `ask_user`
- `final_answer`

动作通用字段：

```json
{
  "actionId": "act_123",
  "type": "search_logs",
  "reason": "需要确认 timeout 是否集中在 compaction 期间",
  "evidenceRefs": ["grep_results.json#matches/12"],
  "input": {},
  "risk": "safe_read_only"
}
```

模型不能提供任意命令、任意文件路径、任意仓库 URL 或任意 SSH 地址。动作输入必须由 Server 按对应模块 schema 校验并映射到配置白名单。

## 自动执行与审批

MVP 默认自动执行：

- task workspace 内的日志搜索
- 白名单 Tool Runner 调用
- 已配置仓库和 ref 上的只读代码检索
- Case Store 只读召回

默认需要用户批准：

- SSH/SCP 环境采集
- 可能扩大远程采集范围的动作
- 配置明确标记为 `approval_required` 的动作

`ask_user` 进入 `WAITING_FOR_USER`，问题必须说明所需信息、原因、是否必填和可接受格式。用户回答作为同一任务的新事件继续分析。

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

所有结论必须引用任务内证据。历史 Case 只能作为参考，不能替代当前任务证据。
