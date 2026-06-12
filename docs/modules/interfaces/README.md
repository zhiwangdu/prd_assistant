# Module Interfaces 方案

## 目标

MVP 使用单一 Rust binary 和内部模块边界。Server 持有任务状态和执行权限，Analysis Orchestrator 持有证据包构建、Claude MCP 配置、预算和等待态，Claude Code 只提供推理/代码上下文分析，证据模块只执行受约束能力。

## 当前实现

Server 已在 `server/src/domain/contracts.rs` 落地第一版公共契约，并新增 Claude Code / Domain Adapter 摘要接口：

- `TaskContext`
- `AgentAction` / `ActionKind` / `ActionRisk`
- `EvidenceRef`
- `EvidenceArtifact` / `EvidenceType` / `EvidenceSummary`
- `EvidenceProvider`

Action 和 Evidence 使用稳定 JSON 名称，artifact 路径必须是 workspace 相对路径。Tool Runner 已成为第一个消费该契约的模块：规则版工具 action、Tools 页面手动运行和 Claude MCP `logagent.run_domain_tool` 走同一执行接口。

Server 现在也支持 Log Analysis Session 和 `taskKind=tool_run` 的手动工具运行任务。Session 是用户可见的恢复单元，保存草稿、上传引用、历史 task runs 和 timeline；每次分析 run 仍创建一个绑定 `sessionId` 的 `taskKind=log_analysis` task workspace 快照。`tool_run` 路径复用 TaskStore、workspace 和 `tool_results` 产物，但不绑定 Session、不进入 `PLAN_ANALYSIS`；Claude MCP `logagent.run_domain_tool` 复用同一个工具 registry。

Claude Code Session Runner 提供配置摘要、dry-run 诊断和 `analysis_package.json` / `claude_prompt.md` / `claude_mcp_config.json` / `claude_session.json` / `mcp_calls.jsonl` / `agent_response.json` session 输入/响应产物。`PLAN_ANALYSIS` 当前直接调用 Claude Code CLI；CLI 只接收短 stdin prompt，证据包通过任务 MCP `analysis_package` resource 读取，structured outcome 必须映射到等待态或 final answer 契约。完整 Metadata 不进入 package，Claude 初始只看到 outline/counts，通过 `logagent.query_metadata` 按需分页读取 slice。

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

pub trait ClaudeSessionRunner {
    async fn run(&self, request: ClaudeSessionRequest)
        -> anyhow::Result<ClaudeSessionResponse>;
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

`ClaudeSessionResponse` 只能是 `completed`、`waiting_for_user` 或 `waiting_for_approval`。Server 将等待请求映射到任务状态，并继续校验最终结果 evidence refs；模块不能反向控制任务状态。

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
logagent.search_logs
logagent.get_log_slice
logagent.run_domain_tool
logagent.recall_cases
logagent.get_metadata_topology
logagent.query_metadata
logagent.request_user_input
logagent.request_approval
```

所有 MCP tool input 由 Server 检查 schema、预算、白名单、幂等和审批要求。会产生证据的 tool 必须写入 workspace artifact 并返回 canonical evidence refs。`logagent.get_metadata_topology` 是兼容 alias，只返回 outline；`logagent.query_metadata` 写入 `metadata_slices/<stable_id>.json`，返回 background ref，不新增最终 evidence ref 类型。

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
logagent.list_tools
logagent.list_domain_adapters
```

只读 HTTP MCP tools 不能写 workspace artifact，不能创建或恢复 Session，不能上传、审批或运行 Tool Runner。

当前 Executor 已按持久化 phase 循环分派 handler，并在推进阶段时校验 expected phase。重启恢复保留中断 phase，为后续 `RUN_TOOL` 和 `PLAN_ANALYSIS` 分支提供基础。
