# Server Spec

## 目标

Server 是 LogAgent 的任务管理、上传接收、证据流水线和 WEBUI 托管入口。

Server 也是 Analysis Orchestrator、LogAgent MCP tools 和 Claude Code session 的唯一领域执行边界。Analysis Orchestrator、LLM Gateway 和 Claude Code 都不能绕过 Server 直接调用领域工具、SSH、任意文件系统路径或外部工具。

## 当前状态

已实现 Rust/Axum 服务：

- 健康检查
- API Key middleware
- multipart 上传
- multipart 批量上传
- 分片上传
- upload JSON 持久化和重启续传
- task 创建
- Log Analysis Session 创建、草稿更新、上传绑定、run 创建和 timeline
- task JSON 持久化、列表、详情和重启恢复
- semaphore 限制的后台执行
- phase 驱动的可恢复 Executor dispatcher
- TaskContext、Action、EvidenceArtifact 和 EvidenceProvider 公共契约
- Tool Runner MVP 和 `RUN_TOOL` phase
- `PLAN_ANALYSIS` Claude Code session runner、MCP config 生成、等待态和 evidence ref 校验
- `WAITING_FOR_USER` / `WAITING_FOR_APPROVAL` 恢复 API
- Analysis State Store MVP 和 `/api/tasks/:task_id/analysis`
- Claude Code structured outcome / FinalAnswer schema 校验
- LLM Gateway `binary` provider 预留分支，固定调用 `<binary_path> run <prompt>` 并解析 stdout JSON
- runtime LLM output debug 开关和 `/api/debug/llm`
- Settings LLM 诊断接口：`/api/settings/llm`、`/api/settings/llm/models`、`/api/settings/llm/chat`
- Settings Claude Code 诊断接口：`/api/settings/agent-backends`、`/api/settings/agent-backends/:backend_id/test`
- Settings Domain Adapter 摘要接口：`/api/settings/domain-adapters`
- 只读 HTTP MCP：`POST /api/mcp/readonly`
- Skills / Tools 导出下载：`GET /api/exports/skills.zip`、`GET /api/exports/tools.zip`
- Claude Code session 输入/响应产物：`analysis_package.json`、`claude_prompt.md`、`claude_mcp_config.json`、`claude_session.json`、`mcp_calls.jsonl`、`agent_response.json`
- Claude Code session 的 Metadata 按需加载：`analysis_package.json` 只包含 `metadataContextOutline`，任务 MCP `metadata_context` resource 和 `logagent.get_metadata_topology` 返回 outline，`logagent.query_metadata` 按 section/filter/limit/cursor 写入 `metadata_slices/<stable_id>.json`，`logagent.get_metadata_field_types` 从已导入 Metadata Store 查询指定 measurement 的字段类型，`logagent.get_metadata_tag_fields` 只返回该 measurement 的 Tag 字段
- task artifact 查询
- metadata 查询和导入确认
- Skill-backed System Context、Codex-compatible Skill registry、Skill preview、Markdown Skill 导入、按需 MCP reference 读取和 Metadata adapter
- Memory SQLite store、Case Store schema v2 兼容 API、legacy JSON 启动导入、FTS/BM25 + 关键词 fallback 召回、任务确认 Case、手工 Case 创建和 LLM-assisted 文本导入草稿
- Tools API、`tool_run` task 和首个 `pprof_analyzer` 插件
- Remote Executor API、执行机 JSON store、`remote_command_run` task、`EXECUTE_REMOTE_COMMAND` phase 和白名单 SSH 命令执行；默认模板 `smoke_ls_root` 用于低风险 `ls -la /root` smoke
- upload pipeline
- WEBUI 静态托管，目录为 Vite 构建的 `webui/out`

代码结构已整理为单 crate 内部分层目录：`http/` 承载路由，`domain/` 承载公共类型，`stores/` 承载本地 JSON 持久化、Memory SQLite store 和 Case 兼容 facade，`services/` 承载 Log Analyzer、Tool Runner、Metadata、Claude Code Session Runner、Domain Adapter、LLM Gateway 和 Tools 插件等内部能力，`mcp.rs` 承载 LogAgent MCP stdio server，`pipeline/` 承载任务流水线和可恢复 executor，`support/` 承载配置、鉴权、错误和路径安全。

## HTTP 接口

公开接口：

```http
GET /health
GET /
GET /_next/*
```

受保护接口：

```http
POST /api/uploads
POST /api/uploads/batch
POST /api/uploads/init
POST /api/uploads/:upload_id/chunks?offset=<bytes>
POST /api/uploads/:upload_id/complete
POST /api/sessions
GET /api/sessions
GET /api/sessions/:session_id
PATCH /api/sessions/:session_id
POST /api/sessions/:session_id/uploads
DELETE /api/sessions/:session_id/uploads/:upload_id
POST /api/sessions/:session_id/tasks
GET /api/sessions/:session_id/timeline
POST /api/tasks
GET /api/tasks
GET /api/tasks/:task_id
GET /api/tasks/:task_id/analysis
POST /api/tasks/:task_id/messages
POST /api/tasks/:task_id/actions/:action_id/decision
GET /api/tasks/:task_id/artifacts
GET /api/tasks/:task_id/result
GET /api/tools
GET /api/tools/:tool_id
POST /api/tools/:tool_id/runs
GET /api/tools/runs
GET /api/tools/runs/:task_id
GET /api/tools/runs/:task_id/result
GET /api/tools/runs/:task_id/artifacts
GET /api/executors
POST /api/executors
GET /api/executors/:executor_id
PATCH /api/executors/:executor_id
DELETE /api/executors/:executor_id
GET /api/executor-command-templates
GET /api/executor-runs
POST /api/executor-runs
GET /api/executor-runs/:task_id
GET /api/executor-runs/:task_id/result
POST /api/tasks/:task_id/case
POST /api/cases
POST /api/cases/imports
GET /api/cases/imports/:draft_id
PATCH /api/cases/imports/:draft_id
POST /api/cases/imports/:draft_id/messages
POST /api/cases/imports/:draft_id/confirm
GET /api/cases
GET /api/cases/:case_id
PATCH /api/cases/:case_id
GET /api/debug/llm
PUT /api/debug/llm
GET /api/settings/llm
GET /api/settings/llm/models
POST /api/settings/llm/chat
GET /api/settings/agent-backends
POST /api/settings/agent-backends/:backend_id/test
GET /api/settings/domain-adapters
POST /api/mcp/readonly
GET /api/exports/skills.zip
GET /api/exports/tools.zip
GET /api/skills
GET /api/skills/:skill_id
POST /api/skills/imports
POST /api/skills/preview
GET /api/system-context/resources
POST /api/system-context/resources
GET /api/system-context/resources/:context_id
PATCH /api/system-context/resources/:context_id
POST /api/system-context/resources/:context_id/versions
PATCH /api/system-context/resources/:context_id/versions/:version_id
POST /api/system-context/resources/:context_id/versions/:version_id/activate
POST /api/system-context/preview
GET /api/metadata/instances
GET /api/metadata/instances/:instance_id
DELETE /api/metadata/instances/:instance_id
GET /api/metadata/instances/:instance_id/snapshot
POST /api/metadata/instances/:instance_id/refresh
GET /api/metadata/clusters/:cluster_id
GET /api/metadata/clusters/:cluster_id/nodes
POST /api/metadata/snapshots/fetch
POST /api/metadata/imports
POST /api/metadata/imports/fetch
GET /api/metadata/imports/:import_id/preview
POST /api/metadata/imports/:import_id/confirm
```

