# Metadata Spec

## 目标

Metadata 模块管理实例、集群和节点元数据，为日志分析提供业务、部署和拓扑上下文。

Metadata 在产品入口上归入 System Context 和 Domain Adapter。现有 `/api/metadata/*` API 和 Store 保持不变，System Context 通过 `metadata_instance` adapter 暴露可注入摘要，并随 task 创建固化到 `system_context.json`；`opengemini_influxdb` Domain Adapter 使用 Metadata 作为拓扑和 shard/index 诊断证据来源。

## 当前状态

基础实现已完成：

- 本地 JSON 文件存储。
- 实例、集群和集群节点查询 API。
- JSON/YAML 模板导入预览。
- openGemini `/getdata` snapshot 解析。
- openGemini `PtView` 和 `Databases` 重点字段解析。
- Server 侧按 URL 拉取真实元数据并生成导入预览。
- 导入确认后写入 store。
- openGemini 导入要求用户手工提供 `instanceId`，并以该值作为唯一业务键；原始 `ClusterID` 只作为 `sourceClusterId` 标签保留。
- Instance 支持可选 `remark` 备注名；openGemini 拉取和导入请求可携带，空值不保存，服务端限制最长 120 个字符。
- 已导入 Instance 列表和按 InstanceID 读取拓扑快照。
- WEBUI Metadata 页面支持实时 URL 加载、JSON 文件上传和手动 JSON 文本三种导入方式。
- task context 关联 `instanceId` / `nodeId`；`clusterId` 仅兼容旧请求和内部拓扑。
- `metadata_context.json` workspace 快照和 LLM 摘要。
- Claude Code 初始 evidence package 和任务 MCP `metadata_context` resource 只暴露 `metadataContextOutline`；完整 Metadata 需通过 `logagent.query_metadata` 按 section/filter/分页读取 bounded slice。
- 任务 stdio MCP 和只读 HTTP MCP 提供 `logagent.get_metadata_field_types`，从指定 `instanceId/database/measurement` 查询 field 类型；RP 可省略并回退到 DB 默认 RP，field 可省略以返回全部字段。
- 只读 HTTP MCP Metadata 资源和 tools，可读取已导入 instance 列表、snapshot 和 field type，不写入 Metadata Store。

仍待实现：

- CSV 模板解析。

## 职责边界

负责：

- `instance_id` 到产品、版本、环境和拓扑快照的映射。
- 集群节点拓扑。
- 元数据模板导入、校验、预览和确认。
- 为 task context 提供可引用的 `instanceId`、`nodeId`，并在内部补齐拓扑快照。
- 为 WEBUI 展示实例和集群元数据。

不负责：

- SSH/SCP 采集。
- 日志解压和 grep。
- 代码仓切换和检索。
- Analysis Orchestrator 调查和 Claude Code session 调用。

## 数据模型草案

Instance:

```json
{
  "instanceId": "i-123",
  "remark": "生产集群 A",
  "clusterId": "i-123",
  "nodeId": "node-1",
  "product": "redis",
  "version": "7.2.4",
  "environment": "prod",
  "region": "cn-hangzhou",
  "owner": "team-a",
  "tags": {
    "service": "cache",
    "tier": "storage"
  }
}
```

Cluster:

```json
{
  "clusterId": "i-123",
  "name": "redis-prod-1",
  "product": "redis",
  "version": "7.2.4",
  "environment": "prod",
  "nodes": ["node-1", "node-2", "node-3"],
  "databases": [],
  "partitionViews": []
}
```

Database:

```json
{
  "name": "mydb",
  "defaultRetentionPolicy": "autogen",
  "replicaN": 1,
  "retentionPolicies": [
    {
      "name": "autogen",
      "replicaN": 1,
      "shardGroupDuration": 604800000000000,
      "measurements": [
        {
          "name": "testmst_0000",
          "versionName": "testmst_0000",
          "shardKeyType": "hash",
          "schema": [
            { "name": "tagk", "typ": 6 },
            { "name": "value", "typ": 3 }
          ]
        }
      ],
      "shardGroups": [
        {
          "id": 1,
          "shardIds": [1],
          "owners": [0]
        }
      ]
    }
  ]
}
```

PartitionView:

```json
{
  "database": "mydb",
  "ptId": 0,
  "ownerNodeId": 2,
  "status": 0,
  "statusText": "online",
  "version": 1,
  "replicaGroupId": 0
}
```

Node:

```json
{
  "nodeId": "node-1",
  "instanceId": "i-123",
  "hostname": "redis-1",
  "host": "10.0.0.1",
  "sshAlias": "redis-prod-1-a",
  "role": "primary",
  "zone": "az-a",
  "status": "active",
  "labels": {
    "rack": "r1"
  }
}
```

