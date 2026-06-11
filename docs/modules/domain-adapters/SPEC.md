# Domain Adapters Spec

## 目标

定义数据库和存储系统专项诊断能力包，让 LogAgent 在成熟 agent 后端之上提供差异化的领域证据能力。

## 当前状态

已实现内置 registry：

- `opengemini_influxdb`，状态 `active`。
- `cassandra`，状态 `skeleton`。
- `rocksdb`，状态 `skeleton`。

已实现 API：

```http
GET /api/settings/domain-adapters
```

响应包含：

- `id`
- `displayName`
- `status`
- `products`
- `evidenceKinds`
- `plannedTools`
- `notes`

## 适配器能力模型

每个 adapter 后续至少定义：

- `products`：可匹配产品名和别名。
- `logPatterns`：日志关键词、错误码、组件名、阶段名。
- `metadataSources`：可解释的元数据或拓扑输入。
- `tools`：可用诊断工具及其证据引用格式。
- `systemContext`：可注入的 Runbook、架构说明和术语。
- `evidenceSummarizer`：面向 Claude Code / MCP resource 的证据摘要规则。

## 初始领域边界

`opengemini_influxdb`：

- 继续承载现有 Metadata Explorer、PT/Shard/Index 诊断和 Influx query 工具链。

`cassandra`：

- 第一阶段仅定义 skeleton。
- 后续优先处理 ring ownership、repair、compaction、tombstone、read/write latency 和 `nodetool` 输出。

`rocksdb`：

- 第一阶段仅定义 skeleton。
- 后续优先处理 LOG、MANIFEST、OPTIONS、SST、compaction、flush、write stall 和 perf context。

## 安全约束

- adapter 不能放宽 Server action schema、工具白名单、路径限制或审批策略。
- adapter 只能推荐工具和证据来源，实际执行仍由 Server 校验。
- adapter 提供的 runbook 和 prompt 片段属于背景信息，不能替代当前任务证据。

## 验收标准

- Settings 能列出三类 adapter。
- `opengemini_influxdb` 标记为 active。
- `cassandra` 和 `rocksdb` 标记为 skeleton，不能误导为已具备完整诊断能力。
- 新增产品领域时必须同步 README/SPEC，并至少提供 fixture 或最小验收场景。