Metadata 的用户主键为手工输入的 `instanceId`，可选 `remark` 作为用户备注名。`GET /api/metadata/instances` 返回已导入列表、备注名和摘要计数，`GET /api/metadata/instances/:instance_id/snapshot` 返回该实例对应的 openGemini 拓扑快照，`POST /api/metadata/instances/:instance_id/refresh` 使用已保存的 `rawSnapshot` 重新归一化并覆盖当前快照，`DELETE /api/metadata/instances/:instance_id` 删除该实例及其非共享 cluster/node 记录。`POST /api/metadata/snapshots/fetch` 和 `POST /api/metadata/imports/fetch` 接受可选 `remark`，空值不保存，超过 120 个字符返回 `400`。旧 cluster 查询接口保留兼容；WebUI 不再要求用户输入 ClusterID。重复确认导入同一个 `instanceId` 时，Server 必须先清理旧快照再写入新快照，v1 不保留历史版本。

`GET /api/metadata/clusters/:cluster_id` 返回的 cluster 包含：

- `databases`: openGemini `Databases` 的库、默认 RP、保留策略、Measurements schema 和 ShardGroups 摘要。
- `partitionViews`: openGemini `PtView` 的 database partition owner、状态、版本和 RGID。
- `rawSnapshot`: 原始 openGemini `/getdata` JSON。
- 完整 Shard、IndexGroup、Index、MstVersions 和节点连接字段。

Shard 和 Index 的 `owners` 是 PT ID，不是 NodeID。

受保护接口必须携带：

```text
Authorization: Bearer <api-key>
```

## 只读 HTTP MCP 和导出

`POST /api/mcp/readonly` 是面向个人本地 Claude Code 的高级只读入口，与 `logagent-server mcp --task-id ...` 的任务 stdio MCP 分离。HTTP MCP 只读取共享知识，不绑定 task，不读取 task workspace，不启动/恢复 Session，不上传文件，不运行 Tool Runner，不发起审批或远程 SSH/SCP，不修改 Case、Metadata、Skills 或 System Context。

支持 JSON-RPC 方法：

```text
initialize
resources/list
resources/read
tools/list
tools/call
```

只读 resources：

```text
logagent://skills
logagent://skills/{skill_id}
logagent://metadata/instances
logagent://metadata/instances/{instance_id}/snapshot
logagent://cases/recent
logagent://tools/catalog
logagent://domain-adapters
```

只读 tools：

```text
logagent.search_cases
logagent.get_case
logagent.list_skills
logagent.get_skill
logagent.get_skill_reference
logagent.preview_system_context
logagent.list_metadata_instances
logagent.get_metadata_snapshot
logagent.get_metadata_field_types
logagent.get_metadata_tag_fields
logagent.list_tools
logagent.list_domain_adapters
```

工具目录由 `/api/tools`、`logagent://tools/catalog` 和 `logagent.list_tools` 共享同一批 descriptor。每个 descriptor 必须包含 `source`、`tags`、`readOnly`、`editable`、`exportable`、`runnable`、`backend`、`paramsSchema`、`paramsTemplate` 和 `outputViews`。手动配置的外部工具使用 `source=configured`；内置 metadata 工具使用 `source=built_in`，并且必须是只读、不可编辑、不可导出、可通过 `POST /api/tools/:tool_id/runs` 手动运行。当前内置 metadata catalog tools 包括 `logagent.list_metadata_instances`、`logagent.get_metadata_snapshot`、`logagent.get_metadata_field_types` 和 `logagent.get_metadata_tag_fields`。

`logagent.get_metadata_field_types` 参数为必填 `instanceId`、`database`、`measurement`，可选 `retentionPolicy` 和 `field`。`retentionPolicy` 省略时使用 DB 默认 RP；`field` 可为字符串或字符串数组，省略时返回 measurement 全部 fields。返回字段包含 `typ` 和 `typeLabel`，openGemini 枚举码 `0..7` 映射为 `Unknown/Integer/Unsigned/Float/String/Boolean/Tag/Unknown`。

`logagent.get_metadata_tag_fields` 参数为必填 `instanceId`、`database`、`measurement`，可选 `retentionPolicy`，不支持 `field` 参数。它复用 field type 查询的定位和默认 RP 规则，但只返回 `typ=6` / `typeLabel=Tag` 的字段；返回结构仍使用 `fields`、`missingFields=[]` 和 `finalEvidenceAllowed=false`。

`POST /api/skills/imports` 接收 JSON：`skillId`、`name`、`description`、`markdown` 和可选 `filename`。Server 在第一个配置的 `skills.roots` 下创建 `<skillId>/SKILL.md` 和默认 `logagent.json`，默认 manifest 使用 `schemaVersion=1`、`displayName=name`、`taskKinds=["log_analysis"]`、`includeByDefault=false`、`priority=0` 和空 `references`。导入成功后重载整个 Skill Registry，并返回导入后的 Skill detail；重复 `skillId`、非法 ID、空字段、禁用 skills、无可写 root 或非 `.md/.markdown` filename 会返回明确错误。当前版本不支持覆盖已有 Skill，也不支持上传 reference 文件。

