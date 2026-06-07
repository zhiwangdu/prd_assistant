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
- `tools`
- `code_repos`
- `environments`
- `metadata`
- `analysis_agent`
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
  data_dir: "/data/logagent"
  max_upload_bytes: 2147483648
  max_files_per_task: 20

llm:
  provider: "openai_compatible"
  base_url_env: "LOGAGENT_LLM_BASE_URL"
  api_key_env: "LOGAGENT_LLM_API_KEY"
  model_env: "LOGAGENT_LLM_MODEL"
  max_input_chars: 60000
  max_output_tokens: 4096
  request_timeout_seconds: 120

tools:
  flux_query_analyzer:
    enabled: true
    path_env: LOGAGENT_TOOL_FLUX_QUERY_ANALYZER
    timeout_seconds: 30
    max_output_bytes: 1048576
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

analysis_agent:
  max_rounds: 12
  max_llm_calls: 12
  max_actions: 20
  max_repeated_action: 2
  max_questions_per_round: 3
  max_running_seconds: 900
  approval_required_actions:
    - collect_environment

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

code_evidence:
  worktree_root: "/data/logagent/code_worktrees"
  max_worktrees_per_repo: 5
  cleanup_policy: "least_recently_used"

environment_collector:
  max_parallel_nodes: 4
  connect_timeout_seconds: 10
  command_timeout_seconds: 30
  retries: 1

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
- 用户输入不能覆盖白名单路径、白名单命令或代码仓地址。
- `rg`、外部工具、SSH key、repo path 都在启动时做存在性校验。
- Analysis Agent 预算必须有有限默认值，不能通过用户消息提高。
- `server.max_concurrent_tasks` 控制单 Server 进程后台任务并发，缺省为 2，非正值按 1 处理。
- `llm.provider` 默认 `stub`；`openai_compatible` 从 `base_url_env` 和 `api_key_env` 读取真实连接信息。
- `llm.model_env` 可选；配置后从对应环境变量读取模型名并优先于静态 `llm.model`，变量缺失或值为空时启动失败。
- 当前单次结果调用会对解析/schema 错误做一次修正重试，`max_input_chars` 用于裁剪 grep evidence。
- `tools.<name>.path` 或 `tools.<name>.path_env` 启用时必须解析为绝对路径；参数只支持 `{input_file}`、`{manifest_path}`、`{grep_results_path}`、`{workspace}`、`{action_id}` 占位符。
- 禁用工具不读取 `path_env`，便于在模板配置中保留未安装工具。
- 未配置 `tools` 时 `RUN_TOOL` 阶段无副作用跳过。
- 等待用户和等待审批时间不计入 `max_running_seconds`。
