# Roadmap Spec

## 目标

按“持久化基础 -> 证据能力 -> 调查闭环 -> 模型网关 -> 用户交互 -> Case”推进，避免把单次 LLM 调用误当作完整 Agent。

## 当前进度

已完成上传、解压、初始 grep、Metadata API/WebUI 和静态 WebUI 托管。Analysis Agent、LLM Gateway 和相关恢复 API 尚未实现。

## 下一阶段优先级

1. Server 持久化任务列表、稳定状态和执行阶段。
2. Metadata 接入 task context 并写入 `metadata_context.json`。
3. Tool Runner、Code Evidence 和 Environment Collector。
4. Analysis state/event store、action executor 和预算控制。
5. LLM Gateway 和结构化决策。
6. `WAITING_FOR_USER` / `WAITING_FOR_APPROVAL` API 与 WebUI。
7. Case Store 保存和召回。

## 阶段门槛

- 没有持久化和恢复前，不接入自动多轮循环。
- 没有 action schema、白名单和预算前，不允许模型请求执行动作。
- 没有 message/decision 幂等前，不开放等待状态恢复。
- 没有证据引用校验前，不允许最终结果进入 Case Store。

## 验收标准

- 每个阶段可从 API 或 WebUI 独立验证。
- 多轮、追问、审批、拒绝、预算终止和重启恢复都有测试。
- 组件 README/SPEC、根 PROGRESS 和 roadmap 保持一致。
