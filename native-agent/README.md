# Native Agent 方案

## 技术选型

Native Agent 使用 Rust 实现。

原因：

- 单文件部署
- 跨平台方便
- 静态类型和内存安全适合本地常驻 Agent
- 文件路径校验、HTTP 上传、进程管理等能力成熟
- 后续可扩展本地文件扫描、SSH 诊断、离线模式

语言优先级：

```text
Rust -> C/C++ -> Go/Python/Java 等
```

## 职责

- 启动本地 HTTP Server。
- 接收 Chrome 插件提交的文件路径和元信息。
- 校验文件大小、后缀、路径合法性。
- 上传日志包到服务端。
- 维护本机活动 Session。
- 上传完成后把文件附加到活动 Session；没有活动 Session 时自动创建一个 `Native import ...` Session 并设为活动。
- 默认通过 `native_agent.server_api=v2` 指向 `server-v2`。
- 返回任务 URL 给插件或自动打开 WebUI。

## 本地接口

```http
POST /imports
GET /workspace/current
PUT /workspace/current
DELETE /workspace/current
Content-Type: application/json

{
  "filePath": "/Users/xxx/Downloads/redis.tar.gz",
  "filename": "redis.tar.gz",
  "sourceUrl": "https://logs.xxx.com/export/123"
}
```

## 本地运行

先启动 V2 Server，再启动 Native Agent。

```bash
./scripts/v2-local.sh start
export LOGAGENT_NATIVE_API_KEY=dev-token
cargo run -p logagent-native-agent -- --config examples/native-agent-v2-50993.yaml
```

健康检查：

```bash
curl http://127.0.0.1:17321/health
```

返回：

```json
{"status":"ok"}
```

本地导入测试：

```bash
curl -X POST http://127.0.0.1:17321/imports \
  -H 'Content-Type: application/json' \
  --data '{
    "filePath": "testing/fixtures/downloads/sample.log",
    "filename": "sample.log",
    "sourceUrl": "file://sample.log"
  }'
```

成功时返回：

```json
{
  "uploadId": "upl_...",
  "sessionId": "ws_...",
  "taskId": null,
  "url": "http://127.0.0.1:50993/sessions/ws_..."
}
```

## 部署方式

开发机或个人电脑上运行 Native Agent。

构建 release：

```bash
cargo build --release -p logagent-native-agent
```

运行：

```bash
LOGAGENT_NATIVE_API_KEY=<same-as-server> \
  ./target/release/logagent-native-agent --config /path/to/logagent.yaml
```

配置要求：

- `native_agent.bind` 建议保持 `127.0.0.1:17321`。
- `native_agent.server_base_url` 指向 V2 Server 地址，默认 `http://127.0.0.1:50993`。
- `native_agent.server_api` 支持 `v1` 和 `v2`，默认 `v2`；`v1` 仅作为对接外部旧服务的兼容模式保留。
- `native_agent.allowed_dirs` 包含浏览器下载目录。
- `storage.max_upload_bytes` 与 Server 保持一致或更小。
- `native_agent.upload_chunk_bytes` 控制分片上传大小，默认 512KB。
- `native_agent.state_path` 控制活动 Session 状态文件，默认 `~/.logagent/native-agent-state.json`。

大文件上传：

- 小于等于 `native_agent.upload_chunk_bytes` 的文件走普通 multipart。
- 大于该阈值的文件自动走分片上传：
  - V1: `POST /api/uploads/init`
  - V2: `POST /api/v2/sessions/{session_id}/uploads/init`
  - V1: `POST /api/uploads/{upload_id}/chunks?offset=...`
  - V2: `POST /api/v2/uploads/{upload_session_id}/chunks?offset=...`
  - V1: `POST /api/uploads/{upload_id}/complete`
  - V2: `POST /api/v2/uploads/{upload_session_id}/complete`
- 如果 ECS 前面有网关限制单请求大小，把 `upload_chunk_bytes` 配得小于网关限制，例如 `524288`。

macOS 可用 launchd 自启动；Linux 可用 systemd user service。MVP 阶段也可以手动运行。

## 服务端接口

Native Agent 从本地配置读取服务端地址和 API Key。API Key 不写死在代码中。

配置示例：

```yaml
native_agent:
  server_base_url: "http://logagent:50993"
  server_api: "v2"
  api_key_env: "LOGAGENT_NATIVE_API_KEY"
```

连接 V2 默认本机端口：

```bash
export LOGAGENT_NATIVE_API_KEY=dev-token
cargo run -p logagent-native-agent -- --config examples/native-agent-v2-50993.yaml
```

V1 兼容上传路径仍保留给外部旧服务：

```http
POST /api/uploads
Authorization: Bearer <api_key>
```

V2 上传会先确保活动 Session，然后直接使用 Session 作用域接口：

```http
POST /api/v2/sessions/:session_id/uploads
Authorization: Bearer <api_key>
```

V1 兼容附加到 Session：

```http
POST /api/sessions/:session_id/uploads
Authorization: Bearer <api_key>

{
  "uploadIds": ["upl_123"]
}
```

没有活动 Session 时先创建：

```http
POST /api/sessions
```

V2 使用对应的 `POST /api/v2/sessions`，返回的 `sessionId` 为 `ws_...`。

## 安全边界

- 只监听 `127.0.0.1`。
- 校验文件必须来自允许目录或用户确认范围。
- 不接触浏览器 Cookie。
- 不上传超限文件。
