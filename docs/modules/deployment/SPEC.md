# Deployment Spec

## 目标

MVP 采用尽量简单的部署形态：Rust Server + WEBUI 静态目录 + Native Agent 本机进程。

## 当前状态

已支持：

- Server 从项目根目录启动并托管 `webui/`。
- `scripts/start-local.sh` 支持真实 LLM、stub 和前台调试模式；后台模式必须在非交互 shell 中保持 Server 进程存活。
- 工作目录脚本通过 `LOGAGENT_WORK_DIR` 定位运行目录，支持初始化 `bin/config/data/logs/run/webui`、快速编译 Server、快速编译 WebUI、启动、停止、重启、状态和日志查看；缺少 `LOGAGENT_WORK_DIR` 时必须报错。
- 根目录 `deploy/` 提供可复制到 runtime 的部署模板：`.env.example`、`logagent.example.yaml`、`logagentctl.sh`、`rebuild-install.sh` 和 README。该模板默认父目录为 `LOGAGENT_APP_DIR`，脚本 best-effort 加载 `$HOME/.bashrc` 后自动加载同目录 `.env`，真实 `.env` 和 active `logagent.yaml` 不提交。
- `deploy/logagent-v2ctl.sh` 和 `deploy/rebuild-v2-install.sh` 提供 V2 Python/FastAPI runtime 的快速构建、安装、启动、停止、重启、状态和日志查看。V2 默认使用 `$LOGAGENT_APP_DIR/server-v2/.venv`、`$LOGAGENT_APP_DIR/data-v2`、`$LOGAGENT_APP_DIR/webui/out` 和端口 `50993`。`logagent-v2ctl.sh start/restart` 会等待 health ready，启动失败时清理 stale pid 并返回非零状态；默认只按当前 runtime 的 pid file 判定运行进程，避免多实例互相误控。`rebuild-v2-install.sh --with-tools` 会把 source-built analyzers 构建到 `$LOGAGENT_APP_DIR/bin/tools`，`--tools-only --only-tool <name>` 用于快速单工具重建。
- `deploy/install-deps.sh` 支持快速安装从源码 rebuild 需要的通用依赖：git、curl、C/C++ build tools、pkg-config、Node.js/npm、Go，并在缺少 cargo 时通过 rustup 安装 Rust。运行已构建 Server binary 不要求单独安装 SQLite。
- `scripts/build-tools.sh` 从 `third_party/` submodules 构建 InfluxQL、Flux、openGemini storage 和 InfluxDB storage analyzers；`scripts/build-server.sh` 会安装到 `$LOGAGENT_WORK_DIR/bin/tools/`，`deploy/rebuild-install.sh` 会安装到 `$LOGAGENT_APP_DIR/bin/tools/`。构建前会调用 `scripts/configure-tool-submodules.sh`，支持通过 `LOGAGENT_SUBMODULE_BASE_URL` 或各 `LOGAGENT_SUBMODULE_*_URL` 把 submodule clone 地址覆盖到本地 Git config，以便内网镜像环境初始化 submodule；该脚本不得改写顶层仓库 `origin`。
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
- V2 `logagent-v2ctl.sh start/restart` 必须等待 `/health` 成功；进程提前退出或 health 超时必须清理 pid 文件并返回失败。
- V2 控制脚本默认必须按当前 runtime pid file 管理进程，不得在未显式开启发现模式时通过全局 `pgrep` 控制其它运行目录的 V2 实例。
- V2 deploy 模板的 `--with-tools` / `--tools-only` 能复用 `scripts/build-tools.sh` 构建 InfluxQL、Flux、openGemini storage 和 InfluxDB storage analyzer，并且 `.env.example` 提供 V2 工具路径、Fetch allowlist/request/response 边界、pprof 和 Huawei package sync 的环境变量样例。
- 运行目录快捷脚本在缺少 `LOGAGENT_WORK_DIR` 时失败；设置后能初始化工作目录、编译 Server、同步 WebUI、启动/停止/重启服务。
- 运行目录和 deploy rebuild 脚本会从 submodules 构建 source-built analyzers，并把默认配置中的工具路径指向对应构建产物。
- 内网环境可以在不修改 `.gitmodules` 的情况下通过 `.env` 或环境变量指定 source-built analyzer submodule clone URL；`build-tools.sh` 和手工 `configure-tool-submodules.sh` 都必须把这些 URL 写入本地 Git submodule config，并且在 submodule 目录存在但未初始化时不得把顶层仓库 `origin` 改成 submodule URL。
- macOS 上运行 `scripts/build-all.sh` 后，若 `192.168.31.128` 可 ping 通，应自动触发远端 pull/rebuild/start；不可达时必须跳过远端部署且本地构建仍成功。
- Native Agent `/health` 可访问。
- 远端 Server 监听 `0.0.0.0` 时 Native Agent 可上传。
- README 和 SPEC 在部署方式或端口变更时同步更新。
- Server 重启后能恢复等待中的任务，并安全处理执行中断的 MCP tool 副作用。
- 个人只读 HTTP MCP 和导出下载可通过 API Key 访问；Tools 包 manifest 标明 Server OS/arch 和 skipped 工具。
