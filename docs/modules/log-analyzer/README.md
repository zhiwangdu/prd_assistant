# Log Analyzer 方案

## 实现建议

优先使用 Rust 实现。语言优先级：

```text
Rust -> C/C++ -> Go/Python/Java 等
```

日志解压、文件遍历、正则归一化、`rg` 调用和大文件处理都适合用 Rust 实现。已有 C/C++ 日志工具可以通过 Tool Runner 复用。

## 系统依赖

Log Analyzer 依赖 `rg`。启动时需要检查 `rg_path` 是否存在并可执行。

配置示例：

```yaml
log_analyzer:
  rg_path: "rg"
  context_lines: 50
  keywords:
    - error
    - exception
    - timeout
    - fail
    - failed
    - panic
    - fatal
    - refused
    - denied
    - verify
```

关键词必须配置化，不能写死在代码里。

Python V2 使用 `LOGAGENT_V2_GREP_KEYWORDS` 配置初始 grep 关键词，默认值与 Rust/V1 对齐：

```text
error,exception,timeout,fail,failed,panic,fatal,refused,denied,verify
```

`LOGAGENT_V2_MAX_GREP_MATCHES` 默认同样与 Rust/V1 对齐为 `200`。用户问题不会自动拆词进入初始 grep；需要追加关键词时由后续 `logagent.search_logs` 调用完成。

## 职责

Log Analyzer 负责把原始日志包压缩成 LLM 可消费的证据。

它同时是 Analysis Orchestrator 的日志证据提供方。除初始关键词扫描外，还支持 Server 根据 Claude MCP `logagent.search_logs` 发起受限的后续检索；检索范围只能位于当前 task workspace。后续检索不会覆盖初始 `grep_results.json`，而是写入稳定 `log_searches/logsearch_*.json` 并返回命中行正文、按关键词计数、未命中关键词和 Rust/V1 兼容的顶层 evidence refs，避免模型只根据计数误判异常类型。

核心产物：

- `manifest.json`
- `error_summary.json`
- `contexts.jsonl`
- `tool_inputs/index.json`
- `log_searches/<search_id>.json`

## 支持格式

- `.log`
- `.txt`
- `.zip`
- `.tar.gz`
- `.tgz`
- `.tar`

`.tar.gz` / `.tgz` 如果 gzip tar 解压失败，会自动按普通 `.tar` 再尝试一次；两种方式都失败才返回异常。

批量任务中，每个上传文件由 Server pipeline 解压到独立目录：

```text
extracted/<文件基名>/
```

普通文本上传和普通 archive 成员在 V2 manifest、grep、log slice 和 Tool Runner fallback 中也使用这个稳定逻辑路径；同名上传会按上传顺序追加 `_2`、`_3` 等后缀，避免多个上传内的 `app.log` 等同名路径互相覆盖。旧的裸文件名或 archive 原始成员路径在唯一匹配时仍可作为 `logagent.get_log_slice` 和显式 Tool Runner 输入 selector。

匹配以下包名的节点日志包使用专用预处理：

```text
<packageId>_<instanceId>_<nodeId>_<yyyy_MM_dd_HH_mm_ss_micros>_logs.tar.gz
```

`packageId`、`instanceId`、`nodeId` 使用 Rust/V1 的 ASCII 字母数字安全规则；时间戳 7 个分段必须满足 `4-2-2-2-2-2-6` 位数字宽度。

这类包按节点和采集时间展开，不再套文件基名目录：

```text
extracted/<nodeId>/<timestamp>/{tsdb,stream,agent}/...
```

archive 内可以存在一层或多层顶层包装目录，例如 `<package>/var/chroot/...`。`./`、`<package>/` 等目录项会被跳过；预处理只对普通文件在规范化后的路径组件中查找支持的日志路径前缀，而不是要求它必须位于 archive 根目录。

目录映射：

- `/var/chroot/gemini/log/tsdb/**` -> `tsdb`
- `/var/chroot/gemini/log/stream/**` -> `stream`
- `/home/Ruby/log/**` -> `agent`

如果一个匹配命名格式的节点日志包中没有任何文件落在上述三类目录下，EXTRACT phase 会失败并返回明确错误，避免把空 manifest 误判为成功解包。

