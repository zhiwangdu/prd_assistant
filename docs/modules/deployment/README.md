# Deployment 方案

## MVP 部署形态

MVP 推荐使用单一 Rust binary，内部按 crate/module 拆分。

原因：

- 降低个人开发和部署复杂度。
- 避免一开始维护多个服务、队列和跨服务 API。
- 模块边界仍可通过 Rust trait 和目录结构保持清晰。

## 推荐结构

```text
logagent/
  crates/
    server/
    log_analyzer/
    tool_runner/
    code_evidence/
    environment_collector/
    analysis_agent/
    llm_gateway/
    case_store/
    config/
  apps/
    logagentd/
    native_agent/
    webui/
```

## 运行方式

开发环境：

```bash
logagentd --config ./logagent.yaml
```

仓库内本地快速启动可使用：

```bash
./scripts/start-local.sh --llm
```

该脚本会构建 Server、后台启动、写入 `/tmp/logagent-server-llm.pid` 和 `/tmp/logagent-server-llm.log`，并等待 `/health` 成功；`--stub` 使用本地 stub provider，`--foreground` 用于调试启动日志。

运行目录脚本通过 `LOGAGENT_WORK_DIR` 显式定位运行目录。该变量未设置时，脚本必须直接报错，避免把 pid、日志、数据或构建产物写到不明确的位置：

```bash
export LOGAGENT_WORK_DIR=/opt/logagent
export LOGAGENT_NATIVE_API_KEY=<secret>

./scripts/init-workdir.sh
./scripts/build-server.sh
./scripts/build-webui.sh
./scripts/server-service.sh start|stop|restart|status|logs
```

`init-workdir.sh` 创建 `bin/`、`config/`、`data/`、`logs/`、`run/` 和 `webui/`，并生成 `config/server.yaml`。`build-server.sh` 编译并安装 `$LOGAGENT_WORK_DIR/bin/logagent-server`，并调用 `build-tools.sh` 从 `third_party/` submodules 构建 `$LOGAGENT_WORK_DIR/bin/tools/` 下的源码引用诊断工具。`build-tools.sh` 会先读取 `LOGAGENT_SUBMODULE_BASE_URL` 或各 `LOGAGENT_SUBMODULE_*_URL`，把自定义 clone 地址写入本地 Git submodule 配置，再按需初始化 submodule，适配无法访问 GitHub 的内网部署；该步骤不会修改顶层仓库 `origin`，只有已初始化的 submodule worktree 才会同步更新其自身 `origin`。`build-webui.sh` 编译并同步 `$LOGAGENT_WORK_DIR/webui/out`。`server-service.sh` 使用 `$LOGAGENT_WORK_DIR/run/logagent-server.pid`、`$LOGAGENT_WORK_DIR/logs/logagent-server.log` 和 `$LOGAGENT_WORK_DIR/config/server.yaml` 管理服务。

测试/长期运行环境也可以使用仓库根目录 `deploy/` 模板：

```bash
cp -a deploy /opt/logagent/deploy
cd /opt/logagent/deploy
cp .env.example .env
cp logagent.example.yaml logagent.yaml
./install-deps.sh
./rebuild-install.sh
./logagentctl.sh start
```

`deploy/logagentctl.sh` 和 `deploy/rebuild-install.sh` 会自动加载同目录 `.env`，默认使用父目录作为 `LOGAGENT_APP_DIR`。`logagentctl.sh` 以 detached 后台方式启动 Server，适合从非交互 shell 或自动化脚本执行。部署脚本会预创建 `data/uploads`、`data/sessions`、`data/tasks`、`data/workspaces`、`data/cases`、`data/case_imports` 和 `data/memory`；其中 `data/memory/memory.sqlite` 是 Memory SQLite 主索引，`data/cases/*.json` 保留为 legacy Case 迁移和回滚源。`deploy/rebuild-v2-install.sh` 也会在存在 `$HOME/.cargo/env` 时加载它，保证非交互 SSH shell 下用 `--with-tools` / `--tools-only` 构建 Flux analyzer 时能找到 rustup-managed `cargo`。

