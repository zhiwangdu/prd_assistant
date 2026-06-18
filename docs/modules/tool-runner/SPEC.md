# Tool Runner Spec

## 目标

Tool Runner 负责按白名单调用已有外部分析工具，把结果标准化为任务证据，供 Domain Adapter、Claude Code 和最终结果引用。

## 当前状态

Server 已实现 Tool Runner MVP：配置白名单、规则版工具 action、Claude MCP `logagent.run_domain_tool`、可恢复 `RUN_TOOL` phase、timeout、stdout/stderr/result 持久化、stdout JSON 摘要解析和 artifacts API 展示。真实工具可通过固定 `path` 或 `path_env` 环境变量接入，固定 `path` 支持 `${ENV}` 展开。`examples/server-tools.yaml` 提供 `flux_query_analyzer`、`influxql_analyzer`、`opengemini_storage_analyzer` 和 `influxdb_storage_analyzer` 模板。当前四个工具源码通过 `third_party/` submodules 引用，并由 `scripts/build-tools.sh` 构建部署；构建阶段可通过 `LOGAGENT_SUBMODULE_BASE_URL` 或各 `LOGAGENT_SUBMODULE_*_URL` 覆盖 submodule clone 地址，适配内网镜像。

Server 也已实现面向 WebUI 手动执行的 Tools API。`tool_run` task 复用上传、workspace、TaskStore 和后台 Executor；创建手动 tool run 时会校验上传数量和上传文件名是否匹配工具 descriptor 的 `acceptedSuffixes`，而 `params.inputFiles` 可在不附带上传时复用已有 Workspace 输入。`pprof_analyzer` 通过 Rust/V1 configured command catalog 形态调用 Go 可执行文件的 `tool pprof` 子命令，但在 V2 中标记为 `manualOnly`，不进入任务 MCP `logagent.run_domain_tool` 或 OpenAI-compatible / binary Agent provider prompt 的可选 enum。configured command tools 会生成 `extracted/`、`manifest.json`、`grep_results.json` 和可能的 `tool_inputs/index.json` 后复用 ToolRunner，也可通过 reserved `params.inputFiles` 显式复用当前 Workspace 的已知输入。V2 分析任务现在在首轮 Agent provider 请求之前自动运行命中的 input-based configured subprocess 工具；需要运行时 `{params.name}` 或必填 params 的工具不会被自动运行，仍由 Agent/用户显式调用。自动结果会作为 `preRunToolResults`、`tool_result` evidence 和 `allowedEvidenceRefs` 写入 Agent 请求链路。V2 task MCP `logagent.run_domain_tool` 支持 `toolId` 新协议和 V1 兼容的 `tool` / `inputFile` 参数，并在 `tools/list` schema 中通过 `anyOf` 同时广告这两种形态；legacy `tool` / `inputFile` 调用使用 `act_mcp_tool_<stable_digest>` action id 并复用重复相同参数的结果；configured tool 默认 action id 使用 Rust/V1 `act_tool_<tool_id>` 前缀，input-file 运行追加稳定输入 hash；Agent provider prompt 的 `availableTools` 对同一工具使用同形 schema 和 enum。`inputFile` 会映射到同一显式输入选择逻辑；响应保留 V2 `result/artifact/evidence`，并补齐 Rust/V1 `artifactPath`、`summary`、`evidenceRefs`，多输入工具额外返回 `artifactPaths`，有 findings 时返回最终答案可引用的 `finalEvidenceRefs`。同一 run 内重复 `toolId + actionId` 会复用已有结果，避免 Agent 重试或人工复查时重复生成 artifact。V2 的 `tool_inputs/index.json` 已覆盖节点日志包通用 `log_text` JSONL、InfluxQL/Flux 查询 JSONL，以及在相关 analyzer 启用时从直接上传或 archive 内安全抽取的 storage analyzer 文件/目录输入（`.tssp`、`.tssp.init`、`.tsm`、`.tsi`、TSI/mergeset 目录和 `_series` 目录），并写成 `tool_inputs/storage/` 或 `tool_inputs/storage_dirs/` artifact。内置 metadata tools 可无上传运行。工具目录 descriptor 统一包含 `source/tags/readOnly/editable/exportable/runnable/paramsTemplate`。内置 metadata tools 包括 instance list、snapshot、field types 和 tag fields，其中 `logagent.get_metadata_tag_fields` 不接受上传或 `field` 参数，只返回 Tag 类型字段。
普通 configured subprocess 工具的 descriptor 与 Rust/V1 保持 command 语义：
`source=configured`、`backend=command`、`readOnly=false`、`editable=true`、
`exportable=enabled`、`minFiles=1`，`acceptedSuffixes` 原样来自
`match.filePatterns`。
Python V2 的 `LOGAGENT_V2_TOOLS_JSON` 兼容 descriptor array 和 Rust/V1
风格的 tool-id object map；每个 descriptor 可使用 V2 `command`，也可使用
V1 `path`、`path_env` / `pathEnv`，并接受 camelCase 或 snake_case limit
字段。Python V2 的 `LOGAGENT_V2_TOOL_*_ANALYZER` 快捷环境变量和 Rust/V1
`LOGAGENT_TOOL_*_ANALYZER` 别名会生成与 `examples/server-tools.yaml`
对齐的 configured descriptors；V2 专用变量优先生效。Flux/InfluxQL
查询工具使用 V1 示例 args、`timeoutSeconds=30` 和 `maxInputFiles=3`；
openGemini storage 使用完整 TSSP/TSI/mergeset 文件模式和
`maxInputFiles=10`；InfluxDB storage 使用 `timeoutSeconds=60`、
`maxInputFiles=5` 和 V1 TSM/TSI 文件模式。V2 在加载
`LOGAGENT_V2_TOOLS_JSON` 路径和这些 source-built analyzer 环境变量时会展开
`${ENV}` / `$ENV` 和 `~`；enabled 工具必须解析为绝对路径后才进入
工具 registry，disabled 描述可保留相对路径但不会 runnable 或 exportable。
`LOGAGENT_V2_TOOLS_JSON.id` 与 Rust/V1 `tools.<name>` 对齐，只允许非空
ASCII 字母、数字、`_` 和 `-`；内置 `logagent.*` 工具是固定 Server 能力，
不从该用户配置命名空间加载。
`LOGAGENT_V2_TOOLS_JSON.match.filePatterns` 和 `keywords` 在配置加载阶段
统一转小写，HTTP/MCP 工具目录输出与 Rust/V1 保持一致。
Configured subprocess 的自定义 `paramsSchema` 在运行前必须按 V1 常见 JSON
Schema 子集校验：`type`、`enum`、`oneOf` / `anyOf`、字符串长度、数值
min/max、数组 `items` / min/max items，以及嵌套对象 `required` /
`additionalProperties=false`。未知参数仍由顶层
`additionalProperties=false` 拒绝。

