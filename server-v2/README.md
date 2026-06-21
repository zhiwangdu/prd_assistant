# LogAgent V2 Server

`server-v2/` 是 V2 分支的当前 Server 实现：一个单进程 Python/FastAPI 应用，使用 SQLite WAL、本地 artifact store 和 DB-backed jobs 承载 Log Analysis、Tools、Metadata、System Context、Memory、MCP、Code Evidence 和 Remote Executor。

旧 Rust `server/` crate 已从 V2 分支移除。V2 的权威 API 前缀是 `/api/v2`；少量 `/api/*` alias 仅用于 Native Agent、WebUI 或历史 taskId 语义兼容，新功能应优先接入 `/api/v2`。

## 当前架构

```text
FastAPI app
  -> API key auth
  -> SQLite store (sessions, runs, uploads, artifacts, jobs, cases, metadata)
  -> local workspace/artifact store
  -> background job executor
  -> Analysis Orchestrator
  -> Agent Provider Runtime
       - stub (default)
       - openai_compatible
       - binary
       - claude_code (optional)
  -> task MCP / read-only MCP
  -> evidence modules
       - log analyzer
       - tool runner / fetch / built-in tools
       - metadata
       - skills and system context
       - case memory
       - code evidence
       - remote executor and environment collector
```

V2 Server owns every execution boundary. Agent providers only return structured outcomes; tools, Fetch, SSH/SCP, code search, Metadata reads and Case recall all go through Server-side schemas, allowlists, budgets and approval checks.

## 核心模型

| Model | 说明 |
|-------|------|
| Workspace | WebUI Analyze 的用户工作区。 |
| Session | 一组问题、草稿、上传引用和多次分析 run 的历史。兼容 API 中也承担 task grouping 语义。 |
| Run / Task | 一次分析执行。每个 run 创建独立 workspace 快照和 artifact 集合。 |
| Upload | 上传文件或分片上传结果，可被多个 run 引用。 |
| Artifact | workspace 内的 manifest、grep、tool result、analysis state、final result 等可审计产物。 |
| Evidence | 可被最终结果引用的受控 artifact 片段。 |
| Action | 等待用户回答、等待审批或工具动作的持久化记录。 |
| Job | 后台执行队列记录，支持重启恢复。 |
| Case | 人工确认后的历史经验，当前激活 `memoryType=case`。 |
| Metadata instance | 产品/版本/环境/节点/数据库等诊断上下文快照。 |

## Run 生命周期

1. 用户在 WebUI 创建或选择 Session，填写问题，可选上传文件或选择 Metadata/System Context。
2. Server 从 Session 创建 run/task workspace 快照。
3. 前置阶段复制 raw inputs、解压日志、生成 `manifest.json`、执行初始 `grep_results.json`。
4. Server 固化 `metadata_context.json`、`system_context.json`、`case_context.json` 和可用工具/执行机/代码仓摘要。
5. Analysis Orchestrator 生成 `analysis_package.json` 和 provider prompt。
6. 当前 Agent provider 返回 completed、waiting_for_user、waiting_for_approval 或失败。
7. task MCP tools 按需写入 `log_searches/`、`log_slices/`、`tool_results/`、`metadata_slices/`、`code_evidence/`、`environment_evidence/` 等产物。
8. Server 校验最终答案 schema 和 evidence refs，写入 `result.json` / `result.md`，并生成短 alias。

预算耗尽、动作重复或证据不足属于可解释分析终止，通常生成低置信度结果并进入 `SUCCEEDED`；不可恢复系统错误才进入 `FAILED`。

## Agent Providers

`LOGAGENT_V2_AGENT_PROVIDER` 控制推理后端：