`deploy/logagent.example.yaml` 包含默认关闭的 `embedding` 配置块、`claude_code` 配置和 `mcp.transport=stdio`。当前部署不需要 `LOGAGENT_EMBEDDING_API_KEY`；默认需要 `LOGAGENT_CLAUDE_CODE_PATH` 指向 `claude` CLI。`deploy/.env.example` 还提供 V2 Fetch allowlist、request/response size、redirect 和 credential secret 示例，以及 submodule 内网镜像变量：`LOGAGENT_SUBMODULE_BASE_URL` 适合四个工具仓库位于同一 Git namespace 的场景，单仓库变量 `LOGAGENT_SUBMODULE_INFLUXQL_URL`、`LOGAGENT_SUBMODULE_FLUX_URL`、`LOGAGENT_SUBMODULE_OPENGEMINI_URL` 和 `LOGAGENT_SUBMODULE_INFLUXDB_URL` 优先级更高。

V2 `deploy/logagent-v2ctl.sh start` 和 `restart` 会等待配置的 health URL
成功；如果进程启动后退出或超过 `LOGAGENT_V2_STARTUP_TIMEOUT_SECONDS`
仍未 ready，脚本会清理 stale pid 并以非零状态退出。控制脚本默认只信任
当前 runtime 的 pid file，不通过全局进程扫描接管其它运行目录的 V2 进程。

个人本地 Claude Code 不由部署脚本自动接管。Server 运行后会在受保护 API 下提供：

- `POST /api/mcp/readonly`：只读 HTTP MCP 知识入口。
- `GET /api/exports/skills.zip`：当前索引 Skills 全量包。
- `GET /api/exports/tools.zip`：当前 enabled 工具的 Server 平台二进制快照包。

这些接口都需要 `Authorization: Bearer <api-key>`。Tools 包只保证与 Server 所在 OS/arch 匹配；跨平台使用需要个人自行准备对应工具。

生产或测试环境：

- systemd 管理 `logagentd`
- systemd 管理 Native Agent，或用户登录后自启动
- WebUI 可由 Rust Server 静态托管，也可独立由前端 dev server 构建产物部署

## 系统依赖

- 运行已构建 Server binary 不需要单独安装 SQLite；Server 使用 bundled SQLite。
- 从源码运行 `deploy/rebuild-install.sh` 需要 `cargo`、Node.js/npm、Go、`git`、`curl`、C/C++ 编译工具和 `pkg-config`。
- `deploy/install-deps.sh` 支持 macOS Homebrew、Debian/Ubuntu apt、Fedora dnf、RHEL/CentOS yum 和 Arch pacman，可快速安装通用构建依赖，并在缺少 `cargo` 时通过 rustup 安装最小 Rust toolchain。
- `rg`、`ssh`、`scp` 后续 Environment Collector 和代码/环境采集会用到；当前核心上传分析链路不是硬依赖。
- InfluxQL、Flux、openGemini storage 和 InfluxDB storage analyzers 由 `third_party/` submodules 构建到 `bin/tools/`，部署样例默认启用；V2 的 `LOGAGENT_V2_TOOL_*_ANALYZER` 环境变量会按 `examples/server-tools.yaml` 的 args、timeout、`maxInputFiles` 和 match rules 自动注册这些工具；`pprof_analyzer` 运行时需要配置 Go 可执行文件。

启动时应检查依赖是否存在，并在 WebUI/日志中暴露健康检查结果。

## 后续演进

当任务量变大后再拆分：

- Worker 进程
- 队列
- 独立 Tool Runner
- 独立 Environment Collector

第一版不拆。

Analysis Orchestrator、Claude Code Session Runner、LogAgent MCP 与 LLM Gateway 是进程内逻辑组件，但状态、事件和待处理请求必须持久化到 Server 数据目录，不能依赖进程内会话。Server 重启后恢复 `RUNNING` 和等待状态，避免重复执行已完成 MCP tool 副作用。
