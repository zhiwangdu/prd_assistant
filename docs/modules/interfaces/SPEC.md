# Interfaces Spec

## 目标

定义 Server、Analysis Orchestrator、Agent Backend、LLM Gateway、Domain Adapter 和证据模块之间的稳定契约。

## 当前状态

Server 已实现第一版 `TaskContext`、Action、Evidence 和 `EvidenceProvider` 契约，以及持久化 phase 驱动的 Executor dispatcher。Tool Runner 已实现第一个 Evidence Provider。Agent Backend 配置摘要、dry-run 诊断、外部契约产物和 Domain Adapter 内置 registry 已实现；真实外部 Agent Backend 执行尚未接入。

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
agent_request.json
agent_response.json
domain_context.json
```

公共 JSON 必须包含 `schemaVersion`。证据引用使用 workspace 相对路径和稳定 selector，禁止把绝对敏感路径暴露给模型或 WebUI。

Log Analysis 公开历史入口是 Session。Session 保存草稿、upload 引用、task run 列表、active task 和 timeline；task workspace 仍是每次执行的不可变快照。

Task schema 现在包含 `taskKind` 和可选 `sessionId`：

- `log_analysis`：完整上传、解压、grep、Tool Runner、Analysis Orchestrator、Agent Backend result 流程。
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

## Action 契约

支持：

- `search_logs`
- `run_tool`
- `collect_code_evidence`
- `collect_environment`
- `ask_user`
- `final_answer`

每个 action 必须有 `actionId`、`type`、`reason`、`evidenceRefs`、`input`、`risk` 和 `fingerprint`。模块输出必须关联 `actionId`，保证审计和幂等。

第一版 Rust/JSON 契约要求：

- Action type 使用 snake_case。
- risk 使用稳定大写枚举。
- Evidence artifact 使用 workspace 相对路径，拒绝绝对路径和 `..`。
- Provider 返回的 artifact 在持久化前必须通过路径校验。

## Agent Backend 契约

第一阶段已暴露 Settings 摘要、dry-run 诊断和 task workspace 契约产物。当前 `agent_response.json` 标记为 `not_invoked`，后续真实外部后端运行时必须遵守：

- Server 生成 `analysis_package.json` 和 `agent_request.json`。
- 后端只返回 `agent_response.json`。
- `agent_response.json` 只能表达已支持 action 或 final answer。
- Server 继续负责 action schema、白名单、预算、幂等和审批。

## Rust 接口

优先定义：

- `AnalysisAgent`
- `AgentBackend`
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

- action 无法绕过 Server 直接执行。
- 外部 Agent Backend 无法绕过 Server 直接执行。
- Domain Adapter 只能推荐证据组织和工具能力，不能放宽白名单或审批策略。
- 等待状态可通过 message 或 decision 恢复。
- 重复 action 不产生重复副作用。
- 公共 JSON schema 可版本化。
- `RUNNING` 任务重启后保留 phase，并从该 phase 幂等恢复。
- phase 推进带 expected phase 校验，陈旧执行器不能覆盖状态。
- `tool_run` 任务不能混入 `/api/tasks` 日志分析列表，必须通过 `/api/tools/runs` 查询。
- Log Analysis 历史必须以 `/api/sessions` 为主入口；每次重新分析创建新的 task run。
- README 和 SPEC 在接口、状态或 action 变更时同步更新。
