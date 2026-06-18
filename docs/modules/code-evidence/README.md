# Code Evidence 方案

## 实现建议

优先使用 Rust 实现。语言优先级：

```text
Rust -> C/C++ -> Go/Python/Java 等
```

代码证据模块涉及 git worktree 管理、`rg/git grep` 调用、路径约束和证据结构化，长期适合用 Rust 实现只读、安全的检索流程。V2 clean-room 分支当前按 Python/FastAPI 例外实现了只读 detached worktree 检索 MVP。

## 职责

Code Evidence 根据用户输入的软件产品和版本，定位对应代码分支或 tag，并结合实际代码生成证据链，供 Claude Code 做代码上下文分析。

Analysis Orchestrator 后续可根据 `code_investigation` mode 或 Claude MCP 请求执行新的关键词或符号检索。Server 必须把请求限制到 task 已确定的 product/version、配置仓库和 search roots。

## 当前状态

Python V2 已实现只读 worktree 检索 MVP：

- 管理员通过 `LOGAGENT_V2_CODE_REPOS_JSON` 配置本地 git repo、默认 ref、版本到 ref 映射和相对 search roots。
- 管理员可通过 `LOGAGENT_V2_CODE_WORKTREE_ROOT` 配置代码 worktree cache 根目录；未配置时默认使用 `data_dir/code_worktrees`。
- 管理员可通过 `LOGAGENT_V2_CODE_WORKTREE_MAX_PER_REPO` 控制每个 product
  保留的 detached worktree 数量；默认 5，非正值按 1 处理。
- Task MCP / Agent provider prompt 在存在配置仓库时广告 `logagent.search_code`。
- `logagent.search_code` 只接受配置内的 `product`、`version` 或受控 `gitRef`，用 `git rev-parse` 锁定 commit，再在 cache 根目录下创建或复用 detached `git worktree`，最后在该 worktree 内执行 `git grep` 做只读检索。
- 若当前 run 的 Session 绑定了 Metadata `instanceId`，且该 instance snapshot 带有
  `product` / `version`，`logagent.search_code` 会要求请求中的 `product` /
  `version` 与该上下文一致；未传 `version` 时自动继承 instance version，显式
  `gitRef` 也必须等于该 version 映射到的配置 ref。
- 结果写入当前 run 的 `code_evidence/<action_id>.json` evidence artifact，并记录 `repo.repoPath`、`worktree.path`、`worktree.commit` 和是否复用缓存；匹配行可作为最终答案 ref：`code_evidence/<action_id>.json#matches/<index>`。
- 同一 run 内相同 product/ref/keywords/maxMatches 请求会复用已有 artifact，不重复写入。
- 每次 search 创建或复用 worktree 后，V2 会更新该 worktree 目录 mtime 作为
  使用标记，并按 least-recently-used 清理同 product 下超过上限的旧 `wt_*`
  目录；当前 search 正在使用的 worktree 不会被删除，清理结果写入
  `worktree.cleanup` 供审计。

尚未实现启动孤儿 worktree 扫描、版本间 diff、commit 对比、函数级符号解析或 fix mode 代码修改。

## 输入示例

```json
{
  "actionId": "act_123",
  "product": "influxdb",
  "version": "3.0.2",
  "question": "为什么这个 Flux 查询在该版本上变慢？"
}
```

## 配置示例

当前 Python V2 通过环境变量配置：

```bash
export LOGAGENT_V2_CODE_REPOS_JSON='{
  "influxdb": {
    "repoPath": "/data/repos/influxdb",
    "defaultRef": "main",
    "versionRefs": {
      "3.0.2": "v3.0.2",
      "3.0.1": "v3.0.1",
      "2.7.8": "v2.7.8"
    },
    "searchRoots": ["query", "storage", "influxql", "flux"]
  }
}'
export LOGAGENT_V2_CODE_WORKTREE_ROOT=/data/logagent/code_worktrees
export LOGAGENT_V2_CODE_WORKTREE_MAX_PER_REPO=5
```

规划中的统一 YAML 形态仍保留 `code_repos` 概念，字段语义与环境变量一致。

## 流程

