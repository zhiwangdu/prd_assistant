# Skills Spec

## 目标

用 Codex-compatible Skills 承载稳定诊断知识，让 LogAgent Server 只负责索引、匹配、快照摘要和安全 reference 读取。

## 配置

```yaml
skills:
  enabled: true
  roots:
    - skills
  max_skill_chars: 4000
  max_reference_chars: 20000
```

默认启用，默认 root 为仓库内 `skills/`。相对 root 优先按配置文件目录解析，目录不存在时回退到当前工作目录。

## 输入

- Skill 目录必须包含 `SKILL.md`。
- `SKILL.md` 必须有 YAML frontmatter：

```markdown
---
name: openGemini Diagnosis
description: Diagnose openGemini clusters.
---
```

- 可选 `logagent.json` 提供匹配和 reference manifest。

## API

```http
GET /api/skills
GET /api/skills/:skill_id
POST /api/skills/imports
POST /api/skills/preview
POST /api/mcp/readonly
GET /api/exports/skills.zip
```

`POST /api/skills/preview` 请求：

```json
{
  "skillIds": ["opengemini-diagnosis"],
  "product": "opengemini",
  "version": "1.3.0",
  "environment": "test",
  "instanceId": "inst-1"
}
```

响应返回将写入 task 的 `resources[]` 和 prompt preview。

`POST /api/skills/imports` 请求：

```json
{
  "skillId": "custom-runbook",
  "name": "Custom Runbook",
  "description": "Team-specific diagnostic steps.",
  "markdown": "Use current task evidence first.",
  "filename": "custom-runbook.md"
}
```

响应返回导入后的 `SkillDetailResponse`。Server 必须：

- 在第一个配置的 `skills.roots` 下写入 `<skillId>/SKILL.md` 和 `<skillId>/logagent.json`。
- 用请求的 `name` / `description` 生成 `SKILL.md` frontmatter，并把 `markdown` 写为正文。
- 生成默认 `logagent.json`：`schemaVersion=1`、`displayName=name`、`taskKinds=["log_analysis"]`、`includeByDefault=false`、`priority=0`、空 `references`。
- 导入后重载整个 Skill Registry 并替换内存快照；如果重载失败，删除本次新建目录并保留旧快照。
- 拒绝重复 `skillId`、非法 ID、空字段、禁用 skills、无可写 root，以及非 `.md/.markdown` 的可选 filename。
- v1 不覆盖已有 Skill，不上传 references，不设置自动匹配字段；导入 Skill 仅通过用户显式选择进入 Analyze。

## Task Artifact

`system_context.json` schema v2 中的 Skill item 包含：

- `kind=diagnostic_skill`
- `skillId`
- `revision`
- `sourceRoot`
- `sourcePath`
- `summary`
- `content`，即裁剪后的 SKILL.md 注入片段
- `references[]`，只含 referenceId/path/title/summary

## MCP

新增 tool：

```text
logagent.get_skill_reference
```

输入：

```json
{
  "skillId": "opengemini-diagnosis",
  "referenceId": "ref_xxx"
}
```

也可用声明的 `path`。Server 必须校验：

- Skill 已在当前 task 快照中。
- referenceId/path 已在该 Skill 快照的 `references[]` 中。
- 当前 registry 中 Skill revision 与 task 快照一致。
- reference 文件路径仍在 Skill 目录内。

成功后写入：

```text
skill_references/skill_ref_xxx.json
```

并返回 `backgroundRef=skill_references/skill_ref_xxx.json#content`。

只读 HTTP MCP 也暴露 Skill 读取能力：

```text
resources: logagent://skills, logagent://skills/{skill_id}
tools: logagent.list_skills, logagent.get_skill, logagent.get_skill_reference
```

`logagent.get_skill` 响应保留 V2 顶层 skill 字段并补齐 Rust/V1 `skill` 包装。该入口不依赖 task snapshot，不写入 `skill_references/` artifact，只读取当前 registry 中已声明的 reference，并返回 `finalEvidenceAllowed=false`。

`skills.zip` 必须打包当前 registry 中所有 Skill 目录的普通文件，保留相对目录结构，并在根目录写入 `manifest.json`。导出不得跟随 symlink，不得包含 Skill 目录外文件。

## 验收

- 合法/非法 `SKILL.md`、缺失/非法 `logagent.json`、重复 `skillId` 能被测试覆盖。
- Markdown Skill 导入后，`GET /api/skills`、`GET /api/skills/:skill_id`、Analyze Skill resolve 和 `skills.zip` 都能读取最新 registry。
- 导入重复 `skillId` 不覆盖已有目录；非法 ID、空 `name/description/markdown`、禁用 skills 和不可写 root 返回明确错误。
- path traversal 和 root 外 reference 被拒绝。
- 显式 Skill、自动匹配 Skill、无 Metadata 仅显式 Skill 的 task 创建语义正确。
- `system_context.json` schema v2 包含 selected skills + metadata adapter。
- MCP reference 读取成功写 artifact，拒绝未选择 Skill、未声明 reference 和越界路径。
- 只读 HTTP MCP Skill tool 可读取合法 reference，并拒绝未知 Skill、未声明 reference 和越界 path。
- `skills.zip` 覆盖多 Skill、reference 文件、symlink 不跟随和 manifest 生成。
- 最终结果 evidence ref 拒绝 Skill/System Context refs。