| Provider | 默认 | 行为 |
|----------|------|------|
| `stub` | 是 | 不调用外部模型，生成确定性低置信度结果，适合本地 smoke 和无模型部署。 |
| `openai_compatible` | 否 | 调用 OpenAI-compatible `/chat/completions`，保存稳定 response audit metadata。 |
| `binary` | 否 | 固定调用管理员配置的 `<binary_path> run <prompt>`，用于本地 provider PoC。 |
| `claude_code` | 否 | 调用 Claude Code CLI，通过 task MCP 读取证据和请求能力。 |

Claude Code 不是 V2 默认依赖。只有选择 `claude_code` provider 时，才需要配置 `LOGAGENT_V2_CLAUDE_CODE_PATH` 或兼容的 `LOGAGENT_CLAUDE_CODE_PATH`。

Provider 通用审计产物：

```text
analysis_package.json
agent_request.json
agent_response.json
analysis_state.json
analysis_events.jsonl
```

Claude Code provider 额外产物：

```text
claude_prompt.md
claude_mcp_config.json
claude_session.json
mcp_calls.jsonl
```

## API 概览

公共：

- `GET /health`
- `GET /`

主要受保护 V2 API：

- Workspaces：`/api/v2/workspaces`
- Sessions：`/api/v2/sessions`
- Runs：`/api/v2/runs`
- Tasks compatibility：`/api/v2/tasks`
- Uploads：`/api/v2/uploads`
- Artifacts / result / analysis：`/api/v2/tasks/:task_id/artifacts`、`/api/v2/tasks/:task_id/result`、`/api/v2/tasks/:task_id/analysis`
- Messages / approvals：`/api/v2/tasks/:task_id/messages`、`/api/v2/tasks/:task_id/actions/:action_id/decision`
- Metadata：`/api/v2/metadata/*`
- System Context / Skills：`/api/v2/system-context/*`、`/api/v2/skills/*`
- Tools / Fetch / Executors：`/api/v2/tools`、`/api/v2/tools/runs`、`/api/v2/fetch/*`、`/api/v2/executors/*`
- Cases / Memory：`/api/v2/cases`、`/api/v2/memory/*`
- Settings：`/api/v2/settings/llm`、`/api/v2/settings/agent-backends`、`/api/v2/settings/domain-adapters`
- MCP：`POST /api/v2/mcp/readonly`、`POST /api/v2/mcp/task/:run_id`
- Exports：`GET /api/v2/exports/skills.zip`、`GET /api/v2/exports/tools.zip`

所有受保护接口必须携带：

```text
Authorization: Bearer <api-key>
```

## 配置

核心环境变量：

| Env | 默认 | 说明 |
|-----|------|------|
| `LOGAGENT_V2_API_KEY` | `dev-token`（本地脚本） | API Bearer token。生产必须显式配置。 |
| `LOGAGENT_V2_HOST` | `127.0.0.1` | 监听地址。 |
| `LOGAGENT_V2_PORT` | `50993` | 监听端口。 |
| `LOGAGENT_V2_DATA_DIR` | `/tmp/logagent-v2-local`（本地脚本） | SQLite、uploads、workspaces、metadata、cases。 |
| `LOGAGENT_V2_WEBUI_DIR` | `webui/out` 或 runtime static dir | 静态 WebUI 目录。 |
| `LOGAGENT_V2_AGENT_PROVIDER` | `stub` | `stub`、`openai_compatible`、`binary`、`claude_code`。 |
| `LOGAGENT_V2_AGENT_TIMEOUT_SECONDS` | `120` | 单次 provider 调用超时。 |
| `LOGAGENT_V2_AGENT_MAX_ROUNDS` | `4` | 单 run 最大 provider 轮次。 |
| `LOGAGENT_V2_AGENT_MAX_LLM_CALLS` | `4` | 单 run 最大 provider 调用数。 |
| `LOGAGENT_V2_AGENT_MAX_ACTIONS` | `6` | 单 run 最大工具动作数。 |
| `LOGAGENT_V2_AGENT_MAX_TOTAL_TOKENS` | `200000` | provider usage token 预算。 |
| `LOGAGENT_V2_LLM_BASE_URL` | 无 | OpenAI-compatible base URL。 |
| `LOGAGENT_V2_LLM_API_KEY` | 无 | OpenAI-compatible API Key。 |
| `LOGAGENT_V2_LLM_MODEL` | 无 | OpenAI-compatible model。 |
| `LOGAGENT_V2_AGENT_BINARY_PATH` | 无 | binary provider 可执行文件。 |
| `LOGAGENT_V2_CLAUDE_CODE_PATH` | 无 | Claude Code provider CLI 路径。 |
| `LOGAGENT_CLAUDE_CODE_PATH` | 无 | Claude Code provider 兼容路径变量。 |
| `LOGAGENT_V2_TOOLS_DIR` | 自动探测 | source-built analyzer 和工具目录。 |
| `LOGAGENT_V2_CODE_REPOS_JSON` | 无 | Code Evidence 本地仓库配置。 |
| `LOGAGENT_V2_CODE_WORKTREE_ROOT` | `data_dir/code_worktrees` | Code Evidence detached worktree cache。 |

