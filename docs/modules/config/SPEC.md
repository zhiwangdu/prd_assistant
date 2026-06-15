# Config Spec

## 目标

统一使用 `logagent.yaml` 描述 Server、Native Agent、存储、安全和模块配置。

## 当前状态

Server 和 Native Agent 已读取部分配置。示例文件：

- `examples/logagent.yaml`
- `examples/server-test.yaml`
- `examples/server-tools.yaml`
- `examples/server-influxql-tool.yaml`
- `examples/server-pprof-tool.yaml`
- `examples/native-agent-remote-50992.yaml`

## 配置范围

当前已用：

- `server.bind`
- `server.public_base_url`
- `server.max_concurrent_tasks`
- `native_agent.bind`
- `native_agent.server_base_url`
- `native_agent.api_key_env`
- `native_agent.allowed_dirs`
- `native_agent.allowed_suffixes`
- `native_agent.request_timeout_seconds`
- `native_agent.upload_chunk_bytes`
- `native_agent.state_path`
- `storage.data_dir`
- `storage.max_upload_bytes`
- `storage.max_chunk_bytes`
- `skills.enabled`
- `skills.roots`
- `skills.max_skill_chars`
- `skills.max_reference_chars`
- `auth.api_keys`
- `log_analyzer.keywords`
- `log_analyzer.max_matches`
- `llm.provider`
- `llm.base_url_env`
- `llm.api_key_env`
- `llm.binary_path`
- `llm.binary_path_env`
- `llm.binary_max_output_bytes`
- `llm.model_env`
- `llm.model`
- `llm.request_timeout_seconds`
- `llm.max_input_chars`
- `llm.max_output_tokens`
- `claude_code.command_path`
- `claude_code.command_path_env`
- `claude_code.default_mode`
- `claude_code.max_session_seconds`
- `claude_code.max_output_bytes`
- `claude_code.permission_profiles.<mode>.*`
- `claude_code.permission_profiles.<mode>.allowed_tools` 会自动追加 `mcp__logagent__*`，保证任务 MCP tools 在 `dontAsk` 模式下可用。
- `mcp.enabled`
- `mcp.transport`
- `tools.<name>.enabled`
- `tools.<name>.path`
- `tools.<name>.path_env`
- `tools.<name>.timeout_seconds`
- `tools.<name>.max_output_bytes`
- `tools.<name>.max_input_files`
- `tools.<name>.args`
- `tools.<name>.match.file_patterns`
- `tools.<name>.match.keywords`
- `tools.pprof_analyzer.path` / `path_env`，首版必须指向 Go 可执行文件，Server 固定追加 `tool pprof` 子命令。
- `remote_execution.enabled`
- `remote_execution.ssh_binary`
- `remote_execution.host_key_policy`
- `remote_execution.connect_timeout_seconds`
- `remote_execution.command_timeout_seconds`
- `remote_execution.max_output_bytes`
- `remote_execution.commands.<command_id>.display_name`
- `remote_execution.commands.<command_id>.description`
- `remote_execution.commands.<command_id>.enabled`
- `remote_execution.commands.<command_id>.argv`
- `remote_execution.commands.<command_id>.timeout_seconds`
- `analysis.max_rounds`
- `analysis.max_llm_calls`
- `analysis.max_actions`
- `analysis.max_repeated_action_fingerprints`

待扩展：

- product/version 到代码仓 ref 映射
- SSH/SCP 测试环境节点到 Environment Collector 的批量采集映射；当前 Remote Executor 已支持 WebUI 显式执行机和白名单 SSH 命令模板。
- metadata store 路径和模板导入限制；当前 store 使用 `storage.data_dir/metadata`，模板支持 YAML/JSON/openGemini `/getdata`
- LLM 多轮重试、用量和 request id 审计
- Analysis Orchestrator 追问、运行时间和 approval 预算
- action 审批策略
- Case Store 存储路径

