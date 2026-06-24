# LocalToolHub 产品架构与使用流程

最后更新：2026-06-24

本文档根据当前仓库文档与代码实现，结合业界成熟产品和实践，推导
LocalToolHub 的目标产品架构、核心领域对象、使用流程和后续路线。本文是目标
架构文档，不表示所有目标能力都已完成实现。

## 1. 产品定位

LocalToolHub 是个人本地部署的 Tool/MCP Workbench。

核心目标：

- 让运维、开发、测试工具可发现、可配置、可运行、可审计。
- 通过一个 Rust/Axum 本地 Server 统一执行边界、参数校验、allowlist、超时、
  artifact 和 run history。
- 通过 WebUI 服务人工操作，通过 MCP 服务 Claude Code、Codex、Cursor、
  OpenCode 等外部 AI/IDE client。
- 交付形态收敛为单个本地二进制 + `webui/out` + `bin/tools` + 本地 `data/`
  目录。

明确非目标：

- 不做通用多轮 Agent 后端。
- 不把 Claude Code、LangChain、LangGraph 或任意模型服务作为默认运行依赖。
- 不引入 PostgreSQL、Redis、Elasticsearch 作为 MVP 必需依赖。
- 不提供自由 shell、自由 SSH、自由本地文件读取或自动修改用户代码的默认能力。

LocalToolHub 可以被外部 Agent 通过 MCP 调用，但它自身的默认产品闭环是
工具工作台，而不是聊天式自主 Agent。

## 2. 目标用户与典型任务

| 用户 | 典型任务 | 需要的产品能力 |
|------|----------|----------------|
| 本地开发者 | 上传日志包、运行 analyzer、查看 stdout/stderr/result/artifacts。 | Tools、Runs History、Artifact 下载、日志包预处理。 |
| 运维/测试人员 | 管理 Fetch endpoint、执行受控 SSH/Docker 命令、保存 runbook 结果。 | Fetch、Executors、Cases、审计历史。 |
| 工具维护者 | 接入新的内置工具、源码构建 analyzer 或配置式二进制。 | Tool Catalog、参数 schema、output views、测试 fixture。 |
| AI/IDE 使用者 | 从 Claude Code/Codex/Cursor 调用同一套本地工具和上下文。 | MCP tools/list、tools/call、resources/list、resources/read。 |
| 个人知识维护者 | 把确认过的问题经验固化为 Case 或 Skill。 | Cases、Skills、Metadata、MCP context resources。 |

## 3. 业界实践映射

| 实践 | 官方来源 | 可借鉴点 | LocalToolHub 映射 |
|------|----------|----------|-------------------|
| 开发者门户和 Catalog | Backstage Software Catalog 将软件实体、owner、metadata 通过源码中的 descriptor 管理，并在门户中统一展示：<https://backstage.io/docs/features/software-catalog/>；descriptor 结构参考：<https://backstage.io/docs/features/software-catalog/descriptor-format/> | 平台应先有 typed catalog，而不是散落的按钮和脚本入口。 | `ToolDescriptor`、Skills、Metadata、Cases、MCP resources 都是 catalog 化资源；工具定义应来自代码或配置并同时服务 WebUI/MCP。 |
| Workflow run、日志和 artifact | GitHub Actions 将 workflow 定义为可触发的 jobs/steps，并提供 run status、logs、artifacts：<https://docs.github.com/en/actions/concepts/workflows-and-actions/workflows>、<https://docs.github.com/en/actions/how-tos/monitor-workflows/use-workflow-run-logs>、<https://docs.github.com/en/actions/tutorials/store-and-share-data> | 每次执行都要留下状态、日志、结果和可下载产物，失败也要可诊断。 | Tool、Fetch、Executor、dev_selftest 都应进入 run/artifact 模型，stdout/stderr/result/support files 都是审计材料。 |
| Runbook/job automation 审计 | Rundeck audit trail 记录用户、来源、资源和 action：<https://docs.rundeck.com/docs/administration/security/audit-trail.html> | 远程操作必须模板化、目标显式、结果可审计。 | Executor 只能使用管理好的 SSH/Docker record 和命令模板，不提供自由 shell，执行结果进入 `/api/executor-runs` 和 `/api/runs`。 |
| MCP 工具和资源边界 | MCP tools 让模型调用外部能力，resources 让应用暴露上下文：<https://modelcontextprotocol.io/specification/2025-06-18/server/tools>、<https://modelcontextprotocol.io/specification/2025-06-18/server/resources> | AI client 应通过 schema 发现工具，通过 URI 读取上下文。 | `/api/mcp` 和 `mcp-serve` 暴露 `tools/list`、`tools/call`、`resources/list`、`resources/read`，工具 schema 与 WebUI catalog 同源。 |
| 本地 MCP transport 安全 | MCP Streamable HTTP 要求鉴权，并提醒本地 server 校验 `Origin`、优先绑定 localhost：<https://modelcontextprotocol.io/specification/2025-06-18/basic/transports> | 本地工具 server 仍然是高权限边界。 | LocalToolHub 使用 API key，支持 `mcp.allowed_origins`，MCP 调用不得绕过 Server policy。 |

