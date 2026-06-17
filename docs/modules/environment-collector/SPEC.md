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
- V2 command template descriptor 与 Rust/V1 对齐：`enabled` 同时反映全局 remote execution 开关和模板自身开关，`timeoutSeconds` 总是模板覆盖值或默认远程命令 timeout。
- V2 已接入 `collect_environment` 审批后的 evidence 闭环：批准 input 带
  `executorId` 和 `commandId` 时，通过 Remote Executor 执行该白名单命令，
  完成后写入 `environment_evidence/<action_id>/result.json`，状态为
  `COLLECTED` 或 `REMOTE_FAILED`，并重新排队同一个 analysis run；远程目标
  无效时写入 `REMOTE_REJECTED` evidence；没有远程目标时仍写入
  V1-compatible `MOCK` evidence。
- 当前未实现 SCP 文件拉取、多节点采集和 Agent 自动选择 executor/command。

## 输入

- 测试环境节点配置
- SSH 用户和密钥环境变量
- 采集路径白名单
- 可选命令白名单
- 用户问题和软件版本
- 经批准的 `action_id`、环境和采集项选择
- V2 单命令采集要求批准 input 包含已存在的 `executorId` 和已配置的
  `commandId`
- V2 SSH executable 来自 `LOGAGENT_V2_REMOTE_SSH_COMMAND`，默认
  `/usr/bin/ssh`；启用 remote execution 时必须解析为绝对路径
- V2 host key policy 来自 `LOGAGENT_V2_REMOTE_HOST_KEY_POLICY`，只允许
  `accept-new`、`strict` 或 `no`

## 处理流程

```text
connect ssh
  -> optionally collect files by scp
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
- V2 批准 `collect_environment` 后可通过 fake ssh 执行单个白名单 remote
  command，生成 background-only `environment_evidence`，并重新排队原 analysis
  run。
- README 和 SPEC 在采集协议或配置变更时同步更新。
