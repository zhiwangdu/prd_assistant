# Server 方案

## 技术选型

服务端优先使用 Rust 实现。

可选框架：

- Axum
- Actix Web
- Poem

语言优先级：

```text
Rust -> C/C++ -> Go/Python/Java 等
```

如果已有大量 Python 资产，FastAPI 可以作为兼容选项；但新模块默认优先 Rust。

## 职责

Server 是任务管理、分析调度和内部证据能力的统一承载进程。当前 MVP 不把日志解析、工具执行、Metadata、Analysis Orchestrator、Claude Code Session Runner、LogAgent MCP、Domain Adapters、LLM Gateway 或 Memory/Case Store 拆成独立 crate / 服务；这些能力都在 `server` crate 内部分层实现，后续确有独立生命周期或部署需求时再迁出。

负责：

- 上传管理
- Log Analysis Session 管理和恢复
- 任务创建和状态流转
- 完成后生成并持久化面向用户的 task alias
- 编排 Log Analyzer、Tool Runner、Code Evidence、Environment Collector、Analysis Orchestrator、LogAgent MCP 和 Claude Code Session Runner
- 持久化分析上下文、事件、预算、待回答问题和待审批动作
- 校验 Claude Code structured outcome、MCP tool 请求和最终 evidence refs
- 管理模块输出和任务失败原因
- 查询和关联实例、集群、节点元数据
- 管理 Skill-backed System Context、Diagnostic Skill 索引和 Metadata adapter
- 用户消息、动作批准/拒绝和任务恢复
- Memory 存储和 Case 兼容召回
- Claude Code 配置摘要和 dry-run 诊断
- Domain Adapter 能力摘要
- 个人本地 Claude Code 的只读 HTTP MCP 知识入口
- Skills / Tools 包导出下载
- WebUI API

## 内部职责边界

- HTTP/API：请求解析、鉴权、响应封装。
- Pipeline/Executor：任务 phase 调度、幂等恢复、状态推进。
- Stores：upload、task、analysis state 的本地 JSON 持久化，以及 Memory SQLite 索引和 Case 兼容层。
- Services：Log Analyzer、Tool Runner、Metadata、Claude Code Session Runner、Domain Adapter、LLM Gateway、Tools 插件等内部能力。
- Domain：Task、Upload、Action、Evidence、Result 等公共数据结构。
- Support：配置、错误、ID、路径安全和鉴权等支撑代码。

## 代码结构

当前 Server 已按单 crate、内部分层目录拆分：

```text
server/src
  main.rs              # 启动入口
  app.rs               # AppState 组装和任务启动恢复
  http/                # HTTP 路由和 handler
    health.rs
    uploads.rs
    tasks.rs
    tools.rs
    settings.rs
    mcp_readonly.rs
    exports.rs
    metadata.rs
    cases.rs
    debug.rs
  domain/              # DTO / TaskContext / Action / Evidence 公共契约
    models.rs
    contracts.rs
  stores/              # 本地 JSON store、Memory SQLite store 和兼容 facade
    upload_store.rs
    task_store.rs
    case_store.rs
    memory_store.rs
    system_context_store.rs
    analysis_state.rs
  services/            # Server 内部能力实现
    log_analyzer.rs
    agent_backend.rs
    agent_contracts.rs
    domain_adapters.rs
    tool_runner.rs
    tools.rs
    metadata.rs
    skill_registry.rs
    llm_gateway.rs
  pipeline/            # 任务流水线和可恢复 executor
    mod.rs
    executor.rs
  support/             # 配置、鉴权、错误、ID 和路径安全
    config.rs
    auth.rs
    error.rs
    fs_utils.rs
    id.rs
```

后续新增 Code Evidence、Environment Collector 或扩展 Claude Code MCP / Domain Adapter 时，默认继续放在 `services/`、`pipeline/`、`stores/` 或 `domain/` 的对应层内，只有出现明确的独立发布、复用或部署边界时才拆 crate。

- API 层只做请求解析和响应。
- Pipeline 负责任务编排。
- Services 只执行自己的内部能力，不直接改变 task 状态。
- Stores 只负责持久化和状态原子更新。
- 能力设计文档统一归档在 `docs/modules/`，Server 行为变化同步更新本 README / SPEC。

## 任务来源

```text
upload/environment
  -> Session 草稿、问题和可选上传引用
  -> 用户显式创建一次 task run 快照
  -> 基础采集、解压和初始日志证据
  -> Analysis Orchestrator 调查循环
  -> Server 执行安全动作或进入等待状态
  -> 新证据回填并继续分析
  -> final result
  -> silent task alias generation for UI display
```

## 状态流转

```text
QUEUED
RUNNING
WAITING_FOR_USER
WAITING_FOR_APPROVAL
SUCCEEDED
FAILED
```

`RUNNING` 下另存执行阶段，例如 `EXTRACT`、`SEARCH_LOGS`、`PLAN_ANALYSIS` 和 `EXECUTE_ACTION`。等待状态接收用户输入或审批后恢复到 `RUNNING`。

