# Security Spec

## 目标

限制 LogAgent 的文件访问、命令执行、远程采集和密钥暴露风险。

## 当前状态

已实现：

- Server API Key middleware。
- Native Agent 本地路径白名单。
- Native Agent 文件后缀和大小限制。
- 压缩包 safe join 防路径逃逸。
- Fetch endpoint 默认关闭；启用时要求 32-byte base64 secret key、出网 allowlist、AES-256-GCM credential store 和全链路脱敏展示。

## 安全边界

- Chrome Extension 不直接上传远端。
- Native Agent 只读取配置允许目录。
- Server 只在 workspace 内处理任务产物。
- Tool Runner 只能执行白名单工具。
- Tools 页面手动工具运行只能引用 Server UploadStore 中已完成上传，不能传入任意本地路径、远程 URL 或自由 argv；`pprof_analyzer` 的 `PPROF_TMPDIR` 必须位于 task workspace 内。
- Fetch endpoint 不接受任意 URL 出网，只允许访问 `fetch.allowed_hosts` 中的 `http/https` 目标；redirect 每跳重新校验 allowlist，跨 host redirect 不转发 Authorization/Cookie。
- Fetch import 和运行时 override 不能设置 `Host`、`Content-Length`、`Transfer-Encoding`、`Connection` 等受控 header，不能使用上传文件、form、proxy、cert/key、resolve 或 connect-to 等会扩大网络或文件边界的 cURL 参数。
- LLM binary provider 只能执行配置中的绝对路径模型二进制，固定 argv 为 `run` 和完整 prompt，不拼接 shell；该执行路径属于模型 Provider 适配，不开放为 Analysis Agent action。
- Claude Code 只能通过 `claude_code` 配置声明；第一阶段 Settings 诊断只检查路径，不执行 CLI。
- `analysis_package.json`、`claude_prompt.md`、`claude_mcp_config.json`、`claude_session.json`、`mcp_calls.jsonl` 和 `agent_response.json` 是 workspace 内契约产物，不携带密钥，也不授权 Claude Code 绕过 Server 执行领域命令、SSH 或状态写入；`claude_prompt.md` 只包含短启动指令，证据通过任务 MCP resource 读取，完整 Metadata 不进入 prompt/package。
- `logagent.query_metadata` 只能读取当前 task workspace 的 `metadata_context.json`，按 section/filter/limit/cursor 写入 bounded `metadata_slices/<stable_id>.json` 背景上下文，不扩大 Claude native file `Read` 权限，也不新增最终 evidence ref 类型。
- Claude MCP tool call 必须经过 Server schema、白名单、预算、幂等和审批校验。
- Claude CLI `allowedTools` 必须包含任务 MCP 命名空间 `mcp__logagent__*`；Server 自动注入该 allowlist。用户审批 API 只恢复 LogAgent Server 侧等待状态，不能扩大 Claude CLI native tool 权限。
- 只读 HTTP MCP 只能读取共享知识资源和只读 tools；禁止创建、读取、启动或恢复 Session，禁止读取 task workspace，禁止上传文件，禁止运行 Tool Runner，禁止审批、SSH/SCP 或修改 Case/Metadata/Skills/System Context。
- 只读 HTTP MCP 可以通过工具目录展示 `logagent.fetch` descriptor，但 `tools/call logagent.fetch` 必须拒绝；Fetch 执行范围仅限任务 MCP 和受保护 Server `tool_run` API。
- `skills.zip` 不跟随 symlink，不允许路径逃逸；`tools.zip` 不包含 API Key、环境变量值、Server 配置原文、workspace 数据或上传文件，无法打包的 enabled 工具只能标记 skipped。
- Environment Collector 只能访问配置节点和路径。
- LLM 不能直接执行命令。
- Analysis Orchestrator 和 Claude Code 只能通过 structured outcome / MCP tools 表达意图，Server 是唯一领域执行者。
- 远程采集默认需要显式批准。
- 远程采集必须通过 approval gate；未批准前不执行。真实 SSH/SCP 接入时仍需配置节点、路径和命令白名单。
- 不持久化隐藏思维链。

## 密钥

- 密钥来自环境变量。
- 不写入日志、manifest、grep 结果或前端任务记录。
- Fetch credential encryption key 来自 `fetch.secret_key_env` 指向的环境变量；值必须是 32-byte base64 key。Authorization、Cookie 和 token/api_key/secret/password/session 类 query/body 字段只能以密文保存，响应 artifact、API 和 UI 只显示 `<redacted>` 或脱敏 URL/header/body preview。

## 验收标准

- 无 API Key 访问受保护接口返回 401。
- 非白名单文件路径导入失败。
- 压缩包路径逃逸失败。
- 未知 action、越权参数和重复 action 被拒绝。
- 未批准的远程采集不执行。
- Prompt injection 不能改变工具、路径、仓库或环境白名单。
- Prompt injection 不能改变 LLM binary provider 的可执行路径、subcommand 或 argv 结构。
- Prompt injection 不能改变 Claude Code 命令路径、analysis mode、permission profile 或 MCP tool 白名单。
- Prompt injection 不能把只读 HTTP MCP 升级为写入入口或工具执行入口。
- Prompt injection 不能让 Fetch 访问 allowlist 外地址、绕过 redirect 校验、展示密文原值、设置受控 header，或把只读 HTTP MCP 升级为 Fetch 执行入口。
- Prompt injection 不能要求 Server 把完整 Metadata 放入默认 prompt/package，或把 `metadata_slices/*` 升级为最终 evidence ref。
- Fetch response evidence ref 只接受当前任务中真实存在且 `tool=logagent.fetch` 的 `tool_results/<action_id>/result.json#response`。
- 导出下载不能泄露密钥、环境变量值、上传文件或 task workspace。
- README 和 SPEC 在安全策略变更时同步更新。
