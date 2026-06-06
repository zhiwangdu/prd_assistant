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

## 配置

```yaml
llm:
  provider: "openai_compatible"
  base_url_env: "LOGAGENT_LLM_BASE_URL"
  api_key_env: "LOGAGENT_LLM_API_KEY"
  model: "gpt-4.1"
  max_input_tokens: 64000
  max_output_tokens: 4096
  request_timeout_seconds: 120
  max_retries: 2
```

模型名只是配置示例，不作为固定依赖。

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

每次调用只返回以下之一：

- `action`：Analysis Agent 可请求的结构化动作。
- `final_answer`：符合最终结果 schema 的候选结论。

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
