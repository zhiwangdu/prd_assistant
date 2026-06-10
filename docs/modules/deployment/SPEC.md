# Deployment Spec

## 目标

MVP 采用尽量简单的部署形态：Rust Server + WEBUI 静态目录 + Native Agent 本机进程。

## 当前状态

已支持：

- Server 从项目根目录启动并托管 `webui/`。
- `scripts/start-local.sh` 支持真实 LLM、stub 和前台调试模式；后台模式必须在非交互 shell 中保持 Server 进程存活。
- 运行目录 `deploy/` 提供 `README.md`、`env.example`、`logagent.example.yaml`、`logagent.yaml`、`logagentctl.sh` 和 `rebuild-install.sh`；脚本通过 `LOGAGENT_APP_DIR` 和 `LOGAGENT_SRC_DIR` 定位运行目录与源码目录，支持启动、停止、查看日志，以及从源码快速编译并替换 `bin/logagent-server`。
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
- Runtime `bin/logagent-server` in the deployment directory
- Native Agent binary
- `webui/out`
- `deploy/logagent.yaml`
- `deploy/env.example`
- `deploy/logagentctl.sh`
- `deploy/rebuild-install.sh`
- 环境变量密钥
- 持久化 tasks、analysis state/events 和 workspaces 的数据目录

## 验收标准

- Server 启动后 `/health` 和 `/` 可访问。
- 运行目录快捷脚本能编译 Server、替换 runtime binary，并在服务运行时重启。
- Native Agent `/health` 可访问。
- 远端 Server 监听 `0.0.0.0` 时 Native Agent 可上传。
- README 和 SPEC 在部署方式或端口变更时同步更新。
- Server 重启后能恢复等待中的任务，并安全处理执行中断的 action。
