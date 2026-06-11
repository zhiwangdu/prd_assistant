# Roadmap Spec

## 目标

按“持久化基础 -> 证据能力 -> Claude Code MCP 适配 -> Domain Adapter -> 用户交互 -> Case”推进。LogAgent 不复制 Claude Code 的通用能力，而是提供可审计证据工作台和领域诊断增强。

## 当前进度

已完成上传与 Upload session 持久化、任务持久化、解压、初始 grep、Metadata API/WebUI、task Metadata context、可恢复 Executor、Tool Runner MVP、Tools 页面 MVP、`pprof_analyzer` 示例工具、真实 `influxql_analyzer` smoke、单次 LLM Gateway、Analysis 用户追问/审批恢复 API、Claude Code session runner 配置/诊断、LogAgent MCP artifacts、Domain Adapter registry 和静态 WebUI 托管。当前 `influxql-analyzer` 已配置到 `/usr/bin/influxql-analyzer`，可直接调用；真实 Environment Collector 尚未实现并延后。

## 下一阶段优先级

1. 完善 Claude Code session runner 的 structured outcome、用量审计、错误分类、resume 和 permission profile。
2. 围绕现有上传、Metadata、Tool Runner、Tools、Claude Code MCP、Domain Adapter 和 WebUI 逻辑补齐完整产品闭环，包括任务创建、工具运行、追问/审批、证据展示、结果确认和本地 smoke。
3. 接入真实 `flux_query_analyzer`，并扩展 `influxql_analyzer` compare mode delta 字段映射。
4. Cassandra 和 RocksDB domain adapter 的真实 fixture、日志模式和工具设计。
5. Code Evidence。
6. Environment Collector，将当前 approval 后的 mock evidence 替换为真实 SSH/SCP 采集。

## 阶段门槛

- 没有持久化和恢复前，不接入长时间 session orchestration。
- 没有 MCP tool schema、白名单和预算前，不允许模型请求执行领域动作。
- 没有 message/decision 幂等前，不开放等待状态恢复。
- 没有证据引用校验前，不允许最终结果进入 Case Store。

## 验收标准

- 每个阶段可从 API 或 WebUI 独立验证。
- 多轮、追问、审批、拒绝、预算终止和重启恢复都有测试。
- 组件 README/SPEC、根 PROGRESS 和 roadmap 保持一致。
