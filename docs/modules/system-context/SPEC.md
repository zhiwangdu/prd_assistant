# System Context Spec

## 目标

System Context 管理 LogAgent 可复用的长期背景资源，并在每次 Log Analysis run 创建时固化为 task 快照，保证 Prompt 输入可恢复、可审计。

## API

受保护接口：

```http
GET /api/system-context/resources
POST /api/system-context/resources
GET /api/system-context/resources/:context_id
PATCH /api/system-context/resources/:context_id
POST /api/system-context/resources/:context_id/versions
PATCH /api/system-context/resources/:context_id/versions/:version_id
POST /api/system-context/resources/:context_id/versions/:version_id/activate
POST /api/system-context/preview
```

资源类型：

```text
prompt_pack
architecture_doc
runbook
glossary
tool_capability
metadata_instance
knowledge_note
```

版本状态：

```text
draft
active
archived
```

内容类型：

```text
text
markdown
mermaid
json_summary
metadata_adapter
```

## Task 集成

`AnalysisSessionRecord` 持久化 `systemContextIds`。创建 Log Analysis task 时，Server 会合并：

- Session 显式选择的资源。
- 启用且 `includeByDefault=true` 的匹配资源。
- 当前 Metadata context 的 adapter summary。

合并结果写入 `system_context.json`，`TaskRecord.systemContextPath` 指向该快照。`GET /api/tasks/:task_id/artifacts` 返回 `systemContext` 和 `systemContextPath`。

## 安全和约束

- System Context 不能保存密钥。
- `metadata_instance` 由 Metadata Store adapter 生成，不能通过 System Context API 直接创建。
- Prompt Pack 不能绕过代码中的 schema、安全边界、工具白名单或审批策略。
- System Context 不作为最终结果 evidence ref；它只能作为背景参考。

## 验收

- 资源创建、更新、版本激活后重启可加载。
- Prompt preview 能展示实际将注入的背景资源。
- 创建 Log Analysis run 后 workspace 包含 `system_context.json`。
- Session timeline 包含 `system_context_recorded`。
- Agent Backend 输入包含 System Context 摘要；当前 `claude_agent_sdk` 后端会通过 `analysis_package.json` 接收这些背景资源。
- Metadata 原有 API 和 WebUI 拓扑展示保持可用。