## 密钥规则

配置文件只保存环境变量名，不直接保存密钥值。

```yaml
auth:
  api_keys:
    - name: "native-agent"
      value_env: "LOGAGENT_NATIVE_API_KEY"
```

路径类配置可使用 `${VAR}` 引用环境变量。当前 Server 已支持 `storage.data_dir` 展开，例如：

```yaml
storage:
  data_dir: "${LOGAGENT_APP_DIR}/data"
```

缺少被引用的环境变量、变量为空或占位符未闭合时，Server 启动失败。

## 验收标准

- 缺少必要密钥环境变量时启动失败。
- `storage.data_dir` 中的 `${VAR}` 能展开为环境变量值，缺失或空值时启动失败。
- 配置有默认值，但示例文件必须展示推荐值。
- `native_agent.state_path` 默认 `~/.logagent/native-agent-state.json`，用于保存当前活动 `sessionId`。
- `server.max_concurrent_tasks` 默认 2，并发下限为 1。
- `llm.provider` 默认 `stub`；真实 Provider 缺少 URL 或 API Key 环境变量时启动失败。
- `llm.provider` 支持 `stub`、`openai_compatible` 和预留的 `binary`。
- `llm.model_env` 配置后优先于 `llm.model`；对应环境变量缺失或模型名为空时启动失败。
- `llm.provider: "binary"` 时必须配置 `binary_path` 或 `binary_path_env`；解析后的二进制路径必须是绝对路径。
- binary provider 运行时固定调用 `<binary_path> run <prompt>`，用户输入不能覆盖二进制路径或 argv。
- `llm.binary_max_output_bytes` 默认 1MiB，非正值按 1024 bytes 下限处理。
- 未配置 `claude_code.command_path` 时默认要求 `LOGAGENT_CLAUDE_CODE_PATH`。
- `claude_code.command_path` 或 `command_path_env` 解析结果必须是绝对路径。
- `claude_code.default_mode` 仅支持 `diagnose`、`code_investigation` 和 `fix`。
- `mcp.transport` 当前只支持 `stdio`。
- `skills.enabled` 默认 true，`skills.roots` 默认 `skills`。
- `skills.max_skill_chars` 和 `skills.max_reference_chars` 有下限和上限裁剪，避免过大 prompt 或 reference artifact。
- 启用的 tool path 或 path_env 解析结果必须是绝对路径；非法工具名、相对路径、缺失/空 path_env 启动失败。
- `tools.<name>.max_input_files` 默认 1，非正值按 1 处理。
- 禁用工具不读取 `path_env`。
- 用户输入不能覆盖 tool path 或自由 argv。
- `examples/server-influxql-tool.yaml` 只启用真实 `influxql_analyzer`，用于本地 smoke；当前真实工具路径固定为 `/usr/bin/influxql-analyzer`。
- `examples/server-pprof-tool.yaml` 只启用 `pprof_analyzer`，通过 `LOGAGENT_TOOL_PPROF_GO` 指向 Go 可执行文件。
- `remote_execution.ssh_binary` 启用时必须为绝对路径，默认 `/usr/bin/ssh`。
- `remote_execution.host_key_policy` 只允许 `accept-new`、`strict` 或 `no`。
- `remote_execution.commands` 为空时内置 `smoke_ls_root`；自定义命令模板必须有非空 argv。
- WebUI Remote Executor 只能选择 `remote_execution.commands` 白名单模板，不能提交自由命令。
- Analysis 预算字段默认值为 `max_rounds=4`、`max_llm_calls=4`、`max_actions=6`、`max_repeated_action_fingerprints=1`，非正值按 1 处理。
- 用户输入不能扩展当前允许的 action 类型；未知 action 类型在 LLM schema 校验阶段失败。
- 用户输入不能修改预算、白名单和审批策略。
- README 和 SPEC 在配置字段变更时同步更新。
