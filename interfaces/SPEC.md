# Interfaces Spec

## 目标

定义 Server、Analysis Agent、LLM Gateway 和证据模块之间的稳定契约。

## 当前状态

已有 Server DTO 和 Pipeline 内部模型；本文件定义待实现的调查闭环接口。

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
- README 和 SPEC 在接口、状态或 action 变更时同步更新。