因此目标产品形态是：本地开发者门户 + 受控 runbook/job runner + MCP tool
server，而不是 chat-first autonomous agent。

## 4. 总体产品架构

```text
Browser WebUI
External MCP Clients
Optional Chrome Extension + Native Agent
        |
        v
Rust/Axum LocalToolHub Server
  - Auth、Origin policy、config、settings
  - Tool Catalog 和 capability descriptors
  - Run orchestration：TaskStore + TaskExecutor
  - Controlled backends：built-in tools、configured tools、Fetch、Executor、dev_selftest
  - Context resources：Metadata、Skills、Cases、Code Evidence、System Context
  - MCP JSON-RPC：tools/list、tools/call、resources/list、resources/read
        |
        v
Local runtime state
  - data/uploads
  - data/workspaces/task_*
  - data/artifacts
  - data/metadata
  - data/cases
  - data/dev_selftest
  - bin/tools
  - webui/out
```

### 4.1 Client Surfaces

- **WebUI**：人工控制面。默认进入 Tools，再进入 Runs History、Skills、MCP、
  Metadata、Fetch、Executors、Cases、Settings。
- **MCP clients**：自动化和 AI/IDE 集成面。读取同一 catalog 和 context
  resources。长任务使用 `runMode:"queued"`，再用 `logagent.runs.get` /
  `logagent.runs.result` 轮询。
- **Chrome Extension + Native Agent**：可选导入桥。只把用户确认过的下载文件导入
  Workbench，不执行诊断，不读取浏览器密钥。

### 4.2 Server Control Plane

Server 是唯一执行边界，负责：

- API key 鉴权和受保护 HTTP API。
- 配置解析、环境变量 secret 引用和本地路径约束。
- Tool descriptor、params schema、上传文件数量和后缀校验。
- Tool、Fetch、Executor、Code Evidence、artifact path allowlist。
- MCP 启停、HTTP Origin policy、stdio `mcp-serve`。
- run 创建、状态迁移、queued/running 任务恢复。

### 4.3 Execution Plane

所有可执行能力都应进入同一执行契约：

```text
request
  -> auth/schema/allowlist/path/budget validation
  -> create run record
  -> execute controlled backend
  -> persist result/stdout/stderr/support artifacts
  -> return bounded summary + artifact refs
```

执行面包含：

- **Built-in tools**：日志包预处理、批量 InfluxQL 分析、metadata 查询、Fetch、
  Huawei package sync、GeminiDB Influx 管理、dev_selftest。
- **Configured tools**：配置式外部二进制或 source-built analyzer，通过 argv 模板
  执行，不走 shell 字符串拼接。
- **Fetch backend**：从 cURL 导入 endpoint，脱敏 credential，按 host allowlist
  手动运行，保存 response artifact。
- **Executor backend**：SSH/Docker executor record + command template，作为受控
  runbook 操作面。
- **dev_selftest backend**：多步本地开发自测流程，以工具组形式存在，跨步骤共享
  `data/dev_selftest/runs/{runId}`。

### 4.4 Context Plane

上下文资源默认不执行动作，只让工具和 MCP client 更可解释、更可复用：