Server 还实现内置 `logagent.preprocess_log_package` 和 `logagent.fetch` runnable tools。预处理 tool 复用 Analyze 解压链路，按节点日志包生成 `tool_inputs` 和摘要 result；Fetch tool 复用 `tool_run`、Tools 目录和 `tool_results` artifact，但执行由 `fetch.enabled`、AES-256-GCM credential store、HTTP allowlist 和 reqwest 负责；二者都不导出到 `tools.zip`。

Server 还实现内置 Huawei package sync runnable tool `logagent.huawei_cloud_package_sync`。它复用 `tool_run`、Tools 目录和 `tool_results` artifact，执行由 `huawei_cloud.package_sync` 配置、Huawei OBS 签名 PUT/HEAD 和 GaussDB SQL 连接负责；首版仅支持受保护 Tools API 手动运行，不暴露为任务 MCP tool，也不导出到 `tools.zip`。V2 catalog descriptor 对齐 Rust/V1：display name 为 `Huawei OBS + GaussDB Package Sync`，tag 包含 `huawei-cloud`，`outputViews=["summary","obs","gaussdb","json"]`。

Server 还提供只读工具目录和工具包导出：

- V2 HTTP `/api/v2/tools`、`POST /api/mcp/readonly` 中的
  `logagent://tools/catalog` / `logagent-v2://tools/catalog` 和
  `logagent.list_tools` 返回同一份 catalog envelope：`schemaVersion`、完整
  `tools` descriptors、V1-compatible `configuredTools` summary 和
  `sourceBuiltAnalyzers` status；只读 MCP 不执行工具。目录中也包含
  `logagent.get_metadata_tag_fields`，configured
  command descriptor 的 `paramsSchema` 同时提供 Rust/V1 顶层
  `configuredArgs` / `match` 只读项和 V2 `properties` 镜像。
