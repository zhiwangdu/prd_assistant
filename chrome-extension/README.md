# Chrome Extension

Chrome Extension 是可选导入桥。它监听浏览器下载完成事件，用户确认后把本地文件路径交给 Native Agent，再由 Native Agent 上传到本地 LogAgent Workbench。

## 职责

- 监听 Chrome 下载完成。
- 根据 URL 前缀和文件后缀过滤候选日志包。
- 弹出通知让用户确认。
- 调用 Native Agent `POST /imports`。

它不直接连接 LogAgent Server，不保存 API Key，不读取 Cookie 或 Authorization header。

## 本地安装

1. 打开 `chrome://extensions`。
2. 开启 Developer mode。
3. Load unpacked，选择 `chrome-extension/`。
4. Options 中确认 Native Agent URL，默认 `http://127.0.0.1:17321`。

## 默认后缀

```text
.log
.txt
.zip
.tar.gz
.tgz
.tar
```

## 验证

- Native Agent 已启动。
- 下载匹配文件后出现确认通知。
- 点击后 Native Agent 收到 `/imports`。
- WebUI Runs 或 Uploads 中能看到导入文件。
