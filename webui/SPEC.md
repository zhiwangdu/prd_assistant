# WebUI Spec

## 目标

WEBUI 提供手动上传、任务查看和证据浏览入口，第一版直接由 Rust Server 静态托管。

## 当前状态

已实现无构建前端：

- `webui/index.html`
- `webui/styles.css`
- `webui/app.js`

## 当前功能

- 健康检查。
- 顶部固定 API Key 输入和 localStorage 保存。
- 一个或多个文件上传。
- 小文件 multipart 上传。
- 大文件 512 KiB 分片上传。
- 使用 `uploadIds` 创建批量任务。
- localStorage 记录最近任务。
- 查询 `/api/tasks/:task_id/artifacts`。
- 展示 manifest 文件清单。
- 展示 grep 命中。
- Metadata 查询、导入预览和确认。
- openGemini `/getdata` URL 拉取和归一化预览。
- 集群查询重点展示 `PtView` 分区状态和 `Databases` 库表/RP/shard 摘要。

## API

WEBUI 同源调用 Server：

```http
GET /health
POST /api/uploads
POST /api/uploads/batch
POST /api/uploads/init
POST /api/uploads/:upload_id/chunks?offset=<bytes>
POST /api/uploads/:upload_id/complete
POST /api/tasks
GET /api/tasks/:task_id/artifacts
GET /api/metadata/instances/:instance_id
GET /api/metadata/clusters/:cluster_id
GET /api/metadata/clusters/:cluster_id/nodes
POST /api/metadata/imports
POST /api/metadata/imports/fetch
GET /api/metadata/imports/:import_id/preview
POST /api/metadata/imports/:import_id/confirm
```

受保护接口使用：

```text
Authorization: Bearer <api-key>
```

## 页面

- 顶部连接区：API Key、健康检查。
- 导入：来源 URL、多文件选择、上传进度。
- 任务：浏览器本地最近任务。
- 证据：文件清单、grep 命中、原始 JSON。
- Metadata：展示实例 ID、集群节点、`PtView`、`Databases`、真实元数据拉取、模板导入预览和确认。

## 约束

- 第一版不引入 Node 构建步骤。
- 不在前端持久化敏感分析结果，任务列表仅作为本机快速入口。
- API Key 保存在 localStorage，仅用于本地 MVP。

## 验收标准

- `GET /` 能返回页面。
- `node --check webui/app.js` 通过。
- 页面能上传一个或多个 sample.log、创建任务、显示 artifacts。
- Metadata 页面支持按实例 ID 查询、按集群查看节点、模板导入预览和确认。
- Metadata 页面支持通过 Server 拉取 `127.0.0.1:8091/getdata`，避免浏览器 CORS 限制。
- Metadata 页面能直接展示 openGemini `PtView` 的 PT owner/status 和 `Databases` 的 RP、schema、ShardGroups。
- README 和 SPEC 在页面、接口或部署方式变更时同步更新。
