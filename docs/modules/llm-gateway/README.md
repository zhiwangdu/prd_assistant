# LLM Gateway 方案

该文档已归档到 `docs/modules/llm-gateway/`。组件职责已收窄为 LLM Gateway；自主调查、多轮状态和用户追问由 Analysis Agent 负责。

## 职责

LLM Gateway 负责：

- OpenAI-compatible 等 Provider 适配
- 将 Analysis Agent 的当前状态和证据组装为 Prompt
- token 估算、证据排序和裁剪
- 调用模型
- 校验结构化响应 schema
- 对超时、限流和可恢复解析错误做有限重试
- 返回模型用量和 Provider request id 供审计

LLM Gateway 不负责：

- 保存任务状态或会话
- 决定任务状态流转
- 直接调用工具、代码仓、文件系统或 SSH
- 执行动作或审批
- 保存隐藏思维链

## 当前实现

当前作为 Server 内部 Rust 模块实现了单次最终结果生成和多轮 action decision：

```text
question + session_text_input.json + system_context.json + manifest.json + grep_results.json + metadata_context.json + tool_results
  -> Prompt 裁剪
  -> stub、OpenAI-compatible Chat Completions 或 binary provider
  -> schema / evidence ref 校验与可追踪别名规范化
  -> result.json / result.md
  -> silent task alias generation for UI display
```

`binary` provider 是预留的大模型调用分支。启用后 Gateway 会使用参数数组调用配置的二进制：

```text
<binary_path> run "<prompt>"
```

该分支不拼接 shell，不允许用户覆盖 binary path 或 argv。当前环境不要求接入真实二进制，已通过单元测试中的 mock binary 验证最终结果生成和 `PLAN_ANALYSIS` action decision 都能解析 stdout JSON。

Task Executor 在 `PLAN_ANALYSIS` 阶段会循环调用 ActionDecision / FinalAnswer 双模式入口。`final_answer` 直接持久化结果，`search_logs` 和 `run_tool` 执行动作后回到下一轮，直到最终答案、预算耗尽或重复 fingerprint 被阻止。当前不记录 Provider request id；该能力留给后续审计阶段。当前会对最终结果和 action decision 的解析/schema 错误各做一次受控修正重试，HTTP、鉴权、限流和超时错误不重试。

响应解析接受纯 JSON、完整 JSON Markdown 代码围栏，或附带说明文本但只包含一个可解析顶层 JSON object 的响应。`PLAN_ANALYSIS` 期望双模式 `action | final_answer` 外层结构；如果真实模型直接返回裸最终结果 JSON，或把最终结果多包一层 `result` / `answer` / `finalAnswer`，Gateway 会将其规范化为 `final_answer` 并继续执行同一套 evidence 校验。多个 JSON object、无 JSON object 或最终结果核心 schema 不合法仍会拒绝。

重试时 Gateway 只把解析/schema 错误摘要和结果 schema 要求追加给模型，不保存原始响应，不暴露 API Key。两次都失败时，错误信息包含最新解析失败原因和上一次失败原因。

Server 提供进程内 runtime debug 开关，WebUI 顶部的 `LLM debug` 可调用 `/api/debug/llm` 开启或关闭。开启后 Gateway 只把模型 response content 打印到 Server stderr，便于定位 schema 漂移；不会打印 prompt、API Key 或 HTTP headers。该开关默认关闭，Server 重启后恢复关闭。

Server 还提供受保护的 Settings 诊断接口，供 WebUI Settings 页面验证当前 LLM 服务：

- `GET /api/settings/llm`：返回 provider、模型、超时和输入/输出限制等摘要，不返回密钥。
- `GET /api/settings/llm/models`：测试模型列表接口；OpenAI-compatible 调用 `/models`，stub/binary 返回配置模型。
- `POST /api/settings/llm/chat`：发送一条简单 user message，返回模型响应。

诊断接口使用 `{ok,result,error}` 响应；Provider HTTP、鉴权、限流、网络、超时、JSON decode 等异常会写入 `error`，便于页面直接展示。

`PLAN_ANALYSIS` 的 OpenAI-compatible action decision 调用会生成 `llmcall_*` callId，并通过 Analysis State Store 记录 `llm_call_started`、`llm_call_completed` 和 `llm_call_schema_retry`。schema retry 和最终失败会带上 callId，WebUI Task execution 可直接展示对应轮次。

成功 task 的 alias 由独立 LLM Gateway 调用生成，输入为用户问题、最终结果、manifest 文件名和 Metadata 摘要。该命名调用只返回 `{"alias":"..."}`，schema 错误重试一次；调用失败时由 Server 用最终 summary/question 生成短标题。alias 生成不触发 Analysis State Store 的 `llm_call_*` 事件，不写 `analysis_events.jsonl`，也不写 Session timeline。

Metadata Prompt 摘要包含解析后的 ID、产品、版本、环境、选中节点状态、集群节点数量、数据库名和 PT 在线摘要；不会发送 Metadata `rawSnapshot`。

