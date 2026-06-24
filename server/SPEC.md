# Server Spec

## 目标

Rust Server 是 LocalToolHub 的唯一执行边界。它接收 WebUI 和 MCP 请求，校验参数和权限，执行受控工具，保存 artifact，并返回结构化结果。

Server 必须实现
[`docs/product-architecture-and-flows.md`](../docs/product-architecture-and-flows.md)
定义的控制面和执行面：所有工具、Fetch、Executor、dev_selftest 和 MCP
调用共享 catalog/run/artifact/history 语义；任何新增执行能力不得绕过该边界。

## 职责边界

必须提供：

- 本机 HTTP API 和静态 WebUI。
- Tool Catalog、Tool Runner 和内置工具。
- Run History、Artifact Store 和 result API。
- Metadata、Fetch、Executor、Code Evidence、Log Analyzer、Skills/System Context、Case Notes。
- MCP Server，支持外部客户端读取资源和调用工具。
- 配置、密钥引用、allowlist、timeout、预算、路径安全和审计。

不得默认提供：

- Claude Code 作为 Server 后端。
- 自研 Agent 决策循环。
- 任意 shell 命令输入。
- 任意远程主机访问。
- 任意本地路径读取。

## Run 模型

统一 run 字段：

```text
runId
toolId or operation
status
phase
inputSummary
resultArtifactId
stdoutArtifactId
stderrArtifactId
supportArtifacts
createdAt
updatedAt
error
```

状态：

```text
QUEUED
RUNNING
WAITING_FOR_APPROVAL
SUCCEEDED
FAILED
CANCELLED
```

`WAITING_FOR_APPROVAL` 仅用于远程采集、SCP、大范围文件读取等高风险动作。普通工具运行不进入用户追问循环。

## Tool Catalog

每个工具 descriptor 必须包含：

```text
toolId
displayName
description
source: built_in | configured | source_built
backend
runnable
readOnly
manualOnly
acceptedSuffixes
minFiles / maxFiles
paramsSchema
paramsTemplate
outputViews
timeoutSeconds
unavailableReason
```

WebUI、HTTP API 和 MCP `tools/list` 必须共享同一 catalog。

`examples/logagent.yaml` 的 `tools:` 段声明全部外部工具（`pprof_analyzer` + `influxql_analyzer` / `flux_query_analyzer` / `opengemini_storage_analyzer` / `influxdb_storage_analyzer`），默认 `enabled: false` 且使用 `path_env`，使 catalog 在 Linux/Windows 上无需外部二进制即包含全部工具；启用时由 `path_env` 指向平台对应的绝对二进制路径（Windows 带 `.exe`）。built-in 工具（preprocess、batch influxql、metadata ×4、fetch、huawei package sync、GeminiDB Influx 实例管理 ×6：create/delete/list/rename/toggle_ssl/restart）始终在 catalog 中，按各自子系统开关启用。GeminiDB Influx 工具组用 `X-Auth-Token` 鉴权（token 仅来自 env），endpoint/projectId 支持配置默认 + 每次运行 params 覆盖；请求方法、路径和参数映射以 HuaweiCloud NoSQL API v3 文档为准：创建实例使用官方 create body 字段和 `flavor` 数组，列表默认 `datastore_type=influxdb`，SSL 使用 `POST .../ssl-option` + `ssl_option=on|off`，重启实例不带 body、仅可选 `node_id`。

## Tool Runner

执行要求：

- 只运行 catalog 中 enabled/runnable 的工具。
- 参数必须通过 schema 校验。
- 输入文件必须来自当前 upload/run/artifact 或明确 allowlist。
- stdout/stderr 必须保存为 artifact。
- 结构化 JSON stdout 优先解析为 result。
- 非零退出、timeout 和 parser error 必须写入 result/error，不吞掉原始输出。
- 密钥和敏感 header 必须脱敏。

## MCP

MCP endpoint 支持两种传输：HTTP（`POST /api/mcp`，stateless streamable-http：按 `Accept` 返回 `application/json` 或单帧 SSE `event: message`，回显 `MCP-Protocol-Version`，不签发 `Mcp-Session-Id`）和 stdio（`mcp-serve` 子命令）。MCP tools 必须复用 Tool Runner 和各能力模块，不得另开执行通道。
`mcp.enabled=false` 时 HTTP `/api/mcp` 和 stdio `mcp-serve` 必须都拒绝服务。
跨域：`mcp.allowed_origins` 非空时校验 `Origin`（仅放行列表内来源，浏览器跨域请求拒绝；无 `Origin` 头的非浏览器/隧道客户端始终放行）；为空则不校验（localhost / SSH 隧道场景）。Windows 远程连 Linux 优先 SSH 隧道；直接暴露需 TLS + API key + `allowed_origins`。

`tools/list` 对外返回的每个 `inputSchema` 必须是根部 `type: "object"` 的 JSON Schema。
如果内部 `ToolDescriptor.params_schema` 仍是早期 configured tool 使用的「根部属性 map」形状，
MCP 边界必须把它包装成 `{type:"object", properties:{...}}`；非对象或非 object 根 schema
必须降级为空 object schema。该兼容层只影响 MCP 输出，不改变 `/api/tools` catalog 的内部
`paramsSchema` 展示。

