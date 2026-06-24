# dev_selftest 全量用法（以 Claude Code 为入口）

> 面向「坐在 Windows Claude Code 前、驱动 Linux LocalToolHub server 跑开发自测闭环」的全量操作手册。
> 所有步骤均通过 MCP 工具调用完成，server 不开自由 shell、不自带 Agent 循环。

---

## 1. 是什么

`logagent.dev_selftest.*` 是一组内置 catalog 工具，让远程 MCP 客户端（Claude Code）驱动 Linux
server 完成 **sync → build → deploy → run_tests → report** 开发自测闭环。每次调用是一个 `ToolRun`
（共享 Tool Runner 执行边界），跨多次调用通过持久 run 工作区 `data/dev_selftest/runs/{runId}/` 串联。

```mermaid
flowchart LR
    subgraph Win["Windows（开发机）"]
        CC["Claude Code<br/>MCP 客户端"]
    end
    subgraph Lin["Linux（LocalToolHub server）"]
        SRV["Rust Server (Axum)<br/>POST /api/mcp"]
        TR["Tool Runner / Executor Runner<br/>唯一执行边界"]
        WS["run 工作区<br/>data/dev_selftest/runs/{runId}/"]
        DK["Docker 引擎"]
    end
    subgraph Cluster["openGemini 集群（docker 容器）"]
        META["3× ts-meta"]
        SS["3× ts-store + ts-sql"]
    end
    CC -- "tools/call (MCP)" --> SRV
    SRV --> TR
    TR --> WS
    TR -- "docker compose up" --> DK
    DK --> META
    DK --> SS
    TR -- "docker run --rm (测试容器)" --> DK
```

---

## 2. 前置准备

```mermaid
flowchart TD
    A["1. server 配置<br/>examples/server-dev-selftest.yaml<br/>dev_selftest.enabled: true"] --> B["2. 环境变量<br/>LOGAGENT_NATIVE_API_KEY=...<br/>（内网可选 OG_BASE_IMAGE / GOPROXY / ...）"]
    B --> C["3. docker 访问<br/>server 进程在 docker 组，或 sg docker -c '...'"]
    C --> D["4. 起 server<br/>cargo run -p logagent-server -- --config ..."]
    D --> E["5. 建立 SSH 隧道（Windows→Linux）<br/>ssh -L 50994:127.0.0.1:50994 user@host"]
    E --> F["6. Claude Code 接 MCP"]
```

**server 启动**：

```bash
# Linux 上
export LOGAGENT_NATIVE_API_KEY=<your-key>
sg docker -c 'cargo run -p logagent-server -- --config examples/server-dev-selftest.yaml'
# 监听 127.0.0.1:50994（dev_selftest demo 配置）
```

**Claude Code 连接 MCP**（streamable-http，经 SSH 隧道）：

```bash
# Windows 上，先把远端口转到本地
ssh -L 50994:127.0.0.1:50994 user@linux-host -N
```

```json
// .mcp.json（或 Claude Code MCP 配置）
{
  "mcpServers": {
    "logagent": {
      "url": "http://127.0.0.1:50994/api/mcp",
      "headers": { "Authorization": "Bearer <your-key>" }
    }
  }
}
```

> 直连（不经隧道）需 TLS + API key + `mcp.allowed_origins`；localhost/隧道场景可留空。

```mermaid
flowchart TD
    subgraph Conn["连接方式"]
        T1["SSH 隧道（推荐）<br/>本地 localhost → 远 server<br/>allowed_origins 留空"]
        T2["stdio<br/>logagent-server mcp-serve<br/>（server 同机时）"]
        T3["直连 HTTPS<br/>需 TLS + API key + allowed_origins"]
    end
```

---

## 3. 五个工具 + run 模型

