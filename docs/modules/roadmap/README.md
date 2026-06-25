# Roadmap

## Phase 1: Two-Module Cleanup

- 文档和示例只保留 dev_selftest + 日志分析。
- 删除或改写 Fetch、Metadata、Executor、Case、Skills、LLM/Agent 叙事。

## Phase 2: Dev Self-Test Hardening

- 固化 inline Docker test 派发。
- 清理 `max_input_chars`、旧 `TaskKind::RemoteCommandRun` 等兼容遗留。
- 增加 run workspace 清理和失败 artifact 可观测性。

## Phase 3: Log Analysis Quality

- 完善日志包预处理、batch InfluxQL 分析和 analyzer descriptor。
- 增加典型日志 fixture 与 smoke。

## Phase 4: Packaging

- Rust binary + webui/out + bin/tools + data。
- 本地安装脚本和 smoke。

## Phase 5: MCP Integration Polish

- MCP 文档、示例、资源输出和外部客户端接入保持稳定。
- 可选 workflow 只能作为外部 MCP client skill，不进入 Server 默认依赖。
