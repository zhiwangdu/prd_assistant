# Code Evidence Spec

## 目标

Code Evidence 根据用户输入的软件产品和版本，定位到管理员配置的代码 ref，收集与日志现象相关的代码证据，供 Claude Code 引用。

## 当前状态

Python V2 已实现只读 worktree 检索和文件级 diff MVP：

- 通过 `LOGAGENT_V2_CODE_REPOS_JSON` 配置本地 git repo、默认 ref、版本到 ref 映射和相对 search roots。
- 通过可选 `LOGAGENT_V2_CODE_WORKTREE_ROOT` 配置代码 worktree cache 根目录；未配置时使用 `data_dir/code_worktrees`。
- 通过可选 `LOGAGENT_V2_CODE_WORKTREE_MAX_PER_REPO` 配置每个 product cache
  保留的 detached worktree 数量；默认 5，非正值按 1 处理。
- Task MCP 和 OpenAI-compatible / binary provider prompt 在存在配置仓库时广告 `logagent.search_code` 和 `logagent.diff_code`。
- `logagent.search_code` 使用 `git rev-parse` 固化 commit，再创建或复用 cache root 下的 detached `git worktree`，更新当前 worktree 目录 mtime 作为使用标记，按 least-recently-used 清理同 product 下超过上限的旧 `wt_*` worktree，最后在该 worktree 内执行 `git grep` 检索；不会 pull 或修改管理员配置的源代码仓。
- 准备 search worktree 时，V2 会扫描同 product cache root 下的 `wt_*`
  目录，记录不是有效 git worktree 或未注册到当前 repo 的 orphan，结果写入
  `worktree.cleanup.orphanScan`；首版只记录告警，不自动删除 orphan。
- `logagent.diff_code` 使用配置内的 `baseVersion` / `targetVersion` 或受控 `baseGitRef` / `targetGitRef`，用 `git rev-parse` 固化 base/target commit，在管理员配置的 repo 内执行只读 `git diff --numstat <base> <target> -- <searchRoots>`，返回最多 50 个文件级变更摘要；不会创建 writable worktree、pull 或修改源码仓。
- 如果当前 run 所属 Session 绑定了 Metadata `instanceId`，且 snapshot
  `instance.product` / `instance.version` 存在，Code Evidence 工具必须把
  请求限制在该 product 上；`logagent.search_code` 的 `version` 和
  `logagent.diff_code` 的 `targetVersion` 省略时继承 instance version，
  显式 target `gitRef` 必须等于该 version 的配置 ref。`logagent.diff_code`
  的 base 可选择更早的配置 version/ref。
- 结果写入当前 run 的 `code_evidence/<action_id>.json` artifact。search
  记录 `repo.repoPath`、`worktree.root`、`worktree.path`、`worktree.commit`、
  `worktree.reused`、`worktree.maxPerRepo` 和 `worktree.cleanup`，
  `matches[].ref` 可作为最终答案 evidence ref；diff 记录 base/target ref、
  commit、`diffs[]` 文件变更摘要，`diffs[].ref` 可作为最终答案 evidence ref。

尚未实现符号级解析、patch hunk / AST diff 和 fix mode 代码修改。

## 输入

- `product`
- search: `version`，可选 `gitRef`，必须等于配置的 `defaultRef` 或 `versionRefs` 中的值
- search: `keywords[]` 或 `query`
- search: `maxMatchesPerKeyword`，按 1..10 裁剪
- diff: `baseVersion` / `baseGitRef`
- diff: `targetVersion` / `targetGitRef`
- diff: `maxFiles`，按 1..50 裁剪
- 管理员配置的本地 repo、版本映射和 search roots

## 处理流程

```text
metadata instance product/version guard
  -> search:
       version -> branch/tag/ref mapping
       git rev-parse <ref>^{commit}
       create or reuse detached worktree under code worktree root
       git grep under configured searchRoots inside that worktree
       extract file/line/text evidence refs
       write code_evidence/<action_id>.json
  -> diff:
       base/target version -> branch/tag/ref mapping
       git rev-parse <baseRef>^{commit} and <targetRef>^{commit}
       git diff --numstat <baseCommit> <targetCommit> -- <searchRoots>
       extract file-level diff refs
       write code_evidence/<action_id>.json
```