日志轮转按目录语义处理：目录下所有普通文件都纳入对应 log group，不依赖 `.log`、`.log.gz` 或其他后缀。gzip 文件用 magic bytes 识别，初始 grep 和 `logagent.get_log_slice` 都透明解码；解码失败的 gzip 文件保留在 manifest 中并记录 warning，检索时跳过。

预处理还会生成 analyzer-ready 输入：

```text
tool_inputs/index.json
tool_inputs/log_text/<nodeId>/<timestamp>/<logGroup>.jsonl
tool_inputs/influxql_analyzer/<nodeId>/<timestamp>.jsonl
```

`log_text` JSONL 是 V1 风格的通用逐行文本流，按节点、采集时间和日志组聚合且不绑定具体 `toolIds`；`influxql_analyzer` JSONL 只包含能从 raw SQL、JSON 字段或 `key=value` 字段中明确提取 `query` / `sql` / `stmt` / `statement` 的记录，供 Tool Runner 优先传给 `influxql_analyzer`。

如果 Log Analysis Session 没有上传日志，Server pipeline 仍会创建 `raw/` 和 `extracted/` 目录，并生成 `session_text_input.json`、空文件列表的 `manifest.json` 和空 matches 的 `grep_results.json`，让 Analysis Orchestrator 可以基于用户问题、Metadata、Case 和后续交互继续运行。

## manifest

```json
{
  "taskId": "task_xxx",
  "uploadIds": ["upl_xxx"],
  "uploads": [
    {
      "uploadId": "upl_xxx",
      "filename": "pkg_instance_node_2026_06_16_09_58_02_561564_logs.tar.gz",
      "size": 4096,
      "rawPath": "artifacts/ws_xxx/artfile_xxx/pkg_instance_node_2026_06_16_09_58_02_561564_logs.tar.gz",
      "extractedDir": "extracted/node/2026_06_16_09_58_02_561564",
      "packageId": "pkg",
      "instanceId": "instance",
      "nodeId": "node",
      "packageTimestamp": "2026_06_16_09_58_02_561564",
      "nodeDir": "extracted/node/2026_06_16_09_58_02_561564",
      "logGroups": [{ "name": "tsdb", "fileCount": 2, "compressedFileCount": 1 }],
      "ignoredFileCount": 1,
      "ignoredPathSamples": ["wrapper/tmp/ignored.log"]
    }
  ],
  "files": [
    {
      "path": "extracted/node/2026_06_16_09_58_02_561564/tsdb/influxdb.log",
      "size": 2147483648,
      "uploadId": "upl_xxx",
      "instanceId": "instance",
      "nodeId": "node",
      "packageTimestamp": "2026_06_16_09_58_02_561564",
      "logGroup": "tsdb",
      "originalPath": "wrapper/var/chroot/gemini/log/tsdb/influxdb.log",
      "compressed": false
    }
  ]
}
```

V2 manifest 保留已有 `sizeBytes`、`sourceUploadId`、`artifactId` 等扩展字段，同时补齐 Rust/V1 的 upload summary、file-level package metadata 和 gzip 统计，便于 Agent 不读取完整文件也能判断节点包结构。

## rg 检索

关键词扫描由配置生成，例如：

```bash
rg -i "error|exception|timeout|fail|failed|panic|fatal|refused|denied|verify" extracted/
```

`grep_results.json` 和后续 `log_searches/*.json` 的每条 match 同时写入 V2 字段 `path` / `lineNumber` / `ref` 和 Rust/V1 兼容字段 `file` / `line` / `evidenceRef`。

上下文提取：

```bash
rg -i -C 50 "error|exception|timeout|fail|failed|panic|fatal|refused|denied|verify" extracted/
```

## 归一化策略

第一版不做复杂聚类，先做正则归一化 + 计数排序：

- 数字替换为 `<num>`
- UUID 替换为 `<uuid>`
- IP 替换为 `<ip>`
- 时间戳替换为 `<ts>`
- 过长随机片段替换为 `<token>`

## error_summary

```json
{
  "topPatterns": [
    {
      "pattern": "TimeoutException while connecting to <ip>:<num>",
      "count": 50,
      "examples": [
        "app.log:1234 TimeoutException while connecting to 10.0.0.1:6379"
      ]
    }
  ]
}
```

## MVP 限制

- 单文件默认上限 2GB。
- 单任务默认最多 20 个日志文件。
- 超限任务标记为 `FAILED`，提示用户拆分或调整配置。
