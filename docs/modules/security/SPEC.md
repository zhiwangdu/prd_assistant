# Security Spec

## Requirements

- Secret 只来自 env/local secret，不写 artifact。
- Authorization/Cookie/token 默认脱敏。
- 解压和 artifact download 防路径逃逸。
- MCP 不能绕过 Server policy。
- Tool Runner 只能执行 catalog 中 enabled/runnable 的工具。
- dev_selftest 参数只能选择配置好的 profile id 和 runId，不能提交自由 shell。
- Docker test target 的 image/network/workdir/volume/env 必须通过安全校验。

## Acceptance

- 安全失败返回明确错误。
- 日志和导出包扫描不含 secret 原文。
- 移除的 Fetch/Executor/Metadata/Case/Skills 路径不能绕过当前策略。