- **Metadata**：openGemini/InfluxDB 等实例快照、拓扑、字段和 tag 查询。
- **Skills/System Context**：可复用 runbook、工具说明、诊断背景。
- **Cases/Memory**：人工确认的问题经验、关键词/FTS 召回。
- **Code Evidence**：目标架构要求提供只读 repo/ref/file/line 证据；当前仓库文档把
  它列为核心能力，但当前 `server/src` 尚未形成完整服务/API，需要后续补齐后才能称为
  完整 code-investigation 闭环。
- **Runs/Tools catalog**：近期运行和工具目录同时作为 MCP resources 暴露。

### 4.5 Persistence And Artifacts

本地 `data/` 是产品契约的一部分：

```text
data/
  uploads/                 # 上传或导入的原始文件
  workspaces/task_xxx/     # 每次 run 的输入快照、result、stdout/stderr、support files
  artifacts/               # 可下载逻辑 artifact，按实现需要存放
  metadata/                # 实例快照
  cases/                   # 经验记录
  dev_selftest/runs/       # 多步自测工作区
```

Artifact 只能通过 Server API 以逻辑路径或 artifact id 暴露，必须鉴权并防路径穿越。
stdout、stderr、结构化 result 和 support files 都是审计材料。

## 5. 核心领域契约

| 契约 | 含义 | 当前证据 |
|------|------|----------|
| `ToolDescriptor` | 可发现能力，包含 schema、source、backend、文件限制、output views。 | `server/src/domain/models.rs`、`server/src/services/tools.rs` |
| `TaskRecord` / run | 工具运行和远程命令运行的持久记录。 | `TaskKind::{ToolRun,RemoteCommandRun}`、`/api/runs`、MCP platform tools |
| Artifact | 受控下载的 result/stdout/stderr/support 文件。 | `/api/runs/:id/result`、`/api/runs/:id/artifacts`、`/api/artifacts/*` |
| `RemoteExecutorRecord` | 托管 SSH 或 Docker 执行目标。 | `kind: ssh|docker`、`/api/executors`、`/api/executor-runs` |
| MCP Resource | URI 化上下文，如 skills、metadata snapshot、cases、runs、catalog。 | `logagent://skills`、`logagent://metadata/instances`、`logagent://runs/recent` |
| Skill / Case / Metadata | WebUI 和 MCP 共用的本地上下文存储。 | `server/src/http/skills.rs`、`cases.rs`、`metadata.rs` |

后续清理 `TaskRecord` 中的历史 optional 字段时，应通过 schema version 和兼容迁移处理，
不能破坏现有 run/artifact 可读性。

## 6. 端到端使用流程

### 6.1 首次本地部署

1. 构建或安装 Rust binary、WebUI static、可选 analyzer 到 runtime 目录。
2. 配置 `server.bind`、`storage.data_dir`、API key env、`mcp.enabled` 和可选
   tools directory。
3. 启动 Server，检查 `/health`。
4. 打开 WebUI，输入 API key，确认 Tools catalog 和 MCP 页面可用。
5. 只在配置 allowlist 后启用高风险能力：Fetch、Executors、Code Evidence、
   dev_selftest。

验收信号：

- 无 LLM/Claude 配置也能启动。
- `/api/tools` 返回 catalog。
- MCP `initialize`、`tools/list` 可用。
- WebUI 默认进入 Tools。

### 6.2 WebUI 手动工具运行

1. 用户打开 **Tools**，搜索、筛选、查看分组。
2. 用户选择工具，查看 params schema、文件数量限制、后缀限制、runnable 状态。
3. 用户上传文件或选择已有 artifact。
4. WebUI 调用 `POST /api/tools/:tool_id/runs`。
5. Server 校验 descriptor、params、upload 状态和 allowlist。
6. Server 创建 queued run，执行 backend，保存 artifacts。
7. WebUI 轮询 run，展示 status、result JSON、stdout/stderr、support artifacts。
8. run 保留在 **Runs History**。

这是产品默认闭环，必须不依赖任何外部 Agent。

### 6.3 日志包分析

1. 用户上传一个或多个 `.tar.gz`、`.tgz`、`.tar` 日志包。
2. 用户运行 `logagent.preprocess_log_package` 或
   `logagent.batch_influxql_analysis`。
