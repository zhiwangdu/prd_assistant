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
