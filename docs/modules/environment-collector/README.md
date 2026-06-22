# Environment Collector

Environment Collector 通过 Executor 模板做受控 SSH/SCP 采集。它是工具能力，不是远程运维平台。

## 职责

- 管理 executor。
- 管理命令模板和文件模板。
- 运行模板并保存 stdout/stderr/file artifacts。
- 高风险动作要求审批或显式用户触发。
