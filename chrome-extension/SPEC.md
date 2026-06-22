# Chrome Extension Spec

## 目标

把浏览器下载文件安全交给本机 Native Agent，用于 LogAgent 本地工具工作台。

## 输入

- Chrome downloads record。
- Options：`agentBaseUrl`、`urlPrefixes`、`fileSuffixes`。

## 输出

```http
POST /imports
Content-Type: application/json

{
  "filePath": "/Users/me/Downloads/package.tar.gz",
  "filename": "package.tar.gz",
  "sourceUrl": "https://example/download/package.tar.gz"
}
```

## 安全约束

- 用户点击确认后才导入。
- 不传 Cookie、Authorization header 或网页内容。
- 路径合法性由 Native Agent `allowed_dirs` 再校验。
- 插件不保存 LogAgent API Key。

## 验收

- Options 保存后影响后续匹配。
- 未匹配下载不触发通知。
- 匹配下载点击后 Native Agent 收到请求。
- README 和 SPEC 在接口或匹配规则变化时同步更新。
