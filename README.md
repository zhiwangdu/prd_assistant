# LogAgent MVP 总览

当前权威文档入口是本总览、[SPEC.md](./SPEC.md)、各可运行组件目录，以及 [docs/modules](./docs/modules/README.md) 中的 Server 内部能力文档。

## 目标

LogAgent 是面向开发和运维诊断的证据工作台，也是 Claude Code 的领域诊断增强层。主入口是团队共享 Server 的 WebUI `Analyze`，用户通过浏览器完成日志、流水线和问题分析；高级入口是只读 HTTP MCP，个人本地 Claude Code 可连接共享 Server 读取 Skills、Metadata、Case、工具目录和领域能力摘要，用于纯本地分析。

LogAgent 不接管个人本地 Claude Code 环境。Server 只提供受保护的只读 MCP endpoint、Skills 全量 zip、Tools 二进制快照 zip 和配置示例；本地 Claude Code 的安装、注册、API Key header 注入和工具包引用由个人环境处理。

当前重点场景是快速问题分析、日志分析、日常测试流水线失败分析和数据库/存储系统专项诊断。第一批领域继续覆盖 openGemini/InfluxDB，并新增 Cassandra、RocksDB 的 Domain Adapter 骨架。

## 技术选型原则

能用 Rust 实现的模块优先使用 Rust。整体语言优先级：

```text
Rust -> C/C++ -> Go/Python/Java 等
```

默认建议：

- 本地 Agent、服务端 API、日志分析器、工具调度器、代码证据、环境采集优先使用 Rust。
- 已有 C/C++ 工具可直接复用，通过 Tool Runner 统一调用。
- Python/Go/Java 主要作为已有生态或历史工具的兼容选项，不作为新模块首选。

核心链路：

```text
日志来源
  - 浏览器下载 / 手动上传
  - 测试环境 SSH/SCP 采集
    |
    v
基础证据提取
  - rg 日志检索
  - Skill-backed System Context 背景资源
  - 实例和集群元数据
  - 外部工具调用
  - 对应版本代码检索
  - 环境状态采集
    |
    v
Analysis Orchestrator
  - 汇总任务证据、领域上下文和预算
  - 生成 Claude MCP 配置
  - 启动或恢复 Claude Code session
    |
    v
Claude Code session
  - 通过 LogAgent MCP resources/tools 获取证据
  - 按权限模式使用允许的 native tools
  - 返回结构化 session outcome
    |
    v
人工确认
    |
    v
Case 沉淀与召回
```

## 规划架构图

```mermaid
flowchart LR
    subgraph Inputs["用户与数据来源"]
        User["用户 / WebUI"]
        Chrome["Chrome Extension"]
        TestEnv["测试环境"]
    end

    subgraph Local["用户本机"]
        Native["Native Agent"]
    end

    subgraph ServerBoundary["LogAgent Server（单 Rust 进程）"]
        API["API / Auth / Task Manager"]
        ReadonlyMcp["Read-only HTTP MCP<br/>Knowledge resources / tools"]
        Orchestrator["Pipeline / Action Executor"]
        Agent["Analysis Orchestrator<br/>证据包、MCP 配置、等待态"]
        Gateway["Claude Code Session Runner<br/>CLI + MCP config"]
        Mcp["LogAgent MCP Server<br/>Resources / Tools"]

        subgraph Evidence["受控证据能力"]
            Domains["Domain Adapters<br/>openGemini/InfluxDB、Cassandra、RocksDB"]
            SysCtx["System Context<br/>Diagnostic Skills、Metadata adapter"]
            Log["Log Analyzer"]
            Tool["Tool Runner"]
            Code["Code Evidence"]
            Env["Environment Collector"]
            Meta["Metadata"]
            Cases["Memory<br/>Case Store compatibility"]
        end

        Store[("Session Store / Task Store / Workspace<br/>session、runs、events、evidence、result")]
    end

    Model["Claude Code CLI"]
    Repos["已配置代码仓"]
    Tools["白名单诊断工具"]

    Chrome --> Native
    Native -->|"上传日志 / 附加到当前 Session"| API
    User -->|"创建 Session、可选上传、启动 run、回答、审批"| API
    User -.->|"个人 Claude Code 只读知识入口"| ReadonlyMcp
    API --> Orchestrator
    Orchestrator --> Agent
    Agent --> Gateway
    Gateway -->|"--mcp-config"| Model
    Model --> Gateway
    Model --> Mcp
    Mcp --> Domains
    ReadonlyMcp --> SysCtx
    ReadonlyMcp --> Meta
    ReadonlyMcp --> Cases
    ReadonlyMcp --> Tool

    Orchestrator --> Domains
    Orchestrator --> Log
    Orchestrator --> SysCtx
    Orchestrator --> Tool
    Orchestrator --> Code
    Orchestrator -->|"批准后"| Env
    Orchestrator --> Meta
    Orchestrator --> Cases

    Tool --> Tools
    Code --> Repos
    Env --> TestEnv

    API <--> Store
    Orchestrator <--> Store
    Agent <--> Store
    Evidence --> Store

    Agent -->|"pending prompt / approval"| API
    API -->|"时间线、问题、审批、最终结果"| User
    Agent -->|"structured outcome"| Store
    Store -->|"人工确认后沉淀"| Cases
```

