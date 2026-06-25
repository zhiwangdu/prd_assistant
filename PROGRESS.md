# Development Progress

Last updated: 2026-06-25

Historical main-branch progress was archived to
`docs/archive/PROGRESS-history-main-2026-06-22.md`.

## Current Branch

- Branch: `converge/two-modules`（从 `rewrite/local-toolhub-rust` 切出）
- Base: `origin/main`
- Product direction: 收敛为两模块 —— dev_selftest（Linux 跨机自测）+ 日志分析（上传日志即分析）
- Runtime target: Rust single binary + WebUI static files + local tools dir + local data dir

## 2026-06-25 openGemini 配置一键探测生成脚本

- 新增 `deploy/probe-opengemini-config.sh`：探测 `LOGAGENT_APP_DIR` / `LOGAGENT_SRC_DIR`（当前环境优先，非交互场景可直接解析
  `~/.bashrc` export 行）、`git`/`docker`/`curl`、Docker daemon/compose、openGemini demo artifact、`8086` 端口和
  allowlisted git repo/ref。
- 脚本默认生成 `$LOGAGENT_APP_DIR/deploy/server-opengemini.yaml`，使用绝对路径写入 `dev_selftest` 的 git/build/docker/test
  allowlist；默认 repo/ref 为本次已验证的 `ssh://git@github.com/zhiwangdu/openGemini.git` +
  `devselftest/go126-sonic-latest-20260625-233438`，支持 `--repo-url` / `--git-ref` / `--output` / `--data-dir` 覆盖。
- 同步更新 `deploy/README.md`、`deploy/devselftest/opengemini/README.md` 和 dev_selftest 模块文档。
- 验证：`bash -n deploy/probe-opengemini-config.sh`；本机临时 `LOGAGENT_APP_DIR`/`LOGAGENT_SRC_DIR` 生成配置；SSH 到
  `duzhiwang@192.168.31.128` 后以非交互方式运行脚本，成功解析远端 `.bashrc` 并生成
  `/home/duzhiwang/workspace/data/prd_assistant/deploy/server-opengemini.yaml`。

## 2026-06-25 openGemini GitHub 分支真实流程验证

- 按用户指定从 `git@github.com:zhiwangdu/openGemini.git` 拉源码，新建并推送测试分支
  `devselftest/go126-sonic-latest-20260625-233438`（commit `add5057`），改动为 `go 1.26` 和
  `github.com/bytedance/sonic v1.15.2`（latest）及相应 `go.sum`。
- 本地完整 server 使用临时配置 `/tmp/localtoolhub-og-gh.Hd3pVl/server-github.yaml` 启动，`dev_selftest.git.repos`
  allowlist 指向 `ssh://git@github.com/zhiwangdu/openGemini.git`，ref allowlist 指向上述测试分支；MCP
  `initialize`、`tools/list` 正常，dev_selftest tools 暴露 `runMode:"queued"`。
- 真实 workflow 已执行到 `sync_workspace`、`build`、`deploy`：`sync_workspace` 从 GitHub SSH 分支 clone 成功；
  `build` 产出 `ts-meta`、`ts-store`、`ts-sql`；`deploy` 使用仓库 `deploy/devselftest/opengemini/docker-compose.yml`
  拉起 3 meta + 3(sql+store) openGemini 集群并通过 `SHOW DATABASES` health check。
- 第一轮 `run_tests` 暴露 smoke 脚本确定性问题：写点后立即 SELECT 偶发只能拿到空 result，手动等待 2 秒后同一集群可查询到写入点。
  修复 `deploy/devselftest/opengemini/tests/smoke.sh`，SELECT 改为有上限短轮询；同步更新 openGemini demo 文档和 dev_selftest 模块文档。
- 修复后停掉旧 compose 项目并从同一 GitHub 测试分支重新跑全新 workflow：`devselftest_1782402117507_8`。
  `sync_workspace` (`task_1782402117506_7`) 从 GitHub SSH 分支同步成功；`build` (`task_1782402127609_9`) 产出三份二进制；
  `deploy` (`task_1782402147721_10`) 使用 example compose 成功拉起集群；`run_tests` (`task_1782402157823_11`) 通过 alpine
  inline Docker smoke；`report` (`task_1782402167947_12`) 返回 `SUCCEEDED` 且 `failedSteps: []`。
- 验证命令：`sh -n deploy/devselftest/opengemini/tests/smoke.sh`、`git diff --check`、真实 MCP queued
  openGemini workflow fresh run。

## 2026-06-25 破坏性边界收敛：Server 只供 MCP，dev_selftest workflow 进本地 skill

目标：消除 dev_selftest workflow 的双重真相。Server 不再被描述为 workflow 编排器，也不恢复 skill registry /
server 托管 skill / runbook 兼容入口；它只暴露 MCP tools/resources 和受控执行边界。Claude Code 本地负责
修改代码、commit、push，再通过安装的 skill 编排 Linux ToolHub 上的 `sync_workspace -> build -> deploy -> run_tests -> report`。

- 新增顶层 `skills/README.md`、`skills/SPEC.md`，定义用户安装型 Claude Code skill 分发目录：仓库分发、客户端安装、
  Server 不扫描、不加载、不提供下载/安装 API，新 skill 不使用 legacy `logagent.json` manifest。
- 新增 `skills/dev-selftest-pipeline/SKILL.md` 与 `references/workflow.md`：记录 Claude Code 本地改代码并
  commit/push 的前置动作、MCP step 顺序、queued 调用轮询、`devselftest_*` 工作区 id 与 `task_*` queued run id
  的区别、失败处理和已移除路径。
- 删除旧 `docs/runbooks/dev-selftest-pipeline/`（含 legacy `logagent.json`），`docs/runbooks/README.md` 改为仅保留
  日志分析 runbook 作者参考；新增 workflow 必须放到 `skills/`。
- 根 `README.md`/`SPEC.md`、`server/README.md`/`server/SPEC.md`、`docs/modules/dev-selftest/*`、`docs/modules/README.md`、
  `docs/modules/roadmap/README.md`、`CLAUDE.md` 同步改为 “Server 提供 dev_selftest MCP step tools，workflow 由客户端 skill 编排”。

验证：本次为文档/skill-only 改动，`git diff --check`、`git diff --cached --check` 通过；未触碰 Rust/WebUI 代码，不跑 cargo/npm。

后续真实集成验证发现：Claude Code 通过 MCP 只能看到 `tools/list.inputSchema`，虽然 Server `tools/call`
已支持 `runMode:"queued"`，但 schema 未公开该字段，导致本地 skill 要求的 queued 轮询路径无法被 Claude 正确调用。
修复：`server/src/mcp_server.rs` 为所有 runnable catalog tool 的 MCP inputSchema 自动追加可选
`runMode: "sync"|"queued"`，platform tools（`logagent.runs.get/result`）不追加；新增 MCP schema 回归测试。
真实集成验证：
- 临时目录 `/tmp/localtoolhub-e2e.RCC8bl` 创建 bare git remote、客户端 clone、server config/data、轻量 Docker compose fixture 和本地测试镜像 `localtoolhub-fixture-http:e2e`。
- 完整 server 通过 `cargo run -p logagent-server -- --config /tmp/localtoolhub-e2e.RCC8bl/server.yaml` 启动，HTTP `/health`、MCP `initialize`、`tools/list` 冒烟通过。
- 在临时客户端 clone 中安装 `.claude/skills/dev-selftest-pipeline` 和 project `.mcp.json`；用真实 `claude` CLI 2.1.176 通过 `--mcp-config ... --strict-mcp-config` 连接本地 MCP。
- 第一轮真实 Claude Code 跑通同步路径：本地修改 `src/app.txt`、commit/push、MCP `sync_workspace -> build -> deploy -> run_tests -> report`，报告 `SUCCEEDED`；同时暴露 `runMode` 未公开问题。
- 修复后第二轮真实 Claude Code queued 路径跑通：`devselftest_1782396515045_2`；build `task_1782396522917_3`、deploy `task_1782396536307_4`、run_tests `task_1782396553952_5` 均经 `logagent.runs.get` 轮询到 `SUCCEEDED` 并用 `logagent.runs.result` 读取结构化结果；最终 `report` 为 `SUCCEEDED` 且 `failedSteps: []`。
- 复测结束后停止临时 server、git daemon 和 Docker compose 项目；证据目录保留在 `/tmp/localtoolhub-e2e.RCC8bl`。

验证命令：`cargo fmt --check`、`cargo check`、`cargo test -p logagent-server`（77 passed）、真实 Claude Code queued workflow 复测、`git diff --check`。

## 2026-06-25 dev_selftest sync_workspace 收敛为 git-only

目标：简化源码同步方案。Windows 端 Claude Code 负责本地修改、commit、push；ToolHub 不再接收源码 tarball
或创建空 source stub，只从配置 allowlist 中的 git repo/ref 同步源码。

- `logagent.dev_selftest.sync_workspace` 参数收敛为必填 `gitRepo` + `gitRef`（`runId` 仍可选，用于复用已有 dev_selftest run；`label` 仍可选）。`uploadId` 和无源码参数会在参数校验阶段拒绝。
- 新 run 的 `source/` 为空 git workspace 时执行 `git clone --depth 1 --branch <gitRef> <gitRepo> source/`；已有 run 且 `source/.git` 存在时执行 `remote set-url`、`fetch --prune`、`checkout`、`pull --ff-only origin <gitRef>`，对应“Windows push 后 ToolHub pull”。
- 为避免误覆盖旧非 git source，若复用 run 时 `source/` 非空但不是 git checkout，`sync_workspace` 返回失败并要求创建新 `runId`。
- 更新 Tool descriptor / MCP 单测 / dev_selftest 闭环测试：测试使用 fake git，不依赖外网；闭环测试覆盖同一 `runId` 二次 sync 进入 pull 路径。
- 文档同步：根 README/SPEC、`server/SPEC.md`、dev_selftest 模块文档、runbook、openGemini demo 和 `examples/server-dev-selftest.yaml` 均改为 git-only 流程。

验证计划：`cargo fmt --check`、`cargo check`、`cargo test -p logagent-server`、示例 YAML 解析、`git diff --check`。

