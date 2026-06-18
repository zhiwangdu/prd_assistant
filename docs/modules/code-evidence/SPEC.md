# Code Evidence Spec

## 目标

Code Evidence 根据用户输入的软件产品和版本，定位到管理员配置的代码 ref，收集与日志现象相关的代码证据，供 Claude Code 引用。

## 当前状态

Python V2 已实现只读 `git grep` MVP：

- 通过 `LOGAGENT_V2_CODE_REPOS_JSON` 配置本地 git repo、默认 ref、版本到 ref 映射和相对 search roots。
- Task MCP 和 OpenAI-compatible / binary provider prompt 在存在配置仓库时广告 `logagent.search_code`。
- `logagent.search_code` 使用 `git rev-parse` 固化 commit，再执行 `git grep <commit>` 检索，不 checkout、不 pull、不修改仓库。
- 如果当前 run 所属 Session 绑定了 Metadata `instanceId`，且 snapshot
  `instance.product` / `instance.version` 存在，`logagent.search_code` 必须把
  请求限制在该 product/version 上；省略 `version` 时继承 instance version，
  显式 `gitRef` 必须等于该 version 的配置 ref。
- 检索结果写入当前 run 的 `code_evidence/<action_id>.json` artifact，`matches[].ref` 可作为最终答案 evidence ref。

尚未实现独立 worktree/cache、版本间 diff、commit 对比、符号级解析和 fix mode 代码修改。

## 输入

- `product`
- `version`
- 可选 `gitRef`，必须等于配置的 `defaultRef` 或 `versionRefs` 中的值
- `keywords[]` 或 `query`
- `maxMatchesPerKeyword`，按 1..10 裁剪
- 管理员配置的本地 repo、版本映射和 search roots

## 处理流程

```text
metadata instance product/version guard
  -> version -> branch/tag/ref mapping
  -> git rev-parse <ref>^{commit}
  -> git grep <commit> under configured searchRoots
  -> extract file/line/text evidence refs
  -> write code_evidence/<action_id>.json
```

## 输出

```text
code_evidence/<action_id>.json
```

当前字段：

- `product`
- `version`
- `ref`
- `commit`
- `repo.product`
- `repo.searchRoots`
- `taskContext.instanceId`
- `taskContext.product`
- `taskContext.version`
- `keywords`
- `keywordCounts`
- `matchCount`
- `truncated`
- `matches`
- `matches[].file`
- `matches[].lineNumber`
- `matches[].text`
- `matches[].ref`
- `finalEvidenceAllowed`

最终答案可引用：

```text
code_evidence/<action_id>.json#matches/<index>
```

## 安全约束

- 代码仓只读检索，不自动修改代码。
- 当前实现不切换分支、不创建 worktree、不运行构建脚本。
- 后续需要 checkout 或 fix mode 时，必须使用独立 worktree/cache，不能影响用户工作区。
- 版本 ref、显式 `gitRef` 和 search roots 必须来自管理员配置。
- `searchRoots` 必须是安全相对路径，不能包含绝对路径、`.`、`..`、空 segment 或反斜杠。
- MCP 请求不能覆盖 task 的 product/version/ref 安全映射；当 task 通过
  Metadata instance 已确定 product/version 时，请求 product/version/gitRef 必须与之匹配。

## 验收标准

- 给定版本能定位到确定 ref 或明确报错。
- 证据包含 repo、ref、commit 和文件行号。
- 同一检索请求可幂等恢复，不影响用户工作区。
- Task MCP 和 provider prompt 只在配置仓库存在时广告 `logagent.search_code`。
- 最终答案只接受当前 run 中实际存在的 `code_evidence/...#matches/<index>`。
- README 和 SPEC 在版本映射或证据结构变更时同步更新。
