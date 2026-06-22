# Config

配置目标是本地单机开箱即用。默认只需要 bind、data_dir 和 API Key；高级工具按需启用。

## 原则

- secret 只引用环境变量。
- 默认关闭高风险能力：Fetch、Executor、Code Evidence 写操作。
- 工具目录和数据目录可通过环境变量展开。
- LLM/Agent 配置是可选项。
