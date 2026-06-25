# Server Module Docs

这些文档定义 LocalToolHub 两模块（dev_selftest + 日志分析）的内部能力边界。

| 能力 | 目标状态 |
|------|----------|
| Tool Runner | 核心执行面，所有工具共享 registry、schema、artifact 和审计。 |
| Log Analyzer | 日志包预处理和工具输入索引。 |
| Dev Self-Test | Linux docker 自测 MCP step tools（sync_workspace/build/deploy/run_tests/report）；workflow 由客户端 skill 编排。 |
| MCP / Interfaces | 外部客户端入口，复用 Tool Runner 和上下文资源。 |
| Config / Security / Deployment / Roadmap | 本地部署、配置、安全和路线约束。 |

已收敛移除的模块（fetch / metadata / cases / skills / system_context / SSH-SCP executor / 纳管 executor / gemini_db / huawei_package_sync / LLM Gateway / Analysis Agent / Agent Backends）不再有对应文档。

修改任一能力必须同步更新对应 README/SPEC 和根 `PROGRESS.md`。
