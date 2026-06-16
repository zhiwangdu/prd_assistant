# Environment Collector Spec

## 目标

Environment Collector 面向测试环境节点，通过 SSH/SCP 收集日志和环境信息，替代浏览器下载和本地上传链路。

## 当前状态

已实现通用 Remote Executor 基础框架，但尚未实现完整 Environment Collector：

- 执行机资产通过 `/api/executors` 管理，持久化到 `storage.data_dir/executors`。
- 白名单命令模板通过 `remote_execution.commands` 配置，`GET /api/executor-command-templates` 暴露给 WebUI。
- `POST /api/executor-runs` 创建 `taskKind=remote_command_run`，后台 `EXECUTE_REMOTE_COMMAND` phase 调用系统 `ssh` 二进制执行模板 argv。
- 结果写入 `workspaces/<task_id>/remote_command/{result.json,stdout.txt,stderr.txt}`，通过 `/api/executor-runs/:task_id/result` 查询。
- V2 clean-room Server 已提供等价 Remote Executor MVP：`/api/v2/executors`、`/api/v2/executor-command-templates`、`/api/v2/executor-runs`，executor 和 run 存入 SQLite，白名单模板来自环境变量，结果写入 `remote_runs/<run_id>/remote_command/{result.json,stdout.txt,stderr.txt}`。
- 当前未接入 Analysis Agent 的 `collect_environment` 审批动作，也未实现 SCP 文件拉取和多节点采集。

## 输入

- 测试环境节点配置
- SSH 用户和密钥环境变量
- 采集路径白名单
- 可选命令白名单
- 用户问题和软件版本
- 经批准的 `action_id`、环境和采集项选择

## 处理流程

```text
connect ssh
  -> collect files by scp
  -> run whitelisted commands
  -> store collected/
  -> generate environment_evidence.json
  -> hand off to Server pipeline
```

## 输出

```text
collected/
environment_evidence.json
remote_command/
  result.json
  stdout.txt
  stderr.txt
```

建议记录：

- hostname
- os/kernel
- process status
- disk/memory
- service logs
- collected file list
- command outputs
- action id、批准事件引用和采集时间

## 安全约束

- 只连接配置中的测试环境节点。
- SCP 路径必须在白名单内。
- SSH 命令必须在白名单内。
- 不支持任意远程 shell。
- WebUI 显式执行只允许选择白名单命令模板，不允许输入自由命令。
- 默认必须有有效批准记录，配置可对特定只读采集项另行放宽。
- Claude Code 不能直接执行 SSH/SCP，只能通过 LogAgent MCP 请求审批和结构化采集意图。

## 验收标准

- 配置节点可连通时能收集文件到 workspace。
- 非白名单路径和命令被拒绝。
- 采集失败保留错误原因。
- 拒绝动作不会执行连接，并能作为分析事件继续任务。
- Remote Executor smoke 可用 fake ssh 自动测试；真实 ECS smoke 使用已配置免密的 `root@112.74.50.120` 执行 `smoke_ls_root`。
- README 和 SPEC 在采集协议或配置变更时同步更新。
