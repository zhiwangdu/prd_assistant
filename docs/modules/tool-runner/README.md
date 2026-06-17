# Tool Runner 方案

## 实现建议

优先使用 Rust 实现。语言优先级：

```text
Rust -> C/C++ -> Go/Python/Java 等
```

Tool Runner 涉及进程执行、timeout、stdout/stderr 捕获和路径校验，适合用 Rust 做严格边界控制。已有 C/C++ 编译工具直接作为被调用工具接入。

## 职责

Tool Runner 负责调用已有编译好的诊断工具，并把工具输出转成 Claude Code 和最终结果可引用的结构化证据。Domain Adapter 负责说明某类系统适合哪些工具，Tool Runner 仍然只按 Server 白名单执行。

调用来源可以是初始规则、Tools 页面手动运行或 Claude MCP `logagent.run_domain_tool`，但都必须由 Server 映射到配置中的工具和参数模板。Claude Code 不能提供可执行路径或自由 argv。V2 `logagent.run_domain_tool` 同时接受新协议 `toolId` 和 V1 兼容的 `tool` / `inputFile`，且 `tools/list` schema 通过 `anyOf` 同时广告这两种调用形态；`inputFile` 会映射成受控的 `params.inputFiles`，只能解析当前 Workspace 中已知的 `extracted/...`、`tool_inputs/...` 或 manifest 文本路径。响应同时保留 V2 `result/artifact/evidence` 和 Rust/V1 顶层 `artifactPath`、`summary`、`evidenceRefs`，多输入工具额外返回 `artifactPaths`，有 findings 时返回最终答案可引用的 `finalEvidenceRefs`。V2 `tool_inputs/index.json` 现在还会在相关 storage analyzer 启用时，从直接上传或 archive 内安全抽取 `.tssp`、`.tssp.init`、`.tsm`、`.tsi`、TSI/mergeset 目录和 `_series` 目录，作为 `tool_inputs/storage/` 或 `tool_inputs/storage_dirs/` artifact 输入。Server 还提供内置 `logagent.preprocess_log_package`、`logagent.fetch` 和 `logagent.huawei_cloud_package_sync` runnable 工具；`pprof_analyzer` 的 catalog 按 Rust/V1 configured command 形态暴露，但在 V2 中保持 manual-only；预处理工具复用 Analyze 解压链路并生成 tool-ready inputs，Fetch 复用 `tool_run` / `tool_results` 展示和 evidence 机制但执行边界来自 `fetch` 配置、credential store 和 HTTP allowlist，Huawei sync 首版只支持 WebUI/API 手动运行，用于把单个上传包同步到 Huawei OBS 并校验/更新 GaussDB 记录。

当前 Server 已实现共享 `AgentAction`、`EvidenceArtifact`、`EvidenceProvider` 契约、`RUN_TOOL` phase 和 MCP tool 调用入口。Tool Runner MVP 作为 Server 内部 Rust 模块运行，当前由 Server 规则根据 `manifest.json` / `grep_results.json` 自动生成工具 action；Claude MCP `logagent.run_domain_tool` 复用同一个执行通道。

目标工具示例：

- `flux_query_analyzer`
- `influxql_analyzer`
- `opengemini_storage_analyzer`
- `influxdb_storage_analyzer`

## 配置示例

```yaml
tools:
  flux_query_analyzer:
    enabled: true
    path_env: LOGAGENT_TOOL_FLUX_QUERY_ANALYZER
    timeout_seconds: 30
    max_input_files: 3
    match:
      file_patterns:
        - "*.jsonl"
        - "*.ndjson"
      keywords:
        - "flux"
        - "\"query\""
        - "duration_ms"
    args:
      - "--input"
      - "{input_file}"
      - "--format"
      - "json"
      - "--top-k"
      - "20"
      - "--max-input-lines"
      - "100000"
      - "--max-error-findings"
      - "20"

  influxql_analyzer:
    enabled: true
    path_env: LOGAGENT_TOOL_INFLUXQL_ANALYZER
    timeout_seconds: 30
    max_input_files: 3
    match:
      file_patterns:
        - "*.jsonl"
      keywords:
        - "influxql"
        - "\"query\""
        - "select"
        - "show series"
    args:
      - "-input"
      - "{input_file}"
      - "-output"
      - "json"
      - "-detail-limit"
      - "5"
```

