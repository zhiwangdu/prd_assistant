# Testing Spec

## 目标

用低成本测试覆盖 MVP 闭环，优先验证真实上传、解压、grep 和 WEBUI 依赖接口。

## 当前状态

已有：

- `testing/fixtures/downloads/sample.log`
- Server 单测覆盖 `.tar` 和 `.tar.gz` 解压。
- Server 单测覆盖 Upload Store 持久化、重启续传、损坏记录、严格 offset/size 校验和完成状态。
- Server 单测覆盖小文件和批量 multipart 上传的 flush-before-persist 行为。
- Server 单测覆盖 Metadata task context 推导、冲突校验、artifact 持久化和 LLM Prompt。
- Server 单测覆盖 Task Store 持久化/恢复、幂等 pipeline 和任务 API 状态码。
- Server 单测覆盖 Action/Evidence JSON 契约、安全 artifact 路径、expected phase 推进以及从 `SEARCH_LOGS` / `GENERATE_RESULT` 恢复。
- Server Upload API 并发单测使用进程内原子序号生成临时目录，避免并发测试之间清理对方 payload。
- Server 单测覆盖 mock Claude SDK adapter 端到端结果、Prompt 裁剪、响应解析和 evidence ref 校验。
- Server 单测覆盖 LLM evidence ref 行号/索引范围规范化和无法映射引用拒绝。
- Server 单测覆盖字符串形式 root cause 的内嵌 `evidenceRefs` 抽取和规范化。
- Server 单测覆盖单字符串形式 `missingInformation` 规范化为字符串数组。
- Server 单测覆盖 LLM schema 修正重试提示和字段级解析错误消息。
- Server 单测覆盖 AgentDecision / FinalAnswer 双模式解析、裸最终结果 JSON 和常见最终结果包裹变体包装、mock adapter action decision 和未开放 action 拒绝。
- Server 单测覆盖 action decision schema 修正重试提示，防止真实模型首轮缺少顶层 `type` 时直接失败。
- Server 单测覆盖 `PLAN_ANALYSIS` 多轮 mock Claude SDK `search_logs` action、action keywords 驱动的 grep 重建、重复 fingerprint 防护和预算终止结果。
- Server 单测覆盖 `WAITING_FOR_USER` message API 恢复任务，以及 `WAITING_FOR_APPROVAL` approval API 写入 mock environment evidence 后恢复任务。
- Server Task API 并发单测使用进程内原子序号生成临时目录，避免并发测试之间清理对方 workspace。
- Server 单测覆盖 Analysis State Store state/event 持久化和 `/api/tasks/:task_id/analysis`。
- Server 单测覆盖 Agent backend call lifecycle event、callId 和 adapter error details。
- Server 单测覆盖 LLM Gateway runtime response logging debug 开关的默认关闭和切换。
- Server 单测覆盖静态 LLM 模型名、`model_env` 优先级以及缺失/空环境变量校验。
- Server 单测覆盖纯 JSON、JSON 代码围栏、自然语言包裹的唯一 JSON object 和多个 JSON object 拒绝。
- Server 单测覆盖 Tool Runner 配置校验、规则版多输入文件选择、稳定 action id、fake tool 执行、timeout、dispatcher `RUN_TOOL` 阶段和 artifacts API。
- Server 单测覆盖真实 `influxql-analyzer` Report stdout 到 Tool Runner summary/findings 的转换、compare report 的基础 delta findings，以及 `flux_query_analyzer` 缺少通用 `summary/findings` 时从 `metrics/topQueries/parseErrors` 生成 summary/findings 的 fallback parser。
- Server 单测覆盖 Tools API、`pprof_analyzer` 手动 `tool_run` task、fake `go tool pprof` 执行和 pprof top 文本解析。
- Server 单测覆盖 Tool Runner 固定 `path` 的 `${ENV}` 展开、`path_env`、`max_input_files` 解析、缺失/空 env 拒绝以及禁用工具不读取 env。
- Server 单测覆盖 Remote Executor API、执行机创建、白名单模板发现、`remote_command_run` task、fake ssh 执行、result API，以及 `/api/tasks` 不混入 remote command run。
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
- `GET /api/tasks/:task_id/result`
- `GET /api/tasks/:task_id/analysis`
- `POST /api/tasks/:task_id/messages`
- `POST /api/tasks/:task_id/actions/:action_id/decision`

## 验收标准

- 新增归档格式必须有解压测试。
- 新增 API 必须有 smoke 验证方式。
- WebUI 新增交互必须通过 lint、typecheck 和 build；Task execution loop 摘要、backend callId 展示和 LLM debug 开关属于 WebUI 回归检查范围。
- 任务持久化变更必须覆盖损坏 JSON、启动恢复、终态保护和 artifacts 状态约束。
- Executor 变更必须覆盖每个已实现 phase 的中断恢复和陈旧 phase 推进拒绝。
- Tool Runner 变更必须覆盖白名单、timeout、stdout/stderr、幂等和 artifacts 暴露。
- Tool Runner 真实工具 smoke 使用 `examples/server-tools.yaml` 和 `LOGAGENT_TOOL_*` 路径环境变量；单工具验证使用 `scripts/smoke-flux-query-analyzer.sh`、`scripts/smoke-influxql-analyzer.sh`、`scripts/smoke-opengemini-storage-analyzer.sh` 和 `scripts/smoke-influxdb-storage-analyzer.sh`。这些 smoke 必须通过 `scripts/build-tools.sh` 初始化源码，并支持 `LOGAGENT_SUBMODULE_BASE_URL` 或单仓库 `LOGAGENT_SUBMODULE_*_URL` 指向内网 clone 地址；自动测试不得依赖预装真实工具二进制。
- pprof Tools smoke 使用 `examples/server-pprof-tool.yaml` 和 `LOGAGENT_TOOL_PPROF_GO="$(command -v go)"`，自动测试使用 fake Go 脚本。
- Remote Executor 真实 smoke 使用 WebUI `Tools / Executors` 新增 `root@112.74.50.120:22`，运行内置 `smoke_ls_root`，只执行低风险 `ls -la /root`；自动测试使用 fake ssh 脚本。
- `influxql_analyzer` compare mode parser 必须覆盖 batch summary、fingerprint delta 和 rule delta 的结构化 findings。
- `flux_query_analyzer` parser 必须覆盖没有通用 `summary/findings` 的 stdout，
  仍能从 `metrics/topQueries/parseErrors` 生成 summary 和结构化 findings。
- 产品闭环 smoke 使用 `scripts/smoke-product-loop.sh`，覆盖上传、真实 InfluxQL Tool Runner、Case 保存和下一任务 `caseContext` 召回。
- 上传持久化变更必须覆盖 payload/记录不一致、未完成上传和重启后的续传 offset。
- multipart 上传变更必须覆盖单文件和批量路径，防止 `COMPLETE` 记录先于 payload flush。
- Agent/LLM 自动测试必须使用 mock Claude SDK adapter、stub provider 或纯解析测试，不依赖外网、真实密钥或付费请求。
- LLM evidence ref 变更必须保证最终结果只保存 canonical `grep_results.json#matches/<index>`。
- WEBUI 修改至少跑 lint、typecheck 和 build。
- Analysis Agent 必须覆盖多轮、追问、审批、预算、重复动作和重启恢复。
- README 和 SPEC 在测试策略或 fixture 变更时同步更新。
