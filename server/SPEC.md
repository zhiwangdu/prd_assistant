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
- task 创建
- task JSON 持久化、列表、详情和重启恢复
- semaphore 限制的后台执行
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
GET /api/tasks/:task_id/analysis
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
  tasks/
    task_xxx.json
  uploads/
    upl_xxx/
  workspaces/
    task_xxx/
      raw/
        upl_xxx/
      extracted/
        package_name/
      manifest.json
      grep_results.json
      analysis_state.json
      analysis_events.jsonl
      result.json
      result.md
```

## 当前任务模型与 Pipeline

`TaskRecord` 包含 `schemaVersion`、任务 ID/来源/上传 ID、raw 输入、来源 URL、用户问题、状态、阶段、attempts、错误、artifact/result 路径和 RFC 3339 时间。

```text
POST task
  -> validate UploadRecord[]
  -> copy raw files into raw/<upload_id>/
  -> persist QUEUED
  -> return 202
background executor
  -> RUNNING / attempts + 1
  -> clean derived artifacts
  -> extract/copy each upload into extracted/<package_name>/
  -> collect manifest files
  -> simple grep
  -> write manifest.json and grep_results.json
  -> GENERATE_RESULT
  -> one LLM Gateway call
  -> write result.json and result.md
  -> SUCCEEDED or FAILED
```

`POST /api/tasks` accepts either single-file `uploadId` or batch `uploadIds`. Batch uploads are analyzed in one workspace so later stages can run joint analysis across all logs.

`question` 可选，长度不能超过 `llm.max_input_chars / 2`。

任务文件使用临时文件加 rename 原子替换。启动时损坏 JSON 必须失败；`QUEUED` 和被重置的 `RUNNING` 按创建时间恢复。任务派生产物可删除重建，raw 快照必须保留。

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
- `llm.model`
- `llm.request_timeout_seconds`
- `llm.max_input_chars`
- `llm.max_output_tokens`

## 待实现

- `WAITING_FOR_USER`、`WAITING_FOR_APPROVAL` 的恢复 API 和完整 Analysis Agent 状态机。
- task context 关联 Metadata。
- Tool Runner、Code Evidence 和 Environment Collector 编排。
- 多轮 Analysis Agent、message/approval API、模型用量和 Provider request id 审计。
- Case Store 写入和召回。

## 验收标准

- `cargo fmt --check`、`cargo check`、`cargo test` 通过。
- WEBUI `npm run lint`、`npm run typecheck`、`npm run build` 通过。
- `/health` 正常。
- `/` 从 `webui/out` 返回 WEBUI。
- 上传 sample.log 或多个文件后能创建 task 并读取 artifacts。
- stub 模式能单次生成结构化结果并通过 result API 读取。
- 批量任务的 manifest `files[].path` 必须带包名目录前缀。
- 受保护接口无 API Key 时返回 401。
- 等待用户或审批的任务可恢复，重复 message/decision/action 不产生重复执行。
- 达到分析预算时能生成带不确定性的结果并正常终止。
- README 和 SPEC 在接口、配置或 pipeline 变更时同步更新。