## 执行原则

- 只允许调用配置文件中声明的工具。
- 工具路径必须是绝对路径。
- 参数只允许使用预定义占位符。
- 使用参数数组执行，不拼接 shell 字符串。
- 每次执行必须设置 timeout。
- stdout、stderr、exit code、耗时都要保存。
- 工具失败不应导致整个任务失败，除非标记为必需。
- 只读 HTTP MCP 的工具目录和 `tools.zip` 导出不能触发 Tool Runner 执行，不能读取 API Key、环境变量值、Server 配置原文、workspace 数据或上传文件。
- 工具目录必须通过 descriptor 标记 `source/tags/readOnly/editable/exportable/runnable/paramsTemplate`；内置工具使用 `source=built_in`，只读、不可编辑、不可导出，是否支持页面手动运行由 `runnable` 决定。
- configured subprocess 工具按 Rust/V1 command descriptor 形态暴露：
  `source=configured`、`backend=command`、`readOnly=false`、`editable=true`、
  `exportable=enabled`、`minFiles=1`，并将 `acceptedSuffixes` 原样设置为
  `match.filePatterns`。
- `logagent.fetch` 使用 `source=built_in`、`backend=fetch`、不可导出、不可编辑、无需上传文件；只有 `fetch.enabled=true` 时才可运行。只读 HTTP MCP 可看到 descriptor，但不能执行该工具。
- `logagent.huawei_cloud_package_sync` 使用 `source=built_in`、`backend=huawei_cloud_package_sync`、不可导出、不可编辑、`minFiles=maxFiles=1`、`acceptedSuffixes=["*"]`；只有 `huawei_cloud.package_sync.enabled=true` 且 OBS/GaussDB 凭据环境变量解析成功时才可运行。它执行用户提交的 SQL，首版视受保护 Tools API 使用者为信任边界，不对 SQL 做业务语义限制。

## 当前实现状态

