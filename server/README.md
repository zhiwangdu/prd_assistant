# Server

`server/` 是 LogAgent Local Tool/MCP Workbench 的 Rust/Axum 服务端。目标交付形态是一个本地单二进制，托管 WebUI，管理工具目录，执行受控工具，保存 run/artifact，并提供 MCP Server 给外部客户端使用。

## 目标职责

Server 负责：

- API Key 鉴权和本机 HTTP API。
- WebUI 静态托管。
- Tool Catalog 和 Tool Runner。
- 统一 Run History 和 Artifact Store。
- Metadata、Fetch、Executor、Code Evidence、Log Analyzer、Skills/System Context 和 Case Notes。
- MCP resources/tools。
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

旧分析 Agent 模块（`agent_backend.rs`、`llm_gateway.rs`、`analysis_state.rs`、`session_store.rs`、`agent_contracts.rs`、`domain_adapters.rs`、旧 `mcp.rs`、`http/{sessions,tasks,debug,settings}.rs`）已在阶段 5 删除；运行时只剩工具工作台语义（tools / runs / artifacts / metadata / fetch / executors / MCP / cases / system_context）。

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

## 验证

```bash
cargo fmt --check
cargo check
cargo test
```

Server 行为变化必须同步更新本 README、[SPEC.md](./SPEC.md)、相关 `docs/modules/*` 文档和根 [PROGRESS.md](../PROGRESS.md)。
