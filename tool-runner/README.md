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

Server 已实现共享 `AgentAction`、`EvidenceArtifact`、`EvidenceProvider` 契约和 `RUN_TOOL` phase。Tool Runner 实现时直接消费这些契约，不再新增独立的 ad-hoc pipeline 分支。

目标工具示例：

- `flux_query_analyzer`
- `influxql_analyzer`

## 配置示例

```yaml
tools:
  flux_query_analyzer:
    enabled: true
    path: /opt/logagent/tools/flux_query_analyzer
    timeout_seconds: 30
    input_mode: file
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
    path: /opt/logagent/tools/influxql_analyzer
    timeout_seconds: 30
    input_mode: file
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

## 输出结构

```json
{
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
  "rawOutputPath": "tool_results/flux_query_analyzer.raw.json"
}
```
