# Environment Collector Spec

## 目标

Environment Collector 面向测试环境节点，通过 SSH/SCP 收集日志和环境信息，替代浏览器下载和本地上传链路。

## 当前状态

已实现通用 Remote Executor 基础框架、V2 审批后的单目标/结构化批量环境采集、
Agent 可见候选摘要、单 executor 自动补齐，以及多 executor / 多模板场景的确定性唯一 hint 匹配；V2 已内置通用、openGemini、Cassandra 和 RocksDB 基础只读环境模板：

- 执行机资产通过 `/api/executors` 管理，持久化到 `storage.data_dir/executors`。
- 白名单命令模板通过 `remote_execution.commands` 配置，`GET /api/executor-command-templates` 暴露给 WebUI。
- V2 未设置 `LOGAGENT_V2_REMOTE_COMMANDS_JSON` 时默认暴露 `smoke_ls_root`、
  `system_uname`、`uptime_load`、`disk_usage`、`memory_usage`、
  `process_overview`、`network_listeners`、`opengemini_processes`、
  `opengemini_config_dirs`、`opengemini_log_dirs` 和
  `opengemini_data_dirs`、`cassandra_processes`、`cassandra_config_dirs`、
  `cassandra_log_dirs`、`cassandra_data_dirs`、`rocksdb_data_dirs`、
  `rocksdb_wal_dirs` 和 `rocksdb_log_dirs` 只读命令模板；产品模板只使用固定
  进程名和常见目录候选，不允许 shell 管道、重定向、glob 或用户输入 argv；
  显式设置该环境变量会替换整套默认模板。
- `POST /api/executor-runs` 创建 `taskKind=remote_command_run`，后台 `EXECUTE_REMOTE_COMMAND` phase 调用系统 `ssh` 二进制执行模板 argv。
- 结果写入 `workspaces/<task_id>/remote_command/{result.json,stdout.txt,stderr.txt}`，通过 `/api/executor-runs/:task_id/result` 查询。
- V2 clean-room Server 已提供等价 Remote Executor MVP：`/api/v2/executors`、`/api/v2/executor-command-templates`、`/api/v2/executor-file-templates`、`/api/v2/executor-runs`，executor 和 run 存入 SQLite，白名单模板来自环境变量，create/list/get 响应补齐 Rust/V1 TaskResponse-compatible 顶层 `taskId`、`runId`、`url`、`taskKind=remote_command_run`、`sessionId=null`、`analysisMode=diagnose` 和 `analysisLanguage=zh-CN`，命令结果写入 `remote_runs/<run_id>/remote_command/{result.json,stdout.txt,stderr.txt}`，并通过 `/api/v2/executor-runs/:run_id/files/result|stdout|stderr` 提供受保护下载。
- V2 command template descriptor 与 Rust/V1 对齐：`enabled` 同时反映全局 remote execution 开关和模板自身开关，`timeoutSeconds` 总是模板覆盖值或默认远程命令 timeout。
- V2 command template id 与 Rust/V1 对齐：只允许非空 ASCII 字母、数字、`_` 和 `-`。
- V2 command template argv 与 Rust/V1 对齐：加载时逐项 trim、丢弃空字符串，归一化后仍必须非空。
- V2 已接入 `collect_environment` 审批后的 evidence 闭环：批准 input 带
  `executorId` 且只选择 `commandId` 或 `fileId` 之一时，通过 Remote Executor
  执行白名单命令，或通过 `LOGAGENT_V2_REMOTE_SCP_COMMAND` 按白名单文件模板
  SCP 拉取单个有大小上限的远程文件；如果只有一个已启用 executor，批准
  input 可只提供 `commandId` 或 `fileId`，Server 自动补齐 executor。Provider
  归一化后的 action 也可把目标字段放在 payload 顶层或 `environmentInput` /
  `remoteInput` 中。多 executor 或多模板场景下，批准 input 可使用
  `target` / `executor` / `node` / `host` 类 hint 和 `template` / `command` /
  `file` 类 hint；Server 只在它们唯一匹配已启用 executor 和 command/file
  template 时调度，匹配不到或有歧义时写入 `REMOTE_REJECTED` evidence，不执行
  SSH/SCP。批准 input 也可携带最多 20 个 `targets[]` /
  `remoteTargets[]` 批量目标；每个目标必须指向已启用 executor，或可通过
  唯一 executor 规则或唯一 hint 补齐，并选择或唯一 hint 一个白名单
  command/file 模板。单目标完成后写入
  `environment_evidence/<action_id>/result.json`；批量目标使用
  `environment:<action_id>:<index>` 幂等键排队，等全部 remote run 终态后写入
  一个聚合 result，状态为 `COLLECTED`、`PARTIALLY_COLLECTED` 或
  `REMOTE_FAILED`，并重新排队同一个 analysis run。远程目标无效时写入
  `REMOTE_REJECTED` evidence；没有远程目标时仍写入 V1-compatible `MOCK`
  evidence。远程采集完成后，V2 还会把 `remote_result.json`、`stdout.txt`、
  `stderr.txt` 和可选 `collected_file.bin` 注册为当前 run 的 support
  artifacts；批量模式使用
  `environment_evidence/<action_id>/targets/<index>/...` 逻辑路径，供 artifact
  聚合和任务 MCP `artifact_index` 审计。