| 工具 | 关键参数 | 返回 |
|---|---|---|
| `logagent.dev_selftest.sync_workspace` | `label`，源三选一：`uploadId` / `gitRepo`+`gitRef` / 省略(空桩) | `{runId, status, sourceRef}` |
| `logagent.dev_selftest.build` | `{runId, buildProfile}` | `{status, exitCode, artifacts}` |
| `logagent.dev_selftest.deploy` | `{runId, profile}` | `{status, projectName, deployTarget}` |
| `logagent.dev_selftest.run_tests` | `{runId, testSuite}`（可选 `runMode`） | `{status, exitCode, executor, stdoutPath, stderrPath}` |
| `logagent.dev_selftest.report` | `{runId}` | `{status, reportPath, failedSteps, steps}` |
| `logagent.runs.get` | `{runId}`（platform 工具） | `{status, phase, resultAvailable}` |
| `logagent.runs.result` | `{runId}`（platform 工具） | 结构化结果 |

**关键约定**：`sync_workspace` 建 run 并返回 `runId`，**后续每次调用都带这个 `runId`**。MCP 参数可传
`{params:{...}}` 或顶层（`arguments` 即 `inputSchema`）；`runMode`/`uploadIds` 始终顶层。

```mermaid
flowchart TD
    S["sync_workspace<br/>建 run → 返回 runId"] --> B["build<br/>编译 + 收 artifact"]
    B --> D["deploy<br/>docker_cluster up + health"]
    D --> R["run_tests<br/>docker 派发 / 本地桩"]
    R --> P["report<br/>聚合 progress"]
    P -. "失败步仍执行后续" .-> R
```

---

## 4. 端到端流水线

### 4.1 总览

```mermaid
sequenceDiagram
    participant CC as Claude Code
    participant SRV as Server (MCP)
    participant TR as Tool Runner
    participant DK as Docker
    participant OG as openGemini 集群

    Note over CC,OG: 1) sync
    CC->>SRV: sync_workspace {gitRepo, gitRef:"main"}
    SRV->>TR: git clone → source/
    TR-->>CC: {runId, status:"OK"}

    Note over CC,OG: 2) build（queued）
    CC->>SRV: build {runId, buildProfile:"opengemini", runMode:"queued"}
    SRV-->>CC: {runId, status:"QUEUED"}
    SRV->>TR: build-opengemini.sh（go1.26+sonic + go build）
    CC->>SRV: runs.get {runId}（轮询）
    SRV-->>CC: {status:"SUCCEEDED"}
    CC->>SRV: runs.result {runId}
    SRV-->>CC: {artifacts:[ts-meta,ts-store,ts-sql]}

    Note over CC,OG: 3) deploy
    CC->>SRV: deploy {runId, profile:"opengemini_cluster"}
    SRV->>DK: docker compose -p devselftest_{runId}_... up -d
    DK->>OG: 6 容器启动（meta→store→sql 门控）
    SRV->>OG: health check: curl SHOW DATABASES（轮询）
    SRV-->>CC: {status:"OK", deployTarget:Docker}

    Note over CC,OG: 4) run_tests（docker 派发）
    CC->>SRV: run_tests {runId, testSuite:"opengemini_smoke", runMode:"queued"}
    SRV-->>CC: {runId, status:"QUEUED"}
    SRV->>DK: docker run --rm --network host alpine:3.20 sh /tests/smoke.sh
    DK->>OG: wget SHOW/CREATE/write/SELECT 127.0.0.1:8086
    CC->>SRV: runs.get {runId}（轮询）
    SRV-->>CC: {status:"SUCCEEDED"}

    Note over CC,OG: 5) report
    CC->>SRV: report {runId}
    SRV-->>CC: {status:"SUCCEEDED", reportPath, steps}
```

### 4.2 sync vs queued（runMode）

`tools/call` 接受可选 `runMode: "sync" | "queued"`（默认 `sync`）。短步骤同步；长步骤（build、run_tests）
用 `queued` 立即返回 `{runId, status:"QUEUED"}`，再用 platform 工具 `logagent.runs.get` 轮询，
`SUCCEEDED` 后 `logagent.runs.result` 取结构化结果。**一个 queued 调用一个 run，无子 run；runs.get/result
不建 ToolRun，不污染 run history。**

