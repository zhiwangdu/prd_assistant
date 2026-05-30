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
- 创建分析任务。
- 返回任务 URL 给插件或自动打开 WebUI。

## 本地接口

```http
POST /imports
Content-Type: application/json

{
  "filePath": "/Users/xxx/Downloads/redis.tar.gz",
  "filename": "redis.tar.gz",
  "sourceUrl": "https://logs.xxx.com/export/123"
}
```

## 服务端接口

Native Agent 从本地配置读取服务端地址和 API Key。API Key 不写死在代码中。

配置示例：

```yaml
native_agent:
  server_base_url: "http://logagent:8080"
  api_key_env: "LOGAGENT_NATIVE_API_KEY"
```

上传：

```http
POST /api/uploads
Authorization: Bearer <api_key>
```

创建任务：

```http
POST /api/tasks
Authorization: Bearer <api_key>

{
  "uploadId": "upl_123"
}
```

返回：

```json
{
  "taskId": "task_456",
  "url": "http://logagent/tasks/task_456"
}
```

## 安全边界

- 只监听 `127.0.0.1`。
- 校验文件必须来自允许目录或用户确认范围。
- 不接触浏览器 Cookie。
- 不上传超限文件。
