# Testing Spec

## 目标

用低成本测试覆盖 V2 MVP 闭环，优先保证上传、解压、grep、Metadata、Tool Runner、Analysis Orchestrator、Agent Provider Runtime 和 WebUI 依赖接口不会回归。

## 当前状态

已有自动化测试：

- `server-v2/tests/`：pytest 覆盖 Store、schema 初始化、上传、workspace/session/run/task API、artifact、Analysis 状态机、追问/审批、预算终止、Agent provider 解析、Tool Runner、Fetch、Remote Executor、Metadata、System Context、Case Memory 和 Code Evidence。
- `webui/`：lint、typecheck 和 build 覆盖 V2 Analyze、Memory、System Context、Metadata、Tools、Fetch、Executors 和 Settings 的类型/构建正确性。

已有 fixture：

- `downloads/sample.log`
- `redis_timeout`
- `influxql_slow_query`
- `environment_disk_full`

fixture 目前是 smoke/golden seed，不代表完整自动化集成测试已经接入。

## 必跑检查

Server V2 代码改动：

```bash
server-v2/.venv/bin/python -m ruff check server-v2/logagent_v2 server-v2/tests
server-v2/.venv/bin/python -m pytest -q server-v2/tests
```

WebUI 改动：

```bash
cd webui
npm run lint
npm run typecheck
npm run build
```

文档/脚本轻改至少运行：

```bash
git diff --check
```

Rust 检查只适用于仍存在的 Rust 组件改动。V2 Server 不再要求 `cargo fmt --check`、`cargo check` 或 `cargo test`。

## Fixture 要求

新增 fixture 必须包含：

- 原始输入，例如 `logs/`、`tool_outputs/` 或 `collected/`。
- 一个 expected JSON，描述期望关键词、证据类型、严重级别或结构化字段。
- README/SPEC 更新，说明该 fixture 是否已经接入自动化测试。

## 手工验收

V2 产品闭环 smoke：

```bash
./scripts/v2-local.sh build
./scripts/v2-local.sh start
./scripts/v2-local.sh status
```

真实工具 smoke：

```bash
./scripts/v2-local.sh build --with-tools
./scripts/v2-local.sh smoke-tools
./scripts/smoke-source-built-analyzers.sh
```

真实工具 smoke 会通过 `scripts/build-tools.sh` 初始化源码，并支持 `LOGAGENT_SUBMODULE_BASE_URL` 或单仓库 `LOGAGENT_SUBMODULE_*_URL` 指向内网 clone 地址。

## 验收标准

- 任务失败有明确错误原因。
- LLM 输入不会超过配置预算。
- 输出结论只能引用合法 evidence refs。
- 外部工具失败不会导致整个任务无结果，除非工具标记为必需。
- 不保存或快照测试隐藏思维链。
- 自动测试使用 stub/mock/fake，不依赖真实模型、真实 SSH、真实密钥或外网。
