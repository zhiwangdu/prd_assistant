# dev_selftest 用法（MCP 客户端入口）

本文面向通过外部 MCP 客户端驱动 Linux LocalToolHub 跑开发自测闭环的场景。Server 只提供受控工具，
不运行自由 shell，不内置 Agent 循环。

## 1. 前置准备

1. 使用 `examples/server-dev-selftest.yaml` 作为配置模板。
2. 设置 `LOGAGENT_NATIVE_API_KEY`。
3. 将 `dev_selftest.enabled` 改为 `true`，并把示例中的 `/path/to/repo` 替换为仓库绝对路径。
4. 确认 server 进程有 Docker 访问权限。
5. 在 Windows 端完成代码修改、commit 和 push；ToolHub 只从配置 allowlist 中的 git repo/ref 同步源码。
6. 启动 server：

```bash
export LOGAGENT_NATIVE_API_KEY=<your-key>
sg docker -c 'cargo run -p logagent-server -- --config examples/server-dev-selftest.yaml'
```

远程使用时优先通过 SSH tunnel 访问本机监听地址：

```bash
ssh -L 50994:127.0.0.1:50994 user@linux-host -N
```

MCP HTTP endpoint：

```text
http://127.0.0.1:50994/api/mcp
Authorization: Bearer <your-key>
```

## 2. 工具顺序

| 步骤 | 工具 | 说明 |
|---|---|---|
| 1 | `logagent.dev_selftest.sync_workspace` | 从 allowlisted `gitRepo/gitRef` 同步源码，返回 `runId`；新 run clone，已有 run pull。 |
| 2 | `logagent.dev_selftest.build` | 运行配置好的 build profile，收集 `artifact_globs`。 |
| 3 | `logagent.dev_selftest.deploy` | 运行 `docker_cluster` profile，执行 compose up 和 health check。 |
| 4 | `logagent.dev_selftest.run_tests` | 使用 test suite 的 inline Docker target 执行测试；无 docker target 时走本地桩。 |
| 5 | `logagent.dev_selftest.report` | 聚合 `progress.json`、日志和结果，生成报告。 |
| 查询 | `logagent.runs.get` / `logagent.runs.result` | 轮询 queued run，不创建新的 ToolRun。 |

后续步骤都必须携带 `sync_workspace` 返回的同一个 `runId`。

`sync_workspace` 参数示例：

```json
{
  "name": "logagent.dev_selftest.sync_workspace",
  "arguments": {
    "label": "pr-123",
    "gitRepo": "https://github.com/openGemini/openGemini.git",
    "gitRef": "main"
  }
}
```

若需要在同一个 dev_selftest run 上重新同步 Windows 端刚 push 的提交，再带上已有 `runId` 调用同一工具。

## 3. Queued 调用

`tools/call` 支持 `runMode: "sync" | "queued"`，默认 `sync`。build 或 run_tests 较慢时使用
`queued`：

```json
{
  "name": "logagent.dev_selftest.run_tests",
  "arguments": {
    "runId": "devselftest_...",
    "testSuite": "opengemini_smoke",
    "runMode": "queued"
  }
}
```

然后轮询：

```json
{ "name": "logagent.runs.get", "arguments": { "runId": "task_..." } }
{ "name": "logagent.runs.result", "arguments": { "runId": "task_..." } }
```

`logagent.runs.get/result` 是 platform 工具，只读 `TaskStore`，不会污染 run history。

## 4. Inline Docker 测试

当前收敛后的测试派发只保留 inline Docker target：

```yaml
remote_execution:
  commands:
    opengemini_smoke:
      enabled: true
      argv: ["sh", "/tests/smoke.sh"]
      timeout_seconds: 180

dev_selftest:
  test_suites:
    opengemini_smoke:
      command: opengemini_smoke
      timeout_seconds: 180
      docker:
        image: "alpine:3.20"
        network: "host"
        volumes:
          - "/path/to/repo/deploy/devselftest/opengemini/tests:/tests:ro"
```

执行时 server 构造：

```text
docker run --rm --network host -v <tests>:/tests:ro \
  -e DEVSELFTEST_HOST=127.0.0.1 -e DEVSELFTEST_PORT=8086 \
  alpine:3.20 sh /tests/smoke.sh
```

系统 env（`DEVSELFTEST_HOST`、`DEVSELFTEST_PORT`、run 目录变量）最终优先，用户配置的 env 不能覆盖测试目标。

## 5. Run 工作区

```text
data/dev_selftest/runs/{runId}/
  source/
  artifacts/
  logs/
  tool_results/
  progress.json
  report.md
  report.json
```

每个步骤写入结构化 `result.json`，原始 stdout/stderr 写入 `logs/`。artifact 对外只暴露逻辑 ID，
不暴露任意本地路径。

## 6. 失败处理

- 任一步失败会把 dev_selftest run 标记为 `FAILED`，并在 `progress.json` 记录错误和 evidence。
- 后续仍可调用 `report`，报告会列出 `failedSteps`。
- `docker_cluster` health check 失败不会执行自动回滚；清理由操作者按 compose project 手动执行。

```bash
docker compose -p devselftest_<runId>_opengemini_cluster down
```

## 7. 已移除路径

当前用法不包含 SSH/SCP executor、托管 executor record、`suite.executor`、Huawei package sync、
GeminiDB create instance 或 Server 托管 skills。需要更复杂的自动化时，应放在外部 MCP client/runbook 中，
不能成为 Server 默认依赖。
