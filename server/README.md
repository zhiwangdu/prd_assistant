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

Server 是任务管理和分析调度中心。Server 只负责编排，不直接实现日志解析、工具执行、代码检索或 SSH 采集的具体逻辑。

负责：

- 上传管理
- 任务创建和状态流转
- 编排 Log Analyzer、Tool Runner、Code Evidence、Environment Collector、Analysis Agent 和 LLM Gateway
- 持久化分析上下文、事件、预算、待回答问题和待审批动作
- 校验并执行 Analysis Agent 的结构化动作
- 管理模块输出和任务失败原因
- 查询和关联实例、集群、节点元数据
- 用户消息、动作批准/拒绝和任务恢复
- Case 存储和召回
- WebUI API

## 职责边界

- Server：任务状态、API、调度、错误汇总。
- Log Analyzer：解压、manifest、简单 grep 检索、日志摘要。
- Tool Runner：外部工具调用。
- Code Evidence：版本代码检索。
- Environment Collector：测试环境采集。
- Metadata：实例 ID、集群和节点元数据。
- Analysis Agent：调查策略、事实/假设/缺口、多轮动作和终止判断。
- LLM Gateway：证据裁剪、Prompt 组装、模型调用和结构化响应解析。

## 代码结构

当前 Server 已按框架拆分：

```text
server/src
  main.rs              # 启动入口和 Axum app 装配
  api/                 # HTTP 路由
    health.rs
    uploads.rs
    tasks.rs
    metadata.rs          # 实例/集群/节点元数据 API
  auth.rs              # API Key middleware
  config.rs            # logagent.yaml 加载和默认值
  contracts.rs         # TaskContext、Action、Evidence 和 Provider 公共契约
  error.rs             # API 错误响应
  fs_utils.rs          # 文件名和路径安全工具
  id.rs                # MVP ID 生成
  log_analyzer.rs      # 解压、manifest 文件扫描、简单 grep
  models.rs            # DTO / task context / evidence output
  pipeline.rs          # 各执行阶段的幂等处理函数
  state.rs             # AppState 组装和任务启动恢复
  task_store.rs        # 持久化 TaskRecord、状态转换和原子 JSON 更新
  task_executor.rs     # Tokio semaphore 和可恢复 phase dispatcher
  tool_runner.rs       # 白名单工具执行、规则 action 和 tool result artifact
  upload_store.rs      # 持久化 UploadRecord、分片进度和启动恢复
```

后续新增 Tool Runner、Code Evidence、Environment Collector、Analysis Agent 和 LLM Gateway 时，应保持这个模式：

- API 层只做请求解析和响应。
- Pipeline 负责任务编排。
- 各模块只执行自己的能力。
- 新模块的运行和部署方式同步写入对应 README。

## 任务来源

```text
upload/environment
  -> 基础采集、解压和初始日志证据
  -> Analysis Agent 调查循环
  -> Server 执行安全动作或进入等待状态
  -> 新证据回填并继续分析
  -> final result
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

当前基础 pipeline 实际产生 `QUEUED`、`RUNNING`、`SUCCEEDED`、`FAILED`，dispatcher 已支持 `EXTRACT`、`SEARCH_LOGS`、`RUN_TOOL`、`PLAN_ANALYSIS` 和 `GENERATE_RESULT`。`PLAN_ANALYSIS` 已启用多轮 LLM action loop，受轮数、LLM 调用、action 数和重复 fingerprint 预算限制；`WAITING_FOR_USER`、`WAITING_FOR_APPROVAL` 已进入公共模型，但在对应模块实现前不会由正常任务生成。

## 数据目录

```text
/data/logagent
  uploads/
  workspaces/
  tasks/
  cases/
  code_worktrees/
