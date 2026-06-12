# System Context Spec

## 目标

System Context 在每次 Log Analysis run 创建时固化 Skill-backed 背景快照，保证 Prompt 输入可恢复、可审计，同时保持 Metadata adapter 的运行时事实上下文。

## API

新入口：

```http
GET /api/skills
GET /api/skills/:skill_id
POST /api/skills/preview
```

兼容入口：

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

`GET /api/system-context/resources` 默认只返回 `metadata_instance` adapter。旧非 Metadata resources 不删除，但不作为新 UI 和新任务入口。

## Resource Kind

`system_context.json` schema v2 的 `resources[]` 支持：

```text
diagnostic_skill
metadata_instance
```

旧快照和旧 store 资源仍可包含：

```text
prompt_pack
architecture_doc
runbook
glossary
tool_capability
knowledge_note
```

## Task 集成

`AnalysisSessionRecord` 兼容保留 `systemContextIds`，新增 `skillIds`。创建 Log Analysis task 时，Server 会合并：

- Session 显式选择的 `skillIds`。
- 有 `logagent.json`、`includeByDefault=true` 且匹配 Metadata product/version/environment 的 managed Skills。
- 当前 Metadata context 的 adapter summary。

合并结果写入 `system_context.json`，`TaskRecord.systemContextPath` 指向该快照。`GET /api/tasks/:task_id/artifacts` 返回 `systemContext` 和 `systemContextPath`。

## 安全和约束

- System Context 不能保存密钥。
- `metadata_instance` 由 Metadata Store adapter 生成，不能通过 System Context API 直接创建。
- Skill reference 只能通过 MCP `logagent.get_skill_reference` 读取 task 快照中已声明的 references。
- 禁止绝对路径、`..`、未声明 reference 和未选择 Skill。
- `system_context.json`、`diagnostic_skill` 和 `skill_references/*` 不作为最终结果 evidence ref。

## 验收

- 创建 Log Analysis run 后 workspace 包含 `system_context.json` schema v2。
- Session timeline 包含 `system_context_recorded`，并记录 Skill count / resource count。
- Claude Code 输入包含 Diagnostic Skills 摘要和 Metadata adapter 摘要。
- MCP `resources/read system_context` 可读取快照。
- Metadata 原有 API 和 WebUI 拓扑展示保持可用。
