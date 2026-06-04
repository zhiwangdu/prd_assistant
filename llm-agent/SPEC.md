# LLM Agent Spec

## 目标

LLM Agent 把日志、工具、代码、环境和 Case 证据整理成受约束的模型输入，并输出结构化故障分析结果。

## 当前状态

未实现代码，已有设计方向。

## 输入

- `manifest.json`
- `grep_results.json`
- `tool_results/*.json`
- `code_evidence.json`
- `environment_evidence.json`
- 相似 Case
- 用户问题

## 输出

```text
result.md
result.json
```

结构化结果建议包含：

- `summary`
- `symptoms`
- `likely_root_causes`
- `evidence`
- `next_checks`
- `fix_suggestions`
- `confidence`

## 约束

- LLM 不能直接执行命令。
- 结论必须引用证据来源。
- 无证据支撑的判断必须标注为推测。
- 输入需要裁剪，优先保留错误上下文和工具输出摘要。

## 验收标准

- 输出能追溯到具体证据文件或行号。
- 缺少关键证据时输出明确的不确定性。
- README 和 SPEC 在 Prompt、模型或输出 schema 变更时同步更新。
