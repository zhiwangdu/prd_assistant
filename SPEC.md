# LogAgent MVP Spec

## 目标

LogAgent 把用户问题、日志包、元数据、工具结果、代码证据、历史 Case 和测试环境采集结果转换成可审计证据链。Server 负责证据采集、领域适配、状态、MCP 能力、执行边界和最终结果校验。

产品入口：

- WebUI `Analyze`：团队主入口，负责 Session-first 分析、上传、用户追问、审批、结果查看和 Case 确认。
- 只读 HTTP MCP：个人高级入口，允许本地 Claude Code/Codex 等客户端读取共享 Skills、Metadata、Case、Tools catalog 和 Domain Adapter 摘要；不读取 Session，不写入数据，不执行远程工具。

V2 Server 当前运行入口是 `server-v2/`。旧 Rust `server/` crate 已从 V2 分支删除。

## 技术原则

新实现优先使用 Rust，语言优先级：

```text
Rust -> C/C++ -> Go/Python/Java 等
```

V2 Server 是当前例外：使用 Python/FastAPI、SQLite WAL、本地 artifact store、DB-backed jobs 和 Agent Provider Runtime 评估小团队单机部署形态。不引入 PostgreSQL/Redis，不要求兼容全部 Rust V1 API。

## 组件边界

| 组件 | Spec |
|------|------|
| Chrome Extension | [chrome-extension/SPEC.md](./chrome-extension/SPEC.md) |
| Native Agent | [native-agent/SPEC.md](./native-agent/SPEC.md) |
| Server V2 | [server-v2/SPEC.md](./server-v2/SPEC.md) |
| WebUI | [webui/SPEC.md](./webui/SPEC.md) |
| Testing | [testing/SPEC.md](./testing/SPEC.md) |

Server V2 内部能力：

| 能力 | Spec |
|------|------|
| Agent Provider Runtime | [docs/modules/agent-backends/SPEC.md](./docs/modules/agent-backends/SPEC.md) |
| Analysis Orchestrator | [docs/modules/analysis-agent/SPEC.md](./docs/modules/analysis-agent/SPEC.md) |
| Log Analyzer | [docs/modules/log-analyzer/SPEC.md](./docs/modules/log-analyzer/SPEC.md) |
| Tool Runner / Fetch | [docs/modules/tool-runner/SPEC.md](./docs/modules/tool-runner/SPEC.md) |
| Domain Adapters | [docs/modules/domain-adapters/SPEC.md](./docs/modules/domain-adapters/SPEC.md) |
| Metadata | [docs/modules/metadata/SPEC.md](./docs/modules/metadata/SPEC.md) |
| Skills | [docs/modules/skills/SPEC.md](./docs/modules/skills/SPEC.md) |
| System Context | [docs/modules/system-context/SPEC.md](./docs/modules/system-context/SPEC.md) |
| LLM Gateway | [docs/modules/llm-gateway/SPEC.md](./docs/modules/llm-gateway/SPEC.md) |
| Memory / Case Store | [docs/modules/case-store/SPEC.md](./docs/modules/case-store/SPEC.md) |
| Code Evidence | [docs/modules/code-evidence/SPEC.md](./docs/modules/code-evidence/SPEC.md) |
| Environment Collector | [docs/modules/environment-collector/SPEC.md](./docs/modules/environment-collector/SPEC.md) |
| Config / Interfaces / Security / Deployment / Roadmap | [docs/modules](./docs/modules/README.md) |

## 核心数据流

```text
Chrome Extension -> Native Agent -> V2 upload API -> Session uploads
WebUI -> V2 upload API -> Session uploads
Question-only Session -> explicit run
Session uploads -> explicit run
  -> task workspace snapshot
  -> extract / manifest / initial grep
  -> metadata, system context, case and tool context
  -> Analysis Orchestrator
  -> Agent Provider Runtime
  -> task MCP controlled tools/resources
  -> validated result artifacts
```

Agent Provider Runtime 支持 `stub`、`openai_compatible`、`binary` 和可选 `claude_code`。默认 provider 必须是 `stub`。领域工具、Fetch、Code Evidence、Metadata 查询和远程采集都必须由 Server 执行，provider 只能请求结构化动作或返回结构化结果。

## 状态模型

稳定状态：

```text
QUEUED
RUNNING
WAITING_FOR_USER
WAITING_FOR_APPROVAL
SUCCEEDED
FAILED
```

阶段示例：

