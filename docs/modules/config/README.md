# Configuration 方案

## 目标

MVP 使用单一配置文件 `logagent.yaml`，避免每个模块各自维护零散配置。

所有模块从自己的 section 读取配置：

- `server`
- `auth`
- `storage`
- `llm`
- `embedding`
- `log_analyzer`
- `skills`
- `tools`
- `fetch`
- `huawei_cloud`
- `remote_execution`
- `claude_code`
- `mcp`
- `domain_adapters`（当前为内置 registry，暂不需要配置）
- `code_repos`
- `environments`
- `metadata`
- `analysis`
- `webui`

## 示例

```yaml
server:
  bind: "0.0.0.0:8080"
  public_base_url: "http://localhost:8080"
  max_concurrent_tasks: 2
  cors_allowed_origins:
    - "http://localhost:5173"

auth:
  api_keys:
    - name: "native-agent"
      value_env: "LOGAGENT_NATIVE_API_KEY"
    - name: "webui"
      value_env: "LOGAGENT_WEB_API_KEY"

storage:
  data_dir: "${LOGAGENT_APP_DIR}/data"
  max_upload_bytes: 2147483648
  max_files_per_task: 20

skills:
  enabled: true
  roots:
    - skills
  max_skill_chars: 4000
  max_reference_chars: 20000

llm:
  provider: "openai_compatible"
  base_url_env: "LOGAGENT_LLM_BASE_URL"
  api_key_env: "LOGAGENT_LLM_API_KEY"
  model_env: "LOGAGENT_LLM_MODEL"
  max_input_chars: 60000
  max_output_tokens: 4096
  request_timeout_seconds: 120

# 预留 binary provider 示例：
# llm:
#   provider: "binary"
#   binary_path_env: "LOGAGENT_LLM_BINARY_PATH"
#   model: "binary-reserved"
#   binary_max_output_bytes: 1048576
#   max_input_chars: 60000
#   max_output_tokens: 4096
#   request_timeout_seconds: 120

claude_code:
  command_path_env: "LOGAGENT_CLAUDE_CODE_PATH"
  default_mode: "diagnose"
  max_session_seconds: 600
  max_output_bytes: 1048576

mcp:
  enabled: true
  transport: "stdio"

fetch:
  enabled: false
  secret_key_env: "LOGAGENT_FETCH_SECRET_KEY"
  allowed_hosts:
    - "http://127.0.0.1:8091"
  request_timeout_seconds: 30
  max_request_bytes: 1048576
  max_response_bytes: 2097152
  max_redirects: 3

huawei_cloud:
  package_sync:
    enabled: false
    timeout_seconds: 60
    obs:
      endpoint: "https://obs.cn-north-4.myhuaweicloud.com"
      bucket: "example-bucket"
      object_prefix: "packages"
      access_key_env: "LOGAGENT_HUAWEI_OBS_ACCESS_KEY"
      secret_key_env: "LOGAGENT_HUAWEI_OBS_SECRET_KEY"
      security_token_env: "LOGAGENT_HUAWEI_OBS_SECURITY_TOKEN"
    gaussdb:
      host: "127.0.0.1"
      port: 8000
      database: "postgres"
      user: "gaussdb"
      password_env: "LOGAGENT_HUAWEI_GAUSSDB_PASSWORD"
      sslmode: "disable"

tools:
  flux_query_analyzer:
    enabled: true
    path_env: LOGAGENT_TOOL_FLUX_QUERY_ANALYZER
    timeout_seconds: 30
    max_output_bytes: 1048576
    max_input_files: 3
    args:
      - "--input"
      - "{input_file}"
      - "--format"
      - "json"
    match:
      file_patterns:
        - "*.flux"
        - "*.log"
      keywords:
        - "flux"
        - "query"

  influxql_analyzer:
    enabled: true
    path: "${LOGAGENT_APP_DIR}/bin/tools/influxql-analyzer"
    timeout_seconds: 30
    max_output_bytes: 1048576
    max_input_files: 3
    args:
      - "-input"
      - "{input_file}"
      - "-output"
      - "json"
      - "-detail-limit"
      - "5"
    match:
      file_patterns:
        - "*.jsonl"
      keywords:
        - "influxql"
        - "\"query\""
        - "select"
        - "show series"

  pprof_analyzer:
    enabled: true
    path_env: LOGAGENT_TOOL_PPROF_GO
    timeout_seconds: 30
    max_output_bytes: 1048576
    max_input_files: 1

remote_execution:
  enabled: true
  ssh_binary: "/usr/bin/ssh"
  host_key_policy: "accept-new"
  connect_timeout_seconds: 10
  command_timeout_seconds: 30
  max_output_bytes: 1048576
  commands:
    smoke_ls_root:
      display_name: "Smoke: list /root"
      description: "Low-risk SSH smoke command for managed ECS executors."
      argv: ["ls", "-la", "/root"]
      timeout_seconds: 10

analysis:
  max_rounds: 4
  max_llm_calls: 4
  max_actions: 6
  max_repeated_action_fingerprints: 1
  max_total_tokens: 200000
  max_runtime_seconds: 300
  max_user_prompts: 3
  max_approvals: 3

embedding:
  provider: "openai_compatible"
  model: "text-embedding-3-small"
  api_key_env: "LOGAGENT_EMBEDDING_API_KEY"
  store: "sqlite"

log_analyzer:
  rg_path: "rg"
  context_lines: 50
  keywords:
    - error
    - exception
    - timeout
    - fail
    - failed
    - panic
    - fatal
    - refused
    - denied
    - verify

# Python V2 当前通过 LOGAGENT_V2_CODE_REPOS_JSON 承载同类配置。
code_repos:
  influxdb:
    repo_path: "/data/repos/influxdb"
    default_ref: "main"
    version_refs:
      "3.0.2": "v3.0.2"
      "3.0.1": "v3.0.1"
    search_roots:
      - "query"
      - "storage"
      - "influxql"
      - "flux"

code_evidence:
  worktree_root: "/data/logagent/code_worktrees"
  max_worktrees_per_repo: 5
  cleanup_policy: "least_recently_used"

environment_collector:
  max_parallel_nodes: 4
  connect_timeout_seconds: 10
  command_timeout_seconds: 30
  retries: 1
  # Python V2 当前通过 LOGAGENT_V2_REMOTE_FILES_JSON 承载单文件 SCP 白名单。
  remote_files:
    tsdb_log:
      display_name: "TSDB log"
      remote_path: "/var/log/opengemini/tsdb.log"
      max_bytes: 16777216

metadata:
  store: "json"
  data_dir: "/data/logagent/metadata"
  allowed_template_types:
    - "yaml"
    - "json"
    - "opengemini"
    - "csv" # reserved, not implemented yet
```

