# Environment Collector 方案

## 实现建议

优先使用 Rust 实现。语言优先级：

```text
Rust -> C/C++ -> Go/Python/Java 等
```

环境采集模块涉及 SSH/SCP、远程命令白名单、文件路径白名单和超时控制，适合用 Rust 做明确的执行边界。

## 职责

Environment Collector 面向测试环境，允许任务跳过浏览器下载和本地上传，直接通过 SSH/SCP 从目标节点收集日志、配置和诊断信息。

Analysis Orchestrator 也可根据 Claude MCP `logagent.request_approval` 的等待请求补充环境采集。该请求默认进入 `WAITING_FOR_APPROVAL`，只有用户批准后 Server 才能将其映射到已配置环境、节点、文件和命令白名单。

## 当前基础能力

当前已落地通用 Remote Executor 框架，并已接入 V2 审批后的单目标环境采集：

- WebUI `Tools / Executors` 可纳管 ECS 执行机，资产持久化在 `storage.data_dir/executors`。
- Server 支持 `remote_command_run` task 和 `EXECUTE_REMOTE_COMMAND` phase，通过系统 `ssh` 二进制执行 `remote_execution.commands` 白名单模板。
- 默认模板 `smoke_ls_root` 执行 `ls -la /root`，用于验证如 `root@112.74.50.120` 的免密 SSH。
- V2 clean-room Server 已提供同类 Remote Executor 基础能力：`/api/v2/executors` 管理 SQLite executor，`/api/v2/executor-command-templates` 暴露环境配置的白名单命令模板，`/api/v2/executor-file-templates` 暴露环境配置的白名单文件模板，`/api/v2/executor-runs` 创建 DB-backed remote command job，并把命令 `result.json`、`stdout.txt`、`stderr.txt` 写入 `LOGAGENT_V2_DATA_DIR/remote_runs/<run_id>/remote_command/`；结果文件可通过受保护的 `/api/v2/executor-runs/:run_id/files/result|stdout|stderr` 下载。
- V2 command template descriptor 与 Rust/V1 对齐：`enabled` 同时反映全局 remote execution 开关和模板自身开关，`timeoutSeconds` 总是模板覆盖值或默认远程命令 timeout。
- V2 command template id 与 Rust/V1 对齐：只允许非空 ASCII 字母、数字、`_` 和 `-`。
- V2 command template argv 与 Rust/V1 对齐：加载时逐项 trim、丢弃空字符串，归一化后仍必须非空。
- V2 `LOGAGENT_V2_REMOTE_SSH_COMMAND` 默认 `/usr/bin/ssh`，支持环境变量和 `~` 展开；启用 remote execution 时必须解析为绝对路径，禁用时可保留相对值但不可执行。
- V2 `LOGAGENT_V2_REMOTE_HOST_KEY_POLICY` 启动时只接受 `accept-new`、`strict` 或 `no`，未知值直接失败，不再静默回退到默认策略。
- V2 已接入 `collect_environment` 审批后的 evidence 闭环：如果 action input
  含有效 `executorId` 且只选择 `commandId` 或 `fileId` 之一，Server 会通过
  Remote Executor 执行白名单命令，或通过 `LOGAGENT_V2_REMOTE_SCP_COMMAND`
  按白名单文件模板 SCP 拉取单个有大小上限的远程文件。完成后写入
  `environment_evidence/<action_id>/result.json`，状态为 `COLLECTED` 或
  `REMOTE_FAILED`，并重新排队同一个 analysis run；同时把远程
  `remote_result.json`、`stdout.txt`、`stderr.txt` 和可选
  `collected_file.bin` 复制为当前 analysis workspace 的 support artifacts，
  并通过 `/api/v2/runs/:run_id/artifacts` 和任务 MCP `artifact_index` 暴露；
  远程目标无效时写入 `REMOTE_REJECTED` evidence；如果没有远程目标，则保留
  与 Rust Server 兼容的 `MOCK` evidence。
- V2 Analyze 审批卡片当前会在 `collect_environment` action 上加载已启用的
  Remote Executor 和白名单命令模板；文件模板可通过 API 和批准 input 使用，
  WebUI 选择器后续补齐。用户批准时可把选择作为 decision `input` 提交，
  Server 会先写回 action payload 再调度采集。
