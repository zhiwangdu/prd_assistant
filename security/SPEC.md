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
- Environment Collector 只能访问配置节点和路径。
- LLM 不能直接执行命令。

## 密钥

- 密钥来自环境变量。
- 不写入日志、manifest、grep 结果或前端任务记录。

## 验收标准

- 无 API Key 访问受保护接口返回 401。
- 非白名单文件路径导入失败。
- 压缩包路径逃逸失败。
- README 和 SPEC 在安全策略变更时同步更新。
