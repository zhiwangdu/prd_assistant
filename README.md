# LocalToolHub

LocalToolHub 是个人本地部署的**两模块工具工作台**：dev_selftest（Linux 跨机自测）+ 日志分析（上传日志即分析），通过 MCP 暴露给外部客户端（Claude Code / Codex / Cursor / OpenCode）。它**不是**通用 Agent，不把 LLM 作为默认后端。项目代码和兼容命令仍保留部分 `logagent` 命名。

## 为什么是这两个模块

这两个模块对应 server 唯二**不可被纯本地执行替代**的场景：

- **dev_selftest** —— 刚需 Linux 环境（docker / go 构建工具链 / DB 集群），而 IDE 与部分内部 MCP 只能在 Windows 端跑。这条是「Windows Claude Code 本地 skill 编排 → Linux server 受控执行 + run history」；skill 负责 workflow，Server 负责 MCP tools/resources 与执行边界。
- **日志分析** —— 一组编译好的 Linux analyzer 二进制（influxql / flux / openGemini / influxdb / pprof）+ 预处理。MCP 连上后上传日志即用，产出结构化 findings + run history。日志包大、analyzer 是 Linux 二进制、要历史回看 —— 也 genuinely 需要一个 server。

其余「通用本地工具」面（fetch / executor / metadata / cases / skills 等）已收敛移除 —— 它们要么不被两模块依赖，要么在纯本地场景被本地 skill 秒杀。

## 产品目标

LocalToolHub 开箱即用地提供：

- **dev_selftest**：提供 `sync_workspace`、`build`、`deploy`、`run_tests`、`report` MCP step tools，以及显式可选的 `cleanup` 环境清理 step 和只读 `diagnose` 诊断 step。Windows 端 Claude Code 完成 commit/push 后，由本地 skill 经 MCP 编排这些 step；Linux ToolHub 只从 allowlisted git repo/ref clone 或 pull，并维护持久工作区 + progress + report + run history。`run_tests` 可接收受限 `testParams` string map，把非敏感运行参数注入 Docker 测试容器的 `DEVSELFTEST_PARAM_*` 环境变量；云实例创建等生命周期仍由外部/internal skill 负责，ToolHub 只执行 Docker 化测试框架。MCP 通过 `logagent://dev_selftest/config` 暴露当前 repo/ref/profile 摘要（含 Docker cluster profile 明细）；用户明确同意后可用 `logagent.dev_selftest.allowlist.update` 追加 repo/ref，或用 `logagent.dev_selftest.profiles.upsert` / WebUI Settings 新增和更新 Docker-backed build/test profile，并写回配置文件。`cleanup` 只对本次 run 的配置化 compose project 执行 `docker compose down`，保留源码、日志、artifact 和报告证据；`diagnose` 只读取 bounded evidence 和执行 allowlisted Docker 只读探测，不做恢复动作。
- **日志分析**：上传日志包 → 预处理（解包/manifest/grep/tool-input 索引）→ 跑配置好的 analyzer → 结构化 findings + artifact。
- **MCP Server**：同一套 tools/resources 经 `POST /api/mcp`（streamable-http）或 `logagent-server mcp-serve`（stdio）暴露给外部客户端；dev_selftest config resource 用于客户端发现 allowlisted repo/ref 和 profile ids。
- **Run History + Artifact Store**：每次工具运行都落 input/stdout/stderr/result/artifacts，统一 `QUEUED→RUNNING→SUCCEEDED/FAILED` 状态，逻辑路径下载。
- **单机部署**：Rust 单二进制 + `webui/out` + `bin/tools` + 本地 data 目录。

## 核心架构

```text
Browser WebUI / External MCP Client
  -> Rust local server (Axum)
    -> Auth (Bearer)
    -> Tool catalog + Tool runner（日志分析 analyzers + 内置工具）
    -> dev_selftest MCP step tools（sync_workspace/build/deploy/run_tests/report/cleanup/diagnose + docker runner）
    -> Uploads + Run history + Artifact store
    -> MCP resources/tools
  -> Local data dir + tools dir
```

## 运行形态

```text
$LOCALTOOLHUB_APP_DIR/
  bin/logagent-server
  bin/tools/
  data/
    uploads/
    workspaces/
    runs/            # task records
    dev_selftest/    # dev_selftest run workspaces
  webui/out/
  deploy/logagent.yaml
```