3. Server 解包、生成 manifest/tool-input index，再按配置调用 analyzer。
4. Result 汇总节点、物化输入、finding、warning、失败 analyzer。
5. stdout/stderr/result/support files 保留为审计证据。

该流程把旧的 chat-first "analyze this" 入口替换为显式工具选择和可检查 artifact。

### 6.4 Fetch Endpoint 管理

1. 用户在 **Fetch** 粘贴 DevTools cURL。
2. Server 解析 preview 并脱敏 Authorization、Cookie、token、secret、password。
3. 用户确认 endpoint 存储，前提是 host 和 credential policy 合法。
4. 用户手动运行 endpoint。
5. response body、status、headers 摘要和脱敏信息进入 run artifact。

Fetch 默认应关闭或严格 allowlist，因为它会从 Server 所在机器访问网络资源。

### 6.5 Executor Runbook 操作

1. Operator 配置 command templates 并启用 Executors。
2. 用户创建 SSH 或 Docker executor record。
3. 用户选择配置好的 command template，不输入自由 shell。
4. Server 创建 `RemoteCommandRun`。
5. SSH runner 或 Docker runner 执行命令，保存 stdout/stderr/result。
6. WebUI 和 `/api/runs` 展示操作历史。

该流程对应 runbook automation：目标显式、命令模板化、结果可审计。

### 6.6 dev_selftest 开发自测

1. 用户启用 `dev_selftest`，配置 allowlisted git repos、build profiles、Docker
   clusters、test suites。
2. 用户调用 `logagent.dev_selftest.sync_workspace`。
3. 用户用同一 dev_selftest run id 调用 `build`、`deploy`、`run_tests`、`report`。
4. 每一步把 progress、logs、artifacts 写入 `data/dev_selftest/runs/{runId}`。
5. `report` 生成 markdown/JSON 摘要，包含 step status、duration、evidence refs。

这是当前唯一目标内置多步 workflow。它仍然是 profile/tool 驱动，不是通用
workflow engine。

### 6.7 MCP Client 使用

1. 外部 client 通过 stdio `mcp-serve` 或 HTTP `/api/mcp` 连接。
2. client 调用 `initialize`，再调用 `tools/list`、`resources/list`。
3. client 读取 tool catalog、skills、metadata、recent cases、recent runs 等资源。
4. client 调用 `tools/call`，参数必须符合 `inputSchema`。
5. 长任务传 `runMode:"queued"`，再用 `logagent.runs.get` /
   `logagent.runs.result` 轮询。
6. Server 应用与 WebUI 相同的 policy，并写入同一 run history。

MCP 是 LocalToolHub 的集成面，不是绕过执行边界的后门。

### 6.8 浏览器下载导入

1. Chrome Extension 监听下载完成。
2. URL 前缀和文件后缀匹配后提示用户确认。
3. Extension 把本地路径交给 Native Agent。
4. Native Agent 校验 `allowed_dirs`、suffix、size，然后上传 Server。
5. 用户回到 WebUI Tools/Runs 继续操作。

Extension 和 Native Agent 不读取浏览器 Cookie、Authorization header，也不执行下载文件。

### 6.9 知识沉淀

1. 用户把确认过的问题经验保存为 Case。
2. 可复用操作步骤沉淀为 Skill/runbook。
3. 后续 WebUI 用户和 MCP clients 可搜索 Cases、读取 Skills。
4. 工具结果应引用 artifact；Cases/Skills 默认是背景上下文，除非后续 evidence
   policy 明确允许它们作为最终证据。

## 7. WebUI 信息架构

目标导航：

```text
Tools
  Runs History
Skills
MCP
Metadata
Fetch
Executors
Cases
Settings
```

页面心智模型：

- **Tools**：现在可以运行什么。
- **Runs History**：发生过什么，artifact 在哪里。
- **MCP**：外部 client 如何复用同一平台。
- **Metadata / Skills / Cases**：有哪些本地上下文可被工具和 client 使用。
- **Fetch / Executors**：配置了哪些受控外部动作。
- **Settings**：本地路径、API key、工具目录、导出和安全开关。

## 8. 能力完整性矩阵

