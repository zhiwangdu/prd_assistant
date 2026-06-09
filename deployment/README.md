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

运行目录建议把部署资产统一放在 `deploy/` 下，并通过环境变量定位运行目录和源码目录：

```bash
cd "$LOGAGENT_APP_DIR/deploy"
cp env.example .env
set -a
source .env
set +a

./logagentctl.sh start|stop|restart|status|logs
./rebuild-install.sh [--server-only] [--no-restart]
```

`LOGAGENT_APP_DIR` 指向运行目录，`LOGAGENT_SRC_DIR` 指向源码仓库。`deploy/logagentctl.sh` 从 `$LOGAGENT_APP_DIR/bin/logagent-server` 启动服务，使用 `$LOGAGENT_APP_DIR/deploy/logagent.yaml`、`$LOGAGENT_APP_DIR/logagent-server.pid` 和 `$LOGAGENT_APP_DIR/logagent-server.log`。`deploy/rebuild-install.sh` 会从 `$LOGAGENT_SRC_DIR` 编译 `logagent-server`，替换运行目录 binary，默认同步 `webui/out`，并在服务原本运行时自动重启；`--server-only` 只替换 Rust 二进制，`--no-restart` 跳过重启。

生产或测试环境：

- systemd 管理 `logagentd`
- systemd 管理 Native Agent，或用户登录后自启动
- WebUI 可由 Rust Server 静态托管，也可独立由前端 dev server 构建产物部署

## 系统依赖

- `rg`
- `git`
- `ssh`
- `scp` 或 Rust SSH/SFTP 库
- 已配置的外部分析工具，例如 `flux_query_analyzer`
- SQLite，或后续 PostgreSQL + pgvector

启动时应检查依赖是否存在，并在 WebUI/日志中暴露健康检查结果。

## 后续演进

当任务量变大后再拆分：

- Worker 进程
- 队列
- 独立 Tool Runner
- 独立 Environment Collector

第一版不拆。

Analysis Agent 与 LLM Gateway 是进程内逻辑组件，但状态、事件和待处理请求必须持久化到 Server 数据目录，不能依赖进程内会话。Server 重启后恢复 `RUNNING` 和等待状态，避免重复执行已完成 action。
