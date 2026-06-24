# WebUI

WebUI 是 LocalToolHub 的本地管理页面。它应该以工具使用和配置管理为第一屏，而不是以 Agent 分析为第一屏。

WebUI 是目标产品架构中的人工控制面，负责把
[`docs/product-architecture-and-flows.md`](../docs/product-architecture-and-flows.md)
定义的 catalog -> run -> artifact -> history 流程可视化。MCP 页面展示外部
client 如何复用同一套能力边界。

## 目标页面

顶部导航（已实现，默认进入 Tools）。顶层标签页只用英文展示，不再中英双语：

```text
Tools            ← 顶层
  Runs History   ← Tools 的子项（缩进虚框小标签）
Skills | MCP | Metadata | Fetch | Executors | Cases | Settings
```

Analyze/Operations 页面已降级，默认不在导航中；旧视图文件将在服务端 fat 代码删除阶段一并清理。

页面职责：

- Tools：查看工具目录（可搜索、按 Source/可运行筛选、按功能类别分组紧凑列表，适配几十个工具）、参数 schema、可用性，运行工具并查看结果。
  - Runs History（Tools 子项）：统一查看 tool/fetch/executor/preprocess/code evidence 的运行历史和 artifacts。
- Skills：管理 Skills 目录。
- MCP：集中展示 `/api/mcp` streamable-http endpoint、stdio 配置示例、Authorization / protocol header、支持的 JSON-RPC 方法、tools/resources 搜索，以及 `resources/read` preview；长任务示例使用 `runMode:"queued"` + `logagent.runs.get/result` 轮询。
- Metadata：导入、刷新和浏览 openGemini/InfluxDB 元数据。
- Fetch：从 cURL 导入 endpoint，管理凭据和手动运行。
- Executors：管理 SSH/SCP executor、命令模板和远程采集结果。
- Cases：管理人工经验记录和搜索。
- Settings：API Key、本地路径、工具目录、source-built analyzer 状态、安全开关和 Skills/Tools 导出；MCP 接入配置不在 Settings 重复展示。

旧 `Analyze` 页面可以在迁移期保留，但不再是目标主入口。

## 技术栈

- React 18
- Vite
- TypeScript
- Tailwind CSS
- shadcn/ui 风格组件
- 构建输出：`webui/out`

## 交互原则

- 工具运行必须展示参数、输入文件、状态、stdout/stderr、result JSON 和 artifacts。
- 长表格需要固定表头和局部滚动。
- 所有下载 artifact 的请求必须携带 Authorization header。
- 敏感字段只显示脱敏值。
- 不在 UI 中展示模型隐藏思维链或未清洗 provider 原文。
- MCP 配置只展示示例，不直接写用户本地客户端配置；页面预览使用 `/api/mcp`，不再使用旧 `/api/mcp/readonly`。

## 本地运行

```bash
cd webui
npm install
npm run dev
```

生产构建：

```bash
cd webui
npm run build
```

Rust Server 托管 `webui/out`。

## 验证

```bash
npm run lint
npm run typecheck
npm run build
```

WebUI 行为变化必须同步更新本 README、[SPEC.md](./SPEC.md) 和根 [PROGRESS.md](../PROGRESS.md)。