- 已实现 `server/src/tool_runner.rs`。
- 已支持配置解析、绝对路径校验、timeout、stdout/stderr 捕获、输出截断和幂等复用。
- 已支持 `{input_file}`、`{manifest_path}`、`{grep_results_path}`、`{workspace}`、`{action_id}` 占位符。
- 已支持固定 `path` 或环境变量 `path_env` 指定工具路径；固定 `path` 支持 `${ENV}` 展开；启用工具时最终路径必须是绝对路径。
- 已支持 `max_input_files` 控制单个工具在同一任务中最多处理的匹配输入文件数量，默认 1。
- 规则版 action 和 V2 task MCP 先使用显式 `inputFile` / `inputFiles`，再读取 `tool_inputs/index.json` 中声明给当前 toolId 的 materialized input；只要存在匹配项，就只使用这些 tool-ready 输入并受 `max_input_files` 限制，不再补充原始日志候选。V2 的 materialized input 覆盖 InfluxQL/Flux JSONL 和启用 analyzer 的 storage 文件/目录输入；后者从直接上传或 archive 内的 `.tssp`、`.tssp.init`、`.tsm`、`.tsi`、TSI/mergeset 目录和 `_series` 目录生成 artifact。没有显式输入且没有匹配 materialized input 时才按 manifest 文件模式匹配，并用 grep keyword 补充候选；每个 action id 包含工具名和输入文件稳定哈希，避免批量任务结果目录冲突。
- 已支持 `tool_results/<action_id>/result.json`、`stdout.txt`、`stderr.txt`。
- 已支持从工具 stdout JSON 中提取 `summary` 和 `findings`；stdout 不是 JSON 时保留原始输出并使用通用 summary，不影响任务成功。
- artifacts API 和 WebUI 能展示 tool result 与结构化 findings。
- LLM Gateway 会读取 Tool Runner summary/findings 并允许最终结果引用 `tool_results/<action_id>/result.json#findings/<index>`。
- 已新增 `examples/server-tools.yaml` 作为真实 `flux_query_analyzer` / `influxql_analyzer` 接入模板；默认启动配置仍不强依赖这些二进制。
- 已新增 `examples/server-influxql-tool.yaml` 作为单独验证真实 `influxql-analyzer` 的配置；该配置通过 `LOGAGENT_TOOL_INFLUXQL_ANALYZER` 指向构建产物。
- 已适配真实 `influxql-analyzer` Report stdout：`total_records`、`fingerprints`、`special_rules`、`parse_errors` 和 `realtime_query` 会标准化为 `ToolRunRecord.summary/findings`。
- 已增强真实 `influxql-analyzer` CompareReport stdout：`batch_a` / `batch_b`、`statement_delta`、`qps_delta`、`new_fingerprints`、`removed_fingerprints`、`changed_fingerprints` 和 `rule_deltas` 会转成可读 summary/findings，包含 count/qps A->B、delta、规则和 normalized query。
- 当前 `influxql-analyzer` 源码通过 `third_party/influxql` submodule 引用，默认跟踪 `git@github.com:zhiwangdu/influxql.git` 的 `influxql-analyzer` 分支；CLI 入口为 `cmd/influxql-analyze`，LogAgent 构建产物名固定为 `influxql-analyzer`。
- 当前 `flux_query_analyzer` 源码通过 `third_party/flux` submodule 引用，默认跟踪 `git@github.com:zhiwangdu/flux.git` 的 `feature/query-stats` 分支；CLI 入口为 `libflux/flux-core` 的 `query_stats`，LogAgent 构建产物名固定为 `flux_query_analyzer`。stdout JSON 已适配通用 `summary/findings` 提取，并通过 `--top-k`、`--max-input-lines` 和 `--max-error-findings` 控制输入和输出规模。
- 当前 `opengemini_storage_analyzer` 源码通过 `third_party/openGemini` submodule 引用，默认跟踪 `git@github.com:zhiwangdu/openGemini.git` 的 `openGemini-tools` 分支；CLI 入口为 `app/opengemini-storage-analyzer`，用于只读检查 TSSP 和 TSI mergeset 文件。
- 当前 `influxdb_storage_analyzer` 源码通过 `third_party/influxdb` submodule 引用，默认跟踪 `git@github.com:zhiwangdu/influxdb.git` 的 `influxdb-tools` 分支；CLI 入口为 `cmd/influxdb_storage_analyzer`，用于只读检查 TSM、TSI 和 `_series` 文件。
- 四个源码 submodule 的 Go module、CI/build image 或开发说明已统一到 Go 1.26；构建 source-built analyzers 的环境需要提供 Go 1.26 或可自动下载 Go 1.26 toolchain。
- 内网镜像环境可通过 `LOGAGENT_SUBMODULE_BASE_URL` 统一指定仓库 namespace，或通过 `LOGAGENT_SUBMODULE_INFLUXQL_URL`、`LOGAGENT_SUBMODULE_FLUX_URL`、`LOGAGENT_SUBMODULE_OPENGEMINI_URL`、`LOGAGENT_SUBMODULE_INFLUXDB_URL` 分别指定 clone URL。`scripts/build-tools.sh` 会先调用 `scripts/configure-tool-submodules.sh` 写入本地 Git submodule config，再按需初始化 submodule；不会修改提交中的 `.gitmodules`，也不会改写顶层仓库 `origin`。只有 submodule 已经是独立初始化的 Git worktree 时，脚本才会同步更新该 submodule 自身的 `origin`。
- Server 已新增 Tools API 和 `tool_run` task，用于用户在 WebUI 手动运行工具。`pprof_analyzer` 复用 Rust/V1 configured command catalog 形态和 workspace 产物目录，由 Tools 插件适配器固定调用 `go tool pprof` 并解析 top/tree/raw 结果；V2 结果同时保留 artifact id 映射和 Rust/V1-style `artifactPaths`（`top.txt`、`tree.txt`、`raw.txt`、`stderr.txt`、可选 `graph.svg`），并返回 `profileType`、`total` 和 top 表格。configured command tools 也可手动运行，Server 会先生成 `extracted/`、`manifest.json`、`grep_results.json` 和可能的 `tool_inputs/index.json`，也可以通过 `params.inputFiles` 显式复用已有 Workspace 输入，再按白名单 args 模板调用工具；内置 metadata tools 可无上传运行并返回 JSON result。
- 内置日志包预处理 tool 当前为 `logagent.preprocess_log_package`，支持批量 `.tar.gz` 上传，运行结果写入 `tool_results/<action_id>/result.json`，摘要包含节点、日志组、轮转 gzip、忽略文件和 materialized tool inputs。
- 内置 metadata tools 当前包括 `logagent.list_metadata_instances`、`logagent.get_metadata_snapshot`、`logagent.get_metadata_field_types` 和 `logagent.get_metadata_tag_fields`。其中 tag fields 工具复用 field types 的 instance/database/measurement/RP 定位规则，但不提供 `field` 参数，只返回 Tag 类型字段。
- 内置 Fetch tool 当前为 `logagent.fetch`，参数兼容 `endpointId` 和 V1 `fetchId`，并支持可选 string map `variables`、可选临时 string map `headers` 和可选 string `body` override。`variables` 只替换 endpoint URL 中的 `{name}` 占位符，并在替换后执行 allowlist 校验；临时 headers 只作用于本次请求且拒绝受控头。`logagent.list_fetch_endpoints` 在 Fetch 关闭时返回错误，开启时返回 Rust/V1 `schemaVersion=1`、endpoint summary 和 `finalEvidenceAllowed=false`。运行结果写入 `tool_results/<action_id>/result.json`，同时保存 bounded response body artifact 并提供逻辑 `tool_results/<action_id>/response_body.bin` 和实际 V2 artifact id/path；最终答案可引用 `tool_results/<action_id>/result.json#response`。
- 内置 Huawei package sync tool 当前为 `logagent.huawei_cloud_package_sync`，参数为可选 `objectKey`、必填 `updateSql` 和必填 `querySql`，接受任意一个已完成 upload 作为同步包。运行结果写入 `tool_results/<action_id>/result.json`，包含 OBS PUT/HEAD 状态、GaussDB affected rows、最多 200 行 query preview、失败步骤、耗时和凭据环境变量名；不保存原始 SQL 和密钥值。OBS/GaussDB 网络或 SQL 执行失败会写入 `status=FAILED` 的 result artifact，工具任务本身仍可成功完成以便 WebUI 展示错误细节。
- 只读 HTTP MCP 通过 `logagent://tools/catalog` 和 `logagent.list_tools` 暴露同一份工具目录、configured args、match rules 和内置 metadata 工具 descriptor；V2 对应 `logagent-v2://tools/catalog`，返回同样的 `schemaVersion`、完整 `tools` 和 V1-compatible `configuredTools` 形态；该入口不运行工具。
- `GET /api/exports/tools.zip` 会对当前 enabled 且解析为普通可执行文件的 configured 工具生成 Server 平台二进制快照、wrapper、示例配置和 `tools-manifest.json`。缺失、非普通文件、不可执行或读取失败的工具只在 manifest 中标记 skipped，不让下载失败；内置工具不进入导出包。

