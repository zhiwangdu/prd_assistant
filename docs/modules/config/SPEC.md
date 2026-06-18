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
- `examples/server-fetch.yaml`
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
- `fetch.enabled`
- `fetch.secret_key_env`
- `fetch.allowed_hosts`
- `fetch.request_timeout_seconds`
- `fetch.max_request_bytes`
- `fetch.max_response_bytes`
- `fetch.max_redirects`
- `huawei_cloud.package_sync.enabled`
- `huawei_cloud.package_sync.timeout_seconds`
- `huawei_cloud.package_sync.obs.endpoint`
- `huawei_cloud.package_sync.obs.bucket`
- `huawei_cloud.package_sync.obs.object_prefix`
- `huawei_cloud.package_sync.obs.access_key_env`
- `huawei_cloud.package_sync.obs.secret_key_env`
- `huawei_cloud.package_sync.obs.security_token_env`
- `huawei_cloud.package_sync.gaussdb.host`
- `huawei_cloud.package_sync.gaussdb.port`
- `huawei_cloud.package_sync.gaussdb.database`
- `huawei_cloud.package_sync.gaussdb.user`
- `huawei_cloud.package_sync.gaussdb.password_env`
- `huawei_cloud.package_sync.gaussdb.sslmode`
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
- `tools.<name>` 的 name 只允许非空 ASCII 字母、数字、`_` 和 `-`
- `tools.<name>.match.file_patterns` 和 `keywords` 加载后统一转小写
- `remote_execution.enabled`
- `remote_execution.ssh_binary`
- `remote_execution.host_key_policy`
- `remote_execution.connect_timeout_seconds`
- `remote_execution.command_timeout_seconds`
- `remote_execution.max_output_bytes`
- `remote_execution.scp_binary` / V2 `LOGAGENT_V2_REMOTE_SCP_COMMAND`
- `remote_execution.file_max_bytes` / V2 `LOGAGENT_V2_REMOTE_FILE_MAX_BYTES`
- `remote_execution.commands.<command_id>.display_name`
- `remote_execution.commands.<command_id>.description`
- `remote_execution.commands.<command_id>.enabled`
- `remote_execution.commands.<command_id>.argv`
- `remote_execution.commands.<command_id>.timeout_seconds`
- `remote_execution.files.<file_id>.display_name` / V2 `LOGAGENT_V2_REMOTE_FILES_JSON[].displayName`
- `remote_execution.files.<file_id>.description` / V2 `LOGAGENT_V2_REMOTE_FILES_JSON[].description`
- `remote_execution.files.<file_id>.enabled` / V2 `LOGAGENT_V2_REMOTE_FILES_JSON[].enabled`
- `remote_execution.files.<file_id>.remote_path` / V2 `LOGAGENT_V2_REMOTE_FILES_JSON[].remotePath`
- `remote_execution.files.<file_id>.timeout_seconds` / V2 `LOGAGENT_V2_REMOTE_FILES_JSON[].timeoutSeconds`
- `remote_execution.files.<file_id>.max_bytes` / V2 `LOGAGENT_V2_REMOTE_FILES_JSON[].maxBytes`
- `code_repos.<product>.repo_path` / V2 `LOGAGENT_V2_CODE_REPOS_JSON[].repoPath`
- `code_repos.<product>.default_ref` / V2 `LOGAGENT_V2_CODE_REPOS_JSON[].defaultRef`
- `code_repos.<product>.version_refs` / V2 `LOGAGENT_V2_CODE_REPOS_JSON[].versionRefs`
- `code_repos.<product>.search_roots` / V2 `LOGAGENT_V2_CODE_REPOS_JSON[].searchRoots`
- `code_evidence.worktree_root` / V2 `LOGAGENT_V2_CODE_WORKTREE_ROOT`
- `code_evidence.max_worktrees_per_repo` / V2 `LOGAGENT_V2_CODE_WORKTREE_MAX_PER_REPO`
- `analysis.max_rounds`
- `analysis.max_llm_calls`
- `analysis.max_actions`
- `analysis.max_repeated_action_fingerprints`
- `analysis.max_total_tokens`
- `analysis.max_runtime_seconds`
- `analysis.max_user_prompts`
- `analysis.max_approvals`

待扩展：

