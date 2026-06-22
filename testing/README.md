# Testing

`testing/` 保存 LogAgent V2 的人工 smoke 输入和后续集成测试 fixture。当前自动化测试主体在 `server-v2/tests/` 和 `webui/` 自身检查中。

## 当前 Fixture

```text
testing/fixtures/
  downloads/
    sample.log
  redis_timeout/
    logs/redis-timeout.log
    expected_error_summary.json
  influxql_slow_query/
    logs/query.log
    tool_outputs/influxql_report.json
    expected_code_keywords.json
  environment_disk_full/
    collected/df.txt
    expected_environment_evidence.json
```

这些 fixture 是最小可用样例，用于手工 smoke、未来 golden test 和文档示例。当前还没有完整的 fixture runner；新增 fixture 时应同时说明期望字段，避免只放原始日志。

## 自动化覆盖

V2 Server 测试位于 `server-v2/tests/`，覆盖：

- SQLite Store、schema 初始化、上传、分片和 artifact。
- Workspace/Session/Run/Task API。
- Analysis Orchestrator 状态、追问、审批、预算和 evidence ref 校验。
- Agent Provider Runtime 的 stub/mock、OpenAI-compatible 解析和错误分类。
- Tool Runner、Fetch、Remote Executor、Metadata、System Context、Case Memory 和 Code Evidence 的核心路径。

WebUI 检查位于 `webui/`，覆盖 TypeScript 类型、lint 和生产构建。

## 必跑检查

V2 Server 修改优先运行：

```bash
server-v2/.venv/bin/python -m ruff check server-v2/logagent_v2 server-v2/tests
server-v2/.venv/bin/python -m pytest -q server-v2/tests
```

WebUI 修改必须运行：

```bash
cd webui
npm run lint
npm run typecheck
npm run build
```

Rust 相关检查只在修改仍存在的 Rust 组件时运行，例如 `native-agent/` 或 `chrome-extension` 配套原生代码；V2 Server 已迁移到 Python/FastAPI，不再要求为 server-v2 改动执行 cargo。

## 手工 Smoke

本地 V2 闭环：

```bash
./scripts/v2-local.sh build
./scripts/v2-local.sh start
./scripts/v2-local.sh status
```

真实 analyzer smoke：

```bash
./scripts/v2-local.sh build --with-tools
./scripts/v2-local.sh smoke-tools
./scripts/smoke-source-built-analyzers.sh
```

内网 clone 地址可通过 `LOGAGENT_SUBMODULE_BASE_URL` 或单仓库 `LOGAGENT_SUBMODULE_*_URL` 覆盖。自动测试不得依赖真实二进制、真实 SSH 节点、真实模型 API Key 或外网。
