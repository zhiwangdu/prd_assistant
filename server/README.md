# Server

`server/` 是 LocalToolHub 的 Rust/Axum 服务端，收敛为两个模块：**dev_selftest**（Linux 跨机自测流水线）+ **日志分析**（预处理 + analyzer）。目标交付形态是本地单二进制，托管 WebUI，管理工具目录，执行受控工具，保存 run/artifact，并提供 MCP Server 给外部客户端使用。

## 目标职责

Server 负责：

- API Key 鉴权和本机 HTTP API。
- WebUI 静态托管。
- Tool Catalog 和 Tool Runner（日志分析 analyzer + 内置工具）。
- dev_selftest 流水线（sync/build/deploy/run_tests/report）+ docker runner。
- 统一 Run History 和 Artifact Store。
- MCP resources/tools；`mcp.enabled=false` 时 `/api/mcp` 与 `mcp-serve` 均拒绝服务。
- 配置加载、路径安全、敏感信息脱敏、timeout、allowlist 和审计。

Server 不负责：

- 自研通用 Agent / 复杂多轮推理状态机。
- 任意 shell、任意 SSH 或任意文件读取。
- fetch / metadata / cases / skills / SSH-SCP executor / 云实例管理（已收敛移除）。

## 目标内部结构

```text
server/src
  main.rs              # parse config, AppState, mount router + ServeDir; mcp-serve subcommand
  app.rs               # AppState god-object (uploads / dev_selftest / tasks / executor / tool_runner)
  http/                # Axum handlers: health, tools, runs, artifacts, uploads
  services/            # tools, tool_runner, log_analyzer, remote_execution (docker runner), dev_selftest
  stores/              # task_store, upload_store, dev_selftest_store (JSON per record)
  pipeline/            # executor.rs (async task runner) + mod.rs (extract/prepare/search)
  domain/              # contracts, models
  mcp_server.rs        # task-free MCP server (stdio + POST /api/mcp)
  support/             # config, auth, error, fs_utils, id, docker_target
```

旧分析 Agent 模块（`agent_backend`/`llm_gateway`/`analysis_state`/`session_store`/`agent_contracts`/`domain_adapters`/旧 `mcp`/`http/{sessions,tasks,debug,settings}`）已在阶段 5 删除；fetch / gemini_db / huawei_package_sync / metadata / cases / system_context / skills / executor_store / http/{fetch,executors,cases,system_context,metadata,skills,exports,mcp_readonly} 在两模块收敛中删除。运行时只剩 dev_selftest + 日志分析 + 共享底座。crate、binary、配置文件和 MCP namespace 仍保留 `logagent` 兼容命名。

## 数据目录

```text
data/
  uploads/
  workspaces/
    task_xxx/          # input, result.json, stdout.txt, stderr.txt, artifacts/
  dev_selftest/
    runs/
      devselftest_xxx/ # source/ artifacts/ logs/ report.md report.json progress.json
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

`/api/runs*`、`/api/artifacts/:artifact_id`、`POST /api/mcp` 已实现；`/api/tools/runs*` 保留为兼容别名。

## Dev Self-Test 配置门控

`dev_selftest.enabled=false` 是真正的关闭态：占位的 `docker.binary` 不会阻断 Server 启动，整组 `logagent.dev_selftest.*` 工具保持禁用。只有 `dev_selftest.enabled=true` 时，docker binary、compose 文件和 build/test profile 才进入严格 allowlist 校验；执行参数仍只能选择配置好的 profile id。

测试套件（`dev_selftest.test_suites.*`）：带 `docker` 块的套件经 executor docker runner（`run_executor_command` 的 `ExecutorTarget::Docker` 分支，`docker run --rm --network host <image> <argv>`）内联派发；无 `docker` 块则走本地桩。`command`（引用 `remote_execution.commands` 模板）与非空 `argv` 互斥。docker target（image/network/workdir/volumes/env）做安全校验；系统 env（`DEVSELFTEST_HOST/PORT` + run 目录 var）最终优先。纳管 executor record 路径已移除，dev_selftest 只用 inline docker。详见 `server/SPEC.md` 与 `deploy/devselftest/opengemini/README.md`。

## 本地运行

```bash
export LOGAGENT_NATIVE_API_KEY=dev-token
cargo run -p logagent-server -- --config examples/server-test.yaml
```

面向 Linux 机器的完整部署流程见
[`deploy/SERVER_DEPLOYMENT.md`](../deploy/SERVER_DEPLOYMENT.md)。

## 平台兼容性 (Linux / Windows)

Server 的非测试代码不依赖任何 Unix-only API，可在 Linux 和 Windows 上编译运行：

- `tokio::signal::ctrl_c`、`tokio::process::Command`、`std::env::temp_dir()` 等均为跨平台 API。
- 所有 `std::os::unix` 调用都在 `#[cfg(unix)]` 守卫下（非测试代码）或位于 `#[cfg(all(test, unix))]` 的测试模块中。
- `examples/logagent.yaml` 的 `tools:` 段声明全部 analyzer，默认 `enabled: false` 并使用 `path_env`，因此无需外部二进制即可加载。启用时把对应 `path_env` 指向绝对二进制路径（Windows 上带 `.exe`）并把 `enabled` 改为 `true`。

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
