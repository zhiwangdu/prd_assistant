# Code Evidence

Code Evidence 对配置的本地代码仓做只读检索，帮助开发和测试定位实现细节。

## 职责

- 管理 repo/ref/search roots 配置。
- 执行 rg/git diff 等只读查询。
- 输出文件、行号、片段和 artifact。
- 通过 WebUI 和 MCP 暴露。