```mermaid
flowchart TD
    Start["tools/call"] --> Dec{runMode?}
    Dec -- "sync（默认）" --> Sync["内联执行<br/>阻塞返回结果"]
    Dec -- "queued" --> Q["立即返回<br/>{runId, status:QUEUED}"]
    Q --> Poll["runs.get {runId}<br/>轮询"]
    Poll --> St{status?}
    St -- "RUNNING/QUEUED" --> Poll
    St -- "SUCCEEDED" --> Res["runs.result {runId}<br/>取结构化结果"]
    St -- "FAILED" --> Fail["读 error / logs"]
```

| 步骤 | 推荐 runMode |
|---|---|
| sync_workspace | sync |
| build | queued（编译慢） |
| deploy | sync（compose up 快；health 已轮询）或 queued |
| run_tests | queued（测试可能慢） |
| report | sync |

---

## 5. 各步骤详解

### 5.1 sync_workspace（源码同步）

```mermaid
flowchart TD
    SW["sync_workspace"] --> Dec{源?}
    Dec -- "uploadId" --> UP["tarball 解到 source/<br/>extract_upload"]
    Dec -- "gitRepo + gitRef" --> GIT["git clone --depth 1 --branch<br/>须在 git.repos allowlist"]
    Dec -- "省略" --> EMPTY["空桩 source/"]
    UP --> Out["返回 runId + sourceRef"]
    GIT --> Out
    EMPTY --> Out
```

```
sync_workspace { label:"feat-x", gitRepo:"https://github.com/openGemini/openGemini.git", gitRef:"main" }
→ { runId:"devselftest_...", status:"OK", sourceRef:"git:...@main" }
```

### 5.2 build（真编译）

在 `source/{working_dir}` 跑配置式 `command`（首元素=二进制），`artifact_globs` 收集到 `artifacts/`，
写 `logs/build.{stdout,stderr}.txt`。openGemini demo 的 build 脚本先做 go1.26+sonic 兼容升级再 `go build`。

### 5.3 deploy — Path 1（docker 集群）

```mermaid
flowchart TD
    D["deploy {profile:opengemini_cluster}"] --> ENV["deploy_env 注入<br/>DEVSELFTEST_RUN_DIR/SOURCE_DIR/<br/>ARTIFACTS_DIR/PROJECT_NAME"]
    ENV --> UP["docker compose -p devselftest_{runId}_{profile}<br/>-f compose up -d"]
    UP --> GATE["entrypoint 顺序门控<br/>meta:8091 就绪 → store:8400 → sql"]
    GATE --> HC["health check 轮询<br/>curl SHOW DATABASES"]
    HC --> Ok{ok?}
    Ok -- "yes" --> Rec["记录 deployTarget:Docker<br/>{cluster, exposed_port:8086}"]
    Ok -- "no（超时）" --> Fail["FAILED（P1 不回滚）"]
    Rec --> Done["返回 {status:OK, projectName}"]
```

> 关键约束（openGemini）：容器需**静态 IP**（raft 用 `rpc-bind-address` 串作 Server ID，主机名会不选主）、
> `ubuntu:24.04`（22.04 libstdc++ 过旧）、顺序门控（`depends_on` 仅排序，entrypoint 须等就绪）。

### 5.4 run_tests（P2 双模式）

```mermaid
flowchart TD
    RT["run_tests {testSuite}"] --> Dec{suite.docker?}
    Dec -- "存在（docker 模式）" --> DV["经 executor docker runner 派发"]
    Dec -- "无（stub 模式）" --> ST["P1 本地桩<br/>server 主机跑 suite.argv"]
    DV --> Argv["argv/timeout ← suite.command 模板<br/>（无则 suite.argv）"]
    Argv --> Vol["volume ${DEVSELFTEST_*} 插值<br/>断言 host 绝对"]
    Vol --> Env["env 合并：<br/>user=suite.env ∪ docker.env<br/>system=DEVSELFTEST_HOST/PORT+run目录（最终优先）"]
    Env --> Run["docker run --rm --network host<br/>[-v tests:/tests:ro] [-e ...]<br/>alpine:3.20 sh /tests/smoke.sh"]
    Run --> Log["写 logs/tests.{stdout,stderr}.txt<br/>result.json 带 executor 字段"]
    ST --> Log
```

