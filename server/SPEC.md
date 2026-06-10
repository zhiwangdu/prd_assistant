# Server Spec

## 目标

Server 是 LogAgent 的任务管理、上传接收、证据流水线和 WEBUI 托管入口。

Server 也是 Analysis Agent action 的唯一执行边界。Analysis Agent 和 LLM Gateway 都不能直接调用 shell、SSH、文件系统或外部工具。

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
- `PLAN_ANALYSIS` 多轮 LLM action loop、预算和重复 fingerprint 防护
- `WAITING_FOR_USER` / `WAITING_FOR_APPROVAL` 恢复 API
- Analysis State Store MVP 和 `/api/tasks/:task_id/analysis`
- LLM Gateway ActionDecision / FinalAnswer 双模式 schema
- LLM Gateway `binary` provider 预留分支，固定调用 `<binary_path> run <prompt>` 并解析 stdout JSON
- runtime LLM output debug 开关和 `/api/debug/llm`
- task artifact 查询
- metadata 查询和导入确认
- Case Store schema v2、本地 JSON 召回、任务确认 Case、手工 Case 创建和 LLM-assisted 文本导入草稿
- Tools API、`tool_run` task 和首个 `pprof_analyzer` 插件
- upload pipeline
- WEBUI 静态托管，目录为 Vite 构建的 `webui/out`

代码结构已整理为单 crate 内部分层目录：`http/` 承载路由，`domain/` 承载公共类型，`stores/` 承载本地 JSON 持久化，`services/` 承载 Log Analyzer、Tool Runner、Metadata、LLM Gateway 和 Tools 插件等内部能力，`pipeline/` 承载任务流水线和可恢复 executor，`support/` 承载配置、鉴权、错误和路径安全。HTTP API、配置结构、任务 schema 和 workspace artifact 路径保持不变。

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

Metadata 的用户主键为手工输入的 `instanceId`，可选 `remark` 作为用户备注名。`GET /api/metadata/instances` 返回已导入列表、备注名和摘要计数，`GET /api/metadata/instances/:instance_id/snapshot` 返回该实例对应的 openGemini 拓扑快照。`POST /api/metadata/snapshots/fetch` 和 `POST /api/metadata/imports/fetch` 接受可选 `remark`，空值不保存，超过 120 个字符返回 `400`。旧 cluster 查询接口保留兼容；WebUI 不再要求用户输入 ClusterID。

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
  case_imports/
    caseimp_xxx.json
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
      tool_results/
        act_tool_xxx/
          result.json
          stdout.txt
          stderr.txt
      analysis_state.json
      analysis_events.jsonl
      result.json
      result.md
```

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

`AnalysisSessionRecord` 包含 `schemaVersion`、`sessionId`、`title`、`question`、`sourceUrl`、`instanceId`、`nodeId`、`uploadIds`、`taskIds`、`activeTaskId`、`status` 和 RFC 3339 时间。Session status 使用 `draft`、`ready`、`running`、`waiting_for_user`、`waiting_for_approval`、`succeeded`、`failed`。

`TaskRecord` 包含 `schemaVersion`、`taskKind`、任务 ID、可选 `alias`、`sessionId`、来源/上传 ID、raw 输入、来源 URL、用户问题、解析后的 instance/cluster/node ID、工具 ID/参数/结果路径、状态、阶段、attempts、错误、metadata/artifact/result 路径和 RFC 3339 时间。`log_analysis` task 必须绑定 Session；`tool_run` task 不绑定 Session。

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
  -> PLAN_ANALYSIS: bounded LLM action loop
      - final_answer: persist result.json/result.md and SUCCEEDED
      - search_logs: rebuild grep_results.json from action keywords, then next round
      - run_tool: execute whitelisted tool action, then next round
      - ask_user: persist pendingUserPrompts and WAITING_FOR_USER
      - collect_environment: persist pendingApprovals and WAITING_FOR_APPROVAL
      - budget/repeated fingerprint: persist low-confidence result and SUCCEEDED
  -> persist GENERATE_RESULT
  -> GENERATE_RESULT: LLM Gateway call using grep/metadata/tool evidence, with one correction retry for result schema errors
  -> append analysis state/events
  -> write result.json/result.md
  -> generate task alias through LLM Gateway without analysis/timeline events
  -> SUCCEEDED or FAILED
```

`tool_run` 任务通过 `POST /api/tools/:tool_id/runs` 创建，请求引用已完成的 `uploadIds`，Server 创建 raw snapshot 并从 `RUN_TOOL` phase 启动；首版不执行 `EXTRACT`、`SEARCH_LOGS` 或 LLM 阶段。`GET /api/tasks` 默认只返回 `log_analysis` 任务，工具运行使用 `/api/tools/runs` 系列接口查询。

