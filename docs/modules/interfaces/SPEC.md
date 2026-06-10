# Interfaces Spec

## 目标

定义 Server、Analysis Agent、LLM Gateway 和证据模块之间的稳定契约。

## 当前状态

Server 已实现第一版 `TaskContext`、Action、Evidence 和 `EvidenceProvider` 契约，以及持久化 phase 驱动的 Executor dispatcher。Tool Runner 已实现第一个 Evidence Provider。Analysis Agent、Action Store 和 Provider 注册表尚未实现。

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
```

公共 JSON 必须包含 `schemaVersion`。证据引用使用 workspace 相对路径和稳定 selector，禁止把绝对敏感路径暴露给模型或 WebUI。

Task schema 现在包含 `taskKind`：

- `log_analysis`：完整上传、解压、grep、Tool Runner、Analysis Agent、LLM result 流程。
- `tool_run`：手动工具运行，复用上传、TaskStore、workspace 和 `RUN_TOOL` phase，成功后通过 `/api/tools/runs/:task_id/result` 暴露工具结果。

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

## Rust 接口

优先定义：

- `AnalysisAgent`
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
- 等待状态可通过 message 或 decision 恢复。
- 重复 action 不产生重复副作用。
- 公共 JSON schema 可版本化。
- `RUNNING` 任务重启后保留 phase，并从该 phase 幂等恢复。
- phase 推进带 expected phase 校验，陈旧执行器不能覆盖状态。
- `tool_run` 任务不能混入 `/api/tasks` 日志分析列表，必须通过 `/api/tools/runs` 查询。
- README 和 SPEC 在接口、状态或 action 变更时同步更新。
