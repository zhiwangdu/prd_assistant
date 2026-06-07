# Tool Runner Spec

## 目标

Tool Runner 负责按白名单调用已有外部分析工具，把结果标准化为任务证据。

## 当前状态

Server 已实现 Tool Runner MVP：配置白名单、规则版 `run_tool` action、可恢复 `RUN_TOOL` phase、timeout、stdout/stderr/result 持久化、stdout JSON 摘要解析和 artifacts API 展示。真实工具可通过固定 `path` 或 `path_env` 环境变量接入，`examples/server-tools.yaml` 提供 `flux_query_analyzer` / `influxql_analyzer` 模板。

## 首批工具

- `flux_query_analyzer`
- `influxql_analyzer`

## 输入

- Task workspace
- 工具名称
- `action_id`
- 工具参数模板
- 工具路径，来自固定 `path` 或 `path_env` 环境变量
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

- `schema_version`
- `tool`
- `action_id`
- `status`
- `command`
- `exit_code`
- `duration_ms`
- `stdout_path`
- `stderr_path`
- `summary`
- `findings`

当 stdout 是 JSON 时，Tool Runner 会尽量提取：

- `summary` / `message` / `title`
- `findings` / `issues` / `diagnostics`
- finding 内的 `severity` / `level` / `status`
- finding 内的 `file` / `path` / `filename`
- finding 内的 `line` / `lineNumber` / `startLine`
- finding 内的 `message` / `summary` / `description` / `detail` / `title` / `cause`

stdout 不是 JSON 或字段不匹配时，不判定为工具失败，只保留 stdout/stderr 并生成通用 summary。

LLM Gateway 会读取 result artifact 中的 summary/findings。finding 的最终答案引用格式固定为：

```text
tool_results/<action_id>/result.json#findings/<index>
```

## 安全约束

- 只能调用配置白名单里的工具。
- 启用工具必须解析出绝对路径；禁用工具不读取 `path_env`。
- 参数必须由模板和结构化输入生成，不能拼接任意用户命令。
- 工具执行需要超时和输出大小限制。
- 工作目录限制在 task workspace 或只读工具目录。
- Analysis Agent 只能选择允许的工具和结构化参数，不能传入任意命令。

## 验收标准

- 配置不存在的工具不可调用。
- `path_env` 缺失、为空或解析出非绝对路径时启动失败。
- 工具超时后任务记录失败原因。
- stdout/stderr 可追溯。
- JSON stdout 中的 summary/findings 会写入 result artifact；非 JSON stdout 不影响任务成功。
- Tool finding evidence ref 可被 LLM 最终结果引用并通过 Gateway 校验。
- 重复 action id 幂等，结果可回填到同一分析 revision。
- 未配置或未匹配工具时 `RUN_TOOL` 阶段直接跳过，不影响现有 LLM 结果。
- README 和 SPEC 在工具协议或结果结构变更时同步更新。
