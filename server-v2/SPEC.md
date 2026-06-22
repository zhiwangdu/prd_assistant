# LogAgent V2 Server Spec

## 目标

V2 Server 是 LogAgent 的当前服务端实现，负责把用户问题、上传日志、Metadata、System Context、工具结果、代码证据和环境采集结果转换为可审计诊断证据链。

V2 Server 必须提供：

- Session-first Log Analysis。
- 可恢复 run/task pipeline。
- 受控 Agent provider runtime。
- Tool Runner、Fetch、Metadata、Skills/System Context、Case Memory、Code Evidence 和 Remote Executor 能力。
- 只读 HTTP MCP 和 run-scoped task MCP。
- WebUI 静态托管。
- SQLite + 本地 artifact store 的单机部署形态。

旧 Rust `server/` crate 不属于 V2 分支运行面。

## 职责边界

Server 负责：

- API 鉴权、输入校验和状态持久化。
- 上传、分片上传、解压、manifest、grep 和 artifact 管理。
- Analysis Orchestrator 状态机、预算、等待恢复和最终结果校验。
- Tool Runner、Fetch、Code Evidence、Remote Executor 和 Metadata 查询的白名单执行。
- MCP resources/tools 的 schema、run ownership、预算和审计。
- Case Memory 的人工确认和召回。
- WebUI 静态资源托管。

Server 不负责：

- 企业级日志平台或全文检索集群。
- 通用远程运维。
- 任意命令执行。
- 个人本地 Claude Code、Codex 或 OpenCode 的安装和认证管理。
- 保存模型隐藏思维链。

## API 和兼容策略

权威 API 前缀是 `/api/v2`。兼容 alias 只用于已存在的 Native Agent/WebUI/历史 taskId 入口，不作为新功能设计入口。

必须保持：

- 所有受保护 API 使用 `Authorization: Bearer <api-key>`。
- `/health` 无需鉴权。
- `GET /` 托管 WebUI。
- task/run result 在最终结果生成前可返回 409，并携带当前状态。
- `/api/v2/tools` 同时返回 `tools` 和兼容 alias `toolPlugins`。
- run/task scoped API 必须校验 action、artifact 和 workspace 属于当前 run/task。

维护约束：

- 新 endpoint 必须优先放入领域 APIRouter，不应继续扩大单体 `api.py`。
- `/api/*` alias 必须说明兼容对象；新 WebUI 和 Server 内部调用默认使用 `/api/v2/*`。
- 大型请求/响应模型应逐步从路由注册文件拆出，保持路由文件只承担 HTTP 映射、鉴权和错误转换。

## 数据存储

`LOGAGENT_V2_DATA_DIR` 下的关键目录：

```text
logagent-v2.sqlite3
uploads/
workspaces/
metadata/
cases/
code_worktrees/
```

每个 run/task workspace 必须只写入自身目录。artifact path 对外使用 workspace-relative 逻辑路径，不能暴露任意本机路径。

SQLite 必须启用 WAL 或等价单机并发策略。Job 和 run 状态必须支持进程重启后的恢复或安全终止。

Store 连接生命周期要求：

- 每个 `Store` 实例应复用受锁保护的 SQLite connection，避免每次 DAO 操作都 connect/close。
- FastAPI lifespan shutdown 必须关闭 Store connection。
- schema 初始化必须记录当前 `PRAGMA user_version` 和 `schema_migrations` 基线。
- 后续 schema 变化必须以版本化 migration 表达；兼容性 `ALTER TABLE ADD COLUMN` 只能作为迁移实现细节，不能成为无版本补丁堆。

## Run Pipeline

稳定状态：

```text
QUEUED
RUNNING
WAITING_FOR_USER
WAITING_FOR_APPROVAL
SUCCEEDED
FAILED
```

典型阶段：

```text
UPLOAD
COLLECT
EXTRACT
SEARCH_LOGS
RUN_TOOL
COLLECT_CODE
PLAN_ANALYSIS
EXECUTE_ACTION
GENERATE_RESULT
```

Pipeline 要求：