关键控制边界：

- Analysis Orchestrator、LLM Gateway 和 Claude Code 都不能绕过 LogAgent MCP/Server 边界直接执行领域工具、读取任意任务外路径或连接 SSH。
- Server Action Executor 是唯一执行入口，负责 schema、白名单、预算、幂等和审批检查。
- 日志搜索、白名单工具和只读代码检索可自动执行；环境 SSH/SCP 采集默认等待用户批准。
- `LOGAGENT_CLAUDE_CODE_PATH` 是默认 Claude Code CLI 路径来源。Log Analysis run 会写出 `analysis_package.json`、`claude_prompt.md`、`claude_mcp_config.json`、`claude_session.json`、`mcp_calls.jsonl` 和 Claude session 语义的 `agent_response.json`。Claude CLI 只接收短 stdin 启动 prompt，证据包通过任务专属 MCP `analysis_package` resource 读取，避免大 prompt 进入 argv 或 stdin；完整 `metadata_context.json` 不进入该 package，Claude 初始只看到 `metadataContextOutline`，需要细节时通过 `logagent.query_metadata` 按 section/filter/分页读取。未配置或调用失败时任务失败，不自动 fallback。
- Log Analysis 公开入口是可恢复的 Session；每次分析 run 仍创建一个 Server task workspace 快照。
- WebUI 主入口显示为 `Analyze`，仍使用 Session-first 分析能力，并继续默认调用 Server 机器上的 Claude Code、任务专属 stdio MCP 和 Server 本地 workspace。
- 个人高级入口是 `POST /api/mcp/readonly`，只读返回 Skills、Metadata、Case、Tools catalog 和 Domain Adapter 等共享知识；不读取/启动/恢复 Session，不上传文件，不审批，不运行远程工具，不写入 Server 数据。
- Settings 提供只读 MCP URL、Authorization header 提示、Claude Code HTTP MCP 配置示例，以及 `skills.zip` / `tools.zip` 下载入口。
- Session 可以只包含用户问题而不包含上传日志；这种 run 会生成 `session_text_input.json`、空 raw/input 快照、空 manifest 文件列表和空 grep evidence，再由 Analysis Orchestrator 基于问题、Metadata、Case 和后续交互继续分析。
- `WAITING_FOR_USER` 支持用户提交补充信息，也支持声明没有更多信息并请求基于当前证据直接生成最终结果；该意图会写入 `analysis_state.json` 并通过 `analysis_package.json` 约束下一轮 Claude Code 不再继续追问。
- Log Analysis run 会固化 `system_context.json`，把已选择或自动匹配的 Diagnostic Skills 和 Metadata adapter 摘要作为背景参考带入 Prompt；System Context 和 Skill reference 不能替代当前任务证据。
- 成功的 Log Analysis run 会在最终结果生成后静默调用 LLM Gateway 生成短 alias，用于 WebUI 展示；该命名调用不写入 Session timeline 或 analysis events。
- 所有 Session、任务上下文、事件、证据和结果都持久化到 Session Store / Task Store / Workspace，支持重启恢复。
- WebUI 可实时展示 Task execution、Claude Code session、MCP calls 和 evidence artifact；LLM response content 日志只能通过顶部 debug 开关手动开启。
- Memory 当前只激活 `memoryType=case`，通过兼容的 Case API 接收人工确认后的 Case，包括成功任务最终结果确认和用户通过 LLM-assisted 文本导入确认的手工 Case。

