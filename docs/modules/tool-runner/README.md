# Tool Runner

Tool Runner 是 LocalToolHub 的核心。所有内置工具、配置工具、source-built analyzer、Fetch、Executor 和 Code Evidence 都应以统一 run/artifact 模型呈现。

## 职责

- 维护 Tool Catalog。
- 校验参数和输入文件。
- 执行工具或内置 backend。
- 保存 stdout/stderr/result/support artifacts。
- 给 WebUI 和 MCP 提供同一执行入口。

## 初始工具

- `logagent.preprocess_log_package`
- `logagent.fetch`
- `pprof_analyzer`
- `flux_query_analyzer`
- `influxql_analyzer`
- `opengemini-storage-analyzer`
- `influxdb_storage_analyzer`
