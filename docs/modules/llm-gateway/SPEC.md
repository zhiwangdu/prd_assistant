# LLM Gateway Spec

## 目标

为 LogAgent 提供受约束、可替换的模型辅助能力，用于 Case import、alias 生成和兼容恢复路径的结构化结果生成。Log Analysis 的运行后端已切换为 Claude Code session runner，Claude Code 不归入 LLM Gateway。

## 当前状态

已实现最小单次调用版本：

- `stub` Provider。
- OpenAI-compatible `/chat/completions` Provider。
- `binary` Provider 预留分支，使用参数数组调用 `<binary_path> run <prompt>` 并解析 stdout JSON。
- 支持通过 `llm.model_env` 从环境变量读取模型名，并保留静态 `llm.model` 兼容。
- session text、manifest/grep/metadata Prompt 和字符数裁剪。
- System Context 背景资源 Prompt 和字符数裁剪。
- tool result summary/findings Prompt 和字符数裁剪。
- 最终结果 schema、confidence、session text、grep evidence ref 和 tool finding evidence ref 校验。
- FinalAnswer schema 和 parser，供 Claude Code runner 与兼容恢复路径复用。
- Claude Code 返回裸最终结果 JSON，或返回多包一层的 `final_answer.result.result` / `answer` / `finalAnswer` 时，会规范化为最终结果并继续做 evidence ref 校验。
- 可追踪 evidence ref 别名规范化：裸日志行号/范围和 `#start-#end` 索引范围会映射为 `grep_results.json#matches/<index>`。
- 响应解析接受纯 JSON、单个 JSON Markdown 代码围栏，或混有额外自然语言但只包含一个可解析顶层 JSON object 的内容。
- 最终结果解析/schema 错误会追加修正提示并重试一次；Provider HTTP、鉴权、限流和超时错误不重试。
- 成功 task 的 alias 生成调用，输出 `{"alias":"..."}`，用于 UI 展示而不是分析证据。
- `result.json` / `result.md` 持久化。
- Task Executor 在 `PLAN_ANALYSIS` 阶段已改为调用 Claude Code session runner，并复用本模块的最终结果 evidence ref 校验。
- Settings LLM 诊断接口：`/api/settings/llm`、`/api/settings/llm/models`、`/api/settings/llm/chat`。
- Claude Code 诊断接口已由 session runner registry 提供，LLM Gateway 不负责 Claude Code runtime。
- session 输入/响应文件由 Analysis Orchestrator 写入，LLM Gateway 不执行 `analysis_package.json` / `claude_prompt.md` / `claude_mcp_config.json` / `claude_session.json` / `mcp_calls.jsonl` / `agent_response.json`。

## 当前输入

- 用户问题。
- `session_text_input.json#question` 用户输入文本证据。
- manifest 文件摘要。
- grep match 索引、文件、行号、关键词和文本。
- task 创建时固化的 Metadata 摘要，包括产品、版本、环境、节点状态、数据库和 PT 统计。
- task 创建时固化的 System Context 摘要，包括 Prompt Pack、架构文档、Runbook、工具能力说明和 Metadata adapter。
- Tool Runner 的工具名、状态、退出码、耗时、summary 和 findings。

## 当前输出

结构化最终结果包含 summary、symptoms、likelyRootCauses、nextChecks、fixSuggestions、missingInformation 和 confidence。根因证据最终只保存有效的 session text、grep match 或 tool finding 引用。
根因证据也可以引用当前 task 创建时固化的历史 Case context，canonical 格式为：

- `case_context.json#cases/<index>`

真实模型输出 `case_<id>` 或“历史案例 case_<id>”时，Gateway 会在当前 `case_context.json` 中查找对应 Case 并规范化为 canonical ref；找不到或索引越界必须拒绝。

System Context 不属于最终结果 evidence ref。模型可以参考其中背景知识，但根因证据不能引用 `system_context.json`。

Task alias 输出只包含 `alias` 字段。alias 必须是短标题，不能包含 task ID、时间戳、`LogAgent`、`task`、`run` 等泛化词。alias 调用不记录到 `analysis_events.jsonl` 或 Session timeline；schema 错误最多重试一次，最终失败由 Server fallback，不影响 task 成功状态。

用户输入文本 evidence ref 使用固定 canonical 格式：

- `session_text_input.json#question`

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
- 不支持的结构化输出

