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
- API Key 输入和 localStorage 保存。
- 一个或多个文件上传。
- 小文件 multipart 上传。
- 大文件 512 KiB 分片上传。
- 使用 `uploadIds` 创建批量任务。
- localStorage 记录最近任务。
- 查询 `/api/tasks/:task_id/artifacts`。
- 展示 manifest 文件清单。
- 展示 grep 命中。

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
```

受保护接口使用：

```text
Authorization: Bearer <api-key>
```

## 页面

- 导入：API Key、来源 URL、多文件选择、上传进度。
- 任务：浏览器本地最近任务。
- 证据：文件清单、grep 命中、原始 JSON。

## 约束

- 第一版不引入 Node 构建步骤。
- 不在前端持久化敏感分析结果，任务列表仅作为本机快速入口。
- API Key 保存在 localStorage，仅用于本地 MVP。

## 验收标准

- `GET /` 能返回页面。
- `node --check webui/app.js` 通过。
- 页面能上传一个或多个 sample.log、创建任务、显示 artifacts。
- README 和 SPEC 在页面、接口或部署方式变更时同步更新。
