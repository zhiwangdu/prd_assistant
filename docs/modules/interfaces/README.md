# Module Interfaces 方案

## 目标

MVP 使用单一 Rust binary 和内部模块边界。Server 持有任务状态和执行权限，Analysis Orchestrator 持有证据包构建、预算和动作校验，Agent Backend 只提供推理/代码上下文分析，证据模块只执行受约束能力。

## 当前实现

Server 已在 `server/src/contracts.rs` 落地第一版公共契约，并新增 Agent Backend / Domain Adapter 摘要接口：

- `TaskContext`
- `AgentAction` / `ActionKind` / `ActionRisk`
- `EvidenceRef`
- `EvidenceArtifact` / `EvidenceType` / `EvidenceSummary`
- `EvidenceProvider`

Action 和 Evidence 使用稳定 JSON 名称，artifact 路径必须是 workspace 相对路径。Tool Runner 已成为第一个消费该契约的模块：规则版 `run_tool` action 和未来 LLM action 走同一执行接口。

Server 现在也支持 Log Analysis Session 和 `taskKind=tool_run` 的手动工具运行任务。Session 是用户可见的恢复单元，保存草稿、上传引用、历史 task runs 和 timeline；每次分析 run 仍创建一个绑定 `sessionId` 的 `taskKind=log_analysis` task workspace 快照。`tool_run` 路径复用 TaskStore、workspace 和 `tool_results` 产物，但不绑定 Session、不进入 `PLAN_ANALYSIS`；后续 Analysis Orchestrator 的 `run_tool` action 会逐步复用同一个工具 registry。

Agent Backend 第一阶段提供配置摘要、dry-run 诊断和 `analysis_package.json` / `agent_request.json` / `agent_response.json` 契约产物，不改变现有 `PLAN_ANALYSIS` 执行路径。当前 `agent_response.json` 为 `not_invoked` 占位；后续外部后端实际输出仍必须映射到现有 action/final answer 契约。

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
pub trait AnalysisAgent {
    async fn next_step(
        &self,
        task: &TaskContext,
        analysis: &AnalysisContext,
        evidence: &EvidenceBundle,
    ) -> anyhow::Result<AgentDecision>;
}

pub trait LlmGateway {
    async fn decide(&self, input: AnalysisPromptInput) -> anyhow::Result<LlmDecision>;
}

pub trait AgentBackend {
    async fn run(&self, request: AgentBackendRequest)
        -> anyhow::Result<AgentBackendResponse>;
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

`AgentDecision` 只能是受支持的结构化 action 或 `final_answer`。Server 将 action 映射到对应模块，模块不能反向控制任务状态。

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

## Action

```rust
pub enum AgentActionKind {
    SearchLogs,
    RunTool,
    CollectCodeEvidence,
    CollectEnvironment,
    AskUser,
    FinalAnswer,
}
```

所有 action 包含 id、reason、evidence refs、typed input、risk 和 fingerprint。Server 在执行前检查 schema、预算、白名单、幂等和审批要求。

当前 Executor 已按持久化 phase 循环分派 handler，并在推进阶段时校验 expected phase。重启恢复保留中断 phase，为后续 `RUN_TOOL` 和 `PLAN_ANALYSIS` 分支提供基础。
