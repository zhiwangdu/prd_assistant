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
  model: "gpt-4.1"
  max_input_tokens: 64000
  max_output_tokens: 4096
  request_timeout_seconds: 120
  max_retries: 2

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
- 等待用户和等待审批时间不计入 `max_running_seconds`。