当前版本对最终结果解析/schema 错误最多调用两次。第二次仍失败，或遇到 Provider HTTP、鉴权、限流、网络、超时错误时，任务进入对应失败阶段。最终结果失败进入 `FAILED / GENERATE_RESULT`；`PLAN_ANALYSIS` 的 Claude structured outcome 失败由 Claude runner 写入 `FAILED / PLAN_ANALYSIS`。

裸最终结果 JSON 和常见最终结果包裹变体会作为最终结果兼容；其他缺失必要字段且不满足最终结果 schema 的响应仍会失败。当前 `PLAN_ANALYSIS` 通过 Claude MCP `request_user_input` 进入 `WAITING_FOR_USER`，通过 `request_approval` 进入 `WAITING_FOR_APPROVAL`。

binary provider 错误包括：

- `llm.binary_path` 或 `llm.binary_path_env` 缺失、为空或不是绝对路径。
- 二进制启动失败。
- `<binary_path> run <prompt>` 超时。
- 进程非零退出。
- stdout 不是 UTF-8。
- stdout 超过 `llm.binary_max_output_bytes`。
- stdout 中没有合法的结构化 JSON 或 schema / evidence ref 校验失败。

## 安全约束

- 不直接执行任何 action。
- 不接收密钥、SSH key、Cookie 或完整敏感配置。
- 不保存模型隐藏思维链。
- binary provider 只调用配置中的绝对路径二进制，固定 argv 为 `run` 和完整 prompt，不拼接 shell，不接受用户输入覆盖可执行路径或 argv。
- Provider 原始响应仅在显式安全调试配置下短期保存，默认只保留结构化结果和用量。
- runtime LLM output debug 开关默认关闭，仅在当前 Server 进程内生效；开启时只把模型 response content 打印到 Server stderr，不打印 prompt、API Key 或 headers。
- 模型名可来自环境变量，但不得记录 API Key；模型环境变量缺失或值为空时启动失败。
- Prompt 中的日志、Case、System Context 和用户文本视为不可信数据，不能覆盖系统 schema、MCP tool 白名单或 permission profile。

## 验收标准

- stub Provider 能返回最终结果。
- binary Provider 能通过 mock binary 验证 `<binary_path> run <prompt>` 调用路径，stdout JSON 可用于最终结果生成和 action decision。
- FinalAnswer parser 兼容裸最终结果 JSON 与常见最终结果包裹变体。
- 非法 schema、confidence 或 evidence ref 被拒绝。
- 最终结果 schema 解析失败时会重试一次，最终错误包含最新失败原因和上一轮失败原因。
- 可映射的行号/索引范围 evidence ref 会规范化为 canonical refs。
- 可映射的历史 Case ID evidence ref 会规范化为 `case_context.json#cases/<index>`。
- 可追踪的字符串形式 root cause 会规范化为对象形式。
- 单字符串形式的列表字段会规范化为字符串数组。
- 纯 JSON、完整 JSON 代码围栏和包含唯一顶层 JSON object 的自然语言响应可解析；多个 JSON object、无 JSON object 或 schema 不合法必须拒绝。
- 输入裁剪后不超过字符上限且保留 grep 和 tool 证据引用。
- 只填写对话框文本时，最终结果可以引用 `session_text_input.json#question`。
- Metadata `rawSnapshot` 不进入 Prompt。
- System Context 背景进入 Prompt，但不能作为最终结果 evidence ref。
- Tool Runner stdout/stderr 原文不进入 Prompt；只使用 result summary/findings。
- 鉴权、限流、5xx、网络、超时和解析失败产生明确错误。
- Gateway 无法直接访问 Tool Runner、Environment Collector、Claude Code session 或任务状态存储。
- Gateway 不负责外部成熟 agent CLI 的认证、进程交互和输出协议。
- `/api/debug/llm` 可手动开启和关闭 LLM response content 日志，Server 重启后恢复关闭。
- `/api/settings/llm` 可读取不含密钥的当前 LLM 配置摘要；`/api/settings/llm/models` 可测试模型列表获取；`/api/settings/llm/chat` 可发送简单消息并返回响应或完整异常文本。
- LLM Gateway 调用必须带 `llmcall_*` callId，并记录 started/completed/schema_retry 事件；schema retry 和最终错误必须能关联该 callId。
- Task alias 生成不得写入 analysis event 或 Session timeline，且 UI 可用 alias 替代裸 task ID。
