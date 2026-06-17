# 安全与可靠性方案

## API 和上传

- 服务端 API 使用简单 API Key。
- API Key 从 `logagent.yaml` 引用的环境变量读取。
- 启动时检查 API Key 是否存在。
- API Key 不写入任务日志，不进入 LLM Prompt。
- Native Agent 只接受 `127.0.0.1` 请求。
- Native Agent 校验文件路径，避免任意文件上传。
- 不在日志或数据库中保存 Cookie、Authorization、session token。
- Fetch endpoint 导入的 Authorization、Cookie、token/api_key/secret/password/session 等敏感值只进入 Server credential store，并用 AES-256-GCM 加密持久化；API、WebUI、日志、artifact 和 LLM evidence 只展示脱敏值。
- Huawei package sync 的 OBS access key、secret key、可选 security token 和 GaussDB password 只从环境变量读取；工具目录、日志和 `tool_results` 只记录环境变量名、对象 key、状态和摘要，不记录密钥值或原始 SQL。
- 上传文件大小限制。
- 任务失败要保留错误信息，方便定位。

## LLM 约束

- LLM 输出必须保留原始证据引用。
- LLM 不能直接执行任意命令。
- SSH key、API Key、repo path 等敏感配置不进入 Prompt。
- LLM Provider、model、base_url、token 预算必须配置化。
- `llm.provider: "binary"` 只能调用配置中的绝对路径二进制，固定 argv 为 `run` 和完整 prompt，不拼接 shell，不接受用户输入覆盖路径或参数。
- LLM 输入必须经过证据裁剪，不能直接塞入全部日志和工具输出。
- LLM Gateway 只返回结构化 action 或最终答案候选，不持有执行能力。
- 不保存模型隐藏思维链，只保留决策摘要、事实、假设和证据引用。

## Analysis Orchestrator / Claude Code MCP

- Claude MCP tool call 必须通过 Server 的 schema、预算、白名单、幂等和审批校验。
- Claude Code permission profile 自动允许 `mcp__logagent__*`，只开放任务专属 LogAgent MCP server；native built-in tools 仍由 `tools` / `allowed_tools` / `disallowed_tools` 控制。LogAgent 的用户审批只控制 Server 侧 approval-gated action，不能替代 Claude CLI 的 tool allowlist。
- Claude Code 只能通过 `claude_code` 配置接入；领域能力只能通过 LogAgent MCP 调用，不能直接执行 LogAgent 工具、SSH、任务外文件系统或状态变更。
- 只读 HTTP MCP 只面向个人本地 Claude Code 读取共享知识；它不能创建、读取、启动或恢复 Session，不能读取 task workspace，不能上传文件，不能运行 Tool Runner，不能审批或远程采集，不能修改 Case、Metadata、Skills 或 System Context。
- 只读 HTTP MCP 可以展示工具目录中的 `logagent.fetch` descriptor，但不能执行 Fetch endpoint；Fetch 执行只允许任务 MCP 和 Server 手动 `tool_run` 路径。
- 只读 HTTP MCP 可以展示工具目录中的 `logagent.huawei_cloud_package_sync` descriptor，但不能执行该非只读工具；首版执行范围仅限受保护 Tools API 手动 `tool_run`。
- 第一阶段 Claude Code dry-run 诊断只检查配置路径，不执行 CLI；`analysis_package.json`、`claude_prompt.md`、`claude_mcp_config.json`、`claude_session.json`、`mcp_calls.jsonl` 和 `agent_response.json` 只是 workspace 内契约产物。`claude_prompt.md` 只保存短启动 prompt，证据包通过任务 MCP resource 读取；完整 Metadata 不进入 prompt/package，默认只暴露 outline。
- `logagent.query_metadata` 只能从当前 task workspace 的 `metadata_context.json` 生成 bounded slice，写入 `metadata_slices/<stable_id>.json` 并审计到 `mcp_calls.jsonl`；它不扩大 Claude native file `Read` 权限，slice 也不能作为最终 evidence ref。
- task workspace 日志搜索、白名单工具和只读代码检索可自动执行。
- SSH/SCP 环境采集默认需要用户批准。
- 当前 `logagent.request_approval` 在批准前只写入 pending approval，不执行采集；真实 SSH/SCP 执行器后续仍必须受白名单约束。
- 用户消息、日志和 Case 内容都视为不可信输入，不能覆盖系统指令或安全策略。
- 重复 MCP waiting request 或预算超限后终止或等待人工输入。
- 达到预算时输出信息不足或低置信度结果，不能自动扩大权限。

## 外部工具

- 外部工具只能从白名单配置中调用。
- 调用外部工具时使用参数数组，不拼接 shell 字符串。
- 限制外部工具执行时间、输出大小和可访问目录。
- 工具执行结果要保留 exit code、stderr 和原始输出路径，方便审计。
- Tools 页面创建的手动工具运行也必须走同一白名单和 workspace 边界。`pprof_analyzer` 只分析已上传到 Server 的本地 profile 文件，不接受 URL source，并把 `PPROF_TMPDIR` 设置到当前 task workspace 内。
- 节点日志包预处理只从包内三类日志目录生成 `extracted/` 和 `tool_inputs/`：`/var/chroot/gemini/log/tsdb`、`/var/chroot/gemini/log/stream`、`/home/Ruby/log`。其他普通文件忽略并统计；archive 中的 `..`、Windows drive、symlink/hardlink 和特殊文件会被拒绝。
- Tool Runner 可消费 `tool_inputs/...`，但路径仍必须是当前 workspace 相对路径，不能由用户或 Claude Code 传入任意文件系统路径。
- 内置 Fetch tool 不调用外部命令，执行边界来自 `fetch.enabled`、32-byte base64 secret key、`fetch.allowed_hosts`、请求/响应大小限制、timeout 和 redirect 限制。每次请求和每个 redirect hop 都必须命中 `http/https` allowlist，跨 host redirect 不转发 Authorization/Cookie。
- Fetch cURL import 和运行时 header override 必须拒绝 `Host`、`Content-Length`、`Transfer-Encoding`、`Connection` 等受控 header，拒绝 form/upload/proxy/cert/resolve/connect-to 等扩大网络或文件边界的参数。
- 内置 Huawei package sync 不接受任意本地路径或远程 URL，只能读取 Server UploadStore 中已完成上传的一个 raw snapshot 文件。OBS `objectKey` 必须是安全相对 key；GaussDB SQL 来自受保护 API 使用者，首版不开放给 Claude MCP 自动执行。
- `tools.zip` 导出只打包当前 enabled 且解析为普通可执行文件的工具二进制、wrapper 和示例配置；不导出 API Key、环境变量值、Server 配置原文、workspace 数据或上传文件。缺失或不可执行工具只在 manifest 标记 skipped。

## 代码仓

- 只允许使用 `LOGAGENT_V2_CODE_REPOS_JSON` / 配置中的本地 repo、version ref 和 search roots。
- 不允许用户传任意 repo URL。
- 代码检索只读执行；当前 V2 只运行 `git rev-parse` 和 `git grep <commit>`，不 checkout、不 pull、不创建 worktree。
- 禁止任务流程中自动修改代码、提交代码或运行构建脚本。
- 最终答案中的代码证据只接受当前任务实际生成的 `code_evidence/<action_id>.json#matches/<index>` ref。

## 测试环境采集

- SSH/SCP 只允许访问配置中的测试环境节点。
- SSH 诊断命令必须使用白名单 argv 数组。
- 不允许拼接用户输入作为远程命令。
- 采集文件路径必须在配置白名单内。
