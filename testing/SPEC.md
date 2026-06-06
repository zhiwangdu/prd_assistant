# Testing Spec

## 目标

用低成本测试覆盖 MVP 闭环，优先验证真实上传、解压、grep 和 WEBUI 依赖接口。

## 当前状态

已有：

- `testing/fixtures/downloads/sample.log`
- Server 单测覆盖 `.tar` 和 `.tar.gz` 解压。
- Server 单测覆盖 Task Store 持久化/恢复、幂等 pipeline 和任务 API 状态码。
- 手工 smoke 验证过 WEBUI 上传、任务创建和 artifacts 查询。

## 必跑检查

```bash
cargo fmt --check
cargo check
cargo test
cd webui && npm run lint
cd webui && npm run typecheck
cd webui && npm run build
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
- `GET /api/tasks`
- `GET /api/tasks/:task_id`
- `GET /api/tasks/:task_id/artifacts`
- `GET /api/tasks/:task_id/analysis`（实现后）
- task message 和 action decision 的恢复/幂等（实现后）

## 验收标准

- 新增归档格式必须有解压测试。
- 新增 API 必须有 smoke 验证方式。
- 任务持久化变更必须覆盖损坏 JSON、启动恢复、终态保护和 artifacts 状态约束。
- WEBUI 修改至少跑 lint、typecheck 和 build。
- Analysis Agent 必须覆盖多轮、追问、审批、预算、重复动作和重启恢复。
- README 和 SPEC 在测试策略或 fixture 变更时同步更新。