`GET /api/exports/skills.zip` 打包 Server 当前索引到的 Skill 普通文件，不跟随 symlink。包内根目录包含 `manifest.json`，每个 Skill 保留相对目录结构并记录 `skillId`、`displayName`、`revision`、`sourceRoot`、`sourcePath` 和文件列表。导入后的 Skill 重载成功后会出现在该导出包中。

`GET /api/exports/tools.zip` 打包当前 enabled 且解析后是普通可执行文件的工具二进制。包结构固定为：

```text
tools-manifest.json
README.md
bin/<tool_id>/<binary_name>
wrappers/<tool_id>.sh
config/examples/<tool_id>.yaml
```

`tools-manifest.json` 记录 `toolId`、display name、configured args、match rules、Server OS/arch、binary filename、sha256、size、packaged/skipped 状态和 skipped reason。缺失、非普通文件、无执行权限或读取失败的工具只在 manifest 中标记 skipped，不让整个下载失败。导出包只包含 enabled 且 `source=configured` 的可执行工具，不包含 `source=built_in` 的内置工具，不包含 API Key、环境变量值、Server 配置原文、workspace 数据或上传文件。

## 数据目录

```text
data_dir/
  uploads/
    upl_xxx.json
    upl_xxx/
      filename.log
  sessions/
    sess_xxx.json
  session_workspaces/
    sess_xxx/
      session_events.jsonl
  tasks/
    task_xxx.json
  executors/
    executor_xxx.json
  memory/
    memory.sqlite
  cases/
    case_xxx.json
  case_imports/
    caseimp_xxx.json
  system_context/
    resources/
      ctx_xxx.json
  workspaces/
    task_xxx/
      raw/
        upl_xxx/
      extracted/
        package_name/
      session_text_input.json
      manifest.json
      grep_results.json
      metadata_context.json
      metadata_slices/
      system_context.json
      skill_references/
        skill_ref_xxx.json
      tool_results/
        act_tool_xxx/
          result.json
          stdout.txt
          stderr.txt
      remote_command/
        result.json
        stdout.txt
        stderr.txt
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

Diagnostic Skills 默认来自仓库内 `skills/`；`POST /api/skills/imports` 会写入第一个配置的 `skills.roots`，目录结构为 `<root>/<skillId>/SKILL.md` 和 `<root>/<skillId>/logagent.json`，不写入 task workspace。

## Upload Store

`UploadRecord` 包含 `schemaVersion`、upload ID、文件名、已接收大小、预期大小、`UPLOADING`/`COMPLETE` 状态、payload 路径和 RFC 3339 时间。

记录使用同目录临时文件加 rename 原子更新：

```text
storage.data_dir/uploads/<upload_id>.json
storage.data_dir/uploads/<upload_id>/<filename>
```

启动时加载全部上传 JSON。损坏 JSON、非法路径、缺失 payload、完成记录大小不一致必须启动失败。未关联记录的孤儿上传目录只记录告警，不自动删除。

小文件 multipart 和批量 multipart 上传必须先完整写入并 flush payload，再创建 `COMPLETE` 记录。记录持久化前会校验 payload 实际大小等于记录大小。

分片只支持顺序追加，chunk offset 必须等于当前已接收大小。完成时实际大小必须等于 init 声明的预期大小。重启时 `UPLOADING` 记录以 payload 实际长度校正进度，可继续从该 offset 上传。

## 当前任务模型与 Pipeline

`AnalysisSessionRecord` 包含 `schemaVersion`、`sessionId`、`title`、`question`、`sourceUrl`、`instanceId`、`nodeId`、兼容保留的 `systemContextIds`、新的 `skillIds`、`uploadIds`、`taskIds`、`activeTaskId`、`status` 和 RFC 3339 时间。Session status 使用 `draft`、`ready`、`running`、`waiting_for_user`、`waiting_for_approval`、`succeeded`、`failed`。

`TaskRecord` 包含 `schemaVersion`、`taskKind`、任务 ID、可选 `alias`、`sessionId`、来源/上传 ID、raw 输入、来源 URL、用户问题、解析后的 instance/cluster/node ID、工具 ID/参数/结果路径、状态、阶段、attempts、错误、metadata/system context/artifact/result 路径和 RFC 3339 时间。`log_analysis` task 必须绑定 Session；`tool_run` task 不绑定 Session。

```text
POST session task
  -> validate Session and referenced UploadRecord[]
  -> validate referenced UploadRecord[] when present
  -> copy raw files into raw/<upload_id>/, or create an empty raw/input snapshot for question-only analysis
  -> persist QUEUED
  -> append taskId / activeTaskId to Session
  -> return 202
background executor
  -> RUNNING / attempts + 1
  -> dispatch persisted phase
  -> EXTRACT: clean/rebuild extracted + manifest
  -> persist SEARCH_LOGS
  -> SEARCH_LOGS: rebuild grep evidence
  -> persist RUN_TOOL
  -> RUN_TOOL: rule-based configured tool actions, writes tool_results
  -> persist PLAN_ANALYSIS
  -> PLAN_ANALYSIS: Claude Code session orchestration
      - refresh analysis_package.json
      - write claude_mcp_config.json
      - start/resume Claude Code with --mcp-config
      - MCP tools write mcp_calls.jsonl and evidence/background artifacts
      - completed: validate final evidence refs, persist result.json/result.md and SUCCEEDED
      - waiting_for_user: persist pendingUserPrompts and WAITING_FOR_USER
      - waiting_for_approval: persist pendingApprovals and WAITING_FOR_APPROVAL
  -> persist GENERATE_RESULT only for compatibility recovery paths
  -> GENERATE_RESULT: LLM Gateway auxiliary call using grep/metadata/tool evidence, with one correction retry for result schema errors
  -> append analysis state/events
  -> write result.json/result.md
  -> generate task alias through LLM Gateway without analysis/timeline events
  -> SUCCEEDED or FAILED