docker 模式下，测试容器用 `--network host` 经宿主暴露端口 `127.0.0.1:8086` 访问 ts-sql。系统 env
**最终优先**——用户 `env` 不能把测试悄悄打到错误目标。`smoke.sh` 默认用 busybox `wget`（alpine 预装），
无 apt/外网依赖；curl 优先。

### 5.5 executor runner（P2 抽通）

`run_executor_command` 支持 `ExecutorTarget::{Ssh, Docker}`，dev_selftest 复用 Docker 分支。SSH 分支
行为不变（保留 `TimedOut` 语义）。runner 不检查 `remote_execution.enabled`，故 SSH 关闭时仍可用 Docker 分支。

```mermaid
flowchart LR
    In["ExecutorRunInput<br/>{target, argv, extra_env, launcher, ...}"] --> Br{target?}
    Br -- "Ssh" --> SSH["ssh -o BatchMode=yes ...<br/>user@host <argv><br/>extra_env 忽略"]
    Br -- "Docker" --> DK["docker run --rm --network ...<br/>[--env] [--volume] <image> <argv><br/>extra_env 覆盖 target.env"]
    SSH --> Out["ExecutorOutcome<br/>status:Ok/Failed/TimedOut/SpawnFailed"]
    DK --> Out
```

### 5.6 report

聚合 `progress.json` 步骤账本 + 证据 → `report.md`（表格：step/status/durationMs/error）+
`report.json`。总体 `SUCCEEDED`（无失败步）或 `FAILED`（含 `failedSteps`）。

---

## 6. Run 工作区与结果读取

```mermaid
flowchart TD
    Run["data/dev_selftest/runs/{runId}/"] --> A["source/（同步的源码）"]
    Run --> B["artifacts/（build 产物）"]
    Run --> C["logs/build.*.txt<br/>logs/deploy.*.txt<br/>logs/tests.*.txt"]
    Run --> D["tool_results/{actionId}/result.json<br/>（每步结构化结果）"]
    Run --> E["progress.json（步骤账本）"]
    Run --> F["report.md / report.json"]
```

每次 `logagent.dev_selftest.*` 调用写一个 `result.json`（含 `status` OK/FAILED/SUCCEEDED、`runId`、
`durationMs`、`error`、步骤特定字段）。`logs/*.txt` 是原始 stdout/stderr。`report.md`/`report.json` 是
最终聚合。**artifact 路径对外是逻辑 ID，非原始本地路径**；下载带 `Authorization` 头。

---

## 7. 失败处理

```mermaid
flowchart TD
    Step["某步失败"] --> Mark["标记 run FAILED<br/>progress.json 记 error"]
    Mark --> Cont["后续步仍执行<br/>（仍可调 report）"]
    Cont --> Rep["report 总体 FAILED<br/>列 failedSteps"]
    Mark -. "P1 deploy health 失败" .-> NoRoll["不回滚<br/>（回滚在 P2 SSH 路径）"]
```

读失败原因：该步 `result.json` 的 `error` + `logs/*.stderr.txt`；`report.md` 的 `failedSteps`。

---

## 8. 三条部署路径

原始设计三条部署路径，当前仅 Path 1 实现：

```mermaid
flowchart TD
    Build["build 产物"] --> P1
    Build -. "deferred" .-> P2
    Build -. "deferred (P3)" .-> P3

    P1["Path 1: docker 本地集群<br/>docker_cluster profile<br/>✅ 已实现 + 已对真实集群跑通"]
    P2["Path 2: SSH 二进制替换<br/>ssh_binary_replace + 受控 SCP<br/>备份/重启/health/回滚<br/>⏳ deferred"]
    P3["Path 3: 打包 + API 建实例<br/>OBS package_sync +<br/>geminidb.create_instance + 轮询<br/>⏳ deferred (P3)"]

    P1 --> DT1["DevSelftestDeployTarget::Docker"]
    P2 --> DT2["::Ssh { executor_id }（类型已预留）"]
    P3 --> DT3["::Instance { instance_id, endpoint }（类型已预留）"]
```

---

## 9. 配置与内网