- question-only run 必须可执行，并生成 `session_text_input.json#question`。
- 无上传时仍生成空 manifest 和空 grep artifact。
- 压缩包解压必须防路径逃逸。
- 初始 grep 使用配置化关键词；用户问题拆词不得自动污染初始扫描。
- 后续日志搜索必须写入独立 `log_searches/` artifact，不覆盖初始 `grep_results.json`。
- 每个阶段要么幂等，要么通过 job/run state 防止重复副作用。

## Analysis Orchestrator

Orchestrator 必须持久化：

```text
analysis_state.json
analysis_events.jsonl
analysis_package.json
agent_request.json
agent_response.json
result.json
result.md
```

`analysis_state.json` 至少记录：

- run/task/session id
- 当前 status 和 phase
- 用户问题、补充消息和 finalize intent
- evidence index
- pending user prompt / approval
- 已执行 action fingerprints
- 轮次、provider 调用、动作、token、运行时间、用户追问和审批预算
- `analysisLanguage`

`analysis_events.jsonl` 只保存可审计事件、简短理由和 evidence refs，不保存隐藏思维链。

预算耗尽、重复 action 或证据不足时，应生成 `budgetLimited=true` 的低置信度结果并进入 `SUCCEEDED`。只有不可恢复系统错误进入 `FAILED`。

## Agent Provider Runtime

`LOGAGENT_V2_AGENT_PROVIDER` 支持：

- `stub`
- `openai_compatible`
- `binary`
- `claude_code`

默认必须是 `stub`。Claude Code 仅在 provider 选择为 `claude_code` 时是运行依赖。

Provider contract：

- 输入来自 `analysis_package.json` 和 provider prompt。
- 输出必须是 completed / waiting_for_user / waiting_for_approval / classified failure。
- Provider 不得直接执行领域工具、SSH/SCP、Fetch、代码检索或 Metadata 查询。
- 所有 provider request/response 必须有安全审计 artifact。
- API Key、Authorization header、Cookie、真实 binary path 等敏感信息不得写入 artifact。

Claude Code provider 额外要求：

- CLI path 必须是绝对、常规、可执行文件。
- 大证据包通过 task MCP resource 读取，不进入 argv/stdin。
- permission profile 必须自动允许 `mcp__logagent__*`。
- `diagnose` 默认禁用 native tools。
- `claude_session.json` 必须记录 provider status、session id、resume id、usage/cost、错误和 response artifact id。

## MCP

只读 MCP：

- Endpoint：`POST /api/v2/mcp/readonly`
- 面向个人本地 Claude Code/Codex 等高级入口。
- 只能读取 Skills、Metadata、Case、Tools catalog 和 Domain Adapter 摘要。
- 不得读取或操作 Session/Run，不得上传文件，不得执行 Fetch/SSH/SCP，不得写入 Server 数据。

Task MCP：

- Endpoint：`POST /api/v2/mcp/task/:run_id`
- 仅作用于当前 run。
- 支持 resources/read、tools/list、tools/call 和 JSON-RPC batch。
- 所有 tool call 必须校验 run ownership、schema、allowlist、预算和审批。
- 会产生证据的调用必须写入 workspace artifact 并追加审计事件。

Task MCP resource 主 URI 使用：

```text
logagent://task/<run_id>/<resource>
```

兼容 alias：

```text
logagent-v2://run/<run_id>/<resource>
```

## 证据和引用

最终答案允许引用当前 run 中的可追踪证据：

- `session_text_input.json#question`
- `grep_results.json#matches/<index>`
- `log_searches/<id>.json#matches/<index>`
- `log_slices/<id>.json#lines`
- `tool_results/<action_id>/result.json#findings/<index>`
- `code_evidence/<action_id>.json#matches/<index>`
- `code_evidence/<action_id>.json#diffs/<index>`
- `case_context.json#cases/<index>`，仅作为历史参考

以下只能作为背景，不能作为最终根因 evidence ref：

- `system_context.json`
- Diagnostic Skill reference
- `metadata_slices/*.json`
- Domain Adapter general outline

非法、越界、跨 run 或无法解析的 evidence ref 必须拒绝。

## 能力模块

