# LocalToolHub Spec

## 目标

LocalToolHub 收敛为两个模块的本地工具工作台：

- **dev_selftest**：在 Linux server 上提供 `sync_workspace`、`build`、`deploy`、`run_tests`、`report` 受控 MCP step tools，由外部 MCP 客户端的本地 skill（如 Windows 上的 Claude Code）编排；源码只通过 allowlisted git repo/ref 同步，Windows 端 commit/push 后 ToolHub clone 或 pull，每次 run 有持久工作区 + run history。
- **日志分析**：上传日志包 → 预处理 → 跑一组编译配置好的 analyzer（influxql/flux/openGemini/influxdb/pprof）→ 结构化 findings + artifact + run history。

Server 提供 Web 管理页、工具目录、工具运行、artifact/run history 和 MCP Server。它不把 Claude Code / Codex / LangChain 或模型服务作为默认运行依赖。

## 非目标

- 不做自研通用 Agent / 多轮推理状态机。
- 不做 Server 侧 workflow 编排、skill registry、skill 下载 API 或 runbook 兼容入口。
- 不做企业级日志平台、通用远程运维平台。
- 不引入 PostgreSQL、Redis、Elasticsearch 作为依赖。
- 不自动修改用户代码或远程机器。
- 不再做 fetch / metadata / cases / skills / SSH-SCP executor / 云实例管理等通用本地工具面（已收敛移除）。

## 核心能力

| 能力 | 要求 |
|------|------|
| Tool Catalog | 展示内置工具与配置的 analyzer，含参数 schema、输入约束、输出视图和可用性。 |
| Tool Runner | 执行白名单工具，保存 stdout/stderr/result/artifacts，支持 timeout 和幂等。 |
| Artifact Store | 每次运行都有逻辑路径、下载、预览和审计元数据。 |
| Run History | 工具运行、dev_selftest 运行都进入统一历史。 |
| Log Analyzer | 预处理日志包，生成 manifest、grep/search 和工具输入索引；驱动配置的 analyzer。 |
| Dev Self-Test | git-only sync_workspace/build/deploy(docker)/run_tests/report MCP step tools；完整 workflow 由客户端 skill 编排，docker runner 复用 remote_execution 的 docker 分支。 |
| MCP Server | 暴露 resources/list/read、tools/list/call 给外部客户端。 |
| WebUI | Tools-first 管理页面（Tools / Runs History / MCP / Settings）。 |

## 数据流

```text
WebUI or MCP client
  -> tool / dev_selftest step request
  -> Server validates auth, schema, allowlist, budget and paths
  -> Server executes controlled action
  -> stdout/stderr/raw output parsed into structured result
  -> artifacts and run record persisted
  -> WebUI/MCP returns bounded summary and artifact refs
```

## 状态模型

统一 run 状态：

```text
QUEUED
RUNNING
SUCCEEDED
FAILED
CANCELLED
```

工具工作台不以 `WAITING_FOR_USER` 作为默认分析循环状态；用户输入应体现在显式参数、配置或重新运行。

## API 方向

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
GET /api/settings/*
```

旧 `/api/sessions/*`、`/api/tasks/*` 仅作迁移兼容（如有），不作为新功能入口。不得新增 Server 侧 workflow API、skill 下载 API、自动初始化工作区 API 或 agent loop API。

## MCP 要求

MCP 是外部智能客户端集成入口。MCP tool 调用必须与 WebUI tool run 共享同一 registry、schema、allowlist、timeout、artifact store 和审计逻辑。复杂 workflow 只能存在于客户端 skill 或外部客户端中，Server MCP 只暴露 resources/tools。
`mcp.enabled=false` 时 HTTP `/api/mcp` 和 stdio `mcp-serve` 必须都拒绝服务。

当前保留 `logagent.*` tool id 和 `logagent://` resource URI 作为兼容 namespace，产品显示名使用 LocalToolHub。

资源示例：

```text
logagent://runs/recent
logagent://tools/catalog
```

工具示例：

```text
logagent.preprocess_log_package
logagent.batch_influxql_analysis
logagent.dev_selftest.sync_workspace
logagent.dev_selftest.build
logagent.dev_selftest.deploy
logagent.dev_selftest.run_tests
logagent.dev_selftest.report
logagent.runs.get
logagent.runs.result
# + 配置的 analyzer: pprof_analyzer / influxql_analyzer / flux_query_analyzer /
#   opengemini_storage_analyzer / influxdb_storage_analyzer
```

## 配置

配置文件暂保留 `logagent.yaml`，语义为本地两模块平台：

```yaml
server:
  bind: 127.0.0.1:50992
storage:
  data_dir: ${LOGAGENT_APP_DIR}/data
tools:
  influxql_analyzer:
    enabled: false
    path_env: LOGAGENT_INFLUXQL_ANALYZER_PATH
remote_execution:
  commands: {}      # dev_selftest test suite 引用的命令模板
mcp:
  enabled: true
dev_selftest:
  enabled: false
```

所有 secret 必须通过环境变量引用，不写入配置样例。

## 验收

- Rust checks 通过：`cargo fmt --check`、`cargo check`、`cargo test`。
- WebUI checks 通过：`npm run lint`、`npm run typecheck`、`npm run build`。
- 本机启动后 `/` 打开管理页面，`/health` 返回 ok。
- Tool Catalog 能显示内置工具和 analyzer 可用性。
- 任一工具运行能生成 run record、result 和 artifact。
- MCP `tools/list` 与 WebUI catalog 一致（仅两模块工具）。
- dev_selftest 默认关闭或受 allowlist 控制。
- 日志、artifact、导出包不包含密钥原文。
- README/SPEC/PROGRESS 随行为变化同步更新。