`POST /api/sessions/:session_id/tasks` creates a new `log_analysis` task snapshot from the current Session. `POST /api/tasks` remains available for compatibility and tests but now requires `sessionId`. Both paths accept single-file `uploadId`, batch `uploadIds`, or no uploads for question-only analysis at the task creation layer. Every task writes `session_text_input.json` so the dialog text can be cited as `session_text_input.json#question`. Question-only tasks persist `uploadIds=[]` and `inputs=[]`, write an empty `raw/` snapshot, and still generate `manifest.json` / `grep_results.json` with empty file and match lists. Optional `instanceId` / `nodeId` are resolved against Metadata before persistence. `clusterId` remains accepted for compatibility but is deprecated as a user-facing selector.

`GET /api/sessions/:session_id/timeline` returns a unified time-ordered stream. Session events include session creation, draft update, upload attach/detach, text-only input recording, task creation, Metadata context summary, Case recall count, and task status changes. Task analysis events include manifest, grep, tool output, LLM calls, model decisions, ask_user, approval, environment evidence and final result events.

`question` 可选，长度不能超过 `llm.max_input_chars / 2`。

LLM Gateway 响应解析接受纯 JSON、完整 JSON Markdown 代码围栏，或包含唯一顶层 JSON object 的自然语言响应。Prompt 包含 session text、grep evidence、Metadata 摘要、历史 Case 摘要和 Tool Runner summary/findings；stdout/stderr 原文不进入 Prompt。`llm.provider` 支持默认 `stub`、OpenAI-compatible Chat Completions，以及预留 `binary` provider；binary provider 只调用配置中的绝对路径二进制，固定 argv 为 `run` 和完整 prompt，stdout 使用同一套 JSON/schema/evidence 校验。`PLAN_ANALYSIS` 的 ActionDecision 当前开放 `search_logs`、`run_tool`、`ask_user`、`collect_environment` 和 `final_answer`；暂不开放 `collect_code_evidence`。`collect_environment` 必须使用 `REQUIRES_APPROVAL` risk。每轮决策前检查 `analysis.max_rounds` / `analysis.max_llm_calls`，每个 action 执行前检查 `analysis.max_actions` 和同一 fingerprint 重复次数。达到预算或重复上限时生成低置信度最终结果并进入 `SUCCEEDED`。可追踪的字符串形式 root cause、`matches/<index>` / `matches/<start>-<end>` 引用别名、`case_<id>` 历史 Case 引用别名、单字符串列表字段、裸最终结果 JSON，以及 `final_answer.result.result` / `answer` / `finalAnswer` 等常见最终结果包裹变体会规范化为正式结果结构。最终结果允许引用 `session_text_input.json#question`、`grep_results.json#matches/<index>`、`case_context.json#cases/<index>` 或 `tool_results/<action_id>/result.json#findings/<index>`；未知 action、缺少 `summary` 等核心字段、越界 Case 或越界 finding 会拒绝。`GENERATE_RESULT` 和 `PLAN_ANALYSIS` 的解析/schema 错误都会追加修正提示并重试一次；多个 JSON object、无 JSON object 或两次 schema 都不合法时任务进入对应 `FAILED` 阶段。Provider HTTP、鉴权、限流、网络和超时错误不重试。

成功的 Log Analysis task 在 `result.json` / `result.md` 写入后，会用最终结果、用户问题、manifest 和 Metadata 摘要调用 LLM Gateway 生成短 `alias`。alias 调用不通过 Analysis State Store 的 LLM call event 回调，不写 `analysis_events.jsonl`，也不追加 Session timeline event。alias schema 错误会重试一次；Provider 或 schema 最终失败时，Server 使用最终 summary 或问题文本生成短标题，不让 core task 因命名失败而失败。

`POST /api/tasks/:task_id/messages` 请求：

```json
{
  "questionId": "act_ask_user_xxx",
  "message": "异常发生在 10:00-10:30",
  "idempotencyKey": "client-generated-key"
}
```

仅 `WAITING_FOR_USER` 任务可调用。Server 记录 `user_message_received` event，清理对应 pending prompt，将任务恢复为 `QUEUED / PLAN_ANALYSIS` 并重新入队。

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

