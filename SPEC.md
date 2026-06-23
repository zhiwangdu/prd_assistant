# LocalToolHub Spec

## 目标

LocalToolHub 是面向个人本地部署的运维、开发和测试效率工具。Server 提供 Web 管理页、工具目录、工具运行、artifact/run history、Metadata、Fetch、SSH/SCP Executor、Code Evidence 和 MCP Server。

目标不是自研通用 Agent。外部 Agent 可以作为 MCP client 使用 LocalToolHub；LocalToolHub 不把 Claude Code、Codex、LangChain 或模型服务作为默认运行依赖。

## 非目标

- 不做企业级日志平台。
- 不做通用远程运维平台。
- 不做自研多轮 Agent 后端。
- 不要求共享团队 Server 才能使用。
- 不引入 PostgreSQL、Redis、Elasticsearch 作为 MVP 依赖。
- 不自动修改用户代码或远程机器。

## 核心能力

| 能力 | 要求 |
|------|------|
| Tool Catalog | 展示内置、配置和源码构建工具，含参数 schema、输入约束、输出视图和可用性。 |
| Tool Runner | 执行白名单工具，保存 stdout/stderr/result/artifacts，支持 timeout 和幂等。 |
| Artifact Store | 每次运行都有逻辑路径、下载、预览和审计元数据。 |
| Run History | 工具运行、fetch run、executor run、preprocess run 都进入统一历史。 |
| Metadata | 管理 openGemini/InfluxDB 等实例快照，供 WebUI 和 MCP 查询。 |
| Fetch | 从 cURL 导入 endpoint，保存脱敏配置，加密凭据，受控执行 HTTP 请求。 |
| Executor | 管理 SSH/SCP executor 和命令/文件模板，禁止自由 shell。 |
| Code Evidence | 只读检索本地配置代码仓，输出文件/行号/diff 证据。 |
| Log Analyzer | 预处理日志包，生成 manifest、grep/search 和工具输入索引。 |
| Skills | 管理可复用诊断说明、runbook、工具说明和 MCP 资源。 |
| Case Notes | 保存人工确认的经验记录和关键词/FTS 召回。 |
| MCP Server | 暴露 resources/list/read、tools/list/call 给外部客户端。 |
| WebUI | Tools-first 管理页面，负责配置、运行、查看和导出。 |

## 数据流

```text
WebUI or MCP client
  -> tool/fetch/executor/code/log request
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

可选等待状态只用于需要人审批的远程采集或危险动作：

```text
WAITING_FOR_APPROVAL
```

工具工作台不以 `WAITING_FOR_USER` 作为默认分析循环状态；用户输入应体现在显式参数、配置或重新运行。

## API 方向

保留并强化：

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
GET /api/metadata/*
GET /api/fetch/*
POST /api/fetch/*
GET /api/executors/*
POST /api/executors/*
POST /api/mcp
GET /api/settings/*
```

迁移期兼容但不新增能力：

```http
/api/sessions/*
/api/tasks/*
```

## MCP 要求

MCP 是外部智能客户端集成入口。MCP tool 调用必须与 WebUI tool run 共享同一 registry、schema、allowlist、timeout、artifact store 和审计逻辑。
`mcp.enabled=false` 时 HTTP `/api/mcp` 和 stdio `mcp-serve` 必须都拒绝服务。

当前保留 `logagent.*` tool id 和 `logagent://` resource URI 作为兼容 namespace，产品显示名使用 LocalToolHub。

资源示例：

```text
logagent://tools/catalog
logagent://runs/recent
logagent://metadata/instances
logagent://metadata/instances/<id>/snapshot
logagent://cases/recent
logagent://skills
```

工具示例：

```text
logagent.run_tool
logagent.preprocess_log_package
logagent.search_logs
logagent.fetch
logagent.query_metadata
logagent.search_code
logagent.run_executor_template
logagent.search_cases
```

## 配置

配置文件暂保留 `logagent.yaml`，但语义调整为本地工具平台：

```yaml
server:
  bind: 127.0.0.1:50992
storage:
  data_dir: ${LOGAGENT_APP_DIR}/data
tools:
  directories:
    - ${LOGAGENT_APP_DIR}/bin/tools
fetch:
  enabled: false
executors:
  enabled: false
mcp:
  enabled: true
```

所有 secret 必须通过环境变量引用，不写入配置样例。

## 验收

- Rust checks 通过：`cargo fmt --check`、`cargo check`、`cargo test`。
- WebUI checks 通过：`npm run lint`、`npm run typecheck`、`npm run build`。
- 本机启动后 `/` 打开管理页面，`/health` 返回 ok。
- Tool Catalog 能显示内置和源码构建工具可用性。
- 任一工具运行能生成 run record、result 和 artifact。
- MCP `tools/list` 与 WebUI catalog 一致。
- Fetch/Executor/Code Evidence 默认关闭或受 allowlist 控制。
- 日志、artifact、导出包不包含密钥原文。
- README/SPEC/PROGRESS 随行为变化同步更新。