```text
COLLECT
EXTRACT
SEARCH_LOGS
RUN_TOOL
COLLECT_CODE
PLAN_ANALYSIS
EXECUTE_ACTION
GENERATE_RESULT
```

预算耗尽、动作重复或证据不足属于可解释终止，应生成低置信度结果并进入 `SUCCEEDED`。不可恢复系统错误进入 `FAILED`。

## 当前已实现

- Chrome Extension 识别下载完成并调用 Native Agent。
- Native Agent 默认上传到 V2 Session-scoped endpoint。
- V2 Server 支持上传、分片上传、Session、Run/Task、artifact、result、analysis、message 和 approval API。
- question-only run 可执行，并生成 `session_text_input.json#question`。
- Log Analyzer 支持文本和常见 archive，openGemini 节点日志包会归类到节点/时间/日志组结构。
- 初始 `grep_results.json` 使用固定关键词；后续 `logagent.search_logs` 写入独立 `log_searches/`。
- Metadata 支持 JSON/YAML/CSV/openGemini 导入，run context 默认只暴露 outline，细节通过 bounded slice 查询。
- Skill-backed System Context 支持 Diagnostic Skills、Markdown Skill、`logagent.json` 匹配、Skill reference 和 Metadata adapter。
- Tool Runner 支持白名单工具、source-built analyzers、Fetch、`pprof_analyzer` 示例和 Huawei package sync 内置工具。
- `/api/v2/tools` 返回 `tools` 和兼容 alias `toolPlugins`，并展示 source-built analyzer 可用性。
- Remote Executor 支持白名单 SSH 命令和审批后的 SCP file template；Environment Collector 支持单目标和 approved `targets[]` 批量采集。
- Code Evidence 支持配置本地 git repo、detached worktree cache、只读 search 和文件级 diff。
- Memory 当前激活 `memoryType=case`，支持人工确认 Case、LLM-assisted import draft 和关键词 fallback 召回。
- Analysis Orchestrator 持久化 `analysis_state.json` / `analysis_events.jsonl`，支持用户追问、审批恢复、预算终止和最终结果校验。
- Agent Provider Runtime 支持 `stub`、OpenAI-compatible、binary 和可选 Claude Code provider；Claude Code 不是默认依赖。
- 只读 HTTP MCP 和 run-scoped task MCP 已接入。
- WebUI 已切到 V2 Analyze、Memory、System Context、Metadata、Tools、Executors 和 Settings。
- `scripts/v2-local.sh`、`deploy/rebuild-v2-install.sh` 和 `deploy/logagent-v2ctl.sh` 支持本地/运行时管理。

## 待实现能力

- 更多真实 openGemini/InfluxDB fixture 和 analyzer 规则验证。
- Cassandra/RocksDB domain adapter 的日志模式、工具和环境模板完善。
- Agent provider runtime 的真实模型配置、错误分类、等待恢复和产品化交互继续收敛。
- Memory embedding provider 或 sqlite-vec/pgvector 增强召回。
- Code Evidence 符号级解析、patch hunk / AST diff 和 fix mode 隔离修改。
- Environment Collector 真实环境 smoke 和生产 fixture 验证。

## 全局安全约束

- API Key 只通过环境变量或 runtime secret 注入。
- 密钥、Cookie、Authorization header 不写入日志、manifest、artifact 或提交历史。
- 上传解压不能逃逸 workspace。
- Tool Runner、Fetch、Code Evidence、Remote Executor 必须有白名单。
- Fetch 默认关闭，启用时必须配置 allowlist 和 credential encryption key。
- SSH/SCP 默认需要用户审批。
- Agent provider 不能绕过 Server 执行工具、读任意任务外路径或访问 SSH。
- System Context、Diagnostic Skill reference、Metadata slice 和历史 Case 不能替代当前任务证据。
- 不保存模型隐藏思维链。

## 全局验收

- Rust checks only apply when modifying remaining Rust components such as
  `native-agent/`: `cargo fmt --check`、`cargo check`、`cargo test`。
- V2 Server：`ruff check` 和相关 pytest。
- WebUI：`npm run lint`、`npm run typecheck`、`npm run build`。
- WebUI 能创建 Session、上传或 question-only run、查看 timeline/result。
- `/api/v2/tools` 能展示 source-built analyzer 注册和可执行状态。
- `WAITING_FOR_USER` / `WAITING_FOR_APPROVAL` 可恢复。
- 最终结果 evidence refs 全部合法。
- 修改后同步更新对应 README/SPEC 和 [PROGRESS.md](./PROGRESS.md)。
