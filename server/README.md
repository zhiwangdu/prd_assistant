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
- LLM 分析调用
- Case 存储和召回
- WebUI API

## 职责边界

- Server：任务状态、API、调度、错误汇总。
- Log Analyzer：解压、manifest、rg 检索、日志摘要。
- Tool Runner：外部工具调用。
- Code Evidence：版本代码检索。
- Environment Collector：测试环境采集。
- LLM Agent：证据裁剪、Prompt 组装、模型调用。

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
- `POST /api/tasks` 创建任务。
- 同步解压 `.zip`、`.tar.gz`、`.tgz`，普通 `.log` / `.txt` 直接复制到 `extracted/`。
- 递归扫描文本行，按配置关键词做简单 grep。
- 写入 `manifest.json` 和 `grep_results.json`。

当前已实现接口：

```http
GET /health
POST /api/uploads
POST /api/tasks
```

`POST /api/uploads` 使用 multipart：

- `file`: 上传文件
- `filename`: 原始文件名
- `source`: 可选来源标记

`POST /api/tasks` 请求：

```json
{
  "uploadId": "upl_123",
  "sourceUrl": "https://logs.example/export/123"
}
```

本地启动：

```bash
export LOGAGENT_NATIVE_API_KEY=dev-token
cargo run -p logagent-server -- --config examples/logagent.yaml
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
