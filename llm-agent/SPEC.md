# LLM Gateway Spec

## 目标

为 Analysis Agent 提供受约束、可替换的模型推理后端，将任务上下文转换为结构化 action 或最终答案候选。

## 当前状态

已实现最小单次调用版本：

- `stub` Provider。
- OpenAI-compatible `/chat/completions` Provider。
- 支持通过 `llm.model_env` 从环境变量读取模型名，并保留静态 `llm.model` 兼容。
- manifest/grep/metadata Prompt 和字符数裁剪。
- 最终结果 schema、confidence 和 grep evidence ref 校验。
- 可追踪 evidence ref 别名规范化：裸日志行号/范围和 `#start-#end` 索引范围会映射为 `grep_results.json#matches/<index>`。
- 响应解析接受纯 JSON 或单个 JSON Markdown 代码围栏，拒绝混有额外自然语言的内容。
- `result.json` / `result.md` 持久化。

## 当前输入

- 用户问题。
- manifest 文件摘要。
- grep match 索引、文件、行号、关键词和文本。
- task 创建时固化的 Metadata 摘要，包括产品、版本、环境、节点状态、数据库和 PT 统计。

## 当前输出

结构化最终结果包含 summary、symptoms、likelyRootCauses、nextChecks、fixSuggestions、missingInformation 和 confidence。根因证据最终只保存有效的 `grep_results.json#matches/<index>`。Gateway 可接受并规范化以下可追踪别名：

- `12`：映射到原始日志行号 12 对应的 grep match。
- `12-14`：映射到原始日志行号 12 到 14 对应的 grep matches。
- `#0-#7`：映射到 grep match 索引 0 到 7。

无法映射的行号或越界索引必须拒绝。

## 错误

必须区分：

- Provider 超时或网络错误
- 限流
- 鉴权失败
- 输入超限
- 输出 schema 无效
- 不支持的 action

当前版本每个任务只调用一次，不自动重试；任何 Provider 或 schema 错误使任务进入 `FAILED / GENERATE_RESULT`。

## 安全约束

- 不直接执行任何 action。
- 不接收密钥、SSH key、Cookie 或完整敏感配置。
- 不保存模型隐藏思维链。
- Provider 原始响应仅在显式安全调试配置下短期保存，默认只保留结构化结果和用量。
- 模型名可来自环境变量，但不得记录 API Key；模型环境变量缺失或值为空时启动失败。
- Prompt 中的日志、Case 和用户文本视为不可信数据，不能覆盖系统 action schema。

## 验收标准

- stub Provider 能返回最终结果。
- 非法 schema、confidence 或 evidence ref 被拒绝。
- 可映射的行号/索引范围 evidence ref 会规范化为 canonical refs。
- 纯 JSON 和完整 JSON 代码围栏可解析，附带额外自然语言的响应被拒绝。
- 输入裁剪后不超过字符上限且保留证据引用。
- Metadata `rawSnapshot` 不进入 Prompt。
- 鉴权、限流、5xx、网络、超时和解析失败产生明确错误。
- Gateway 无法直接访问 Tool Runner、Environment Collector 或任务状态存储。
