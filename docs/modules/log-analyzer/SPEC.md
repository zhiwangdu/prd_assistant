# Log Analyzer Spec

## 目标

Log Analyzer 把上传文件展开为统一 workspace 结构，生成文件清单和初始 grep 证据。

## 当前状态

已作为 Server 内部 Rust 模块实现：

- 解压 `.zip`
- 解压 `.tar.gz`
- 解压 `.tgz`
- 解压 `.tar`
- `.tar.gz` / `.tgz` 解压失败后 fallback 到普通 `.tar`
- 普通 `.log` / `.txt` 复制到 `extracted/`
- 匹配 `<packageId>_<instanceId>_<nodeId>_<yyyy_MM_dd_HH_mm_ss_micros>_logs.tar.gz` 的节点日志包按 `extracted/<nodeId>/<timestamp>/{tsdb,stream,agent}/` 展开；archive 内允许顶层包装目录和 `./` 等目录项，普通文件路径中出现 `var/chroot/gemini/log/{tsdb,stream}` 或 `home/Ruby/log` 即可分类
- 节点日志包内三类日志目录下的所有普通文件都作为轮转日志纳入，不依赖文件名后缀；gzip 内容通过 magic bytes 识别并在 grep / 日志切片时透明解码
- 节点日志包若不包含任何支持日志目录，EXTRACT phase 失败并返回明确错误，不生成空成功 manifest
- 为后续工具生成 `tool_inputs/index.json`、通用 `log_text` JSONL 和 `influxql_analyzer` JSONL
- 递归生成 `manifest.json`
- 按关键词生成初始 `grep_results.json`
- Claude MCP `logagent.search_logs` 后续检索写入独立 `log_searches/logsearch_*.json`，返回 `matches[].text`、`keywordCounts`、`unmatchedKeywords` 和稳定 `log_searches/...#matches/<index>` evidence refs，不覆盖初始 `grep_results.json`；MCP 响应同时提供 Rust/V1 兼容的顶层 `artifactPath`、`totalMatches`、`matches` 和 `evidenceRefs`
- 支持没有上传文件的文本问题分析；此时 Server 仍写入 `session_text_input.json`，`manifest.files`、`manifest.uploads` 和 grep `matches` 为空。

规划扩展：

- 继续扩展 Claude MCP `logagent.search_logs` 参数，支持正则、文件 glob 和上下文行数。

## 输入

- Server 上传产物 `raw_path`
- Task workspace `extracted_dir`
- `log_analyzer.keywords`
- `log_analyzer.max_matches`

无上传的 Session task 输入为空，但仍提供 Task workspace；Log Analyzer 对空 `extracted_dir` 返回空清单和空 grep 结果。节点日志包预处理只保留路径中匹配 `/var/chroot/gemini/log/tsdb`、`/var/chroot/gemini/log/stream` 和 `/home/Ruby/log` 的三类目录，archive 顶层可额外包一层或多层目录，目录项会跳过；其他普通文件忽略并在 manifest upload summary 中记录数量和样例路径。

## 输出

`manifest.json`：

```json
{
  "uploadId": "upl_xxx",
  "taskId": "task_xxx",
  "source": "upload",
  "filename": "sample.log",
  "sourceUrl": "webui-smoke",
  "toolInputsPath": "tool_inputs/index.json",
  "uploads": [
    {
      "uploadId": "upl_xxx",
      "filename": "pkg_instance_node_2026_06_16_09_58_02_561564_logs.tar.gz",
      "extractedDir": "extracted/node/2026_06_16_09_58_02_561564",
      "instanceId": "instance",
      "nodeId": "node",
      "packageTimestamp": "2026_06_16_09_58_02_561564",
      "logGroups": [{ "name": "tsdb", "fileCount": 2, "compressedFileCount": 1 }]
    }
  ],
  "files": [
    {
      "path": "node/2026_06_16_09_58_02_561564/tsdb/influxdb.log",
      "size": 159,
      "nodeId": "node",
      "logGroup": "tsdb",
      "originalPath": "var/chroot/gemini/log/tsdb/influxdb.log"
    }
  ]
}
```

`tool_inputs/index.json`：

```json
{
  "schemaVersion": 1,
  "generatedBy": "log_package_preprocessor",
  "inputs": [
    {
      "path": "tool_inputs/log_text/node/2026_06_16_09_58_02_561564/tsdb.jsonl",
      "inputKind": "log_text_jsonl",
      "scope": "log_group",
      "nodeId": "node",
      "recordCount": 1200,
      "sourceFiles": ["extracted/node/2026_06_16_09_58_02_561564/tsdb/influxdb.log"]
    },
    {
      "path": "tool_inputs/influxql_analyzer/node/2026_06_16_09_58_02_561564.jsonl",
      "inputKind": "influxql_jsonl",
      "scope": "package",
      "toolIds": ["influxql_analyzer"],
      "nodeId": "node",
      "recordCount": 12,
      "sourceFiles": ["extracted/node/2026_06_16_09_58_02_561564/tsdb/influxdb.log"]
    }
  ]
}
```

`grep_results.json`：

```json
{
  "keywords": ["error", "timeout"],
  "totalMatches": 2,
  "matches": []
}
```

## 安全约束

- 通用压缩包路径必须通过 safe join，禁止 `../` 或绝对路径逃逸；节点日志包允许 archive 内绝对路径但会先规范化并拒绝 `..`、Windows drive、symlink/hardlink 和特殊文件。
- grep 只读取文件内容，不执行文件。
- action 不能指定 workspace 外路径，正则复杂度和执行时间受限。
- 单任务最多保留 `max_matches` 条命中。

## 验收标准

- `.zip`、`.tar.gz`、`.tgz`、`.tar` 都能解包。
- 纯 tar 文件即使命名为 `.tar.gz`，也能 fallback 解包。
- 节点日志包按 V1 文件名规则解析：package/instance/node id 只能是 ASCII 字母数字，时间戳必须满足 `yyyy_MM_dd_HH_mm_ss_micros` 分段宽度。
- 节点日志包按 nodeId/timestamp/logGroup 展开，archive 顶层包装目录和 `./` 目录项不会阻止识别日志路径，轮转 gzip 文件可被 grep 和日志切片读取。
- 节点日志包没有任何支持日志目录时任务失败并给出明确错误，不产生空成功 manifest。
- `tool_inputs/index.json` 中每个节点日志组都有通用 `log_text_jsonl` 输入；该输入不绑定具体 `toolIds`，供显式后续工具或人工排查复用。
- `tool_inputs/index.json` 中的 `influxql_analyzer` 输入只包含明确提取出查询文本的记录；节点日志包记录必须保留 V1 `line` / `logGroup` 字段，并可额外提供 V2 `lineNumber`。
- 路径逃逸压缩包被拒绝。
- manifest 路径使用 `/` 分隔。
- 无上传文本问题分析生成空 manifest 和 `totalMatches=0` 的 grep 结果。
- README 和 SPEC 在支持格式或产物结构变更时同步更新。
- 相同 action id 重试不会生成不一致结果。