```

每个任务持久化到 `tasks/<task_id>.json`。写入使用同目录临时文件加 rename；启动时任何损坏的任务 JSON 都会导致 Server 明确启动失败。

任务 workspace：

```text
/data/logagent/workspaces/task_456
  raw/
  extracted/
  collected/
  manifest.json
  error_summary.json
  metadata_context.json
  contexts.jsonl
  tool_results/
  code_evidence.json
  environment_evidence.json
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
- `POST /api/tasks` 创建任务。
- `GET /api/tasks` 返回按创建时间倒序的持久化任务列表。
- `GET /api/tasks/:task_id` 返回完整 `TaskRecord`。
- `GET /api/tasks/:task_id/artifacts` 读取任务产物。
- `GET /api/tasks/:task_id/result` 读取结构化 LLM 分析结果。
- 同步解压 `.zip`、`.tar.gz`、`.tgz`、`.tar`，普通 `.log` / `.txt` 直接复制到 `extracted/<文件基名>/`。
- `.tar.gz` / `.tgz` 如果 gzip tar 解压失败，会自动按普通 `.tar` fallback 再尝试一次。
- 创建任务支持 `uploadId` 单文件和 `uploadIds` 批量文件；请求验证上传后先复制到 workspace raw 快照、持久化 `QUEUED`，以 `202 Accepted` 立即返回。
- 后台执行器使用 `server.max_concurrent_tasks` 控制并发，默认 2。
- 后台执行器按持久化 phase 循环分派单个幂等 handler；每个 handler 成功后使用期望 phase 校验原子推进到下一阶段。
- Server 重启时将 `RUNNING` 重置为 `QUEUED` 但保留 phase，并与已有 `QUEUED` 一起按创建时间恢复；`SUCCEEDED`、`FAILED` 不自动重跑。
- 仅从 `EXTRACT` 恢复时清理 `extracted/`、`manifest.json`、`grep_results.json`、`result.json` 和 `result.md`；从后续阶段恢复时复用已完成的前置产物。
- `RUNNING` 缺少 phase、`SUCCEEDED` 仍保留 phase 或未知 phase 枚举会使 Server 明确启动失败。
- 小文件和批量 multipart 上传在写完 payload 后会显式 flush 文件，再持久化 `UploadRecord`，避免记录校验时读到未落盘的 0 字节 payload。
- `RUN_TOOL` 阶段按 manifest/grep 对已配置工具生成规则版 `run_tool` action；manifest file pattern 优先，grep keyword 补充候选，每个工具最多选择 `max_input_files` 个输入文件；未匹配或未配置工具时直接进入 `PLAN_ANALYSIS`。
- Tool Runner 只执行 `tools` 白名单中的绝对路径工具，路径可来自固定 `path` 或 `path_env` 环境变量，使用参数数组，不拼接 shell；stdout/stderr/result 写入 `tool_results/<action_id>/`。
- 规则版 Tool Runner action id 使用工具名和输入文件稳定哈希，批量任务中同一工具的不同输入文件会写入不同 `tool_results/<action_id>/`。
- Tool Runner 会从 JSON stdout 中提取 `summary` 和 `findings` 写入 `result.json`；非 JSON stdout 保持可追溯但不会导致任务失败。
- `examples/server-tools.yaml` 提供 `flux_query_analyzer` / `influxql_analyzer` 的环境变量路径模板。
- Analysis State Store 写入 `analysis_state.json` 和 `analysis_events.jsonl`，记录 manifest、grep、tool action、model decision、final result 和 failure 事件；真实工具未完成时可继续用 mock 工具验证 action/event/evidence 链路。
- task 创建时解析可选 `instanceId` / `clusterId` / `nodeId` 并保留 `metadata_context.json`；pipeline 重跑不清理该快照。
- 未关联 TaskRecord 的 workspace 只记录告警，不自动删除。
- 递归扫描文本行，按配置关键词做简单 grep。
- `RUN_TOOL` 后进入 `PLAN_ANALYSIS`，循环调用 LLM Gateway 生成 `action | final_answer` 决策；`search_logs` 会用模型给出的关键词重建 `grep_results.json` 并回到下一轮，`run_tool` 会通过同一 Tool Runner 执行通道写入 `tool_results` 并回到下一轮，`final_answer` 会直接持久化为 `result.json` / `result.md` 并成功结束。
- `PLAN_ANALYSIS` 在达到 `analysis` 预算或发现重复 action fingerprint 时，不进入 `FAILED`，而是生成低置信度、带终止原因的 `result.json` / `result.md` 并正常结束。
- `GENERATE_RESULT` 仍保留为兼容恢复和兜底路径，Prompt 包含 manifest、grep、Metadata 摘要和 Tool Runner summary/findings；最终结果解析/schema 错误会追加修正提示并重试一次，仍失败时任务进入 `FAILED / GENERATE_RESULT`。`PLAN_ANALYSIS` 的 action decision 解析/schema 错误也会追加修正提示并重试一次，仍失败时任务进入 `FAILED / PLAN_ANALYSIS`。
- LLM Gateway 会把可追踪的行号/索引范围 evidence ref 规范化为 `grep_results.json#matches/<index>`；无法映射的引用仍按 schema 错误处理。
- LLM Gateway 允许最终结果引用 Tool Runner finding，格式为 `tool_results/<action_id>/result.json#findings/<index>`；未知 action 或越界 finding 会按 schema 错误处理。
- LLM Gateway 会把可追踪的字符串形式 root cause，例如 `原因（evidenceRefs: [matches/0-3]）`，规范化为对象形式。
- LLM Gateway 会把真实模型返回的单字符串列表字段规范化为单元素数组，例如 `missingInformation: "..."`。
- LLM Gateway 会把 `PLAN_ANALYSIS` 中真实模型返回的裸最终结果 JSON，或多包一层的 `final_answer.result.result` / `answer` / `finalAnswer`，规范化为真正的 `final_answer`；缺少 `summary` 等核心字段的结果仍会拒绝。
- stub Provider 用于默认开发和自动测试；真实 Provider 使用 OpenAI-compatible Chat Completions。
- LLM 模型可通过 `llm.model_env` 引用环境变量；未配置时继续使用静态 `llm.model`。
- OpenAI-compatible 响应可为纯 JSON、完整 JSON Markdown 代码围栏，或包含唯一顶层 JSON object 的自然语言响应；多个 JSON object、无 JSON object 或 schema 不合法时按协议错误处理。
- LLM 解析/schema 错误会返回最新失败原因和上一轮失败原因；Provider HTTP、鉴权、限流和超时错误不重试。

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