- Code Evidence 启动孤儿 worktree 扫描、版本间 diff 和 fix mode 隔离修改配置；当前 V2 已支持 product/version 到本地 git ref 的只读映射、detached worktree cache 和 LRU 清理。
- SSH/SCP 测试环境节点到 Environment Collector 的批量采集映射；当前 Remote Executor 已支持 WebUI 显式执行机、白名单 SSH 命令模板，以及 V2 审批后的单文件 SCP 模板。
- metadata store 路径和模板导入限制；当前 store 使用 `storage.data_dir/metadata`，模板支持 YAML/JSON/openGemini `/getdata`
- LLM 多轮重试、外部用量汇总和 request id 审计
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

Fetch credential encryption key 也只能通过环境变量提供：

```yaml
fetch:
  enabled: true
  secret_key_env: "LOGAGENT_FETCH_SECRET_KEY"
  allowed_hosts:
    - "http://127.0.0.1:8091"
```

`LOGAGENT_FETCH_SECRET_KEY` 必须是 32-byte base64 key。`fetch.enabled=false` 时不会读取该环境变量。

Huawei OBS + GaussDB package sync 密钥也只能通过环境变量提供；该工具默认关闭，关闭时不会读取这些环境变量：

```yaml
huawei_cloud:
  package_sync:
    enabled: true
    timeout_seconds: 60
    obs:
      endpoint: "https://obs.cn-north-4.myhuaweicloud.com"
      bucket: "example-bucket"
      object_prefix: "packages"
      access_key_env: "LOGAGENT_HUAWEI_OBS_ACCESS_KEY"
      secret_key_env: "LOGAGENT_HUAWEI_OBS_SECRET_KEY"
      security_token_env: "LOGAGENT_HUAWEI_OBS_SECURITY_TOKEN"
    gaussdb:
      host: "gaussdb.example.internal"
      port: 8000
      database: "postgres"
      user: "gaussdb"
      password_env: "LOGAGENT_HUAWEI_GAUSSDB_PASSWORD"
      sslmode: "disable"
```

路径类配置可使用 `${VAR}` 引用环境变量。当前 Server 已支持 `storage.data_dir`、`skills.roots[]` 和 `tools.<name>.path` 展开，例如：

```yaml
storage:
  data_dir: "${LOGAGENT_APP_DIR}/data"

tools:
  influxql_analyzer:
    path: "${LOGAGENT_APP_DIR}/bin/tools/influxql-analyzer"
```

缺少被引用的环境变量、变量为空或占位符未闭合时，Server 启动失败。

## 验收标准

- 缺少必要密钥环境变量时启动失败。
- `storage.data_dir`、`skills.roots[]` 和 `tools.<name>.path` 中的 `${VAR}` 能展开为环境变量值，缺失或空值时启动失败。
- 配置有默认值，但示例文件必须展示推荐值。
- `native_agent.state_path` 默认 `~/.logagent/native-agent-state.json`，用于保存当前活动 `sessionId`。
- `server.max_concurrent_tasks` 默认 2，并发下限为 1。
- `llm.provider` 默认 `stub`；真实 Provider 缺少 URL、模型名、API Key 或 CLI 路径环境变量时启动失败。V2 `LOGAGENT_V2_AGENT_PROVIDER` 只允许 `stub`、`openai_compatible`、`binary` 或 `claude_code`。
- `llm.provider` 支持 `stub`、`openai_compatible`、预留的 `binary` 和 Claude Code CLI provider。
- `llm.model_env` 配置后优先于 `llm.model`；对应环境变量缺失或模型名为空时启动失败。
- `llm.provider: "binary"` 时必须配置 `binary_path` 或 `binary_path_env`；解析后的二进制路径必须是绝对路径。V2 `LOGAGENT_V2_AGENT_BINARY_PATH` 在选择 binary provider 时必须解析为绝对路径。
- binary provider 运行时固定调用 `<binary_path> run <prompt>`，用户输入不能覆盖二进制路径或 argv。
- `llm.binary_max_output_bytes` 默认 1MiB，非正值按 1024 bytes 下限处理。
- 未配置 `claude_code.command_path` 时默认要求 `LOGAGENT_CLAUDE_CODE_PATH`。Python V2 选择 `LOGAGENT_V2_AGENT_PROVIDER=claude_code` 时要求 `LOGAGENT_V2_CLAUDE_CODE_PATH` 或兼容的 `LOGAGENT_CLAUDE_CODE_PATH`。
- `claude_code.command_path` 或 `command_path_env` 解析结果必须是绝对路径；Python V2 运行时还会校验 regular/executable。
- `claude_code.default_mode` 仅支持 `diagnose`、`code_investigation` 和 `fix`。
- Python V2 必须支持 `LOGAGENT_V2_CLAUDE_CODE_PERMISSION_PROFILES_JSON`
  覆盖 `diagnose`、`code_investigation`、`fix` 的 permission profile，并自动给
  每个 profile 的 allowed tools 补入 `mcp__logagent__*`。扁平
  `LOGAGENT_V2_CLAUDE_CODE_PERMISSION_MODE`、`LOGAGENT_V2_CLAUDE_CODE_TOOLS`、
  `LOGAGENT_V2_CLAUDE_CODE_ALLOWED_TOOLS` 和
  `LOGAGENT_V2_CLAUDE_CODE_DISALLOWED_TOOLS` 只覆盖 `diagnose` profile，以兼容旧
  V2 部署。