### Log Analyzer

必须支持 `.log`、`.txt`、`.zip`、`.tar.gz`、`.tgz`、`.tar`。openGemini 节点日志包应按节点、时间和日志组归类，生成 stable manifest、tool input index 和 analyzer-ready 输入。

### Tool Runner

只能执行配置或内置白名单工具。Source-built analyzers 可以来自 `third_party/` 构建结果或 runtime tools dir。Catalog 必须报告存在性、可执行性和不可用原因。

### Fetch

默认关闭。启用时必须配置 allowlist hosts 和 32-byte base64 secret key。Credential set 加密保存；WebUI、API、日志和 artifact 只展示脱敏值。Task MCP 可执行 `logagent.fetch`；只读 MCP 不开放 Fetch 执行。

### Metadata

支持 JSON/YAML/CSV/openGemini 导入。Run 创建时固化 Metadata outline；完整结构保存在 workspace，细节通过 bounded query slice 暴露。

### System Context / Skills

支持 Diagnostic Skills、Markdown Skill、`logagent.json` 匹配 manifest、Skill reference 读取和 Metadata adapter。System Context 是背景，不是当前 run 证据。

### Code Evidence

只允许访问管理员配置的本地 git repo、version/ref 和 search roots。Worktree cache 必须 detached、只读、可清理。Search/diff 结果写入 `code_evidence/`。

### Remote Executor / Environment Collector

只允许配置的 executor 和模板。SSH/SCP 默认需要审批。SCP 文件必须命中白名单 file template 和大小上限。多目标批量采集必须在审批输入中显式确认。

### Memory / Case

当前激活 `memoryType=case`。Case 写入必须来自用户确认；模型或 MCP tool 不能直接写长期 Memory。Case recall 只能作为参考。

## 配置约束

必须支持环境变量优先配置。关键配置：

- `LOGAGENT_V2_API_KEY`
- `LOGAGENT_V2_HOST`
- `LOGAGENT_V2_PORT`
- `LOGAGENT_V2_DATA_DIR`
- `LOGAGENT_V2_WEBUI_DIR`
- `LOGAGENT_V2_AGENT_PROVIDER`
- `LOGAGENT_V2_AGENT_*` budgets
- `LOGAGENT_V2_LLM_*`
- `LOGAGENT_V2_AGENT_BINARY_PATH`
- `LOGAGENT_V2_CLAUDE_CODE_PATH`
- `LOGAGENT_CLAUDE_CODE_PATH`
- `LOGAGENT_V2_TOOLS_DIR`
- `LOGAGENT_V2_CODE_REPOS_JSON`

密钥必须只从环境变量或 runtime secret 读取，不能提交到配置样例、日志或 artifact。

## 安全验收

- API Key 鉴权覆盖所有受保护接口。
- 上传解压不能路径逃逸。
- Artifact path 不暴露任意本机路径。
- Tool Runner、Fetch、Code Evidence、Remote Executor 都有白名单。
- SSH/SCP 默认审批。
- Agent provider 无法绕过 Server 执行能力模块。
- Claude Code provider 默认不要求配置；选择该 provider 后路径校验严格。
- 不保存隐藏思维链。
- Fetch credential 加密保存且脱敏展示。
- 最终结果 evidence refs 必须全部合法。

## 功能验收

- WebUI 可完成 Session 创建、question-only run、上传 run、timeline 查看和 result 查看。
- Native Agent 默认可上传到 V2 Session-scoped endpoint。
- `/api/v2/tools` 返回 runnable tools 和 `toolPlugins` alias。
- source-built analyzers 构建后能在 catalog 中显示可执行状态。
- Metadata 导入后可进入 run context。
- System Context module 能选择/匹配 Skills。
- WAITING_FOR_USER 和 WAITING_FOR_APPROVAL 可恢复。
- budget-limited run 以低置信度结果成功结束。
- 只读 MCP 不写入数据，task MCP 只影响当前 run。
- Server 重启后可恢复 queued/running/waiting jobs 或给出安全终态。
- Store maintenance pytest 覆盖 SQLite 连接复用和 schema version 记录。