- `GET /api/v2/exports/tools.zip` 打包当前 enabled 且解析为普通可执行文件的 configured 工具二进制、enabled `pprof_analyzer` 的 Go executable、wrapper、示例配置和 `tools-manifest.json`；缺失、非普通文件、无执行权限或读取失败的工具在 manifest 标记 skipped，Fetch、Metadata、preprocess 和 Huawei sync 等没有独立可执行文件的内置工具不导出。普通 configured 工具示例使用 `LOGAGENT_V2_TOOLS_JSON` 形态，且 command 必须是绝对路径占位；`pprof_analyzer` 示例必须使用绝对路径形式的 `LOGAGENT_V2_PPROF_GO_COMMAND`，避免把 Go executable 当成 generic subprocess，也避免复制相对路径后被 V2 启动校验拒绝。
- `logagent.fetch` descriptor 可通过 `/api/tools`、`logagent://tools/catalog` 和 `logagent.list_tools` 看到；只读 HTTP MCP 必须拒绝 `tools/call logagent.fetch`，并对所有 catalog configured/manual built-in tool call 返回明确 readonly 错误。V2 `POST /api/v2/fetch/endpoints/:endpoint_id/runs` 排队 Fetch `tool_run`，可复用传入的 `workspaceId`，未传时自动创建隔离 workspace；`GET /api/v2/fetch/runs` 只读列出 `toolId=logagent.fetch` 的持久化 tool runs，并支持 `endpointId`、`fetchId`、V1 风格 `fetch_id`、`workspaceId` 和 `limit` 过滤。
- `PATCH /api/v2/fetch/endpoints/:endpoint_id` 必须基于 hydrate 后的完整 endpoint 合并 partial update，只把脱敏 URL/header/body 写入 `fetch_endpoints`，并刷新或删除 `fetch_credential_sets` 以匹配合并结果。
- `logagent.huawei_cloud_package_sync` descriptor 可通过 `/api/tools`、`logagent://tools/catalog` 和 `logagent.list_tools` 看到；只读 HTTP MCP 必须拒绝执行任何 catalog configured/manual built-in tool，包括只读但需要 run/workspace 上下文的 preprocess tool。

## 首批工具

- `flux_query_analyzer`
- `influxql_analyzer`，真实 CLI 已验证，源码来自 `third_party/influxql` 的 `cmd/influxql-analyze`，LogAgent 构建产物名为 `influxql-analyzer`，普通 Report 参数为 `-input <file> -output json -detail-limit 5`，CompareReport 参数为 `-input-a <baseline.jsonl> -input-b <candidate.jsonl> -output json -detail-limit 3`
- `opengemini_storage_analyzer`，源码来自 `third_party/openGemini` 的 `app/opengemini-storage-analyzer`，参数为 `--input <file> --format json`
- `influxdb_storage_analyzer`，源码来自 `third_party/influxdb` 的 `cmd/influxdb_storage_analyzer`，参数为 `-input <file> -kind auto -max-samples 10`
- `pprof_analyzer`，通过 `LOGAGENT_TOOL_PPROF_GO` 或 `LOGAGENT_V2_PPROF_GO_COMMAND` 指向 Go 可执行文件；V2 默认关闭，启用时 command 必须解析为绝对路径。catalog 按 Rust/V1 `source=configured` / `backend=command` 暴露，`paramsSchema` 同时包含 V1 顶层 `sampleIndex` / `nodeCount` / `generateSvg` 和 V2 `properties` 镜像；Server 固定调用 `go tool pprof -top/-tree/-raw`，默认 `nodeCount=50`，`sampleIndex` 只允许字母、数字、`_` 和 `-`，`generateSvg` 必须是 JSON boolean；top/tree/svg 传入 `-nodecount=<nodeCount>`，top/tree/raw/svg 都传入 `-symbolize=none`
- `logagent.huawei_cloud_package_sync`，受 `huawei_cloud.package_sync` 控制，上传一个已入库包到 Huawei OBS，执行 GaussDB update/query SQL，并写入 JSON result

## 输入

- Task workspace
- 工具名称
- `action_id`
- 工具参数模板
- 工具路径，来自固定 `path` 或 `path_env` 环境变量；固定 `path` 可使用 `${ENV}` 占位符
- 构建 source-built analyzers 时的 submodule clone URL override，来自 `LOGAGENT_SUBMODULE_BASE_URL` 或单仓库 `LOGAGENT_SUBMODULE_*_URL`；这些变量只影响本地 Git submodule 初始化，不进入 Server 运行时工具白名单，也不能改写顶层仓库 `origin`
- 工具 catalog `sourceBuiltAnalyzers` 固定报告四个 source-built analyzer ID
  的 registered/enabled/runnable/status 状态，作为部署观测字段，不作为执行入口。