```

`tool_run` 任务通过 `POST /api/tools/:tool_id/runs` 创建，请求可引用已完成的 `uploadIds`，Server 创建 raw snapshot 并从 `RUN_TOOL` phase 启动。只有 descriptor 中 `enabled=true` 且 `runnable=true` 的工具可通过该接口创建手动 run。`pprof_analyzer` 继续直接读取 raw profile；configured command tools 会先执行 extract/search 准备，生成 `extracted/`、`manifest.json` 和 `grep_results.json` 后再按白名单 args 模板运行；内置 metadata tools 可无上传运行并写入 JSON result。`GET /api/tasks` 默认只返回 `log_analysis` 任务，工具运行使用 `/api/tools/runs` 系列接口查询。

`POST /api/sessions/:session_id/tasks` creates a new `log_analysis` task snapshot from the current Session. `POST /api/tasks` remains available for compatibility and tests but now requires `sessionId`. Both paths accept single-file `uploadId`, batch `uploadIds`, or no uploads for question-only analysis at the task creation layer. Every task writes `session_text_input.json` so the dialog text can be cited as `session_text_input.json#question`. Question-only tasks persist `uploadIds=[]` and `inputs=[]`, write an empty `raw/` snapshot, and still generate `manifest.json` / `grep_results.json` with empty file and match lists. Optional `instanceId` / `nodeId` are resolved against Metadata before persistence. `clusterId` remains accepted for compatibility but is deprecated as a user-facing selector. Session `skillIds` are resolved with Metadata product/version/environment, managed Skills with `includeByDefault=true` may auto-match, and the selected Skill summaries plus Metadata adapter are written to `system_context.json` schema v2. Legacy `systemContextIds` are deserialized for old Sessions but no longer inject old non-Metadata resources into new tasks.

`GET /api/sessions/:session_id/timeline` returns a unified time-ordered stream. Session events include session creation, draft update, upload attach/detach, text-only input recording, task creation, Metadata context summary, System Context resource count, Case recall count, and task status changes. Task analysis events include manifest, grep, tool output, Agent backend calls, model decisions, ask_user, approval, environment evidence and final result events. Metadata slice reads, field type lookups and tag field lookups are audited in `mcp_calls.jsonl` and write `metadata_slices/<stable_id>.json` as background context.

`question` 可选，长度不能超过 `llm.max_input_chars / 2`。

Claude Code stdout 必须返回 JSON envelope，Server 优先读取 `structured_output`、`structuredOutput`、`result` 或根对象中的 structured outcome。允许的 outcome 为 `completed`、`waiting_for_user` 和 `waiting_for_approval`。`LOGAGENT_CLAUDE_CODE_PATH` 可直接指向 Claude Code CLI `claude` 二进制；Server 使用 `--print --output-format json --json-schema ... --mcp-config claude_mcp_config.json --strict-mcp-config` 调用，并按 `analysisMode` permission profile 传入 native tool policy。每个 permission profile 都会自动追加 `mcp__logagent__*` 到 Claude CLI `--allowedTools`，因此 `dontAsk` 模式可以自动使用任务 MCP tools；LogAgent 用户审批 API 不会改变 Claude CLI allowlist。Server 将短启动 prompt 写入 `claude_prompt.md` 并通过 stdin 传给 Claude CLI，`analysis_package.json` 必须通过任务 MCP `analysis_package` resource 读取，避免大 prompt 进入 argv 或 stdin；其中 Metadata 只包含 outline/counts，不包含完整 databases、measurements、shards 或 indexes payload。完整 `metadata_context.json` 仍保存在 workspace，Claude 需要细节时调用 `logagent.query_metadata`，参数支持 `section`、`database`、`retentionPolicy`、`measurement`、`nodeId`、`ownerNodeId`、`ptId`、`shardId`、`indexId`、`limit`、`cursor`，返回 bounded `items`、`total`、`nextCursor`、`truncated` 和 `backgroundRef`；需要从指定已导入 Metadata instance 精确查询 field 类型时调用 `logagent.get_metadata_field_types`，参数支持 `instanceId`、`database`、`measurement`、可选 `retentionPolicy` 和可选单个或多个 `field`；只需要该 measurement 全部 Tag 字段时调用 `logagent.get_metadata_tag_fields`，它使用相同的 instance/database/measurement/RP 定位规则并写入 `metadata_slices/tag_fields_<stable_id>.json`。当 `analysis_package.analysisState.finalizeRequested=true` 时，Claude prompt 要求直接返回 `completed`，不能再返回 `waiting_for_user`。最终结果允许引用 `session_text_input.json#question`、`grep_results.json#matches/<index>`、`case_context.json#cases/<index>` 或 `tool_results/<action_id>/result.json#findings/<index>`；System Context、Diagnostic Skills、`skill_references/*` 和 `metadata_slices/*` 只能作为背景，不能作为最终 evidence ref；缺少 `summary` 等核心字段、越界 Case 或越界 finding 会拒绝。Claude CLI 非零退出、超时、stdout 非 JSON、非法 structured output 或非法 evidence ref 都会写入失败的 `agent_response.json` 和 `claude_session.json` 并使任务进入 `FAILED / PLAN_ANALYSIS`。LLM Gateway 仍保留 stub、OpenAI-compatible Chat Completions 和预留 binary provider，用于 Case import、alias 和兼容恢复的 `GENERATE_RESULT` 辅助路径。

成功的 Log Analysis task 在 `result.json` / `result.md` 写入后，会用最终结果、用户问题、manifest 和 Metadata 摘要调用 LLM Gateway 生成短 `alias`。alias 调用不通过 Analysis State Store 的 LLM call event 回调，不写 `analysis_events.jsonl`，也不追加 Session timeline event。alias schema 错误会重试一次；Provider 或 schema 最终失败时，Server 使用最终 summary 或问题文本生成短标题，不让 core task 因命名失败而失败。

`POST /api/tasks/:task_id/messages` 请求：

```json
{
  "questionId": "act_ask_user_xxx",
  "message": "异常发生在 10:00-10:30",
  "resumeMode": "continue",
  "idempotencyKey": "client-generated-key"
}
```

仅 `WAITING_FOR_USER` 任务可调用。`resumeMode` 默认为 `continue`；传 `finalize` 表示用户没有更多补充信息，Server 记录 `user_message_received` event、清理对应 pending prompt、写入 `analysisState.finalizeRequested=true`，将任务恢复为 `QUEUED / PLAN_ANALYSIS` 并重新入队。

