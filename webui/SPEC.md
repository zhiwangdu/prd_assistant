# WebUI Spec

## 目标

WebUI 提供 LocalToolHub 的可视化管理能力。用户应该可以不依赖任何 Agent，直接完成工具配置、运行、结果查看和 MCP 配置复制。

## 页面要求

顶部导航顺序为 `Tools → Skills → MCP → Metadata → Fetch → Executors → Cases → Settings`，默认进入 Tools。顶层标签页只用英文展示（不再中英双语）；页面内部文案仍随语言切换。Runs 不再是独立顶层标签，而是 Tools 的子项「Runs History」（缩进虚框小标签）。

### Tools

- 从 `GET /api/tools` 读取 catalog。
- 左侧 catalog 为可搜索、可筛选、按类别分组紧凑列表（适配几十个工具）：
  - 搜索框按 displayName / toolId / description / tags 过滤；搜索时切换为扁平「Results (N)」结果列表。
  - Source 分段筛选（All / Built-in / Configured）与「仅可运行」开关。
  - 无搜索时按派生功能类别分组（Analyzers / Metadata / Fetch / Sync / Other），每组带计数，空组隐藏。
  - 紧凑行：状态点（绿=可运行、琥珀=启用但不可运行、灰=禁用）+ 名称 + 来源标签；选中高亮，其余详情在右侧面板。
  - 顶部计数 `shown / total`；无工具 / 无匹配各有空状态。
- 展示 toolId、来源、backend、runnable、输入文件限制、参数模板、不可用原因（右侧详情面板）。
- 支持上传或选择已有 artifact 作为输入。
- 调用 `POST /api/tools/:tool_id/runs` 创建 run。
- 轮询 run 状态并展示 result/stdout/stderr/artifacts。

#### Runs History（Tools 子项）

- 展示所有工具、Fetch、Executor、Log preprocess 和 Code Evidence run。
- 支持按类型、状态、工具和时间筛选。
- 支持 artifact 下载和 result JSON 展开。

### Skills

- 展示可复用 Skills / runbook 资源，作为工具运行的背景能力。
- Skills 从 System Context 集合页拆出为独立导航项；Metadata 已是独立导航项。

### MCP

- 展示 `/api/mcp` endpoint、Authorization header 示例、客户端配置示例。
- 同时展示 streamable-http 与 stdio 两种接入配置；HTTP 示例必须包含 `Authorization` 和 `MCP-Protocol-Version` header。
- 展示当前 server 支持的 MCP JSON-RPC 方法：`initialize`、`ping`、`tools/list`、`tools/call`、`resources/list`、`resources/read`。
- 展示 tools 和 resources 列表，支持搜索；选中 tool 时展示 `inputSchema` 和同步/queued `tools/call` 示例，选中 resource 时调用 `resources/read` 并预览 JSON 文本。
- 长任务示例使用 `runMode:"queued"`，轮询示例使用 platform 工具 `logagent.runs.get` / `logagent.runs.result`，并注明轮询不创建 ToolRun。
- 不写入用户本地 Claude Code/Codex/Cursor 配置。

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

### Cases

- 管理人工经验记录的增删改查与搜索。

### Settings

- 展示 API health、工具目录、source-built analyzer 状态、本地数据目录、安全开关。
- Skills/Tools ZIP 导出可保留在 Settings；MCP client 接入配置统一放在 MCP 页面，避免重复和过期。
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