- `max_input_files`，单个工具在同一任务中最多自动选择的输入文件数量，默认 1
- 日志片段、查询文本或 manifest 文件
- 可选显式输入选择：V2 task MCP top-level `inputFile`、`params.inputFiles` 或手动 tool_run `params.inputFiles`。这些路径必须是 workspace-relative，只能解析到当前 Workspace 的 manifest 文本路径、对应 `extracted/...` 虚拟路径或当前 run 的 `tool_inputs/...` entry，不能是任意本地路径。
- 可选 `tool_inputs/index.json`，由日志包预处理或 V2 初始证据 materializer 生成。Tool Runner 自动选择输入时优先使用声明给当前 toolId 的 materialized input；只要存在匹配项，就只使用这些 tool-ready 输入并受 `max_input_files` 限制。V2 materializer 会为节点日志包生成通用 `log_text` JSONL，为 InfluxQL/Flux 查询生成 JSONL artifact，也会在相关 analyzer 启用时为 openGemini/InfluxDB storage analyzers 从直接上传或 archive 内抽取 `.tssp`、`.tssp.init`、`.tsm`、`.tsi` 文件和 TSI/mergeset、`_series` 目录作为 `tool_inputs/storage/` 或 `tool_inputs/storage_dirs/` artifact。`log_text` 输入不绑定 `toolIds`，不会被自动分配给 configured analyzer。只有没有显式输入且没有匹配 materialized input 时，才回退到 manifest file pattern 和 grep keyword。
- V2 每次执行 configured subprocess 前会物化独立 tool workspace，并把
  subprocess `cwd` 设置到该目录。`{workspace}`、`{manifest_path}` 和
  `{grep_results_path}` 指向这个视图内的路径，而不是 artifact store 的内部目录。
  该 workspace 至少包含当前 run 的 `manifest.json`、`grep_results.json`，以及
  可选 `tool_inputs/index.json`。

## 输出

建议产物：

```text
tool_results/
  act_tool_flux_query_analyzer/
    result.json
    stdout.txt
    stderr.txt
  act_tool_influxql_analyzer/
    result.json
    stdout.txt
    stderr.txt
```

每个结果至少包含：

- `schema_version`
- `tool`
- `action_id`
- `status`
- `command`
- `exit_code`
- `duration_ms`
- `stdout_path`
- `stderr_path`
- `summary`
- `findings`

Python V2 configured subprocess result 还保留 additive 字段：
`toolId`、`displayName`、`params`、`argv`、`stdoutPreview`、`stderrPreview` 和
`parsedStdout`。V2 必须为 configured subprocess stdout/stderr 分别写入
bounded artifact，并在 result/evidence 中暴露 `stdoutArtifactId` /
`stderrArtifactId`；`stdout_path` / `stderr_path` 仍对应 Rust/V1 风格逻辑路径。
非 0 退出、timeout 和 subprocess 启动失败必须写成 `FAILED` / `TIMED_OUT`
result。

V2 run artifact 聚合必须把工具结果引用的非 evidence 支持产物列入
`supportArtifacts`，并在 task MCP `artifact_index` 中暴露同一批 artifact：
configured subprocess stdout/stderr、Fetch response body、`pprof_analyzer`
top/tree/raw/stderr/SVG 等都必须使用 Rust/V1 逻辑路径
`tool_results/<action_id>/...`，同时保留实际 V2 artifact id、content type、
sha256 和 size。支持产物必须标记 `source="support"`，不得被当作最终答案
evidence ref。

当 stdout 是 JSON 时，Tool Runner 会尽量提取：

- `summary` / `message` / `title`
- `findings` / `issues` / `diagnostics`
- finding 内的 `severity` / `level` / `status`
- finding 内的 `file` / `path` / `filename`
- finding 内的 `line` / `lineNumber` / `startLine`
- finding 内的 `message` / `summary` / `description` / `detail` / `title` / `cause`

上述字符串字段必须兼容 Rust/V1 parser：JSON number 转成字符串，JSON boolean
不作为字符串字段。
V2 configured subprocess 会在写入 `result.json` 前规整 `findings[].file`：
当工具输出的绝对路径指向当前输入 artifact 时，替换成 `inputFile` 的
workspace-relative 逻辑路径；当它指向输入目录下的子文件时，替换成
`<inputFile>/<relative-child>`。不属于当前输入的绝对路径保持原值，原始
stdout/stderr 仍作为 support artifact 保存。

真实 `influxql-analyzer` 的 Report stdout 会被专门适配：

- Report 识别与 Rust/V1 一致：stdout JSON 同时包含 `total_records`、
  `total_statements` 和 `fingerprints` key 即进入专门 parser；`fingerprints`
  不是数组时只跳过 fingerprint findings，不退回通用 parser。