当前 dispatcher 已支持 `EXTRACT`、`SEARCH_LOGS`、`RUN_TOOL`、`PLAN_ANALYSIS` 和 `GENERATE_RESULT`。`PLAN_ANALYSIS` 会生成 `analysis_package.json`、短启动 `claude_prompt.md` 和 `claude_mcp_config.json`，启动或恢复 Claude Code session；Claude 通过 LogAgent MCP resources/tools 读取证据包、请求日志检索、日志切片、领域工具、按需分页 Metadata slice、Case recall、用户追问和审批。`request_user_input` 会进入 `WAITING_FOR_USER`，`request_approval` 会进入 `WAITING_FOR_APPROVAL`。

## 运行日志

Server 使用 `tracing` 输出结构化运行日志。未设置 `RUST_LOG` 时默认启用：

```bash
RUST_LOG=logagent_server=info,tower_http=info
```

日志始终写到 stderr，避免 `logagent-server mcp` stdio 模式污染 JSON-RPC stdout。HTTP trace 会记录 method、URI、status 和 latency；业务日志覆盖上传、Session/Task 创建、用户消息恢复、审批恢复、phase 推进、Claude Code session、MCP 调用、Tool Runner、Metadata、System Context 和 Case 写操作。

日志分级约定：

- `info`：成功的生命周期事件、phase 切换、任务入队/恢复、工具/Claude session 完成摘要。
- `warn`：4xx/409 请求拒绝、预算耗尽、工具非零退出或超时、可回退的 store/search 问题。
- `error`：5xx 响应、任务 phase 失败、Claude CLI 调用失败、工具启动失败。

普通运行日志不打印 Authorization、API Key、HTTP headers、请求正文、上传内容、Prompt 或 LLM 原文输出。LLM response content 只受 `/api/debug/llm` 运行期开关控制，默认关闭。

## 数据目录

```text
/data/logagent
  uploads/
  sessions/
  session_workspaces/
  system_context/
  workspaces/
  tasks/
  cases/
  memory/
    memory.sqlite
  case_imports/
  code_worktrees/
  # 默认 Skill root 为仓库内 skills/；可通过 skills.roots 改为其他 Codex Skill 目录
```

每个 Session 持久化到 `sessions/<session_id>.json`，事件追加到 `session_workspaces/<session_id>/session_events.jsonl`。每个任务持久化到 `tasks/<task_id>.json`。Memory 主索引持久化到 `memory/memory.sqlite`，legacy Case JSON 保留在 `cases/` 作为迁移/回滚源。写入使用同目录临时文件加 rename；启动时任何损坏的 Session/任务 JSON 都会导致 Server 明确启动失败。

任务 workspace：

```text
/data/logagent/workspaces/task_456
  raw/
  extracted/
  collected/
  session_text_input.json
  manifest.json
  error_summary.json
  metadata_context.json
  metadata_slices/
  system_context.json
  skill_references/
  contexts.jsonl
  tool_results/
  code_evidence.json
  environment_evidence.json
  analysis_package.json
  claude_prompt.md
  claude_mcp_config.json
  claude_session.json
  mcp_calls.jsonl
  agent_response.json
  analysis_state.json
  analysis_events.jsonl
  result.json
  result.md
```

## 核心数据

`task` 需要记录：

- `source`: `upload` / `environment`
- `instance_id`: 用户输入或从 Metadata 选择的实例 ID
- `cluster_id`: 关联集群 ID
- `node_id`: 关联节点 ID
- `product`: 软件产品，例如 `influxdb`
- `version`: 用户输入的软件版本
- `question`: 用户问题
- `status`: 当前任务状态
- `phase`: 当前执行阶段
- `analysis_revision`: 当前分析 revision

## API Key

API Key 从统一配置读取，实际值通过环境变量提供。

```yaml
auth:
  api_keys:
    - name: "native-agent"
      value_env: "LOGAGENT_NATIVE_API_KEY"
    - name: "webui"
      value_env: "LOGAGENT_WEB_API_KEY"
```

MVP 要求：

- 启动时检查 env 是否存在。
- API Key 只存 hash 或只保存在进程内，不写入任务日志。
- 后续再支持轮换和多用户权限。

## MVP 运行

本阶段 Server 先实现最小闭环：