## 模板导入

当前模板支持 JSON/YAML/openGemini `/getdata` JSON，CSV 预留但暂未实现。导入流程：

```text
upload template
  -> parse
  -> validate required fields
  -> detect duplicate instance/node/cluster
  -> preview changes
  -> confirm
  -> write metadata store
```

模板格式候选：

- CSV：适合人工维护和批量表格。
- YAML：适合集群拓扑层级结构。
- JSON：适合程序导出。

API 层保留 `templateType` 字段。

## API

```http
GET /api/metadata/instances
GET /api/metadata/instances/:instance_id
GET /api/metadata/instances/:instance_id/snapshot
GET /api/metadata/clusters/:cluster_id
GET /api/metadata/clusters/:cluster_id/nodes
POST /api/metadata/snapshots/fetch
POST /api/metadata/imports
POST /api/metadata/imports/fetch
GET /api/metadata/imports/:import_id/preview
POST /api/metadata/imports/:import_id/confirm
POST /api/mcp/readonly
```

导入请求建议：

```json
{
  "templateType": "yaml",
  "filename": "metadata.yaml",
  "content": "..."
}
```

真实 openGemini 元数据拉取请求：

```json
{
  "url": "http://127.0.0.1:8091/getdata",
  "instanceId": "prod-og-1",
  "remark": "生产集群 A",
  "templateType": "opengemini",
  "filename": "opengemini-getdata.json"
}
```

导入预览建议：

```json
{
  "importId": "meta_imp_123",
  "summary": {
    "instances": 10,
    "clusters": 2,
    "nodes": 10,
    "databases": 1,
    "partitionViews": 1,
    "warnings": 1,
    "errors": 0
  },
  "changes": []
}
```

## 存储

当前使用本地 JSON 文件。

建议目录：

```text
data_dir/
  metadata/
    instances.json
    clusters.json
    nodes.json
    imports/
```

后续可升级为 SQLite 表：

- `metadata_instances`
- `metadata_clusters`
- `metadata_nodes`
- `metadata_imports`

## 与任务的关系

Task 创建请求后续扩展：

```json
{
  "uploadIds": ["upl_1"],
  "instanceId": "i-123",
  "nodeId": "node-1",
  "sourceUrl": "..."
}
```

Task workspace 写入：

```text
metadata_context.json
metadata_slices/<stable_id>.json
```

解析规则：

- 未提供 Metadata ID 时仍写入空选择的 context。
- `instanceId` 自动补全其内部拓扑快照和可选 `nodeId`。
- `nodeId` 自动补全其 `instanceId` 和内部拓扑快照。
- 显式 ID 冲突或未知时返回 `400`。
- context 是创建时快照，后续 Metadata Store 更新不改变历史任务。
- cluster `rawSnapshot` 不进入 task context。
- `analysis_package.json` 不内联完整 context，只写 `metadataContextOutline`，包含 `metadataContextPath`、选中 ID、产品/版本/环境、section count 和查询入口。
- 任务 MCP `resources/read metadata_context` 和 `logagent.get_metadata_topology` 返回 outline；`logagent.query_metadata` 支持 `section`、`database`、`retentionPolicy`、`measurement`、`nodeId`、`ownerNodeId`、`ptId`、`shardId`、`indexId`、`limit`、`cursor`。
- `logagent.query_metadata` section 覆盖 `overview | nodes | databases | retention_policies | measurements | fields | shard_groups | shards | index_groups | indexes | partition_views`，返回 bounded `items`、`total`、`nextCursor`、`truncated` 和 `backgroundRef`，并写入 `metadata_slices/<stable_id>.json` / `mcp_calls.jsonl`。
- `logagent.get_metadata_field_types` 必填 `instanceId`、`database`、`measurement`，可选 `retentionPolicy` 和 `field`；`field` 支持字符串或字符串数组，省略时返回 measurement 下所有 fields，结果写入 `metadata_slices/field_types_<stable_id>.json`。

用于 Analysis Orchestrator、Claude Code、Tool Runner、Code Evidence 和 Environment Collector 的受控证据输入。

## WEBUI

