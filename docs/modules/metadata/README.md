# Metadata 方案

## 当前实现状态

Metadata 模块已完成基础 Rust Server 实现。

模块目标是管理实例 ID 对应的业务和部署元数据、集群节点信息，并把这些信息提供给后续日志分析、环境采集、代码证据和 WEBUI 展示。

产品入口上，Metadata 已纳入 System Context：现有 Metadata Store/API 继续保持专业拓扑模型和诊断能力，System Context 通过只读 `metadata_instance` adapter 把已导入 Instance 摘要纳入通用背景资源目录，并在 task 创建时固化到 `system_context.json`。

已实现：

- 本地 JSON 文件存储。
- `instance` / `cluster` / `node` 查询。
- JSON/YAML 模板导入预览。
- openGemini `/getdata` 真实元数据解析。
- openGemini `PtView` 分区视图解析。
- openGemini `Databases` 库、保留策略、表结构和 shard group 摘要解析。
- Server 侧从真实元数据 URL 拉取并预览。
- openGemini 导入依赖用户手工输入 `instanceId`，并以 `instanceId` 作为唯一业务键；原始 `ClusterID` 仅保存在 `sourceClusterId` 标签中。
- Instance 支持可选 `remark` 备注名，openGemini 实时加载和导入预览可随 `instanceId` 一起提交。
- 导入确认后写入 metadata store，并支持按已导入 Instance 列表查看。
- WEBUI Metadata 页面支持实时 URL 加载、JSON 文件上传和手动 JSON 文本三种导入方式。
- task 创建时关联 `instanceId` / `nodeId`；`clusterId` 已从用户入口弃用，仅作为兼容字段保留。
- 在 task workspace 原子写入 `metadata_context.json`。
- 将产品、版本、环境、节点状态、数据库和 PT 摘要提供给 LLM Gateway。

暂未实现：

- CSV 模板解析。

## 职责

负责：

- 维护实例 ID 到产品、版本、集群、节点、角色等信息的映射。
- 维护集群拓扑和节点清单。
- 支持按模板批量导入元数据。
- 为 Server task 提供以 `instanceId` 为主的上下文。
- 为 WEBUI 提供元数据查询和展示。

不负责：

- 不直接采集测试环境信息；采集由 Environment Collector 负责。
- 不直接分析日志；调查由 Analysis Agent 编排，模型调用由 LLM Gateway 负责。
- 不直接管理代码仓；代码版本证据由 Code Evidence 负责。

## 核心对象

实例：

- `instance_id`
- `remark`：用户备注名，可选，最长 120 个字符
- `cluster_id`：内部拓扑快照键，openGemini 导入时等于 `instance_id`
- `product`
- `version`
- `environment`
- `region`
- `owner`
- `tags`

集群：

- `cluster_id`
- `name`
- `product`
- `version`
- `environment`
- `nodes`
- `databases`
- `partition_views`

数据库：

- `name`
- `default_retention_policy`
- `replica_n`
- `mark_deleted`
- `retention_policies`

保留策略：

- `name`
- `replica_n`
- `duration`
- `shard_group_duration`
- `measurements`
- `shard_groups`

表结构：

- `name`
- `version_name`
- `shard_key_type`
- `schema`
- `mark_deleted`
- `engine_type`

分区视图：

- `database`
- `pt_id`
- `owner_node_id`
- `status`
- `status_text`
- `version`
- `replica_group_id`

节点：

- `node_id`
- `instance_id`
- `hostname`
- `host`
- `ssh_alias`
- `role`
- `zone`
- `status`
- `labels`

## 模板导入

当前支持：

- YAML
- JSON
- openGemini `/getdata` JSON，`templateType` 使用 `opengemini`

预留但暂未实现：

- CSV

导入方式：

- WEBUI 实时加载 openGemini `/getdata` URL。
- WEBUI 上传 JSON 模板文件。
- WEBUI 手动粘贴 JSON 模板文本。
- Server 调用 Metadata Importer 解析。
- 校验字段和重复项。
- 生成导入预览。
- 用户确认后写入 Metadata Store。

