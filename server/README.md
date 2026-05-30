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
