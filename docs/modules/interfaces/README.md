# Module Interfaces 方案

## 目标

V2 使用单一 Python/FastAPI Server 进程和内部模块边界。Server 持有任务状态和执行权限，Analysis Orchestrator 持有证据包构建、provider prompt、task MCP 配置、预算和等待态，Agent Provider Runtime 只返回结构化 outcome，证据模块只执行受约束能力。

## 当前实现

V2 已在 Python 数据模型和 API/MCP handler 中落地第一版公共契约，并提供 Agent provider / Domain Adapter 摘要接口：

- `TaskContext`
- `AgentAction` / `ActionKind` / `ActionRisk`
- `EvidenceRef`
- `EvidenceArtifact` / `EvidenceType` / `EvidenceSummary`
- `EvidenceProvider`

Action 和 Evidence 使用稳定 JSON 名称，artifact 路径必须是 workspace 相对路径。Tool Runner 已成为第一个消费该契约的模块：规则版工具 action、Tools 页面手动运行和 task MCP `logagent.run_domain_tool` 走同一执行接口。日志包预处理会写入 `tool_inputs/index.json`，声明后续工具可消费的 materialized input。Fetch endpoint 复用 `tool_run` / `tool_results` 产物面，但执行契约由 Server 内置 Fetch service 提供：endpoint 和密文 credential set 持久化在 `storage.data_dir/fetch`，任务 MCP `logagent.fetch` 写入 `tool_results/<action_id>/result.json#response`。Huawei package sync 同样复用 `tool_run` / `tool_results`，但首版只支持受保护 Tools API 手动运行，结果是 `tool_results/<action_id>/result.json`，不新增最终答案 evidence ref 类型。

Server 现在也支持 Log Analysis Session 和 `taskKind=tool_run` 的手动工具运行任务。Session 是用户可见的恢复单元，保存草稿、`analysisMode`、语言、上传引用、历史 task runs 和 timeline；每次分析 run 仍创建一个绑定 `sessionId` 的 `taskKind=log_analysis` task workspace 快照。`tool_run` 路径复用 TaskStore、workspace 和 `tool_results` 产物，但不绑定 Session、不进入 `PLAN_ANALYSIS`；task MCP `logagent.run_domain_tool` 复用同一个工具 registry。

Agent Provider Runtime 提供配置摘要、dry-run 诊断和 `analysis_package.json` / `agent_request.json` / `agent_response.json` 输入/响应产物。`PLAN_ANALYSIS` 调用当前 `LOGAGENT_V2_AGENT_PROVIDER`，默认 `stub`，可选 `openai_compatible`、`binary` 或 `claude_code`。Provider structured outcome 必须映射到等待态或 final answer 契约。完整 Metadata 不进入 package；provider 初始只看到 outline/counts，通过 `logagent.query_metadata` 按需分页读取 slice。Python V2 task MCP 也会把成功的 `resources/read` 和 `tools/call` 追加到 `mcp_calls.jsonl`，并通过 `mcp_calls` resource 和 run analysis resources 暴露解析后的调用列表；同时补齐 V1 兼容资源 `artifact_index`、`case_context` 和 `tool_results`，分别用于发现当前 run 产物、读取最近 Case 背景和聚合工具/Fetch 结果。Task MCP resource 主 URI 为 `logagent://task/<run_id>/<resource>`，并保留 `logagent-v2://run/<run_id>/<resource>` alias。V2 也持久化 `session_text_input.json`，允许最终答案引用 `session_text_input.json#question`，把 `case_<id>` / `历史案例 case_<id>` 规范化为 `case_context.json#cases/<index>`，并把 V1 grep aliases（`matches/<index>`、`matches/<start>-<end>`、`#<start>-#<end>`、可命中初始 grep 的行号/行号范围）规范化为 canonical `grep_results.json#matches/<index>`。HTTP result endpoint 在 final answer/result artifact 生成前返回 409 和当前 run status，成功后返回 finalAnswer、result/Markdown artifacts 和 evidence metadata。Task MCP `logagent.get_skill_reference` 返回稳定 `skill_references/skill_ref_<hash>.json` 背景 artifact envelope，包括 `backgroundRef`、`canonicalRef`、`evidenceRefs` 和 `finalEvidenceAllowed=false`。Python V2 的只读 MCP 和 task MCP handler 均接受单个 JSON-RPC request 或 JSON-RPC batch array；batch array 会按输入顺序返回每个 request 的响应。二者都支持 V1 的 `ping` 和空 `prompts/list`。选择 `claude_code` provider 时还会生成 `claude_prompt.md`、`claude_mcp_config.json` 和 `claude_session.json`。