`tools/call` 支持可选 `runMode: "sync"|"queued"`（默认 `sync`）。`queued` 创建一个 `ToolRun` 经 `TaskExecutor` 入队并立即返回 `{runId, status:"QUEUED", url}`，不等待执行；长任务用 `queued`，再用 `logagent.runs.get` / `logagent.runs.result` 轮询。

`logagent.runs.get` / `logagent.runs.result` 是 MCP 原生 platform 工具（`ToolDescriptor.platform=true`，`runnable=false`）：`tools/call` 直接读 `TaskStore`，**不创建 ToolRun**，避免轮询污染 run history。HTTP 端等价能力仍由 `/api/runs/*` 提供。

最低方法：

```text
initialize
resources/list
resources/read
tools/list
tools/call
```

## Dev Self-Test

开发自测流水线（P1：docker 闭环）。一组内置工具 `logagent.dev_selftest.*`（sync_workspace / build / deploy / run_tests / report），通过持久 run 工作区 `data/dev_selftest/runs/{runId}/`（含 `source/`、`artifacts/`、`logs/`、`progress.json`、`report.md`）跨多次 tool 调用串联，复用 Tool Runner 同一执行边界。`dev_selftest.enabled=false` 时整组禁用。

- `dev_selftest.enabled=false` 时关闭整组工具，并允许配置中保留未填写或占位的 `docker.binary`，不得阻断 Server 启动。
- `dev_selftest.enabled=true` 时，所有 build/docker/test 命令、`docker.binary`、`compose_file`、git 仓库+ref 必须来自配置 allowlist 且绝对路径；tool 参数只能选 profile id 并携带 `runId`，不得自由 shell。
- P1 实现：tarball/git 源码同步、配置式 build + artifact glob 收集、`docker_cluster` 部署（`docker compose -p … up -d` + 声明式 health check）、规则化 report。P1 不做 health check 失败回滚（P2 的 SSH 二进制替换路径才做）。
- `run_tests` 两种模式：带 `docker` 块的测试套件经 **executor docker runner** 派发（见下）；无 `docker` 块则走 P1 本地桩（在 Server 主机跑配置式 `argv`）。`run_tests` 支持 `runMode:"queued"`：返回 `{runId,status:"QUEUED"}` 后用 `logagent.runs.get`/`runs.result` 轮询（platform 工具，不建 ToolRun）。
- **executor runner（P2 docker 切片）**：把 executor 执行逻辑抽成可复用的 `run_executor_command`，支持 `ExecutorTarget::{Ssh, Docker}` 两种 target。SSH 分支行为与原 `run_remote_command_task` 逐字一致（`ssh -o BatchMode=yes … user@host <argv>`），超时映射保留 `TimedOut` 语义（`ExecutorRunStatus::{Ok,Failed,TimedOut,SpawnFailed}`）。Docker 分支构造 `docker run --rm --network <net|"host"> [--workdir] [--env] [--volume] <image> <argv>`，`extra_env`（系统 env）后置覆盖 `target.env`（用户 env）。runner 不检查 `remote_execution.enabled`（纯工具；开关在任务/handler 入口），故 dev_selftest 可在 SSH 关闭时复用 Docker 分支。
- **dev_selftest 内联 Docker target**：`run_tests` 对 `docker` 块内联构建 `ExecutorTarget::Docker`（image/network/workdir/volumes/env 来自配置），argv/timeout 取自 `suite.command` 引用的 `remote_execution.commands` 模板（无 `command` 则用 `suite.argv`）。volume host 侧 `${DEVSELFTEST_*}` 经 `deploy_env` 插值并断言插值后绝对。系统 env（`DEVSELFTEST_HOST/PORT` + run 目录 4 var）**最终优先**，用户 `env` 不可覆盖。`--network host` 下 `127.0.0.1:<host 暴露端口>` 即 ts-sql。
- **Docker executor 纳管（P2）**：`RemoteExecutorRecord` 增 `kind: ExecutorKind::{Ssh,Docker}`（默认 `Ssh`，旧 record 向后兼容）+ `docker: Option<DockerTargetSpec>`。store `validate_executor` 按 kind 分支（Ssh⇒host/user/port；Docker⇒`validate_docker_target`，host 侧仅绝对路径，不接受 `${DEVSELFTEST_*}` 占位符）。`/api/executors` create/patch 接受 `kind`+`docker`（Docker 时 host/user 可空）。`run_remote_command_task` 按 kind 分支——Docker 经 `run_executor_command`（`docker run --rm --network … <image> <argv>`，volumes 原样、不绑定 run），**计入 `/api/executor-runs` run history**（`RemoteCommandRunRecord` 带 `kind`+`dockerImage`）。配置 seeding：`remote_execution.executors`（启动 `seed_executors` create-if-absent，不覆盖 API 创建的 record）+ `remote_execution.docker_binary`。
- **dev_selftest 消费 executor record**：`DevSelftestTestSuite` 增 `executor: Option<String>`（record id），与 inline `docker` 互斥。`run_tests` 派发优先级：`executor`（managed record）> inline `docker` > P1 本地桩。executor-record 模式查 record（须存在 + enabled + `kind=Docker`，ssh-kind 报未支持），从 `record.docker` 构建 target（volumes 原样），argv/timeout 取 `suite.command` 模板或 `suite.argv`，系统 env 最终优先——同 inline 路径。对应「executor 纳管 + 指定执行」。
- **配置 + 安全校验**：`DevSelftestTestSuite` 增 `command: Option<String>` / `docker: Option<DevSelftestTestDocker>` / `executor: Option<String>`；`command` 与非空 `argv` 互斥且至少一个；`command` 须配 `docker` 或 `executor`；`executor` 与 `docker` 互斥；`executor` 须为 `executor_…` id。`DockerTargetSpec`（`support::docker_target.rs`，dev_selftest 与 executor record 共享）校验：image 非空且不以 `-` 开头；network 为 `host` 或安全标识符；workdir 绝对无 `..`；volume 为 `host:absolute|${DEVSELFTEST_*}:container:absolute[:ro|rw]`（`${DEVSELFTEST_*}` 占位符仅 dev_selftest inline 允许，executor record 仅绝对路径）；env 键 `^[A-Z_][A-Z0-9_]*$`。
- `deploy` 把 run 目录环境变量（`DEVSELFTEST_RUN_DIR/SOURCE_DIR/ARTIFACTS_DIR/PROJECT_NAME`）注入 `docker compose` **和** health check 命令，使 compose 可用 `${DEVSELFTEST_SOURCE_DIR}` 挂载本次 run 编译出的二进制（通用，非 openGemini 专属）。
- MCP `tools/call` 参数：catalog 工具既接受 `{params:{...}}`（HTTP `POST /api/tools/:id/runs` 信封）也接受顶层参数（MCP 规范，`arguments` 即 `inputSchema`），后者自动剥离 `runMode/uploadIds`——真实 MCP 客户端（Claude Code）可按 schema 直接传顶层参数。`logagent.runs.get/result` 等 platform 工具的 `runId` 仍在 `arguments` 顶层。
- Docker 路径已对真实 **openGemini** 3meta+3(sql+store) 集群端到端跑通（sync→build→deploy→run_tests→report 全 SUCCEEDED）。集群 artifact（compose/模板/entrypoint/build 脚本）作为默认 demo 纳入仓库 `deploy/devselftest/opengemini/`，单模板 + entrypoint 按 `OG_ADDR/OG_ID/OG_META_*` env 替换占位符。内网可配置（经 server 进程 env，无代码改动）：`OG_BASE_IMAGE` 换镜像名、`GOPROXY/GOSUMDB` 换 Go 模块源、`dev_selftest.git.repos` 换 openGemini 源码镜像。关键约束：容器需**静态 IP**（openGemini raft 用 `rpc-bind-address` 串作 Server ID，主机名会与绑定 IP 不匹配导致不选主）、`ubuntu:24.04`（22.04 libstdc++ 过旧）、顺序启动门控（meta→store→sql，`depends_on` 仅排序，entrypoint 须等就绪）、store 探活用容器自身 IP 而非 127.0.0.1。
- 后续：P2 docker 切片已完成（executor runner 抽通 SSH/Docker target + dev_selftest 内联 Docker target 真实 smoke + Docker executor 纳管 record/CRUD/run history/seeding + dev_selftest `executor` 引用消费）；仍 deferred：参数化 executor 命令模板（`{var}` + 小 JSON Schema）、ssh-kind executor 的 dev_selftest 测试分发（受控 SCP + `ssh_binary_replace` 部署）。P3 重构 package-sync core + OBS 发布 + `geminidb.create_instance` + 轮询就绪。

