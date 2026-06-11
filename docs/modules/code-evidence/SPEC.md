# Code Evidence Spec

## 目标

Code Evidence 根据用户输入的软件产品和版本，切换或定位到对应代码分支，收集与日志现象相关的代码证据，供 Agent Backend 引用。

## 当前状态

未实现代码，已有设计方向。

## 输入

- `product`
- `version`
- 可选 `git_ref`
- grep 关键词和错误片段
- 仓库配置
- 经 Server 校验的 `action_id` 和检索词

## 处理流程

```text
version -> branch/tag/ref mapping
  -> prepare read-only worktree
  -> search related symbols/errors
  -> extract file/line/function snippets
  -> write code_evidence.json
```

## 输出

```text
code_evidence.json
```

建议字段：

- `product`
- `version`
- `repo`
- `ref`
- `commit`
- `matches`
- `files`
- `notes`
- `action_id`

## 安全约束

- 代码仓只读检索，不自动修改代码。
- 分支切换不能影响用户工作区，优先使用独立 worktree/cache。
- 搜索优先 `rg`。
- action 不能覆盖 task 的 product/version/ref 安全映射。

## 验收标准

- 给定版本能定位到确定 ref 或明确报错。
- 证据包含 repo、ref、commit 和文件行号。
- 同一 action 可幂等恢复，不影响用户工作区。
- README 和 SPEC 在版本映射或证据结构变更时同步更新。
