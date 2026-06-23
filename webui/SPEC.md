# WebUI Spec

## 目标

WebUI 提供 LocalToolHub 的可视化管理能力。用户应该可以不依赖任何 Agent，直接完成工具配置、运行、结果查看和 MCP 配置复制。

## 页面要求

### Tools

- 从 `GET /api/tools` 读取 catalog。
- 展示 toolId、来源、backend、runnable、输入文件限制、参数模板、不可用原因。
- 支持上传或选择已有 artifact 作为输入。
- 调用 `POST /api/tools/:tool_id/runs` 创建 run。
- 轮询 run 状态并展示 result/stdout/stderr/artifacts。

### Runs

- 展示所有工具、Fetch、Executor、Log preprocess 和 Code Evidence run。
- 支持按类型、状态、工具和时间筛选。
- 支持 artifact 下载和 result JSON 展开。

### Metadata

- 支持 URL、文件、文本导入。
- 支持实例列表、快照查看、Raw JSON、拓扑、Schema 和诊断。
- 重复导入同一实例时必须展示覆盖后的最新快照。

### Fetch

- 支持 DevTools bash cURL 预览和导入。
- Authorization、Cookie、token、secret、password 等字段必须脱敏。
- 支持 endpoint 启停、删除、手动运行和 response artifact 查看。

### Executors

- 支持 executor CRUD。
- 只能选择 Server 返回的命令或文件模板。
- 不提供自由 shell 输入。
- 展示 stdout/stderr/result artifact。

### MCP

- 展示 `/api/mcp` endpoint、Authorization header 示例、客户端配置示例。
- 展示 resources 和 tools 列表。
- 不写入用户本地 Claude Code/Codex/Cursor 配置。

### Skills

- 展示可复用 Skills / runbook 资源，作为工具运行的背景能力。
- Skills 从 System Context 集合页拆出为独立导航项；Metadata 已是独立导航项。

### Settings

- 展示 API health、工具目录、source-built analyzer 状态、本地数据目录、安全开关。
- LLM/Agent 设置只作为可选 automation，不是默认必填项。

## 构建和部署

- Vite 输出目录固定为 `webui/out`。
- Rust Server 托管该目录。
- 开发态可代理 `/api` 到本地 Server。

## 验收

- `npm run lint`、`npm run typecheck`、`npm run build` 通过。
- Tools 页面可以完成一次内置工具运行并展示 artifacts。
- MCP 页面通过 `/api/mcp` 展示的 tools/resources 与 Server 返回一致。
- Fetch 和 Executor 页面不泄露敏感信息。
- 页面刷新后 run history 仍来自 Server，不依赖 localStorage 作为真源。