## 配置

配置必须支持环境变量展开。secret 只允许通过 env 引用。

关键配置：

```yaml
server.bind
storage.data_dir
auth.api_keys[].value_env
tools.directories
fetch.enabled
fetch.allowed_hosts
executors.enabled
code_evidence.repos
mcp.enabled
```

## 部署文档

Server 的正式部署手册维护在 [`deploy/SERVER_DEPLOYMENT.md`](../deploy/SERVER_DEPLOYMENT.md)。手册必须覆盖源码目录与 runtime 目录分离、`.env` 与 `logagent.yaml` 配置、`rebuild-install.sh` 构建安装、`logagentctl.sh` 启停、WebUI/MCP 验证、systemd 可选托管、升级、备份、回滚和常见排障；不得要求把 secret 写入仓库或配置样例。

## 验收标准

- `/health` 无鉴权可用。
- `/` 返回 WebUI 静态页面。
- `/api/tools` 与 MCP tools/list 一致。
- `mcp.enabled=false` 时 `/api/mcp` 返回 JSON-RPC error，`mcp-serve` 启动失败。
- 手动 tool run 生成 result/stdout/stderr artifacts。
- Fetch/Executor/Code Evidence 默认关闭或受 allowlist 限制。
- 旧 Agent/Analyze 路径不再是新开发主线。
- `cargo fmt --check`、`cargo check`、相关 `cargo test` 通过。