只读 HTTP MCP 是独立接口面，面向个人本地 Claude Code 读取共享知识，不绑定 task，不读取 workspace，不执行 action。它只暴露 Case、Skills、Metadata、Tools catalog 和 Domain Adapter 摘要等资源和只读 tools。

## 核心数据

```rust
pub struct TaskContext {
    pub task_id: String,
    pub session_id: Option<String>,
    pub source: TaskSource,
    pub product: Option<String>,
    pub version: Option<String>,
    pub instance_id: Option<String>,
    pub cluster_id: Option<String>,
    pub node_id: Option<String>,
    pub question: String,
    pub workspace: PathBuf,
}

pub struct AnalysisContext {
    pub revision: u64,
    pub facts: Vec<Fact>,
    pub hypotheses: Vec<Hypothesis>,
    pub gaps: Vec<InformationGap>,
    pub pending_requests: Vec<PendingRequest>,
    pub budget: AnalysisBudget,
}

pub struct EvidenceBundle {
    pub manifest_path: Option<PathBuf>,
    pub log_evidence_paths: Vec<PathBuf>,
    pub tool_results_dir: Option<PathBuf>,
    pub code_evidence_paths: Vec<PathBuf>,
    pub environment_evidence_paths: Vec<PathBuf>,
    pub metadata_context_path: Option<PathBuf>,
    pub similar_cases: Vec<CaseRef>,
}
```

## 模块接口

```rust
pub trait LlmGateway {
    async fn decide(&self, input: AnalysisPromptInput) -> anyhow::Result<LlmDecision>;
}

pub trait AgentProviderRuntime {
    async fn run(&self, request: AgentProviderRequest)
        -> anyhow::Result<AgentProviderResponse>;
}

pub trait DomainAdapter {
    fn summarize(&self, task: &TaskContext, evidence: &EvidenceBundle)
        -> anyhow::Result<DomainContext>;
}

pub trait LogAnalyzer {
    async fn analyze(&self, ctx: &TaskContext) -> anyhow::Result<LogAnalysisOutput>;
    async fn search(&self, request: LogSearchRequest) -> anyhow::Result<LogSearchOutput>;
}

pub trait ToolRunner {
    async fn run(&self, request: ToolRequest) -> anyhow::Result<ToolRunOutput>;
}

pub trait CodeEvidenceProvider {
    async fn collect(&self, request: CodeEvidenceRequest)
        -> anyhow::Result<CodeEvidenceOutput>;
}

pub trait EnvironmentCollector {
    async fn collect(&self, request: EnvironmentRequest)
        -> anyhow::Result<EnvironmentOutput>;
}
```

`AgentProviderResponse` 只能是 `completed`、`waiting_for_user` 或 `waiting_for_approval`。Server 将等待请求映射到任务状态，并继续校验最终结果 evidence refs；模块不能反向控制任务状态。

## 状态与阶段

稳定状态：

```text
QUEUED
RUNNING
WAITING_FOR_USER
WAITING_FOR_APPROVAL
SUCCEEDED
FAILED
```

执行阶段单独记录：

```text
UPLOAD
COLLECT
EXTRACT
SEARCH_LOGS
RUN_TOOL
COLLECT_CODE
PLAN_ANALYSIS
EXECUTE_ACTION
ANALYZE_RESULT
GENERATE_RESULT
```

稳定状态供 API、恢复和终态判断使用；执行阶段供进度和审计使用。不得把每个内部步骤扩展为无法恢复的任务状态。

任务类型：

```text
log_analysis
tool_run
```

`log_analysis` 继续执行完整日志分析 pipeline。`tool_run` 从 `RUN_TOOL` phase 开始，由工具插件写入结果后直接进入 `SUCCEEDED`。

## MCP Tools

任务 stdio MCP tools：

```text
logagent.search_logs             # { keywords, maxMatches? }
logagent.get_log_slice           # { path, lineNumber, before?, after? } or { path, startLine, endLine }
logagent.run_domain_tool
logagent.list_fetch_endpoints
logagent.fetch
logagent.recall_cases
logagent.get_metadata_topology
logagent.query_metadata
logagent.get_metadata_field_types
logagent.get_metadata_tag_fields
logagent.request_user_input
logagent.request_approval
```

