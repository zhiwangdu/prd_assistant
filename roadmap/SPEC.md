# Roadmap Spec

## 目标

按“持久化基础 -> 证据能力 -> 调查闭环 -> 模型网关 -> 用户交互 -> Case”推进，避免把单次 LLM 调用误当作完整 Agent。

## 当前进度

已完成上传与 Upload session 持久化、任务持久化、解压、初始 grep、Metadata API/WebUI、task Metadata context、可恢复 Executor、Tool Runner MVP、真实 `influxql_analyzer` smoke、单次 LLM Gateway、Analysis Agent 用户追问/审批恢复 API 和静态 WebUI 托管。真实 Environment Collector 尚未实现。

## 下一阶段优先级

1. 接入真实 `flux_query_analyzer`，并扩展 `influxql_analyzer` compare mode delta 字段映射。
2. Environment Collector，将当前 approval 后的 mock evidence 替换为真实 SSH/SCP 采集。
3. Code Evidence。
5. Case Store 保存和召回。

## 阶段门槛

- 没有持久化和恢复前，不接入自动多轮循环。
- 没有 action schema、白名单和预算前，不允许模型请求执行动作。
- 没有 message/decision 幂等前，不开放等待状态恢复。
- 没有证据引用校验前，不允许最终结果进入 Case Store。

## 验收标准

- 每个阶段可从 API 或 WebUI 独立验证。
- 多轮、追问、审批、拒绝、预算终止和重启恢复都有测试。
- 组件 README/SPEC、根 PROGRESS 和 roadmap 保持一致。
