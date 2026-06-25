# Security

安全目标是本机工具平台不越权、不泄密、可审计。

## 边界

- API Key 鉴权。
- Tool allowlist。
- Upload 解压路径安全。
- Analyzer 输入文件和 dev_selftest profile allowlist。
- Inline Docker test target 校验。
- Artifact 逻辑路径。
- Secret 脱敏。
- MCP Origin 校验（直连场景）。
