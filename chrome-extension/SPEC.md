# Chrome Extension Spec

## 目标

识别浏览器下载完成事件，把符合规则的本地文件交给 Native Agent 导入。

## 当前状态

已实现 Manifest V3 插件：

- `chrome.downloads.onChanged` 监听下载完成。
- 根据 URL 前缀和文件后缀过滤。
- 弹出 Chrome notification，由用户点击确认发送。
- 调用 Native Agent `POST /imports`。
- Options 页面支持配置 Agent 地址、URL 前缀和文件后缀。

## 输入

- Chrome 下载记录：`id`、`url`、`filename`、`state`。
- Options 配置：
  - `agentBaseUrl`
  - `urlPrefixes`
  - `fileSuffixes`

## 输出

Native Agent 请求：

```http
POST /imports
Content-Type: application/json

{
  "filePath": "/Users/xxx/Downloads/redis.tar.gz",
  "filename": "redis.tar.gz",
  "sourceUrl": "https://logs.example/export/123"
}
```

## 匹配规则

下载 URL 必须命中配置的 URL 前缀，文件名必须命中配置后缀。

默认后缀：

- `.log`
- `.txt`
- `.zip`
- `.tar.gz`
- `.tgz`
- `.tar`

## 安全约束

- 插件只把本地路径交给 Native Agent，不直接上传到远端 Server。
- 是否允许读取该路径由 Native Agent 的 `allowed_dirs` 决定。
- 插件不保存 API Key。

## 验收标准

- Options 配置保存后立即影响后续下载匹配。
- 未匹配 URL 或后缀的下载不会触发导入通知。
- 点击通知后 Native Agent 收到 `/imports` 请求。
- README 和 SPEC 在匹配规则或接口变更时同步更新。