Fetch、Remote Executor、Huawei OBS/GaussDB package sync 和 source-built analyzer 的详细配置见 `docs/modules/tool-runner/`、`docs/modules/environment-collector/` 和 `deploy/.env.example`。

## 本地运行

构建 WebUI 并启动本地 V2：

```bash
scripts/v2-local.sh build
scripts/v2-local.sh start
scripts/v2-local.sh status
```

默认地址：

```text
http://127.0.0.1:50993/
```

停止或查看日志：

```bash
scripts/v2-local.sh logs
scripts/v2-local.sh stop
```

源码 analyzer 可选构建和 smoke：

```bash
scripts/v2-local.sh build --with-tools
scripts/v2-local.sh smoke-tools
scripts/smoke-source-built-analyzers.sh
```

## Runtime 部署

运行时重建安装：

```bash
deploy/rebuild-v2-install.sh
deploy/logagent-v2ctl.sh start
deploy/logagent-v2ctl.sh status
```

部署脚本会加载 runtime `deploy/.env`，构建 WebUI，安装 Python 依赖，按需构建 `third_party/` source-built analyzers，并把 tools catalog 状态打印到 `status` 输出。

## 验证

文档或配置改动至少运行：

```bash
git diff --check
rg -n "stale provider wording|legacy server wording" README.md SPEC.md server-v2 docs/modules
```

Python Server 改动优先运行：

```bash
server-v2/.venv/bin/python -m ruff check server-v2/logagent_v2 server-v2/tests
server-v2/.venv/bin/python -m pytest -q server-v2/tests
```

WebUI 改动运行：

```bash
cd webui
npm run lint
npm run typecheck
npm run build
```

## 安全边界

- API Key 只通过环境变量或 runtime secret 注入，不写入日志和 artifact。
- 上传解压必须限制在 workspace 内，禁止路径逃逸。
- Tool Runner 只能执行白名单工具。
- Fetch 默认关闭；启用后必须配置 allowlist host 和 32-byte base64 secret key。
- Remote Executor/Environment Collector 默认需要用户审批，并只能执行白名单命令或拉取白名单 file template。
- Code Evidence 只读访问管理员配置的本地仓库和 ref。
- Agent provider 不直接拥有领域工具、SSH/SCP、Fetch、代码仓或 Metadata 的执行能力。
- 不保存模型隐藏思维链，只保存结构化 outcome、简短理由、事件和 evidence refs。

## 相关文档

- [SPEC.md](./SPEC.md)
- [Agent Provider Runtime](../docs/modules/agent-backends/README.md)
- [Analysis Orchestrator](../docs/modules/analysis-agent/README.md)
- [Tool Runner](../docs/modules/tool-runner/README.md)
- [Metadata](../docs/modules/metadata/README.md)
- [System Context](../docs/modules/system-context/README.md)
- [Interfaces](../docs/modules/interfaces/README.md)
- [Deployment](../docs/modules/deployment/README.md)
