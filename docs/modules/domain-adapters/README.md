# Domain Adapters 方案

## 定位

Domain Adapters 是 LogAgent 的领域差异化层。成熟 agent 后端可以做通用推理，但具体系统的问题分析需要 LogAgent 提供可靠的领域证据组织方式，包括日志模式、元数据结构、工具结果、Runbook、Case 和测试流水线线索。

第一批领域：

- `opengemini_influxdb`：当前默认 active adapter。
- `cassandra`：skeleton。
- `rocksdb`：skeleton。

## 当前实现状态

已实现内置只读 registry，并通过 Settings API 暴露摘要：

```http
GET /api/settings/domain-adapters
```

当前 registry 不改变任务执行路径，主要用于产品方向显式化和后续任务创建/自动识别的接口基础。

## 领域职责

每个 Domain Adapter 后续负责：

- 产品和版本识别。
- 日志关键词、错误码、组件名和阶段分类。
- 元数据或拓扑解释。
- 可用工具清单和工具结果解释。
- 可注入 System Context / Runbook 片段。
- 测试流水线失败阶段和 artifact 分类。
- 面向 agent backend 的证据摘要。

## 初始领域

`opengemini_influxdb`：

- openGemini `/getdata` 元数据。
- DB/PT/Shard/Index 拓扑。
- InfluxQL/Flux 查询分析。
- `influxql_analyzer`、`flux_query_analyzer`、`pprof_analyzer`。

`cassandra`：

- system.log / debug.log。
- schema、ring、token ownership。
- repair、compaction、tombstone、read/write latency。
- 计划工具：`nodetool status`、`tpstats`、`compactionstats`。

`rocksdb`：

- RocksDB LOG、MANIFEST、OPTIONS。
- SST 元数据、flush、compaction、write stall。
- 计划工具：`ldb`、`sst_dump`、日志解析器。

## 后续计划

1. 在 task/session 创建时记录 `domainAdapterId`。
2. 基于上传文件、Metadata 产品字段和用户问题自动推荐 adapter。
3. 让 Tool Runner、System Context 和 Agent Backend 请求构建读取 adapter 能力。
4. 为 Cassandra/RocksDB 增加 fixture 和解析测试。