- `total_records`、`records_in_window`、`total_statements`、`parse_error_count` 进入 summary。
- `special_rules` 进入 findings，例如 `large_limit`、`no_time_filter`、`group_by_high_cardinality_risk`、`meta_query`。
- `parse_errors` 进入 high severity findings。
- `realtime_query.non_realtime` / `unknown` 进入实时性分类 findings。
- 有规则命中的高频 fingerprint 进入低优先级 query statistics findings。

真实 `influxql-analyzer` CompareReport stdout 也会被专门适配：

- `statement_delta`、`qps_delta`、`batch_a` 和 `batch_b` 进入 summary。
- `new_fingerprints` / `removed_fingerprints` / `changed_fingerprints` 进入 findings，包含 statement type、count A->B、qps A->B、delta、rules 和 normalized query。
- `rule_deltas` 进入 findings，包含 rule、count A->B 和 qps A->B。
- 新增 fingerprint 和正向规则增长默认 high severity，移除 fingerprint 默认 low severity。
- 真实 CLI smoke 必须覆盖 `-input-a` / `-input-b` compare mode；当 `removed_fingerprints` 或 `changed_fingerprints` 为 `null` 时，V2 parser 应按空列表处理并继续解析新增 fingerprint 和规则 delta。

真实 `flux_query_analyzer` stdout JSON 会被专门适配：

- 若 stdout 已包含通用 `summary` / `findings`，V2 保持工具输出原样作为 summary 和 findings。
- 若只包含 `metrics`、`topQueries` 和 `parseErrors`，V2 会从 `metrics.totalRows`、`parseSuccessCount`、`uniqueTemplateCount`、`newTemplateCount`、`parseErrorCount`、`queriesWithDuration` 和 `globalLatencyMs.p95` 生成 summary。
- `parseErrors[]` 进入 high severity findings。
- `topQueries[]` 进入模板 finding，包含 count、ratio、p95、fingerprint 和 normalized query；p95 >= 1000ms 标记 high，p95 >= 200ms 标记 medium。
- `metrics.newTemplateCount` 大于 0 时生成 medium severity baseline/template finding。

真实 storage analyzers stdout JSON 也走通用 parser：

- `opengemini_storage_analyzer` 检查 TSSP 和 TSI mergeset 文件/目录。
- `influxdb_storage_analyzer` 检查 TSM、TSI 和 `_series` 文件/目录。
- 二者只读，不修复输入数据，不接受自由 argv。

stdout 不是 JSON 或字段不匹配时，不判定为工具失败，只保留 stdout/stderr 并生成通用 summary。

LLM Gateway 会读取 result artifact 中的 summary/findings。finding 的最终答案引用格式固定为：

```text
tool_results/<action_id>/result.json#findings/<index>
```

Fetch response 的最终答案引用格式固定为：

```text
tool_results/<action_id>/result.json#response
```

该引用只允许用于当前任务中真实存在且 `tool=logagent.fetch` 的 action；未知 action 或非 Fetch action 必须拒绝。

Huawei package sync 的 `result.json` 至少包含：

- `toolId=logagent.huawei_cloud_package_sync`
- `actionId`
- `status=OK|FAILED`
- `summary`
- `failedStep` 和 `error`
- `input.uploadId/filename/size/rawPath`
- `obs.endpoint/bucket/objectKey/url/put/head`
- `gaussdb.host/port/database/user/sslmode/updateAffectedRows/queryRows/queryRowsTruncated`
- `sql.updateSqlProvided/updateSqlLength/querySqlProvided/querySqlLength`
- `timings`
- `credentialMetadata` 中的环境变量名，包含 V1 `gaussdbPasswordEnv=null`
  和 V2 `gaussdbDsnEnv=LOGAGENT_V2_HUAWEI_GAUSSDB_DSN`

OBS `url` 必须使用 Rust/V1 virtual-hosted bucket 形态并按 object key path
segment 编码；OBS HEAD `contentLength` 有值时必须是数字。

原始 SQL、OBS access key/secret key/security token、GaussDB password 不得写入 `result.json`。

## 安全约束

- 只能调用配置白名单里的工具。
- 启用工具必须在配置加载阶段解析出绝对路径；禁用工具不读取
  `path_env`，V2 的 disabled JSON descriptor 也不会 runnable 或 exportable。
