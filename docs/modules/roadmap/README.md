# Roadmap

## 工期预估

| 组件 | 预估 |
|------|------|
| 已有上传、Metadata 和 WebUI 闭环 | 已完成 MVP |
| Server 持久化与可恢复状态机 | 4~6 天 |
| Tool Runner MVP | 已完成 |
| Code Evidence | 已完成 V2 只读 MVP |
| Environment Collector | 已完成 V2 远程命令、单文件 SCP、批量采集和唯一 hint 选型 |
| Analysis Orchestrator | 4~6 天 |
| Claude Code MCP Session Runner | 已完成 MVP |
| Domain Adapters | 持续迭代 |
| LLM Gateway | 3~4 天 |
| Case Store | 3~4 天 |
| WebUI 调查交互 | 4~6 天 |
| 集成、安全和恢复测试 | 4~6 天 |

## 第 1 阶段：持久化任务基础

- Server 持久化任务列表、Upload session、稳定状态和执行阶段。（已完成）
- Metadata 接入 task context，生成 `metadata_context.json`。（已完成）
- WebUI 从 Server 读取任务列表，不再只依赖 localStorage。（已完成）
- 将线性 Pipeline 重构为可恢复 Executor dispatcher，并定义 Action/Evidence 协议。（已完成）
- 定义 analysis state/event store 和 schema version。

## 第 2 阶段：当前产品闭环

- Tool Runner MVP 已接入 Server；真实 InfluxQL、Flux、openGemini storage 和 InfluxDB storage analyzers 已通过 `third_party/` submodules 引用，并由 `scripts/build-tools.sh` 构建到 LogAgent 工具目录，`scripts/smoke-source-built-analyzers.sh` 可聚合运行四个真实 CLI smoke。下一步基于真实生产 fixture 扩展风险规则和 storage finding 映射。
- Tools 页面 MVP 已接入 Server 和 WebUI，首个 `pprof_analyzer` 通过 `tool_run` task 复用上传、任务状态、workspace 和 artifact 机制；后续更多工具应按同一 registry/adapter 方式扩展。
- 围绕现有上传、Metadata、Tool Runner、Claude Code MCP、Domain Adapter 和 WebUI 流程补齐端到端产品闭环。
- 完善任务创建、等待用户、审批、结果展示、证据跳转、结果确认和 smoke 流程，使当前逻辑可稳定演示和反复使用。
- 所有结果关联 `actionId` 并使用稳定证据引用。

## 第 3 阶段：Claude Code MCP 与 Domain Adapter

- 已新增 `claude_code` 配置、`mcp` 配置、Claude Code session runner 和 Settings dry-run 诊断。
- 已新增 `opengemini_influxdb` active adapter，以及 Cassandra/RocksDB skeleton adapter。
- 已固化 `analysis_package.json`、`claude_prompt.md`、`claude_mcp_config.json`、`claude_session.json`、`mcp_calls.jsonl` 和真实 `agent_response.json` session 输入/响应产物。
- 下一步完善 Claude Code usage/cost、session resume、mode-specific native tool policy 和 MCP tool tests。
- Claude 通过 MCP tools 请求日志、工具、Case、Metadata、用户追问和审批。
- 安全只读动作自动执行；远程采集默认等待批准。

## 第 4 阶段：LLM Gateway

- 作为 Case import、alias 和兼容恢复路径保留 Provider 配置和错误分类。
- Prompt 组装、证据裁剪和 token 预算。
- final answer 结构化输出和 evidence ref 校验。
- LLM Gateway stub provider、Case/alias 辅助调用和有限重试。
- 不保存隐藏思维链。

## 第 5 阶段：WebUI 调查交互

- 调查时间线、事实/假设/缺口和预算展示。
- 待补充问题卡片和 message 提交。
- 待审批动作卡片、风险说明和批准/拒绝。
- 任务恢复、最终结果和证据跳转。

## 第 6 阶段：Case Store

- 仅保存人工确认后的最终结果。
- embedding 和 Top 5 相似召回。
- Case 编辑、禁用和检索。
- 不沉淀中间假设、隐藏推理或未验证结论。

## 最后阶段：远程和代码证据

- Code Evidence 已完成版本到配置 ref 的只读 `git grep` MVP；后续补独立 worktree/cache、版本 diff、符号级解析和 fix mode 隔离修改。
- Environment Collector 已完成审批后的白名单 SSH 命令、单文件 SCP、最多
  20 个 approved `targets[]` 批量采集，以及多 executor/template 的唯一 hint
  选型，并已内置通用、openGemini、Cassandra 和 RocksDB 基础只读环境模板。
- 后续补真实环境 smoke 和生产 fixture 验证。

## 后续质量提升

- 更好的日志模式归一化。
- 版本间 diff / commit 对比。
- 更多测试环境采集模板。
- Cassandra 和 RocksDB domain adapter 的真实 fixture、工具和 runbook。
- 失败任务诊断和观测指标。
- pgvector 迁移。

MVP 保持单 Orchestrator、任务级上下文和单 Rust Server，不引入 Multi-Agent、长期用户记忆、独立队列或 Worker，也不复制成熟 agent 产品的通用能力。