## 本地真实工具 smoke

```bash
export LOGAGENT_NATIVE_API_KEY=dev-token
export LOGAGENT_TOOL_FLUX_QUERY_ANALYZER=/abs/path/to/flux_query_analyzer
export LOGAGENT_TOOL_INFLUXQL_ANALYZER=/abs/path/to/influxql_analyzer
cargo run -p logagent-server -- --config examples/server-tools.yaml
```

`server-tools.yaml` 可配合 mock Claude CLI 单独验证 Tool Runner。上传 Flux 查询 NDJSON/JSONL（每行包含 `time`、`query` 和可选 `duration_ms`）或包含 `flux`、`"query"`、`duration_ms` 关键词的日志会触发 `flux_query_analyzer`；上传 `.jsonl` 或包含 `influxql`、`"query"`、`select`、`show series`、`show measurements` 关键词的日志会触发 `influxql_analyzer`。

只验证真实 Flux 工具时：

```bash
./scripts/smoke-flux-query-analyzer.sh
```

只验证真实 InfluxQL 工具时：

```bash
./scripts/smoke-influxql-analyzer.sh
```

`examples/server-influxql-tool.yaml` 使用 `path_env: LOGAGENT_TOOL_INFLUXQL_ANALYZER`。部署环境中 `deploy/rebuild-install.sh` 会把同一源码构建为 `$LOGAGENT_APP_DIR/bin/tools/influxql-analyzer`，`deploy/logagent.example.yaml` 默认指向该路径。

