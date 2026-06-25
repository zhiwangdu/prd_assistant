# Testing

Testing 目录保存两模块 LocalToolHub 的 fixture、smoke 策略和验收说明。测试重点是
dev_selftest、日志分析、MCP、run/artifact 和安全边界。

## 测试重点

- Tool Catalog descriptor 完整性。
- Tool Runner 参数校验、timeout、stdout/stderr、result artifact。
- Built-in tools：日志包预处理、batch InfluxQL 分析、dev_selftest、platform runs 查询。
- Configured analyzers：pprof、Flux、InfluxQL、openGemini storage、InfluxDB storage。
- MCP tools/list 与 WebUI catalog 一致。
- Artifact path 安全和下载鉴权。
- Upload 解压路径安全。
- dev_selftest 禁止自由 shell，inline Docker target 必须通过 allowlist 校验。
- WebUI Tools-first 页面构建和类型正确。

## Fixture

```text
testing/fixtures/
  downloads/sample.log
  redis_timeout/
  influxql_slow_query/
  environment_disk_full/
```

fixture 应包含原始输入和 expected JSON。第三方真实工具 smoke 可以继续使用 `scripts/smoke-*.sh`。

## 常用检查

```bash
cargo fmt --check
cargo check
cargo test
cd webui && npm run lint
cd webui && npm run typecheck
cd webui && npm run build
git diff --check
```

## 手工 Smoke

目标 smoke：

```bash
cargo run -p logagent-server -- --config examples/local-toolhub.yaml
curl http://127.0.0.1:50992/health
curl -H "Authorization: Bearer $LOGAGENT_NATIVE_API_KEY" http://127.0.0.1:50992/api/tools
```

真实 analyzer smoke：

```bash
scripts/smoke-flux-query-analyzer.sh
scripts/smoke-influxql-analyzer.sh
scripts/smoke-opengemini-storage-analyzer.sh
scripts/smoke-influxdb-storage-analyzer.sh
```

自动测试不得依赖真实模型、真实 SSH 节点、真实密钥或外网。
