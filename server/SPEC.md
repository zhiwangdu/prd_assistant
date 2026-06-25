# Server Spec

## 目标

Rust Server 是 LocalToolHub 的唯一执行边界。它接收 WebUI 和 MCP 请求，校验参数和权限，执行受控工具，保存 artifact，并返回结构化结果。

## 职责边界

必须提供：

- 本机 HTTP API 和静态 WebUI。
- Tool Catalog、Tool Runner 和内置工具。
- Run History、Artifact Store 和 result API。
- Dev Self-Test MCP step tools、Log Analyzer（预处理 + analyzer）。
- MCP Server，支持外部客户端读取资源和调用工具。
- 配置、密钥引用、allowlist、timeout、预算、路径安全和审计。

不得默认提供：

- Claude Code 作为 Server 后端。
- 自研 Agent 决策循环。
- Server 侧 workflow 编排、skill registry、skill 下载 API 或 runbook 兼容入口。
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
SUCCEEDED
FAILED
CANCELLED
```

当前两模块没有用户审批等待态；高风险执行能力必须在配置 allowlist 中提前收敛。运行期配置变更入口仅限 dev_selftest git repo/ref allowlist 热更新和 Docker-backed build/test profile upsert：它们都要求用户明确同意并传入 `confirmedUserConsent=true`，不启动 workflow 或修改既有 run。

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

`examples/logagent.yaml` 的 `tools:` 段声明全部 analyzer（`pprof_analyzer` + `influxql_analyzer` / `flux_query_analyzer` / `opengemini_storage_analyzer` / `influxdb_storage_analyzer`），默认 `enabled: false` 且使用 `path_env`，使 catalog 在 Linux/Windows 上无需外部二进制即包含全部工具；启用时由 `path_env` 指向平台对应的绝对二进制路径（Windows 带 `.exe`）。built-in 工具（`logagent.preprocess_log_package`、`logagent.batch_influxql_analysis`、`logagent.dev_selftest.*`、platform `logagent.runs.get/result`）始终在 catalog 中，按各自开关启用。

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

`tools/call` 支持可选 `runMode: "sync"|"queued"`（默认 `sync`）。`queued` 创建一个 `ToolRun` 经 `TaskExecutor` 入队并立即返回 `{runId, status:"QUEUED", url}`，不等待执行；长任务用 `queued`，再用 `logagent.runs.get` / `logagent.runs.result` 轮询。
MCP `tools/list` 必须在所有可运行 catalog tool 的 `inputSchema` 中公开可选 `runMode`，否则真实客户端（如 Claude Code）不会把 queued 参数作为合法 tool input 传入。Platform tools（`logagent.runs.get/result`）不得公开或接受 queued。

`logagent.runs.get` / `logagent.runs.result` 是 MCP 原生 platform 工具（`ToolDescriptor.platform=true`，`runnable=false`）：`tools/call` 直接读 `TaskStore`，**不创建 ToolRun**，避免轮询污染 run history。HTTP 端等价能力仍由 `/api/runs/*` 提供。

最低方法：

```text
initialize
resources/list
resources/read
tools/list
tools/call
```

MCP resources 必须包含：

```text
logagent://runs/recent
logagent://tools/catalog
logagent://dev_selftest/config
```

`logagent://dev_selftest/config` 返回脱敏摘要：`gitRepos`、`defaultGitRepo`、`defaultGitRef`、`buildProfiles`、`dockerProfiles`、`testSuites` 以及 build/test profile 明细（host/docker、image、timeout），供客户端 skill 在调用 `sync_workspace` 前发现真实 allowlist，不得猜测或读取 Server 本机配置文件。

## Dev Self-Test

开发自测能力是一组受控 MCP step tools（docker 闭环）。Server 提供 `logagent.dev_selftest.*`（sync_workspace / build / deploy / run_tests / report）和持久 run 工作区 `data/dev_selftest/runs/{runId}/`（含 `source/`、`artifacts/`、`logs/`、`progress.json`、`report.md`）；完整 workflow 由外部 MCP client / 本地 skill 串联，Server 不提供 workflow API 或 agent loop。`dev_selftest.enabled=false` 时整组禁用。

- `dev_selftest.enabled=false` 时关闭整组工具，并允许配置中保留未填写或占位的 `docker.binary`，不得阻断 Server 启动。
- `dev_selftest.enabled=true` 时，所有 build/docker/test 命令、`docker.binary`、`compose_file`、git 仓库+ref 必须来自配置 allowlist 且绝对路径；tool 参数只能选 profile id 并携带 `runId`，不得自由 shell。
- git repo/ref allowlist 启动时从 `dev_selftest.git.repos` 初始化为运行时状态；`sync_workspace` 参数校验读取运行时 allowlist。
- 热更新入口：MCP tool `logagent.dev_selftest.allowlist.update` 与 HTTP `PUT /api/settings/dev-selftest/git-allowlist`。参数为 `repoUrl`、`gitRef`、`setDefault`（默认 `true`）、`confirmedUserConsent`、`reason`。若 `confirmedUserConsent` 不为 `true` 必须拒绝。Server 必须校验 URL/ref、用配置的 git binary 执行 `ls-remote --exit-code <repoUrl> <gitRef>`，再结构化更新 `--config` YAML 的 `dev_selftest.git.repos`，写回成功后才更新内存 allowlist。
- 热更新策略为追加/设默认：旧 repo/ref 保留；`setDefault=true` 时新 repo/ref 排到第一位，成为 `logagent://dev_selftest/config` 的默认推荐。已创建的 `devselftest_*` 工作区、已排队或运行中的 task 不被修改。
- build/test profile 启动时从 `dev_selftest.builds` / `dev_selftest.test_suites` 初始化为运行时 registry；`build` 和 `run_tests` 参数校验读取 registry，并把选中的 profile snapshot 写入 queued task params，避免排队后被后续 profile update 改写执行内容。
- Docker-backed profile 热更新入口：MCP tool `logagent.dev_selftest.profiles.upsert` 与 HTTP `PUT /api/settings/dev-selftest/profiles/:kind/:id`。参数为 `kind: build|test`、`id`、`image`、`argv`、可选 `timeoutSeconds/network/workdir/volumes/env/artifactGlobs/displayName`、`confirmedUserConsent`、`reason`。若 `confirmedUserConsent` 不为 `true` 必须拒绝。Server 必须校验 profile id、Docker target 与非空 argv，结构化更新 `--config` YAML，写回成功后才更新内存 registry。执行工具仍只接收 profile id，不接收任意 shell。
- 当前实现：git-only 源码同步、配置式 build + artifact glob 收集、`docker_cluster` 部署（`docker compose -p … up -d` + 声明式 health check）、规则化 report。`sync_workspace` 必须提供 allowlisted `gitRepo` + `gitRef`；新 run 对 `source/` 执行 clone，复用已有 run 时对已有 git checkout 执行 fast-forward pull。health check 失败不做自动回滚，证据写入 logs/report。
- `build` 两种模式：旧 host command profile 继续直接在 run `source/` 中执行；带 `docker` 块的 build profile 通过 Docker runner 执行镜像内 `argv`，默认挂载 `${DEVSELFTEST_SOURCE_DIR}` 到 `/workspace/source:rw`、`${DEVSELFTEST_ARTIFACTS_DIR}` 到 `/workspace/artifacts:rw`，默认 `workdir=/workspace/source`，继续按 `artifact_globs` 从源码目录收集产物。
- `run_tests` 两种模式：带 `docker` 块的测试套件经 **inline Docker runner** 派发（见下）；无 `docker` 块则走本地桩（在 Server 主机跑配置式 `argv`）。`run_tests` 支持 `runMode:"queued"`：返回 `{runId,status:"QUEUED"}` 后用 `logagent.runs.get`/`runs.result` 轮询（platform 工具，不建 ToolRun）。
- **Docker runner**：可复用的 `run_executor_command` 只支持 `ExecutorTarget::Docker`（构造 `docker run --rm --network <net|"host"> [--workdir] [--env] [--volume] <image> <argv>`，`extra_env` 系统环境变量后置覆盖 `target.env` 用户环境变量，超时映射 `ExecutorRunStatus::{Ok,Failed,TimedOut,SpawnFailed}`）。runner 是纯工具，不检查任何 enable 开关（开关在 dev_selftest 入口），dev_selftest 直接复用。SSH/SCP executor 与「纳管」executor record 路径已移除。
- **dev_selftest 内联 Docker target**：`run_tests` 对 `docker` 块内联构建 `ExecutorTarget::Docker`（image/network/workdir/volumes/env 来自配置），argv/timeout 取自 `suite.command` 引用的 `remote_execution.commands` 模板（无 `command` 则用 `suite.argv`）。volume host 侧 `${DEVSELFTEST_*}` 经 `deploy_env` 插值并断言插值后绝对。系统 env（`DEVSELFTEST_HOST/PORT` + run 目录 4 var）**最终优先**，用户 `env` 不可覆盖。`--network host` 下 `127.0.0.1:<host 暴露端口>` 即 ts-sql。
- **配置 + 安全校验**：`DevSelftestTestSuite` 有 `command: Option<String>` / `docker: Option<DevSelftestTestDocker>`；`command` 与非空 `argv` 互斥且至少一个；`command` 须配 `docker`。`DockerTargetSpec`（`support::docker_target.rs`）校验：image 非空且不以 `-` 开头；network 为 `host` 或安全标识符；workdir 绝对无 `..`；volume 为 `host:absolute|${DEVSELFTEST_*}:container:absolute[:ro|rw]`；env 键 `^[A-Z_][A-Z0-9_]*$`。
- `deploy` 把 run 目录环境变量（`DEVSELFTEST_RUN_DIR/SOURCE_DIR/ARTIFACTS_DIR/PROJECT_NAME`）注入 `docker compose` **和** health check 命令，使 compose 可用 `${DEVSELFTEST_SOURCE_DIR}` 挂载本次 run 编译出的二进制（通用，非 openGemini 专属）。
- MCP `tools/call` 参数：catalog 工具既接受 `{params:{...}}`（HTTP `POST /api/tools/:id/runs` 信封）也接受顶层参数（MCP 规范，`arguments` 即 `inputSchema`），后者自动剥离 `runMode/uploadIds`——真实 MCP 客户端（Claude Code）可按 schema 直接传顶层参数。`logagent.runs.get/result` 等 platform 工具的 `runId` 仍在 `arguments` 顶层。
- Docker 路径已对真实 **openGemini** 3meta+3(sql+store) 集群端到端跑通（sync→build→deploy→run_tests→report 全 SUCCEEDED）。集群 artifact（compose/模板/entrypoint/build 脚本）作为默认 demo 纳入仓库 `deploy/devselftest/opengemini/`，单模板 + entrypoint 按 `OG_ADDR/OG_ID/OG_META_*` env 替换占位符。内网可配置（经 server 进程 env，无代码改动）：`OG_BASE_IMAGE` 换镜像名、`GOPROXY/GOSUMDB` 换 Go 模块源、`dev_selftest.git.repos` 换 openGemini 源码镜像。关键约束：容器需**静态 IP**（openGemini raft 用 `rpc-bind-address` 串作 Server ID，主机名会与绑定 IP 不匹配导致不选主）、`ubuntu:24.04`（22.04 libstdc++ 过旧）、顺序启动门控（meta→store→sql，`depends_on` 仅排序，entrypoint 须等就绪）、store 探活用容器自身 IP 而非 127.0.0.1。
- 后续：dev_selftest 继续只保留 MCP step tools + inline Docker 派发；workflow 放在客户端 skill。SSH/SCP executor、纳管 executor record、fetch/metadata/cases/server-side skills/gemini_db/huawei_package_sync 不再回到默认产品面。仍 deferred：参数化命令模板（`{var}` + 小 JSON Schema）、`max_input_chars` 等 vestigial 字段清理、`TaskKind::RemoteCommandRun` 等兼容变体在旧数据清退后移除。

## 配置

配置必须支持环境变量展开。secret 只允许通过 env 引用。

关键配置：

```yaml
server.bind
storage.data_dir
auth.api_keys[].value_env
tools.<name>.{enabled, path_env}
remote_execution.commands
mcp.enabled
dev_selftest.enabled
```

## 部署文档

Server 的正式部署手册维护在 [`deploy/SERVER_DEPLOYMENT.md`](../deploy/SERVER_DEPLOYMENT.md)。手册必须覆盖源码目录与 runtime 目录分离、`.env` 与 `logagent.yaml` 配置、`rebuild-install.sh` 构建安装、`logagentctl.sh` 启停、WebUI/MCP 验证、systemd 可选托管、升级、备份、回滚和常见排障；不得要求把 secret 写入仓库或配置样例。

## 验收标准

- `/health` 无鉴权可用。
- `/` 返回 WebUI 静态页面。
- `/api/tools` 与 MCP tools/list 一致。
- `mcp.enabled=false` 时 `/api/mcp` 返回 JSON-RPC error，`mcp-serve` 启动失败。
- 手动 tool run 生成 result/stdout/stderr artifacts。
- dev_selftest 默认关闭或受 allowlist 限制。
- 旧 Agent/Analyze、fetch/executor/metadata/cases/skills 路径不再是新开发主线（已收敛移除）。
- `cargo fmt --check`、`cargo check`、相关 `cargo test` 通过。
