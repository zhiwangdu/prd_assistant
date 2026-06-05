# WebUI

## 当前实现状态

第一版 WEBUI 已实现为 Rust Server 静态托管的轻量页面，不需要单独 Node/Vite 构建。

当前能力：

- 检查 Server `/health`
- 输入 API Key
- 上传一个或多个日志文件
- 大文件按 512 KiB 分片上传
- 小文件直接 multipart 上传
- 使用 `uploadIds` 创建批量任务 `/api/tasks`
- 查询任务产物 `/api/tasks/:task_id/artifacts`
- 展示 `manifest.json` 文件清单
- 展示 `grep_results.json` grep 命中
- 在浏览器 localStorage 中保留最近任务
- 按实例 ID 查询元数据。
- 按集群 ID 展示节点列表。
- 从 `http://127.0.0.1:8091/getdata` 拉取真实 openGemini 元数据并预览。
- 输入 YAML/JSON 模板并预览导入结果。
- 确认导入后写入 Server metadata store。
- task 创建时关联 `instanceId`、`clusterId`、`nodeId`。

## 文件结构

```text
webui/
  index.html
  styles.css
  app.js
```

## 本地运行方式

从项目根目录启动 Server：

```bash
export LOGAGENT_NATIVE_API_KEY=dev-token
cargo run -p logagent-server -- --config examples/server-test.yaml
```

浏览器打开：

```text
http://127.0.0.1:50992/
```

如果使用 `examples/logagent.yaml`，默认地址是：

```text
http://127.0.0.1:8080/
```

## 配置项

WEBUI 直接使用当前页面同源 Server API，不需要单独配置后端地址。

需要在页面里输入 API Key，对应 Server 配置中的 `auth.api_keys[].value_env`，本地示例为：

```bash
LOGAGENT_NATIVE_API_KEY=dev-token
```

## 部署方式

生产部署时把 `webui/` 目录放在 Server 进程工作目录下。Rust Server 会用 `ServeDir("webui")` 托管静态资源：

```text
GET /              -> webui/index.html
GET /styles.css    -> webui/styles.css
GET /app.js        -> webui/app.js
```

## 健康检查和验证方式

```bash
curl http://127.0.0.1:50992/health
```

页面验证：

1. 打开 WEBUI。
2. 输入 API Key。
3. 选择一个或多个 `.log`、`.txt`、`.zip`、`.tar.gz`、`.tgz` 或 `.tar`。
4. 点击上传并创建任务。
5. 查看证据链里的文件清单和 grep 命中。

## 接口约定

WEBUI 调用的受保护接口都需要：

```text
Authorization: Bearer <api-key>
```

接口：

- `POST /api/uploads`
- `POST /api/uploads/batch`
- `POST /api/uploads/init`
- `POST /api/uploads/:upload_id/chunks?offset=<bytes>`
- `POST /api/uploads/:upload_id/complete`
- `POST /api/tasks`
- `GET /api/tasks/:task_id/artifacts`

- `GET /api/metadata/instances/:instance_id`
- `GET /api/metadata/clusters/:cluster_id`
- `GET /api/metadata/clusters/:cluster_id/nodes`
- `POST /api/metadata/imports`
- `POST /api/metadata/imports/fetch`
- `GET /api/metadata/imports/:import_id/preview`
- `POST /api/metadata/imports/:import_id/confirm`

## 后续范围

下一步再补：

- Server 持久化任务列表
- 任务状态流转
- task 创建时关联 Metadata
- 产品版本和代码 ref 输入
- 测试环境采集入口
- 外部工具结果展示
- Case 库页面

## 交互原则

- 任务详情页要优先呈现证据链，而不是只展示 LLM 结论。
- 工具结果、代码证据、环境证据应可折叠查看。
- Case 确认前允许人工修改标题、现象、根因和解决方案。
