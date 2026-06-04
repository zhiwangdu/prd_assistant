# Testing Spec

## 目标

用低成本测试覆盖 MVP 闭环，优先验证真实上传、解压、grep 和 WEBUI 依赖接口。

## 当前状态

已有：

- `testing/fixtures/downloads/sample.log`
- Server 单测覆盖 `.tar` 和 `.tar.gz` 解压。
- 手工 smoke 验证过 WEBUI 上传、任务创建和 artifacts 查询。

## 必跑检查

```bash
cargo fmt --check
cargo check
cargo test
node --check webui/app.js
```

## 集成验证

Server：

```bash
export LOGAGENT_NATIVE_API_KEY=dev-token
cargo run -p logagent-server -- --config examples/server-test.yaml
```

验证：

- `GET /health`
- `GET /`
- `POST /api/uploads`
- `POST /api/tasks`
- `GET /api/tasks/:task_id/artifacts`

## 验收标准

- 新增归档格式必须有解压测试。
- 新增 API 必须有 smoke 验证方式。
- WEBUI JS 修改至少跑 `node --check`。
- README 和 SPEC 在测试策略或 fixture 变更时同步更新。