`influxql-analyzer` 输入应是 JSONL，每行至少包含 `query` 字段，可选 `timestamp` 或 `time`。CLI 参数使用真实工具协议：

```text
-input <file> -output json -detail-limit 5
```

只验证 storage analyzers 时：

```bash
./scripts/smoke-opengemini-storage-analyzer.sh
./scripts/smoke-influxdb-storage-analyzer.sh
```

`opengemini_storage_analyzer` 接受 `.tssp`、`.tssp.init`、TSI mergeset part 文件或目录，CLI 参数为 `--input {input_file} --format json`。`influxdb_storage_analyzer` 接受 `.tsm`、`.tsi` 或 `_series` 目录，CLI 参数为 `-input {input_file} -kind auto -max-samples 10`。二者 stdout 都输出可被通用 Tool Runner 解析的 `summary/findings` JSON。

验证 pprof Tools 页面：

```bash
export LOGAGENT_NATIVE_API_KEY=dev-token
export LOGAGENT_TOOL_PPROF_GO="$(command -v go)"
cargo run -p logagent-server -- --config examples/server-pprof-tool.yaml
```

访问 `http://127.0.0.1:50997/` 的 Tools 页面选择工具后按预填 JSON 参数模板运行。`pprof_analyzer` 上传 `.pprof`、`.prof`、`.profile` 或 `.pb.gz`；configured command tools 上传匹配文件并可在 `inputFiles` 中指定 `extracted/...` 或 `tool_inputs/...` 路径；metadata built-ins 不需要上传，`logagent.get_metadata_tag_fields` 的模板只需要 `instanceId`、`database`、`measurement` 和可选 `retentionPolicy`。该路径创建 `taskKind=tool_run` 的任务，结果通过 `/api/tools/runs/:task_id/result` 查询。

验证 Huawei package sync 时，需要在配置中启用 `huawei_cloud.package_sync` 并设置 OBS/GaussDB 环境变量；Tools 页面选择 `logagent.huawei_cloud_package_sync` 后上传一个包并填写 JSON 参数：

```json
{
  "objectKey": "packages/demo.tar.gz",
  "updateSql": "update package_table set object_key = 'packages/demo.tar.gz' where id = 'demo'",
  "querySql": "select id, object_key, status from package_table where id = 'demo'"
}
```

`objectKey` 为空时使用配置的 `object_prefix` 加上传文件名。结果从 `/api/tools/runs/:task_id/result` 读取；失败时重点查看 `failedStep` 和 `error`。

## 输出结构

工具 stdout 若为 JSON，Server 会尝试解析以下形态：

```json
{
  "summary": "发现 2 个可能导致慢查询的问题",
  "findings": [
    {
      "severity": "medium",
      "file": "query.log",
      "line": 120,
      "message": "filter 下推失败，可能导致扫描数据量过大"
    }
  ]
}
```

兼容字段：

- summary 可来自 `summary`、`message` 或 `title`。
- findings 数组可来自 `findings`、`issues` 或 `diagnostics`。
- finding 消息可来自 `message`、`summary`、`description`、`detail`、`title` 或 `cause`。
- severity 可来自 `severity`、`level` 或 `status`。
- file 可来自 `file`、`path` 或 `filename`。
- line 可来自 `line`、`lineNumber` 或 `startLine`。

`result.json` 标准化后结构：

```json
{
  "schemaVersion": 2,
  "tool": "flux_query_analyzer",
  "actionId": "act_123",
  "status": "OK",
  "exitCode": 0,
  "durationMs": 1234,
  "summary": "发现 2 个可能导致慢查询的 range/filter 顺序问题",
  "findings": [
    {
      "severity": "medium",
      "file": "query.log",
      "line": 120,
      "message": "filter 下推失败，可能导致扫描数据量过大"
    }
  ],
  "stdoutPath": "tool_results/act_123/stdout.txt",
  "stderrPath": "tool_results/act_123/stderr.txt"
}
```
