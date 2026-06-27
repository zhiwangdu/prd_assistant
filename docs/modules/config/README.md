# Config

配置目标是让两模块本地工作台开箱即用。默认只需要 bind、data_dir、API Key 和
MCP 开关；日志 analyzer、dev_selftest profile 和命令模板按需启用。

## 原则

- secret 只引用环境变量。
- 工具二进制、data 目录、dev_selftest 路径可通过环境变量展开。
- 日志 analyzer 默认可出现在 catalog 中，但外部二进制未配置时保持不可运行。
- `dev_selftest.enabled=false` 是关闭态，占位 docker/build 配置不得阻断 Server 启动。
- `remote_execution` 只保留 dev_selftest 使用的命令模板，不再表示远程 executor 服务。
- dev_selftest demo 配置可由 probe 脚本生成：openGemini 使用 host build profile，InfluxDB 使用 Docker-backed build profile 构建 Linux `influxd`。
- 不配置 LLM、Agent、Fetch、Metadata、Case、Skills 或 SSH/SCP 相关项。
