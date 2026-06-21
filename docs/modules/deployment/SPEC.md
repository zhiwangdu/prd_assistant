# Deployment Spec

## 目标

MVP 采用尽量简单的部署形态：Rust Server + WEBUI 静态目录 + Native Agent 本机进程。

## 当前状态

已支持：

- Server 从项目根目录启动并托管 `webui/`。
- `scripts/start-local.sh` 支持真实 LLM、stub 和前台调试模式；后台模式必须在非交互 shell 中保持 Server 进程存活。
- `scripts/v2-local.sh` 支持 V2 本地 build/start/stop/restart/status/logs/smoke-tools；
  默认使用 `server-v2/.venv`、`/tmp/logagent-v2-local`、端口 `50993` 和
  `target/tools`，`start` 在已有 virtualenv/WebUI 时不得重复执行 editable
  install，显式 `--with-tools` / `--only-tool <name>` 时才构建 source-built
  analyzers；`status` 会在服务运行时通过带 Bearer API Key 的 `/api/v2/tools`
  查询打印 `sourceBuiltAnalyzers` 状态、命令存在性、可执行性和不可用原因，
  便于确认 submodule analyzer 是否已被当前进程注册且真实可执行；
  `smoke-tools` 复用 source-built analyzer 聚合 smoke 入口。
- 工作目录脚本通过 `LOGAGENT_WORK_DIR` 定位运行目录，支持初始化 `bin/config/data/logs/run/webui`、快速编译 Server、快速编译 WebUI、启动、停止、重启、状态和日志查看；缺少 `LOGAGENT_WORK_DIR` 时必须报错。
- 根目录 `deploy/` 提供可复制到 runtime 的部署模板：`.env.example`、`logagent.example.yaml`、`logagentctl.sh`、`rebuild-install.sh` 和 README。该模板默认父目录为 `LOGAGENT_APP_DIR`，脚本 best-effort 加载 `$HOME/.bashrc` 后自动加载同目录 `.env`，真实 `.env` 和 active `logagent.yaml` 不提交。
- `deploy/logagent-v2ctl.sh` 和 `deploy/rebuild-v2-install.sh` 提供 V2 Python/FastAPI runtime 的快速构建、安装、启动、停止、重启、状态和日志查看。V2 默认使用 `$LOGAGENT_APP_DIR/server-v2/.venv`、`$LOGAGENT_APP_DIR/data-v2`、`$LOGAGENT_APP_DIR/webui/out` 和端口 `50993`。`logagent-v2ctl.sh start/restart` 会等待 health ready，启动失败时清理 stale pid 并返回非零状态；默认只按当前 runtime 的 pid file 判定运行进程，避免多实例互相误控；`status` 会保留这个 pid-file 作用域，并在 health 成功后尝试 authenticated tools catalog probe，打印四个 source-built analyzer 的 registered/enabled/runnable、命令存在性、可执行性和不可用原因。完整 `rebuild-v2-install.sh` 默认会把 source-built analyzers 构建到 `$LOGAGENT_APP_DIR/bin/tools`，从而初始化所需 `third_party/` submodules；`--skip-tools` 显式跳过 analyzer clone/compile，`--tools-only --only-tool <name>` 用于快速单工具重建；`<name>` 支持短名 `influxql|flux|opengemini|influxdb`，也支持 V2 catalog ID `influxql_analyzer|flux_query_analyzer|opengemini_storage_analyzer|influxdb_storage_analyzer`；脚本会在存在 `$HOME/.cargo/env` 时加载它，以支持非交互 SSH shell 下 Flux 和 InfluxDB analyzer 构建所需的 rustup cargo。
- `logagent-v2ctl.sh help`、`--help` 和 `-h` 必须打印 usage 并以 0 退出；未知命令仍返回错误，避免自动化脚本把拼写错误当成成功操作。
- V2 部署脚本回归测试覆盖 `scripts/v2-local.sh --help` 中的
  `smoke-tools` 入口、`logagent-v2ctl.sh --help` 的成功 usage 输出、
  启动超时参数校验、`logagent-v2ctl.sh` 默认 pid file 作用域、
  未安装 runtime 时 `start` 的快速失败路径、
  `rebuild-v2-install.sh --help`、缺少 `LOGAGENT_SRC_DIR` 的快速失败路径，
  默认 full install 调用 `scripts/build-tools.sh`、`--skip-tools` 显式跳过工具构建，
  以及 `--tools-only --only-tool <name>` 对 `scripts/build-tools.sh` 的
  canonical 单工具参数透传。