- 参数必须由模板和结构化输入生成，不能拼接任意用户命令。
- 工具执行需要超时和输出大小限制。
- 工作目录限制在 task workspace 或只读工具目录。
- Claude Code 只能通过 `logagent.run_domain_tool` 选择允许的工具、受控 workspace-relative 输入和结构化参数，不能传入任意命令、本地路径或环境变量。
- 只读 HTTP MCP 和 `tools.zip` 导出不能运行 Tool Runner，不能导出 API Key、环境变量值、Server 配置原文、workspace 数据或上传文件；只读 HTTP MCP 对 catalog configured/manual built-in tool call 必须返回 readonly 拒绝错误；内置工具必须标记为只读、不可编辑、不可导出，是否可手动运行由 descriptor 的 `runnable` 决定。
- Fetch endpoint 默认关闭；启用后 `LOGAGENT_V2_FETCH_SECRET_KEY` 必须是有效 Fernet 32-byte base64 key，`fetch.allowed_hosts` / `LOGAGENT_V2_FETCH_ALLOWED_HOSTS` 必须非空，只允许访问 allowlist 中的 `http/https` 目标。条目支持 `host`、`host:port` 和 scheme-specific `http(s)://host[:port]`；URL 形式会固定 scheme 和端口，省略端口时使用默认端口。默认不跟随 redirect，只有 endpoint `followRedirects=true` 时才按上限逐跳跟随；每个 redirect hop 重新校验 allowlist，跨 host 不转发 Authorization/Cookie，所有 sensitive header/query/body 值必须以 Rust/V1 兼容 `<redacted>` 脱敏展示并加密持久化，URL query 和 form-style body preview 中按标准编码显示为 `%3Credacted%3E`。
- V2 Fetch endpoint 必须迁移为 endpoint-level `schemaVersion=2`，并持久化 `refreshPolicy.mode=manual_only`；自动 token refresh policy 当前不支持，任何非 `manual_only` refresh mode 必须在保存前拒绝。
- 更新 sensitive endpoint 时必须先 hydrate 原 credential set，再合并 PATCH 字段；缺少有效 `LOGAGENT_V2_FETCH_SECRET_KEY` 时不得写入更新后的 endpoint row。
- V2 Fetch cURL import accepts copied bash commands with an optional leading
  `$` shell prompt, matching the Rust/V1 import tolerance, while still rejecting
  unsupported flags instead of broadening the network or filesystem boundary.
  Supported safe flags cover URL, request method, headers, body, cookies,
  User-Agent, Referer, compression, HEAD, and location. `--url` / `--url=...`
  set the endpoint URL but must still reject any second positional URL.
  `--user-agent` / `-A` and `--referer` / `-e` map to ordinary headers and
  remain subject to Server header validation. `--location` maps to endpoint
  `followRedirects=true`.
- Huawei package sync 默认关闭；启用后 OBS endpoint 必须是 `http/https` 且不含 path/query/fragment，bucket 只允许字母、数字、`.` 和 `-`，OBS access key、secret key、可选 security token 和 V2 GaussDB DSN 必须来自环境变量并在启动时校验。用户只能引用 Server 已完成 upload，不能传本地路径或 URL；OBS `objectKey` 和默认 `object_prefix` 必须是相对 key，不能包含 `..`、空 path segment、反斜杠、`?`、`#` 或控制字符。该工具会执行受保护 API 使用者提交的 SQL，首版不对 SQL 做表名或语句类型白名单。

## 验收标准

- 配置不存在的工具不可调用。
- `path_env` 缺失、为空或解析出非绝对路径时启动失败。
- 工具超时后任务记录失败原因。
- stdout/stderr 可追溯。
- `/api/v2/runs/:run_id/artifacts` 和 task MCP `artifact_index` 必须能发现
  工具支持产物，包括 configured subprocess `stdout.txt` / `stderr.txt`、
  Fetch `response_body.bin` 和 pprof top/tree/raw/stderr/SVG 输出，并保持
  `finalAllowed=false`。
- JSON stdout 中的 summary/findings 会写入 result artifact；非 JSON stdout 不影响任务成功。
- Tool findings 中指向当前输入 artifact 的绝对 `file` 路径必须规整为
  workspace-relative 逻辑路径，避免最终答案 evidence 暴露本机 artifact/tmp
  路径；原始 stdout/stderr support artifacts 仍保留审计原文。
- Flux stdout 即使缺少通用 `summary/findings`，也必须能从
  `metrics/topQueries/parseErrors` 生成 summary 和可引用 findings。