| 能力 | 目标状态 | 当前状态 | 下一步压力 |
|------|----------|----------|------------|
| Tool Catalog | WebUI 和 MCP 共用一个 registry。 | `services::tools::descriptors` 已作为 catalog 来源，MCP 过滤 runnable/platform tools。 | 继续把特殊 backend 收敛到共享 descriptor/executor 契约。 |
| Run History | Tool、Fetch、Executor、workflow step 有统一或可追溯的 run 记录。 | Tool/remote command 使用 `TaskRecord`；dev_selftest 有独立 step workspace。 | 决定 dev_selftest step 是否成为 `/api/runs` first-class child。 |
| Artifact Store | 成功和失败 run 都能查看 result/stdout/stderr/support artifacts。 | result/artifact API 已存在，artifact index 仍随 backend 分散。 | 统一所有 backend 的 artifact index shape，覆盖 failed/running run。 |
| MCP | 与 WebUI 同 catalog/context/policy。 | `/api/mcp` + `mcp-serve` 已实现 resources/tools 和 queued polling。 | 决定 legacy `/api/mcp/readonly` 保留还是迁移删除。 |
| Fetch | cURL 导入、脱敏、allowlisted 执行。 | endpoint 和 run API 已存在。 | 继续收紧 host/private-network policy 和 credential 扫描。 |
| Executors | SSH/Docker 托管 record、模板化命令、run history。 | SSH/Docker kind 已实现，Docker dev_selftest 消费已实现。 | 参数化模板和 SSH dev_selftest 派发仍 deferred。 |
| Metadata | 导入、浏览、查询快照。 | 当前 Metadata dashboard 和 metadata tools 已实现。 | remote fetch policy 必须继续按安全能力处理。 |
| Code Evidence | 只读 repo/ref search，输出稳定 file/line 证据。 | 文档列为核心能力，当前 `server/src` 尚无完整服务。 | 实现前不能声称完整 code-investigation 闭环。 |
| Skills/Cases | 可搜索 runbook 和人工经验。 | WebUI/API store 已存在。 | 明确 final evidence 与 background context 边界。 |
| Packaging | 单 binary + static WebUI + tools dir + data dir。 | deploy 脚本和文档已存在。 | 在不破坏兼容前提下继续收敛 `logagent-*` 命名。 |

## 9. 产品不变量

- 每条执行路径都创建可审计 run record，或明确记录为某个 workflow/sub-run step。
- 每个可执行动作必须预先声明：tool descriptor、configured tool、fetch endpoint、
  executor template 或 dev_selftest profile。
- 默认功能不依赖 LLM provider、Claude Code、LangChain 或外部数据库。
- MCP client 与 WebUI 用户看到同一能力边界和同一 policy。
- Secret 只能来自环境变量或本地 secret 引用，日志、artifact、导出包和 UI 必须脱敏。
- Artifact 只通过 Server 控制的逻辑路径访问，不暴露任意本机路径。
- 高风险能力默认关闭或必须 allowlist。
- 可选 automation 可以编排已有工具，但不能成为运行产品的唯一方式。

## 10. 架构路线

### P0：显式化目标架构

- 保持本文档在根 README/SPEC 和模块文档中可发现。
- 将 `docs/architecture_review.md` 标记为 pre-pivot 历史快照。
- 后续 README/SPEC 变更必须维护 Tools/MCP Workbench 定位。

### P1：加固 Workbench 核心

- 统一所有 run kind 的 result/stdout/stderr/support artifact index。
- 完成 failed/running run 的 `/api/runs` 可观察性。
- 将特殊工具 backend 尽量收敛到统一 executor interface。
- 收紧 Fetch 和 metadata remote fetch 安全策略。
- 明确 legacy `/api/mcp/readonly` 去留。

### P2：补齐受控环境和代码证据

- 实现参数化 executor command template，带小 JSON Schema 校验。
- 实现 SSH-kind dev_selftest 派发，覆盖受控 SCP 和 `ssh_binary_replace`。
- 实现只读 Code Evidence：repo/ref allowlist、worktree cache、search/snippet、
  stable file/line refs。

### P3：稳定工具层上的可选自动化

- 仅在 tool/run/artifact 契约稳定后增加可选 workflow/report 层。
- 自动化应保持 declarative/profile-based，不重新引入默认自研多轮 Agent 后端。