- `POST /api/uploads` 接收 Native Agent 上传的 multipart 文件。
- `POST /api/uploads/batch` 接收多个 multipart 文件并返回多个 upload id。
- 每个上传记录原子持久化到 `storage.data_dir/uploads/<upload_id>.json`，Server 重启后可继续使用已完成上传或续传未完成的分片上传。
- `POST /api/sessions` 创建 draft Session。
- `GET /api/sessions` 返回按更新时间倒序的 Session 历史。
- `GET /api/sessions/:session_id` 返回完整 Session。
- `PATCH /api/sessions/:session_id` 更新 title、question、sourceUrl、instanceId、nodeId 或 draft/ready 状态。
- `POST /api/sessions/:session_id/uploads` 把已完成上传附加到 Session。
- `DELETE /api/sessions/:session_id/uploads/:upload_id` 从未运行中的 Session 移除 upload 引用，不删除 upload payload。
- `POST /api/sessions/:session_id/tasks` 按当前 Session 创建新的 Log Analysis task 快照；Session 可以没有上传日志，仅凭问题文本启动分析。
- `GET /api/sessions/:session_id/timeline` 合并 Session events 和该 Session 下 task 的 analysis events。
- `POST /api/mcp/readonly` 提供面向个人本地 Claude Code 的只读 HTTP MCP，支持读取 Skills、Metadata、Case、Tools catalog 和 Domain Adapter 摘要，不操作 Session/task/workspace。
- `GET /api/exports/skills.zip` 打包当前索引的 Skill 普通文件、references 和 `manifest.json`，跳过 symlink 和路径逃逸。
- `GET /api/exports/tools.zip` 打包 enabled 且解析为普通可执行文件的工具二进制、wrapper、示例配置和 `tools-manifest.json`；缺失、非普通文件、不可执行或读取失败的工具标记 skipped，不让下载失败。
- `GET /api/skills`、`GET /api/skills/:skill_id` 和 `POST /api/skills/preview` 管理可选 Diagnostic Skills 和注入预览。
- `GET /api/system-context/resources` 默认只返回 Metadata adapter；旧非 Metadata resources 仍保存在数据目录但不再作为新任务入口。
- `GET /api/settings/llm` 返回当前 LLM Provider 配置摘要，不包含密钥。
- `GET /api/settings/llm/models` 测试当前 LLM Provider 的模型列表接口，返回 `{ok,result,error}` 诊断响应。
- `POST /api/settings/llm/chat` 使用当前 LLM Provider 发送一条简单 user message，返回 `{ok,result,error}` 诊断响应。
- `GET /api/settings/agent-backends` 返回 Claude Code session runner 配置摘要，不返回命令路径。
- `POST /api/settings/agent-backends/:backend_id/test` 执行 dry-run 诊断，检查 Claude Code 命令路径存在且是普通文件。
- `GET /api/settings/domain-adapters` 返回内置领域 adapter 能力摘要。
- `GET /api/system-context/resources/:context_id` / `PATCH /api/system-context/resources/:context_id` 保留旧资源兼容读取和维护。
- `POST /api/system-context/resources/:context_id/versions` 新增资源版本。
- `PATCH /api/system-context/resources/:context_id/versions/:version_id` 更新资源版本。
- `POST /api/system-context/resources/:context_id/versions/:version_id/activate` 激活版本。
- `POST /api/system-context/preview` 保留旧资源预览兼容；新 Skill 预览使用 `POST /api/skills/preview`。
- `POST /api/tasks` 保留兼容/测试入口，但 Log Analysis task 必须提供 `sessionId`。
- `GET /api/tasks` 返回按创建时间倒序的持久化 Log Analysis task run 列表。
- `GET /api/tasks/:task_id` 返回完整 `TaskRecord`。
- `GET /api/tasks/:task_id/artifacts` 读取任务产物。
- `GET /api/tasks/:task_id/result` 读取结构化 LLM 分析结果。
- `POST /api/tasks/:task_id/case` 将成功任务的最终结果人工确认保存为 Case。
- `POST /api/cases` 手工录入不绑定任务的 Case。
- `POST /api/cases/imports` 从粘贴文本或 UTF-8 文本文件创建 Case 导入草稿，并调用 LLM Gateway 整理为结构化 Case。
- `GET /api/cases/imports/:draft_id` 读取 Case 导入草稿。
- `PATCH /api/cases/imports/:draft_id` 保存用户对结构化草稿的手工修正。
- `POST /api/cases/imports/:draft_id/messages` 提交缺失信息回答，并再次调用 LLM Gateway 合并草稿。
- `POST /api/cases/imports/:draft_id/confirm` 确认草稿并保存为 `sourceType=manual` Case。
- `GET /api/cases` 按关键词召回本地 Case。
- `GET /api/cases/:case_id` 读取 Case 详情。
- `PATCH /api/cases/:case_id` 编辑 Case 文本、元信息、证据引用或禁用 Case。
- `POST /api/tasks/:task_id/messages` 接收等待中的用户回答，追加 analysis event，并将任务从 `WAITING_FOR_USER` 恢复为 `QUEUED / PLAN_ANALYSIS`。
- `POST /api/tasks/:task_id/actions/:action_id/decision` 接收等待中的审批批准或拒绝，追加 analysis event，并将任务从 `WAITING_FOR_APPROVAL` 恢复为 `QUEUED / PLAN_ANALYSIS`。
- `GET /api/debug/llm` / `PUT /api/debug/llm` 读取或修改当前进程内的 LLM 输出日志开关。
- 同步解压 `.zip`、`.tar.gz`、`.tgz`、`.tar`，普通 `.log` / `.txt` 直接复制到 `extracted/<文件基名>/`。
- `.tar.gz` / `.tgz` 如果 gzip tar 解压失败，会自动按普通 `.tar` fallback 再尝试一次。
- 创建 Log Analysis task 支持 `uploadId` 单文件、`uploadIds` 批量文件或无上传的文本问题分析，但必须绑定 `sessionId`；有上传时先验证并复制到 workspace raw 快照，无上传时创建空 raw/input 快照，持久化 `QUEUED` 后以 `202 Accepted` 立即返回。
- 每次 Session 创建 task run 时都会写入 task `sessionId`，并把 taskId 追加到 Session `taskIds`，更新 `activeTaskId/status`。
- Task 状态进入 RUNNING、WAITING、SUCCEEDED 或 FAILED 时会同步更新所属 Session，并追加 `task_status_changed` event。
- Task 创建时固化完整 `metadata_context.json`、Skill-backed `system_context.json` 和 `case_context.json`，同时向 Session timeline 追加 Metadata summary、Skill/System Context resource count 和 Case recall count。Claude Code 初始 `analysis_package.json` 和任务 MCP `metadata_context` resource 只提供 Metadata outline/counts，细节必须通过 `logagent.query_metadata` 读取 bounded slice。
- 后台执行器使用 `server.max_concurrent_tasks` 控制并发，默认 2。
- 后台执行器按持久化 phase 循环分派单个幂等 handler；每个 handler 成功后使用期望 phase 校验原子推进到下一阶段。
- Server 重启时将 `RUNNING` 重置为 `QUEUED` 但保留 phase，并与已有 `QUEUED` 一起按创建时间恢复；`SUCCEEDED`、`FAILED` 不自动重跑。
- 仅从 `EXTRACT` 恢复时清理 `extracted/`、`manifest.json`、`grep_results.json`、`result.json` 和 `result.md`；从后续阶段恢复时复用已完成的前置产物。
- `RUNNING` 缺少 phase、`SUCCEEDED` 仍保留 phase 或未知 phase 枚举会使 Server 明确启动失败。
- 小文件和批量 multipart 上传在写完 payload 后会显式 flush 文件，再持久化 `UploadRecord`，避免记录校验时读到未落盘的 0 字节 payload。
- `RUN_TOOL` 阶段按 manifest/grep 对已配置工具生成规则版 `run_tool` action；manifest file pattern 优先，grep keyword 补充候选，每个工具最多选择 `max_input_files` 个输入文件；未匹配或未配置工具时直接进入 `PLAN_ANALYSIS`。
- Tool Runner 只执行 `tools` 白名单中的绝对路径工具，路径可来自固定 `path` 或 `path_env` 环境变量，使用参数数组，不拼接 shell；stdout/stderr/result 写入 `tool_results/<action_id>/`。
- Tools API 支持手动 `tool_run` 任务，复用上传、raw snapshot、TaskStore、后台 Executor、状态轮询和 workspace；`GET /api/tasks` 默认只列出日志分析任务，工具运行从 `/api/tools/runs` 查询。
- `pprof_analyzer` 是首个 Tools 插件，通过配置的 Go 可执行文件运行 `go tool pprof`，生成 top/tree/raw 文本结果，并把 top 输出解析为结构化表格。
- 规则版 Tool Runner action id 使用工具名和输入文件稳定哈希，批量任务中同一工具的不同输入文件会写入不同 `tool_results/<action_id>/`。
- Tool Runner 会从 JSON stdout 中提取 `summary` 和 `findings` 写入 `result.json`；非 JSON stdout 保持可追溯但不会导致任务失败。
- `examples/server-tools.yaml` 提供 `flux_query_analyzer` / `influxql_analyzer` 的环境变量路径模板。
- `examples/server-influxql-tool.yaml` 只启用真实 `influxql_analyzer`，用于本地单工具 smoke；当前固定调用 `/usr/bin/influxql-analyzer`，真实 CLI 参数为 `-input {input_file} -output json -detail-limit 5`。
- 真实 `influxql-analyzer` Report stdout 会标准化成 Tool Runner findings，包括 `large_limit`、`no_time_filter`、`group_by_high_cardinality_risk`、`meta_query`、parse error 和 realtime classification 发现。
- Analysis State Store 写入 `analysis_state.json` 和 `analysis_events.jsonl`，记录 manifest、grep、tool action、Agent backend call started/completed、model decision、final result 和 failure 事件；真实工具未完成时可继续用 mock 工具验证 action/event/evidence 链路。
- task 创建时解析可选 `instanceId` / `nodeId` 并保留 `metadata_context.json`；同时解析 Session 选择的 `skillIds` 和匹配的 managed Skills，固化 `system_context.json` schema v2。旧 `systemContextIds` 和 `clusterId` 请求字段仅兼容解析，pipeline 重跑不清理这些上下文快照。
- task 创建时按用户问题通过 Memory 召回本地已确认 Case，写入 `session_text_input.json` 和 `case_context.json`；artifacts API 返回 `textInput` 和 `caseContext`，Claude MCP resources 会把用户输入作为 `session_text_input.json#question` 证据，并把历史 Case 作为参考上下文。Case 兼容 API 当前使用 schema v2，`sourceType=task` 记录绑定任务结果，`sourceType=manual` 记录由用户手工录入且不包含 `taskId/sourceResultPath`。
- Memory 当前只启用 `memoryType=case`。Server 启动时会把 `storage.data_dir/cases/*.json` idempotent 导入 `storage.data_dir/memory/memory.sqlite`，搜索优先使用 SQLite FTS/BM25 并合并关键词重叠分数；FTS 不可用时回退到关键词重叠。旧 JSON 文件不会被删除，新增和更新 Case 仍同步写 JSON 作为回滚源。
- Memory 管理页的手工录入路径已升级为 LLM-assisted import：Server 将未确认草稿持久化到 `storage.data_dir/case_imports`，只支持粘贴文本和 UTF-8 文本类文件，PDF/DOCX 暂不解析；缺少 `title`、`symptom`、`rootCause` 或 `solution` 时通过连续对话补齐。
- LLM Gateway 支持 `stub`、OpenAI-compatible Chat Completions 和预留 `binary` provider；binary provider 固定调用 `<binary_path> run <prompt>`，stdout 复用现有结构化 JSON/schema/evidence 校验。
- 未关联 TaskRecord 的 workspace 只记录告警，不自动删除。
- 递归扫描文本行，按配置关键词做简单 grep。
- `RUN_TOOL` 后进入 `PLAN_ANALYSIS`。分析前会刷新 `analysis_package.json`、`claude_prompt.md` 和 `claude_mcp_config.json`，随后调用 Claude Code CLI，并把 session 信息写入 `claude_session.json`、MCP 调用写入 `mcp_calls.jsonl`、真实响应写入 `agent_response.json`。`LOGAGENT_CLAUDE_CODE_PATH` 可直接指向 Claude Code CLI `claude` 二进制；Server 会使用 `--print --output-format json --json-schema ... --mcp-config ... --strict-mcp-config` 调用并解析 CLI envelope。Server 自动注入 `--allowedTools mcp__logagent__*`，保证 `dontAsk` 模式下任务 MCP tools 可直接使用；`diagnose` 仍通过 `--tools ""` 禁用 native tools。证据包不再内联到 CLI 参数或 stdin；Claude 通过任务 MCP `analysis_package` resource 读取，且 package 内的 Metadata 只保留 `metadataContextOutline`。Claude 输出只能是 `completed`、`waiting_for_user` 或 `waiting_for_approval` structured outcome；完成时直接持久化 `result.json` / `result.md`，等待态复用现有 message/approval API。
- `PLAN_ANALYSIS` 通过 Claude MCP `logagent.request_user_input` 和 `logagent.request_approval` 进入等待态。真实 SSH/SCP 执行器后续仍替换在 Environment Collector 内，且必须走审批。
- `PLAN_ANALYSIS` 在达到 `analysis` 预算或发现重复 action fingerprint 时，不进入 `FAILED`，而是生成低置信度、带终止原因的 `result.json` / `result.md` 并正常结束。
- `GENERATE_RESULT` 仍保留为兼容恢复和非 Agent Loop 辅助路径；Log Analysis 正常运行不再从 `PLAN_ANALYSIS` fallback 到该阶段。`PLAN_ANALYSIS` 的 Claude CLI 或 adapter 非零退出、超时、stdout 非 JSON、非法 action 或非法 evidence ref 都会写入失败的 `agent_response.json` 并使任务进入 `FAILED / PLAN_ANALYSIS`。
- Claude Code final answer evidence refs 会按现有结果校验器校验，允许引用 `session_text_input.json#question`、`grep_results.json#matches/<index>`、`case_context.json#cases/<index>` 和 `tool_results/<action_id>/result.json#findings/<index>`；未知 action、越界 finding、`system_context.json`、`diagnostic_skill` 或 `skill_references/*` final evidence ref 会按 schema 错误处理。
- Claude Code runner 会读取 `structured_output` / `structuredOutput` / `result` envelope 并抽取 structured outcome；缺少 `summary` 等核心字段的完成结果仍会拒绝。
- stub Provider 仅用于 LLM Gateway 辅助能力和自动测试；Log Analysis 开发和 CI 使用 mock `claude` CLI。
- LLM 模型可通过 `llm.model_env` 引用环境变量；未配置时继续使用静态 `llm.model`。
- `llm.provider: "binary"` 时从 `llm.binary_path` 或 `llm.binary_path_env` 读取绝对路径，使用参数数组调用二进制，不拼接 shell，不依赖当前环境存在真实模型二进制。
- OpenAI-compatible 响应可为纯 JSON、完整 JSON Markdown 代码围栏，或包含唯一顶层 JSON object 的自然语言响应；多个 JSON object、无 JSON object 或 schema 不合法时按协议错误处理。
- LLM 解析/schema 错误会返回最新失败原因和上一轮失败原因；Provider HTTP、鉴权、限流和超时错误不重试。
- `PLAN_ANALYSIS` 的真实后端调用会生成 `agentcall_*` callId；Task error、debug 日志、analysis events 和 `agent_response.json` 可用于定位失败轮次。
- LLM 输出日志 debug 开关默认关闭、只保存在 Server 进程内。开启后仅把模型 response content 打印到 Server stderr，不打印 prompt 或 API Key。