System Context Prompt 摘要包含任务创建时固化的 Prompt Pack、架构文档、Runbook、工具能力说明和 Metadata adapter 资源。System Context 只作为背景参考，不能替代当前任务证据，也不能作为最终结果 `evidenceRefs`。

用户输入文本会作为 `session_text_input.json#question` 进入 Prompt，并可作为只填写对话框文本时的最终结果 evidence ref。

Tool Runner Prompt 摘要包含工具名、执行状态、退出码、耗时、summary 和结构化 findings。工具 finding 的 canonical evidence ref 是 `tool_results/<action_id>/result.json#findings/<index>`。

grep evidence ref 的 canonical 形式是 `grep_results.json#matches/<index>`。历史 Case 参考的 canonical 形式是 `case_context.json#cases/<index>`。真实模型偶尔返回裸日志行号或范围，例如 `12-14`，或索引范围 `#0-#7`；Gateway 会在能唯一映射到当前 grep evidence 时规范化为 canonical refs。真实模型返回 `case_<id>` 或“历史案例 case_<id>”时，会在当前 task 的 `case_context.json` 中查找并规范化为 canonical Case ref。无法映射到 session text、grep evidence、Case context 或 tool finding 的引用仍会拒绝，任务进入对应失败阶段。

真实模型偶尔把 `likelyRootCauses` 写成字符串数组，并把 `evidenceRefs` 嵌在字符串中，例如 `原因（evidenceRefs: [matches/0, matches/1]）`。Gateway 会把这种可追踪字符串规范化为 `{ cause, evidenceRefs }`，并支持 `matches/<index>` / `matches/<start>-<end>` 别名；没有证据引用的根因仍会被拒绝。

真实模型偶尔把 `symptoms`、`nextChecks`、`fixSuggestions` 或 `missingInformation` 中的单个列表字段写成字符串。Gateway 会把非空字符串规范化为单元素数组，空字符串规范化为空数组；数组中的非字符串值仍会拒绝。

## 配置

```yaml
llm:
  provider: "openai_compatible"
  base_url_env: "LOGAGENT_LLM_BASE_URL"
  api_key_env: "LOGAGENT_LLM_API_KEY"
  model_env: "LOGAGENT_LLM_MODEL"
  max_input_chars: 60000
  max_output_tokens: 4096
  request_timeout_seconds: 120
```

模型名不作为固定依赖。`model_env` 配置后从环境变量读取模型名并优先于兼容用的静态 `model`；变量缺失或值为空时 Server 启动失败。

binary provider 示例：

```yaml
llm:
  provider: "binary"
  binary_path_env: "LOGAGENT_LLM_BINARY_PATH"
  model: "binary-reserved"
  binary_max_output_bytes: 1048576
  request_timeout_seconds: 120
  max_input_chars: 60000
  max_output_tokens: 4096
```

`binary_path` 或 `binary_path_env` 解析后的路径必须是绝对路径。stdout 必须返回与 OpenAI-compatible content 相同的结构化 JSON；非零退出、超时、非 UTF-8 stdout、超出 `binary_max_output_bytes` 或 schema 不合法都会使对应 LLM 调用失败。

## 输入

- 用户问题和 task 元信息
- 已确认事实、候选假设和信息缺口
- 最近分析事件摘要
- manifest、日志、工具、代码、环境、Metadata 和 System Context 背景
- Top 5 相似历史 Case
- 本轮剩余预算
- 允许的 action schema

## Token 预算

不能把全部日志或完整事件流直接放入 Prompt。建议按以下优先级裁剪：

1. 用户问题、补充消息、约束和剩余预算。
2. 已确认事实、未解决缺口和最近动作结果。
3. 带文件/行号的日志证据。
4. 工具 finding、代码证据和环境摘要。
5. 相似 Case 摘要。

裁剪结果必须保留证据引用，记录被省略的证据类别和数量。

## 结构化响应

当前 Task Executor 已在 `PLAN_ANALYSIS` 启用多轮 `action | final_answer` 响应，并由 Analysis 预算控制轮数、LLM 调用、action 数和重复 fingerprint。用户追问和审批恢复 API 已启用。

第一版已支持 action：

- `search_logs`
- `run_tool`
- `ask_user`
- `collect_environment`
- `final_answer`

暂未开放 action：

- `collect_code_evidence`

响应必须区分：

- 已确认事实
- 候选假设
- 信息缺口
- 简短决策依据
- 证据引用

Gateway 对未知动作、缺字段、无效枚举和超预算响应返回 schema 错误，由 Analysis Agent 决定重试或终止。

## Prompt 约束

- 日志证据引用文件和行号。
- 工具证据标明工具名和结果路径。
- 代码证据标明产品、版本、ref、commit、文件和行号。
- 环境证据标明节点、采集项及输出路径。
- 历史 Case 明确标记为参考。
- 无证据时明确不确定，禁止编造已执行动作。
- 不输出隐藏思维链，只输出简短可审计理由。