- Flux、InfluxQL、openGemini storage 和 InfluxDB storage smoke 脚本必须能从 submodule 源码构建或复用对应真实工具，并验证 stdout JSON 的 tool id、summary 或关键 finding；InfluxQL smoke 必须同时覆盖普通 Report 和 CompareReport。
- 四个 source-built analyzer submodule 的 Go module 和显式 CI/build image 基线保持在 Go 1.26；本地或部署构建环境必须提供 Go 1.26，或启用 Go toolchain 自动下载能力。
- `scripts/build-tools.sh` 和 `scripts/configure-tool-submodules.sh` 必须支持用环境变量或 CLI 参数把四个工具 submodule clone URL 写入本地 Git config，并保持 `.gitmodules` 默认 GitHub 地址和顶层仓库 `origin` 不被修改。若 submodule 目录只是父仓库内的未初始化目录，脚本不得对该目录执行 `remote set-url origin`。
- Tool finding evidence ref 可被 LLM 最终结果引用并通过 Gateway 校验。
- `pprof_analyzer` 手动运行必须创建 `tool_run` task，action id 使用 Rust/V1 前缀 `act_tool_pprof_analyzer_<run_id>`，成功后 `/api/tools/runs/:task_id/result` 返回 profile type、total、top 表格、`error`、`durationMs`、`createdAt`、Rust/V1-style `artifacts` / `artifactPaths`（top/tree/raw/stderr/SVG 逻辑路径）和 V2 `artifactIds` 映射；top/tree/raw 都成功时 status 才是 `OK`，SVG 失败只进入 warnings。
- V2 `/api/v2/tools/runs/:run_id/result` 在 tool run 未成功前必须返回
  HTTP 409 并带当前 status；成功后必须保留 V2 `run` / `artifact` /
  `result` 对象，同时补齐 Rust/V1-compatible 顶层 `runId`、`toolId` 和
  `resultPath`，方便迁移调用方不解析嵌套 run。
