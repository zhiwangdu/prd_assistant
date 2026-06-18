# Code Evidence 方案

## 实现建议

优先使用 Rust 实现。语言优先级：

```text
Rust -> C/C++ -> Go/Python/Java 等
```

代码证据模块涉及 git worktree 管理、`rg/git grep` 调用、路径约束和证据结构化，长期适合用 Rust 实现只读、安全的检索流程。V2 clean-room 分支当前按 Python/FastAPI 例外实现了只读 `git grep` MVP。

## 职责

Code Evidence 根据用户输入的软件产品和版本，定位对应代码分支或 tag，并结合实际代码生成证据链，供 Claude Code 做代码上下文分析。

Analysis Orchestrator 后续可根据 `code_investigation` mode 或 Claude MCP 请求执行新的关键词或符号检索。Server 必须把请求限制到 task 已确定的 product/version、配置仓库和 search roots。

## 当前状态

Python V2 已实现只读 `git grep` MVP：

- 管理员通过 `LOGAGENT_V2_CODE_REPOS_JSON` 配置本地 git repo、默认 ref、版本到 ref 映射和相对 search roots。
- Task MCP / Agent provider prompt 在存在配置仓库时广告 `logagent.search_code`。
- `logagent.search_code` 只接受配置内的 `product`、`version` 或受控 `gitRef`，用 `git rev-parse` 锁定 commit，再用 `git grep <commit>` 做只读检索。
- 若当前 run 的 Session 绑定了 Metadata `instanceId`，且该 instance snapshot 带有
  `product` / `version`，`logagent.search_code` 会要求请求中的 `product` /
  `version` 与该上下文一致；未传 `version` 时自动继承 instance version，显式
  `gitRef` 也必须等于该 version 映射到的配置 ref。
- 结果写入当前 run 的 `code_evidence/<action_id>.json` evidence artifact，匹配行可作为最终答案 ref：`code_evidence/<action_id>.json#matches/<index>`。
- 同一 run 内相同 product/ref/keywords/maxMatches 请求会复用已有 artifact，不重复写入。

尚未实现独立 worktree/cache、版本间 diff、commit 对比、函数级符号解析或 fix mode 代码修改。

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

```yaml
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
```

规划中的统一 YAML 形态仍保留 `code_repos` 概念，字段语义与环境变量一致。

## 流程

1. 根据 `product` 找到配置的代码仓；若任务绑定 Metadata instance，则先校验 product 与 instance 上下文一致。
2. 根据 `version` 找到 tag 或 branch；若任务绑定 Metadata instance 且请求省略 version，则自动使用 instance version。
3. 当前 V2 用 `git rev-parse <ref>^{commit}` 固化 commit，不 checkout、不 pull、不创建 worktree。
4. 从显式 `keywords` 或 `query` 中提取关键词。
5. 使用 `git grep <commit>` 在配置的 `searchRoots` 检索。
6. 抽取相关文件、行号、命中正文和 evidence ref。
7. 生成 `code_evidence/<action_id>.json`。

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

以下是后续独立 worktree/cache 的规划配置，当前 V2 MVP 尚未实现：

```yaml
code_evidence:
  worktree_root: "/data/logagent/code_worktrees"
  max_worktrees_per_repo: 5
  cleanup_policy: "least_recently_used"
```

清理策略：

- worktree 按 repo + ref 复用。
- 超过上限时删除最近最少使用的 worktree。
- 正在被任务使用的 worktree 不删除。
- 启动时扫描孤儿 worktree 并记录告警。

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
    "searchRoots": ["query", "storage"]
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
- 当前 V2 第一版只做只读代码检索和证据引用，不 checkout、不 pull、不修改代码仓。
- 不自动拉取陌生仓库，不自动修改代码。
- 后续可增加版本间 diff / commit 对比，用于定位回归。
- Claude Code 不能通过 MCP 请求改写 repo、ref、search root 或执行构建脚本；fix mode 的 Edit/Test 后续必须在隔离 worktree 中开放。
