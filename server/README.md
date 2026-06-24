# Server

`server/` 是 LocalToolHub 的 Rust/Axum 服务端。目标交付形态是一个本地单二进制，托管 WebUI，管理工具目录，执行受控工具，保存 run/artifact，并提供 MCP Server 给外部客户端使用。

Server 在目标产品架构中是唯一执行边界：WebUI、MCP client、Fetch、Executor
和 dev_selftest 都必须通过同一套 registry、schema、allowlist、run history
和 artifact 审计。完整架构和使用流程见
[`docs/product-architecture-and-flows.md`](../docs/product-architecture-and-flows.md)。

## 目标职责

Server 负责：

- API Key 鉴权和本机 HTTP API。
- WebUI 静态托管。
- Tool Catalog 和 Tool Runner。
- 统一 Run History 和 Artifact Store。
- Metadata、Fetch、Executor、Code Evidence、Log Analyzer、Skills/System Context 和 Case Notes。
- MCP resources/tools；`mcp.enabled=false` 时 `/api/mcp` 与 `mcp-serve` 均拒绝服务。
- 配置加载、路径安全、敏感信息脱敏、timeout、allowlist 和审计。

Server 不负责：

- 自研通用 Agent。
- 默认启动 Claude Code session。
- 复杂多轮推理状态机。
- 任意 shell、任意 SSH 或任意文件读取。
- 企业级多用户权限和集中式日志平台。

## 现有资产复用

本分支从 main 的 Rust Server 出发。可复用资产包括：

- Axum app、API Key middleware、静态 WebUI 托管。
- 上传、workspace、task/artifact store 的一部分实现。
- Tool Runner、Fetch、Remote Executor、Metadata、Skills、Case、MCP 的已有代码。
- `third_party/` analyzer 构建脚本和工具配置经验。

需要瘦身或降级的资产：

- Session-first Analyze 产品主线。
- Claude Code Session Runner 作为 Server 后端。
- Analysis Orchestrator 多轮状态机。
- LLM Gateway 在主路径上的决策职责。

## 目标内部结构

```text
server/src
  main.rs
  app.rs
  http/
    health.rs
    tools.rs
    runs.rs
    artifacts.rs
    metadata.rs
    fetch.rs
    executors.rs
    mcp.rs
    settings.rs
  domain/
    tool.rs
    run.rs
    artifact.rs
    metadata.rs
  stores/
    run_store.rs
    artifact_store.rs
    metadata_store.rs
    settings_store.rs
    case_store.rs
  services/
    tool_catalog.rs
    tool_runner.rs
    builtins.rs
    log_analyzer.rs
    fetch.rs
    remote_execution.rs
    code_evidence.rs
    mcp.rs
  support/
    auth.rs
    config.rs
    error.rs
    fs_utils.rs
    id.rs
```

旧分析 Agent 模块（`agent_backend.rs`、`llm_gateway.rs`、`analysis_state.rs`、`session_store.rs`、`agent_contracts.rs`、`domain_adapters.rs`、旧 `mcp.rs`、`http/{sessions,tasks,debug,settings}.rs`）已在阶段 5 删除；运行时只剩工具工作台语义（tools / runs / artifacts / metadata / fetch / executors / MCP / cases / system_context）。现阶段 crate、binary、配置文件和 MCP namespace 仍保留 `logagent` 兼容命名，用户可见产品名使用 LocalToolHub。

## 数据目录

```text
data/
  logagent.sqlite            # 可选，统一索引和配置
  uploads/
  runs/
    run_xxx/
      input/
      result.json
      stdout.txt
      stderr.txt
      artifacts/
  artifacts/
  metadata/
  cases/
  code_worktrees/
```

Artifact 对外只暴露逻辑路径和 artifact id，不暴露任意本机路径。

## API 方向

核心接口：

```http
GET /health
GET /
GET /api/tools
GET /api/tools/:tool_id
POST /api/tools/:tool_id/runs
GET /api/runs
GET /api/runs/:run_id
GET /api/runs/:run_id/result
GET /api/runs/:run_id/artifacts
GET /api/artifacts/:artifact_id
POST /api/mcp
```

`/api/runs*`、`/api/artifacts/:artifact_id`、`POST /api/mcp` 已实现（阶段 2）；`/api/tools/runs*` 保留为兼容别名。

能力接口：

```http
/api/metadata/*
/api/fetch/*
/api/executors/*
/api/settings/*
/api/cases/*
/api/skills/*
```

旧 Session/Task API 只作迁移兼容，不作为新功能入口。

## GeminiDB Influx 工具组

Server 内置 6 个 GeminiDB Influx 实例管理工具：create / delete / list /
rename / toggle SSL / restart。工具通过 `huawei_cloud.gemini_db` 配置开关启用，
使用 `X-Auth-Token` 鉴权（token 只从 `auth_token_env` 读取），并支持在每次
tool run 的 params 中用 `endpoint` / `projectId` 覆盖配置默认值。