## 原则

- 密钥不直接写入配置文件，只引用环境变量。
- `storage.data_dir`、`skills.roots[]` 和 `tools.<name>.path` 支持 `${VAR}` 环境变量展开；例如运行目录部署可使用 `${LOGAGENT_APP_DIR}/data` 和 `${LOGAGENT_APP_DIR}/bin/tools/influxql-analyzer`。缺少变量或变量为空时 Server 启动失败。
- `native_agent.state_path` 保存本机活动 Session，Chrome 导入默认附加到该 Session；缺省为 `~/.logagent/native-agent-state.json`。
- 用户输入不能覆盖白名单路径、白名单命令或代码仓地址。
- `rg`、外部工具、SSH key、repo path 都在启动时做存在性校验。
- Analysis Agent 预算必须有有限默认值，不能通过用户消息提高。
- `server.max_concurrent_tasks` 控制单 Server 进程后台任务并发，缺省为 2，非正值按 1 处理。
- `llm.provider` 默认 `stub`；`openai_compatible` 从 `base_url_env` 和 `api_key_env` 读取真实连接信息。V2 `LOGAGENT_V2_AGENT_PROVIDER` 只允许 `stub`、`openai_compatible`、`binary` 或 `claude_code`；选择 `openai_compatible` 时 `LOGAGENT_V2_AGENT_BASE_URL`、`LOGAGENT_V2_AGENT_MODEL` 和 `LOGAGENT_V2_AGENT_API_KEY` 必须非空。
- `llm.model_env` 可选；配置后从对应环境变量读取模型名并优先于静态 `llm.model`，变量缺失或值为空时启动失败。
- `llm.provider: "binary"` 为预留二进制模型调用分支；`binary_path` 或 `binary_path_env` 解析结果必须是绝对路径。V2 `LOGAGENT_V2_AGENT_BINARY_PATH` 选择 binary provider 时必须解析为绝对路径，运行时固定调用 `<binary_path> run <prompt>`，stdout 按结构化 LLM JSON 解析。
- `llm.binary_max_output_bytes` 默认 1MiB，非正值按 1024 bytes 下限处理。
- `claude_code.command_path` 或 `command_path_env` 必须解析为绝对路径；默认环境变量为 `LOGAGENT_CLAUDE_CODE_PATH`。Python V2 选择 `LOGAGENT_V2_AGENT_PROVIDER=claude_code` 时要求 `LOGAGENT_V2_CLAUDE_CODE_PATH` 或兼容的 `LOGAGENT_CLAUDE_CODE_PATH` 解析为绝对路径，诊断和运行时还会校验 regular/executable。
- `claude_code.default_mode` 支持 `diagnose`、`code_investigation` 和 `fix`，默认 `diagnose`。
- `claude_code.max_session_seconds` 控制一次 Claude Code headless session 的最长运行时间，推荐值为 600 秒，避免较大日志包的 MCP 检索和工具调用在规划分析阶段过早超时。
- `claude_code.permission_profiles` 可覆盖各模式的 `permission_mode`、native tools、allowed/disallowed tools 和 worktree 要求。Server 会自动给所有 profile 的 `allowed_tools` 追加 `mcp__logagent__*`，使 `dontAsk` 模式下的任务 MCP tools 不需要用户侧 Claude CLI 交互批准；`diagnose` 的 `tools: ""` 仍表示禁用 built-in native tools。
- Python V2 对应提供 `LOGAGENT_V2_CLAUDE_CODE_PERMISSION_PROFILES_JSON`，JSON
  object 以 `diagnose`、`code_investigation`、`fix` 为 key；扁平
  `LOGAGENT_V2_CLAUDE_CODE_PERMISSION_MODE`、`TOOLS`、`ALLOWED_TOOLS` 和
  `DISALLOWED_TOOLS` 仅作为 `diagnose` profile 的兼容覆盖。
