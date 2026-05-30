# Module Interfaces 方案

## 目标

MVP 采用单一 Rust binary + 内部 crate/module 拆分。Server 负责编排，各分析模块负责执行。

边界：

- Server：任务状态、API、调度、错误汇总。
- Log Analyzer：解压、manifest、rg 检索、日志摘要。
- Tool Runner：外部工具调用。
- Code Evidence：版本代码检索。
- Environment Collector：测试环境采集。
- LLM Agent：证据裁剪、Prompt 组装、模型调用。

## 核心数据类型

```rust
pub struct TaskContext {
    pub task_id: String,
    pub source: TaskSource,
    pub product: Option<String>,
    pub version: Option<String>,
    pub question: String,
    pub workspace: PathBuf,
}

pub enum TaskSource {
    Upload,
    Environment,
}

pub struct EvidenceBundle {
    pub manifest_path: Option<PathBuf>,
    pub error_summary_path: Option<PathBuf>,
    pub contexts_path: Option<PathBuf>,
    pub tool_results_dir: Option<PathBuf>,
    pub code_evidence_path: Option<PathBuf>,
    pub environment_evidence_path: Option<PathBuf>,
    pub similar_cases: Vec<CaseRef>,
}
```

## 模块接口

```rust
pub trait LogAnalyzer {
    async fn analyze(&self, ctx: &TaskContext) -> anyhow::Result<LogAnalysisOutput>;
}

pub trait ToolRunner {
    async fn run_tools(
        &self,
        ctx: &TaskContext,
        log_output: &LogAnalysisOutput,
    ) -> anyhow::Result<ToolRunOutput>;
}

pub trait CodeEvidenceProvider {
    async fn collect_code_evidence(
        &self,
        ctx: &TaskContext,
        evidence: &EvidenceBundle,
    ) -> anyhow::Result<Option<CodeEvidenceOutput>>;
}

pub trait EnvironmentCollector {
    async fn collect_environment(&self, ctx: &TaskContext) -> anyhow::Result<EnvironmentOutput>;
}

pub trait LlmAgent {
    async fn analyze(&self, ctx: &TaskContext, evidence: &EvidenceBundle) -> anyhow::Result<LlmResult>;
}
```

## 状态机

```text
CREATED
UPLOADED
COLLECTING
EXTRACTING
SEARCHING
RUNNING_TOOLS
COLLECTING_CODE
ANALYZING
DONE
FAILED
```

`COLLECTING` 只用于 environment 来源任务；upload 来源任务从 `UPLOADED` 进入 `EXTRACTING`。

