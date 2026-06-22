# Native Agent

Native Agent 是可选本机导入桥。它接收 Chrome Extension 传来的本地文件路径，做目录、后缀和大小校验，然后上传到本地或远端 LogAgent Workbench。

## 职责

- 只监听 `127.0.0.1`。
- 接收 `POST /imports`。
- 维护当前 workspace/run 上下文的轻量状态。
- 校验 `allowed_dirs`、`allowed_suffixes`、`max_upload_bytes`。
- 小文件 multipart 上传，大文件分片上传。
- 返回 upload/run/workbench URL。

Native Agent 不执行文件内容，不读取浏览器 Cookie，不保存 Server API Key 原文。

## 本地运行

```bash
export LOGAGENT_NATIVE_API_KEY=dev-token
cargo run -p logagent-native-agent -- --config examples/logagent.yaml
```

健康检查：

```bash
curl http://127.0.0.1:17321/health
```

## 接口

```http
GET /health
POST /imports
GET /workspace/current
PUT /workspace/current
DELETE /workspace/current
```

## 配置

```yaml
native_agent:
  bind: 127.0.0.1:17321
  server_base_url: http://127.0.0.1:50992
  api_key_env: LOGAGENT_NATIVE_API_KEY
  allowed_dirs: ["~/Downloads"]
  allowed_suffixes: [".log", ".txt", ".zip", ".tar.gz", ".tgz", ".tar"]
  upload_chunk_bytes: 524288
```

## 验证

- 合法文件可上传到 LogAgent。
- 非 allowed_dirs 文件被拒绝。
- 超过大小限制文件被拒绝。
- 大文件使用分片上传。
