# 安全与可靠性方案

## API 和上传

- 服务端 API 使用简单 API Key。
- API Key 从 `logagent.yaml` 引用的环境变量读取。
- 启动时检查 API Key 是否存在。
- API Key 不写入任务日志，不进入 LLM Prompt。
- Native Agent 只接受 `127.0.0.1` 请求。
- Native Agent 校验文件路径，避免任意文件上传。
- 不在日志或数据库中保存 Cookie、Authorization、session token。
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
- Claude Code 只能通过 `claude_code` 配置接入；领域能力只能通过 LogAgent MCP 调用，不能直接执行 LogAgent 工具、SSH、任务外文件系统或状态变更。
- 只读 HTTP MCP 只面向个人本地 Claude Code 读取共享知识；它不能创建、读取、启动或恢复 Session，不能读取 task workspace，不能上传文件，不能运行 Tool Runner，不能审批或远程采集，不能修改 Case、Metadata、Skills 或 System Context。
- 第一阶段 Claude Code dry-run 诊断只检查配置路径，不执行 CLI；`analysis_package.json`、`claude_mcp_config.json`、`claude_session.json`、`mcp_calls.jsonl` 和 `agent_response.json` 只是 workspace 内契约产物。
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
- `tools.zip` 导出只打包当前 enabled 且解析为普通可执行文件的工具二进制、wrapper 和示例配置；不导出 API Key、环境变量值、Server 配置原文、workspace 数据或上传文件。缺失或不可执行工具只在 manifest 标记 skipped。

## 代码仓

- 只允许使用配置中的本地 repo 和 ref。
- 不允许用户传任意 repo URL。
- 代码检索只读执行。
- 禁止任务流程中自动修改代码、提交代码或运行构建脚本。

## 测试环境采集

- SSH/SCP 只允许访问配置中的测试环境节点。
- SSH 诊断命令必须使用白名单 argv 数组。
- 不允许拼接用户输入作为远程命令。
- 采集文件路径必须在配置白名单内。