当前阶段继续复用 `logagent-server` crate、`LOGAGENT_*` 环境变量、`logagent.*` tool id 和 `logagent://` MCP resource namespace 作为兼容层。产品可见名使用 LocalToolHub。

## 模块

| 目录 | 职责 | 文档 |
|------|------|------|
| `server/` | Rust 本地 Server、dev_selftest step tools、日志分析工具运行、MCP、artifact、配置和静态 WebUI 托管 | [README](./server/README.md) / [SPEC](./server/SPEC.md) |
| `webui/` | 本地管理页面（Tools / Runs History / MCP / Settings） | [README](./webui/README.md) / [SPEC](./webui/SPEC.md) |
| `native-agent/` | 可选本机文件导入桥（Chrome Extension 传递下载文件 → 上传） | [README](./native-agent/README.md) / [SPEC](./native-agent/SPEC.md) |
| `chrome-extension/` | 可选 Chrome 下载监听和导入确认 | [README](./chrome-extension/README.md) / [SPEC](./chrome-extension/SPEC.md) |
| `deploy/` | 单机 runtime 部署、重建和控制脚本 | [README](./deploy/README.md) |
| `skills/` | 用户安装到 Claude Code 的本地 skill 分发目录；Server 不加载、不提供 API | [README](./skills/README.md) / [SPEC](./skills/SPEC.md) |
| `docs/runbooks/` | 诊断 runbook 作者参考（legacy manifest 仅留档，不再由 server 托管） | [README](./docs/runbooks/README.md) |
| `docs/modules/` | Server 内部能力边界和后续开发约束 | [README](./docs/modules/README.md) |
| `third_party/` | 上游诊断工具源码引用；不改写上游 README | - |

## 模块边界

Server 内部能力以两模块为中心：

- **dev_selftest** 与 **日志分析** 是核心；dev_selftest 的完整 workflow 由客户端 skill 编排，Server 只提供 MCP step tools。
- `remote_execution` 只保留 docker runner（dev_selftest 的 inline docker test 复用）+ command 模板；SSH/SCP executor 与「纳管」executor record 已移除。
- MCP 是外部 Agent 的集成入口。
- 旧 Log Analysis Agent / LLM Gateway / Claude Code runner / fetch / metadata / cases / server-side skills / executors / workflow engine 模块已移除，不再作为目标架构。

## API 原则

- 接口使用 `/api/tools*`、`/api/runs*`、`/api/artifacts*`、`/api/mcp*`、`/api/settings*` 等工具工作台语义。
- 所有受保护接口使用 `Authorization: Bearer <api-key>`。
- MCP resources/tools 与 WebUI 使用同一个 registry 和同一套执行边界；`mcp.enabled=false` 时 HTTP `/api/mcp` 与 stdio `mcp-serve` 都必须拒绝服务。

## 安全边界

- API Key 只从环境变量或本地 secret 配置读取。
- 不把密钥、Cookie、Authorization header 写入日志、artifact 或导出包。
- dev_selftest 的 docker target、git repo、build/test profile 都走配置/运行时 allowlist；tool params 只选 profile id + 携带 runId，不接受任意 shell。git repo/ref allowlist 支持受控热更新：先 `git ls-remote` 验证、原子写回 `--config` YAML，再更新内存状态。Docker-backed build/test profile 也支持用户确认后的受控 upsert；已排队任务会携带 profile snapshot，已存在的 dev_selftest run 不被改写。
- `run_tests.testParams` 只允许非凭据字符串；它们会以 `--env DEVSELFTEST_PARAM_*=...` 传给 Docker，启动期间可被同机进程看到，因此不得传 password/token/secret/auth 等敏感值。
- MCP client 不能绕过 Server 直接执行本机命令或读取任意路径。
- Artifact path 对外使用逻辑路径，不暴露任意本机路径。

## 验证

```bash
cargo fmt --check
cargo check
cargo test
cd webui && npm run lint && npm run typecheck && npm run build
git diff --check
```

每次修改后必须同步更新对应 README/SPEC 和 [PROGRESS.md](./PROGRESS.md)。历史进展已归档到 `docs/archive/`。
