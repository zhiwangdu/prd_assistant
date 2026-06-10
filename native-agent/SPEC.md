# Native Agent Spec

## 目标

Native Agent 在本机接收 Chrome Extension 导入请求，校验本地文件后上传到 Server。

## 当前状态

已实现 Rust/Axum 服务：

- `GET /health`
- `POST /imports`
- 本地路径白名单校验
- 文件后缀校验
- 文件大小限制
- 小文件 multipart 上传
- 大文件分片上传
- 本地活动 Session 状态文件
- `/workspace/current` 读写/清理当前 Session
- 上传后附加到 Server Session；没有活动 Session 时自动创建 Session

## 输入

```json
{
  "filePath": "/Users/xxx/Downloads/redis.tar.gz",
  "filename": "redis.tar.gz",
  "sourceUrl": "https://logs.example/export/123"
}
```

## 输出

Native Agent 调用 Server：

- `POST /api/uploads`
- `POST /api/uploads/init`
- `POST /api/uploads/:upload_id/chunks?offset=<bytes>`
- `POST /api/uploads/:upload_id/complete`
- `POST /api/sessions`
- `POST /api/sessions/:session_id/uploads`

`POST /imports` 输出：

```json
{
  "uploadId": "upl_...",
  "sessionId": "sess_...",
  "taskId": null,
  "url": "http://server/sessions/sess_..."
}
```

## 配置

配置来源为 `logagent.yaml` 的 `native_agent` 和 `storage` 部分：

- `bind`
- `server_base_url`
- `api_key_env`
- `allowed_dirs`
- `allowed_suffixes`
- `request_timeout_seconds`
- `upload_chunk_bytes`
- `state_path`，默认 `~/.logagent/native-agent-state.json`
- `storage.max_upload_bytes`

## 安全约束

- 只允许上传 `allowed_dirs` 下的文件。
- 只允许上传配置后缀。
- API Key 只从环境变量读取。
- 不执行用户文件中的任何内容。

## 验收标准

- `/health` 返回 `{"status":"ok"}`。
- 合法文件能上传并附加到 Server Session，不自动创建分析 task。
- 合法文件能上传并附加到活动 Server Session。
- 无活动 Session 时能自动创建 `Native import <filename>` Session，并写入本地 state_path。
- `/workspace/current` 能读取、设置和清理活动 Session。
- 超出目录、后缀或大小限制的请求被拒绝。
- 大文件按 `upload_chunk_bytes` 分片上传。
- README 和 SPEC 在上传协议或配置变更时同步更新。
