# Dev Self-Test Tools

开发自测模块让外部 MCP 客户端驱动 Linux LocalToolHub 完成
`sync_workspace -> build -> deploy -> run_tests -> report`。完整 workflow 不在 Server 内编排，
而是由客户端本地 skill（如 `skills/dev-selftest-pipeline/`）串联。Server 负责受控 MCP tools、
持久工作区、artifact、run history 和安全边界。

能力以 `logagent.dev_selftest.*` 内置工具进入 Tool Catalog / Tool Runner / MCP，不开自由 shell，
不引入 Agent 后端，也不提供 workflow API、skill registry 或 runbook 兼容入口。

## 职责

- 维护 dev_selftest run 工作区和 `DevSelftestRunRecord` 索引。
- 同步源码：只允许 allowlisted git repo/ref。Windows 端先 commit/push，ToolHub 负责 clone 或 pull。
- 暴露配置发现：`logagent://dev_selftest/config` 返回当前 allowlisted repo/ref、默认 repo/ref、build/docker/test profile ids 和 build/test profile 明细，供客户端 skill 选择参数。
- 热更新 git allowlist：用户明确同意后，MCP `logagent.dev_selftest.allowlist.update` 或 WebUI Settings 可追加 repo/ref、设为默认、写回 Server 配置，并即时影响后续 `sync_workspace`。
- 热更新 Docker-backed build/test profile：用户明确同意后，MCP `logagent.dev_selftest.profiles.upsert` 或 WebUI Settings 可新增/更新 Docker profile、写回 Server 配置，并即时影响后续 `build` / `run_tests` 参数校验。
- 执行配置式 build：旧 host command profile 继续可用；Docker build profile 在镜像内执行 `argv`，把匹配的产物收集到 run `artifacts/`。
- 执行 `docker_cluster` 部署：`docker compose up -d` + 声明式 health check。
- 执行测试：优先使用 test suite 的 inline `docker` target；无 docker target 时走本地桩。
- 生成 `report.md` / `report.json`，聚合每步状态和证据。
- 复用 `TaskExecutor` + `TaskStore` 的 `runMode:"queued"` / `logagent.runs.get/result` 模型；queued 返回的 `task_*` 只用于轮询，`sync_workspace` 返回的 `devselftest_*` 才是后续 step 的工作区 id。

## 边界

- 所有命令、二进制、路径、compose 文件、repo/ref 都来自 `dev_selftest` 配置/运行时 allowlist。
- tool 参数只能选择 profile id、携带 `runId`，并在 `sync_workspace` 里选择 allowlisted `gitRepo/gitRef`；不得传自由 shell 或上传源码包。
- git allowlist 热更新只允许追加/设默认：必须 `confirmedUserConsent=true`，必须通过 URL/ref 校验和 `git ls-remote` 可达性检查；写回配置成功后才更新内存状态。
- Docker profile upsert 只允许 build/test profile：必须 `confirmedUserConsent=true`，必须通过 profile id、非空 argv 和 Docker target 校验；写回配置成功后才更新内存 registry。已排队的 build/run_tests task 携带 profile snapshot，不被后续 upsert 改写。
- `remote_execution.commands` 只作为 test suite 的 argv/timeout 模板，不再表示可纳管远程 executor。
- inline Docker target 只允许受校验的 image/network/workdir/volume/env。
- 密钥只来自 env；report/artifact 只记录 env 名、状态码和脱敏摘要。
- `dev_selftest.enabled=false` 时整组工具禁用，且占位 docker/build 配置不阻断 Server 启动。

## 文件

- `server/src/services/dev_selftest.rs` — 工具组 descriptors/validate/run 与各 step。
- `server/src/services/dev_selftest_allowlist.rs` — 运行时 git allowlist、配置摘要、热更新校验、YAML 写回。
- `server/src/stores/dev_selftest_store.rs` — run 索引（JSON-per-run）。
- `server/src/support/config.rs` — `DevSelftestSettings`、profile 配置和 resolver。
- `server/src/support/docker_target.rs` — inline Docker target 校验。
- `server/src/services/remote_execution.rs` — Docker runner 和命令模板读取。
- `server/src/domain/models.rs` — dev_selftest run/deploy/step/status 模型。
- `skills/dev-selftest-pipeline/` — 本地 Claude Code skill，负责编排 MCP step tools。
- `deploy/devselftest/opengemini/` — 默认 openGemini Docker demo artifact。
- `deploy/probe-opengemini-config.sh` — 探测 Linux 机器环境并生成 openGemini dev_selftest Server 配置。

## 当前实现

- 已实现 `sync_workspace`、`build`、`deploy`、`run_tests`、`report` 五个工具；`sync_workspace` 为 git-only，新 run clone，已有 run pull。
- 已实现 Docker-backed build profile：默认挂载 `source/` 到 `/workspace/source:rw`、`artifacts/` 到 `/workspace/artifacts:rw`，默认 `workdir=/workspace/source`，复杂工具链和脚本推荐固化在镜像中。
- 已实现 `docker_cluster` 部署，默认 demo 为 openGemini 3 meta + 3 (sql+store) 集群。
- 已实现 inline Docker 测试派发：`docker run --rm --network host ... <image> <argv>`。
- 已实现 queued 长任务轮询：`logagent.runs.get` / `logagent.runs.result` 不创建新 run。
- 已实现 `logagent://dev_selftest/config`、`logagent.dev_selftest.allowlist.update` 和 `logagent.dev_selftest.profiles.upsert`；WebUI Settings 使用同一服务读取/保存 allowlist 与 Docker profile。
- 已验证 openGemini demo 的 `sync_workspace -> build -> deploy -> run_tests -> report` 闭环。
- 客户端 skill 默认不在本地编译或测试；每轮改动 commit/push 后直接 `sync_workspace`，以远端 MCP `build` 的错误证据驱动下一轮修改。
- openGemini demo smoke 在写点后对 SELECT 做短轮询；写接口返回成功后，点可能需要很短时间才对查询可见。

## openGemini Demo 约束

- 容器使用静态 IP；openGemini meta 使用 `rpc-bind-address` 字符串作为 raft Server ID。
- 基础镜像使用 `ubuntu:24.04`，避免旧 libstdc++ 缺少运行时符号。
- entrypoint 顺序门控 meta -> store -> sql；`depends_on` 只排序，不等待就绪。
- build 脚本对 openGemini go.mod 做 Go 版本和 `bytedance/sonic` 兼容处理后编译
  `ts-meta`、`ts-store`、`ts-sql`。
- 内网可通过 server 进程 env 覆盖 `OG_BASE_IMAGE`、`GOPROXY`、`GOSUMDB` 和 git mirror。

## 已移除

- SSH/SCP executor、托管 executor record、`/api/executors`、`/api/executor-runs`。
- `suite.executor` 派发和 ssh-kind 测试分发。
- `ssh_binary_replace`、Huawei OBS/package sync、GeminiDB create instance。
- Server 托管 skills、Server 侧 workflow API、旧 `docs/runbooks/dev-selftest-pipeline/` 入口。

修改本模块必须同步更新 `server/SPEC.md`、相关 skill 文档和根 `PROGRESS.md`。
