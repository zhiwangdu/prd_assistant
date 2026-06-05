# Metadata Spec

## 目标

Metadata 模块管理实例、集群和节点元数据，为日志分析提供业务、部署和拓扑上下文。

## 当前状态

基础实现已完成：

- 本地 JSON 文件存储。
- 实例、集群和集群节点查询 API。
- JSON/YAML 模板导入预览。
- openGemini `/getdata` snapshot 解析。
- Server 侧按 URL 拉取真实元数据并生成导入预览。
- 导入确认后写入 store。
- WEBUI Metadata 页面。

仍待实现：

- CSV 模板解析。
- task context 关联 `instanceId` / `clusterId` / `nodeId`。
- `metadata_context.json` 写入 workspace。

## 职责边界

负责：

- `instance_id` 到产品、版本、集群、环境和节点信息的映射。
- 集群节点拓扑。
- 元数据模板导入、校验、预览和确认。
- 为 task context 提供可引用的 `instanceId`、`clusterId`、`nodeId`。
- 为 WEBUI 展示实例和集群元数据。

不负责：

- SSH/SCP 采集。
- 日志解压和 grep。
- 代码仓切换和检索。
- LLM 分析。

## 数据模型草案

Instance:

```json
{
  "instanceId": "i-123",
  "clusterId": "c-redis-prod-1",
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
  "clusterId": "c-redis-prod-1",
  "name": "redis-prod-1",
  "product": "redis",
  "version": "7.2.4",
  "environment": "prod",
  "nodes": ["node-1", "node-2", "node-3"]
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
GET /api/metadata/instances/:instance_id
GET /api/metadata/clusters/:cluster_id
GET /api/metadata/clusters/:cluster_id/nodes
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
  "clusterId": "c-redis-prod-1",
  "nodeId": "node-1",
  "sourceUrl": "..."
}
```

Task workspace 后续可写入：

```text
metadata_context.json
```

用于 LLM Agent、Tool Runner、Code Evidence 和 Environment Collector。

## WEBUI

- Metadata 页面已实现。
- 实例查询。
- 集群节点查询。
- 真实元数据 URL 拉取和预览。
- 模板输入、导入预览和确认。
- task 创建时选择实例/集群。

## 安全约束

- Metadata API 需要 API Key。
- 模板导入只解析数据，不执行模板内容。
- SSH 地址和别名只作为数据保存，不直接触发连接。
- 导入前必须预览并确认。

## 验收标准

- 能通过实例 ID 查询实例元数据。
- 能通过集群 ID 查询集群和节点列表。
- 能提交 YAML/JSON 模板并得到导入预览。
- 能从 `http://127.0.0.1:8091/getdata` 拉取 openGemini metadata 并归一化展示。
- 导入确认后 metadata store 可查询。
- 后续 task 创建可关联 instance/cluster/node 上下文。
- README 和 SPEC 在字段、模板格式或 API 变更时同步更新。