`POST /api/tasks/:task_id/actions/:action_id/decision` 请求：

```json
{
  "decision": "approved",
  "reason": "允许只读采集",
  "idempotencyKey": "client-generated-key"
}
```

`decision` 可为 `approved` 或 `rejected`。仅 `WAITING_FOR_APPROVAL` 任务可调用。当前 `approved` 会写入 mock `environment_evidence/<action_id>/result.json`，记录 `approval_decision_recorded` event，将任务恢复为 `QUEUED / PLAN_ANALYSIS` 并重新入队；真实 SSH/SCP 采集在 Environment Collector 阶段替换该 mock 产物。

`GET /api/debug/llm` 和 `PUT /api/debug/llm` 控制当前 Server 进程内的 LLM 输出日志开关。开关默认关闭，重启后不保留。开启后只打印模型 response content 到 Server stderr，不打印 prompt、API Key 或 HTTP headers。

## 运行日志和安全约束

Server 必须在未设置 `RUST_LOG` 时默认启用 `logagent_server=info,tower_http=info`，并把日志写入 stderr。该约束同样适用于 `logagent-server mcp` stdio 子命令，stdout 只能用于 JSON-RPC 响应。

日志必须覆盖以下控制面事件：启动配置加载、AppState 初始化、未完成任务恢复、HTTP request/response/failure 摘要、上传生命周期、Session/Task/Tool run 创建、用户消息恢复、审批恢复、Executor phase 开始/完成/失败、Claude Code session 开始/完成/失败、MCP resource/tool 调用、Tool Runner 执行/复用/超时、Metadata/System Context/Case 写操作。

日志级别：

- `info` 用于成功生命周期事件和状态推进。
- `warn` 用于 4xx/409 请求拒绝、预算耗尽、工具非零退出/超时和可回退问题。
- `error` 用于 5xx、任务 phase 失败、Claude CLI 调用失败和工具启动失败。

日志不得输出 Authorization、API Key、HTTP headers、请求正文、上传内容、Prompt、Claude stdout 或 LLM response content。唯一例外是 `/api/debug/llm` 开关显式开启后，Server 可按既有调试语义输出模型 response content，且仍不得输出 prompt、API Key 或 HTTP headers。

Settings LLM 诊断接口用于 WebUI 验证当前 Provider 连通性。`GET /api/settings/llm` 返回 provider、当前模型、超时、输入/输出限制和配置项是否存在；不返回 API Key、base URL 原文或 binary path。`GET /api/settings/llm/models` 调用当前 Provider 的模型列表能力，OpenAI-compatible Provider 调用 `<base_url>/models`，stub/binary Provider 返回配置模型。`POST /api/settings/llm/chat` 请求体为 `{"message":"..."}`，用当前 Provider 发送一条简单消息。模型列表和消息测试响应使用 `{ok,result,error}`；Provider HTTP、鉴权、限流、网络、超时或 JSON decode 异常写入 `error`。

Settings Claude Code 诊断接口保留 `/api/settings/agent-backends` 路径作为前端兼容入口，但语义是 Claude Code Session Runner 配置摘要。响应返回单一 `claude_code` 后端、默认分析模式、permission profile、超时和输出大小限制；不返回命令路径。`POST /api/settings/agent-backends/:backend_id/test` 使用 `{ok,result,error}` 响应；诊断检查 Claude Code 命令路径存在且是普通文件，不执行命令。

`PLAN_ANALYSIS` 在调用 Claude Code session 前会刷新输入并记录响应：

- `analysis_package.json`：冻结用户问题、任务信息、manifest、grep、Metadata outline、System Context、Case、Tool results 和 analysis state 摘要；不内联完整 `metadata_context.json`。
- `claude_prompt.md`：短启动 prompt，只包含角色、边界、schema 要求和读取 MCP `analysis_package` resource 的指令。
- `claude_mcp_config.json`：声明 LogAgent MCP stdio server 命令、task id、analysis mode 和资源入口。
- `claude_session.json`：记录 Claude session id、analysis mode、permission profile、MCP config path、prompt delivery 和最近 response path。
- `mcp_calls.jsonl`：追加 MCP resource/tool 调用审计。
- `agent_response.json`：Claude Code session 实际响应记录，包含 `runtimeStatus`、`claudeSessionId`、`analysisMode`、`permissionProfile`、`promptDelivery`、`structuredOutput`、usage/cost、`durationMs`、MCP call path、native tool policy 和错误信息。

这些契约产物不包含密钥，不授权外部后端绕过 Server 执行命令、SSH 或写入 LogAgent 状态。成功任务的 `/api/tasks/:task_id/artifacts` 会返回对应 path 和 JSON 内容。

Settings Domain Adapter 接口用于展示领域诊断能力包。`GET /api/settings/domain-adapters` 返回内置 adapter 列表：`opengemini_influxdb` 为 active，`cassandra` 和 `rocksdb` 为 skeleton。

Case import API 用于替代低效的 Case 手工录入表单。`POST /api/cases/imports` 接受 JSON `{text, filename?}` 或 multipart `file`，仅支持粘贴文本和 UTF-8 文本类文件（`.txt/.md/.log/.json/.yaml/.yml/.csv`）；PDF/DOCX 暂不解析。Server 调用 LLM Gateway 输出 `structuredCase`、`missingFields`、`assistantQuestion` 和 `readyToConfirm`，并持久化到 `storage.data_dir/case_imports/<draft_id>.json`。`title`、`symptom`、`rootCause` 和 `solution` 是确认保存的必填字段；缺失时通过 `POST /api/cases/imports/:draft_id/messages` 连续补充。用户可通过 `PATCH /api/cases/imports/:draft_id` 修正草稿，最后用 `POST /api/cases/imports/:draft_id/confirm` 创建 `sourceType=manual` Case。

