# Tool Runner Spec

## 目标

Tool Runner 负责按白名单调用已有外部分析工具，把结果标准化为任务证据。

## 当前状态

Server 已实现 Tool Runner MVP：配置白名单、规则版 `run_tool` action、可恢复 `RUN_TOOL` phase、timeout、stdout/stderr/result 持久化和 artifacts API 展示。真实工具路径和更精确规则仍需按环境配置。

## 首批工具

- `flux_query_analyzer`
- `influxql_analyzer`

## 输入

- Task workspace
- 工具名称
- `action_id`
- 工具参数模板
- 日志片段、查询文本或 manifest 文件

## 输出

建议产物：

```text
tool_results/
  act_tool_flux_query_analyzer/
    result.json
    stdout.txt
    stderr.txt
  act_tool_influxql_analyzer/
    result.json
    stdout.txt
    stderr.txt
```

每个结果至少包含：

- `tool`
- `action_id`
- `command`
- `exit_code`
- `duration_ms`
- `stdout_path`
- `stderr_path`
- `summary`

## 安全约束

- 只能调用配置白名单里的工具。
- 参数必须由模板和结构化输入生成，不能拼接任意用户命令。
- 工具执行需要超时和输出大小限制。
- 工作目录限制在 task workspace 或只读工具目录。
- Analysis Agent 只能选择允许的工具和结构化参数，不能传入任意命令。

## 验收标准

- 配置不存在的工具不可调用。
- 工具超时后任务记录失败原因。
- stdout/stderr 可追溯。
- 重复 action id 幂等，结果可回填到同一分析 revision。
- 未配置或未匹配工具时 `RUN_TOOL` 阶段直接跳过，不影响现有 LLM 结果。
- README 和 SPEC 在工具协议或结果结构变更时同步更新。
