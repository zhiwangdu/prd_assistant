# System Context 方案

## 当前实现状态

System Context 现在是 task 级背景快照，不再是长期知识正文编辑器。稳定知识、诊断流程、术语和工具说明迁移到 Codex-compatible Skills；System Context 负责把选中或自动匹配的 Skill 摘要、SKILL.md 注入片段、reference 索引和 Metadata adapter 摘要固化为 `system_context.json`。

已实现：

- `system_context.json` schema v2。
- `resources[]` 继续保留，兼容 WebUI、Prompt 和 MCP 消费。
- 新增 `diagnostic_skill` resource kind。
- Metadata instance 继续作为 `metadata_instance` adapter 写入快照。
- 旧 `SystemContextStore` 数据保留在 `storage.data_dir/system_context/resources`，旧读取/维护 API 保留兼容；新任务不再注入旧非 Metadata resources。
- `GET /api/system-context/resources` 默认只返回 Metadata adapter。
- `GET /api/skills`、`GET /api/skills/:skill_id` 和 `POST /api/skills/preview` 负责新的 Skill 入口。
- MCP `resources/read system_context` 继续返回 `system_context.json`。
- 只读 HTTP MCP 提供 `logagent.preview_system_context`，可按 Skill IDs、产品/版本/环境或 Metadata instance 预览将注入的 Skill-backed System Context，但不创建 task、不写 `system_context.json`。

## 职责

负责：

- 固化 task 创建时选择和自动匹配的 Diagnostic Skills。
- 固化 Metadata instance adapter summary。
- 为 Claude Code 和 WebUI 提供可审计背景摘要。
- 记录 Skill revision、source root、reference index 和注入片段。

不负责：

- 编辑 Prompt Pack、Architecture、Runbook、Glossary、Tool Capability 或 Knowledge Note 正文。
- 保存 Skill reference 全文；reference 通过 MCP tool 按需读取并写入 `skill_references/` artifact。
- 替代当前任务证据；最终结论仍必须引用 session text、grep、tool 或 case 等任务内证据。

## 数据目录

```text
data_dir/
  system_context/
    resources/
      ctx_xxx.json   # legacy data, retained
  workspaces/
    task_xxx/
      system_context.json
      skill_references/
        skill_ref_xxx.json
```

## Prompt 语义

System Context 只作为背景参考进入 Claude Code 输入。Skill 内容和 reference 不能覆盖 Server 侧 schema、安全边界、工具白名单、审批策略或最终 evidence 校验。

只读 HTTP MCP 的 System Context preview 只返回预览文本和资源摘要，不能保存、激活或修改 System Context/Skill/Metadata。

最终根因 evidence ref 禁止使用：

```text
system_context.json
diagnostic_skill
skill_references/*
```
