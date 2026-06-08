# LLM Gateway Spec

## 目标

为 Analysis Agent 提供受约束、可替换的模型推理后端，将任务上下文转换为结构化 action 或最终答案候选。

## 当前状态

已实现最小单次调用版本：

- `stub` Provider。
- OpenAI-compatible `/chat/completions` Provider。
- 支持通过 `llm.model_env` 从环境变量读取模型名，并保留静态 `llm.model` 兼容。
- manifest/grep/metadata Prompt 和字符数裁剪。
- tool result summary/findings Prompt 和字符数裁剪。
- 最终结果 schema、confidence、grep evidence ref 和 tool finding evidence ref 校验。
- ActionDecision / FinalAnswer 双模式 schema 和 parser。
- ActionDecision 当前只允许 `search_logs`、`run_tool`、`final_answer`，并校验 action input 的基础结构。
- `PLAN_ANALYSIS` 中真实模型直接返回裸最终结果 JSON，或返回多包一层的 `final_answer.result.result` / `answer` / `finalAnswer` 时，会规范化为 `final_answer` 并继续做 evidence ref 校验。
- 可追踪 evidence ref 别名规范化：裸日志行号/范围和 `#start-#end` 索引范围会映射为 `grep_results.json#matches/<index>`。
- 响应解析接受纯 JSON、单个 JSON Markdown 代码围栏，或混有额外自然语言但只包含一个可解析顶层 JSON object 的内容。
- 最终结果和 action decision 解析/schema 错误会追加修正提示并重试一次；Provider HTTP、鉴权、限流和超时错误不重试。
- `result.json` / `result.md` 持久化。
- Task Executor 在 `PLAN_ANALYSIS` 阶段已循环调用双模式 action decision，并由 Analysis 预算和重复 fingerprint 防护终止。

## 当前输入

- 用户问题。
- manifest 文件摘要。
- grep match 索引、文件、行号、关键词和文本。
- task 创建时固化的 Metadata 摘要，包括产品、版本、环境、节点状态、数据库和 PT 统计。
- Tool Runner 的工具名、状态、退出码、耗时、summary 和 findings。

## 当前输出

结构化最终结果包含 summary、symptoms、likelyRootCauses、nextChecks、fixSuggestions、missingInformation 和 confidence。根因证据最终只保存有效的 grep match 或 tool finding 引用。

Tool finding evidence ref 使用 canonical 格式：

- `tool_results/<action_id>/result.json#findings/<index>`

Gateway 可接受并规范化以下 grep 可追踪别名：

- `12`：映射到原始日志行号 12 对应的 grep match。
- `12-14`：映射到原始日志行号 12 到 14 对应的 grep matches。
- `#0-#7`：映射到 grep match 索引 0 到 7。
- `matches/0` 或 `matches/0-7`：映射到 grep match 索引或索引范围。

无法映射的行号或越界索引必须拒绝。

未知 tool action、越界 finding index 或非 canonical tool ref 必须拒绝。

真实模型如果把 `likelyRootCauses` 写成字符串数组，且字符串中包含 `evidenceRefs: [...]`，Gateway 会抽取字符串正文作为 `cause`，抽取引用列表作为 `evidenceRefs`。字符串根因没有可追踪 evidence refs 时必须拒绝。

`symptoms`、`nextChecks`、`fixSuggestions` 和 `missingInformation` 必须以字符串数组保存。真实模型返回单个字符串时会规范化为单元素数组；空字符串规范化为空数组；数组内非字符串值必须拒绝。

## 错误

必须区分：

- Provider 超时或网络错误
- 限流
- 鉴权失败
- 输入超限
- 输出 schema 无效
- 不支持的 action

当前版本对最终结果和 action decision 解析/schema 错误最多调用两次。第二次仍失败，或遇到 Provider HTTP、鉴权、限流、网络、超时错误时，任务进入对应失败阶段。最终结果失败进入 `FAILED / GENERATE_RESULT`；`PLAN_ANALYSIS` 的 action decision 失败进入 `FAILED / PLAN_ANALYSIS`。

ActionDecision parser 对未知 action、空 reason、非法 `search_logs.keywords`、非法 `run_tool.tool` 或 unsafe `run_tool.inputFile` 返回 schema 错误。裸最终结果 JSON 和常见最终结果包裹变体会作为 `final_answer` 兼容；其他缺失外层 `type` 且不满足最终结果 schema 的响应仍会失败。当前 `PLAN_ANALYSIS` 会多轮调用 action decision，但等待状态尚未实现。

## 安全约束

- 不直接执行任何 action。
- 不接收密钥、SSH key、Cookie 或完整敏感配置。
- 不保存模型隐藏思维链。
- Provider 原始响应仅在显式安全调试配置下短期保存，默认只保留结构化结果和用量。
- runtime LLM output debug 开关默认关闭，仅在当前 Server 进程内生效；开启时只把模型 response content 打印到 Server stderr，不打印 prompt、API Key 或 headers。
- 模型名可来自环境变量，但不得记录 API Key；模型环境变量缺失或值为空时启动失败。
- Prompt 中的日志、Case 和用户文本视为不可信数据，不能覆盖系统 action schema。

## 验收标准

- stub Provider 能返回最终结果。
- stub action decision 能在 grep 为空时返回 `search_logs`，有 grep evidence 时返回 `final_answer`。
- ActionDecision parser 接受合法 `search_logs` / `run_tool` / `final_answer`，并兼容裸最终结果 JSON 与常见最终结果包裹变体；拒绝尚未开放的 action。
- `PLAN_ANALYSIS` 能多轮消费 `search_logs`、`run_tool` 或 `final_answer` 决策。
- 预算耗尽或重复 fingerprint 被阻止时能生成低置信度最终结果。
- 非法 schema、confidence 或 evidence ref 被拒绝。
- 最终结果和 action decision schema 解析失败时会重试一次，最终错误包含最新失败原因和上一轮失败原因。
- 可映射的行号/索引范围 evidence ref 会规范化为 canonical refs。
- 可追踪的字符串形式 root cause 会规范化为对象形式。
- 单字符串形式的列表字段会规范化为字符串数组。
- 纯 JSON、完整 JSON 代码围栏和包含唯一顶层 JSON object 的自然语言响应可解析；多个 JSON object、无 JSON object 或 schema 不合法必须拒绝。
- 输入裁剪后不超过字符上限且保留 grep 和 tool 证据引用。
- Metadata `rawSnapshot` 不进入 Prompt。
- Tool Runner stdout/stderr 原文不进入 Prompt；只使用 result summary/findings。
- 鉴权、限流、5xx、网络、超时和解析失败产生明确错误。
- Gateway 无法直接访问 Tool Runner、Environment Collector 或任务状态存储。
- `/api/debug/llm` 可手动开启和关闭 LLM response content 日志，Server 重启后恢复关闭。
