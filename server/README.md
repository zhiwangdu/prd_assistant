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

Server 是任务管理和分析调度中心。

负责：

- 上传管理
- 任务创建和状态流转
- 日志解压和 manifest 生成
- rg 检索和摘要生成
- 外部分析工具调度
- 代码证据生成
- 测试环境采集调度
- LLM 分析调用
- Case 存储和召回
- WebUI API

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
EXTRACTING
SEARCHING
ANALYZING
DONE
FAILED
```

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
