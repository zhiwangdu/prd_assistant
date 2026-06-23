# Server Module Docs

这些文档定义 Local Tool/MCP Workbench 的内部能力边界。目录名沿用 main 分支历史结构，但目标定位已经从 Agent 分析系统切换为本地工具平台。

| 能力 | 目标状态 |
|------|----------|
| Tool Runner | 核心执行面，所有工具共享 registry、schema、artifact 和审计。 |
| MCP / Interfaces | 外部客户端入口，复用 Tool Runner 和上下文资源。 |
| Metadata | 本地实例快照管理和查询资源。 |
| Fetch | 受控 HTTP endpoint 管理和运行。 |
| Environment Collector | SSH/SCP Executor 模板化远程采集。 |
| Code Evidence | 本地代码仓只读检索。 |
| Log Analyzer | 日志包预处理和工具输入索引。 |
| Skills | 可复用说明、runbook 和工具背景资源。 |
| Memory/Case | 人工经验记录和召回。 |
| LLM Gateway / Analysis Agent / Agent Backends | 可选自动化，不是默认后端。 |
| Config / Security / Deployment / Roadmap | 本地部署、配置、安全和路线约束。 |

修改任一能力必须同步更新对应 README/SPEC 和根 `PROGRESS.md`。
