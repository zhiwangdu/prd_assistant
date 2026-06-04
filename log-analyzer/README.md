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

## 职责

Log Analyzer 负责把原始日志包压缩成 LLM 可消费的证据。

核心产物：

- `manifest.json`
- `error_summary.json`
- `contexts.jsonl`

## 支持格式

- `.log`
- `.txt`
- `.zip`
- `.tar.gz`
- `.tgz`
- `.tar`

## manifest

```json
{
  "files": [
    {
      "path": "redis.log",
      "size": 2147483648,
      "modifiedAt": "2026-05-30T10:00:00Z"
    }
  ]
}
```

## rg 检索

关键词扫描由配置生成，例如：

```bash
rg -i "error|exception|timeout|fail|failed|panic|fatal|refused|denied|verify" extracted/
```

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