- `deploy/install-deps.sh` 支持快速安装从源码 rebuild 需要的通用依赖：git、curl、C/C++ build tools、pkg-config、Node.js/npm、Go，并在缺少 cargo 时通过 rustup 安装 Rust。运行已构建 Server binary 不要求单独安装 SQLite。
- `scripts/build-tools.sh` 从 `third_party/` submodules 构建 InfluxQL、Flux、openGemini storage 和 InfluxDB storage analyzers；`scripts/build-server.sh` 会安装到 `$LOGAGENT_WORK_DIR/bin/tools/`，`deploy/rebuild-install.sh` 会安装到 `$LOGAGENT_APP_DIR/bin/tools/`。构建前会调用 `scripts/configure-tool-submodules.sh`，支持通过 `LOGAGENT_SUBMODULE_BASE_URL` 或各 `LOGAGENT_SUBMODULE_*_URL` 把 submodule clone 地址覆盖到本地 Git config，以便内网镜像环境初始化 submodule；该脚本不得改写顶层仓库 `origin`。InfluxDB storage analyzer 构建必须确保 `third_party/flux` 已初始化，并在构建期间临时把 InfluxDB `go.mod` 中的 `github.com/influxdata/flux` replace 到该本地源码，使 `pkg-config.sh` 能在完整 `libflux` 源码树上调用 cargo，构建结束后必须恢复 InfluxDB `go.mod/go.sum`。这段临时 InfluxDB 构建默认使用 `GOSUMDB=off` 避免内网镜像或代理环境访问公共 checksum DB 失败，但必须尊重用户显式设置的 `GOSUMDB`。
- `scripts/smoke-source-built-analyzers.sh` 聚合运行四个 source-built analyzer
  的真实 CLI smoke，也支持 `--only <name>` 单独验证
  `influxql|flux|opengemini|influxdb` 或对应 V2 catalog ID。
- `deploy/logagentctl.sh` 和 `deploy/rebuild-install.sh` 会预创建 Memory/Case 相关运行目录，包括 `data/memory`、`data/cases` 和 `data/case_imports`；`rebuild-install.sh` 在存在 `$HOME/.cargo/env` 时加载它，以支持非交互 SSH shell 下的 rustup cargo。
- macOS 开发机上的 `scripts/build-all.sh` 在本地 Server/WebUI 编译完成后调用 `scripts/auto-deploy-lan.sh`；该脚本 ping `192.168.31.128`，连通时通过 SSH 到 `duzhiwang@192.168.31.128`，在远端源码目录执行 `git pull --ff-only`，再运行 runtime `deploy/rebuild-install.sh` 和 `logagentctl.sh start/status`。`LOGAGENT_LAN_AUTO_DEPLOY=0` 可关闭该行为。
- `deploy/logagent.example.yaml` 包含默认关闭的 `embedding` 配置块、默认 `claude_code` 配置和 `mcp.transport=stdio`；`LOGAGENT_CLAUDE_CODE_PATH` 默认应指向 `which claude` 输出的绝对路径，Server 会以 Claude Code CLI 非交互 JSON + MCP 模式调用。
- Server 提供个人高级入口所需的受保护接口：`POST /api/mcp/readonly`、`GET /api/exports/skills.zip` 和 `GET /api/exports/tools.zip`。部署脚本不写入个人 Claude Code 配置，不做本地 bootstrap。
- Native Agent 本机启动并连接远端 Server。
- 示例配置支持 50992 测试端口。

## 运行形态

本地闭环：

```text
Chrome Extension -> Native Agent 127.0.0.1 -> Server 127.0.0.1
```

远端测试：

```text
Chrome Extension -> Native Agent 127.0.0.1 -> Server 192.168.x.x
WEBUI -> Server 同源 API
```

## 部署文件

- Server binary
- Runtime `$LOGAGENT_WORK_DIR/bin/logagent-server`
- Runtime `$LOGAGENT_WORK_DIR/bin/tools/*` 或 `$LOGAGENT_APP_DIR/bin/tools/*` source-built analyzers
- Native Agent binary
- `$LOGAGENT_WORK_DIR/webui/out`
- `$LOGAGENT_WORK_DIR/config/server.yaml`
- Runtime `deploy/.env` 和 `deploy/logagent.yaml`
- Repository `deploy/.env.example`、`deploy/logagent.example.yaml`、`deploy/logagentctl.sh`、`deploy/rebuild-install.sh`
- Repository `deploy/logagent-v2ctl.sh`、`deploy/rebuild-v2-install.sh`
- Repository `deploy/install-deps.sh`
- Repository `scripts/v2-local.sh`
- Runtime `$LOGAGENT_APP_DIR/server-v2/.venv`
- Runtime `$LOGAGENT_APP_DIR/data-v2/logagent.sqlite`
- Runtime `$LOGAGENT_APP_DIR/logagent-v2.pid`
- Runtime `$LOGAGENT_APP_DIR/logagent-v2.log`
- `$LOGAGENT_WORK_DIR/data/memory/memory.sqlite`
- `$LOGAGENT_WORK_DIR/data/cases/*.json`
- `$LOGAGENT_WORK_DIR/data/case_imports/*.json`
- `$LOGAGENT_WORK_DIR/run/logagent-server.pid`
- `$LOGAGENT_WORK_DIR/logs/logagent-server.log`
- 环境变量密钥
- 持久化 tasks、analysis state/events 和 workspaces 的数据目录
- 通过受保护接口动态生成的 `skills.zip` 和 `tools.zip` 下载包

