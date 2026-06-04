# Metadata 方案

## 当前实现状态

Metadata 模块先规划框架，暂未实现代码。

模块目标是管理实例 ID 对应的业务和部署元数据、集群节点信息，并把这些信息提供给后续日志分析、环境采集、代码证据和 WEBUI 展示。

## 职责

负责：

- 维护实例 ID 到产品、版本、集群、节点、角色等信息的映射。
- 维护集群拓扑和节点清单。
- 支持按模板批量导入元数据。
- 为 Server task 提供 `instanceId` / `clusterId` 上下文。
- 为 WEBUI 提供元数据查询和展示。

不负责：

- 不直接采集测试环境信息；采集由 Environment Collector 负责。
- 不直接分析日志；分析由 Log Analyzer、Tool Runner 和 LLM Agent 负责。
- 不直接管理代码仓；代码版本证据由 Code Evidence 负责。

## 核心对象

实例：

- `instance_id`
- `cluster_id`
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

模板格式暂定，第一版只先定义导入能力边界。

建议支持：

- CSV
- YAML
- JSON

导入方式：

- WEBUI 上传模板文件。
- Server 调用 Metadata Importer 解析。
- 校验字段和重复项。
- 生成导入预览。
- 用户确认后写入 Metadata Store。

## Server 接口规划

后续接口建议：

```http
GET /api/metadata/instances/:instance_id
GET /api/metadata/clusters/:cluster_id
GET /api/metadata/clusters/:cluster_id/nodes
POST /api/metadata/imports
GET /api/metadata/imports/:import_id/preview
POST /api/metadata/imports/:import_id/confirm
```

受保护接口继续使用：

```text
Authorization: Bearer <api-key>
```

## WEBUI 规划

新增 Metadata 页面：

- 按实例 ID 查询。
- 按集群 ID 查询。
- 展示集群拓扑和节点角色。
- 展示产品、版本、环境、标签。
- 上传模板并预览导入结果。
- 导入确认后显示成功/失败明细。

任务创建时可选择或输入：

- `instanceId`
- `clusterId`
- `nodeId`

这些字段进入 task context，后续用于日志分析和证据关联。

## 本地运行方式

当前未实现独立服务。后续作为 Rust Server 内部模块实现，随 Server 启动。

## 部署方式

Metadata Store 作为 Server 数据目录的一部分，MVP 可先使用本地 JSON/SQLite。

建议目录：

```text
data_dir/
  metadata/
    instances.json
    clusters.json
    imports/
```

## 验证方式

第一版实现后至少验证：

- 单实例查询。
- 集群节点查询。
- 模板导入预览。
- 导入确认。
- task 创建时能关联 `instanceId` / `clusterId`。

## 上下游接口

上游：

- WEBUI 模板导入。
- 用户创建 task 时输入实例或集群信息。

下游：

- Server task context。
- Environment Collector 节点采集。
- Code Evidence 产品版本定位。
- LLM Agent 证据上下文。
