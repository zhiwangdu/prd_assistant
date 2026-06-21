# Deployment 方案

## 当前部署形态

V2 分支采用单机 Python/FastAPI Server：

- `server-v2/` Python virtualenv。
- SQLite WAL 持久化状态。
- 本地 filesystem artifacts。
- `webui/out` 静态页面由 V2 Server 托管。
- source-built analyzers 构建到 `bin/tools/`，由 V2 tools catalog 自动发现。

旧 Rust `server/` crate、V1 control script 和 V1 rebuild script 已从 V2 分支删除。

## 本地开发

```bash
./scripts/v2-local.sh build
./scripts/v2-local.sh start
./scripts/v2-local.sh status
./scripts/v2-local.sh smoke-tools
./scripts/v2-local.sh logs
./scripts/v2-local.sh stop
```

`v2-local.sh` 默认使用 `server-v2/.venv`、`/tmp/logagent-v2-local`、端口
`50993` 和 `target/tools`。需要构建 submodule analyzer 时使用
`--with-tools` 或 `--only-tool influxql|flux|opengemini|influxdb`；`--only-tool`
也接受 V2 catalog ID。

## Runtime 部署

```bash
cd /path/to/runtime/deploy
cp .env.example .env
./install-deps.sh
./rebuild-v2-install.sh
./logagent-v2ctl.sh start
./logagent-v2ctl.sh status
```

`rebuild-v2-install.sh` 创建或复用 `$LOGAGENT_V2_VENV_DIR`，安装 `server-v2`，
初始化 `$LOGAGENT_V2_DATA_DIR` 下的 SQLite，构建并同步 WebUI 到
`$LOGAGENT_V2_WEBUI_DIR`，默认把四个 source-built analyzer 构建到
`$LOGAGENT_APP_DIR/bin/tools`。`--skip-tools` 是完整安装时跳过 submodule
clone/compile 的显式选项；`--tools-only --only-tool <name>` 用于快速重建单个
analyzer。

`logagent-v2ctl.sh status` 在 health 成功后会带 API Key 查询 `/api/v2/tools`，
打印 `sourceBuiltAnalyzers` 中四个 analyzer 的 registered/enabled/runnable、
命令存在性、可执行性和不可用原因。

## 依赖

- Python 3。
- Node.js/npm。
- git、curl、C/C++ build tools、pkg-config。
- Go toolchain，用于 InfluxQL、openGemini 和 InfluxDB analyzer。
- Rust/cargo，仅在构建 Flux analyzer 或 InfluxDB analyzer 的本地 Flux
  `libflux` 时需要。
- OpenSSH client，用于 Remote Executor。

`deploy/install-deps.sh` 支持 macOS Homebrew、Debian/Ubuntu apt、Fedora dnf、
RHEL/CentOS yum 和 Arch pacman。它会在缺少 cargo 时通过 rustup 安装最小 Rust
toolchain，因为部分 analyzer build 仍需要 cargo；V2 Server 自身不依赖 Rust
运行时。

## 工具构建

`scripts/build-tools.sh` 从 `third_party/` submodules 构建 InfluxQL、Flux、
openGemini storage 和 InfluxDB storage analyzers。输出目录优先级：

1. `LOGAGENT_TOOLS_BIN_DIR`
2. `LOGAGENT_WORK_DIR/bin/tools`
3. `target/tools`

部署脚本会显式传入 `$LOGAGENT_APP_DIR/bin/tools`。内网镜像环境可设置
`LOGAGENT_SUBMODULE_BASE_URL` 或各 `LOGAGENT_SUBMODULE_*_URL`；脚本只写本地
Git submodule config，不修改 `.gitmodules` 或顶层仓库 `origin`。

InfluxDB storage analyzer 构建会确保 `third_party/flux` 已初始化，并在构建期
临时把 InfluxDB `go.mod` 中的 `github.com/influxdata/flux` replace 到本地
`third_party/flux`，让 `pkg-config.sh` 从完整 Rust `libflux` 源码树构建。
构建结束后会恢复 InfluxDB `go.mod/go.sum`；未显式设置 `GOSUMDB` 时默认使用
`GOSUMDB=off`。
