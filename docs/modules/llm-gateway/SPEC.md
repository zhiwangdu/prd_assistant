# Optional LLM Gateway Spec

## Requirements

- Provider 配置可为空。
- 调用必须有 timeout 和日志脱敏。
- 输出必须标记为 draft 或 assistant summary，不能替代工具证据。

## Acceptance

- 无 LLM API Key 时全部核心工具功能可用。
