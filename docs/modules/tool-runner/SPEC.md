# Tool Runner Spec

## Descriptor

```text
toolId
displayName
source
backend
runnable
paramsSchema
paramsTemplate
acceptedSuffixes
minFiles/maxFiles
outputViews
```

## Requirements

- 所有工具运行生成 run record。
- stdout/stderr/result 都保存 artifact。
- timeout、exit code、parser error 可见。
- MCP 和 WebUI 共用 runner。
- Secret 脱敏。
- `logagent.runs.get/result` 是 platform 查询工具，不创建 run record。
- Tool Runner 不承载 Fetch、Metadata、Case、Skills 或 SSH/SCP Executor。

## Acceptance

- fake tool 单测覆盖成功、失败、timeout。
- 真实 analyzer smoke 有脚本。
- `/api/tools` 与 MCP `tools/list` 一致。
- dev_selftest 和日志分析工具都通过同一 catalog 暴露。
