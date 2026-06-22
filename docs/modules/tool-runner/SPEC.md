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

## Acceptance

- fake tool 单测覆盖成功、失败、timeout。
- 真实 analyzer smoke 有脚本。
- `/api/tools` 与 MCP `tools/list` 一致。