Server 的 multipart body limit 使用 `storage.max_upload_bytes`。如果 Native Agent 上传稍大的文件时报：

```text
400 failed to read upload field: Error parsing multipart/form-data request
```

优先检查：

- Server 是否已更新到包含 `DefaultBodyLimit` 的版本。
- `storage.max_upload_bytes` 是否大于上传文件大小。
- 如果经过网关，优先让 Native Agent 使用分片上传，并让 `native_agent.upload_chunk_bytes` 小于网关单请求限制。
- Native Agent 和 Server 是否使用同一份或等价的 `logagent.yaml` 限制。

当前已实现接口：

```http
GET /health
POST /api/uploads
POST /api/uploads/batch
POST /api/uploads/init
POST /api/uploads/:upload_id/chunks?offset=<bytes>
POST /api/uploads/:upload_id/complete
POST /api/tasks
GET /api/tasks
GET /api/tasks/:task_id
GET /api/tasks/:task_id/analysis
POST /api/tasks/:task_id/messages
POST /api/tasks/:task_id/actions/:action_id/decision
POST /api/tasks/:task_id/case
GET /api/tasks/:task_id/artifacts
GET /api/tasks/:task_id/result
GET /api/tools
GET /api/tools/:tool_id
POST /api/tools/:tool_id/runs
GET /api/tools/runs
GET /api/tools/runs/:task_id
GET /api/tools/runs/:task_id/result
GET /api/tools/runs/:task_id/artifacts
POST /api/cases
GET /api/cases
GET /api/cases/:case_id
PATCH /api/cases/:case_id
GET /api/debug/llm
PUT /api/debug/llm
GET /api/metadata/instances
GET /api/metadata/instances/:instance_id
GET /api/metadata/instances/:instance_id/snapshot
GET /api/metadata/clusters/:cluster_id
GET /api/metadata/clusters/:cluster_id/nodes
POST /api/metadata/snapshots/fetch
POST /api/metadata/imports
POST /api/metadata/imports/fetch
GET /api/metadata/imports/:import_id/preview
POST /api/metadata/imports/:import_id/confirm
```