Case import API 用于替代低效的 Case 手工录入表单。`POST /api/cases/imports` 接受 JSON `{text, filename?}` 或 multipart `file`，仅支持粘贴文本和 UTF-8 文本类文件（`.txt/.md/.log/.json/.yaml/.yml/.csv`）；PDF/DOCX 暂不解析。Server 调用 LLM Gateway 输出 `structuredCase`、`missingFields`、`assistantQuestion` 和 `readyToConfirm`，并持久化到 `storage.data_dir/case_imports/<draft_id>.json`。`title`、`symptom`、`rootCause` 和 `solution` 是确认保存的必填字段；缺失时通过 `POST /api/cases/imports/:draft_id/messages` 连续补充。用户可通过 `PATCH /api/cases/imports/:draft_id` 修正草稿，最后用 `POST /api/cases/imports/:draft_id/confirm` 创建 `sourceType=manual` Case。

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

工具路径可来自固定 `path` 或 `path_env` 环境变量；启用工具必须解析为绝对路径，禁用工具不读取 `path_env`。工具非零退出、timeout 或 spawn 失败都会生成 `ToolRunRecord`，不直接令任务失败。配置错误、非法 action 或 unsafe path 仍会使任务失败。

当工具 stdout 是 JSON 时，Server 会解析 `summary` 和 `findings` 并写入 `tool_results/<action_id>/result.json`。`findings` 条目包含可选 `severity`、`file`、`line` 和必填 `message`。stdout 不是 JSON 或字段不匹配时不改变工具执行状态，仍保留 stdout/stderr 并使用通用 summary。

真实 `influxql_analyzer` 适配：

- `examples/server-influxql-tool.yaml` 只启用该工具，当前固定路径为 `/usr/bin/influxql-analyzer`；该路径指向 `/home/duzhiwang/workspace/influxql/influxql-analyzer`。
- CLI 参数为 `-input {input_file} -output json -detail-limit 5`。
- 输入文件应为 JSONL 查询日志，每行至少包含 `query`，可选 `timestamp` 或 `time`。
- Report stdout 的 `special_rules` 会生成结构化 findings，例如 `large_limit`、`no_time_filter`、`group_by_high_cardinality_risk`、`meta_query`。
- `parse_errors` 和 `realtime_query` 会生成可引用 findings。

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

已记录事件包括初始化、manifest evidence、grep evidence、Tool Runner action/evidence、LLM call lifecycle、model decision、final result 和 failure。`GET /api/tasks/:task_id/analysis` 返回 state 快照和事件列表。

`PLAN_ANALYSIS` 的真实模型调用必须生成稳定 `llmcall_*` callId，并记录：

- `llm_call_started`
- `llm_call_completed`
- `llm_call_schema_retry`

事件 details 包含 `callId`、`callKind`、`attempt`、`model` 和可选 `error`。Provider 或 schema 最终失败时，task error 必须包含可关联的 callId。

## 规划中的调查编排