## 2026-06-25 文档收敛清理：只保留 dev_selftest + 日志分析

目标：先整理所有文档，把两模块收敛后不相关的 Fetch / Metadata / Case / Skills / SSH-SCP Executor /
纳管 executor / GeminiDB / Huawei package sync / LLM Agent 叙事从当前文档中清掉，避免产品定位继续发散。

- `docs/modules/config|interfaces|security|roadmap|tool-runner|deployment`：重写为当前配置、接口、安全边界、roadmap、Tool Runner 和部署验收；当前 API 只记录 uploads/tools/runs/artifacts/MCP，明确已移除路径不可作为支持接口。
- `docs/modules/dev-selftest/README.md`、`usage-claude-code.md`、`docs/runbooks/dev-selftest-pipeline/*`：收敛为当前 Docker 闭环和 inline Docker test 模式，删除 managed executor、`suite.executor`、SSH 二进制替换、Huawei/GeminiDB P3 路线。
- `testing/README.md`、`testing/SPEC.md`：测试重点改成 dev_selftest、日志分析、MCP、upload/artifact 路径安全和 fake docker；不再要求 Metadata/Fetch/Executor 测试项。
- `deploy/README.md`、`docs/modules/deployment/SPEC.md`、`deploy/devselftest/opengemini/README.md`：部署形态改成 `logagent-server` + analyzer tools + data；说明 `remote_execution` 只剩 dev_selftest command templates，不保存 SSH 私钥也不提供 SSH/SCP executor。
- `examples/server-test.yaml`、`examples/server-dev-selftest.yaml`、`examples/logagent.yaml`、`deploy/logagent.example.yaml`：示例注释同步，`server-test.yaml` 删除旧 SSH smoke executor 示例。
- 删除 `docs/architecture_review.md`，该旧架构评审已不服务当前两模块定位；历史进展仍保留在 `docs/archive/` 和本文件旧条目中。
- `server/README.md`、`server/SPEC.md`：去掉当前状态模型中的审批等待态和 SSH 回滚叙事，统一称 inline Docker runner。

验证：使用 `rg` 扫描旧模块关键词，剩余命中仅作为“已移除/不要使用”的边界说明或历史记录；后续需跑 `git diff --check` 后提交。

## 2026-06-24 收敛为两模块：dev_selftest + 日志分析

动机：原 server 挂了 9 类工具（fetch / gemini_db / huawei_package_sync / metadata / cases / system_context / skills / executors + dev_selftest/日志分析），功能杂、维护面大，且大部分在「纯本地个人用」场景被本地 skill 秒杀。收敛为 server 唯二不可被本地 skill 替代的两模块：dev_selftest（Windows Claude Code → Linux docker/build/test 跨机远程执行 + run history）与日志分析（一组编译好的 Linux analyzer + 预处理，MCP 连上上传日志即用）。三个收敛决策（问答确认）：丢纳管 executor、砍华为云工具、skills 改本地 Claude Code skill。

Step 1 — 砍 leaf 模块（fetch/gemini_db/huawei_package_sync/metadata/cases/system_context/skills）：
- 删 services/{fetch,gemini_db,huawei_package_sync,metadata,skill_registry}、stores/{fetch_store,case_store,case_import_store,memory_store,system_context_store}、http/{fetch,cases,system_context,metadata,skills,exports,mcp_readonly}。
- `services/tools.rs`：descriptors/get_descriptor/validate/run 删对应分支 + 常量；`domain/contracts.rs` 去掉 `services::metadata::TaskMetadataContext` 依赖（`from_record` 不再取 metadata 参数）。`mcp_server.rs` resources 收敛为 `logagent://runs/recent` + `logagent://tools/catalog`。
- `support/config.rs`：删 SkillSettings/FetchSettings/HuaweiCloud*/GeminiDb* + raw config + resolver + 默认值 + 测试；AppConfig/ConfigFile/load_config/prepare_dirs/StorageSettings 同步瘦身。`app.rs` AppState 13→7 字段。
- 存活 catalog：configured analyzer(5) + preprocess_log_package + batch_influxql_analysis + dev_selftest(5) + platform runs.get/result(2)。

Step 2 — 砍 executors/纳管 + 瘦身 remote_execution（docker runner only）：
- `services/remote_execution.rs` 只留 `run_executor_command`/`ExecutorTarget::Docker`/`ExecutorRunInput`/`ExecutorRunStatus`/`ExecutorOutcome`/`command_template`；删 `run_remote_command_task`/Ssh 分支/`command_templates` 列表。
- 删 `stores/executor_store.rs`、`http/executors.rs`（+ `/api/executors*`、`/api/executor-runs*` 路由）；`app.rs` 删 `executors` 字段 + `seed_executors`；`main.rs` 删调用。
- `services/dev_selftest.rs`：删 `run_executor_record_test` + `suite.executor` 派发；`run_tests` 派发收敛为 inline docker > P1 桩。
- `support/config.rs`：`RemoteExecutionSettings` 瘦身为 `{ commands }`；删 SeededExecutor*/resolve_seeded_executors/ssh 默认值/ssh 校验。
- `domain/models.rs`：删孤立的 RemoteExecutor*/RemoteCommand*/ExecutorKind 类型；保留 `TaskKind::RemoteCommandRun`/`TaskSource::RemoteExecutor`/`remote_*` TaskRecord 字段 + `TaskPhase::ExecuteRemoteCommand` 供旧 task 记录反序列化（`#[allow(dead_code)]`），`pipeline/executor` 的 ExecuteRemoteCommand 派发臂 bail。

Step 3 — examples/deploy 同步：`examples/logagent.yaml`+`server-dev-selftest.yaml`+`deploy/logagent.example.yaml` 删 skills/fetch/huawei_cloud 块、remote_execution 瘦身为 `{ commands }`、删 seeded-executor 纳管块 + opengemini_smoke_exec suite；per-tool example yaml 剥离 stale llm/claude_code/analysis 块；删 `server-fetch.yaml`+`server-llm-openai-compatible.yaml`；`scripts/start-local.sh` 去掉死掉的 `--llm` 模式；`deploy/SERVER_DEPLOYMENT.md`+`devselftest/opengemini/README.md` 删 Fetch/SSH-SCP/纳管 段。