- `mcp.enabled` 默认 true，`mcp.transport` 当前只支持 `stdio`。
- `fetch.enabled` 默认 false。启用时必须配置 `fetch.secret_key_env`；V2 使用 `LOGAGENT_V2_FETCH_SECRET_KEY`，对应环境变量值必须是 32-byte base64 key，并且 `fetch.allowed_hosts` / `LOGAGENT_V2_FETCH_ALLOWED_HOSTS` 不能为空。
- `fetch.allowed_hosts` / `LOGAGENT_V2_FETCH_ALLOWED_HOSTS` 条目可写为 `host`、`host:port` 或 `http(s)://host[:port]`；URL 形式会固定 scheme 和端口，省略端口时使用默认端口。Fetch 执行只允许命中这些 `http/https` 目标，每个 redirect hop 都重新校验。
- `fetch.request_timeout_seconds`、`fetch.max_request_bytes`、`fetch.max_response_bytes` 和 `fetch.max_redirects` 控制内置 Fetch tool 的请求超时、请求体大小、响应体大小和 redirect 次数。
- Python V2 runtime 使用环境变量承载同类边界；`LOGAGENT_V2_FETCH_MAX_REQUEST_BYTES` 默认 1048576，用于限制保存的 endpoint body 和运行时 body override 的 UTF-8 字节数。V2 `LOGAGENT_V2_MAX_CONCURRENT_JOBS`、Fetch timeout、request-byte cap 和 response-byte cap 的非正值按 1 处理，Fetch redirect 上限的负值按 0 处理。
- `huawei_cloud.package_sync.enabled` 默认 false。启用时必须配置 OBS `endpoint`、`bucket`、`access_key_env`、`secret_key_env` 和 GaussDB `host`、`database`、`user`、`password_env`；V2 当前使用 `LOGAGENT_V2_HUAWEI_OBS_*` 和 `LOGAGENT_V2_HUAWEI_GAUSSDB_DSN` 扁平环境变量。对应环境变量缺失或为空会导致启动失败。禁用时不读取这些环境变量。
- `huawei_cloud.package_sync.obs.endpoint` / `LOGAGENT_V2_HUAWEI_OBS_ENDPOINT` 只支持 `http/https` 且不能带 path/query/fragment；`object_prefix` 可为空，但非空时必须是安全相对 object key 前缀。
- `huawei_cloud.package_sync.obs.security_token_env` 可选，用于临时凭据；配置后启用时同样必须存在。
- `huawei_cloud.package_sync.gaussdb.port` 默认 8000；首版 `sslmode` 只支持 `disable`。
- `huawei_cloud.package_sync.timeout_seconds` 控制 OBS PUT、GaussDB update、OBS HEAD 和 GaussDB query 每个步骤的独立 timeout，非正值按 1 秒处理。
- `skills.enabled` 默认 true，`skills.roots` 默认 `skills`；相对路径优先按配置文件目录解析，目录不存在时回退到当前工作目录。
- `skills.max_skill_chars` 控制写入 `system_context.json` 的 SKILL.md 注入片段上限，`skills.max_reference_chars` 控制 MCP 按需读取 reference 的正文上限。
- Python V2 Agent 预算对应环境变量包括 `LOGAGENT_V2_AGENT_MAX_ROUNDS`、
  `LOGAGENT_V2_AGENT_MAX_LLM_CALLS`、`LOGAGENT_V2_AGENT_MAX_ACTIONS`、
  `LOGAGENT_V2_AGENT_MAX_REPEATED_ACTION_FINGERPRINTS`、
  `LOGAGENT_V2_AGENT_MAX_TOTAL_TOKENS`、
  `LOGAGENT_V2_AGENT_MAX_RUNTIME_SECONDS` 和
  `LOGAGENT_V2_AGENT_MAX_USER_PROMPTS`、
  `LOGAGENT_V2_AGENT_MAX_APPROVALS`；非正值统一按 1 处理。
