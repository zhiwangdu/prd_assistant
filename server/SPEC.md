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
- task JSON 持久化、列表、详情和重启恢复
- semaphore 限制的后台执行
- phase 驱动的可恢复 Executor dispatcher
- TaskContext、Action、EvidenceArtifact 和 EvidenceProvider 公共契约
- Tool Runner MVP 和 `RUN_TOOL` phase
- Analysis State Store MVP 和 `/api/tasks/:task_id/analysis`
- task artifact 查询
- metadata 查询和导入确认
- upload pipeline
- WEBUI 静态托管，目录为 Vite 构建的 `webui/out`

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
POST /api/tasks
GET /api/tasks
GET /api/tasks/:task_id
GET /api/tasks/:task_id/analysis
GET /api/tasks/:task_id/artifacts
GET /api/tasks/:task_id/result
GET /api/metadata/instances/:instance_id
GET /api/metadata/clusters/:cluster_id
GET /api/metadata/clusters/:cluster_id/nodes
POST /api/metadata/snapshots/fetch
POST /api/metadata/imports
POST /api/metadata/imports/fetch
GET /api/metadata/imports/:import_id/preview
POST /api/metadata/imports/:import_id/confirm
```

规划新增：

```http
POST /api/tasks/:task_id/messages
POST /api/tasks/:task_id/actions/:action_id/decision
```

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
  tasks/
    task_xxx.json
  workspaces/
    task_xxx/
      raw/
        upl_xxx/
      extracted/
        package_name/
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

`TaskRecord` 包含 `schemaVersion`、任务 ID/来源/上传 ID、raw 输入、来源 URL、用户问题、解析后的 instance/cluster/node ID、状态、阶段、attempts、错误、metadata/artifact/result 路径和 RFC 3339 时间。

```text
POST task
  -> validate UploadRecord[]
  -> copy raw files into raw/<upload_id>/
  -> persist QUEUED
  -> return 202
background executor
  -> RUNNING / attempts + 1
  -> dispatch persisted phase
  -> EXTRACT: clean/rebuild extracted + manifest
  -> persist SEARCH_LOGS
  -> SEARCH_LOGS: rebuild grep evidence
  -> persist RUN_TOOL
  -> RUN_TOOL: rule-based configured tool actions, writes tool_results
  -> persist GENERATE_RESULT
  -> GENERATE_RESULT: LLM Gateway call using grep/metadata/tool evidence, with one correction retry for result schema errors
  -> append analysis state/events
  -> write result.json/result.md
  -> SUCCEEDED or FAILED
```

`POST /api/tasks` accepts either single-file `uploadId` or batch `uploadIds`. Optional `instanceId` / `clusterId` / `nodeId` are resolved against Metadata before persistence.

`question` 可选，长度不能超过 `llm.max_input_chars / 2`。

LLM Gateway 响应解析接受纯 JSON、完整 JSON Markdown 代码围栏，或包含唯一顶层 JSON object 的自然语言响应。Prompt 包含 grep evidence、Metadata 摘要和 Tool Runner summary/findings；stdout/stderr 原文不进入 Prompt。可追踪的字符串形式 root cause、`matches/<index>` / `matches/<start>-<end>` 引用别名，以及单字符串列表字段会规范化为正式结果结构。最终结果允许引用 `grep_results.json#matches/<index>` 或 `tool_results/<action_id>/result.json#findings/<index>`；未知 action 或越界 finding 会拒绝。解析/schema 错误会追加修正提示并重试一次；多个 JSON object、无 JSON object 或两次 schema 都不合法时任务进入 `FAILED / GENERATE_RESULT`。

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

Analysis State Store 当前记录固定 pipeline 的审计状态，不驱动多轮 action loop。Server 会写入：

```text
analysis_state.json
analysis_events.jsonl
```

已记录事件包括初始化、manifest evidence、grep evidence、Tool Runner action/evidence、final result 和 failure。`GET /api/tasks/:task_id/analysis` 返回 state 快照和事件列表。

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
- `tools.<name>.enabled`
- `tools.<name>.path`
- `tools.<name>.path_env`
- `tools.<name>.timeout_seconds`
- `tools.<name>.max_output_bytes`
- `tools.<name>.max_input_files`
- `tools.<name>.args`
- `tools.<name>.match.file_patterns`
- `tools.<name>.match.keywords`

## 待实现

- `WAITING_FOR_USER`、`WAITING_FOR_APPROVAL` 的恢复 API 和完整 Analysis Agent 状态机。
- Tool Runner、Code Evidence 和 Environment Collector 编排。
- 更精确的 `flux_query_analyzer`、`influxql_analyzer` 规则和真实工具输出字段映射。
- 多轮 Analysis Agent、message/approval API、模型用量和 Provider request id 审计。
- Case Store 写入和召回。

## 验收标准

- `cargo fmt --check`、`cargo check`、`cargo test` 通过。
- `scripts/start-local.sh` 能校验环境变量、构建 Server、后台启动并等待健康检查；支持真实 LLM、stub 和前台模式。
- WEBUI `npm run lint`、`npm run typecheck`、`npm run build` 通过。
- `/health` 正常。
- `/` 从 `webui/out` 返回 WEBUI。
- 上传 sample.log 或多个文件后能创建 task 并读取 artifacts。
- Metadata ID 自动补全且冲突时拒绝；workspace 保存 `metadata_context.json`，artifacts API 返回快照。
- pipeline 重跑保留 Metadata 快照，LLM Prompt 包含裁剪后的 Metadata 摘要。
- Executor 从 `SEARCH_LOGS` 或 `GENERATE_RESULT` 中断恢复时保留 phase、attempts 加一且不退回 `EXTRACT`。
- `RUN_TOOL` 无工具匹配时必须无副作用跳过；有匹配工具时必须生成 `tool_results` 并进入 `GENERATE_RESULT`。
- 规则版 Tool Runner 必须遵守 `max_input_files`，同一工具不同输入文件必须生成不同稳定 action id。
- `GET /api/tasks/:task_id/artifacts` 返回 `toolResults`。
- Tool Runner JSON stdout 的 summary/findings 必须进入 `toolResults`；非 JSON stdout 必须保持兼容 fallback。
- LLM Prompt 必须包含可裁剪的 Tool Runner summary/findings，并允许最终结果引用有效 tool finding evidence refs。
- `GET /api/tasks/:task_id/analysis` 必须返回 analysis state 和 events；从中间 phase 恢复的旧任务缺少 state 时必须自动生成最小快照继续执行。
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
- 受保护接口无 API Key 时返回 401。
- 等待用户或审批的任务可恢复，重复 message/decision/action 不产生重复执行。
- 达到分析预算时能生成带不确定性的结果并正常终止。
- README 和 SPEC 在接口、配置或 pipeline 变更时同步更新。