- V2 `collect_environment` 批准请求可携带 decision-time `input` override；
  Server 必须先把该 input 写回 action payload，再按 executor/command 调度
  Remote Executor 或回退 MOCK evidence。
- V2 WebUI 在 `collect_environment` 审批卡片中必须同时加载
  `/api/v2/executors`、`/api/v2/executor-command-templates` 和
  `/api/v2/executor-file-templates`；选择远程目标时只能提交 executor 加
  `commandId` 或 `fileId` 之一，留空则保留 MOCK evidence 路径。
- V2 `analysis_package.environmentCollection` 必须暴露已启用 executor、
  command/file 模板、单 executor 推断规则和多 executor 唯一 hint 规则，供
  Agent 构造结构化 `collect_environment` 审批请求。当前批量采集仍需要审批
  输入显式提供 `targets[]`；openGemini、Cassandra 和 RocksDB 基础环境模板
  已内置。

## 输入

- 测试环境节点配置
- SSH 用户和密钥环境变量
- 采集路径白名单；V2 当前通过 `LOGAGENT_V2_REMOTE_FILES_JSON` 配置单文件模板
- 可选命令白名单
- 用户问题和软件版本
- 经批准的 `action_id`、环境和采集项选择
- V2 远程采集要求批准 input 包含已存在的 `executorId`，并且只选择已配置的
  `commandId` 或 `fileId` 之一；如果仅有一个启用 executor，单目标 input 可省略
  `executorId`；如果有多个启用 executor/template，单目标 input 可用能唯一匹配
  已启用候选的 `target` / `executor` / `node` / `host` hint 和 `template` /
  `command` / `file` hint
- V2 批量采集要求批准 input 包含 `targets[]` / `remoteTargets[]`，数组长度为
  1..20；每个 target 均包含已存在的 `executorId`，或可继承父级 executor /
  使用唯一 executor 推断 / 唯一 hint 匹配，并且只选择或唯一 hint 已配置的
  `commandId` 或 `fileId` 之一
- V2 `commandId` 必须使用非空 ASCII 字母、数字、`_` 和 `-`
- V2 remote command argv 会在配置加载时 trim 并丢弃空字符串，归一化后必须非空
- V2 SSH executable 来自 `LOGAGENT_V2_REMOTE_SSH_COMMAND`，默认
  `/usr/bin/ssh`；启用 remote execution 时必须解析为绝对路径