analysis 响应可在任务存在后读取 `analysis_state.json` 和 `analysis_events.jsonl`。`PLAN_ANALYSIS` 会写入 Claude Code call started/completed、MCP waiting request 和 final result 事件，事件 details 包含 `callId`、`callKind`、`attempt`、analysis mode 和 session id。`WAITING_FOR_USER` 时 `state.pendingUserPrompts[]` 包含 `questionId`、`question`、`reason`、`required` 和 `answerFormat`；`WAITING_FOR_APPROVAL` 时 `state.pendingApprovals[]` 包含 `actionId`、`actionType`、`reason`、`risk`、`input` 和 `evidenceRefs`。artifacts 响应在成功日志分析任务中包含 `textInput`、`caseContext`、`analysisPackage`、`claudeMcpConfig`、`claudeSession`、`mcpCalls`、`agentResponse` 和 `toolResults`；`textInput` 来自 `session_text_input.json`，记录任务创建时的用户输入；`caseContext` 来自 `case_context.json`，记录任务创建时召回的历史 Case；`analysisPackage`、`claudeMcpConfig`、`claudeSession`、`mcpCalls` 和 `agentResponse` 分别来自 Claude Code session 输入、MCP 配置、session resume 状态、MCP 调用审计和真实响应文件；`agentResponse` 包含 runtime status、prompt delivery、structured output、usage/cost、耗时、native tool policy 和错误；`toolResults` 每项来自 `tool_results/<action_id>/result.json`。`toolResults[].findings` 是结构化工具发现，当前包含可选 `severity`、`file`、`line` 和必填 `message`。真实 `influxql_analyzer` findings 由 Report stdout 中的 `special_rules`、`parse_errors`、`realtime_query` 和命中规则的 fingerprint 生成。Tools 运行的 `result` 通过 `/api/tools/runs/:task_id/result` 读取，首版 `pprof_analyzer` 返回 profile type、sample index、total、top 函数表和 top/tree/raw/stderr artifact 路径。