## 项目目录

根目录只保留当前真实可运行的组件和工程支撑目录。日志分析、Metadata、Tool Runner、Analysis Orchestrator、Claude Code Session Runner、LogAgent MCP、Domain Adapters、LLM Gateway、Memory/Case Store 等能力目前都作为 `server` crate 的内部模块实现；后续确实需要独立发布或部署时，再从 Server 内部迁出。

| 目录 | 职责 | Spec |
|------|------|------|
| [chrome-extension](./chrome-extension/README.md) | Chrome 插件，识别下载并触发上传 | [SPEC](./chrome-extension/SPEC.md) |
| [native-agent](./native-agent/README.md) | 本地 Rust Agent，接收插件请求并上传日志 | [SPEC](./native-agent/SPEC.md) |
| [server](./server/README.md) | Rust 服务端，任务、上传、证据流水线、只读 HTTP MCP、导出包和 API | [SPEC](./server/SPEC.md) |
| [webui](./webui/README.md) | Vite WebUI、Analyze、任务证据、Memory、Skill-backed System Context、Metadata、Tools 和 Settings 可视化 | [SPEC](./webui/SPEC.md) |
| [deploy](./deploy/README.md) | Runtime 部署模板、环境变量示例、服务控制和重建安装脚本 | [Deployment SPEC](./docs/modules/deployment/SPEC.md) |
| [examples](./examples) | 本地配置样例和工具 smoke 配置 | - |
| [scripts](./scripts) | 工作目录初始化、Server/WebUI 快捷编译、服务启停和 smoke 脚本 | - |
| [testing](./testing/README.md) | 测试 fixture、集成测试和 mock Claude CLI | [SPEC](./testing/SPEC.md) |
| [third_party](./third_party) | 源码引用的诊断工具 submodules：InfluxQL、Flux、openGemini storage 和 InfluxDB 1.x storage analyzers | - |

Server 内部能力的设计文档已归档到 [docs/modules](./docs/modules/README.md)：

| 能力 | 文档 |
|------|------|
| Claude Code Session Runner | [README](./docs/modules/agent-backends/README.md) / [SPEC](./docs/modules/agent-backends/SPEC.md) |
| Log Analyzer | [README](./docs/modules/log-analyzer/README.md) / [SPEC](./docs/modules/log-analyzer/SPEC.md) |
| Tool Runner | [README](./docs/modules/tool-runner/README.md) / [SPEC](./docs/modules/tool-runner/SPEC.md) |
| Domain Adapters | [README](./docs/modules/domain-adapters/README.md) / [SPEC](./docs/modules/domain-adapters/SPEC.md) |
| Metadata | [README](./docs/modules/metadata/README.md) / [SPEC](./docs/modules/metadata/SPEC.md) |
| Skills | [README](./docs/modules/skills/README.md) / [SPEC](./docs/modules/skills/SPEC.md) |
| System Context | [README](./docs/modules/system-context/README.md) / [SPEC](./docs/modules/system-context/SPEC.md) |
| Analysis Agent | [README](./docs/modules/analysis-agent/README.md) / [SPEC](./docs/modules/analysis-agent/SPEC.md) |
| LLM Gateway | [README](./docs/modules/llm-gateway/README.md) / [SPEC](./docs/modules/llm-gateway/SPEC.md) |
| Memory / Case Store compatibility | [README](./docs/modules/case-store/README.md) / [SPEC](./docs/modules/case-store/SPEC.md) |
| Memory | [README](./docs/modules/memory/README.md) / [SPEC](./docs/modules/memory/SPEC.md) |
| Code Evidence | [README](./docs/modules/code-evidence/README.md) / [SPEC](./docs/modules/code-evidence/SPEC.md) |
| Environment Collector | [README](./docs/modules/environment-collector/README.md) / [SPEC](./docs/modules/environment-collector/SPEC.md) |
| Config / Interfaces / Security / Deployment / Roadmap | [docs/modules](./docs/modules/README.md) |