## 验收标准

- Server 启动后 `/health` 和 `/` 可访问。
- `deploy/install-deps.sh --dry-run` 和 `--help` 可执行，不修改宿主。
- deploy 模板启动前会创建 `data/memory`、`data/cases` 和 `data/case_imports`，且重建安装不能删除已有运行数据。
- V2 deploy 模板能创建 virtualenv、安装 `server-v2`、初始化 SQLite、同步 WebUI、启动/停止/重启服务，并且不删除已有 `data-v2`。
- `scripts/v2-local.sh --help` 可执行；`build` 能创建/更新本地 V2
  virtualenv 并初始化 SQLite；`start --no-build` 在已有 virtualenv 时能直接
  启动并等待 health；`status`、`stop`、`restart`、`logs` 使用本地 pid/log
  文件，不影响 runtime deploy pid；`status` 会用 API Key 查询
  `/api/v2/tools` 并打印 `sourceBuiltAnalyzers` 状态、命令存在性、可执行性
  和不可用原因，查询失败不得导致服务状态检查失败；
  `smoke-tools --only-tool <name>` 必须把 V2 catalog ID
  规范化后传给聚合 smoke 脚本。
- V2 `logagent-v2ctl.sh start/restart` 必须等待 `/health` 成功；进程提前退出或 health 超时必须清理 pid 文件并返回失败。
- V2 `logagent-v2ctl.sh help|--help|-h` 必须返回成功并打印 usage；未知命令必须返回失败。
- V2 控制脚本默认必须按当前 runtime pid file 管理进程，不得在未显式开启发现模式时通过全局 `pgrep` 控制其它运行目录的 V2 实例。
- V2 部署脚本回归测试必须能在不启动长期服务、不修改宿主全局环境的情况下验证帮助输出、配置校验、pid file 作用域、未安装 runtime 的失败提示、本地 `status` 的 tools catalog Bearer 查询和 analyzer 状态输出、tools-only 单工具重建不会创建 V2 virtualenv 或同步 WebUI，以及 V2 catalog toolId 会规范化为底层 `scripts/build-tools.sh` 的短名。
- V2 deploy 模板的默认 full install、`--with-tools` 和 `--tools-only` 能复用 `scripts/build-tools.sh` 构建 InfluxQL、Flux、openGemini storage 和 InfluxDB storage analyzer，非交互 SSH shell 下也能通过 `$HOME/.cargo/env` 找到 rustup-managed `cargo`，并且 InfluxDB analyzer 的 `pkg-config.sh` 必须使用本地 `third_party/flux` 源码构建 `libflux`，不得回退到缺少 Rust workspace 的 Go module cache；未显式设置 `GOSUMDB` 时，InfluxDB 临时构建默认关闭公共 checksum DB 查询。`.env.example` 提供 V2 工具路径、Fetch allowlist/request/response 边界、Remote Executor SSH 边界、pprof 和 Huawei package sync 的环境变量样例。设置 `LOGAGENT_V2_TOOL_*_ANALYZER` 后，V2 会按 `examples/server-tools.yaml` 的 args、timeout、`maxInputFiles` 和 match rules 自动注册对应 analyzer；`--skip-tools` 是唯一跳过默认 source-built analyzer clone/compile 的完整安装选项。
- 运行目录快捷脚本在缺少 `LOGAGENT_WORK_DIR` 时失败；设置后能初始化工作目录、编译 Server、同步 WebUI、启动/停止/重启服务。
- 运行目录和 deploy rebuild 脚本会从 submodules 构建 source-built analyzers，并把默认配置中的工具路径指向对应构建产物。
- 构建 source-built analyzers 后，聚合 smoke 脚本应能全量或单工具运行
  InfluxQL、Flux、openGemini storage 和 InfluxDB storage analyzer smoke。
- 内网环境可以在不修改 `.gitmodules` 的情况下通过 `.env` 或环境变量指定 source-built analyzer submodule clone URL；`build-tools.sh` 和手工 `configure-tool-submodules.sh` 都必须把这些 URL 写入本地 Git submodule config，并且在 submodule 目录存在但未初始化时不得把顶层仓库 `origin` 改成 submodule URL。
- macOS 上运行 `scripts/build-all.sh` 后，若 `192.168.31.128` 可 ping 通，应自动触发远端 pull/rebuild/start；不可达时必须跳过远端部署且本地构建仍成功。
- Native Agent `/health` 可访问。
- 远端 Server 监听 `0.0.0.0` 时 Native Agent 可上传。
- README 和 SPEC 在部署方式或端口变更时同步更新。
- Server 重启后能恢复等待中的任务，并安全处理执行中断的 MCP tool 副作用。
- 个人只读 HTTP MCP 和导出下载可通过 API Key 访问；Tools 包 manifest 标明 Server OS/arch 和 skipped 工具。
