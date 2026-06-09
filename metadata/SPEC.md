# Metadata Spec

## 目标

Metadata 模块管理实例、集群和节点元数据，为日志分析提供业务、部署和拓扑上下文。

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
- 已导入 Instance 列表和按 InstanceID 读取拓扑快照。
- WEBUI Metadata 页面。
- task context 关联 `instanceId` / `nodeId`；`clusterId` 仅兼容旧请求和内部拓扑。
- `metadata_context.json` workspace 快照和 LLM 摘要。

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
- Analysis Agent 调查和 LLM Gateway 调用。

## 数据模型草案

Instance:

```json
{
  "instanceId": "i-123",
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
```

解析规则：

- 未提供 Metadata ID 时仍写入空选择的 context。
- `instanceId` 自动补全其内部拓扑快照和可选 `nodeId`。
- `nodeId` 自动补全其 `instanceId` 和内部拓扑快照。
- 显式 ID 冲突或未知时返回 `400`。
- context 是创建时快照，后续 Metadata Store 更新不改变历史任务。
- cluster `rawSnapshot` 不进入 task context。

用于 Analysis Agent、Tool Runner、Code Evidence、Environment Collector 和 LLM Gateway 的受控证据输入。

## WEBUI

- Metadata 页面已实现。
- Metadata 页面包含 Overview、Nodes、Partitions、Topology、Databases、Schemas、Diagnostics 和 Raw JSON。
- Metadata 页面展示已导入 Instance 列表，读取已存快照时只要求 InstanceID。
- Shard/Index Owners 按 PT ID 解析，经 PtView 关联 DataNode。
- Topology 主链固定为 `DataNode -> Database/PT -> ShardGroup -> Shard -> IndexGroup -> Index`。
- DataNode 是一级大容器；缺失 DataNode/PT 使用异常虚拟容器展示。
- Topology 支持 Database、DataNode、时间范围、仅异常、Shard/Index 显隐筛选和详情面板。
- 实例查询。
- 已导入 Instance 列表和按 InstanceID 查询拓扑快照。
- 兼容集群节点查询，并重点展示 `PtView` 分区归属和 `Databases` 库表/RP/shard 摘要。
- 真实元数据 URL 拉取和预览。
- 模板输入、导入预览和确认。
- task 创建时选择实例和可选节点。

## 安全约束

- Metadata API 需要 API Key。
- 模板导入只解析数据，不执行模板内容。
- SSH 地址和别名只作为数据保存，不直接触发连接。
- 导入前必须预览并确认。

## 验收标准

- 能通过实例 ID 查询实例元数据。
- 能展示已导入 Instance 列表，并通过 InstanceID 查询拓扑快照。
- 能通过兼容集群 ID 查询集群和节点列表。
- 能从 cluster 查询结果中直接看到 openGemini `PtView` 的 PT owner 和状态。
- 能从 cluster 查询结果中直接看到 openGemini `Databases` 的默认 RP、Measurements schema 和 ShardGroups。
- 能提交 YAML/JSON 模板并得到导入预览。
- 能在用户输入 InstanceID 后从 `http://127.0.0.1:8091/getdata` 拉取 openGemini metadata 并归一化展示。
- 导入确认后 metadata store 可查询。
- task 创建可关联 instance/node 上下文并返回固化快照。
- README 和 SPEC 在字段、模板格式或 API 变更时同步更新。