1. 根据 `product` 找到配置的代码仓；若任务绑定 Metadata instance，则先校验 product 与 instance 上下文一致。
2. 根据 `version` 找到 tag 或 branch；若任务绑定 Metadata instance 且请求省略 version，则自动使用 instance version。
3. 当前 V2 用 `git rev-parse <ref>^{commit}` 固化 commit。
4. 从显式 `keywords` 或 `query` 中提取关键词。
5. 在 `LOGAGENT_V2_CODE_WORKTREE_ROOT` 或默认 `data_dir/code_worktrees` 下创建/复用 detached `git worktree`，路径必须保持在 cache root 内。
6. 使用 `git grep` 在该 worktree 的配置 `searchRoots` 内检索。
7. 抽取相关文件、行号、命中正文和 evidence ref。
8. 生成 `code_evidence/<action_id>.json`。

## 关键词提取策略

MVP 使用规则优先的关键词提取，不依赖 LLM 生成检索词。

来源优先级：

1. 工具结果中的 `symbol`、`rule`、`operator`、`error_code`。
2. 日志上下文中的函数名、错误码、模块名。
3. 用户问题中的产品领域词，例如 `query`、`planner`、`compaction`、`write`。
4. 文件名、measurement、SQL/Flux 关键字。

处理规则：

- 去掉停用词和过短词。
- 保留 snake_case、CamelCase、错误码和带点模块名。
- 每个任务最多生成 20 个代码检索关键词。
- 每个关键词最多保留 Top 10 命中。

## Worktree 清理

当前 V2 已实现基础 worktree 创建和复用：

- cache root 来自 `LOGAGENT_V2_CODE_WORKTREE_ROOT`，未配置时为 `data_dir/code_worktrees`。
- worktree 按 repo path、product、ref 和 commit 生成稳定路径。
- 如果缓存路径存在且 `HEAD` 等于目标 commit，则复用。
- 如果缓存路径存在但不是目标 commit 或不是有效 worktree，则在确认路径没有逃出 cache root 后删除并重建。

V2 已实现每个 product 的 LRU 清理：

```yaml
code_evidence:
  worktree_root: "/data/logagent/code_worktrees"
  max_worktrees_per_repo: 5
  cleanup_policy: "least_recently_used"
```

清理策略：

- worktree 按 repo + ref 复用。
- 每次 search 会 touch 当前 worktree 目录作为最近使用标记。
- 超过上限时删除最近最少使用的同 product `wt_*` worktree。
- 当前 search 正在使用的 worktree 不删除。
- 启动时扫描孤儿 worktree 并记录告警仍是后续工作。

## 输出结构

```json
{
  "product": "influxdb",
  "version": "3.0.2",
  "ref": "v3.0.2",
  "actionId": "code_123",
  "commit": "6f2a...",
  "repo": {
    "product": "influxdb",
    "repoPath": "/data/repos/influxdb",
    "searchRoots": ["query", "storage"]
  },
  "worktree": {
    "mode": "git_worktree",
    "root": "/data/logagent/code_worktrees",
    "path": "/data/logagent/code_worktrees/influxdb/wt_abc123",
    "commit": "6f2a...",
    "reused": true,
    "maxPerRepo": 5,
    "cleanup": {
      "policy": "least_recently_used",
      "removedCount": 0,
      "remainingCount": 1
    }
  },
  "taskContext": {
    "instanceId": "inst-prod-1",
    "product": "influxdb",
    "version": "3.0.2"
  },
  "keywords": ["PushDownFilterRule"],
  "keywordCounts": {
    "PushDownFilterRule": 1
  },
  "matchCount": 1,
  "matches": [
    {
      "file": "query/planner/rules.go",
      "line": 214,
      "lineNumber": 214,
      "keyword": "PushDownFilterRule",
      "text": "...",
      "reason": "日志中出现 filter pushdown failed，与该规则相关",
      "ref": "code_evidence/code_123.json#matches/0"
    }
  ],
  "finalEvidenceAllowed": true
}
```

## 边界

- 代码仓由管理员预先配置和同步。
- 任务执行时只允许切到配置中允许的 ref。
- 绑定 Metadata instance 的任务中，MCP 请求不能绕过该 instance 的
  product/version 安全映射。
- 当前 V2 第一版只做 detached worktree 只读代码检索和证据引用，不 pull、不修改管理员配置的源代码仓。
- worktree cache 路径必须保持在配置 root 内；删除重建只允许发生在该 root 下。
- 不自动拉取陌生仓库，不自动修改代码。
- 后续可增加版本间 diff / commit 对比，用于定位回归。
- Claude Code 不能通过 MCP 请求改写 repo、ref、search root 或执行构建脚本；fix mode 的 Edit/Test 后续必须在隔离 worktree 中开放。
