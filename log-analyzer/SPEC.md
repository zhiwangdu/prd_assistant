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
- 递归生成 `manifest.json`
- 按关键词生成 `grep_results.json`

## 输入

- Server 上传产物 `raw_path`
- Task workspace `extracted_dir`
- `log_analyzer.keywords`
- `log_analyzer.max_matches`

## 输出

`manifest.json`：

```json
{
  "uploadId": "upl_xxx",
  "taskId": "task_xxx",
  "source": "upload",
  "filename": "sample.log",
  "sourceUrl": "webui-smoke",
  "files": [
    { "path": "sample.log", "size": 159 }
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

- 压缩包路径必须通过 safe join，禁止 `../` 或绝对路径逃逸。
- grep 只读取文件内容，不执行文件。
- 单任务最多保留 `max_matches` 条命中。

## 验收标准

- `.zip`、`.tar.gz`、`.tgz`、`.tar` 都能解包。
- 纯 tar 文件即使命名为 `.tar.gz`，也能 fallback 解包。
- 路径逃逸压缩包被拒绝。
- manifest 路径使用 `/` 分隔。
- README 和 SPEC 在支持格式或产物结构变更时同步更新。