```text
persist task
  -> initial extract/search
  -> load analysis state
  -> Analysis Agent next_step
  -> validate action, budget, whitelist and approval policy
  -> execute or wait
  -> append event and update state revision
  -> repeat or persist final result
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
- `analysis.max_rounds`，默认 4，非正值按 1
- `analysis.max_llm_calls`，默认 4，非正值按 1
- `analysis.max_actions`，默认 6，非正值按 1
- `analysis.max_repeated_action_fingerprints`，默认 1，非正值按 1
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

- 围绕当前上传、Metadata、Tool Runner、Analysis Agent 和 WebUI 逻辑补齐完整产品闭环，包括稳定任务创建、证据展示、追问/审批交互、结果确认和可复用的本地 smoke 流程。
- 更精确的 `flux_query_analyzer` 规则和真实工具输出字段映射。
- `influxql_analyzer` compare mode 已增强 delta 字段映射，后续根据真实 compare smoke 继续调整。
- 多轮 Analysis Agent 的产品化策略、模型用量和 Provider request id 审计。
- Case Store 已完成本地 JSON schema v2、本地召回、任务确认 Case 和手工 Case 创建；任务创建会写入 `case_context.json`，LLM prompt 会包含历史 Case 参考；后续补 embedding 和更正式的 Analysis Agent evidence bundle。当前开发阶段不兼容旧 v1 Case JSON。
- Code Evidence 和真实 Environment Collector 延后到产品闭环稳定后实现。

## 验收标准

- `cargo fmt --check`、`cargo check`、`cargo test` 通过。
- `scripts/start-local.sh` 能校验环境变量、构建 Server、后台启动、释放 shell job 并等待健康检查；支持真实 LLM、stub 和前台模式。
- `scripts/init-workdir.sh`、`scripts/build-server.sh`、`scripts/build-webui.sh`、`scripts/build-all.sh` 和 `scripts/server-service.sh` 必须在缺少 `LOGAGENT_WORK_DIR` 时失败；设置后能初始化运行目录、安装 Server binary 和 WebUI 静态产物，并通过工作目录内的 pid/log/config/data 管理 Server 启停。
- WEBUI `npm run lint`、`npm run typecheck`、`npm run build` 通过。
- `/health` 正常。
- `/` 从 `webui/out` 返回 WEBUI。
- 上传 sample.log、多个文件或只填写 Session 问题后都能创建 task 并读取 artifacts。
- Metadata 以 `instanceId` 作为用户主键；openGemini 导入必须由用户提供 InstanceID，原始 `ClusterID` 仅作为 `sourceClusterId` 标签保留。ID 自动补全且冲突时拒绝；workspace 保存 `metadata_context.json`，artifacts API 返回快照。
- pipeline 重跑保留 Metadata 快照，LLM Prompt 包含裁剪后的 Metadata 摘要。
- Executor 从 `SEARCH_LOGS` 或 `GENERATE_RESULT` 中断恢复时保留 phase、attempts 加一且不退回 `EXTRACT`。
- `RUN_TOOL` 无工具匹配时必须无副作用跳过；有匹配工具时必须生成 `tool_results` 并进入 `GENERATE_RESULT`。
- 规则版 Tool Runner 必须遵守 `max_input_files`，同一工具不同输入文件必须生成不同稳定 action id。
- `GET /api/tasks/:task_id/artifacts` 返回 `textInput` 和 `toolResults`。
- Tool Runner JSON stdout 的 summary/findings 必须进入 `toolResults`；非 JSON stdout 必须保持兼容 fallback。
- 真实 `influxql_analyzer` Report stdout 必须被转换为 `toolResults[].summary/findings`，且 `large_limit`、`no_time_filter` 等规则可在 artifacts 中查看。
- LLM Prompt 必须包含可裁剪的 Tool Runner summary/findings，并允许最终结果引用有效 tool finding evidence refs。
- `GET /api/tasks/:task_id/analysis` 必须返回 analysis state 和 events；从中间 phase 恢复的旧任务缺少 state 时必须自动生成最小快照继续执行。
- `POST /api/tasks/:task_id/case` 只能保存 `SUCCEEDED` 任务，重复确认同一任务不能生成重复 Case。
- `POST /api/cases` 必须能手工创建 `sourceType=manual` Case，必填标题、现象、根因和解决方案，且不包含 `taskId/sourceResultPath`。
- `GET /api/cases` 必须能按关键词召回启用 Case，禁用 Case 默认不返回。
- `PATCH /api/cases/:case_id` 必须能更新 Case 文本、产品、版本、环境、InstanceID、NodeID、证据引用和启用状态。
- 新任务 artifacts 必须返回 `caseContext`，LLM prompt 必须包含历史 Case 参考段落且不能要求模型把历史 Case 当作当前证据。
- LLM Gateway 必须能解析合法 `search_logs`、`run_tool`、`final_answer` decision，并拒绝当前未开放 action。
- phase 推进必须检查期望阶段，陈旧 dispatcher 不能覆盖较新的任务状态。
- multipart 和分片上传记录在重启后可恢复；未完成上传不能创建 task。
- multipart 小文件和批量上传不能在 payload 未 flush 时持久化 `COMPLETE` 记录。
- 非顺序 chunk、大小超过预期和未达到预期大小的 complete 必须失败。
- 损坏上传 JSON、非法 payload 路径或完成记录大小不一致必须阻止启动。
- stub 模式能单次生成结构化结果并通过 result API 读取。
- 真实 Provider 配置 `llm.model_env` 时，环境变量缺失或模型名为空必须启动失败。
- 真实 Provider 返回纯 JSON 或完整 JSON 代码围栏时可解析，额外自然语言不能被静默忽略。
- 真实 Provider 返回可映射的行号/范围或 `#start-#end` evidence ref 时应规范化为 canonical grep match refs；无法映射时必须失败。
- 批量任务的 manifest `files[].path` 必须带包名目录前缀。
- 无上传问题分析任务必须生成 `session_text_input.json`、空 `manifest.files`、空 `manifest.uploads` 和 `grep_results.totalMatches=0`。
- 受保护接口无 API Key 时返回 401。
- 等待用户或审批的任务可恢复，重复 message/decision/action 不产生重复执行。
- 达到分析预算时能生成带不确定性的结果并正常终止。
- README 和 SPEC 在接口、配置或 pipeline 变更时同步更新。
