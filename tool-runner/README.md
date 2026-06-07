# Tool Runner 方案

## 实现建议

优先使用 Rust 实现。语言优先级：

```text
Rust -> C/C++ -> Go/Python/Java 等
```

Tool Runner 涉及进程执行、timeout、stdout/stderr 捕获和路径校验，适合用 Rust 做严格边界控制。已有 C/C++ 编译工具直接作为被调用工具接入。

## 职责

Tool Runner 负责调用已有编译好的诊断工具，并把工具输出转成 LLM 可引用的结构化证据。

调用来源可以是初始规则或 Analysis Agent 的 `run_tool` action，但都必须由 Server 映射到配置中的工具和参数模板。Agent 不能提供可执行路径或自由 argv。

当前 Server 已实现共享 `AgentAction`、`EvidenceArtifact`、`EvidenceProvider` 契约和 `RUN_TOOL` phase。Tool Runner MVP 作为 Server 内部 Rust 模块运行，当前由 Server 规则根据 `manifest.json` / `grep_results.json` 自动生成 `run_tool` action；后续 Analysis Agent 可复用同一个 action 执行通道。

目标工具示例：

- `flux_query_analyzer`
- `influxql_analyzer`

## 配置示例

```yaml
tools:
  flux_query_analyzer:
    enabled: true
    path_env: LOGAGENT_TOOL_FLUX_QUERY_ANALYZER
    timeout_seconds: 30
    match:
      file_patterns:
        - "*.flux"
        - "*.log"
      keywords:
        - "flux"
        - "query"
        - "planner"
    args:
      - "--input"
      - "{input_file}"
      - "--format"
      - "json"

  influxql_analyzer:
    enabled: true
    path_env: LOGAGENT_TOOL_INFLUXQL_ANALYZER
    timeout_seconds: 30
    match:
      file_patterns:
        - "*.sql"
        - "*.log"
      keywords:
        - "influxql"
        - "select"
        - "show series"
    args:
      - "--input"
      - "{input_file}"
      - "--format"
      - "json"
```

## 执行原则

- 只允许调用配置文件中声明的工具。
- 工具路径必须是绝对路径。
- 参数只允许使用预定义占位符。
- 使用参数数组执行，不拼接 shell 字符串。
- 每次执行必须设置 timeout。
- stdout、stderr、exit code、耗时都要保存。
- 工具失败不应导致整个任务失败，除非标记为必需。

## 当前实现状态

- 已实现 `server/src/tool_runner.rs`。
- 已支持配置解析、绝对路径校验、timeout、stdout/stderr 捕获、输出截断和幂等复用。
- 已支持 `{input_file}`、`{manifest_path}`、`{grep_results_path}`、`{workspace}`、`{action_id}` 占位符。
- 已支持固定 `path` 或环境变量 `path_env` 指定工具路径；启用工具时最终路径必须是绝对路径。
- 已支持 `tool_results/<action_id>/result.json`、`stdout.txt`、`stderr.txt`。
- 已支持从工具 stdout JSON 中提取 `summary` 和 `findings`；stdout 不是 JSON 时保留原始输出并使用通用 summary，不影响任务成功。
- artifacts API 和 WebUI 能展示 tool result 与结构化 findings。
- 已新增 `examples/server-tools.yaml` 作为真实 `flux_query_analyzer` / `influxql_analyzer` 接入模板；默认启动配置仍不强依赖这些二进制。

## 本地真实工具 smoke

```bash
export LOGAGENT_NATIVE_API_KEY=dev-token
export LOGAGENT_TOOL_FLUX_QUERY_ANALYZER=/abs/path/to/flux_query_analyzer
export LOGAGENT_TOOL_INFLUXQL_ANALYZER=/abs/path/to/influxql_analyzer
cargo run -p logagent-server -- --config examples/server-tools.yaml
```

`server-tools.yaml` 使用 stub LLM，便于单独验证 Tool Runner。上传 `.flux` 或包含 `flux/planner` 关键词的日志会触发 `flux_query_analyzer`；上传 `.sql` 或包含 `influxql/select/show series/show measurements` 关键词的日志会触发 `influxql_analyzer`。

## 输出结构

工具 stdout 若为 JSON，Server 会尝试解析以下形态：

```json
{
  "summary": "发现 2 个可能导致慢查询的问题",
  "findings": [
    {
      "severity": "medium",
      "file": "query.log",
      "line": 120,
      "message": "filter 下推失败，可能导致扫描数据量过大"
    }
  ]
}
```

兼容字段：

- summary 可来自 `summary`、`message` 或 `title`。
- findings 数组可来自 `findings`、`issues` 或 `diagnostics`。
- finding 消息可来自 `message`、`summary`、`description`、`detail`、`title` 或 `cause`。
- severity 可来自 `severity`、`level` 或 `status`。
- file 可来自 `file`、`path` 或 `filename`。
- line 可来自 `line`、`lineNumber` 或 `startLine`。

`result.json` 标准化后结构：

```json
{
  "schemaVersion": 2,
  "tool": "flux_query_analyzer",
  "actionId": "act_123",
  "status": "OK",
  "exitCode": 0,
  "durationMs": 1234,
  "summary": "发现 2 个可能导致慢查询的 range/filter 顺序问题",
  "findings": [
    {
      "severity": "medium",
      "file": "query.log",
      "line": 120,
      "message": "filter 下推失败，可能导致扫描数据量过大"
    }
  ],
  "stdoutPath": "tool_results/act_123/stdout.txt",
  "stderrPath": "tool_results/act_123/stderr.txt"
}
```
