# LLM Gateway Spec

## 目标

为 Analysis Agent 提供受约束、可替换的模型推理后端，将任务上下文转换为结构化 action 或最终答案候选。

## 当前状态

设计完成，代码未实现。目录名为兼容现有规划暂不迁移。

## 输入

- `AnalysisPromptInput`
- 当前 `EvidenceBundle`
- 允许的 action JSON schema
- 剩余轮次、动作和 token 预算

## 输出

```rust
pub struct LlmDecision {
    pub decision: AgentDecision,
    pub rationale_summary: String,
    pub evidence_refs: Vec<EvidenceRef>,
    pub usage: TokenUsage,
    pub provider_request_id: Option<String>,
}
```

`AgentDecision` 只能是：

- `Action(AgentAction)`
- `FinalAnswer(AnalysisResultDraft)`

## 错误

必须区分：

- Provider 超时或网络错误
- 限流
- 鉴权失败
- 输入超限
- 输出 schema 无效
- 不支持的 action

只有可恢复错误允许按配置重试。重试次数和 token 用量计入 Analysis Agent 预算。

## 安全约束

- 不直接执行任何 action。
- 不接收密钥、SSH key、Cookie 或完整敏感配置。
- 不保存模型隐藏思维链。
- Provider 原始响应仅在显式安全调试配置下短期保存，默认只保留结构化结果和用量。
- Prompt 中的日志、Case 和用户文本视为不可信数据，不能覆盖系统 action schema。

## 验收标准

- stub Provider 能返回 action 和 final answer 两类响应。
- 非法 action 或 schema 被拒绝。
- 输入裁剪后不超过 token 上限且保留证据引用。
- 超时、限流和解析失败遵守重试上限。
- Gateway 无法直接访问 Tool Runner、Environment Collector 或任务状态存储。