- 当前不支持多节点批量采集，也不支持由 Agent 自动选择
  executor/command/file；这些仍属于完整 Environment Collector 后续工作。

## 适用场景

- 测试集群复现问题
- CI 或压测环境自动诊断
- 已知目标节点 IP，需要直接拉日志、配置和运行状态

## 配置示例

```yaml
environments:
  test-influxdb-cluster:
    ssh_user: test
    ssh_key_path: /data/logagent/keys/test_cluster.pem
    nodes:
      - name: meta-1
        host: 10.0.1.11
        roles: ["meta"]
      - name: data-1
        host: 10.0.1.21
        roles: ["data"]
    collect:
      files:
        - /var/log/influxdb/*.log
        - /etc/influxdb/config.toml
      commands:
        - name: process
          argv: ["ps", "-ef"]
        - name: disk
          argv: ["df", "-h"]
        - name: ports
          argv: ["ss", "-lntp"]
```

## 任务输入

```json
{
  "source": "environment",
  "environment": "test-influxdb-cluster",
  "product": "influxdb",
  "version": "3.0.2",
  "question": "压测时写入延迟突然升高，帮我分析原因"
}
```

## 流程

1. 用户选择测试环境和目标节点范围，或 Agent 请求 `collect_environment`
   审批；审批时可补齐已配置的 `executorId` 加 `commandId` 或 `fileId`。
2. 服务端根据配置建立 SSH 连接。
3. 如果批准的是文件模板，V2 当前通过 SCP 拉取白名单路径下的单个有大小上限文件。
4. 如果批准的是命令模板，V2 执行单个 Remote Executor command 模板。
5. 保存到任务 workspace。
6. 生成 `environment_evidence.json` 和 `manifest.json`；V2 还会把远程命令
   `result/stdout/stderr` 和远程文件 `collected_file.bin` 注册为当前 run 的
   support artifacts。
7. 采集证据回填 Analysis Orchestrator，继续同一任务。

## 连接管理

配置示例：

```yaml
environment_collector:
  max_parallel_nodes: 4
  connect_timeout_seconds: 10
  command_timeout_seconds: 30
  retries: 1
```

策略：

- 多节点默认并行采集，最大并发由 `max_parallel_nodes` 控制。
- 单节点内先拉文件，再执行诊断命令。
- 单个节点失败不直接失败整个任务，但会写入 `environment_evidence.json`。
- SSH 连接失败允许按配置重试。
- 命令超时后终止并保留 stderr/timeout 信息。

## 输出目录

```text
collected/
  meta-1/
    files/
    commands/
      process.txt
      disk.txt
      ports.txt
  data-1/
    files/
    commands/
```

## 安全边界

- 只访问配置中的节点。
- 只采集白名单路径。
- 只执行白名单 argv 命令。
- WebUI 显式 remote command run 只能选择已配置模板，不允许输入自由 shell 命令。
- Remote command template id 只能使用非空 ASCII 字母、数字、`_` 和 `-`，避免配置 ID 进入 API/job 路径时出现转义语义。
- Remote command template argv 加载时会 trim 并丢弃空字符串，避免空白配置进入最终 SSH argv。
- Remote file template id 复用 command id 安全规则，`remotePath` 必须是配置中的
  绝对安全路径，拒绝 `..`、`.`、`//`、反斜杠、空格、glob 和非安全字符。
- V2 `collect_environment` 远程执行只接受已存在 executor 和已配置 command/file
  id，不接受自由命令、自由路径或由用户消息临时扩展白名单。
- V2 远程命令和文件输出 artifact 属于 background/support evidence，只能辅助下一轮
  分析和人工审计，不能绕过 final evidence ref 校验。
- WebUI 审批只能选择已启用 executor 和配置模板；不选择远程目标时 Server
  仍会保留兼容 MOCK evidence 路径。
- SSH key 不进入 LLM Prompt。
- 不做通用远程运维平台。
- MCP 请求和用户消息不能增加配置外节点、路径或命令。
- Claude Code 不能直接连接 SSH/SCP。
