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
POST /api/skills/preview
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

## 验收

- 合法/非法 `SKILL.md`、缺失/非法 `logagent.json`、重复 `skillId` 能被测试覆盖。
- path traversal 和 root 外 reference 被拒绝。
- 显式 Skill、自动匹配 Skill、无 Metadata 仅显式 Skill 的 task 创建语义正确。
- `system_context.json` schema v2 包含 selected skills + metadata adapter。
- MCP reference 读取成功写 artifact，拒绝未选择 Skill、未声明 reference 和越界路径。
- 最终结果 evidence ref 拒绝 Skill/System Context refs。