- V2 SCP executable 来自 `LOGAGENT_V2_REMOTE_SCP_COMMAND`，默认
  `/usr/bin/scp`；启用 remote execution 时必须解析为绝对路径
- V2 `fileId` 复用 command id 安全规则；`remotePath` 必须是绝对安全路径，
  不能包含 `..`、`.`、`//`、反斜杠、空格、glob 或非安全字符
- V2 file template 可配置 `maxBytes`；未设置时使用
  `LOGAGENT_V2_REMOTE_FILE_MAX_BYTES`
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
remote_file/
  result.json
  stdout.txt
  stderr.txt
  <basename>
supportArtifacts/
  environment_evidence/<action_id>/remote_result.json
  environment_evidence/<action_id>/stdout.txt
  environment_evidence/<action_id>/stderr.txt
  environment_evidence/<action_id>/collected_file.bin
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
- WebUI 显式执行只允许选择白名单命令模板或白名单文件模板，不允许输入自由命令或任意路径。
- 默认必须有有效批准记录，配置可对特定只读采集项另行放宽。
- Claude Code 不能直接执行 SSH/SCP，只能通过 LogAgent MCP 请求审批和结构化采集意图。

## 验收标准

- 配置节点可连通时能收集文件到 workspace。
- 非白名单路径和命令被拒绝。
- 采集失败保留错误原因。
- 拒绝动作不会执行连接，并能作为分析事件继续任务。
- Remote Executor smoke 可用 fake ssh 自动测试；真实 ECS smoke 使用已配置免密的 `root@112.74.50.120` 执行 `smoke_ls_root`；默认模板还必须覆盖系统版本、uptime/load、磁盘、内存、进程概览、端口监听，以及 openGemini、Cassandra、RocksDB 的基础只读进程/目录候选。
- V2 批准 `collect_environment` 后可通过 fake ssh 执行单个白名单 remote
  command，生成 background-only `environment_evidence`，并重新排队原 analysis
  run。
- V2 批准 `collect_environment` 后可通过 fake scp 拉取单个白名单 remote
  file，生成 background-only `environment_evidence`，注册 `collected_file.bin`
  support artifact，并重新排队原 analysis run。
- V2 远程采集完成后，远程 command result/stdout/stderr 会进入
  `GET /api/v2/runs/:run_id/artifacts` 的 `supportArtifacts` 和任务 MCP
  `artifact_index`，且标记为 `source="support"`。
- V2 API 批准 `collect_environment` 时如果 decision body 提供
  `input.executorId` 加 `input.commandId` 或 `input.fileId`，必须持久化该 input
  并只创建一个幂等 remote run。
- V2 API 批准 `collect_environment` 时如果 action payload 或 decision input
  只提供 `commandId` / `fileId` 且仅有一个启用 executor，必须自动补齐该
  executor 并只创建一个幂等 remote run。
- V2 API 批准 `collect_environment` 时如果 action payload 或 decision input
  只提供 executor/template 的自然语言 hint，必须只在 hint 唯一匹配已启用
  executor 和 command/file template 时创建 remote run；匹配不到或有歧义时必须
  写入 `REMOTE_REJECTED` 且不创建 remote run。
- V2 `analysis_package.environmentCollection` 必须列出当前可用于
  `collect_environment` 的启用 executor、command template、file template 和
  `executorSelection` / hint 规则，且不包含已禁用 executor。
- V2 API 批准 `collect_environment` 时如果 decision body 提供
  `input.targets[]`，必须为每个 target 创建一个独立幂等 remote run；第一个
  target 完成时不得提前写最终 environment evidence，全部 target 终态后写入一个
  聚合 artifact，并在部分失败时记录 `PARTIALLY_COLLECTED`。
- 未配置的 `fileId`、不安全 `remotePath` 或同时提供 `commandId` / `fileId`
  必须被拒绝，不得执行 SSH/SCP。
- README 和 SPEC 在采集协议或配置变更时同步更新。