## Server 接口

已实现接口：

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

真实 openGemini 元数据导入：

```json
{
  "url": "http://127.0.0.1:8091/getdata",
  "instanceId": "prod-og-1",
  "remark": "生产集群 A",
  "templateType": "opengemini",
  "filename": "opengemini-getdata.json"
}
```

解析规则：

- 用户输入 `instanceId` -> `instanceId` 和内部 `clusterId`
- 用户输入 `remark` -> instance `remark`，空值不保存，超 120 字符拒绝
- `ClusterID` -> `labels.sourceClusterId`
- `MetaNodes` -> `<instanceId>:meta-*` 节点
- `DataNodes` -> `<instanceId>:data-*` 节点
- `SqlNodes` -> `<instanceId>:sql-*` 节点
- `Databases`、`Term`、`Index`、`NumOfShards` 等写入 cluster labels
- `Databases` -> cluster `databases`，重点保留默认 RP、RP 参数、Measurements schema、ShardGroups
- `PtView` -> cluster `partitionViews`，重点保留数据库、PT ID、owner data node、状态、版本和 RGID
- 节点的 `Host`、`RPCAddr`、`TCPHost`、`GossipAddr`、`Status`、`Az` 等写入 node fields/labels
- `Shard.Owners` 和 `Index.Owners` -> PT ID
- `MstVersions` -> 逻辑表名到物理表名映射
- `IndexGroups` -> IndexGroup 和 Index 明细
- 原始 `/getdata` -> `rawSnapshot`

受保护接口继续使用：

```text
Authorization: Bearer <api-key>
```

## WEBUI 规划

已新增 Metadata 页面：

- 展示已确认导入的 Instance 列表。
- Instance 列表展示备注名并保持单行省略，Overview 展示完整备注字段。
- 按实例 ID 查询。
- 读取已存快照时使用 InstanceID，不再要求用户输入 ClusterID。
- 展示 DataNode 分栏容器，内部按 Database/DBPT 分组展示 ShardGroup/Shard、IndexGroup/Index。
- 支持拓扑筛选、异常高亮和实体详情面板。
- 展示 `PtView` 分区归属和状态。
- 展示 `Databases`、保留策略、表结构 schema 和 shard group 摘要。
- 展示产品、版本、环境、标签。
- 从真实元数据 URL 拉取并预览。
- 上传 JSON 文件或输入 JSON 文本并预览导入结果。
- 导入确认后显示成功/失败明细。

任务创建时可选择或输入：

- `instanceId`
- `nodeId`

这些字段进入 task context，后续用于日志分析和证据关联。只填写 `instanceId` 或 `nodeId` 时，Server 会从已确认 Metadata 自动推导关联 ID；显式 ID 与元数据关系冲突时拒绝创建任务。旧 `clusterId` 请求字段仍兼容，但 WebUI 不再暴露。

任务创建时固化 `workspaces/<task_id>/metadata_context.json`。快照包含归一化 instance、cluster、node、cluster nodes、产品、版本和环境。为控制大小和避免重复，cluster `rawSnapshot` 不写入任务快照。

## 本地运行方式

Metadata 作为 Rust Server 内部模块实现，随 Server 启动。

## 部署方式

Metadata Store 作为 Server 数据目录的一部分，当前使用本地 JSON 文件。

建议目录：

```text
data_dir/
  metadata/
    instances.json
    clusters.json
    imports/
```

## 验证方式

至少验证：

- 单实例查询。
- 已导入 Instance 列表查询。
- 集群节点查询。
- YAML/JSON 模板导入预览。
- WebUI JSON 文件上传和手动 JSON 文本导入预览。
- 导入确认。

## 上下游接口

上游：

- WEBUI 模板导入。
- 用户创建 task 时输入实例或集群信息。

下游：

- Server task context。
- Environment Collector 节点采集。
- Code Evidence 产品版本定位。
- Analysis Agent 的只读事实上下文。
