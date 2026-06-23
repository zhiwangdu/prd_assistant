# LocalToolHub

LocalToolHub 是个人本地部署的工具工作台和 MCP 工具合集。目标是提高运维、开发和测试效率，而不是自研一个通用 Agent。项目代码和兼容命令仍保留部分 `logagent` 命名，后续可逐步迁移。

## 产品目标

LocalToolHub 应该开箱即用地提供：

- Web 管理页面：配置工具、运行工具、查看结果、管理 Metadata、Fetch、Executor、Case 和 MCP 配置。
- 本地 Tool Hub：统一管理内置工具、源码构建工具和用户配置工具。
- MCP Server：让 Claude Code、Codex、Cursor、OpenCode 等外部客户端调用同一套受控 tools/resources。
- Artifact 和 Run History：每次工具运行都有输入、输出、stdout/stderr、结构化结果和下载入口。
- 单机部署体验：Rust Server 单二进制 + `webui/out` + `bin/tools` + 本地 data 目录。

LocalToolHub 不再把 Claude Code、LangChain/LangGraph 或任何模型编排作为核心后端。外部 Agent 可以通过 MCP 使用 LocalToolHub；Server 自身只负责工具、上下文、执行边界和审计。

## 核心架构

```text
Browser WebUI / External MCP Client / Optional Chrome Extension
  -> Rust local server
    -> Auth and local settings
    -> Tools catalog
    -> Tool runner and built-in tools
    -> Fetch manager
    -> SSH/SCP executor manager
    -> Metadata manager
    -> Log analyzer and package preprocessing
    -> Code evidence search
    -> Case notes and skills/system context
    -> Artifact store and run history
    -> MCP resources/tools
  -> Local data dir + tools dir
```

## 运行形态

```text
$LOCALTOOLHUB_APP_DIR/
  bin/logagent-local
  bin/tools/
  data/
    logagent.sqlite or json stores
    artifacts/
    uploads/
    runs/
    metadata/
    cases/
  webui/out/
  deploy/logagent.yaml
```

当前阶段继续复用 main 分支现有 `logagent-server` crate、`LOGAGENT_*` 环境变量、`logagent.*` tool id 和 `logagent://` MCP resource namespace，作为兼容层保留。产品和用户可见名称收敛到 LocalToolHub。旧 Log Analysis Agent 相关代码只作为迁移来源，不作为目标架构。

## 组件

| 目录 | 职责 | 文档 |
|------|------|------|
| `server/` | Rust 本地 Server、工具运行、MCP、artifact、配置和静态 WebUI 托管 | [README](./server/README.md) / [SPEC](./server/SPEC.md) |
| `webui/` | 本地管理页面，Tools-first 工作台 | [README](./webui/README.md) / [SPEC](./webui/SPEC.md) |
| `native-agent/` | 可选本机文件导入桥，用于 Chrome Extension 传递下载文件 | [README](./native-agent/README.md) / [SPEC](./native-agent/SPEC.md) |
| `chrome-extension/` | 可选 Chrome 下载监听和导入确认 | [README](./chrome-extension/README.md) / [SPEC](./chrome-extension/SPEC.md) |
| `deploy/` | 单机 runtime 部署、重建和控制脚本 | [README](./deploy/README.md) |
| `testing/` | fixture、smoke 和测试策略 | [README](./testing/README.md) / [SPEC](./testing/SPEC.md) |
| `docs/modules/` | Server 内部能力边界和后续开发约束 | [README](./docs/modules/README.md) |
| `third_party/` | 上游诊断工具源码引用；不改写上游 README | - |

## 模块边界

Server 内部能力以本地工具平台为中心：

- Tool Runner / Fetch / Executor 是核心。
- Metadata / Skills / Case 是上下文和管理能力。
- Log Analyzer / Code Evidence 是工具输入和证据能力。
- MCP 是外部 Agent 的集成入口。
- LLM Gateway、Analysis Agent、Claude Code runner 只保留为可选自动化，不作为默认主线。

## API 原则

- 新接口优先使用 `/api/tools*`、`/api/runs*`、`/api/artifacts*`、`/api/mcp*`、`/api/settings*` 等工具工作台语义。
- 旧 `/api/sessions*`、`/api/tasks*` 可在迁移期保留，但不作为新功能设计入口。
- 所有受保护接口使用 `Authorization: Bearer <api-key>`。
- MCP resources/tools 与 WebUI 使用同一个 registry 和同一套执行边界；`mcp.enabled=false` 时 HTTP `/api/mcp` 与 stdio `mcp-serve` 都必须拒绝服务。

## 安全边界

- API Key 只从环境变量或本地 secret 配置读取。
- 不把密钥、Cookie、Authorization header 写入日志、artifact 或导出包。
- Tool Runner、Fetch、SSH/SCP、Code Evidence 都必须有 allowlist。
- Fetch 默认关闭，启用时必须配置 allowed hosts 和 credential secret。
- SSH/SCP 只允许配置内 executor、命令模板和文件模板。
- MCP client 不能绕过 Server 直接执行本机命令或读取任意路径。
- Artifact path 对外使用逻辑路径，不暴露任意本机路径。

## 近期路线

1. 文档和产品定位切换为 Local Tool/MCP Workbench。
2. 裁剪 WebUI 首屏和导航：Tools、Runs、Metadata、Fetch、Executors、MCP、Settings。
3. 收敛 Server API：保留工具运行、artifact、metadata、fetch、executor、MCP，弱化 Agent/Analyze。
4. 统一 Tool Catalog，source-built analyzers 和内置工具都进入同一个 registry。
5. 打包 Rust binary + WebUI static + tools dir 的本地部署路径。
6. 最后再按需要恢复可选诊断 workflow，而不是默认 Agent 后端。

## 验证

```bash
cargo fmt --check
cargo check
cargo test
cd webui && npm run lint && npm run typecheck && npm run build
git diff --check
```

每次修改后必须同步更新对应 README/SPEC 和 [PROGRESS.md](./PROGRESS.md)。历史进展已归档到 `docs/archive/`。
