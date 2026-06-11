# System Context 方案

## 当前实现状态

System Context 是 LogAgent 的通用背景资源中心，用于管理可以进入 Prompt 的长期资源。第一版已作为 Server 内部模块实现，采用本地 JSON Store，不替换现有 Metadata Store，而是把 Metadata instance 作为只读 adapter 纳入统一资源目录。

已实现：

- 本地版本化 `SystemContextStore`。
- 资源类型：Prompt Pack、Architecture Doc、Runbook、Glossary、Tool Capability、Knowledge Note 和 Metadata Instance adapter。
- 资源创建、更新、版本新增、版本激活和列表 API。
- Prompt preview API。
- Log Analysis Session 保存 `systemContextIds`。
- 创建 task run 时固化 `system_context.json`，并向 Session timeline 写入 `system_context_recorded`。
- 当前 `internal_llm` 后端会通过 LLM Gateway 将 `system_context.json` 作为背景参考注入 Prompt。
- WebUI `System Context` 页面，包含资源库、Architecture Mermaid 文本管理和现有 Metadata 页面入口。

## 职责

负责：

- 管理跨 Session、跨 Task 的通用背景知识。
- 版本化 System Prompt / Prompt Pack。
- 管理产品架构说明、Mermaid 架构图、Runbook、术语和工具能力说明。
- 聚合 Metadata instance summary 作为可注入背景资源。
- 承载 Domain Adapter 提供的领域 Runbook、术语和诊断说明。
- 在 task 创建时生成可审计的 `system_context.json` 快照。

不负责：

- 保存密钥、SSH 凭据或模型 API Key。
- 保存原始日志、上传 payload 或 task evidence。
- 替代 Case Store；历史故障仍由人工确认后的 Case Store 管理。
- 替代当前任务证据；最终结论仍必须引用 session text、grep、tool、case 等任务内证据。

## 数据目录

```text
data_dir/
  system_context/
    resources/
      ctx_xxx.json
```

## Prompt 语义

System Context 只作为背景参考进入 Agent Backend 输入。固定安全约束、结构化输出 schema 和 allowed actions 仍由代码追加，Prompt Pack 和 Domain Adapter 内容不能覆盖 Server 侧校验。

Task workspace 会包含：

```text
workspaces/task_xxx/system_context.json
```

其中记录资源标题、类型、版本、summary、裁剪后的 content、来源和 prompt priority。