- `claude_code.max_session_seconds` 默认 600 秒，控制单次 Claude Code session 的超时边界；显式配置非正值按 1 秒下限裁剪。
- `mcp.transport` 当前只支持 `stdio`。
- `fetch.enabled` 默认 false；启用时必须配置非空 `fetch.allowed_hosts` / `LOGAGENT_V2_FETCH_ALLOWED_HOSTS` 和可解码为 32-byte 原始 key 的 `fetch.secret_key_env` / `LOGAGENT_V2_FETCH_SECRET_KEY` 环境变量。
- `fetch.allowed_hosts` / `LOGAGENT_V2_FETCH_ALLOWED_HOSTS` 支持 `host`、`host:port` 和 `http(s)://host[:port]`；URL 形式会固定 scheme 和端口，省略端口时使用默认端口。Fetch 执行、redirect hop 和运行时 URL template 解析结果都必须命中 allowlist。
- `fetch.request_timeout_seconds`、`fetch.max_request_bytes`、`fetch.max_response_bytes` 和 `fetch.max_redirects` 必须有有限默认值；非正或缺省值按安全默认裁剪。
- Python V2 必须提供等价请求体边界：`LOGAGENT_V2_FETCH_MAX_REQUEST_BYTES` 默认 1048576，保存的 endpoint body 和运行时 body override 超过该 UTF-8 字节数时必须在发出 HTTP 请求前拒绝。V2 `LOGAGENT_V2_MAX_CONCURRENT_JOBS`、Fetch timeout、request-byte cap 和 response-byte cap 的非正值按 1 处理，Fetch redirect 上限的负值按 0 处理。
- `huawei_cloud.package_sync.enabled` 默认 false；禁用时不读取 OBS/GaussDB 密钥环境变量。
- 启用 Huawei package sync 时，OBS endpoint 必须是 `http/https` 且无 path/query/fragment，bucket 必须非空且只含字母、数字、`.` 或 `-`，access/secret key 环境变量必须存在且非空。V2 当前使用 `LOGAGENT_V2_HUAWEI_OBS_*` 和 `LOGAGENT_V2_HUAWEI_GAUSSDB_DSN` 扁平环境变量。
- 启用 Huawei package sync 时，GaussDB host/database/user/password_env 必须非空，password 环境变量必须存在且非空；`sslmode` 首版只允许 `disable`。
- Huawei OBS `object_prefix` 和运行时 `objectKey` 必须是安全相对 object key，不能包含 `..`、空 path segment、反斜杠、`?`、`#` 或控制字符。
- `huawei_cloud.package_sync.timeout_seconds` 非正值按 1 秒处理，`gaussdb.port` 默认 8000。
- `skills.enabled` 默认 true，`skills.roots` 默认 `skills`。
- `skills.max_skill_chars` 和 `skills.max_reference_chars` 有下限和上限裁剪，避免过大 prompt 或 reference artifact。
- 启用的 tool path 或 path_env 解析结果必须是绝对路径；固定 `path` 支持 `${ENV}` 展开；非法工具名、相对路径、缺失/空 path_env 启动失败。
- `tools.<name>.max_input_files` 默认 1，非正值按 1 处理。
- 禁用工具不读取 `path_env`。
- 用户输入不能覆盖 tool path 或自由 argv。
- `examples/server-flux-tool.yaml`、`examples/server-influxql-tool.yaml`、`examples/server-opengemini-storage-tool.yaml` 和 `examples/server-influxdb-storage-tool.yaml` 分别只启用一个真实工具，用于本地 smoke；真实工具由 `scripts/build-tools.sh` 从 `third_party/` submodules 构建，并通过对应 `LOGAGENT_TOOL_*` 环境变量指向产物。构建阶段支持 `LOGAGENT_SUBMODULE_BASE_URL` 和单仓库 `LOGAGENT_SUBMODULE_*_URL` 覆盖 submodule clone 地址，部署配置可以把这些变量放在 `.env`。
- `examples/server-pprof-tool.yaml` 只启用 `pprof_analyzer`，通过 `LOGAGENT_TOOL_PPROF_GO` 指向 Go 可执行文件。V2 默认关闭 pprof；启用时 `LOGAGENT_V2_PPROF_GO_COMMAND` / `LOGAGENT_TOOL_PPROF_GO` 必须解析为绝对路径。
- `remote_execution.ssh_binary` 启用时必须为绝对路径，默认 `/usr/bin/ssh`。
- `remote_execution.scp_binary` / V2 `LOGAGENT_V2_REMOTE_SCP_COMMAND` 启用时必须为绝对路径，默认 `/usr/bin/scp`。
- `remote_execution.host_key_policy` 只允许 `accept-new`、`strict` 或 `no`。
- `remote_execution.commands` 为空时 V2 内置 `smoke_ls_root`、`system_uname`、`uptime_load`、`disk_usage`、`memory_usage`、`process_overview`、`network_listeners`、`opengemini_processes`、`opengemini_config_dirs`、`opengemini_log_dirs` 和 `opengemini_data_dirs` 只读模板；openGemini 模板只使用固定进程名和常见目录候选，不允许 shell 管道、重定向或用户输入 argv；自定义命令模板 ID 只允许非空 ASCII 字母、数字、`_` 和 `-`，并且必须有非空 argv。V2 `LOGAGENT_V2_REMOTE_COMMANDS_JSON` 会替换整套默认模板。
- `remote_execution.commands.<id>.argv` 加载时逐项 trim 并丢弃空字符串；归一化后为空时启动失败。
- V2 `LOGAGENT_V2_REMOTE_FILES_JSON` 配置 approved `collect_environment`
  可拉取的单文件模板；`fileId` 复用 command id 安全规则，`remotePath` 必须是
  绝对安全路径并拒绝 `..`、`.`、`//`、反斜杠、空格、glob 和非安全字符。
