# Chrome Extension 方案

## 职责

Chrome 插件负责识别日志下载，并把用户确认后的文件交给 Native Agent 或 Web 上传流程。

## MVP 流程

1. 浏览器正常下载文件。
2. 插件通过 `chrome.downloads.onChanged` 监听下载完成事件。
3. 根据 URL 前缀或文件后缀判断是否可能是日志。
4. 弹出确认：是否交给 LogAgent 分析。
5. 调用 Native Agent 的 `localhost` HTTP 接口，把下载文件附加到当前 LogAgent Session。

## 本地安装

1. 打开 `chrome://extensions`。
2. 开启 Developer mode。
3. 点击 Load unpacked。
4. 选择本目录 `chrome-extension`。
5. 在扩展 Options 中确认 Native Agent URL，默认是 `http://127.0.0.1:17321`。

## 运行前置条件

- Native Agent 已在本机启动。
- Native Agent 的 `allowed_dirs` 包含浏览器下载目录。
- Server 已启动，或 Native Agent 的 `server_base_url` 指向 ECS Server。
- Chrome 对下载文件路径可见。当前实现依赖 `chrome.downloads.search()` 返回的 `filename`。

## 本地验证

1. 启动 Server：

```bash
export LOGAGENT_NATIVE_API_KEY=dev-token
cargo run -p logagent-server -- --config examples/logagent.yaml
```

2. 启动 Native Agent：

```bash
export LOGAGENT_NATIVE_API_KEY=dev-token
cargo run -p logagent-native-agent -- --config examples/logagent.yaml
```

3. 在 Chrome 下载一个匹配后缀的文件，例如 `.log`、`.txt`、`.zip`、`.tar.gz`、`.tgz`、`.tar`。
4. Chrome notification 弹出后点击 `Send to LogAgent`。
5. 成功 notification 显示 `LogAgent session updated`。
6. 检查 Server 侧 Session 和 workspace：

```bash
find data/logagent -maxdepth 5 -type f | sort
```

## 部署方式

Chrome Extension 当前按开发者模式加载：

- `chrome://extensions`
- Developer mode
- Load unpacked
- 选择 `chrome-extension`

如果后续要给多人使用，需要打包并发布到 Chrome Web Store 或企业内部分发；发布前需要固定 extension 权限、图标资源和版本号。

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
  ".tgz",
  ".tar"
]
```

## Chrome API

MVP 使用 `chrome.downloads.onChanged` 判断下载完成，而不是 `onCreated`。

原因：

- `onCreated` 只表示下载任务创建，文件还不可用。
- `onChanged` 可观察 `state.current === "complete"`，此时再提交给 Native Agent。

示例：

```js
chrome.downloads.onChanged.addListener((delta) => {
  if (delta.state?.current !== "complete") {
    return
  }
  chrome.downloads.search({ id: delta.id }, (items) => {
    const item = items[0]
    // 判断文件名、URL，然后弹窗确认
  })
})
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
