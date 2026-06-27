# Tool Runner

Tool Runner 是 LocalToolHub 的核心。日志 analyzer、日志包预处理、batch InfluxQL 分析、
dev_selftest 和 platform run 查询都以统一 tool/run/artifact 模型呈现。

## 职责

- 维护 Tool Catalog。
- 校验参数和输入文件。
- 执行工具或内置 backend。
- 保存 stdout/stderr/result/support artifacts。
- 给 WebUI 和 MCP 提供同一执行入口。

## 初始工具

- `logagent.preprocess_log_package`
- `logagent.batch_influxql_analysis`
- `logagent.dev_selftest.sync_workspace`
- `logagent.dev_selftest.build`
- `logagent.dev_selftest.deploy`
- `logagent.dev_selftest.run_tests`
- `logagent.dev_selftest.report`
- `logagent.dev_selftest.cleanup`
- `logagent.dev_selftest.diagnose`
- `logagent.runs.get`
- `logagent.runs.result`
- `pprof_analyzer`
- `flux_query_analyzer`
- `influxql_analyzer`
- `opengemini-storage-analyzer`
- `influxdb_storage_analyzer`

`logagent.dev_selftest.run_tests` 支持受限 `testParams` string map；Tool Runner/MCP schema
暴露该对象，执行时由 dev_selftest 后端转换为 `DEVSELFTEST_PARAM_*` 环境变量。
