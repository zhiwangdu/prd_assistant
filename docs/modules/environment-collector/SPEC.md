# Environment Collector Spec

## Requirements

- 禁止自由 shell 输入。
- executor、command、file template 都必须配置化。
- SCP 有路径 allowlist 和大小上限。
- SSH 私钥不由 LogAgent 保存。

## Acceptance

- fake ssh 自动测试通过。
- WebUI 可展示 result/stdout/stderr artifacts。