```mermaid
flowchart LR
    subgraph Cfg["dev_selftest 配置（examples/server-dev-selftest.yaml）"]
        G["git: binary + repos allowlist"]
        Bp["builds: command/working_dir/artifact_globs"]
        Dk["docker: binary + clusters<br/>{compose_file, exposed_port, health_check}"]
        Ts["test_suites: argv/command/timeout/env/docker"]
    end
    subgraph Env["内网覆盖（server 进程 env，无代码改动）"]
        E1["OG_BASE_IMAGE（集群镜像）"]
        E2["GOPROXY/GOSUMDB（Go 源）"]
        E3["git.repos（openGemini 源镜像）"]
        E4["DEVSELFTEST_TEST_IMAGE（测试镜像）"]
    end
```

**安全校验**（`enabled:true` 时）：所有 build/docker/test 命令、`docker.binary`、`compose_file`、git 仓库+ref
必须绝对路径且 allowlist；tool 参数只选 profile id + `runId`，无自由 shell。`DevSelftestTestDocker` 校验：
image 不以 `-` 开头、network `host`|安全标识符、workdir 绝对无 `..`、volume
`host:absolute|${DEVSELFTEST_*}:container:absolute[:ro|rw]`、env 键 `^[A-Z_][A-Z0-9_]*$`。`command` 与非空
`argv` 互斥；`command` 须配 `docker` 块。

---

## 10. 完整 Claude Code 驱动示例

```mermaid
sequenceDiagram
    participant U as 用户
    participant CC as Claude Code
    participant S as Server

    U->>CC: "跑一遍 openGemini 自测"
    CC->>S: sync_workspace {gitRepo, gitRef:"main"}
    S-->>CC: runId=devselftest_abc
    CC->>S: build {runId, buildProfile:"opengemini", runMode:"queued"}
    S-->>CC: {runId, status:"QUEUED"}
    loop 轮询
        CC->>S: runs.get {runId}
        S-->>CC: RUNNING
    end
    CC->>S: runs.get {runId}
    S-->>CC: SUCCEEDED
    CC->>S: runs.result {runId}
    S-->>CC: {artifacts:[...]}
    CC->>S: deploy {runId, profile:"opengemini_cluster"}
    S-->>CC: {status:"OK", deployTarget:Docker}
    CC->>S: run_tests {runId, testSuite:"opengemini_smoke", runMode:"queued"}
    S-->>CC: {runId, status:"QUEUED"}
    loop 轮询
        CC->>S: runs.get {runId}
        S-->>CC: RUNNING
    end
    CC->>S: runs.result {runId}
    S-->>CC: {status:"OK", executor:{kind:"docker",...}}
    CC->>S: report {runId}
    S-->>CC: {status:"SUCCEEDED", reportPath, steps}
    CC->>U: 报告：5 步全 OK，report.md 已生成
```

---

## 11. 清理

dev_selftest 无 `docker_down` 工具。多次 run 按 project name `devselftest_{runId}_{profile}` 隔离，
手动清理：

```bash
docker compose -p devselftest_<runId>_opengemini_cluster down
```

---

## 12. 状态总览

```mermaid
flowchart TD
    P0["P0 MCP 传输 + 异步 run<br/>✅ 已合入"]
    P1["P1 docker 闭环（桩 run_tests）<br/>✅ 已合入 + 真实集群跑通"]
    P2["P2 docker 切片<br/>executor runner SSH/Docker + 内联 docker test<br/>✅ 已合入（0019f10）"]
    P2d["P2 deferred<br/>参数化模板 / Docker executor 纳管 / ssh_binary_replace"]
    P3["P3 deferred<br/>package_sync core + create_instance"]

    P0 --> P1 --> P2 --> P2d
    P2d -.-> P3
```

- ✅ Path 1（docker）：deploy + run_tests(docker) 已实现；P1 端到端已验证，P2 docker 派发经单测/集成测试验证（真实集群端到端待执行）。
- ⏳ Path 2（SSH 二进制替换）、Path 3（打包建实例）deferred。
- ⏳ 参数化 executor 命令模板（`{var}`+Schema）、Docker executor 纳管（record+CRUD+run history）、`docker_down` 工具、WebUI 视图、composite `run` 均 deferred。