Memory 当前作为 Server 内部本地知识后端，主库为 `storage.data_dir/memory/memory.sqlite`，第一阶段仅启用 `memoryType=case`。`CaseStore` 仍暴露现有 `/api/cases*` API、`CaseRecord`、`CaseSearchHit` 和 `case_context.json` 结构。启动时 Server 会读取 `storage.data_dir/cases/*.json` 并按 `caseId` idempotent upsert 到 SQLite；旧 JSON 文件不删除，新增和更新 Case 也会同步写 JSON 作为回滚源。SQLite schema 包含 `memory_items`、`memory_chunks` 和 `memory_chunks_fts`；搜索先过滤 `memoryType=case`、`status=active` 和 `enabled`，再使用 FTS/BM25 分数合并关键词重叠分数。若 FTS 创建或查询失败，Server 记录 warning 并回退到关键词重叠召回。

任务文件使用临时文件加 rename 原子替换。Task schema version 4 支持扩展 phase。每次 phase 推进都校验当前持久化 phase，防止陈旧 dispatcher 覆盖状态。

启动时损坏 JSON、未知 phase、`RUNNING` 无 phase 或 `SUCCEEDED` 仍有 phase 必须失败。`RUNNING` 恢复为 `QUEUED` 时保留 phase，重新获得执行许可后 attempts 加一并从该 phase 幂等重跑。全新 `QUEUED` 任务从 `EXTRACT` 开始；终态不恢复。

公共契约包括：

- `TaskContext`
- `AgentAction`：`actionId`、`type`、`reason`、`evidenceRefs`、typed `input`、`risk`、`fingerprint`
- `EvidenceArtifact`：`actionId`、`evidenceType`、workspace 相对路径和裁剪摘要
- `EvidenceProvider`：后续 Tool Runner、Code Evidence 和 Environment Collector 的统一执行接口

证据 artifact 路径必须是 workspace 相对安全路径。

`RUN_TOOL` 当前使用 Server 规则选择工具。规则输入是 `manifest.json` 和 `grep_results.json`，输出与未来 LLM action 相同的 `AgentAction`。规则先按 manifest file pattern 选择输入文件，再按 grep keyword 补充候选；每个工具最多生成 `max_input_files` 个 action。action id 包含工具名和输入文件稳定哈希，保证批量任务中同一工具的不同输入文件写入不同结果目录。每个工具 action 结果写入：

```text
tool_results/<action_id>/
  result.json
  stdout.txt
  stderr.txt
```

工具路径可来自固定 `path` 或 `path_env` 环境变量；固定 `path` 支持 `${ENV}` 展开；启用工具必须解析为绝对路径，禁用工具不读取 `path_env`。工具非零退出、timeout 或 spawn 失败都会生成 `ToolRunRecord`，不直接令任务失败。配置错误、非法 action 或 unsafe path 仍会使任务失败。

当工具 stdout 是 JSON 时，Server 会解析 `summary` 和 `findings` 并写入 `tool_results/<action_id>/result.json`。`findings` 条目包含可选 `severity`、`file`、`line` 和必填 `message`。stdout 不是 JSON 或字段不匹配时不改变工具执行状态，仍保留 stdout/stderr 并使用通用 summary。

真实 `flux_query_analyzer` 适配：

- 源码通过 `third_party/flux` submodule 引用 `git@github.com:zhiwangdu/flux.git` 的 `feature/query-stats` 分支，CLI 入口为 `libflux/flux-core` 的 `query_stats` binary。
- `scripts/build-tools.sh` 构建产物名为 `flux_query_analyzer`；默认输出到 `target/tools/flux_query_analyzer`，设置 `LOGAGENT_WORK_DIR` 时输出到 `$LOGAGENT_WORK_DIR/bin/tools/flux_query_analyzer`，runtime 部署输出到 `$LOGAGENT_APP_DIR/bin/tools/flux_query_analyzer`。
- `examples/server-flux-tool.yaml` 只启用该工具，并通过 `LOGAGENT_TOOL_FLUX_QUERY_ANALYZER` 指向构建产物。
- CLI 参数为 `--input {input_file} --format json --top-k 20 --max-input-lines 100000 --max-error-findings 20`。
- 输入文件应为 Flux 查询 NDJSON/JSONL，每行包含 `time`、`query` 和可选 `duration_ms`。
- stdout JSON 必须包含通用 `summary/findings`，并通过 bounded `topQueries` 和 `parseErrors` 暴露 Top 模板和解析错误摘要；进度输出必须走 stderr。

真实 `influxql_analyzer` 适配：

- 源码通过 `third_party/influxql` submodule 引用 `git@github.com:zhiwangdu/influxql.git` 的 `influxql-analyzer` 分支，CLI 入口为 `cmd/influxql-analyze`。
- `scripts/build-tools.sh` 构建产物名为 `influxql-analyzer`；默认输出到 `target/tools/influxql-analyzer`，设置 `LOGAGENT_WORK_DIR` 时输出到 `$LOGAGENT_WORK_DIR/bin/tools/influxql-analyzer`，runtime 部署输出到 `$LOGAGENT_APP_DIR/bin/tools/influxql-analyzer`。
- `examples/server-influxql-tool.yaml` 只启用该工具，并通过 `LOGAGENT_TOOL_INFLUXQL_ANALYZER` 指向构建产物。
- CLI 参数为 `-input {input_file} -output json -detail-limit 5`。
- 输入文件应为 JSONL 查询日志，每行至少包含 `query`，可选 `timestamp` 或 `time`。
- Report stdout 的 `special_rules` 会生成结构化 findings，例如 `large_limit`、`no_time_filter`、`group_by_high_cardinality_risk`、`meta_query`。
- `parse_errors` 和 `realtime_query` 会生成可引用 findings。

真实 storage analyzer 适配：

- `opengemini_storage_analyzer` 源码通过 `third_party/openGemini` submodule 引用 `openGemini-tools` 分支，CLI 入口为 `app/opengemini-storage-analyzer`；构建产物名为 `opengemini-storage-analyzer`。
- `opengemini_storage_analyzer` 参数为 `--input {input_file} --format json`，用于只读检查 `.tssp`、`.tssp.init` 和 TSI mergeset part 文件/目录。
- `influxdb_storage_analyzer` 源码通过 `third_party/influxdb` submodule 引用 `influxdb-tools` 分支，CLI 入口为 `cmd/influxdb_storage_analyzer`；构建产物名为 `influxdb_storage_analyzer`。
- `influxdb_storage_analyzer` 参数为 `-input {input_file} -kind auto -max-samples 10`，用于只读检查 `.tsm`、`.tsi` 和 `_series` 目录。
- 两个 storage analyzer stdout 都必须包含通用 `summary/findings`，不执行修复或写入输入数据；Tool Runner 只通过白名单路径调用。

