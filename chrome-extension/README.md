# Chrome Extension 方案

## 职责

Chrome 插件负责识别日志下载，并把用户确认后的文件交给 Native Agent 或 Web 上传流程。

## MVP 流程

1. 浏览器正常下载文件。
2. 插件监听下载完成事件。
3. 根据 URL 前缀或文件后缀判断是否可能是日志。
4. 弹出确认：是否交给 LogAgent 分析。
5. 调用 Native Agent 的 `localhost` HTTP 接口。

## 匹配规则

```js
const URL_PREFIXES = [
  "https://xxx/download/",
  "https://logs.xxx.com/export/"
]

const FILE_SUFFIXES = [
  ".log",
  ".txt",
  ".zip",
  ".tar.gz",
  ".tgz"
]
```

## Native Agent 接口

```http
POST http://127.0.0.1:<port>/imports
Content-Type: application/json

{
  "filePath": "/Users/xxx/Downloads/redis.tar.gz",
  "filename": "redis.tar.gz",
  "sourceUrl": "https://logs.xxx.com/export/123"
}
```

## 安全边界

- 插件不把 Cookie、Authorization、session token 传给 Native Agent。
- 第一版优先让浏览器完成下载，Native Agent 只处理已下载文件。
- 用户必须确认后才上传。

