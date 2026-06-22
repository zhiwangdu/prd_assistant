# Code Evidence Spec

## Requirements

- 只能访问配置仓库和 roots。
- 默认 detached/read-only worktree。
- 不自动修改代码。
- 输出必须有文件路径和行号。

## Acceptance

- MCP `logagent.search_code` 返回 bounded matches。
- WebUI 可下载 code evidence artifact。
