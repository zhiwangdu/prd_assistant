# Metadata 方案

## 当前实现状态

Metadata 模块已完成基础 Rust Server 实现。

模块目标是管理实例 ID 对应的业务和部署元数据、集群节点信息，并把这些信息提供给后续日志分析、环境采集、代码证据和 WEBUI 展示。

产品入口上，Metadata 已纳入 System Context 和 Domain Adapter：现有 Metadata Store/API 继续保持专业拓扑模型和诊断能力，System Context 通过只读 `metadata_instance` adapter 把已导入 Instance 摘要纳入通用背景资源目录，并在 task 创建时固化到 `system_context.json`；`opengemini_influxdb` Domain Adapter 把这些拓扑和 shard/index 线索作为专项诊断证据。

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
- 导入确认后写入 metadata store，并支持按已导入 Instance 列表查看；重复导入同一个 `instanceId` 会按新快照覆盖，并清理旧 cluster/node 残留。
- 已导入 openGemini Instance 可用保存的 `rawSnapshot` 手动刷新归一化快照，也可删除单条 Instance 及其对应 cluster/node 记录。
- WEBUI Metadata 页面支持实时 URL 加载、JSON 文件上传和手动 JSON 文本三种导入方式。
- task 创建时关联 `instanceId` / `nodeId`；`clusterId` 已从用户入口弃用，仅作为兼容字段保留。
- 在 task workspace 原子写入 `metadata_context.json`。
- 将产品、版本、环境和拓扑数量摘要提供给 Claude Code MCP resources；完整 Metadata 不再进入 `analysis_package.json` 或任务 MCP 默认 resource。
- 任务 stdio MCP 新增 `logagent.query_metadata`，支持按 section/filter/limit/cursor 读取 bounded slice，并写入 `metadata_slices/<stable_id>.json` 审计背景上下文；`logagent.get_metadata_topology` 作为兼容 alias 返回 outline。
- 任务 stdio MCP 和只读 HTTP MCP 新增 `logagent.get_metadata_field_types`，支持从指定 `instanceId/database/measurement` 查询一个、多个或全部 field 的类型；省略 RP 时使用 DB 默认 RP。
- 任务 stdio MCP 和只读 HTTP MCP 新增 `logagent.get_metadata_tag_fields`，复用同一 instance/database/measurement/RP 定位逻辑，但只返回 `typ=6` / `typeLabel=Tag` 的字段，不支持 `field` 参数。
- 只读 HTTP MCP 通过 `logagent://metadata/instances`、`logagent://metadata/instances/{instance_id}/snapshot`、`logagent.list_metadata_instances`、`logagent.get_metadata_snapshot`、`logagent.get_metadata_field_types` 和 `logagent.get_metadata_tag_fields` 暴露已导入 Metadata，供个人本地 Claude Code 读取；`logagent.get_metadata_snapshot` 响应保留 V2 顶层 snapshot 字段并补齐 Rust/V1 `snapshot` 包装。Python V2 同时接受 `logagent-v2://...` 旧 alias，该入口不导入或修改 Metadata。

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
- 不直接分析日志；调查由 Analysis Orchestrator 编排，推理由 Claude Code 负责。
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

确认导入时以 `instanceId` 作为覆盖边界。重复导入同一个 Instance 会先清理该实例旧的 cluster/node 快照，再写入本次导入结果，v1 不保留历史版本。

## Server 接口

已实现接口：

```http
GET /api/metadata/instances
GET /api/metadata/instances/:instance_id
DELETE /api/metadata/instances/:instance_id
GET /api/metadata/instances/:instance_id/snapshot
POST /api/metadata/instances/:instance_id/refresh
GET /api/metadata/clusters/:cluster_id
GET /api/metadata/clusters/:cluster_id/nodes
POST /api/metadata/snapshots/fetch
POST /api/metadata/imports
POST /api/metadata/imports/fetch
GET /api/metadata/imports/:import_id/preview
POST /api/metadata/imports/:import_id/confirm
```

Python V2 clean-room Server 已提供等价刷新能力：

```http
POST /api/v2/metadata/instances/:instance_id/refresh
```

V2 刷新从 SQLite 中已保存的 `raw_json` 重新运行当前 normalizer 并覆盖
`metadata_instances.snapshot_json`，不会重新请求原始 URL。