## 输出

```text
code_evidence/<action_id>.json
```

当前字段：

- `product`
- search: `version`
- search: `ref`
- search: `commit`
- diff: `operation=diff`
- diff: `baseVersion`
- diff: `targetVersion`
- diff: `baseRef`
- diff: `targetRef`
- diff: `baseCommit`
- diff: `targetCommit`
- `repo.product`
- `repo.repoPath`
- `repo.searchRoots`
- search: `worktree.mode`
- search: `worktree.root`
- search: `worktree.path`
- search: `worktree.commit`
- search: `worktree.reused`
- search: `worktree.maxPerRepo`
- search: `worktree.cleanup.policy`
- search: `worktree.cleanup.removedCount`
- search: `worktree.cleanup.remainingCount`
- search: `worktree.cleanup.removed[]`
- search: `worktree.cleanup.orphanScan.policy`
- search: `worktree.cleanup.orphanScan.scannedCount`
- search: `worktree.cleanup.orphanScan.orphanCount`
- search: `worktree.cleanup.orphanScan.orphans[]`
- `taskContext.instanceId`
- `taskContext.product`
- `taskContext.version`
- search: `keywords`
- search: `keywordCounts`
- search: `matchCount`
- diff: `diffCount`
- `truncated`
- search: `matches`
- search: `matches[].file`
- search: `matches[].lineNumber`
- search: `matches[].text`
- search: `matches[].ref`
- diff: `diffs`
- diff: `diffs[].file`
- diff: `diffs[].addedLines`
- diff: `diffs[].deletedLines`
- diff: `diffs[].binary`
- diff: `diffs[].summary`
- diff: `diffs[].ref`
- `finalEvidenceAllowed`

最终答案可引用：

```text
code_evidence/<action_id>.json#matches/<index>
code_evidence/<action_id>.json#diffs/<index>
```

## 安全约束

- 代码仓只读检索，不自动修改代码。
- 当前实现不 pull、不运行构建脚本、不修改管理员配置的源代码仓。
- `git worktree` cache 路径必须保持在 `LOGAGENT_V2_CODE_WORKTREE_ROOT` 或默认 `data_dir/code_worktrees` 内；删除重建只允许发生在该 root 下。
- LRU 清理只删除 cache root 内同 product 的 `wt_*` 目录，且不得删除当前 search
  正在使用的 worktree。
- 后续 fix mode 修改必须使用独立 writable worktree，不能影响用户工作区或当前只读 evidence cache。
- 版本 ref、显式 search/diff `gitRef` 和 search roots 必须来自管理员配置。
- `searchRoots` 必须是安全相对路径，不能包含绝对路径、`.`、`..`、空 segment 或反斜杠。
- MCP 请求不能覆盖 task 的 product/version/ref 安全映射；当 task 通过
  Metadata instance 已确定 product/version 时，请求 product、search version/gitRef、diff target version/gitRef 必须与之匹配。

## 验收标准

- 给定版本能定位到确定 ref 或明确报错。
- 证据包含 repo、ref、commit 和文件行号。
- 给定两个配置版本或受控 ref 时，diff 能定位 base/target commit 并返回文件级 added/deleted/binary 摘要。
- 同一检索请求可幂等恢复；不同请求可复用同一 commit 的 detached worktree，不影响用户工作区。
- 同一 diff 请求可幂等恢复，不创建 writable worktree，不依赖源 repo 工作区状态。
- 超过 `LOGAGENT_V2_CODE_WORKTREE_MAX_PER_REPO` 时，后续 search 会删除同 product
  最近最少使用的旧 worktree，并在 `worktree.cleanup` 中记录删除摘要。
- 准备 worktree 时会扫描同 product `wt_*` cache 目录并在
  `worktree.cleanup.orphanScan` 中记录 orphan 告警；该扫描不自动删除目录。
- 源 repo 工作区存在未提交修改时，证据仍来自固定 commit 的 cache worktree。
- Task MCP 和 provider prompt 只在配置仓库存在时广告 `logagent.search_code` 和 `logagent.diff_code`。
- 最终答案只接受当前 run 中实际存在的 `code_evidence/...#matches/<index>` 或 `code_evidence/...#diffs/<index>`。
- README 和 SPEC 在版本映射或证据结构变更时同步更新。
