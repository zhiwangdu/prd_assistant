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
- 编排 Log Analyzer、Tool Runner、Code Evidence、Environment Collector、LLM Agent
- 管理模块输出和任务失败原因
- 查询和关联实例、集群、节点元数据
- LLM 分析调用
- Case 存储和召回
- WebUI API

## 职责边界

- Server：任务状态、API、调度、错误汇总。
- Log Analyzer：解压、manifest、简单 grep 检索、日志摘要。
- Tool Runner：外部工具调用。
- Code Evidence：版本代码检索。
- Environment Collector：测试环境采集。
- Metadata：实例 ID、集群和节点元数据。
- LLM Agent：证据裁剪、Prompt 组装、模型调用。

## 代码结构

当前 Server 已按框架拆分：

```text
server/src
  main.rs              # 启动入口和 Axum app 装配
  api/                 # HTTP 路由
    health.rs
    uploads.rs
    tasks.rs
    metadata.rs          # 后续新增，实例/集群/节点元数据 API
  auth.rs              # API Key middleware
  config.rs            # logagent.yaml 加载和默认值
  error.rs             # API 错误响应
  fs_utils.rs          # 文件名和路径安全工具
  id.rs                # MVP ID 生成
  log_analyzer.rs      # 解压、manifest 文件扫描、简单 grep
  models.rs            # DTO / task context / evidence output
  pipeline.rs          # upload 任务执行管线
  state.rs             # AppState 和内存 UploadStore
```

后续新增 Tool Runner、Code Evidence、Environment Collector、LLM Agent 时，应保持这个模式：

- API 层只做请求解析和响应。
- Pipeline 负责任务编排。
- 各模块只执行自己的能力。
- 新模块的运行和部署方式同步写入对应 README。

## 任务来源

```text
upload:
  upload -> extract -> manifest -> rg -> tools -> code evidence -> LLM

environment:
  ssh/scp collect -> manifest -> rg -> tools -> code evidence -> LLM
```

## 状态流转

```text
CREATED
UPLOADED
COLLECTING
EXTRACTING
SEARCHING
RUNNING_TOOLS
COLLECTING_CODE
ANALYZING
DONE
FAILED
```

`COLLECTING` 只用于 environment 来源任务；upload 来源任务从 `UPLOADED` 进入 `EXTRACTING`。

## 数据目录

```text
/data/logagent
  uploads/
  workspaces/
  tasks/
  cases/
  code_worktrees/
```

任务 workspace：

```text
/data/logagent/workspaces/task_456
  raw/
  extracted/
  collected/
  manifest.json
  error_summary.json
  contexts.jsonl
  tool_results/
  code_evidence.json
  environment_evidence.json
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
- `POST /api/tasks` 创建任务。
- `GET /api/tasks/:task_id/artifacts` 读取任务产物。
- 同步解压 `.zip`、`.tar.gz`、`.tgz`、`.tar`，普通 `.log` / `.txt` 直接复制到 `extracted/<文件基名>/`。
- `.tar.gz` / `.tgz` 如果 gzip tar 解压失败，会自动按普通 `.tar` fallback 再尝试一次。
- 创建任务支持 `uploadId` 单文件和 `uploadIds` 批量文件；批量文件进入同一个 workspace，按文件基名区分解压目录。
- 递归扫描文本行，按配置关键词做简单 grep。
- 写入 `manifest.json` 和 `grep_results.json`。

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
GET /api/tasks/:task_id/artifacts
```

后续 Metadata 接口规划：

```http
GET /api/metadata/instances/:instance_id
GET /api/metadata/clusters/:cluster_id
GET /api/metadata/clusters/:cluster_id/nodes
POST /api/metadata/imports
GET /api/metadata/imports/:import_id/preview
POST /api/metadata/imports/:import_id/confirm
```

`POST /api/uploads` 使用 multipart：

- `file`: 上传文件
- `filename`: 原始文件名
- `source`: 可选来源标记

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

`POST /api/tasks` 请求：

```json
{
  "uploadId": "upl_123",
  "sourceUrl": "https://logs.example/export/123"
}
```

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
export LOGAGENT_NATIVE_API_KEY=dev-token
cargo run -p logagent-server -- --config examples/logagent.yaml
```

Server 会静态托管 `webui/`，本地访问：

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