- 当前 `PLAN_ANALYSIS` 检查 session 轮数、Claude 调用次数、动作数、重复
  MCP tool fingerprint、provider usage token、单次 graph invocation 运行时间和
  用户追问/审批次数预算；日志搜索和领域工具执行由 Claude Code 通过 LogAgent
  MCP tools 请求并由 Server 持久化。
- 当前结果调用会对解析/schema 错误做一次修正重试，`max_input_chars` 用于裁剪 grep evidence。
- `tools.<name>` 的 name 只允许非空 ASCII 字母、数字、`_` 和 `-`；内置 `logagent.*` 工具不通过该配置命名空间声明。
- `tools.<name>.path` 或 `tools.<name>.path_env` 启用时必须解析为绝对路径；固定 `path` 可使用 `${ENV}` 占位符；参数只支持 `{input_file}`、`{manifest_path}`、`{grep_results_path}`、`{workspace}`、`{action_id}` 占位符。
- `tools.<name>.match.file_patterns` 和 `keywords` 加载后统一转小写。
- `tools.<name>.max_input_files` 控制规则版 Tool Runner 在单个任务中最多为该工具生成多少个输入文件 action，默认 1，非正值按 1 处理。
- 真实 `flux_query_analyzer`、`influxql_analyzer`、`opengemini_storage_analyzer` 和 `influxdb_storage_analyzer` 源码通过 `third_party/` submodules 引用，推荐运行 `scripts/build-tools.sh` 后用 `examples/server-*-tool.yaml` 或对应 smoke 脚本验证；deploy/runtime 配置可直接把 `path` 指到 `${LOGAGENT_APP_DIR}/bin/tools/...`。如果源码 submodule 需要走内网镜像，可设置 `LOGAGENT_SUBMODULE_BASE_URL`，或按仓库分别设置 `LOGAGENT_SUBMODULE_FLUX_URL`、`LOGAGENT_SUBMODULE_INFLUXQL_URL`、`LOGAGENT_SUBMODULE_OPENGEMINI_URL`、`LOGAGENT_SUBMODULE_INFLUXDB_URL`。
- `influxql_analyzer` 输入为 JSONL 查询日志，参数为 `-input {input_file} -output json -detail-limit 5`；`flux_query_analyzer` 输入为 Flux 查询 JSONL/NDJSON，参数为 `--input {input_file} --format json` 加 bounded top/error 参数；两个 storage analyzers 输入为只读存储文件或目录。
- `pprof_analyzer` 推荐使用 `examples/server-pprof-tool.yaml` 验证；`path` / `path_env` 指向 Go 可执行文件，Server 固定调用 `go tool pprof` 并生成 top/tree/raw 产物。V2 默认关闭 pprof；启用时 `LOGAGENT_V2_PPROF_GO_COMMAND` / `LOGAGENT_TOOL_PPROF_GO` 必须解析为绝对路径。
- 禁用工具不读取 `path_env`，便于在模板配置中保留未安装工具。
- 未配置 `tools` 时 `RUN_TOOL` 阶段无副作用跳过。
- Python V2 未设置 `LOGAGENT_V2_REMOTE_COMMANDS_JSON` 时默认暴露 `smoke_ls_root`、`system_uname`、`uptime_load`、`disk_usage`、`memory_usage`、`process_overview`、`network_listeners`、`opengemini_processes`、`opengemini_config_dirs`、`opengemini_log_dirs`、`opengemini_data_dirs`、`cassandra_processes`、`cassandra_config_dirs`、`cassandra_log_dirs`、`cassandra_data_dirs`、`rocksdb_data_dirs`、`rocksdb_wal_dirs` 和 `rocksdb_log_dirs` 只读命令模板；产品模板只使用固定进程名和常见目录候选，不允许 shell 管道、重定向、glob 或用户输入 argv；设置该环境变量会替换整套默认模板。
- `remote_execution.enabled` 默认 true；`ssh_binary` 启用时必须是绝对路径。
- `remote_execution.scp_binary` / V2 `LOGAGENT_V2_REMOTE_SCP_COMMAND` 启用时必须是绝对路径，默认 `/usr/bin/scp`。
- `remote_execution.host_key_policy` 仅支持 `accept-new`、`strict` 和 `no`，分别映射到 OpenSSH `StrictHostKeyChecking`。
- `remote_execution.commands.<id>` 的 ID 只允许非空 ASCII 字母、数字、`_` 和 `-`。
- `remote_execution.commands.<id>.argv` 是 WebUI 可选择的唯一远程命令来源；用户不能输入自由 shell 命令或扩展 argv。
- `remote_execution.commands.<id>.argv` 加载时会逐项 trim 并丢弃空字符串，归一化后仍必须至少保留一个 argv 项。
- V2 `LOGAGENT_V2_REMOTE_FILES_JSON` 配置 approved `collect_environment`
  可拉取的单文件模板；`fileId` 复用 command id 安全规则，`remotePath` 必须是
  绝对安全路径并拒绝 `..`、`.`、`//`、反斜杠、空格、glob 和非安全字符。
- V2 `LOGAGENT_V2_REMOTE_FILE_MAX_BYTES` 是没有模板级 `maxBytes` 时的默认
  SCP 文件大小上限，默认 16MiB。
- 等待用户和等待审批时间不计入 `max_running_seconds`。
- V2 `LOGAGENT_V2_CODE_REPOS_JSON` 支持 object keyed by product 或 descriptor array；每个 repo 必须提供绝对 `repoPath`，可配置 `defaultRef`、`versionRefs` 和安全相对 `searchRoots`。启用后 task MCP 和 provider prompt 才会广告 `logagent.search_code`。
- V2 `LOGAGENT_V2_CODE_WORKTREE_ROOT` 对应 `code_evidence.worktree_root`；
  未设置时使用 `storage.data_dir/code_worktrees`。
- V2 `LOGAGENT_V2_CODE_WORKTREE_MAX_PER_REPO` 对应
  `code_evidence.max_worktrees_per_repo`，默认 5，非正值按 1 处理；超过上限时按
  least-recently-used 清理同 product 的旧 detached worktree。
- `code_repos` 只能指向管理员预同步的本地 git repo；用户或模型不能覆盖 repo path、search roots，也不能传入未配置的 `gitRef`。
