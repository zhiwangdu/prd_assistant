# Local Runtime Deploy

`deploy/` 保存 LogAgent 本地 Tool/MCP Workbench 的运行时部署模板。目标是把 Rust binary、WebUI 静态文件、工具二进制和本地 data 目录组织成可复制目录。

## 目标目录

```text
$LOGAGENT_APP_DIR/
  bin/
    logagent-local
    tools/
      flux_query_analyzer
      influxql_analyzer
      opengemini-storage-analyzer
      influxdb_storage_analyzer
  data/
    uploads/
    runs/
    artifacts/
    metadata/
    cases/
    code_worktrees/
  webui/out/
  deploy/
    logagent.yaml
    .env
    logagentctl.sh
```

## 环境变量

必需：

- `LOGAGENT_APP_DIR`
- `LOGAGENT_SRC_DIR`
- `LOGAGENT_NATIVE_API_KEY` 或后续统一的 `LOGAGENT_API_KEY`

可选：

- `LOGAGENT_CONFIG`
- `LOGAGENT_SERVER_BIN`
- `LOGAGENT_HEALTH_URL`
- `LOGAGENT_SUBMODULE_BASE_URL`
- `LOGAGENT_SUBMODULE_INFLUXQL_URL`
- `LOGAGENT_SUBMODULE_FLUX_URL`
- `LOGAGENT_SUBMODULE_OPENGEMINI_URL`
- `LOGAGENT_SUBMODULE_INFLUXDB_URL`

LLM/Claude Code 相关变量不再是默认部署必需项，只能用于可选 automation。

## 构建安装

```bash
./rebuild-install.sh
```

目标行为：

- 构建 Rust Server binary。
- 构建 WebUI 到 `webui/out`。
- 按需构建 `third_party/` source-built analyzers 到 `bin/tools/`。
- 创建 data 子目录。
- 不删除已有运行数据。

## 启停

```bash
./logagentctl.sh start
./logagentctl.sh status
./logagentctl.sh logs
./logagentctl.sh restart
./logagentctl.sh stop
```

启动后访问：

```text
http://127.0.0.1:50992/
```

## 安全

- `.env` 不提交。
- secret 只放环境变量。
- 导出包不包含 API Key、Cookie、Authorization header 或数据目录内容。
- SSH 私钥不由 LogAgent 保存，复用系统 ssh/agent。
