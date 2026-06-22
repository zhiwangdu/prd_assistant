# Security Spec

## Requirements

- Secret 只来自 env/local secret，不写 artifact。
- Authorization/Cookie/token 默认脱敏。
- 解压和 artifact download 防路径逃逸。
- MCP 不能绕过 Server policy。
- SSH/SCP 禁止自由路径和自由命令。

## Acceptance

- 安全失败返回明确错误。
- 日志和导出包扫描不含 secret 原文。
