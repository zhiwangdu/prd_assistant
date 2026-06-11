# Tool Runner Spec

## 目标

Tool Runner 负责按白名单调用已有外部分析工具，把结果标准化为任务证据，供 Domain Adapter、Claude Code 和最终结果引用。

## 当前状态

Server 已实现 Tool Runner MVP：配置白名单、规则版工具 action、Claude MCP `logagent.run_domain_tool`、可恢复 `RUN_TOOL` phase、timeout、stdout/stderr/result 持久化、stdout JSON 摘要解析和 artifacts API 展示。真实工具可通过固定 `path` 或 `path_env` 环境变量接入，`examples/server-tools.yaml` 提供 `flux_query_analyzer` / `influxql_analyzer` 模板，`examples/server-influxql-tool.yaml` 提供单独验证真实 InfluxQL 工具的模板。当前本机 `influxql-analyzer` 已安装到 `/usr/bin/influxql-analyzer`，相关文档和代码在 `/home/duzhiwang/workspace/influxql`。

Server 也已实现面向 WebUI 手动执行的 Tools API。`tool_run` task 复用上传、workspace、TaskStore 和后台 Executor；首个 `pprof_analyzer` 通过 `tools.pprof_analyzer` 白名单配置调用 Go 可执行文件的 `tool pprof` 子命令，结果仍写入 `tool_results/<action_id>/`。

## 首批工具

- `flux_query_analyzer`
- `influxql_analyzer`，真实 CLI 已验证，默认本机路径为 `/usr/bin/influxql-analyzer`，参数为 `-input <file> -output json -detail-limit 5`
- `pprof_analyzer`，通过 `LOGAGENT_TOOL_PPROF_GO` 指向 Go 可执行文件，Server 固定调用 `go tool pprof -top/-tree/-raw`

## 输入

- Task workspace
- 工具名称
- `action_id`
- 工具参数模板
- 工具路径，来自固定 `path` 或 `path_env` 环境变量
- `max_input_files`，单个工具在同一任务中最多自动选择的输入文件数量，默认 1
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

真实 `influxql-analyzer` 的 Report stdout 会被专门适配：

- `total_records`、`records_in_window`、`total_statements`、`parse_error_count` 进入 summary。
- `special_rules` 进入 findings，例如 `large_limit`、`no_time_filter`、`group_by_high_cardinality_risk`、`meta_query`。
- `parse_errors` 进入 high severity findings。
- `realtime_query.non_realtime` / `unknown` 进入实时性分类 findings。
- 有规则命中的高频 fingerprint 进入低优先级 query statistics findings。

真实 `influxql-analyzer` CompareReport stdout 也会被专门适配：

- `statement_delta`、`qps_delta`、`batch_a` 和 `batch_b` 进入 summary。
- `new_fingerprints` / `removed_fingerprints` / `changed_fingerprints` 进入 findings，包含 statement type、count A->B、qps A->B、delta、rules 和 normalized query。
- `rule_deltas` 进入 findings，包含 rule、count A->B 和 qps A->B。
- 新增 fingerprint 和正向规则增长默认 high severity，移除 fingerprint 默认 low severity。

当前本机尚未安装 `flux_query_analyzer` / `flux-query-analyzer`，真实 Flux smoke 需要等待二进制就绪后再执行。

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
- Claude Code 只能通过 `logagent.run_domain_tool` 选择允许的工具和结构化参数，不能传入任意命令。

## 验收标准

- 配置不存在的工具不可调用。
- `path_env` 缺失、为空或解析出非绝对路径时启动失败。
- 工具超时后任务记录失败原因。
- stdout/stderr 可追溯。
- JSON stdout 中的 summary/findings 会写入 result artifact；非 JSON stdout 不影响任务成功。
- Tool finding evidence ref 可被 LLM 最终结果引用并通过 Gateway 校验。
- `pprof_analyzer` 手动运行必须创建 `tool_run` task，成功后 `/api/tools/runs/:task_id/result` 返回 profile type、top 表格和 artifact 路径。
- 规则版 action 选择必须先使用 manifest file pattern，再使用 grep keyword 补充候选；同一工具最多生成 `max_input_files` 个 action。
- 同一工具的不同输入文件必须生成不同稳定 action id。
- 重复 action id 幂等，结果可回填到同一分析 revision。
- 未配置或未匹配工具时 `RUN_TOOL` 阶段直接跳过，不影响现有 Claude Code 分析结果。
- README 和 SPEC 在工具协议或结果结构变更时同步更新。