- 内置 metadata 工具必须出现在工具目录中并标记 `source=built_in` / `backend=builtin` / `readOnly=true` / `editable=false` / `exportable=false` / `runnable=true`，tag 包含 `read-only` / `manual-run`，并支持无上传手动运行；manual tool run 结果必须保留 V2 `value`，并补齐 Rust/V1 `params`、`result`、`durationMs` 和 `createdAt` 包装字段；metadata action id 必须使用 Rust/V1 `act_tool_metadata_<tool_id_sanitized>_<run_id>` 前缀，并按 V1 规则把 `.` 等分隔符归一为 `_`；`logagent.get_metadata_field_types` 的 `paramsTemplate` 必须包含 `retentionPolicy` 和 `field=[]`，`logagent.get_metadata_tag_fields` 的 `minFiles=maxFiles=0`，`paramsTemplate` 必须包含 `retentionPolicy` 且不包含 `field`，结果只包含 Tag 字段。
- `logagent.fetch` 必须出现在工具目录中并标记 `source=built_in` / `backend=fetch` / `readOnly=false` / `exportable=false` / `editable=false` / `minFiles=maxFiles=0`；tag 必须包含 `manual-run`，`paramsTemplate` 使用 V1 `fetchId` 主形态并包含 `body=null`，`outputViews=["summary","request","response","body_artifact"]`。fetch 关闭时 `runnable=false`，开启时 WebUI Fetch 子页和任务 MCP 可运行。参数必须兼容 `endpointId` 和 V1 `fetchId`，并支持可选 string map `variables`、临时 string map `headers` 和 string `body` override；URL `{name}` 变量替换后必须重新执行 allowlist 校验，临时 headers 必须拒绝受控头。
- `logagent.list_fetch_endpoints` 在 Fetch 关闭时必须返回错误；开启时必须返回 Rust/V1 `schemaVersion=1`、enabled endpoint summaries、endpoint-level `schemaVersion=2` / `refreshPolicy`、`fetchId`、`urlTemplate`、`credentialVersion` 和 `finalEvidenceAllowed=false`。
- `logagent.preprocess_log_package` 必须出现在工具目录中并标记 `source=built_in` / `backend=builtin` / `readOnly=true` / `exportable=false` / `editable=false` / `runnable=true`，descriptor 描述必须包含 rotated log normalization，`outputViews=["summary","nodes","log_groups","tool_inputs","warnings"]`；支持 1..100 个 `.tar.gz` / `.tgz` 上传，创建手动 tool run 时必须拒绝不匹配的上传文件名。结果必须包含 V1-style `nodes` 聚合、`manifestPath`、`grepResultsPath`、`toolInputsPath`、`toolInputs`、`durationMs` 和 `createdAt`，并保留 V2 `nodePackages` 原始列表以及 artifact id/path 字段；`nodes[]` 必须从 manifest upload summary 聚合 ignored file count、package warnings 和 compressed log group count。
- 任务 MCP `logagent.fetch` 必须按规范化参数生成稳定 `act_fetch_<digest>` action id；同一任务中重复相同参数必须返回相同逻辑 `tool_results/<action_id>/result.json#response` 引用，排队的 API/手动 Fetch `tool_run` 必须使用 Rust/V1 `act_fetch_<run_id>` action id。task MCP 响应必须保留 V2 `result/artifact/evidence`，并补齐 Rust/V1 顶层 `artifactPath`、`statusCode`、`httpOk`、`bodyPreview` 和 `evidenceRefs`。Fetch result 必须使用 Rust/V1 `schemaVersion=3` tool result envelope，包含 `exitCode=null`、`command=[]`、`inputFile=null`、空 `stdoutPath` / `stderrPath`、`findings=[]`、`evidenceRefs=["tool_results/<action_id>/result.json#response"]`，并包含 redacted request、status code、duration、redacted response headers、body preview、body artifact path、truncated 标记、credential version 和 `httpOk`；HTTP 4xx/5xx 不导致 task failed。
- `logagent.huawei_cloud_package_sync` 必须出现在工具目录中并标记 `source=built_in` / `backend=huawei_cloud_package_sync` / `exportable=false` / `editable=false` / `minFiles=maxFiles=1` / `acceptedSuffixes=["*"]` / `outputViews=["summary","obs","gaussdb","json"]`，display name 为 `Huawei OBS + GaussDB Package Sync`，tag 包含 `huawei-cloud`；配置关闭时 `runnable=false`，开启时 WebUI Tool plugins 可手动运行。
- Huawei package sync 必须只接受一个已完成 upload，生成安全 OBS object key，流式上传包，不把密钥或原始 SQL 写入 artifact；OBS/GaussDB 失败必须写入 `status=FAILED`、`failedStep` 和 `error`。
- Configured command tools 必须在 enabled 时 `runnable=true`，通过 `paramsTemplate.inputFiles` 显式输入或按 match rules 自动选择 `extracted/...` 文件，不允许用户传入任意 argv。
- Configured command tools 的 `paramsTemplate.inputFiles` 可显式输入 manifest 文本路径、`extracted/...` 或 `tool_inputs/...` workspace 相对路径；V2 WebUI 手动 `tool_run` 在 Params JSON 提供有效 `inputFiles` 时不得强制新上传文件；V2 task MCP 和 OpenAI-compatible / binary Agent provider prompt 都必须兼容 V1 `tool` + `inputFile` 调用形态，并在工具 input schema 中广告该兼容形态。
- 规则版 action 选择必须先使用 `tool_inputs/index.json` 中匹配 toolId 的 materialized inputs；如果存在匹配项，不得再补充 manifest 或 grep 候选。同一工具最多生成 `max_input_files` 个 action。只有没有匹配 materialized input 时，才按 manifest file pattern 优先、grep keyword 补充候选。
- V2 自动 `RUN_TOOL` 阶段必须在初始 evidence 生成之后、首轮 Agent provider 请求之前运行命中的 input-based configured subprocess 工具；未命中时跳过，命中时必须把 finding refs 注入 `analysis_package.allowedEvidenceRefs` 和 `agent_request.allowedEvidenceRefs`。
- V2 启用的 storage analyzer 文件和目录输入必须能从直接上传或 zip/tar/tar.gz/tgz 中安全识别并写入 `tool_inputs/index.json`；archive 路径必须经过逃逸校验，单次抽取受 `LOGAGENT_V2_MAX_ARCHIVE_FILES` 和 `LOGAGENT_V2_MAX_ARCHIVE_BYTES` 限制。
- 同一工具的不同输入文件必须生成不同稳定 action id。
- 重复 action id 幂等，结果可回填到同一分析 revision。
- 未配置或未匹配工具时 `RUN_TOOL` 阶段直接跳过，不影响现有 Claude Code 分析结果。
- `tools.zip` 覆盖 configured 可执行文件打包、wrapper/config 示例生成、enabled `pprof_analyzer` Go executable 打包、pprof 专用环境变量示例、缺失工具 skipped、sha256/size manifest，并验证没有独立可执行文件的内置工具不会进入 manifest。
- `/api/v2/tools`、只读 MCP tool catalog resource 和 `logagent.list_tools`
  必须返回同一份 V1-shaped catalog payload，包含 `schemaVersion`、`tools` 和
  `configuredTools`、`sourceBuiltAnalyzers`。
- README 和 SPEC 在工具协议或结果结构变更时同步更新。