这些工具的 HTTP 方法、路径和参数映射按 HuaweiCloud NoSQL API v3 文档实现：
创建实例使用官方 create body 字段和 `flavor` 数组，列表默认追加
`datastore_type=influxdb`，SSL 切换调用 `POST .../ssl-option` 并发送
`ssl_option=on|off`，重启整实例时不发送 body（传 `nodeId` 时映射为
`node_id`）。运行结果会保存请求摘要、响应、状态码和脱敏后的 request body。

## Dev Self-Test 配置门控

`dev_selftest.enabled=false` 是真正的关闭态：配置中未填写或保留占位的
`docker.binary` 不会阻断 Server 启动，整组 `logagent.dev_selftest.*` 工具保持禁用。
只有当 `dev_selftest.enabled=true` 时，Docker binary、compose 文件和 build/test
profile 才进入严格 allowlist 校验；执行参数仍只能选择配置好的 profile id。

测试套件（`dev_selftest.test_suites.*`）支持两种模式：带 `docker` 块的套件经 executor
docker runner（`run_executor_command` 的 `ExecutorTarget::Docker` 分支，`docker run --rm
--network host <image> <argv>`）内联派发，临时容器通过宿主暴露端口访问已部署集群；无
`docker` 块则走 P1 本地桩。`command`（引用 `remote_execution.commands` 模板）与非空 `argv`
互斥；docker target（image/network/workdir/volumes/env）做安全校验；系统 env
（`DEVSELFTEST_HOST/PORT` + run 目录 var）最终优先，用户 env 不可覆盖。

Docker executor 已纳管：`RemoteExecutorRecord` 支持 `kind: docker`（+ `docker` spec），`/api/executors`
create/patch 接受 docker 字段，`/api/executor-runs` 对 docker-kind executor 经 `run_executor_command`
派发并计入 run history；`remote_execution.executors` 配置 seeding（启动 create-if-absent）。dev_selftest
测试套件可 `executor: <id>` 引用一个 docker-kind record 派发（与 inline `docker` 互斥）。ssh-kind executor
的 dev_selftest 测试分发仍 deferred。详见 `server/SPEC.md` 与 `deploy/devselftest/opengemini/README.md`。

## 本地运行

当前 main 代码仍使用原命令，后续实现会收敛配置名：

```bash
export LOGAGENT_NATIVE_API_KEY=dev-token
cargo run -p logagent-server -- --config examples/server-test.yaml
```

目标命令：

```bash
cargo run -p logagent-server -- --config examples/local-toolhub.yaml
```

面向 Linux 机器的完整 Server 部署、启停、MCP 接入、systemd 托管、升级和回滚流程见
[`deploy/SERVER_DEPLOYMENT.md`](../deploy/SERVER_DEPLOYMENT.md)。

## 平台兼容性 (Linux / Windows)

Server 的非测试代码不依赖任何 Unix-only API，可在 Linux 和 Windows 上编译运行：

- `tokio::signal::ctrl_c`、`tokio::process::Command`、`std::env::temp_dir()` 等均为跨平台 API。
- 所有 `std::os::unix` 调用都在 `#[cfg(unix)]` 守卫下（非测试代码）或位于 `#[cfg(all(test, unix))]` 的测试模块中（依赖 bash/可执行权限的集成测试只在 Unix 运行；纯解析测试在所有平台运行）。
- `remote_execution.ssh_binary` 默认值按平台选择：Linux `/usr/bin/ssh`，Windows `C:\Windows\System32\OpenSSH\ssh.exe`；可在配置中显式覆盖。
- `examples/logagent.yaml` 的 `tools:` 段声明全部工具（pprof + 4 个 analyzer），默认 `enabled: false` 并使用 `path_env`，因此无需外部二进制即可在两个平台加载，catalog 即包含全部工具。启用时把对应 `path_env` 指向绝对二进制路径（Windows 上带 `.exe`）并把 `enabled` 改为 `true`。

Windows 上构建源码 analyzer 使用 `scripts/build-tools.ps1`（对应 Linux/macOS 的 `scripts/build-tools.sh`），产物为 `bin/tools/*.exe`。

跨平台编译校验（在 Linux 上交叉编译检查 Windows 目标，需要 mingw-w64）：

```bash
rustup target add x86_64-pc-windows-gnu
export CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-gcc
export CXX_x86_64_pc_windows_gnu=x86_64-w64-mingw32-g++
export AR_x86_64_pc_windows_gnu=x86_64-w64-mingw32-ar
cargo check --target x86_64-pc-windows-gnu -p logagent-server
cargo check --tests --target x86_64-pc-windows-gnu -p logagent-server
```

## 验证

```bash
cargo fmt --check
cargo check
cargo test
```

Server 行为变化必须同步更新本 README、[SPEC.md](./SPEC.md)、相关 `docs/modules/*` 文档和根 [PROGRESS.md](../PROGRESS.md)。