Step 4 — WebUI 瘦身：导航收敛为 `Tools(含 Runs History) | MCP | Settings`；删 CasesView/ExecutorsView/SkillsView/metadata/MetadataDashboard（+ dashboard-only metadata/*），`metadata/api.ts` 瘦身为共享 HTTP helper；ToolsView 删 FetchView 组件 + 类型/helper；SettingsView 收敛为 API-key + MCP + Skills 说明。bundle 325KB→239KB。

Step 5 — skills 迁本地 + 文档同步：`skills/` 不再被 server 加载（SkillRegistry/config.skills 已删）；runbook 移到 `docs/runbooks/` 作本地 Claude Code skill 作者参考，删 geminidb skill；根 README/SPEC/CLAUDE.md 改写两模块定位；docs/modules 删 cut 模块目录。

验证：`cargo fmt --check` + `cargo check --all-targets`（零 warning，除环境级 deprecation）+ `cargo test -p logagent-server`（76 passed）；`webui` npm lint/typecheck/build 绿；MCP 冒烟 `tools/list` 仅两模块工具 + `resources/list` 仅 runs/tools-catalog；`scripts/start-local.sh` → `/health` ok。

仍 deferred：`max_input_chars` 为 vestigial（`#[allow(dead_code)]`，待后续清理）；`TaskKind::RemoteCommandRun` 等兼容变体待旧数据清退后移除；docs/modules 部分模块文档待按两模块重写。

## 2026-06-24 P2 Docker executor 纳管：record docker kind + CRUD + 执行/run history + seeding + dev_selftest 消费

目标：让 executor record 支持 docker kind，成为可被 API/配置纳管、可执行、有 run history 的托管实体，并让 dev_selftest 能引用一个 docker-kind executor record 派发测试（「纳管 + 指定执行」）。inline `docker` 块保留。对应 plan 问答选定的「完整纳管 + dev_selftest 消费」范围。

- `server/src/support/docker_target.rs`（新）：共享 `DockerTargetSpec`（image/network/workdir/volumes/env，字段全小写单词，yaml+JSON 同型）+ `validate_docker_target(spec, context, allow_devselftest_placeholders)` + `validate_docker_volume` + helper。config.rs 的 `DevSelftestTestDocker` 改为 `pub use DockerTargetSpec` 别名，迁移原校验逻辑。
- `server/src/domain/models.rs`：`ExecutorKind::{Ssh(default), Docker}`（serde snake_case，旧 record 向后兼容）；`RemoteExecutorRecord` 增 `kind`+`docker: Option<DockerTargetSpec>`；`Create/PatchRemoteExecutorRequest` 增 `kind`+`docker`（host/user 改 `#[serde(default)]`）；`RemoteCommandRunRecord` 增 `kind`+`dockerImage`，schema_version 2。
- `server/src/stores/executor_store.rs`：`validate_executor` 改 pub + 按 kind 分支（Ssh⇒host/user/port；Docker⇒docker spec + `validate_docker_target`，host 仅绝对路径）；新增 `create_if_absent`（seeding 用，不覆盖既有）。
- `server/src/http/executors.rs`：`create_executor`/`patch_executor` 按 `kind` 分支（Docker 不要求 host/user）；新增 `executor_api_runs_docker_kind_through_fake_docker` 测试（create docker executor → `/api/executor-runs` → result `kind=docker`/`dockerImage`/stdout 含 `run --rm --network host`）。
- `server/src/services/remote_execution.rs`：`run_remote_command_task` 按 `executor.kind` 分支构建 `ExecutorTarget`（Docker 从 `record.docker` 原样、`launcher=remote_execution.docker_binary`），docker run 计入 `/api/executor-runs` run history。
- `server/src/support/config.rs`：`RemoteExecutionSettings` 增 `docker_binary` + `executors: Vec<SeededExecutor>`；`SeededExecutorConfig`+`resolve_seeded_executors`（load 时 `validate_executor` 校验）；`RemoteExecutionConfig` 增 `docker_binary`+`executors`。
- `server/src/app.rs`/`main.rs`：`seed_executors`（遍历 `config.remote_execution.executors`，`create_if_absent`，persist 失败 warn+skip），`main.rs` `recover_tasks` 后调用。
- `server/src/services/dev_selftest.rs`：`DevSelftestTestSuite` 增 `executor: Option<String>`（与 `docker` 互斥；id `executor_…`）；`run_run_tests` 派发优先级 `executor` > inline `docker` > P1 桩；新增 `run_executor_record_test`（查 record 须 enabled+`kind=Docker`，从 `record.docker` 构建 target，volumes 原样，系统 env 最终优先；ssh-kind 报未支持）。新增 `docker_executor_record_test_dispatch` 测试。
- demo：`examples/server-dev-selftest.yaml` 增 `remote_execution.docker_binary` + `executors` seed（`executor_opengemini_smoke`）+ `test_suites.opengemini_smoke_exec`（`executor` 引用）；`deploy/devselftest/opengemini/README.md` 增「Docker executor 纳管」段。
- 验证：`cargo fmt --check` + `cargo check` + `cargo test -p logagent-server` 全绿（133 测试，含 `validate_ssh/docker_record_branch`、`create_if_absent_does_not_overwrite`、`executor_api_runs_docker_kind_through_fake_docker`、`docker_executor_record_test_dispatch`、`resolves_seeded_executors`，及 SSH fake 回归 + 既有 docker 用例）；example 配置 `enabled:false`/`true` 均加载并服务 `/health`。
- 文档同步：`server/SPEC.md`、`server/README.md`、`docs/modules/dev-selftest/README.md`、`skills/dev-selftest-pipeline/SKILL.md`、`deploy/devselftest/opengemini/README.md`、`docs/modules/dev-selftest/usage-claude-code.md`。
- 仍 deferred：参数化 executor 命令模板（`{var}`+小 JSON Schema）、ssh-kind executor 的 dev_selftest 测试分发（受控 SCP + `ssh_binary_replace`）、P3 package_sync。

## 2026-06-24 Server 部署手册

目标：补齐面向 Linux Server 的完整部署手册，覆盖从源码 checkout 到 runtime 目录、配置、构建安装、启停、MCP 接入、systemd 托管、升级、备份、回滚和排障的可执行步骤。

- 新增 `deploy/SERVER_DEPLOYMENT.md`：以 `/srv/localtoolhub-src/prd_assistant` + `/opt/localtoolhub` 分离部署为主线，说明依赖安装、`.env`、`logagent.yaml`、`deploy/rebuild-install.sh`、`deploy/logagentctl.sh`、WebUI/MCP 验证、可选 systemd unit、source-built analyzer、Fetch/Executor/dev_selftest 可选能力、安全检查、常见故障和最小验收。
- `deploy/README.md` 增加完整部署手册入口。
- `server/README.md` 在本地运行章节补充 Server 部署手册链接；`server/SPEC.md` 增加部署文档维护要求。
- 验证：本次为文档变更，运行 `git diff --check` 确认无 whitespace 问题。

## 2026-06-24 WebUI MCP 页面接口整理

目标：把 MCP 页面从零散 tools/resources 列表整理成完整的接入与调试参考页，避免 Settings 中重复展示 MCP client 配置。

- `webui/src/McpView.tsx`：重做 MCP 页面结构，启动时调用 `initialize` / `ping` / `tools/list` / `resources/list`，展示 endpoint、protocol、连接状态、tools/resources 计数；新增 streamable-http 与 stdio 配置示例、Authorization / `MCP-Protocol-Version` header、支持方法清单、可复制 JSON-RPC 示例。
- MCP tools/resources 浏览增强：tools/resources 支持搜索；选中 tool 展示 `inputSchema`、同步 `tools/call` 示例和 `runMode:"queued"` 示例；选中 resource 调用 `resources/read` 并预览 JSON 文本；新增 `logagent.runs.get/result` 轮询示例，说明 platform 查询工具不创建运行记录。
- `webui/src/SettingsView.tsx`：移除重复的 MCP HTTP client config 代码块与复制按钮，Settings 仅保留 Skills/Tools ZIP 导出和指向 MCP 页面的说明。
- `webui/src/i18n.ts`：补齐 MCP 页面中英文文案。
- 文档同步：`webui/README.md`、`webui/SPEC.md`。
- 验证：`cd webui && npm run lint`、`cd webui && npm run typecheck`、`cd webui && npm run build`、`git diff --check` 均通过。

## 2026-06-23 P2 docker 切片：executor runner 抽通 SSH/Docker target + dev_selftest 内联 Docker target

目标：推进 P2「真实测试分发」，仍聚焦本地 docker 部署路径——把 executor 执行逻辑抽成可复用 runner 支持 SSH/Docker 两种 target；`run_tests` 对带 `docker` 块的测试套件内联构建 Docker target 跑真实 smoke（临时容器 + shell 脚本连接集群 CREATE/INSERT/SELECT）。**不**实现 Docker executor record / `/api/executors` docker CRUD / run history 纳管（显式 deferred）。

- `server/src/services/remote_execution.rs`：新增 `pub` 类型 `ExecutorRunStatus::{Ok,Failed,TimedOut,SpawnFailed}`、`ExecutorTarget::{Ssh,Docker}`、`ExecutorRunInput`、`ExecutorOutcome`、`pub async fn run_executor_command`。SSH 分支与原 `run_remote_command_task` 逐字一致；Docker 分支 `docker run --rm --network <net|"host"> [--workdir] [--env] [--volume] <image> <argv>`，`extra_env`（系统 env）后置覆盖 `target.env`。runner 不检查 `remote_execution.enabled`（开关在任务/handler 入口）。`run_remote_command_task` 改用它并映射 `ExecutorRunStatus→RemoteCommandStatus`（保留 TimedOut）。
- `server/src/services/dev_selftest.rs`：`run_run_tests` 对 `suite.docker` 内联构建 `ExecutorTarget::Docker` 派发（无 docker 块则走原 P1 桩）。argv/timeout 取自 `suite.command` 引用的 `remote_execution.commands` 模板（无则 `suite.argv`）；volume host 侧 `${DEVSELFTEST_*}` 经 `deploy_env` 插值并断言绝对；系统 env（`DEVSELFTEST_HOST/PORT` + run 目录 4 var）最终优先。新增 `run_docker_test` + `interpolate_volume`。
- `server/src/support/config.rs`：`DevSelftestTestSuite` 增 `command`/`docker`；新增 `DevSelftestTestDocker` + 校验（image 不以 `-` 开头、network `host`|安全标识符、workdir 绝对无 `..`、volume `host:absolute|${DEVSELFTEST_*}:container:absolute[:ro|rw]`、env 键 `^[A-Z_][A-Z0-9_]*$`）；command/argv 互斥且至少一个、command 须配 docker 块。`DevSelftestDockerConfig` 改手写 `Default`（binary 用 `default_docker_binary()`，修 omit docker 块时 binary 空导致 enabled 校验失败）。
- demo：`deploy/devselftest/opengemini/tests/smoke.sh`（`/bin/sh`，curl 优先 else busybox wget，SHOW/CREATE/write/SELECT，无 apt/外网依赖）；`examples/server-dev-selftest.yaml` 增 `remote_execution.commands.opengemini_smoke` + `test_suites.opengemini_smoke`（command + docker 块 alpine:3.20 host 网络）；`deploy/devselftest/opengemini/README.md` 补「Test execution (docker executor)」+ `DEVSELFTEST_TEST_IMAGE` 内网覆盖。
- 验证：`cargo fmt --check` + `cargo check` + `cargo test -p logagent-server` 全绿（123 测试，含新增 `run_executor_command_docker_target`（Ok/Failed/TimedOut/SpawnFailed + 完整 argv/env/volume/network）、`dev_selftest_test_suite_command_argv_rules`、`dev_selftest_test_docker_security_validation`、`docker_executor_test_dispatch`，及 SSH fake 回归 `executor_api_runs_configured_command_through_fake_ssh` 与原 `docker_selftest_closed_loop`）。example 配置 `enabled:false` 与 `enabled:true` 均可加载并服务 `/health`。
- 文档同步：`server/SPEC.md`、`docs/modules/dev-selftest/README.md`、`skills/dev-selftest-pipeline/SKILL.md`、`deploy/devselftest/opengemini/README.md`。
- 仍 deferred：参数化 executor 命令模板（`{var}`+小 JSON Schema）、Docker executor 纳管（record+CRUD+run history）、`ssh_binary_replace` 部署 + SCP、P3 package_sync。

## 2026-06-23 openGemini docker 集群 artifact 纳入仓库 + 内网可配置

目标：把跑通的 openGemini docker 集群 artifact 从本地 scratch 纳入仓库作为默认 demo，并做成内网可配置（换镜像名 + 换源）。

- 新增 `deploy/devselftest/opengemini/`：`build-opengemini.sh`（go1.26+sonic 升级 + `go build`，`GOPROXY` 默认 `https://goproxy.cn,direct`，可 env 覆盖）、`docker-compose.yml`（6 service，`image: ${OG_BASE_IMAGE:-ubuntu:24.04}`，静态 IP，自定义 bridge 网络）、`config/openGemini.conf.template`（上游模板，含 `{{addr}}/{{id}}/{{meta_addr_*}}` 占位符）+ `entrypoint-meta.sh`/`entrypoint-sqlstore.sh`（容器启动时按 `OG_ADDR/OG_ID/OG_META_*` env 替换占位符 + 顺序门控）+ `README.md`。
  - 用单模板 + entrypoint 替换（而非 6 份硬编码配置），减少冗余、IP 集中可改。
- 内网可配置（均经 server 进程 env，由 deploy/build 子进程继承，**无代码改动**）：
  - 镜像名：`OG_BASE_IMAGE=<registry>/ubuntu:24.04`（compose 用 `${OG_BASE_IMAGE:-ubuntu:24.04}`）。
  - Go 模块源：`GOPROXY=<内部代理>`（默认 goproxy.cn），`GOSUMDB=off`（内部代理无法访问 sum.golang.org 时）。
  - openGemini 源码：server 配置 `dev_selftest.git.repos` 换成内部 git 镜像。
- `examples/server-dev-selftest.yaml` 改为 openGemini demo（build/docker/test profile 指向 `deploy/devselftest/opengemini/*`，`enabled: false` 默认可加载，注释说明启用步骤 + 内网覆盖）。
- 验证：用 `deploy/devselftest/opengemini/docker-compose.yml` 单独起 6 容器，meta-1 选主成功（`election won`）、`curl SHOW DATABASES` 返回合法 JSON、~6s 就绪、`down` 干净。example 配置 `enabled:false` 可加载并服务 `/health`。
- 文档同步：`deploy/devselftest/opengemini/README.md`、`server/SPEC.md`、`docs/modules/dev-selftest/README.md`、`skills/dev-selftest-pipeline/SKILL.md`。

## 2026-06-23 Dev self-test Docker 路径跑通：真实 openGemini 3meta+3(sql+store) 集群

目标：把 Path 1（Docker 部署）对着真实 openGemini 集群端到端跑通——`sync → build → deploy → run_tests → report` 全链路对 `openGemini.git` 真实生效，3 meta + 3 (sql+store) = 6 容器 / 9 进程，直到 `report` 状态 `SUCCEEDED`。已达成。

代码改动（仓库内）：
- `server/src/services/dev_selftest.rs`：新增 `deploy_env(run_root, source_dir, artifacts_dir, project_name)`，把 `DEVSELFTEST_RUN_DIR/SOURCE_DIR/ARTIFACTS_DIR/PROJECT_NAME` 注入 `docker compose`（`run_deploy`）**和** health check（`run_health_check` 加 `env` 参数），让 compose 用 `${DEVSELFTEST_SOURCE_DIR}` 挂载本次 run 编译出的二进制。通用、非 openGemini 专属。+ 单测。
- `server/src/mcp_server.rs`：新增 `mcp_tool_params(arguments)`，`tools/call` 既接受 `{params:{...}}`（HTTP 信封）也接受顶层参数（MCP 规范，`arguments` 即 `inputSchema`），后者剥离 `runMode/uploadIds`。修复「真实 MCP 客户端（Claude Code）按 schema 传顶层参数 → 服务端读 `arguments.params` 为空」的阻塞问题。`run_catalog_tool` / `run_catalog_tool_queued` 复用。+ 单测。

openGemini 集群 artifact（scratch，**不进仓库**，在 `~/dev_selftest/opengemini/`）：
- `build-opengemini.sh`：go1.26 兼容（`go mod edit -go=1.26` + 升级 `bytedance/sonic` 到最新 + `go mod tidy`）+ `go build -o build/ts-{meta,store,sql} ./app/ts-*`（绕开 `build.py` 的 click/vet 开销）。产物 `build/ts-meta/ts-store/ts-sql`。
- `docker-compose.yml`：6 service（meta-1/2/3、sqlstore-1/2/3），`ubuntu:24.04` 基础镜像（22.04 的 libstdc++ 过旧，缺 `GLIBCXX_3.4.32`），**静态 IP**（172.28.0.x），自定义 bridge 网络。挂载 `${DEVSELFTEST_SOURCE_DIR}/build`（二进制）+ per-node 配置 + entrypoint。
- `config/{meta,sqlstore}-{1,2,3}.conf`：从仓库 `config/openGemini.conf` 派生（**不是** `conf/`），用静态 IP 替换 `{{addr}}/{{id}}/{{meta_addr_*}}`。
- `entrypoint-meta.sh`、`entrypoint-sqlstore.sh`：顺序门控——sqlstore 等 3 个 meta:8091 就绪 → 起 ts-store → 等 ts-store:8400（**用容器自身 IP，非 127.0.0.1**，因 store 按配置 IP 绑定）→ 起 ts-sql。
- `~/dev_selftest/server-opengemini.yaml`：dev_selftest 配置（openGemini git repo、build profile 指向 build 脚本、docker cluster、test suite 用 curl `SHOW DATABASES`）。

跑通过程中解决的关键坑（执行时细化，记录以便复现）：
1. **go1.26 不兼容**：openGemini go.mod 是 1.24 + sonic v1.13.3，需升 go.mod 到 1.26 + sonic 到最新，否则编译失败（build 脚本内处理）。
2. **raft 选主失败**：openGemini meta 用 `rpc-bind-address` 字符串作为 raft Server ID；用容器**主机名**时，节点绑定的解析 IP 与配置里的主机名串不匹配 → hashicorp raft 判定「not part of a stable configuration」不选主。改用**静态 IP**（对齐官方 `install_cluster.sh` 用 127.0.0.1/2/3）后 meta-1 正常 bootstrap 选主。
3. **libstdc++ 过旧**：ubuntu:22.04 缺 `GLIBCXX_3.4.32`，ts-meta 启动即退；改 `ubuntu:24.04`。
4. **ts-store 端口检查**：store 按 `store-ingest-addr`（容器 IP:8400）绑定，entrypoint 不能用 `127.0.0.1:8400` 探活，须用 `hostname -I` 的容器 IP。
5. **MCP 参数信封**：catalog 工具经 MCP 需把 tool 参数放 `arguments.params`（HTTP 信封），真实 MCP 客户端按 schema 传顶层参数会拿空 → 已用 `mcp_tool_params` 修成两者兼容。

验证（端到端跑通）：
- `cargo fmt --check`、`cargo check`、`cargo test -p logagent-server`（118 通过，+1 `mcp_tool_params`、+1 `deploy_env`）。
- 起 server（`sg docker -c` 激活 docker 组，使 deploy 的 `docker compose` 子进程可用），经 `POST /api/mcp` 驱动：`sync_workspace{gitRepo,gitRef:main}`（41s 克隆）→ `build{buildProfile:opengemini}` runMode:queued（30s 编译出 ts-meta/ts-store/ts-sql，`runs.get` 轮询到 SUCCEEDED）→ `deploy{profile:opengemini_cluster}` runMode:queued（466ms 起 6 容器 + health check `curl SHOW DATABASES`，SUCCEEDED）→ `run_tests{testSuite:opengemini_smoke}`（exit 0）→ `report`（**status SUCCEEDED**，4 步全 OK，`report.md` + `progress.json` + artifacts 齐全）。
- `docker ps` 见 6 个 openGemini 容器；`curl http://127.0.0.1:8086/query?q=SHOW+DATABASES` 返回合法 JSON；`CREATE DATABASE` + `SHOW DATABASES` 验证集群可写可读。
- 文档同步：`server/SPEC.md`、`docs/modules/dev-selftest/README.md`、`skills/dev-selftest-pipeline/SKILL.md`。

不在本里程碑范围：P2（参数化 executor + SCP + ssh_binary_replace）、P3（package-sync core + geminidb create/poll）；把 compose/配置/build 脚本纳入仓库（用户明确不要，scratch 在 `~/dev_selftest/`）。

## 2026-06-23 Server config：dev_selftest 禁用态不再阻断启动

目标：修复 Server 启动加载配置时，即使未启用 `dev_selftest` 也会因为 `dev_selftest.docker.binary` 非绝对路径报错的问题。

- `server/src/support/config.rs`：`resolve_dev_selftest` 增加启用态门控；`dev_selftest.enabled=false` 时允许保留占位 `docker.binary`，不会阻断 Server 启动；`enabled=true` 时仍要求 Docker binary / compose 等路径绝对，build/test profile 仍需完整。
- 增加配置回归测试 `dev_selftest_disabled_allows_placeholder_docker_binary`，覆盖禁用态允许 `docker` 占位、启用态继续报错。
- 文档同步：`server/README.md`、`server/SPEC.md`、`docs/modules/dev-selftest/README.md` 说明禁用态和启用态的校验边界。
- 验证：`cargo fmt --check`、`cargo check`、`cargo test -p logagent-server dev_selftest_disabled_allows_placeholder_docker_binary`、`cargo test -p logagent-server`（117 通过）、`git diff --check` 均通过；用 `/Users/duzhiwang/workspace/db/prd_assistant_v2/deploy/logagent.yaml` 启动当前二进制已越过配置加载并监听 `0.0.0.0:50992`。

## 2026-06-23 Dev self-test pipeline P1：dev_selftest 工具组 + Docker 自测闭环

目标：在 P0（MCP 传输 + 异步 run 模型）之上落地开发自测流水线的第一刀可执行切片——`logagent.dev_selftest.*` 内置工具组，跑通 sync → build → deploy(docker_cluster) → run_tests(桩) → report 闭环。SSH 二进制替换（P2）和打包+云实例（P3）为后续蓝图。

- `server/src/services/dev_selftest.rs`（新）：5 个内置工具（sync_workspace / build / deploy / run_tests / report），镜像 `gemini_db` 的自包含工具组结构（descriptors/get_descriptor/is_dev_selftest_tool/validate_run_params/run_dev_selftest_task）。run = 持久工作区 `data/dev_selftest/runs/{runId}/`（source/ artifacts/ logs/ progress.json report.md/json）+ `DevSelftestRunRecord` 索引；每步追加 progress，写各自 result.json。
  - sync_workspace：tarball 上传解包（复用 `log_analyzer::extract_upload`）/ allowlisted git clone / 空 source（桩）。
  - build：配置式 `command` 在 `source/{working_dir}` 执行（`run_bounded_command`，timeout + 输出上限），按 `artifact_globs`（单层 `*` glob）收集到 artifacts/。
  - deploy（P1 仅 docker_cluster）：`<docker> compose -p devselftest_<run>_<cluster> -f <compose> up -d` + 声明式 health check；记录 Docker target。
  - run_tests（P1 桩）：本地跑配置式 `argv`，注入 `DEVSELFTEST_HOST/PORT`；支持 `runMode:"queued"`。
  - report：聚合 progress.json 生成 report.md + report.json（状态/时长/错误/evidence/失败步骤）。
- `server/src/stores/dev_selftest_store.rs`（新）：`DevSelftestStore`（JSON-per-run + 内存 map），镜像 `RemoteExecutorStore`。
- `server/src/support/config.rs`：`DevSelftestSettings` + 子结构（git/builds/docker/test_suites，含 health_check）+ resolver（绝对路径校验、profile id 校验、git repo+ref allowlist）+ `StorageSettings::dev_selftest_dir/dev_selftest_runs_dir/dev_selftest_run_dir` + `prepare_dirs`；`AppConfig` 新增 `dev_selftest` 字段。
- `server/src/domain/models.rs`：`DevSelftestRunRecord` / `DevSelftestDeployTarget`（Docker/Ssh/Instance，P1 仅用 Docker）/ `DevSelftestStep` / `DevSelftestRunStatus`。
- `server/src/app.rs`：`AppState` 新增 `dev_selftest: DevSelftestStore` + load。
- `server/src/services/tools.rs`：dev_selftest 接入 4 个注册点（descriptors/get_descriptor/validate_tool_run_request/run_tool_task）。
- `skills/dev-selftest-pipeline/`（新）：SKILL.md + logagent.json + references/workflow.md（P1 docker 闭环 runbook + runMode/轮询 + 结果形状）。
- `examples/server-dev-selftest.yaml`（新）+ `examples/logagent.yaml` 增 `dev_selftest:` 禁用块。
- `docs/modules/dev-selftest/README.md`（新）+ `server/SPEC.md` 增 Dev Self-Test 章节。
- 同步为所有测试 `AppConfig { ... }` 字面量补 `dev_selftest` 字段（14 处）。
- 验证：`cargo fmt --check`、`cargo check`、`cargo test -p logagent-server`（116 通过，+7 dev_selftest/store）均通过；`docker_selftest_closed_loop` 集成测试用 fake docker 跑通全链路并校验 report.md + 5 步 progress。本地 MCP 冒烟：`tools/list` 含 5 个 dev_selftest 工具，`tools/call sync_workspace` 返回 runId。

## 2026-06-23 Dev self-test pipeline P0：MCP streamable-http + 异步 run 模型

目标：为「Windows ClaudeCode → Linux LocalToolHub MCP」的开发自测流水线打底：合规的远程 MCP 传输 + 通用的异步 run 模型 + 不污染历史的 run 查询。这是分阶段方案（P0 传输/run 模型 → P1 Docker 自测闭环 → P2 SSH 二进制替换 → P3 打包+云实例）的第一刀。

- `server/src/mcp_server.rs`：
  - `POST /api/mcp` 升级为 stateless streamable-http：按 `Accept` 返回 `application/json` 或单帧 SSE `event: message`，回显 `MCP-Protocol-Version`，**不签发 `Mcp-Session-Id`**（无状态服务器）。新增 `GET /api/mcp` → 405（本服务无服务端推送通知）。
  - `tools/call` 支持可选 `runMode: "sync"|"queued"`（默认 sync，原同步行为不变）。`queued` 创建**一个** ToolRun 经 `TaskExecutor` 入队并立即返回 `{runId,status:"QUEUED",url}`，不等待、不产生子 run、不引入隐藏 worker tool id。
  - 新增 MCP 原生 platform 工具 `logagent.runs.get` / `logagent.runs.result`：`call_tool` 直接读 `TaskStore`，**不创建 ToolRun**，避免轮询污染 run history。
- `server/src/domain/models.rs`：`ToolDescriptor` 新增 `platform: bool`（`#[serde(default)]`），标记 side-effect-free 查询工具，`tools/list` 以 `runnable || platform` 过滤。
- `server/src/services/tools.rs`：新增 `platform_run_descriptors()`（`logagent.runs.get/result`，`platform=true`、`runnable=false`、`read_only=true`），接入 `descriptors()` / `get_descriptor()`；所有现有 ToolDescriptor 构造点补 `platform: false`。
- `server/src/support/config.rs`：`McpSettings` / `McpConfig` 新增 `allowed_origins`；`resolve_mcp` 透传。
- `server/src/mcp_server.rs`：`check_origin` 按 `mcp.allowed_origins` 校验 `Origin`（空列表跳过；无 Origin 头放行；非浏览器/隧道场景不受限）。
- `server/src/main.rs`：`cors_layer` 接受 `allowed_origins`，非空时收紧 CORS 到指定来源（替代无条件 `allow_origin(Any)`）。
- `server/src/http/mod.rs`：`/api/mcp` 增加 `GET`（405）。
- 文档：`server/SPEC.md` MCP 章节更新。
- 验证：`cargo fmt --check`、`cargo check`、`cargo test -p logagent-server`（109 通过）均通过；新增 4 个 mcp_server 测试（queued 可轮询、platform 工具不建 run 记录、streamable JSON/SSE/protocol-version、Origin 拒绝）。本地 `curl` 冒烟：initialize 返回 JSON + 回显 protocol-version、`tools/list` 含 `runs.*`、`Accept: text/event-stream` 返回 SSE、GET 返回 405。

## 2026-06-23 GeminiDB Influx tool 组按官方 API 文档修整

目标：用户指出现有 GeminiDB Influx tools 的请求路径和参数曾由其他模型猜测生成，不可信；本次按 HuaweiCloud NoSQL API v3 实例管理官方文档重新核对并修整。参考页面：实例管理索引 `topic_300000002.html`，创建/删除/查询/改名/SSL/重启分别为 `nosql_05_0014.html`、`nosql_05_0015.html`、`nosql_05_0016.html`、`nosql_05_0102.html`、`nosql_05_0107.html`、`nosql_05_0108.html`（用户给的 `nosql_05_0007.html` 是“如何调用 API”，用于确认调用方式/鉴权背景）。

- `server/src/services/gemini_db.rs`：
  - 保留 6 个 tool 单独分组和 `endpoint` / `projectId` 单次 run 覆盖能力，继续使用配置中的 `X-Auth-Token` env 注入，token 不进 params/result。
  - create 从“body 透传猜字段”改为按官方字段构造请求 body：`name`、`datastore.type=influxdb`、`region`、`availability_zone`、`vpc_id`、`subnet_id`、`security_group_id`、`password`、`mode`、`flavor[]` 等；`flavor_ref`/`volume` 等旧猜测字段不再作为模板；保留高级 `body` 逃生口但要求 `datastore.type=influxdb` 和非空 `flavor`。
  - list 默认追加 `datastore_type=influxdb`，并校验 `mode` 仅允许 Influx 相关值；`datastoreType` 显式传非 `influxdb` 会拒绝。
  - SSL 从错误的 `PUT /instances/{id}/ssl` + 猜测 body 改为官方 `POST /instances/{id}/ssl-option`，params `sslOption=on|off` 映射为 body `{"ssl_option":"on|off"}`。
  - restart 改为官方 `POST /instances/{id}/restart`；无 `nodeId` 时不发送 body，传 `nodeId` 时映射为 `{"node_id":...}`；文档注明当前官方约束里 `node_id` 仅 Redis 云原生集群节点重启支持。
  - rename 保持官方 `PUT /instances/{id}/name` + `{"name": ...}`，名称长度校验收紧到 4..64 bytes。
  - 单测更新覆盖官方路径/方法、create 官方 body、list 默认 Influx 过滤、SSL body、restart 无 body/node body、敏感字段脱敏与原始转发。
- `server/src/http/tools.rs`：扩大 tool run 测试 helper 的轮询窗口并输出最后状态/error，修复完整并行测试下 pprof HTTP 集成测试偶发超 1s 的时序失败。
- 文档同步：`server/README.md`、`server/SPEC.md`、根 `SPEC.md`、`examples/logagent.yaml`、`skills/geminidb-influx-instance-mgmt/SKILL.md`、`references/api-fields.md`、`logagent.json`。
- 验证：`cargo fmt --check`、`cargo check`、`cargo test -p logagent-server`（105 通过）均通过；定向 `cargo test -p logagent-server gemini_db -- --nocapture` 也通过（11 通过）。

## 2026-06-23 GeminiDB Influx 实例管理内置 tool 组 + Skill

历史记录：该初版在当时文档无法在线核实时采用 body 透传策略；当前行为已由上方“按官方 API 文档修整”条目替代。

目标：参考华为云 NoSQL(GeminiDB Influx) API，实现一组 6 个实例生命周期管理内置 tool（创建/删除/查询列表详情/改名/切换 SSL/重启实例或节点），在 catalog 中单独归为「GeminiDB Influx」一组；请求 endpoint 支持灵活动态配置（配置默认 + 每次运行可覆盖）。鉴权用 `X-Auth-Token`（仅从环境变量读）。文档 URL 在本环境被 WAF 拦截无法在线核实，故 create/SSL/restart 的请求体**透传**调用方按文档传入的 body，工具只负责 method+路径+鉴权+endpoint 解析，避免字段名猜错。

- 新增自包含服务模块 `server/src/services/gemini_db.rs`：
  - 6 个 tool id（`logagent.geminidb.{create,delete,list,rename,toggle_ssl,restart}_instance`）；`descriptors(config)`/`get_descriptor()` 产出 6 个 `ToolDescriptor`，`enabled`/`runnable` 跟随 `huawei_cloud.gemini_db.enabled`，默认 disabled 灰显；`backend="gemini_db_influx"`、`tags=[built-in,huawei-cloud,gemini-db,manual-run]`、`min_files=0/max_files=0`。
  - `validate_run_params`：按工具校验（instanceId 必填且仅 `[A-Za-z0-9_-]`、create/toggle_ssl 的 body 必须是对象、rename 的 name 必填、list 过滤项全可选且拒 body、restart body 可选）；`endpoint`/`projectId` 覆盖项非空时校验（endpoint 走 http/https+host+无 path/凭据/query 的 SSRF 校验，projectId 仅 `[A-Za-z0-9_-]`）。
  - `run_gemini_db_task`：解析 endpoint（params 覆盖 > 配置默认）、projectId（同）、auth_token（仅配置/env），`build_plan` 构造 method+path（`/v3/{pid}/instances[/{id}[/name|/ssl|/restart]]`）+query+body，trait `GeminiDbHttpClient`（真实实现注入 `X-Auth-Token`+`Content-Type`，单测用 Fake）发送；结果 `result.json`（`write_json_atomic`）：`status`(HTTP 2xx→OK)/`http`/`request.body`(脱敏)/`response.body`(截断 64KiB)/`endpoint`/`credentialMetadata.authTokenEnv`/`timings`。**脱敏**：存储的 request body 对 `password`/`secret`/`token`/`ak`/`sk` 等键替换为 `<redacted>`；token 绝不入 params/logs/result。
- `server/src/services/tools.rs`：`descriptors()` extend、`get_descriptor()` 解析、`validate_tool_run_request` 与 `run_tool_task` 增加 `gemini_db` 分派（`is_gemini_db_tool` 守卫）；`services/mod.rs` 加 `pub mod gemini_db`。无 HTTP handler/MCP 改动——经 `descriptors()` 自动出现在 `/api/tools`、MCP `tools/list`、WebUI catalog。
- `server/src/support/config.rs`：新增 `GeminiDbSettings`（enabled/timeout_seconds/endpoint/project_id/project_id_env/auth_token_env/auth_token/region）+ `Default`(disabled) + `Debug`(token 脱敏)；`HuaweiCloudSettings` 加 `gemini_db` 字段；raw `GeminiDbConfig` + `HuaweiCloudConfig.gemini_db`；`resolve_gemini_db`（enabled 时校验 endpoint URL、project_id 取值或 env、`auth_token_env` 必填并 `resolve_required_env`）；default/timeout 函数；config 单测（enabled 门控、缺 endpoint/缺 token 报错、endpoint 带 path 报错、project_id 走 env）。
- `examples/logagent.yaml`：`huawei_cloud.gemini_db` 禁用示例块（endpoint/project_id_env/auth_token_env/timeout/region）。
- WebUI：`ToolsView.tsx` `CATEGORY_ORDER` 加 `gemini`，`categoryOf`（tag `gemini-db` 或 `backend==="gemini_db_influx"` → gemini）+ `categoryLabel`；`i18n.ts` 两语言加 `groupGemini: "GeminiDB Influx"`。
- 新增 managed Skill `skills/geminidb-influx-instance-mgmt/`（`SKILL.md` + `logagent.json` + `references/api-fields.md`）：runbook（前置条件、6 工具、endpoint 动态覆盖、body 透传、读结果、安全）+ 各操作最佳已知请求体字段（附文档 URL，注明以线上文档为准）；`toolIds` 含 6 个工具，`skills.roots: ["skills"]` 自动加载。
- 单测：`gemini_db::tests`（descriptor 列表/门控、各 validate 分支、build_plan 的 method/path/query、FakeHttpClient 下 create/delete/list 落 `result.json` 且 status/方法/路径/转发的 body 正确、非 2xx→FAILED、password 脱敏且转发 body 仍含明文、token 不入结果）+ config 2 个新测试。
- 文档：`SPEC.md` 工具示例列表加 6 个 `logagent.geminidb.*`；本条 PROGRESS。
- 验证：`cargo fmt --check`、`cargo test -p logagent-server`（105 通过）；`webui` `npm run lint`/`typecheck`/`build` 通过。（真实联通由启用 config + 设 token env 后跑 `list_instances` 手测覆盖。）

## 2026-06-23 批量 InfluxQL 日志分析内置 tool + Skill

目标：把「上传日志 -> 解包/预处理 -> influxql analyzer 分析」做成一个可发现、可批量的一键工具，并配一个内置 Skill 作为 runbook。现状该流程隐式存在但埋在 `influxql_analyzer`（configured，默认 disabled，`max_input_files: 3`）里，无批量入口。

- 新增内置 tool `logagent.batch_influxql_analysis`（`server/src/services/tools.rs`）：
  - `descriptors()`/`get_descriptor()`/`validate_tool_run_request()`/`run_tool_task()` 四处接线；`batch_influxql_analysis_descriptor(config)` 的 `enabled`/`runnable` 跟随 `influxql_analyzer` 是否配置+启用（pprof 模式），未启用时 catalog 中灰显。
  - `run_batch_influxql_analysis_task`：`prepare_pipeline_run` + `extract_task` 解包预处理（复用 `log_analyzer` 已有的 influxql JSONL 物化），读 `Manifest.tool_inputs_path` 的 `ToolInputIndex` 筛出 `tool_ids` 含 `influxql_analyzer` 的输入，对每个输入用 `tool_runner.execute`（action.input=`{tool: influxql_analyzer, inputFile}`，复用 configured tool 的 path/args + `{input_file}` 替换）跑分析，聚合 `findings[]`。200 输入安全上限（超限只警告）。结果 `result.json`：`preprocessSummary`/`analyzedInputs`/`failedCount`/`findings[]`/`warnings[]`/`status`(OK/PARTIAL/FAILED)。`max_files: 100`，`accepted_suffixes: .tar.gz/.tgz/.tar`。
  - 无 WebUI/MCP 改动：tool 经 `descriptors()` 自动出现在 `/api/tools`、MCP `tools/list`、WebUI Tools「Analyzers」分组（tag `log`）。
- 新增 managed Skill `skills/influxql-batch-analysis/`（`SKILL.md` + `logagent.json` + `references/batch-result.md`）：流程 runbook + 结果 schema；`toolIds` 含 batch tool / `influxql_analyzer` / `preprocess_log_package`；`skills.roots: ["skills"]` 自动加载。
- 单测（`tools::tests`）：descriptor 在 influxql 缺失/禁用/启用下的 `enabled`/`runnable` 门控、`descriptors()`/`get_descriptor()` 列出该 tool、`validate_batch_influxql_params` 接受对象/拒绝非对象。（需要真实 binary 的端到端跑由 smoke/手测覆盖。）
- 文档：`SPEC.md` 工具示例列表加 `logagent.batch_influxql_analysis`；本条 PROGRESS。
- 验证：`cargo fmt --check`、`cargo test -p logagent-server`（94 通过，含 5 个新测试）。
- 端到端已跑通（Go 1.26.4 构建 `target/tools/influxql-analyzer`，临时配置 `influxql_analyzer.enabled: true`）：
  - `GET /api/tools` 列出 `logagent.batch_influxql_analysis`（`enabled`/`runnable`=true）；`GET /api/skills` 列出 `influxql-batch-analysis`（toolIds 含 batch tool）；MCP `tools/list` 含该 tool（共 7 个）。
  - 上传 2 个含 InfluxQL query 的 `_logs.tar.gz`（node1/node2）跑 batch tool → `status: OK`，`influxqlInputs: 2`、`nodes: 2`、`findings[]` 2 条，每条带 `nodeId`/`packageTimestamp` + analyzer 规则（large_limit/has_wildcard/meta_query/no_time_filter）；server 日志无 error/panic。
  - 上传无 query 的包 → tool `status: FAILED`、`influxqlInputs: 0`、warning 正确。
  - 发现并补全 Skill「Input expectations」：preprocessor 要求包名 `<pkgid>_<inst>_<node>_<YYYY>_<MM>_<DD>_<HH>_<MM>_<SS>_<micros>_logs.tar.gz`、tar 内日志须在 `var/chroot/gemini/log/{tsdb,stream}` 或 `home/Ruby/log` 下、query 行须为 JSON 对象（`query`/`sql`/`stmt`/`statement`）或 `query="..."`。

## 2026-06-23 WebUI Tools 目录页重设计（搜索/筛选/分组）

目标：Tools 页 catalog 列表信息杂乱、且工具增长到几十个后「依次排开」不可用。结合工具市场/命令面板的业界实践重做左侧 catalog 卡片，右侧 detail+run 面板不变。

- `ToolsView.tsx` `ToolPluginsView`：左侧 catalog 卡片改为可搜索、可筛选、按类别分组的紧凑列表。
  - 新增状态 `query` / `sourceFilter`(all|built_in|configured) / `runnableOnly`；用 `useMemo` 派生 `filtered`（按 displayName/toolId/description/tags 过滤）与 `groups`。
  - 派生功能类别 `categoryOf`（Analyzers/Metadata/Fetch/Sync/Other，由 tags+toolId+backend 推导，避开冗余 tag）；无搜索时按 `CATEGORY_ORDER` 分组带计数，空组隐藏；搜索时切扁平 `Results (N)`（按 displayName 排序）。
  - 紧凑 `ToolRow`：状态点（绿=enabled&runnable、琥珀=enabled 非 runnable、灰=disabled）+ 名称 + 来源标签；选中高亮。去掉列表里冗余的 `toolId · backend`、双 badge、描述、tags 行（这些已在右侧详情面板）。
  - 头部计数 `toolCount(shown,total)`；空状态 `noTools`/`noMatches`。左列 340px→380px。
  - 右侧 detail/run 面板、`runTool`/`refreshTools`/`refreshRuns`/`selectRun`/轮询 全部不变；`ToolDescriptor` 类型与 `/api/tools` 响应不变（纯前端，无 server 改动）。
- `i18n.ts` `toolsCopy`（中英）：新增 `searchPlaceholder`/`filterAll`/`filterBuiltIn`/`filterConfigured`/`runnableOnly`/`groupAnalyzers`/`groupMetadata`/`groupFetch`/`groupSync`/`groupOther`/`noMatches`/`resultsLabel(n)`/`toolCount(shown,total)`；删除随之 dead 的 `enabledBadge`。
- 文档：`webui/SPEC.md` `### Tools` 补搜索/筛选/分组要求；`webui/README.md` Tools bullet 更新；本条 PROGRESS。
- 验证：`npm run lint` / `typecheck` / `build` 全绿（bundle 325.53 KB）。

## 2026-06-23 WebUI 顶层导航改英文 + Runs 收纳为 Tools 子项

- 顶层导航标签页改为纯英文展示，不再随语言切换中英双语（页面内部文案仍随语言切换）。导航顺序调整为 `Tools → Skills → MCP → Metadata → Fetch → Executors → Cases → Settings`。
- Runs 不再作为独立顶层标签页，改为 Tools 的子项「Runs History」（缩进虚框小标签，点击仍渲染原 `RunsView`）。`App.tsx` 导航数据改为带 `children` 的 `NavItem[]`，用 `Fragment` 渲染父项 + 缩进子项；`navItems` 提为模块级常量（不再依赖 `copy`）。
- `i18n.ts`：删除 `appCopy` 中随之 dead 的 `navTools`/`navRuns`/`navMetadata`/`navFetch`/`navExecutors`/`navMcp`/`navCases`/`navSkills`/`navSettings` 与本就未使用的 `apiKeyRequired`。
- 同步更新 `webui/README.md`（导航顺序图 + 页面职责）、`webui/SPEC.md`（页面要求节重排为 Tools/Runs History/Skills/MCP/Metadata/Fetch/Executors/Cases/Settings，补 Cases，注明顶层英文-only 与 Runs 子项）。
- 验证：`npm run lint` / `npm run typecheck` / `npm run build` 全绿（bundle 322.26 KB）。

## 2026-06-23 清理所有 Rust warning（Wave C dead-code 清理）

目标：`cargo check --all-targets` 零 warning。

- **metadata.rs dead-code 清理（Wave C）**：删除 retired analysis-agent 的 metadata-context-outline 子系统（~850 行）：`MetadataSection` enum + impl、`MetadataSliceQuery`/`MetadataSliceResult`/`MetadataCounts` 结构、`metadata_context_outline`/`metadata_slice_query_from_value`/`query_metadata_context`、以及 `section_outline`/`metadata_counts`/`optional_*_filter`/`validate_metadata_query_filters`/`metadata_query_filters`/`metadata_items_for_section`/`metadata_*_items`/`*_matches`/`shard_ids_for_group`/`pt_owner_filters_match` 等全部 helper。保留 keeper metadata 端点（`get_metadata_field_types`/`get_metadata_tag_fields` 等）和它们依赖的 `measurement_name_matches`/`databases` 视图函数。误删的 `fetch_metadata_content`（async fn，被 import 预览使用）已恢复。同步删除 3 个只测试已删函数的 test（`metadata_outline_*`/`metadata_query_filters_*`/`metadata_query_rejects_*`）及仅被它们使用的 `metadata_context_fixture`。移除随之 unused 的 `serde_json::{json, Value}` import（文件改用 `serde_json::` 全限定）。
- **config.rs**：删除从未读取的 `AppConfig.config_path` 字段（及 11 处 test 构造赋值）和 `McpSettings.transport` 字段（值恒为 "stdio"，`resolve_mcp` 仍校验输入 transport；`rejects_unknown_mcp_transport` 测试不变）。
- **log_analyzer.rs**：`read_log_slice` 仅被一个 test 使用，改用 `#[cfg(test)]` 限定为 test-only（非测试构建不再编译，消除 "never used" warning）。
- **skill_registry.rs**：移除 unused import `SystemContextBundle`。
- **tool_runner.rs 测试**：`action()`/`Fixture::context()`/`EvidenceProvider` import 仅被 3 个 `#[cfg(unix)]` async 测试使用，加 `#[cfg(unix)]` 守卫，消除 Windows `--tests` 下的 dead-code warning。
- 验证：`cargo fmt --check`、`cargo check --all-targets`（零 code warning，唯一 warning 是环境级 `~/.cargo/config` deprecation）、`cargo test -p logagent-server`（89 通过，原 92 删 3 dead test）；Windows 交叉编译 `cargo check --tests --target x86_64-pc-windows-gnu` 同样零 code warning。

## 2026-06-23 WebUI 拆分 System Context 集合页

- 移除 WebUI 顶层 "系统上下文 / System Context" 集合标签页（`SystemContextView`，内部用 Tabs 聚合 Skills + Metadata，其中 Metadata 与已有顶层 Metadata 标签页重复）。
- 把 Skills 拆为独立顶层导航项：新增 `webui/src/SkillsView.tsx`（从 `SystemContextView` 提取 Skills 列表/详情/导入，去掉 Tabs 包装与 Metadata 子页）；`App.tsx` 导航 `system-context` → `skills`，渲染 `SkillsView`；`i18n.ts` `navSystemContext` → `navSkills`（zh "技能" / en "Skills"）。
- 删除 `webui/src/SystemContextView.tsx`。
- 导航收敛为 `Tools | Runs | Metadata | Fetch | Executors | MCP | Cases | Skills | Settings`。
- 后端 `system_context_store` / `/api/system-context/*` 资源 store 与本变更无关（`SystemContextView` 本就未调用该 API），保留不动。
- 文档同步：`webui/README.md`、`webui/SPEC.md`、根 `README.md`、根 `SPEC.md`、`CLAUDE.md`、`docs/modules/README.md`。
- 验证：`npm run lint` / `typecheck` / `build` 全绿（bundle 322.27 KB）。

## 2026-06-23 Server 跨平台 (Linux/Windows) 与全工具 catalog

目标：server 包括所有 tools，兼容 Windows 和 Linux 双平台。

- **非测试代码已跨平台**：审计确认 server/native-agent 非测试代码无未守护的 Unix-only API；`tokio::signal::ctrl_c`、`tokio::process::Command`、`std::env::temp_dir()` 均跨平台。`exports.rs::is_executable` 已有 `#[cfg(unix)]`/`#[cfg(not(unix))]` 双分支。
- **测试模块 Windows 可编译**：`http/tools.rs`、`http/executors.rs` 整个测试模块依赖 bash 假工具 + Unix 可执行权限，改为 `#[cfg(all(test, unix))]`；`services/tool_runner.rs` 把 `PermissionsExt` 从模块级 `use` 移入 `#[cfg(unix)] fn write_tool`，3 个 bash 异步测试加 `#[cfg(unix)]`，纯解析测试仍全平台运行。
- **ssh_binary 默认值跨平台**：`default_ssh_binary()` 改为 Linux `/usr/bin/ssh`、Windows `C:\Windows\System32\OpenSSH\ssh.exe`；`examples/logagent.yaml`、`examples/server-test.yaml`、`deploy/logagent.example.yaml` 移除硬编码 `/usr/bin/ssh`，改用平台默认 + 注释。
- **全工具 catalog**：`examples/logagent.yaml` 新增 `tools:` 段，声明 `pprof_analyzer` + 4 个 analyzer（`flux_query_analyzer` / `influxql_analyzer` / `opengemini_storage_analyzer` / `influxdb_storage_analyzer`），全部 `enabled: false` + `path_env`，使配置在两平台无需外部二进制即可加载，catalog 即包含全部 12 个工具（5 configured + 7 built-in）。
- **Windows 工具构建脚本**：新增 `scripts/build-tools.ps1`，对应 `build-tools.sh`，构建 Go/Rust analyzer 到 `bin/tools/*.exe`。
- 验证：`cargo fmt --check`、`cargo check`、`cargo test -p logagent-server`（92 通过）全绿；**Windows 交叉编译校验通过**——`cargo check --target x86_64-pc-windows-gnu -p logagent-server`（非测试）与 `cargo check --tests --target x86_64-pc-windows-gnu -p logagent-server`（测试）均 Finished（仅原有 dead-code 警告，无 `std::os::unix` 错误）；`logagent-native-agent` 同样通过 Windows 交叉编译。运行时校验：`examples/logagent.yaml` 加载成功，`/api/tools` 返回 12 个工具。

## 2026-06-23 LocalToolHub 命名与 MCP P1 修复

- 产品可见名称从 LogAgent Tool Workbench 收敛为 `LocalToolHub`；WebUI 标题、Settings/MCP 页面文案、MCP `serverInfo.name` 和根/组件文档已更新。
- 保留 `logagent-server` crate/binary、`LOGAGENT_*` 环境变量、`logagent.*` tool id 和 `logagent://` resource URI 作为兼容 namespace，避免打断已有配置和外部客户端。
- 修复 HTTP MCP 配置开关：`mcp.enabled=false` 时 `/api/mcp` 返回 JSON-RPC error；stdio `mcp-serve` 继续在启动时拒绝服务。
- WebUI `McpView` 和 `SettingsView` 从旧 `/api/mcp/readonly` 切换到 `/api/mcp`，页面展示真实 catalog MCP tools/resources。
- 新增 `mcp_server::tests::http_mcp_respects_disabled_config` 覆盖 HTTP MCP 禁用行为。

## 2026-06-23 WebUI 工具页 / MCP 页中英双语

- `i18n.ts` 新增 `toolsCopy` + `mcpCopy`（zh-CN / en-US），覆盖工具目录、工具详情、运行记录、运行状态、pprof 结果、MCP stdio/tools/resources 等全部可见文案。
- `ToolsView`（ToolPluginsView）和 `McpView` 接收 `language` prop，按语言切换文案；App.tsx 透传 `language`。FetchView 暂未国际化（独立页面，后续按需）。
- 验证：`npm run lint` / `typecheck` / `build` 全绿（bundle 318.89→322.66 KB）。

## 2026-06-22 Documentation Pivot

- Reframed LogAgent from a Claude Code-backed analysis workbench into a local tools and MCP workbench.
- Updated root README/SPEC and AGENTS instructions to make Tools, MCP, artifacts, Metadata, Fetch, Executors and local deployment the primary product surface.
- Rewrote Server docs to guide slimming the existing Rust server instead of restoring the old V1 analysis architecture.
- Rewrote WebUI docs to make Tools/Runs/Metadata/Fetch/Executors/MCP/Settings the target navigation.
- Rewrote deploy and testing docs around single-machine Rust runtime and deterministic tool/MCP testing.
- Rewrote all owned `docs/modules/*` README/SPEC files so Analysis Agent, LLM Gateway and Agent Backends are optional automation/client integration rather than core runtime dependencies.
- Updated Chrome Extension and Native Agent docs as optional file import bridges.

## 2026-06-22 WebUI Tools-first 导航（阶段 1）

- 重排 WebUI 导航为 Tools-first：`Tools | Runs | Metadata | Fetch | Executors | MCP | Cases | SystemContext | Settings`，默认进入 Tools。
- 移除 header 的 LLM debug 开关（`/api/debug/llm`）；LLM 面向后续随服务端 fat 代码删除。
- 接入已有孤儿视图 `ExecutorsView`、`metadata/MetadataDashboard`；`ToolsView` 收敛为只渲染 tool plugins。
- 新增最小视图：`RunsView`（消费 `/api/tools/runs`，轮询非终态 run）、`McpView`（stdio 配置示例 + `/api/mcp/readonly` 的 tools/resources 只读预览）；`FetchView` 提升为顶层 nav。
- 降级 `OperationsView`（Analyze）：从导航移除，视图文件保留待阶段 5 删除。
- `appCopy` 精简：移除 LLM 文案与 Analyze/Memory nav 文案，补齐 Tools-first nav 文案；`analysisCopy` 保留供 `OperationsView`。
- 偏差：保留 SystemContext 为第 9 个导航项（核心 keeper，视图已存在），webui/README 的 8 项建议扩展为 9 项。
- 验证：`npm run lint`、`npm run typecheck`、`npm run build` 通过；构建产物 380KB → 329KB（OperationsView 不再打包）。

## 2026-06-22 HTTP API 收敛（阶段 2）

- 新增 `GET /api/runs`、`GET /api/runs/:run_id`、`GET /api/runs/:run_id/result`、`GET /api/runs/:run_id/artifacts`（`http/runs.rs`），统一 run history，支持 `?kind=` 与 `?limit=`。
- 新增 `GET /api/artifacts/*artifact_id`（`http/artifacts.rs`）：按 `<runId>/<relativePath>` 逻辑路径下载，`safe_join` 拒绝穿越，未知 runId 返回 404。
- 新增 `POST /api/mcp` 作为 HTTP JSON-RPC 入口（复用 `mcp_readonly::readonly_mcp`），与 `/api/mcp/readonly` 并存。
- `/api/tools/runs*` 保留为兼容别名；旧 `/api/sessions*`、`/api/tasks*`、`/api/debug/llm` 仅作迁移兼容，不新增能力。
- 已知缺口：`/api/runs` 暂只聚合 `task_store`（tool/remote_command/log_analysis）；FetchStore 的 fetch run 仍走 `/api/fetch/runs`，后续再合并。
- 验证：`cargo fmt --all --check`、`cargo check`、`cargo test --all`（172 通过，+2 新增）全绿。

## 2026-06-22 服务端解耦 ToolRun 路径（阶段 3）

- 探勘确认：ToolRun（RunTool 阶段）与 RemoteCommandRun（ExecuteRemoteCommand 阶段）本就通过 `task_store` 完成、早返回，不走 analysis_state；二者与 fat 模块的实际运行时耦合只有两处。
- 3.1 `pipeline/executor.rs` 错误处理：捕获 `task_kind`，仅 `LogAnalysis` 调用 `analysis_state::record_failure`；ToolRun/RemoteCommandRun 失败只经 `task_store.fail` 记录错误。
- 3.2 `sync_session_status` 对非 `LogAnalysis` 任务直接返回，ToolRun/RemoteCommandRun 路径不再静态调用 `session_store`（`sync_task_status` 本就 no-op，现显式跳过，为阶段 5 删除 session_store 铺路）。
- keeper 模块（http/tools、services/tools、services/tool_runner、services/fetch、http/runs、http/artifacts）本就不 import analysis_state/llm_gateway/agent_backend/session_store，grep 确认 0 命中。
- LogAnalysis 分支仍使用 analysis_state/llm/agent_backend（待阶段 5 删除），本阶段未改动。
- 验证：`cargo fmt --all --check`、`cargo check`、`cargo test --all`（172 通过）全绿。

## 2026-06-22 MCP 重设计为独立 stdio server（阶段 4）

- 新增 `server/src/mcp_server.rs`：面向外部客户端的独立 MCP server（无 `task_id` 依赖）。
  - `run_stdio(config)` stdio 入口；`handle_request`/`handle_http` 统一 JSON-RPC handler（单对象或批量）。
  - `tools/list` = `services::tools::descriptors` 过滤 runnable（与 `/api/tools` 同一 catalog）。
  - `tools/call` 同步运行目录工具：`build_tool_run_task` → `tasks.create` → `start_attempt` → `run_tool_task` → `succeed_tool_run`，产出 ToolRun 记录（进入 `/api/runs` 历史）；失败经 `tasks.fail` 记录。
  - `resources/list`+`resources/read` = skills / metadata-instances(+snapshots) / cases-recent / runs-recent / tools-catalog，无 domain-adapters、无 task-workspace artifacts。
  - 移除 agent-loop 耦合：无 `log_mcp_call` / `waiting_marker_tool` / `request_user_input` / `request_approval` / `analysis_state`。
- 抽取 `services::tools::build_tool_run_task` 共享 helper（HTTP `create_tool_run` 与 MCP `tools/call` 复用任务构造）；`http/tools.rs::create_tool_run` 改用之。
- `main.rs` 新增 `Command::McpServe`（→ `mcp-serve`，无参数）调 `mcp_server::run_stdio`；保留旧 `mcp --task-id --mode`（agent_backend 用，阶段 5 删除）。
- HTTP：`POST /api/mcp` → `mcp_server::http_mcp`（full，可运行工具）；`POST /api/mcp/readonly` 保留（WebUI 只读预览）。WebUI `McpView` stdio 配置示例更新为 `mcp-serve`。
- 已知依赖：`mcp-serve` 经 `AppState::new` 仍需 `LOGAGENT_CLAUDE_CODE_PATH` + LLM env（fat 配置强制），阶段 5 删除 claude_code/llm 配置块后解除。
- 验证：`cargo fmt --all --check`、`cargo check`、`cargo test --all`（173 通过，+1 `mcp_server` 单测）；stdio smoke：`mcp-serve` 的 `initialize`/`tools/list`/`resources/list` 正常，`tools/list` 为 runnable catalog，logs 走 stderr；旧 `mcp --task-id` 不回归（executor 测试仍绿）。

## 2026-06-22 删除 fat 代码（阶段 5）

- **Wave 1（HTTP 分析面）**：删除 `http/sessions.rs`、`http/tasks.rs`、`http/debug.rs`、`http/settings.rs` 及其路由与 mod 声明；移除 `/api/sessions*`、`/api/tasks*`、`/api/debug/llm`、`/api/settings/{llm,agent-backends,domain-adapters}*`。`pprof` 测试中遗留的 `/api/tasks` 断言已移除。
- **Wave 2（执行路径 + fat 模块 + 数据模型，Level 2 purge）**：
  - 删除 fat 模块（~8.8k 行）：`services/{llm_gateway,agent_backend,agent_contracts,domain_adapters}`、`stores/{analysis_state,session_store}`、旧 `mcp.rs`（task-bound MCP，被 `mcp_server.rs` 取代）。
  - 精简 `pipeline/executor.rs`：只保留 ToolRun + RemoteCommandRun 单阶段执行（无 agent loop、无 analysis_state）；`pipeline/mod.rs` 保留 extract/search/prepare（`logagent.preprocess_log_package` 工具依赖），删 generate/persist/render LLM 辅助。
  - 精简 `domain/models.rs`：purge `TaskKind::LogAnalysis`、`TaskStatus::Waiting*`、LogAnalysis-only `TaskPhase` 变体、`AnalysisMode`、`AnalysisLanguage`、`AnalysisSession*` 类型、`AnalysisResult`/`RootCause`/`Confidence`、`TaskRecord.analysis_mode/language`、`CreateTaskRequest`、`TaskListResponse`、`TaskArtifactsResponse`；`default_task_kind`→`ToolRun`。保留 `SystemContextScope::LogAnalysis` 变体（on-disk 兼容，仅删 match arm）。
  - 精简 `support/config.rs`：删 `llm`/`claude_code`/`analysis`/`embedding` 配置块 + 结构 + resolver + 默认值；新增 `ServerSettings.max_input_chars`（keeper 文本上限，从 llm 配置迁入）。`examples/logagent.yaml` 同步。
  - `app.rs`：删 `sessions/llm/agent_backends/domain_adapters` 字段。`main.rs`：删 `Command::Mcp`+`McpArgs`（保留 `mcp-serve`）。
  - `http/cases.rs`：case import 改为 manual-first（无 LLM 抽取）；删 `confirm_task_case` + task→case helper + `/api/tasks/:task_id/case` 路由。`http/mcp_readonly.rs`：删 `list_domain_adapters` 工具/资源。
  - `write_json_atomic` 移到 `support/fs_utils`（fetch + huawei_package_sync 共享，原经 agent_contracts）。
  - 删 `task_store` 的 `succeed`/`advance_phase`/`wait_for_user`/`wait_for_approval`/`resume_waiting`（dead after LogAnalysis 删除）。
  - WebUI：删 `OperationsView.tsx`（孤儿）；`i18n.ts` 删 `analysisCopy`+5 helper；`SettingsView.tsx` 精简为 external-MCP/exports 卡片（LLM/agent-backends/domain-adapters 面板删除）。
- 验证：`cargo fmt --check`、`cargo check`、`cargo test -p logagent-server`（91 通过）；`npm run lint`/`typecheck`/`build` 全绿（bundle 329→318.89 KB）。Smoke：server 无 `LOGAGENT_CLAUDE_CODE_PATH`/LLM env 即可启动，`/health` ok、`/api/tools` 7 工具、`POST /api/mcp` tools/list 5、`mcp-serve` stdio 正常。
- 残留：`services/metadata.rs` 中 ~35 dead-code 警告（retired analysis-agent 的 metadata-context-outline 子系统，与 keeper metadata 端点交织），留作后续 focused 清理（Wave C）。`SystemContextScope::LogAnalysis` 变体保留（on-disk 兼容）。

## Next Steps

- ✅ WebUI navigation pivot to Tools-first（阶段 1 完成）。
- ✅ OperationsView/analysisCopy 删除（阶段 5 Wave 2 完成）。
- ✅ Consolidate HTTP APIs around tools, runs, artifacts, metadata, fetch, executors, MCP and settings.（阶段 2 完成；fetch run 合并待后续）
- 清理 `services/metadata.rs` 的 metadata-context-outline dead code（Wave C）。
- Add a local-toolhub config example and deployment smoke.

## Verification

- `git diff --check`
- stale wording scan over owned docs; remaining hits are explicit non-goal,
  optional automation or migration-source wording
- `cd webui && npm run lint`
- `cd webui && npm run typecheck`
- `cd webui && npm run build`
- docs-only status review
