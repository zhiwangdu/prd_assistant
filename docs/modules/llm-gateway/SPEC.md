# LLM Gateway Spec

## 目标

为 LogAgent 提供受约束、可替换的模型辅助能力。当前范围限定为 Case import、alias 生成和兼容恢复路径的结构化 JSON 处理；Log Analysis 的主调查循环由 Analysis Orchestrator 和 Agent Provider Runtime 执行。

## 输入

- 用户提供的 Case 文本或文件文本。
- 成功 run 的问题、最终结果、manifest 摘要和 Metadata 摘要，用于 alias。
- 兼容恢复路径中的 final answer 文本。
- System Context、Case、日志和工具摘要的 bounded prompt 片段。

## 输出

- Case import draft：`structuredCase`、`missingFields`、`assistantQuestion`。
- Alias：`{"alias":"..."}`。
- 规范化 final answer JSON。
- Provider usage、request id、错误分类和 schema retry 事件。

## Provider

支持：

- `stub`
- OpenAI-compatible `/chat/completions`
- `binary`，固定 argv `<binary_path> run <prompt>`

LLM Gateway provider 与 V2 Agent provider runtime 是两个边界。Agent runtime 也支持 `openai_compatible`、`binary` 和可选 `claude_code`，但其 request/response/run 状态由 Analysis Orchestrator 管理。

## 结构化校验

Gateway 必须：

- 接受纯 JSON、完整 JSON Markdown 代码围栏，或只包含一个可解析顶层 JSON object 的自然语言响应。
- 拒绝多个 JSON object、无 JSON object、非法 schema 或非法 confidence。
- 对解析/schema 错误最多追加一次修正提示并重试。
- 不对 HTTP 鉴权、限流、超时、网络错误或输入过大做 schema retry。

FinalAnswer 可兼容裸最终结果 JSON 和常见包裹字段：`result`、`answer`、`finalAnswer`。

## Evidence Ref

允许引用：

- `session_text_input.json#question`
- `grep_results.json#matches/<index>`
- `log_searches/<search_id>.json#matches/<index>`
- `tool_results/<action_id>/result.json#findings/<index>`
- `case_context.json#cases/<index>`

可规范化别名：

- 原始日志行号或行号范围。
- `matches/<index>` / `matches/<start>-<end>`。
- `#<start>-#<end>`。
- `case_<id>`。

无法映射、越界、跨 run 或引用背景资源的 evidence ref 必须拒绝。System Context、Diagnostic Skills 和 Metadata slices 只能作为背景。

## 错误分类

必须区分：

- `authentication_failed`
- `rate_limited`
- `input_too_large`
- `provider_timeout`
- `provider_server_error`
- `provider_client_error`
- `transport_error`
- `configuration_error`
- `output_too_large`
- `decode_error`
- `parse_error`
- `schema_error`

错误响应不得泄露 API Key、Authorization header、Cookie 或完整 provider headers。

## 安全约束

- LLM Gateway 无执行能力，不能调用 Tool Runner、Fetch、Remote Executor、Code Evidence 或 Metadata 查询。
- binary provider 只允许配置的绝对路径和固定 argv，不拼接 shell。
- 不保存隐藏思维链。
- Prompt 和响应调试默认关闭；开启时也不得打印 prompt、密钥或 headers。
- 模型输出必须经过 schema 和 evidence ref 校验后才能进入持久化结果。

## 验收标准

- stub provider 可生成有效 alias 或结构化 Case draft。
- OpenAI-compatible provider 能记录 request id、model、finish reason、usage 和稳定错误分类。
- binary provider 能通过 mock executable 验证固定 argv 和 stdout JSON 解析。
- Case import 缺必填字段时返回 `missingFields` 和 `assistantQuestion`。
- Alias 失败不影响 run 成功状态，Server 能 fallback。
- 非法 schema、非法 evidence ref、多个 JSON object 或无 JSON object 均被拒绝。
- response debug 开关默认关闭且不泄露敏感信息。
