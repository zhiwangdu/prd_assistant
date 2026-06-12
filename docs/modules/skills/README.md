# Skills 方案

## 当前实现状态

Skills 是 LogAgent 的稳定诊断知识载体，兼容 Codex Skill 目录结构。Server 启动时扫描配置的 Skill roots，默认扫描仓库内 `skills/`。

已实现：

- `SkillRegistry` 读取 `SKILL.md` 和可选 `logagent.json`。
- `SKILL.md` 使用 Codex-compatible frontmatter，只依赖 `name` 和 `description`。
- LogAgent 专用匹配字段放在 `logagent.json`，不污染 Codex Skill 规范。
- 无 `logagent.json` 的外部 Skill 可被用户显式选择，但不会自动匹配。
- Skill revision 使用 `SKILL.md`、`logagent.json` 和 references manifest 的稳定 hash。
- HTTP API：`GET /api/skills`、`GET /api/skills/:skill_id`、`POST /api/skills/preview`。
- Task 创建时把显式 Skill、自动匹配 Skill 和 Metadata adapter 写入 `system_context.json` schema v2。
- MCP tool `logagent.get_skill_reference` 按需读取已快照 Skill 的声明 reference，并写入 `skill_references/<stable_id>.json`。
- 只读 HTTP MCP 通过 `logagent://skills`、`logagent://skills/{skill_id}`、`logagent.list_skills`、`logagent.get_skill` 和 `logagent.get_skill_reference` 暴露当前 Server 索引到的 Skill 知识；该入口不写 workspace artifact。
- `GET /api/exports/skills.zip` 可下载当前索引 Skill 目录快照，包含普通文件和 manifest，跳过 symlink。

## 目录结构

```text
skills/
  opengemini-diagnosis/
    SKILL.md
    logagent.json
    references/
      topology.md
      common-failure-paths.md
```

已内置初始 Skills：

- `opengemini-diagnosis`
- `influxql-analysis`
- `pprof-diagnosis`

## logagent.json

```json
{
  "schemaVersion": 1,
  "skillId": "opengemini-diagnosis",
  "displayName": "openGemini diagnosis",
  "products": ["opengemini", "influxdb"],
  "domainAdapters": ["opengemini_influxdb"],
  "toolIds": ["influxql_analyzer"],
  "taskKinds": ["log_analysis"],
  "includeByDefault": true,
  "priority": 80,
  "maxPromptChars": 2400,
  "references": [
    {
      "path": "references/topology.md",
      "title": "PT, Shard, and Index topology",
      "summary": "How to interpret openGemini metadata topology."
    }
  ]
}
```

`references[]` 只声明 path/title/summary。Server 创建 task 时不复制 reference 全文，只在 MCP tool 被调用时读取。

## 匹配规则

- 显式 `skillIds` 总是优先。
- 自动匹配只考虑有 `logagent.json` 的 managed Skills。
- 无 Metadata product/version/environment 时不自动注入 Skill，只使用显式选择。
- `includeByDefault=true` 且 `products` / `taskKinds` 匹配时自动加入。
- 排序按 `priority` 降序，再按 display name。

## 安全边界

- Server 不执行 Skill `scripts/`。
- reference path 必须是 Skill 目录内的相对路径，禁止绝对路径和 `..`。
- MCP `logagent.get_skill_reference` 只能读取当前 task `system_context.json` 中已选择 Skill 的 manifest references。
- 只读 HTTP MCP 的 `logagent.get_skill_reference` 只能读取当前 registry 中已声明的 reference，不写入 Server 数据。
- `skills.zip` 导出只包含普通文件，不跟随 symlink，不允许路径逃逸。
- 读取 reference 会写入 `skill_references/<stable_id>.json`，该 artifact 是背景引用。
- 最终根因 evidence ref 禁止使用 `system_context.json`、`diagnostic_skill` 和 `skill_references/*`。
