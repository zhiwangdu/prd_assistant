# dev_selftest 用法（MCP 客户端入口）

本文面向通过外部 MCP 客户端驱动 Linux LocalToolHub 跑开发自测闭环的场景。Server 只提供受控 MCP step tools，
不运行自由 shell，不内置 Agent 循环，也不托管 workflow。完整编排应安装或参考 `skills/dev-selftest-pipeline/`。

## 1. 前置准备

1. 使用 `examples/server-dev-selftest.yaml` 作为配置模板。
2. 设置 `LOGAGENT_NATIVE_API_KEY`。
3. 将 `dev_selftest.enabled` 改为 `true`，并把示例中的 `/path/to/repo` 替换为仓库绝对路径。
4. 确认 server 进程有 Docker 访问权限。
5. 在 Windows 端完成代码修改、commit 和 push；ToolHub 只从配置 allowlist 中的 git repo/ref 同步源码。
6. 客户端 workflow 开始前读取 MCP resource `logagent://dev_selftest/config`，不要 SSH 到 Server 或扫描本机配置来猜 repo/ref/profile。
7. 启动 server：

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

## 2. 配置发现与 allowlist 热更新

先读取当前 dev_selftest 摘要：

```json
{
  "method": "resources/read",
  "params": { "uri": "logagent://dev_selftest/config" }
}
```

返回内容包含 `gitRepos`、`defaultGitRepo`、`defaultGitRef`、`buildProfiles`、`dockerProfiles`、`testSuites`
以及 `dockerProfileDetails`、`buildProfileDetails` / `testSuiteDetails`（host/docker、image、timeout）。
客户端只能使用这些值调用 `sync_workspace` / `build` / `deploy` / `run_tests` / `cleanup` / `diagnose`。

如果用户需要的新分支不在 allowlist 中，先停止 workflow 并询问用户是否允许更新。用户明确同意后再调用：

```json
{
  "name": "logagent.dev_selftest.allowlist.update",
  "arguments": {
    "repoUrl": "ssh://git@github.com/zhiwangdu/openGemini.git",
    "gitRef": "feature/dev-selftest",
    "setDefault": true,
    "confirmedUserConsent": true,
    "reason": "User approved dev_selftest for this branch"
  }
}
```

Server 会校验 URL/ref、执行 `git ls-remote`、写回 `--config` YAML，再更新内存 allowlist。成功后重新读取
`logagent://dev_selftest/config`，再继续 `sync_workspace`。

如果需要新增或调整 Docker-backed build/test profile，先停止 workflow 并询问用户是否允许更新。用户明确同意后再调用：

```json
{
  "name": "logagent.dev_selftest.profiles.upsert",
  "arguments": {
    "kind": "build",
    "id": "opengemini_ci",
    "image": "registry.local/localtoolhub/opengemini-builder:latest",
    "argv": ["/usr/local/bin/build-selftest"],
    "timeoutSeconds": 1800,
    "network": "host",
    "workdir": "/workspace/source",
    "artifactGlobs": ["build/ts-meta", "build/ts-store", "build/ts-sql"],
    "confirmedUserConsent": true,
    "reason": "User approved Docker build profile for this branch"
  }
}
```

该 tool 只写入受控 Docker profile，不启动 build/test；后续执行仍只传 profile id。

## 3. 工具顺序

| 步骤 | 工具 | 说明 |
|---|---|---|
| 1 | `logagent.dev_selftest.sync_workspace` | 从 allowlisted `gitRepo/gitRef` 同步源码，返回 `runId`；新 run clone，已有 run pull。 |
| 2 | `logagent.dev_selftest.build` | 运行配置好的 build profile；旧 host command 和 Docker build profile 都支持，收集 `artifact_globs`。 |
| 3 | `logagent.dev_selftest.deploy` | 运行 `docker_cluster` profile，执行 compose up 和 health check。 |
| 4 | `logagent.dev_selftest.run_tests` | 使用 test suite 的 inline Docker target 执行测试；无 docker target 时走本地桩。 |
| 5 | `logagent.dev_selftest.report` | 聚合 `progress.json`、日志和结果，生成报告。 |
| 6 | `logagent.dev_selftest.cleanup` | 可选：report 后对本次 run 的配置化 compose project 执行 `docker compose down`，保留 run 证据。 |
| 诊断 | `logagent.dev_selftest.diagnose` | 失败后读取 bounded evidence，并执行配置化 Docker 只读 probe，返回原因分类和下一步建议。 |
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

## 4. Queued 调用

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

## 5. 失败诊断

任一远端 step 返回 `status:"FAILED"` 后，先调用 `diagnose`，不要 SSH 到 Server 上 cat 日志：

```json
{
  "name": "logagent.dev_selftest.diagnose",
  "arguments": {
    "runId": "devselftest_...",
    "taskRunId": "task_...",
    "includeDockerProbes": true
  }
}
```

该工具只读取本 run 的 `progress.json`、`report.json` 和 `logs/*.txt`，并基于配置化 Docker profile 执行
`docker compose ps/logs`、`docker ps` 等只读 probe。输出包含 `category`（如 `port_conflict`、
`stale_compose_project`、`health_check_failed`、`container_crash`、`build_failed`、`test_failed`）、
bounded 脱敏证据和建议。它不会执行 cleanup、restart、rm 或任意 shell。

## 6. 环境清理

`cleanup` 是显式可选步骤，推荐在 `report` 后调用；失败环境默认保留，便于排查。

```json
{
  "name": "logagent.dev_selftest.cleanup",
  "arguments": {
    "runId": "devselftest_..."
  }
}
```

Server 会从 run 的 Docker deploy target 推导 profile 和 project name，并执行：

```text
docker compose -p devselftest_<runId>_<profile> -f <configured-compose-file> down
```

该步骤不加 `--volumes`，不删除 `source/`、`artifacts/`、`logs/`、`progress.json` 或 `report.*`。
如果 run 尚未 deploy，可显式传 `profile`。

## 7. Inline Docker 测试

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

## 8. Run 工作区

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

## 9. 失败处理

- 核心步骤失败会把 dev_selftest run 标记为 `FAILED`，并在 `progress.json` 记录错误和 evidence。
- 后续仍可调用 `report`，报告会列出 `failedSteps`。
- 失败后优先调用 `logagent.dev_selftest.diagnose` 获取分类、证据片段和建议，再决定修代码、保留现场或 cleanup。
- `docker_cluster` health check 失败不会执行自动回滚；失败环境默认保留用于排查。
- 需要释放环境时，在 `report` 后调用 `logagent.dev_selftest.cleanup`。cleanup 失败只记录清理步骤 evidence，不改变 report 的核心自测结论。

## 10. 已移除路径

当前用法不包含 SSH/SCP executor、托管 executor record、`suite.executor`、Huawei package sync、
GeminiDB create instance、Server 托管 skills 或 Server 侧 workflow API。需要更复杂的自动化时，应放在外部 MCP client/skill 中，
不能成为 Server 默认依赖。

也不要通过 SSH 读取 Server 配置、扫描本机 `prd_assistant` 配置，或为了匹配旧 allowlist 强推到已有分支；除非用户明确要求强推。
