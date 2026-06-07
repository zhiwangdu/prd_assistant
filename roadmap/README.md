# Roadmap

## 工期预估

| 组件 | 预估 |
|------|------|
| 已有上传、Metadata 和 WebUI 闭环 | 已完成 MVP |
| Server 持久化与可恢复状态机 | 4~6 天 |
| Tool Runner | 2~3 天 |
| Code Evidence | 4~6 天 |
| Environment Collector | 4~6 天 |
| Analysis Agent | 6~9 天 |
| LLM Gateway | 3~4 天 |
| Case Store | 3~4 天 |
| WebUI 调查交互 | 4~6 天 |
| 集成、安全和恢复测试 | 4~6 天 |

## 第 1 阶段：持久化任务基础

- Server 持久化任务列表、Upload session、稳定状态和执行阶段。（已完成）
- Metadata 接入 task context，生成 `metadata_context.json`。（已完成）
- WebUI 从 Server 读取任务列表，不再只依赖 localStorage。（已完成）
- 将线性 Pipeline 重构为可恢复 Executor dispatcher，并定义 Action/Evidence 协议。
- 定义 analysis state/event store 和 schema version。

## 第 2 阶段：证据能力

- Tool Runner 接入 `flux_query_analyzer` 和 `influxql_analyzer`。
- Code Evidence 完成版本到 ref 映射和只读 worktree 检索。
- Environment Collector 完成白名单 SSH/SCP 采集。
- 所有结果关联 `actionId` 并使用稳定证据引用。

## 第 3 阶段：Analysis Agent 闭环

- 实现任务级 `analysis_state.json` 和 `analysis_events.jsonl`。
- 实现 facts、hypotheses、information gaps 和 action fingerprint。
- 实现 `search_logs`、`run_tool`、`collect_code_evidence`、`collect_environment`、`ask_user`、`final_answer`。
- 安全只读动作自动执行；远程采集默认等待批准。
- 实现最大轮数、模型调用数、动作数、重复动作、token 和运行时间预算。
- 实现重启恢复、幂等和预算终止。

## 第 4 阶段：LLM Gateway

- Provider 配置和错误分类。
- Prompt 组装、证据裁剪和 token 预算。
- action/final answer 结构化输出校验。
- LLM stub 和有限重试。
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

## 后续质量提升

- 更好的日志模式归一化。
- 版本间 diff / commit 对比。
- 更多测试环境采集模板。
- 失败任务诊断和观测指标。
- pgvector 迁移。

MVP 保持单 Agent、任务级上下文和单 Rust Server，不引入 Multi-Agent、长期用户记忆、独立队列或 Worker。