Tools `pprof_analyzer` 适配：

- `examples/server-pprof-tool.yaml` 通过 `LOGAGENT_TOOL_PPROF_GO` 指向 Go 可执行文件。
- 接受 `.pprof`、`.prof`、`.profile` 和 `.pb.gz` 上传文件。
- 固定使用 argv 调用 `go tool pprof`，不拼接 shell，不接受 URL source，并将 `PPROF_TMPDIR` 限制到当前 workspace 下。
- 默认运行 `-top`、`-tree` 和 `-raw`，可选运行 `-svg`；`-svg` 失败只写入 warning，不阻断 top/raw 结果。
- 标准结果写入 `tool_results/<action_id>/result.json`，包含 profile type、sample index、total、top 函数表和 top/tree/raw/stderr artifact 路径。

Analysis State Store 当前记录 pipeline 和多轮 `PLAN_ANALYSIS` 决策的审计状态。Server 会写入：

```text
analysis_state.json
analysis_events.jsonl
```

已记录事件包括初始化、manifest evidence、grep evidence、Tool Runner action/evidence、Claude Code session call lifecycle、MCP waiting request、final result 和 failure。`GET /api/tasks/:task_id/analysis` 返回 state 快照和事件列表。

`PLAN_ANALYSIS` 的 Claude Code session 调用必须生成稳定 `agentcall_*` callId，并记录：

- `llm_call_started`（兼容事件名，callKind 为 `agent_backend_decision`）
- `llm_call_completed`（兼容事件名，callKind 为 `agent_backend_decision`）
- Claude structured output schema error 会写入失败的 `agent_response.json` 和 `analysis_failed` event

事件 details 包含 `callId`、`callKind`、`attempt`、backend id、analysis mode 和 Claude session id。CLI 失败或 schema 最终失败时，task error 和 `agent_response.json` 必须包含可关联错误。

## 规划中的调查编排

```text
persist task
  -> initial extract/search
  -> load analysis state
  -> build analysis package and Claude MCP config
  -> start/resume Claude Code session
  -> MCP tools persist evidence or waiting marker
  -> validate final outcome and evidence refs
  -> append event and update state revision
  -> wait or persist final result
```

稳定状态为 `QUEUED`、`RUNNING`、`WAITING_FOR_USER`、`WAITING_FOR_APPROVAL`、`SUCCEEDED`、`FAILED`。执行阶段独立记录，不能用阶段代替可恢复状态。

## 配置

- `server.bind`
- `server.public_base_url`
- `server.max_concurrent_tasks`，默认 2
- `auth.api_keys[].value_env`
- `storage.data_dir`
- `storage.max_upload_bytes`
- `storage.max_chunk_bytes`
- `log_analyzer.keywords`
- `log_analyzer.max_matches`
- `llm.provider`: `stub` / `openai_compatible`
- `llm.base_url_env`
- `llm.api_key_env`
- `llm.model_env`，可选，配置后从环境变量读取模型名并优先于 `llm.model`
- `llm.model`
- `llm.request_timeout_seconds`
- `llm.max_input_chars`
- `llm.max_output_tokens`
- `claude_code.command_path`
- `claude_code.command_path_env`
- `claude_code.default_mode`: `diagnose` / `code_investigation` / `fix`
- `claude_code.max_session_seconds`
- `claude_code.max_output_bytes`
- `claude_code.permission_profiles.<mode>.*`
- `mcp.enabled`
- `mcp.transport`: 当前只支持 `stdio`
- `analysis.max_rounds`，默认 4，非正值按 1
- `analysis.max_llm_calls`，默认 4，非正值按 1
- `analysis.max_actions`，默认 6，非正值按 1
- `analysis.max_repeated_action_fingerprints`，默认 1，非正值按 1
- `embedding.enabled`，默认 `false`
- `embedding.provider`，默认 `openai_compatible`
- `embedding.model`，默认 `text-embedding-3-small`
- `embedding.api_key_env`，预留，默认不读取
- `embedding.store`，默认 `sqlite`
- `tools.<name>.enabled`
- `tools.<name>.path`
- `tools.<name>.path_env`
- `tools.<name>.timeout_seconds`
- `tools.<name>.max_output_bytes`
- `tools.<name>.max_input_files`
- `tools.<name>.args`
- `tools.<name>.match.file_patterns`
- `tools.<name>.match.keywords`

`pprof_analyzer` 复用通用 `tools.pprof_analyzer.*` 配置；该工具的 `path` / `path_env` 必须指向 Go 可执行文件，Server 会固定附加 `tool pprof` 子命令。

## 待实现

- 完善 Claude Code session runner 的用量审计、错误分类、resume 和 mode-specific native tool policy。
- 围绕当前上传、Metadata、Tool Runner、Claude Code MCP、Domain Adapter 和 WebUI 逻辑补齐完整产品闭环，包括稳定任务创建、证据展示、追问/审批交互、结果确认和可复用的本地 smoke 流程。
- 基于真实生产 Flux 查询日志继续扩展输入转换、模板风险规则和 baseline 新模板解释。
- `influxql_analyzer` compare mode 已增强 delta 字段映射，后续根据真实 compare smoke 继续调整。
- 多轮 Analysis Orchestrator 的产品化策略、模型用量和 Provider request id 审计。
- Cassandra 和 RocksDB domain adapter 的真实 fixture、日志模式和工具规则。
- Memory 已完成本地 SQLite schema、legacy JSON Case 导入、Case schema v2 兼容 API、本地 FTS/BM25 召回、任务确认 Case 和手工 Case 创建；任务创建会写入 `case_context.json`，LLM prompt 会包含历史 Case 参考；后续补 embedding/vector 召回和更正式的 analysis evidence bundle。当前开发阶段不兼容旧 v1 Case JSON。
- Code Evidence 和真实 Environment Collector 延后到产品闭环稳定后实现。
- Remote Executor 当前只覆盖 WebUI 显式发起的白名单 SSH 命令；Analysis Agent 审批后的真实 Environment Collector 采集、SCP 文件拉取和多节点采集仍待接入。