Python V2 task MCP 也提供 Rust V1 兼容的
`logagent.get_metadata_topology` 和 `logagent.query_metadata`。前者返回当前
run 的 Metadata outline、section counts 和查询提示；后者从当前 run 自动选中的
Metadata instance snapshot 中按 `section`、section-specific filters、`limit`
和 `cursor` 读取 bounded slice，并写入 background-only `metadata_slice`
evidence。Readonly MCP 仍只暴露全局 imported instance/snapshot/field/tag 查询，
不暴露 run-scoped `query_metadata`。

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
- `Measurement.Schema` 同时兼容 `{ "field": <typeCode> }` 简写和 `{ "field": { "Typ": <typeCode>, "EndTime": ... } }` 对象结构；每个字段优先按简写类型码解析，不能解析时再降级读取 `Typ` / `Type` / `type` / `typ` 和 `EndTime`
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
- Instance 列表展示备注名并保持单行省略，支持向左收缩/展开，Overview 展示完整备注字段。
- Instance 列表支持删除单条 metadata；主导入区支持用已存 Raw JSON 手动刷新当前 Instance。
- 按实例 ID 查询。
- 读取已存快照时使用 InstanceID，不再要求用户输入 ClusterID。
- Metadata Explorer 合并原 Topology 和 Databases，提供 `Node / DBPT / Shards` 与 `DB / RP / Shards / Indexes` 两个视角。
- `Node / DBPT / Shards` 不再渲染 Graph，改为按 `Database -> DataNode -> DBPT -> Shards` 级联展开，Shard 行展示所属 RP、ShardGroup、time range、Owners、IndexID 和 Index 状态信息。
- Explorer 支持拓扑筛选、异常高亮、Shard 行/Index 信息显隐和 DBPT 聚合详情面板。
- 展示 `PtView` 分区归属和状态。
- `DB / RP / Shards / Indexes` 视角按 `Database -> RP -> ShardGroup/IndexGroup -> Shard/Index` 级联展开。
- Schemas 页面默认选择第一个非 `_internal` DB 及其第一个 RP，RP 跟随 DB 联动，Measurement/field 用于缩小表结构结果，field type 按 openGemini 枚举码解析为 `0 Unknown`、`1 Integer`、`2 Unsigned`、`3 Float`、`4 String`、`5 Boolean`、`6 Tag`、`7 Unknown`。
- MCP `logagent.get_metadata_field_types` 使用同一类型映射：必填 `instanceId`、`database`、`measurement`，可选 `retentionPolicy` 和 `field`；`field` 支持字符串或字符串数组，省略时返回 measurement 下所有 field 和对应类型；省略 RP 且使用 DB 默认 RP 时返回 `defaultRetentionPolicyUsed=true`。
- MCP `logagent.get_metadata_tag_fields` 使用同一类型映射和默认 RP 规则：必填 `instanceId`、`database`、`measurement`，可选 `retentionPolicy`，只返回 Tag 字段，返回结构与 field types 一致但 `missingFields=[]`。Task MCP 响应写入 `metadata_slices/field_types_<stable_id>.json` / `metadata_slices/tag_fields_<stable_id>.json` 并返回 `artifactPath`、`backgroundRef`、`evidenceRefs`、`finalEvidenceAllowed=false` 和 Rust/V1 `result` 包装；readonly MCP 只返回查询结果和 `result` 包装，不写 task artifact。
- 节点、分区、Explorer 和 Schemas 明细表使用局部滚动和固定表头，便于浏览大量行时识别字段含义。
- Raw JSON 页面按需展开原始 JSON，不在进入页面时全量 stringify 大对象。
- 展示产品、版本、环境、标签。
- 从真实元数据 URL 拉取并预览。
- 上传 JSON 文件或输入 JSON 文本并预览导入结果。
- 导入确认后显示成功/失败明细。

任务创建时可选择或输入：

- `instanceId`
- `nodeId`

这些字段进入 task context，后续用于日志分析和证据关联。只填写 `instanceId` 或 `nodeId` 时，Server 会从已确认 Metadata 自动推导关联 ID；显式 ID 与元数据关系冲突时拒绝创建任务。旧 `clusterId` 请求字段仍兼容，但 WebUI 不再暴露。

任务创建时固化 `workspaces/<task_id>/metadata_context.json`。快照包含归一化 instance、cluster、node、cluster nodes、产品、版本和环境。为控制大小和避免重复，cluster `rawSnapshot` 不写入任务快照。

Claude Code 初始上下文不再直接获得完整 `metadata_context.json`。`analysis_package.json` 的 `evidence.metadataContextOutline` 只包含 `metadataContextPath`、选中的 instance/cluster/node、产品/版本/环境、各 section 的 count/available 和可用查询入口。任务 MCP `resources/read metadata_context` 返回同样 outline；细节必须通过 `logagent.query_metadata` 读取，支持 `overview`、`nodes`、`databases`、`retention_policies`、`measurements`、`fields`、`shard_groups`、`shards`、`index_groups`、`indexes`、`partition_views`。需要针对指定已导入 instance 精确查询 field 类型时使用 `logagent.get_metadata_field_types`，它写入 `metadata_slices/field_types_<stable_id>.json` 并返回 `typeLabel`；只需要该 measurement 的 Tag 字段时使用 `logagent.get_metadata_tag_fields`，它写入 `metadata_slices/tag_fields_<stable_id>.json`。slice 是背景上下文，不作为最终 evidence ref。

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
- Analysis Orchestrator 的只读事实上下文。
