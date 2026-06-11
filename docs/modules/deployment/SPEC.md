# Deployment Spec

## 目标

MVP 采用尽量简单的部署形态：Rust Server + WEBUI 静态目录 + Native Agent 本机进程。

## 当前状态

已支持：

- Server 从项目根目录启动并托管 `webui/`。
- `scripts/start-local.sh` 支持真实 LLM、stub 和前台调试模式；后台模式必须在非交互 shell 中保持 Server 进程存活。
- 工作目录脚本通过 `LOGAGENT_WORK_DIR` 定位运行目录，支持初始化 `bin/config/data/logs/run/webui`、快速编译 Server、快速编译 WebUI、启动、停止、重启、状态和日志查看；缺少 `LOGAGENT_WORK_DIR` 时必须报错。
- 根目录 `deploy/` 提供可复制到 runtime 的部署模板：`.env.example`、`logagent.example.yaml`、`logagentctl.sh`、`rebuild-install.sh` 和 README。该模板默认父目录为 `LOGAGENT_APP_DIR`，脚本自动加载同目录 `.env`，真实 `.env` 和 active `logagent.yaml` 不提交。
- `deploy/install-deps.sh` 支持快速安装从源码 rebuild 需要的通用依赖：git、curl、C/C++ build tools、pkg-config、Node.js/npm，并在缺少 cargo 时通过 rustup 安装 Rust。运行已构建 Server binary 不要求单独安装 SQLite。
- `deploy/logagentctl.sh` 和 `deploy/rebuild-install.sh` 会预创建 Memory/Case 相关运行目录，包括 `data/memory`、`data/cases` 和 `data/case_imports`。
- `deploy/logagent.example.yaml` 包含默认关闭的 `embedding` 配置块和禁用的成熟 agent adapter 配置，保留 `LOGAGENT_EMBEDDING_API_KEY` 与 `LOGAGENT_AGENT_*_PATH` 接入点但当前不强制设置。
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
- Native Agent binary
- `$LOGAGENT_WORK_DIR/webui/out`
- `$LOGAGENT_WORK_DIR/config/server.yaml`
- Runtime `deploy/.env` 和 `deploy/logagent.yaml`
- Repository `deploy/.env.example`、`deploy/logagent.example.yaml`、`deploy/logagentctl.sh`、`deploy/rebuild-install.sh`
- Repository `deploy/install-deps.sh`
- `$LOGAGENT_WORK_DIR/data/memory/memory.sqlite`
- `$LOGAGENT_WORK_DIR/data/cases/*.json`
- `$LOGAGENT_WORK_DIR/data/case_imports/*.json`
- `$LOGAGENT_WORK_DIR/run/logagent-server.pid`
- `$LOGAGENT_WORK_DIR/logs/logagent-server.log`
- 环境变量密钥
- 持久化 tasks、analysis state/events 和 workspaces 的数据目录

## 验收标准

- Server 启动后 `/health` 和 `/` 可访问。
- `deploy/install-deps.sh --dry-run` 和 `--help` 可执行，不修改宿主。
- deploy 模板启动前会创建 `data/memory`、`data/cases` 和 `data/case_imports`，且重建安装不能删除已有运行数据。
- 运行目录快捷脚本在缺少 `LOGAGENT_WORK_DIR` 时失败；设置后能初始化工作目录、编译 Server、同步 WebUI、启动/停止/重启服务。
- Native Agent `/health` 可访问。
- 远端 Server 监听 `0.0.0.0` 时 Native Agent 可上传。
- README 和 SPEC 在部署方式或端口变更时同步更新。
- Server 重启后能恢复等待中的任务，并安全处理执行中断的 action。