## 验收标准

- `cargo fmt --check`、`cargo check`、`cargo test` 通过。
- `scripts/start-local.sh` 能校验环境变量、构建 Server、后台启动、释放 shell job 并等待健康检查；支持真实 LLM、stub 和前台模式。
- `scripts/init-workdir.sh`、`scripts/build-server.sh`、`scripts/build-webui.sh`、`scripts/build-all.sh` 和 `scripts/server-service.sh` 必须在缺少 `LOGAGENT_WORK_DIR` 时失败；设置后能初始化运行目录、安装 Server binary 和 WebUI 静态产物，并通过工作目录内的 pid/log/config/data 管理 Server 启停。
- WEBUI `npm run lint`、`npm run typecheck`、`npm run build` 通过。
- `/health` 正常。
- `/` 从 `webui/out` 返回 WEBUI。
- 上传 sample.log、多个文件或只填写 Session 问题后都能创建 task 并读取 artifacts。
- Metadata 以 `instanceId` 作为用户主键；openGemini 导入必须由用户提供 InstanceID，原始 `ClusterID` 仅作为 `sourceClusterId` 标签保留。ID 自动补全且冲突时拒绝；workspace 保存完整 `metadata_context.json`，artifacts API 返回快照。
- pipeline 重跑保留 Metadata 快照，Claude package/MCP 默认 resource 只包含 Metadata outline/counts；`logagent.query_metadata` 的 limit/cursor/filter 必须生效，非法 section/filter 必须失败；`logagent.get_metadata_field_types` 必须能按指定 instance/database/measurement 查询一个、多个或全部 field 类型；`logagent.get_metadata_tag_fields` 必须返回同一 measurement 下全部 Tag 字段，省略 RP 时使用 DB 默认 RP，且结果不能作为最终 evidence ref。
- Executor 从 `SEARCH_LOGS` 或 `GENERATE_RESULT` 中断恢复时保留 phase、attempts 加一且不退回 `EXTRACT`。
- `RUN_TOOL` 无工具匹配时必须无副作用跳过；有匹配工具时必须生成 `tool_results` 并进入 `GENERATE_RESULT`。
- 规则版 Tool Runner 必须遵守 `max_input_files`，同一工具不同输入文件必须生成不同稳定 action id。
- `GET /api/tasks/:task_id/artifacts` 返回 `textInput` 和 `toolResults`。
- Tool Runner JSON stdout 的 summary/findings 必须进入 `toolResults`；非 JSON stdout 必须保持兼容 fallback。
- 真实 `flux_query_analyzer` stdout JSON 必须被通用 Tool Runner 解析为 `toolResults[].summary/findings`，且 `scripts/smoke-flux-query-analyzer.sh` 能验证 `summary/findings/topQueries`。
- 真实 `influxql_analyzer` Report stdout 必须被转换为 `toolResults[].summary/findings`，且 `large_limit`、`no_time_filter` 等规则可在 artifacts 中查看。
- `opengemini_storage_analyzer` 和 `influxdb_storage_analyzer` 必须通过 `scripts/build-tools.sh` 从 submodule 源码构建，并分别通过 smoke 脚本验证 stdout JSON 的 tool id 和 high severity finding。
- Remote Executor 只能连接已纳管且启用的执行机，只能执行 `remote_execution.commands` 白名单模板；执行结果必须通过 `/api/executor-runs/:task_id/result` 返回 stdout/stderr preview 和 artifact path，且不能出现在 `/api/tasks` 日志分析列表中。
- LLM Prompt 必须包含可裁剪的 Tool Runner summary/findings，并允许最终结果引用有效 tool finding evidence refs。
- `GET /api/tasks/:task_id/analysis` 必须返回 analysis state 和 events；从中间 phase 恢复的旧任务缺少 state 时必须自动生成最小快照继续执行。
- `POST /api/tasks/:task_id/case` 只能保存 `SUCCEEDED` 任务，重复确认同一任务不能生成重复 Case。
- `POST /api/cases` 必须能手工创建 `sourceType=manual` Case，必填标题、现象、根因和解决方案，且不包含 `taskId/sourceResultPath`。
- `GET /api/cases` 必须能通过 Memory 按 FTS/BM25 和关键词 fallback 召回启用 Case，禁用 Case 默认不返回。
- `PATCH /api/cases/:case_id` 必须能更新 Case 文本、产品、版本、环境、InstanceID、NodeID、证据引用和启用状态。
- 新任务 artifacts 必须返回 `caseContext`，LLM prompt 必须包含历史 Case 参考段落且不能要求模型把历史 Case 当作当前证据。
- Claude Code runner 必须能解析合法 structured outcome，并拒绝非法 evidence ref 或未开放 outcome。
- phase 推进必须检查期望阶段，陈旧 dispatcher 不能覆盖较新的任务状态。
- multipart 和分片上传记录在重启后可恢复；未完成上传不能创建 task。
- multipart 小文件和批量上传不能在 payload 未 flush 时持久化 `COMPLETE` 记录。
- 非顺序 chunk、大小超过预期和未达到预期大小的 complete 必须失败。
- 损坏上传 JSON、非法 payload 路径或完成记录大小不一致必须阻止启动。
- mock Claude CLI 模式能单次生成结构化结果并通过 result API 读取。
- 真实 Provider 配置 `llm.model_env` 时，环境变量缺失或模型名为空必须启动失败。
- 真实 Provider 返回纯 JSON 或完整 JSON 代码围栏时可解析，额外自然语言不能被静默忽略。
- 真实 Provider 返回可映射的行号/范围或 `#start-#end` evidence ref 时应规范化为 canonical grep match refs；无法映射时必须失败。
- 批量任务的 manifest `files[].path` 必须带包名目录前缀。
- 无上传问题分析任务必须生成 `session_text_input.json`、空 `manifest.files`、空 `manifest.uploads` 和 `grep_results.totalMatches=0`。
- 受保护接口无 API Key 时返回 401。
- 等待用户或审批的任务可恢复，重复 message/decision/action 不产生重复执行。
- 达到分析预算时能生成带不确定性的结果并正常终止。
- README 和 SPEC 在接口、配置或 pipeline 变更时同步更新。