所有 MCP tool input 由 Server 检查 schema、预算、白名单、幂等和审批要求。Task MCP `summary` resource 保留 Rust/V1 顶层 `taskId`、`sessionId`、`analysisMode`、`analysisLanguage`、`question`、`sourceUrl`、`nodeId` 和 `uploadIds`，同时保留 V2 `run` / `workspace`。`logagent.search_logs.maxMatches` 是 V1 兼容可选参数，按 1..200 裁剪；响应保留 V2 `search` 对象，同时补齐 Rust/V1 顶层 `artifactPath`、`totalMatches`、`keywordCounts`、`unmatchedKeywords`、`matches`、`matches[].index`、`evidenceRefs` 和 `note`。`logagent.get_log_slice` 同时支持中心行和 V1 range 形态，但不能混用；响应保留 V2 `slice` 对象，同时补齐 Rust/V1 顶层 `artifactPath`、`evidenceRefs` 和 `lines`；slice artifact 的 `startLine` / `endLine` 保留请求范围，`lines[]` 只包含实际存在的行。`logagent.search_code` 仅在配置本地代码仓时广告，响应写入 `code_evidence/<action_id>.json` 并返回最终答案可引用的 `code_evidence/<action_id>.json#matches/<index>`。`logagent.run_domain_tool` 的 `tools/list` schema 同时广告 V2 `toolId` 和 Rust/V1 `tool + inputFile` 两种调用形态；响应保留 V2 `result/artifact/evidence`，并补齐 Rust/V1 顶层 `artifactPath`、`summary` 和 `evidenceRefs`；多输入工具额外返回 `artifactPaths`，有 findings 时返回最终答案可引用的 `finalEvidenceRefs`。`logagent.request_user_input` / `logagent.request_approval` 保留 V2 `action`，同时写入并返回 Rust/V1 `mcp_waiting_request.json`、`runtimeStatus` 和 `evidenceRefs`；`request_approval` 可只传 V1 必填的 `reason`，缺省 `actionType` 为 `manual_approval`。会产生证据的 tool 必须写入 workspace artifact 并返回 canonical evidence refs。`logagent.get_metadata_topology` 是兼容 alias，只返回 outline；`logagent.query_metadata` 写入 `metadata_slices/<stable_id>.json`，返回 background ref，不新增最终 evidence ref 类型。`logagent.get_metadata_field_types` / `logagent.get_metadata_tag_fields` 写入 `metadata_slices/field_types_<stable_id>.json` / `metadata_slices/tag_fields_<stable_id>.json`，响应同时提供 V2 顶层 `fields` 和 Rust/V1 `result` 包装。`logagent.list_fetch_endpoints` 在 Fetch 关闭时返回 JSON-RPC error，开启时返回 Rust/V1 `schemaVersion=1`、endpoint summary 和 `finalEvidenceAllowed=false`。`logagent.fetch` 的 response ref 是最终证据，格式为 `tool_results/<action_id>/result.json#response`，且只允许当前任务真实 Fetch action。

只读 HTTP MCP resources 使用 V1 `logagent://skills`、`logagent://skills/<skillId>`、`logagent://metadata/instances`、`logagent://metadata/instances/<instanceId>/snapshot`、`logagent://cases/recent`、`logagent://tools/catalog` 和 `logagent://domain-adapters` URI；Python V2 同时保留 `logagent-v2://...` alias，并在 `resources/list` 中广告静态集合、动态 Skill 和动态 Metadata snapshot 资源。

只读 HTTP MCP tools：

```text
logagent.search_cases
logagent.get_case
logagent.list_skills
logagent.get_skill
logagent.get_skill_reference
logagent.preview_system_context
logagent.list_metadata_instances
logagent.get_metadata_snapshot
logagent.get_metadata_field_types
logagent.get_metadata_tag_fields
logagent.list_tools
logagent.list_domain_adapters
```

`logagent.preview_system_context` 接受 `skillIds`、`product`、`version`、`environment` 和 `instanceId`，返回合并后的 `resources`、拆分的 `skillResources` / `systemResources` 以及 prompt preview，不写 task artifact。
`logagent.get_skill` 响应保留 V2 顶层 skill 字段并补齐 Rust/V1 `skill` 包装。
`logagent.get_metadata_snapshot` 响应保留 V2 顶层 snapshot 字段并补齐 Rust/V1 `snapshot` 包装。
`logagent.recall_cases` 响应返回 Rust/V1 兼容的 `artifactPath`、`caseCount` 和逐 Case `evidenceRefs`，并把逻辑路径写入 background `case_context` evidence。

只读 HTTP MCP tools 不能写 workspace artifact，不能创建或恢复 Session，不能上传、审批、运行 Tool Runner、执行 Fetch endpoint 或执行 Huawei package sync。

当前 Executor 已按持久化 phase 循环分派 handler，并在推进阶段时校验 expected phase。重启恢复保留中断 phase，为后续 `RUN_TOOL` 和 `PLAN_ANALYSIS` 分支提供基础。
