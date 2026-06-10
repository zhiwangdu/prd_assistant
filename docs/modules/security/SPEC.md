# Security Spec

## 目标

限制 LogAgent 的文件访问、命令执行、远程采集和密钥暴露风险。

## 当前状态

已实现：

- Server API Key middleware。
- Native Agent 本地路径白名单。
- Native Agent 文件后缀和大小限制。
- 压缩包 safe join 防路径逃逸。

## 安全边界

- Chrome Extension 不直接上传远端。
- Native Agent 只读取配置允许目录。
- Server 只在 workspace 内处理任务产物。
- Tool Runner 只能执行白名单工具。
- Tools 页面手动工具运行只能引用 Server UploadStore 中已完成上传，不能传入任意本地路径、远程 URL 或自由 argv；`pprof_analyzer` 的 `PPROF_TMPDIR` 必须位于 task workspace 内。
- LLM binary provider 只能执行配置中的绝对路径模型二进制，固定 argv 为 `run` 和完整 prompt，不拼接 shell；该执行路径属于模型 Provider 适配，不开放为 Analysis Agent action。
- Environment Collector 只能访问配置节点和路径。
- LLM 不能直接执行命令。
- Analysis Agent 只能产生结构化意图，Server 是唯一执行者。
- 远程采集默认需要显式批准。
- `collect_environment` 必须使用 `REQUIRES_APPROVAL` risk；未批准前不执行。当前 MVP 批准后仅生成 mock evidence，真实 SSH/SCP 接入时仍需配置节点、路径和命令白名单。
- 不持久化隐藏思维链。

## 密钥

- 密钥来自环境变量。
- 不写入日志、manifest、grep 结果或前端任务记录。

## 验收标准

- 无 API Key 访问受保护接口返回 401。
- 非白名单文件路径导入失败。
- 压缩包路径逃逸失败。
- 未知 action、越权参数和重复 action 被拒绝。
- 未批准的远程采集不执行。
- Prompt injection 不能改变工具、路径、仓库或环境白名单。
- Prompt injection 不能改变 LLM binary provider 的可执行路径、subcommand 或 argv 结构。
- README 和 SPEC 在安全策略变更时同步更新。