- Metadata 页面已实现。
- Metadata 页面包含 Overview、Nodes、Partitions、Metadata Explorer、Schemas、Diagnostics 和 Raw JSON。
- Metadata 页面展示已导入 Instance 列表，读取已存快照时只要求 InstanceID。
- InstanceID 输入旁展示备注名输入框；列表中备注单行省略，Overview 展示备注字段。
- Nodes 页面中 MetaNode 状态固定展示 none；Data/SQL 节点按 0 none、1 alive、2 leaving、3 left、4 failed 映射状态。
- Shard/Index Owners 按 PT ID 解析，经 PtView 关联 DataNode。
- Metadata Explorer 不渲染 Graph，合并原 Topology 和 Databases，提供 `Node / DBPT / Shards` 与 `DB / RP / Shards / Indexes` 两个视角。
- `Node / DBPT / Shards` 视角按 `Database -> DataNode -> DBPT -> Shards` 级联展开，Shard 行必须展示所属 RP、ShardGroup、time range、Owners、IndexID 和 Index 状态信息。
- 缺失 DataNode/PT 使用异常聚合行展示。
- Explorer 支持 Database、DataNode、时间范围、仅异常、Shard 行/Index 信息显隐筛选和 DBPT 聚合详情面板。
- `DB / RP / Shards / Indexes` 视角按 `Database -> RP -> ShardGroup/IndexGroup -> Shard/Index` 级联展开。
- Schemas 页面默认选择第一个非 `_internal` DB 及其第一个 RP，RP 筛选必须跟随 DB 联动，Measurement 或 field 搜索用于缩小结果。
- Schema field type 必须按 openGemini 枚举码解析：`Field_Type_Unknown=0`、`Field_Type_Int=1`、`Field_Type_UInt=2`、`Field_Type_Float=3`、`Field_Type_String=4`、`Field_Type_Boolean=5`、`Field_Type_Tag=6`、`Field_Type_Last=7`，对应展示为 Unknown/Integer/Unsigned/Float/String/Boolean/Tag/Unknown。
- MCP field type 查询返回相同映射的 `typeLabel`，未知扩展码保留原始 `typ` 并展示为 `Type <code>`。
- Metadata 明细表必须支持长列表局部滚动，并在滚动时固定表头。
- Raw JSON 页面必须按需展开原始 JSON，不得在初始渲染时全量 stringify 大对象。
- 实例查询。
- 已导入 Instance 列表和按 InstanceID 查询拓扑快照。
- 兼容集群节点查询，并重点展示 `PtView` 分区归属和 `Databases` 库表/RP/shard 摘要。
- 真实元数据 URL 拉取和预览。
- JSON 文件上传、手动 JSON 文本输入、导入预览和确认。
- 完整 Metadata JSON 模板可不填写 InstanceID；openGemini 原始 JSON 仍需填写 InstanceID。
- task 创建时选择实例和可选节点。

## 安全约束

- Metadata API 需要 API Key。
- 模板导入只解析数据，不执行模板内容。
- SSH 地址和别名只作为数据保存，不直接触发连接。
- 导入前必须预览并确认。
- Metadata slice 默认是背景上下文，不新增最终 evidence ref 类型；最终答案仍不能引用 `metadata_slices/*` 作为根因证据。

## 验收标准

- 能通过实例 ID 查询实例元数据。
- 能展示已导入 Instance 列表，并通过 InstanceID 查询拓扑快照。
- 能在 openGemini 拉取/导入时保存 Instance 备注，并在列表和 Overview 中展示。
- 能通过兼容集群 ID 查询集群和节点列表。
- 能从 cluster 查询结果中直接看到 openGemini `PtView` 的 PT owner 和状态。
- 能从 cluster 查询结果中直接看到 openGemini `Databases` 的默认 RP、Measurements schema 和 ShardGroups。
- 能提交 YAML/JSON 模板并得到导入预览。
- WebUI 能上传 `.json` 文件并得到导入预览。
- WebUI 能粘贴 JSON 文本并得到导入预览。
- 能在用户输入 InstanceID 后从 `http://127.0.0.1:8091/getdata` 拉取 openGemini metadata 并归一化展示。
- `analysis_package.json` 不包含完整 databases/measurements/shards/indexes payload，任务 MCP `metadata_context` resource 返回 outline/counts。
- `logagent.query_metadata` 能按 database/RP/measurement/shard/partition filters 和 limit/cursor 返回 bounded slice，非法 section/filter 返回错误。
- `logagent.get_metadata_field_types` 能按 instance/database/measurement 和可选 RP/field 返回指定字段类型，省略 RP 使用 DB 默认 RP，省略 field 返回全部字段。
- Metadata Explorer 大集群通过级联展开可读，不渲染 Graph。
- Metadata Explorer 能在 `Node / DBPT / Shards` 和 `DB / RP / Shards / Indexes` 两个视角间切换。
- Schemas 页面默认选择非 `_internal` DB 和首个 RP，DB 变化后 RP 选项联动更新，field type 展示为实际类型。
- Raw JSON 大对象默认只展示顶层结构并按需展开，不导致页面卡死。
- 节点、分区、分片、索引和 Schema 表格下翻时表头固定在表格顶部。
- 导入确认后 metadata store 可查询。
- task 创建可关联 instance/node 上下文并返回固化快照。
- README 和 SPEC 在字段、模板格式或 API 变更时同步更新。
