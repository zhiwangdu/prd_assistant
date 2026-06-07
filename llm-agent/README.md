# LLM Gateway 方案

目录名暂保留为 `llm-agent/`，组件职责已收窄为 LLM Gateway。自主调查、多轮状态和用户追问由独立的 Analysis Agent 负责。

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

当前作为 Server 内部 Rust 模块实现了单次最终结果生成：

```text
question + manifest.json + grep_results.json + metadata_context.json
  -> Prompt 裁剪
  -> stub 或 OpenAI-compatible Chat Completions
  -> schema / evidence ref 校验与可追踪别名规范化
  -> result.json / result.md
```

当前不返回 action，不记录模型用量和 Provider request id；这些能力留给多轮 Analysis Agent 阶段。当前会对最终结果的解析/schema 错误做一次受控修正重试，HTTP、鉴权、限流和超时错误不重试。

响应解析接受纯 JSON、完整 JSON Markdown 代码围栏，或附带说明文本但只包含一个可解析顶层 JSON object 的响应。多个 JSON object、无 JSON object 或 schema 不合法仍会拒绝。

重试时 Gateway 只把解析/schema 错误摘要和结果 schema 要求追加给模型，不保存原始响应，不暴露 API Key。两次都失败时，错误信息包含最新解析失败原因和上一次失败原因。

Metadata Prompt 摘要包含解析后的 ID、产品、版本、环境、选中节点状态、集群节点数量、数据库名和 PT 在线摘要；不会发送 Metadata `rawSnapshot`。

evidence ref 的 canonical 形式仍是 `grep_results.json#matches/<index>`。真实模型偶尔返回裸日志行号或范围，例如 `12-14`，或索引范围 `#0-#7`；Gateway 会在能唯一映射到当前 grep evidence 时规范化为 canonical refs。无法映射到 grep evidence 的引用仍会拒绝，任务进入 `FAILED / GENERATE_RESULT`。

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

## 输入

- 用户问题和 task 元信息
- 已确认事实、候选假设和信息缺口
- 最近分析事件摘要
- manifest、日志、工具、代码、环境和 Metadata 证据
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

当前每次任务只调用一次并返回最终结果 JSON；后续再扩展为 `action | final_answer`。

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
