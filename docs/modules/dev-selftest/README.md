# Dev Self-Test Pipeline

开发自测流水线模块：让远程 MCP 客户端（如 Windows 上的 Claude Code）驱动 Linux
LocalToolHub 完成 sync → build → deploy → run_tests → poll → report。能力以一组内置
`logagent.dev_selftest.*` 工具进入 Tool Catalog / Tool Runner / MCP，不开新执行通道，
不引入 Agent 后端。

## 职责

- 维护 dev self-test run（持久工作区 + `DevSelftestRunRecord` 索引）。
- 执行配置式 build / docker deploy / 测试运行（P1 桩）/ 报告生成。
- 复用 `TaskExecutor` + `TaskStore` 的 run/poll 模型（`runMode:"queued"` +
  `logagent.runs.get/result`）。

## 边界

- 所有命令/二进制/路径/compose/repo+ref 来自 `dev_selftest` 配置 allowlist；tool 参数只选
  profile id 并携带 `runId`，无自由 shell。
- 密钥只从 env；report/artifact 只记 env 名、状态码、脱敏摘要。
- `dev_selftest.enabled=false` 整组禁用，且允许 `docker.binary` 等字段保留占位值，不阻断
  主 Server 启动；切到 `enabled=true` 后才执行绝对路径和 profile 完整性校验。

## 文件

- `server/src/services/dev_selftest.rs` — 工具组（descriptors/validate/run + 各 step）。
- `server/src/stores/dev_selftest_store.rs` — run 索引（JSON-per-run）。
- `server/src/support/config.rs` — `DevSelftestSettings` + 子结构 + resolver。
- `server/src/domain/models.rs` — `DevSelftestRunRecord` / `DevSelftestDeployTarget` /
  `DevSelftestStep` / `DevSelftestRunStatus`。
- `skills/dev-selftest-pipeline/` — runbook + workflow 参考。

## 阶段

- **P1（已实现）**：tarball/git 同步、配置式 build、`docker_cluster` 部署 + health check、
  桩测试运行器、规则化 report。集成测试 `docker_selftest_closed_loop` 用 fake docker 跑通
  全链路。
- **P1 docker 路径已对真实 openGemini 跑通**：3 meta + 3 (sql+store) = 6 容器 / 9 进程，
  `sync→build→deploy→run_tests→report` 全链路 `SUCCEEDED`。集群 artifact（compose/模板/
  entrypoint/build 脚本）作为默认 demo 纳入仓库 `deploy/devselftest/opengemini/`（单模板 +
  entrypoint 按 `OG_ADDR/OG_ID/OG_META_*` env 替换占位符），dev_selftest 配置用绝对路径引用。
  内网可配置（经 server 进程 env，无代码改动）：`OG_BASE_IMAGE` 换镜像名、`GOPROXY/GOSUMDB`
  换 Go 模块源、`dev_selftest.git.repos` 换 openGemini 源码镜像。
  关键约束（复现用）：
  - 容器需**静态 IP**：openGemini meta 用 `rpc-bind-address` 串作 raft Server ID，用主机名时
    与绑定的解析 IP 不匹配 → hashicorp raft 判定「not part of a stable configuration」不选主。
    改静态 IP（对齐官方 `install_cluster.sh` 用 127.0.0.1/2/3）后正常选主。
  - 基础镜像 `ubuntu:24.04`：22.04 的 libstdc++ 缺 `GLIBCXX_3.4.32`，二进制启动即退。
  - 顺序启动门控：meta → store → sql；`depends_on` 仅排序，entrypoint 须等 meta:8091 就绪再
    起 store、等 store:8400 再起 sql。store 按 `store-ingest-addr`（容器 IP:8400）绑定，探活
    须用 `hostname -I` 的容器 IP，**非 127.0.0.1**。
  - 构建：openGemini go.mod 非 1.26，build 脚本先 `go mod edit -go=1.26` + 升级
    `bytedance/sonic` 到最新 + `go mod tidy`，再 `go build -o build/ts-{meta,store,sql}`。
  - MCP 参数：catalog 工具经 MCP 可传顶层参数（`mcp_server::mcp_tool_params` 兼容 `{params:{}}`
    与顶层两种）。
- **P2（规划）**：参数化 executor 模板（小子集 JSON Schema + `{var}` 插值，无 shell）+
  受控 file-template SCP + `ssh_binary_replace` 部署（备份/重启/health/回滚）+ 真实测试
  分发到 executor。
- **P3（规划）**：重构 `huawei_package_sync` 为 core（接受本地 artifact）+ upload wrapper；
  `package_create_instance` 部署 profile：OBS 发布 + `geminidb.create_instance` + 轮询就绪。

修改本模块必须同步更新 `server/SPEC.md`、根 `PROGRESS.md`。
