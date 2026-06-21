# Deployment Spec

## 目标

V2 分支部署目标是单机 Python/FastAPI Server + SQLite WAL + 本地 artifacts +
WebUI 静态目录。旧 Rust `server/` crate 不再是
V2 分支的一部分。

## 当前状态

- `scripts/v2-local.sh` 支持 V2 本地 build/start/stop/restart/status/logs/smoke-tools。
- `deploy/logagent-v2ctl.sh` 支持 V2 runtime start/stop/restart/status/logs/smoke-tools。
- `deploy/rebuild-v2-install.sh` 支持安装 `server-v2`、初始化 SQLite、构建并同步 WebUI、构建 source-built analyzers、按需重启 V2。
- `deploy/install-deps.sh` 安装源码 rebuild 需要的通用依赖，并仅为 analyzer build 保留 cargo 安装路径。
- `scripts/build-tools.sh` 构建 InfluxQL、Flux、openGemini storage 和 InfluxDB storage analyzers，支持内网 submodule URL 覆盖和 InfluxDB analyzer 的本地 Flux replace。
- `scripts/smoke-source-built-analyzers.sh` 聚合运行四个 source-built analyzer 的真实 CLI smoke，也支持 `--only <name>` 单独验证。
- `scripts/build-all.sh` 现在执行 V2 本地 build，并在 macOS LAN auto deploy 开启时调用远端 V2 rebuild/start/status。

## 部署文件

- Repository `deploy/.env.example`
- Repository `deploy/install-deps.sh`
- Repository `deploy/logagent-v2ctl.sh`
- Repository `deploy/rebuild-v2-install.sh`
- Repository `scripts/v2-local.sh`
- Repository `scripts/build-tools.sh`
- Repository `scripts/build-webui.sh`
- Runtime `$LOGAGENT_APP_DIR/server-v2/.venv`
- Runtime `$LOGAGENT_APP_DIR/bin/tools/*`
- Runtime `$LOGAGENT_APP_DIR/data-v2/logagent.sqlite`
- Runtime `$LOGAGENT_APP_DIR/data-v2/artifacts`
- Runtime `$LOGAGENT_APP_DIR/webui/out`
- Runtime `$LOGAGENT_APP_DIR/logagent-v2.pid`
- Runtime `$LOGAGENT_APP_DIR/logagent-v2.log`
- 通过受保护接口动态生成的 `skills.zip` 和 `tools.zip` 下载包

## 验收标准

- `deploy/install-deps.sh --dry-run` 和 `--help` 可执行，不修改宿主。
- `scripts/v2-local.sh --help` 可执行；`build` 能创建/更新本地 V2 virtualenv 并初始化 SQLite；`start --no-build` 在已有 virtualenv 时能直接启动并等待 health。
- `scripts/v2-local.sh status`、`stop`、`restart`、`logs` 使用本地 pid/log 文件，不影响 runtime deploy pid。
- V2 `logagent-v2ctl.sh start/restart` 必须等待 `/health` 成功；进程提前退出或 health 超时必须清理 pid 文件并返回失败。
- V2 `logagent-v2ctl.sh help|--help|-h` 必须返回成功并打印 usage；未知命令必须返回失败。
- V2 控制脚本默认必须按当前 runtime pid file 管理进程，不得在未显式开启发现模式时通过全局 `pgrep` 控制其它运行目录的 V2 实例。
- V2 部署脚本回归测试必须能在不启动长期服务、不修改宿主全局环境的情况下验证帮助输出、配置校验、pid file 作用域、未安装 runtime 的失败提示、tools catalog Bearer 查询、analyzer 状态输出和 tools-only 单工具重建。
- V2 deploy 模板的默认 full install、`--with-tools` 和 `--tools-only` 能复用 `scripts/build-tools.sh` 构建 InfluxQL、Flux、openGemini storage 和 InfluxDB storage analyzer。
- 构建 InfluxDB analyzer 时，`pkg-config.sh` 必须使用本地 `third_party/flux` 源码构建 `libflux`，不得回退到缺少 Rust workspace 的 Go module cache；未显式设置 `GOSUMDB` 时默认关闭公共 checksum DB 查询。
- 内网环境可以在不修改 `.gitmodules` 的情况下通过 `.env` 或环境变量指定 source-built analyzer submodule clone URL。
- Native Agent `/health` 可访问，并默认上传到 V2。
- README 和 SPEC 在部署方式或端口变更时同步更新。