analysis 响应可在任务存在后读取 `analysis_state.json` 和 `analysis_events.jsonl`。artifacts 响应在成功任务中包含 `toolResults`，每项来自 `tool_results/<action_id>/result.json`。`toolResults[].findings` 是结构化工具发现，当前包含可选 `severity`、`file`、`line` 和必填 `message`。

以下 Analysis API 为规划接口，尚未实现：

- `POST /api/tasks/:task_id/messages`
- `POST /api/tasks/:task_id/actions/:action_id/decision`

Server 必须保证 message 和 decision 幂等，禁止客户端直接把任务状态改成 `RUNNING`。

`GET /api/metadata/clusters/:cluster_id` 会返回 cluster 基本信息、节点 ID 列表、labels，以及 openGemini 解析出的 `databases` 和 `partitionViews` 摘要。`databases` 包含默认保留策略、RP 参数、Measurements schema 和 ShardGroups；`partitionViews` 对应 `PtView`，用于查看 database partition 的 owner data node、状态、版本和 RGID。

`POST /api/metadata/snapshots/fetch` 只读拉取实时 `/getdata`，返回完整节点字段、Raw JSON、Shard、IndexGroup、Index 和 MstVersions。Shard/Index `Owners` 按 PT ID 保存，关系通过 `PtView` 解析为 `Shard -> PT -> DataNode`。

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
- 只有 `COMPLETE` 上传可以创建 task。
- 启动时损坏的上传 JSON、缺失 payload、完成记录大小不一致会使 Server 明确启动失败。
- 如果进程在 payload 写入后、进度 JSON 更新前中断，启动恢复会以 payload 实际大小修正 `UPLOADING` 记录。

`POST /api/tasks` 请求：

```json
{
  "uploadId": "upl_123",
  "question": "请分析连接超时的可能原因",
  "instanceId": "i-123",
  "clusterId": "c-1",
  "nodeId": "n-1",
  "sourceUrl": "https://logs.example/export/123"
}
```

响应为 `202 Accepted`，包含 `taskId`、`url`、`status`、`phase` 和 `createdAt`。Native Agent 继续使用原有 `taskId`、`url` 字段。

`question` 可选；未提供时使用默认日志分析问题。长度上限为 `llm.max_input_chars / 2`。

Metadata ID 均可选。Server 会基于已确认 Metadata 补全关联 ID 并校验一致性，未知或冲突关系返回 `400`。任务详情返回解析后的 ID；成功任务的 artifacts 响应包含 `metadataContextPath` 和 `metadataContext`。

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

脚本默认使用 `examples/server-llm-openai-compatible.yaml` 和端口 `50994`，将 PID 写入 `/tmp/logagent-server-llm.pid`，日志写入 `/tmp/logagent-server-llm.log`，并等待 `/health` 成功。`--stub` 使用端口 `50992`，`--foreground` 不进入后台。脚本只读取环境变量，不打印或持久化密钥。

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
cargo build --release -p logagent-server
LOGAGENT_NATIVE_API_KEY=<secret> \
  ./target/release/logagent-server --config /etc/logagent/logagent.yaml
```

systemd 可按上述命令封装为 `logagent-server.service`。
