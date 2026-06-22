# Native Agent Spec

## 目标

为 Chrome Extension 和本机脚本提供受控文件导入入口。

## 输入

```json
{
  "filePath": "/Users/me/Downloads/logs.tar.gz",
  "filename": "logs.tar.gz",
  "sourceUrl": "https://example/download/logs.tar.gz"
}
```

## 输出

Native Agent 调用 Server 上传 API，并返回：

```json
{
  "uploadId": "upl_...",
  "url": "http://127.0.0.1:50992/"
}
```

## 安全约束

- 只监听 loopback。
- 只允许上传 `allowed_dirs` 内文件。
- 只允许配置后缀。
- API Key 从环境变量读取。
- 不执行文件内容。

## 验收

- `/health` 返回 ok。
- `/imports` 能上传合法文件。
- `/workspace/current` 能读写当前上下文。
- 非法路径、后缀、大小被拒绝。
- README/SPEC 在上传协议或配置变化时同步更新。