## MVP 边界

第一版不做企业级日志平台，不引入 Elasticsearch/OpenSearch、CMDB、监控接入、通用远程运维、复杂权限体系和 Multi-Agent 编排，也不尝试替代 Codex、Claude Code 或 OpenCode。

关键边界：

- 外部工具只允许白名单配置调用。
- LLM Gateway 不能直接执行任意命令。
- Claude Code 只能按 `analysisMode` permission profile 使用 native tools；领域证据和工具执行必须经过 LogAgent MCP/Server。
- Server 会在每个 Claude Code permission profile 中自动允许任务专属 LogAgent MCP 工具命名空间 `mcp__logagent__*`；`diagnose` 仍通过 `--tools ""` 禁用 native tools。用户审批只控制 LogAgent 内部 approval-gated action，不能替代 Claude CLI 的 `allowedTools` 白名单。
- 安全只读动作可自动执行，SSH/SCP 远程采集默认需要用户批准。
- 代码仓只读检索，不自动改代码。
- SSH/SCP 只访问配置中的测试环境节点。
- pgvector 不是第一版硬依赖，Case embedding 可以先用本地文件或 SQLite。
- MVP 部署形态采用单一 Rust Server binary + Server 内部分层 module；后续确有独立生命周期时再拆 crate 或服务。
- Agent 上下文只在当前任务内持久化；跨任务知识只来自人工确认后的 Case。
- 统一配置使用 `logagent.yaml`，密钥只引用环境变量。

## 当前优先级

当前阶段优先把 LogAgent 重构为“诊断证据工作台 + Claude Code MCP 增强层 + Domain Adapter”：保留 Session-first Log Analysis、Skill-backed System Context、上传、Metadata、Tool Runner、Tools 页面和 Case Store，`PLAN_ANALYSIS` 生成证据包和 MCP 配置后启动或恢复 Claude Code session。Claude Code 通过 LogAgent MCP tools 请求日志搜索、日志切片、领域工具、按需分页 Metadata slice、Skill reference、Case recall、用户追问和审批；Server 继续负责白名单、审批、证据持久化和最终 evidence ref 校验。InfluxQL、Flux、openGemini storage 和 InfluxDB 1.x storage analyzers 已通过 `third_party/` submodules 引用，`scripts/build-tools.sh` 构建并安装到 `target/tools`、`$LOGAGENT_WORK_DIR/bin/tools` 或 runtime `bin/tools`；部署样例默认启用这些源码构建产物。Tools 页面已接入 `pprof_analyzer` 示例工具和 Remote Executor 执行机纳管；Remote Executor 通过白名单 SSH 模板创建 `remote_command_run`，首个 smoke 模板执行低风险 `ls -la /root`。

Code Investigation 和 Fix 模式的真实代码 worktree、以及完整 SSH/SCP Environment Collector 延后到产品闭环稳定后实现；当前 WebUI 显式执行机命令已有通用 Remote Executor 框架，Analysis Agent 审批后的远程采集仍通过 LogAgent approval gate 进入等待态并使用 mock evidence。

## 开发约定

后续每开发或修改一个可运行组件，都必须同步更新该组件目录下的 `README.md` 和 `SPEC.md`；修改 Server 内部能力时，同步更新 `server/README.md`、`server/SPEC.md`，必要时更新 `docs/modules/` 下对应能力文档。

每次修改完文件，也必须同步更新根目录 [PROGRESS.md](./PROGRESS.md)，记录项目进展、行为变化、验证结果或下一步变化。

`README.md` 至少包含：

- 当前实现状态
- 配置项
- 本地运行方式
- 部署方式
- 健康检查或验证方式
- 与上下游组件的接口约定

`SPEC.md` 至少包含：

- 目标和职责边界
- 输入输出
- API 或数据产物
- 配置和安全约束
- 验收标准

已经写好的可运行组件：

- `chrome-extension`
- `native-agent`
- `server`
- `webui`

这些组件的 README 需要随着代码变化持续维护。
