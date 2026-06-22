# Optional Diagnostic Workflows Spec

## Goal

Workflow 是工具组合，不是通用 Agent。

## Requirements

- Workflow step 必须引用 catalog toolId 或内置只读查询。
- 每一步都生成 run/artifact。
- LLM step 可选，失败不影响基础工具结果可查看。
- Workflow 不保存隐藏思维链。

## Acceptance

- 禁用 LLM 后 workflow 仍能运行确定性工具步骤。
- 所有 step 可在 Runs 页面审计。