message 和 approval decision 支持 `idempotencyKey`，重复提交同一 key 不会重复写入用户消息或审批决定。客户端不能直接把任务状态改成 `RUNNING`；只能通过上述 API 恢复等待任务。

Metadata 的用户主键是手工输入的 `instanceId`，可选 `remark` 作为用户备注名。`GET /api/metadata/instances` 返回已导入实例列表、备注名及节点、数据库和 PT view 计数；`GET /api/metadata/instances/:instance_id/snapshot` 返回该实例对应的拓扑快照。旧 `cluster` 查询接口保留为兼容和内部拓扑排查用途，不再作为 WebUI 主入口。

`POST /api/metadata/snapshots/fetch` 只读拉取实时 `/getdata`，请求必须提供 `instanceId`，可选 `remark` 最长 120 个字符。Server 使用该 InstanceID 作为 store 唯一键和内部 snapshot key，原始 openGemini `ClusterID` 仅保存在 `labels.sourceClusterId`。响应返回 instance、备注名、完整节点字段、Raw JSON、Shard、IndexGroup、Index 和 MstVersions。Shard/Index `Owners` 按 PT ID 保存，关系通过 `PtView` 解析为 `Shard -> PT -> DataNode`。

`POST /api/uploads` 使用 multipart：

- `file`: 上传文件
- `filename`: 原始文件名
- `source`: 可选来源标记

