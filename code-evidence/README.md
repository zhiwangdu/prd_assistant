# Code Evidence 方案

## 实现建议

优先使用 Rust 实现。语言优先级：

```text
Rust -> C/C++ -> Go/Python/Java 等
```

代码证据模块涉及 git worktree 管理、`rg/git grep` 调用、路径约束和证据结构化，适合用 Rust 实现只读、安全的检索流程。

## 职责

Code Evidence 根据用户输入的软件产品和版本，定位对应代码分支或 tag，并结合实际代码生成证据链。

## 输入示例

```json
{
  "product": "influxdb",
  "version": "3.0.2",
  "question": "为什么这个 Flux 查询在该版本上变慢？"
}
```

## 配置示例

```yaml
code_repos:
  influxdb:
    repo_path: /data/repos/influxdb
    default_ref: main
    version_refs:
      "3.0.2": "v3.0.2"
      "3.0.1": "v3.0.1"
      "2.7.8": "v2.7.8"
    search_roots:
      - query/
      - storage/
      - influxql/
      - flux/
```

## 流程

1. 根据 `product` 找到配置的代码仓。
2. 根据 `version` 找到 tag 或 branch。
3. 使用 `git worktree` 或只读 checkout 准备代码目录。
4. 从日志错误模式、工具 findings、用户问题中提取关键词。
5. 使用 `rg` 或 `git grep` 在 `search_roots` 检索。
6. 抽取相关文件、行号、函数名和上下文。
7. 生成 `code_evidence.json`。

## 输出结构

```json
{
  "product": "influxdb",
  "version": "3.0.2",
  "ref": "v3.0.2",
  "repoPath": "/data/repos/influxdb",
  "findings": [
    {
      "file": "query/planner/rules.go",
      "line": 214,
      "symbol": "PushDownFilterRule",
      "reason": "日志中出现 filter pushdown failed，与该规则相关",
      "snippet": "..."
    }
  ]
}
```

## 边界

- 代码仓由管理员预先配置和同步。
- 任务执行时只允许切到配置中允许的 ref。
- 第一版只做代码检索和证据引用。
- 不自动拉取陌生仓库，不自动修改代码。
- 后续可增加版本间 diff / commit 对比，用于定位回归。