- V2 `LOGAGENT_V2_REMOTE_FILE_MAX_BYTES` 是没有模板级 `maxBytes` 时的默认
  SCP 文件大小上限，默认 16MiB。
- WebUI Remote Executor 只能选择 `remote_execution.commands` 白名单模板，不能提交自由命令。
- V2 `LOGAGENT_V2_CODE_REPOS_JSON` 支持 object keyed by product 或 descriptor array；repo path 必须是已存在绝对目录，`defaultRef` / `versionRefs` 必须是安全 git ref，`searchRoots` 必须是安全相对路径且会去重。
- V2 `LOGAGENT_V2_CODE_WORKTREE_ROOT` 显式配置时必须是绝对路径；未配置时使用
  `storage.data_dir/code_worktrees`。
- V2 `LOGAGENT_V2_CODE_WORKTREE_MAX_PER_REPO` 默认 5，非正值按 1 处理；超过上限
  时 Code Evidence search 必须按 least-recently-used 清理同 product 的旧 `wt_*`
  worktree。
- `logagent.search_code` 只能访问配置仓库、配置版本 ref 和配置 search roots；未配置仓库时不在 task MCP 或 provider prompt 中广告。
- Analysis 预算字段默认值为 `max_rounds=4`、`max_llm_calls=4`、
  `max_actions=6`、`max_repeated_action_fingerprints=1`、
  `max_total_tokens=200000`、`max_runtime_seconds=300`、
  `max_user_prompts=3`、`max_approvals=3`，非正值按 1 处理。
- 用户输入不能扩展当前允许的 action 类型；未知 action 类型在 LLM schema 校验阶段失败。
- 用户输入不能修改预算、白名单和审批策略。
- README 和 SPEC 在配置字段变更时同步更新。