Server 会以最终保存的安全文件名为准，并在返回 upload id 前完成 payload flush 和记录持久化。

`POST /api/uploads/batch` 使用 multipart：

- `file` 或 `files`: 可重复出现，每个字段对应一个上传文件

返回：

```json
{
  "uploads": [
    { "uploadId": "upl_1", "filename": "node1.tar.gz", "size": 1024 },
    { "uploadId": "upl_2", "filename": "node2.tar.gz", "size": 2048 }
  ],
  "totalSize": 3072
}
```

大文件建议使用分片接口：

1. `POST /api/uploads/init`

```json
{
  "filename": "large.log",
  "size": 10485760
}
```

2. 多次 `POST /api/uploads/{upload_id}/chunks?offset=<bytes>`，body 为 `application/octet-stream`。
3. `POST /api/uploads/{upload_id}/complete`。

Native Agent 会按 `native_agent.upload_chunk_bytes` 自动选择是否分片。

分片上传状态：

```text
UPLOADING -> COMPLETE
```

- init 记录 `expectedSize`，chunk 记录当前 `size`。
- chunk 的 `offset` 必须等于 Server 已持久化的 `size`，不允许覆盖或跳过字节。
- complete 时 payload 实际大小必须等于 `expectedSize`。
- 引用上传创建 task 时，只有 `COMPLETE` 上传可以被使用；无上传的 Session 仍可创建文本问题分析 run。
- 启动时损坏的上传 JSON、缺失 payload、完成记录大小不一致会使 Server 明确启动失败。
- 如果进程在 payload 写入后、进度 JSON 更新前中断，启动恢复会以 payload 实际大小修正 `UPLOADING` 记录。

`POST /api/tasks` 请求：

```json
{
  "sessionId": "sess_123",
  "uploadId": "upl_123",
  "question": "请分析连接超时的可能原因",
  "instanceId": "i-123",
  "nodeId": "n-1",
  "sourceUrl": "https://logs.example/export/123"
}
```

响应为 `202 Accepted`，包含 `taskId`、`url`、`status`、`phase` 和 `createdAt`。Native Agent 继续使用原有 `taskId`、`url` 字段。

`question` 可选；未提供时使用默认日志分析问题。长度上限为 `llm.max_input_chars / 2`。
如果请求只提供 `sessionId` 和问题、不提供 `uploadId` / `uploadIds`，Server 会创建 `inputs=[]` 的文本问题分析任务；该任务仍会生成 `session_text_input.json`、`manifest.json` 和 `grep_results.json`，但文件列表和 grep matches 为空。

Metadata 选择以 `instanceId` 为主，`nodeId` 可选；旧 `clusterId` 字段仍被 Server 兼容解析但已从 WebUI 弃用。Server 会基于已确认 Metadata 补全关联 ID 并校验一致性，未知或冲突关系返回 `400`。任务详情返回解析后的 ID；成功任务的 artifacts 响应继续包含完整 `metadataContextPath` 和 `metadataContext`，用于 WebUI 和历史兼容。Claude Code 初始上下文不直接接收该完整 payload，只能通过 outline 和 `logagent.query_metadata` 按需读取。

`GET /api/tasks/:task_id/artifacts` 仅允许 `SUCCEEDED`；其他状态返回 `409 Conflict`，JSON 中包含当前 `status`。未知任务返回 `404 Not Found`。

`GET /api/tasks/:task_id/result` 返回 `summary`、`symptoms`、`likelyRootCauses`、`nextChecks`、`fixSuggestions`、`missingInformation` 和 `confidence`。非成功任务返回 `409`。

真实 LLM 运行：

```bash
export LOGAGENT_NATIVE_API_KEY=dev-token
export LOGAGENT_LLM_BASE_URL=https://example.invalid/v1
export LOGAGENT_LLM_API_KEY=replace-me
export LOGAGENT_LLM_MODEL=gpt-4.1
cargo run -p logagent-server -- --config examples/server-llm-openai-compatible.yaml
```

`examples/server-llm-openai-compatible.yaml` 使用 `model_env: "LOGAGENT_LLM_MODEL"`。如果同时配置 `model_env` 和静态 `model`，环境变量值优先；变量缺失或值为空时 Server 启动失败。

pprof Tools 页面本地启动：

```bash
cd webui && npm run build && cd ..
export LOGAGENT_NATIVE_API_KEY=dev-token
export LOGAGENT_TOOL_PPROF_GO="$(command -v go)"
cargo run -p logagent-server -- --config examples/server-pprof-tool.yaml
```

访问 `http://127.0.0.1:50997/`，在 Tools 页面上传 `.pprof`、`.prof`、`.profile` 或 `.pb.gz` 文件即可创建 `tool_run` 任务。

批量任务请求：

```json
{
  "uploadIds": ["upl_123", "upl_456"],
  "sourceUrl": "https://logs.example/export/batch"
}
```

批量任务 workspace 示例：

```text
workspaces/task_xxx/
  raw/
    upl_123/node1.tar.gz
    upl_456/node2.tar.gz
  extracted/
    node1/
    node2/
  manifest.json
  grep_results.json
```

无上传的文本问题分析 workspace 示例：

```text
workspaces/task_xxx/
  raw/
  extracted/
  session_text_input.json
  manifest.json
  grep_results.json
```

本地启动：

```bash
cd webui
npm install --omit=optional
npm run build
cd ..
export LOGAGENT_NATIVE_API_KEY=dev-token
cargo run -p logagent-server -- --config examples/logagent.yaml
```

快速后台启动真实 LLM 配置：

```bash
export LOGAGENT_NATIVE_API_KEY=dev-token
export LOGAGENT_LLM_BASE_URL=https://example.invalid/v1
export LOGAGENT_LLM_API_KEY=replace-me
export LOGAGENT_LLM_MODEL=gpt-4.1
./scripts/start-local.sh
```

脚本默认使用 `examples/server-llm-openai-compatible.yaml` 和端口 `50994`，将 PID 写入 `/tmp/logagent-server-llm.pid`，日志写入 `/tmp/logagent-server-llm.log`，后台启动后会释放 shell job 并等待 `/health` 成功。`--stub` 使用端口 `50992`，`--foreground` 不进入后台。脚本只读取环境变量，不打印或持久化密钥。

工作目录脚本适合本地或测试机长期运行。所有脚本都要求显式设置 `LOGAGENT_WORK_DIR`，未设置会直接失败：

```bash
export LOGAGENT_WORK_DIR=/tmp/logagent-runtime
export LOGAGENT_NATIVE_API_KEY=dev-token

./scripts/init-workdir.sh
./scripts/build-server.sh
./scripts/build-webui.sh
./scripts/server-service.sh start
./scripts/server-service.sh status
./scripts/server-service.sh stop
```

`init-workdir.sh` 会创建 `bin/`、`config/`、`data/`、`logs/`、`run/` 和 `webui/`，并生成 `config/server.yaml`，其中 `storage.data_dir` 指向 `$LOGAGENT_WORK_DIR/data`。`build-server.sh` 安装 release Server binary 到 `$LOGAGENT_WORK_DIR/bin/logagent-server`；`build-webui.sh` 构建并同步 `webui/out` 到 `$LOGAGENT_WORK_DIR/webui/out`；`server-service.sh` 从工作目录启动服务，PID 写入 `run/logagent-server.pid`，日志写入 `logs/logagent-server.log`。

Server 会静态托管 Vite 构建的 `webui/out`，本地访问：

```text
http://127.0.0.1:8080/
```

健康检查：

```bash
curl http://127.0.0.1:8080/health
```

返回：

```json
{"status":"ok"}
```

本地端到端验证：

1. 启动 Server。
2. 启动 Native Agent。
3. 调用 Native Agent 的 `/imports`：

```bash
curl -X POST http://127.0.0.1:17321/imports \
  -H 'Content-Type: application/json' \
  --data '{
    "filePath": "testing/fixtures/downloads/sample.log",
    "filename": "sample.log",
    "sourceUrl": "file://sample.log"
  }'
```

验证输出文件：

```bash
find data/logagent -maxdepth 5 -type f | sort
cat data/logagent/workspaces/<task_id>/manifest.json
cat data/logagent/workspaces/<task_id>/grep_results.json
```

ECS 部署时：

- 先在构建环境执行 `cd webui && npm install --omit=optional && npm run build`。
- 将生成的 `webui/out` 随 Server 一起部署。
- 将 `server.bind` 改为 `0.0.0.0:8080`。
- 将 `server.public_base_url` 改为 ECS 的访问地址。
- 开放安全组入站端口，例如 `8080`。
- 在 ECS 环境变量中设置 `LOGAGENT_NATIVE_API_KEY`。
- Native Agent 配置中的 `server_base_url` 指向 ECS 地址。

推荐生产运行方式：

```bash
export LOGAGENT_WORK_DIR=/opt/logagent
export LOGAGENT_NATIVE_API_KEY=<secret>
./scripts/init-workdir.sh
./scripts/build-all.sh
./scripts/server-service.sh start
```

systemd 可封装 `scripts/server-service.sh start|stop`，但服务环境必须提供 `LOGAGENT_WORK_DIR` 和 `LOGAGENT_NATIVE_API_KEY`。
