use std::sync::Arc;

use tokio::sync::Semaphore;
use tracing::{error, warn};

use crate::{
    analysis_state,
    config::LogAnalyzerSettings,
    contracts::{ActionKind, AgentAction, EvidenceProvider, EvidenceRef, TaskContext},
    llm_gateway::{ActionDecision, AgentDecision},
    models::{GrepResults, Manifest, TaskPhase, TaskRecord},
    pipeline::{
        extract_task, generate_task_result, persist_final_answer_decision_result,
        prepare_pipeline_run, read_tool_results, search_task, search_task_with_settings,
    },
    state::AppState,
};

#[derive(Debug)]
pub struct TaskExecutor {
    permits: Arc<Semaphore>,
}

impl TaskExecutor {
    pub fn new(max_concurrent_tasks: usize) -> Self {
        Self {
            permits: Arc::new(Semaphore::new(max_concurrent_tasks.max(1))),
        }
    }

    pub fn enqueue(&self, state: Arc<AppState>, task_id: String) {
        let permits = self.permits.clone();
        tokio::spawn(async move {
            let permit = match permits.acquire_owned().await {
                Ok(permit) => permit,
                Err(err) => {
                    error!(task_id, "task executor closed: {err}");
                    return;
                }
            };
            let _permit = permit;
            if let Err(err) = execute(state.clone(), &task_id).await {
                error!(task_id, "task execution failed: {err}");
            }
        });
    }
}

async fn execute(state: Arc<AppState>, task_id: &str) -> anyhow::Result<()> {
    let mut task = match state.tasks.start_attempt(task_id, TaskPhase::Extract).await {
        Ok(record) => record,
        Err(err) => {
            warn!(task_id, "skipping task that is no longer queued: {err}");
            return Ok(());
        }
    };

    loop {
        let phase = task
            .phase
            .ok_or_else(|| anyhow::anyhow!("running task {task_id} has no phase"))?;
        task = match dispatch_phase(state.clone(), task, phase).await {
            Ok(DispatchOutcome::Continue(task)) => task,
            Ok(DispatchOutcome::Complete) => return Ok(()),
            Err(err) => {
                let workspace = state.config.storage.workspace_dir(task_id);
                if let Err(record_err) =
                    analysis_state::record_failure(&workspace, Some(phase), err.to_string())
                {
                    warn!(task_id, "failed to record analysis failure: {record_err}");
                }
                state
                    .tasks
                    .fail(task_id, Some(phase), err.to_string())
                    .await?;
                return Ok(());
            }
        };
    }
}

enum DispatchOutcome {
    Continue(TaskRecord),
    Complete,
}

async fn dispatch_phase(
    state: Arc<AppState>,
    task: TaskRecord,
    phase: TaskPhase,
) -> anyhow::Result<DispatchOutcome> {
    match phase {
        TaskPhase::Extract => {
            let workspace = state.config.storage.workspace_dir(&task.task_id);
            prepare_pipeline_run(&workspace).await?;
            analysis_state::initialize(&workspace, &task)?;
            extract_task(state.config.clone(), task.clone()).await?;
            analysis_state::record_manifest(&workspace, &task.task_id)?;
            continue_with(
                &state,
                &task.task_id,
                TaskPhase::Extract,
                TaskPhase::SearchLogs,
            )
            .await
        }
        TaskPhase::SearchLogs => {
            let workspace = state.config.storage.workspace_dir(&task.task_id);
            analysis_state::ensure_initialized(&workspace, &task)?;
            search_task(state.config.clone(), &task.task_id).await?;
            let grep = read_json::<GrepResults>(&workspace.join("grep_results.json")).await?;
            analysis_state::record_log_search(&workspace, &grep)?;
            continue_with(
                &state,
                &task.task_id,
                TaskPhase::SearchLogs,
                TaskPhase::RunTool,
            )
            .await
        }
        TaskPhase::RunTool => {
            let workspace = state.config.storage.workspace_dir(&task.task_id);
            analysis_state::ensure_initialized(&workspace, &task)?;
            run_tool_phase(state.clone(), &task).await?;
            continue_with(
                &state,
                &task.task_id,
                TaskPhase::RunTool,
                TaskPhase::PlanAnalysis,
            )
            .await
        }
        TaskPhase::PlanAnalysis => plan_analysis_phase(state.clone(), task).await,
        TaskPhase::GenerateResult => {
            let workspace = state.config.storage.workspace_dir(&task.task_id);
            analysis_state::ensure_initialized(&workspace, &task)?;
            let result =
                generate_task_result(state.config.clone(), state.llm.clone(), task.clone()).await?;
            state
                .tasks
                .succeed(
                    &task.task_id,
                    TaskPhase::GenerateResult,
                    workspace.join("manifest.json").display().to_string(),
                    workspace.join("grep_results.json").display().to_string(),
                    result.result_json_path.display().to_string(),
                    result.result_markdown_path.display().to_string(),
                )
                .await?;
            Ok(DispatchOutcome::Complete)
        }
    }
}

async fn plan_analysis_phase(
    state: Arc<AppState>,
    task: TaskRecord,
) -> anyhow::Result<DispatchOutcome> {
    let workspace = state.config.storage.workspace_dir(&task.task_id);
    analysis_state::ensure_initialized(&workspace, &task)?;
    let manifest = read_json::<Manifest>(&workspace.join("manifest.json")).await?;
    let grep = read_json::<GrepResults>(&workspace.join("grep_results.json")).await?;
    let tool_results = read_tool_results(&workspace).await?;
    let metadata_context = match task.metadata_context_path.as_deref() {
        Some(path) if std::path::Path::new(path) == workspace.join("metadata_context.json") => {
            Some(read_json(&workspace.join("metadata_context.json")).await?)
        }
        Some(_) => anyhow::bail!("task contains invalid metadataContextPath"),
        None => None,
    };
    let decision = state
        .llm
        .decide_next_action(
            &task.question,
            &manifest,
            &grep,
            metadata_context.as_ref(),
            &tool_results,
        )
        .await?;
    record_model_decision(&workspace, &decision)?;
    match decision {
        AgentDecision::Action { decision } => execute_action_decision(state, &task, decision).await,
        AgentDecision::FinalAnswer { result } => {
            let result = result.into_result(&grep, &tool_results)?;
            let output = persist_final_answer_decision_result(&workspace, result).await?;
            state
                .tasks
                .succeed(
                    &task.task_id,
                    TaskPhase::PlanAnalysis,
                    workspace.join("manifest.json").display().to_string(),
                    workspace.join("grep_results.json").display().to_string(),
                    output.result_json_path.display().to_string(),
                    output.result_markdown_path.display().to_string(),
                )
                .await?;
            Ok(DispatchOutcome::Complete)
        }
    }
}

async fn execute_action_decision(
    state: Arc<AppState>,
    task: &TaskRecord,
    decision: ActionDecision,
) -> anyhow::Result<DispatchOutcome> {
    match decision.kind {
        ActionKind::SearchLogs => {
            run_search_logs_action(state.clone(), task, &decision).await?;
            continue_with(
                &state,
                &task.task_id,
                TaskPhase::PlanAnalysis,
                TaskPhase::GenerateResult,
            )
            .await
        }
        ActionKind::RunTool => {
            let action = action_from_decision(task, decision);
            let workspace = state.config.storage.workspace_dir(&task.task_id);
            let context = TaskContext::from_record(task, workspace, None);
            let artifact = state.tool_runner.execute(&context, &action).await?;
            analysis_state::record_tool_artifact(&context.workspace, &action, &artifact)?;
            continue_with(
                &state,
                &task.task_id,
                TaskPhase::PlanAnalysis,
                TaskPhase::GenerateResult,
            )
            .await
        }
        ActionKind::FinalAnswer => {
            continue_with(
                &state,
                &task.task_id,
                TaskPhase::PlanAnalysis,
                TaskPhase::GenerateResult,
            )
            .await
        }
        _ => anyhow::bail!("unsupported action decision type {:?}", decision.kind),
    }
}

async fn run_search_logs_action(
    state: Arc<AppState>,
    task: &TaskRecord,
    decision: &ActionDecision,
) -> anyhow::Result<()> {
    let keywords = decision
        .input
        .get("keywords")
        .and_then(|value| value.as_array())
        .ok_or_else(|| anyhow::anyhow!("search_logs input.keywords must be an array"))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
                .ok_or_else(|| anyhow::anyhow!("search_logs keyword must be a non-empty string"))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let max_matches = decision
        .input
        .get("maxMatches")
        .and_then(|value| value.as_u64())
        .unwrap_or(50) as usize;
    search_task_with_settings(
        state.config.clone(),
        &task.task_id,
        LogAnalyzerSettings {
            keywords,
            max_matches,
        },
    )
    .await?;
    let workspace = state.config.storage.workspace_dir(&task.task_id);
    let grep = read_json::<GrepResults>(&workspace.join("grep_results.json")).await?;
    analysis_state::record_log_search(&workspace, &grep)?;
    Ok(())
}

fn action_from_decision(task: &TaskRecord, decision: ActionDecision) -> AgentAction {
    let action_id = decision.action_id.unwrap_or_else(|| {
        format!(
            "act_{}_{}",
            action_kind_suffix(decision.kind),
            stable_json_hash(&decision.input)
        )
    });
    let fingerprint = decision.fingerprint.unwrap_or_else(|| {
        format!(
            "{}:{}",
            action_kind_suffix(decision.kind),
            stable_json_hash(&decision.input)
        )
    });
    AgentAction {
        schema_version: 1,
        action_id,
        kind: decision.kind,
        reason: decision.reason,
        evidence_refs: decision
            .evidence_refs
            .into_iter()
            .map(parse_evidence_ref)
            .collect(),
        input: decision.input,
        risk: decision.risk,
        fingerprint: format!("task:{}:{fingerprint}", task.task_id),
    }
}

fn record_model_decision(
    workspace: &std::path::Path,
    decision: &AgentDecision,
) -> anyhow::Result<()> {
    let (action_id, message, evidence_refs, details) = match decision {
        AgentDecision::Action { decision } => (
            decision.action_id.clone(),
            format!("model selected {:?}: {}", decision.kind, decision.reason),
            decision.evidence_refs.clone(),
            serde_json::json!({ "decision": decision }),
        ),
        AgentDecision::FinalAnswer { result } => (
            None,
            "model selected final_answer".to_string(),
            result
                .likely_root_causes
                .iter()
                .flat_map(|cause| cause.evidence_refs.iter().cloned())
                .collect(),
            serde_json::json!({ "result": result }),
        ),
    };
    analysis_state::record_model_decision(
        workspace,
        TaskPhase::PlanAnalysis,
        action_id,
        message,
        evidence_refs,
        details,
    )
}

fn parse_evidence_ref(value: String) -> EvidenceRef {
    match value.split_once('#') {
        Some((artifact_path, selector)) => EvidenceRef {
            artifact_path: artifact_path.to_string(),
            selector: Some(selector.to_string()),
        },
        None => EvidenceRef {
            artifact_path: value,
            selector: None,
        },
    }
}

fn action_kind_suffix(kind: ActionKind) -> &'static str {
    match kind {
        ActionKind::SearchLogs => "search_logs",
        ActionKind::RunTool => "run_tool",
        ActionKind::CollectCodeEvidence => "collect_code",
        ActionKind::CollectEnvironment => "collect_env",
        ActionKind::AskUser => "ask_user",
        ActionKind::FinalAnswer => "final_answer",
    }
}

fn stable_json_hash(value: &serde_json::Value) -> u64 {
    use std::hash::{Hash, Hasher};

    let encoded = serde_json::to_string(value).unwrap_or_default();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    encoded.hash(&mut hasher);
    hasher.finish()
}

async fn run_tool_phase(state: Arc<AppState>, task: &TaskRecord) -> anyhow::Result<()> {
    let workspace = state.config.storage.workspace_dir(&task.task_id);
    let manifest = read_json::<Manifest>(&workspace.join("manifest.json")).await?;
    let grep = read_json::<GrepResults>(&workspace.join("grep_results.json")).await?;
    let context = TaskContext::from_record(task, workspace, None);
    for action in state.tool_runner.rule_based_actions(&manifest, &grep) {
        let artifact = state.tool_runner.execute(&context, &action).await?;
        analysis_state::record_tool_artifact(&context.workspace, &action, &artifact)?;
    }
    Ok(())
}

async fn read_json<T: serde::de::DeserializeOwned>(path: &std::path::Path) -> anyhow::Result<T> {
    let raw = tokio::fs::read_to_string(path).await?;
    Ok(serde_json::from_str(&raw)?)
}

async fn continue_with(
    state: &AppState,
    task_id: &str,
    current: TaskPhase,
    next: TaskPhase,
) -> anyhow::Result<DispatchOutcome> {
    let task = state.tasks.advance_phase(task_id, current, next).await?;
    Ok(DispatchOutcome::Continue(task))
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::PathBuf,
        sync::Arc,
        time::{SystemTime, UNIX_EPOCH},
    };

    use chrono::Utc;

    use super::*;
    use crate::{
        config::{
            AppConfig, AuthSettings, LlmProvider, LlmSettings, LogAnalyzerSettings, ServerSettings,
            StorageSettings, ToolMatchSettings, ToolSettings, ToolsSettings,
        },
        models::{TaskInput, TaskSource, TaskStatus},
        pipeline::{extract_task, prepare_pipeline_run, search_task},
    };

    #[tokio::test]
    async fn resumes_from_persisted_search_and_generate_phases() {
        for phase in [TaskPhase::SearchLogs, TaskPhase::GenerateResult] {
            let fixture = Fixture::new(phase);
            let state = fixture.state();
            let task = fixture.task(phase);
            state.tasks.create(task.clone()).await.unwrap();
            prepare_pipeline_run(&fixture.workspace).await.unwrap();
            extract_task(state.config.clone(), task.clone())
                .await
                .unwrap();
            if phase == TaskPhase::GenerateResult {
                search_task(state.config.clone(), &task.task_id)
                    .await
                    .unwrap();
            }

            let recovered = state.tasks.recover_incomplete().await.unwrap();
            assert_eq!(recovered.len(), 1);
            assert_eq!(recovered[0].phase, Some(phase));
            execute(state.clone(), &task.task_id).await.unwrap();

            let completed = state.tasks.get(&task.task_id).await.unwrap();
            assert_eq!(completed.status, TaskStatus::Succeeded);
            assert_eq!(completed.phase, None);
            assert_eq!(completed.attempts, 2);
            assert!(fixture.workspace.join("result.json").exists());
        }
    }

    #[tokio::test]
    async fn dispatcher_runs_configured_tools_before_generating_result() {
        let fixture = Fixture::new(TaskPhase::Extract);
        let tool_path = fixture.write_tool(
            "fake_tool.sh",
            "#!/usr/bin/env bash\nprintf 'input=%s' \"$1\"\n",
        );
        let state = fixture.state_with_tools(ToolsSettings {
            tools: [(
                "fake".to_string(),
                ToolSettings {
                    name: "fake".to_string(),
                    enabled: true,
                    path: tool_path,
                    timeout_seconds: 5,
                    max_output_bytes: 1024,
                    max_input_files: 1,
                    args: vec!["{input_file}".to_string()],
                    match_settings: ToolMatchSettings {
                        file_patterns: vec!["*.log".to_string()],
                        keywords: vec![],
                    },
                },
            )]
            .into_iter()
            .collect(),
        });
        let mut task = fixture.task(TaskPhase::Extract);
        task.status = TaskStatus::Queued;
        task.phase = None;
        task.attempts = 0;
        state.tasks.create(task.clone()).await.unwrap();

        execute(state.clone(), &task.task_id).await.unwrap();

        let completed = state.tasks.get(&task.task_id).await.unwrap();
        assert_eq!(completed.status, TaskStatus::Succeeded);
        let tool_results_dir = fixture.workspace.join("tool_results");
        let result_dirs = fs::read_dir(&tool_results_dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        assert_eq!(result_dirs.len(), 1);
        assert!(result_dirs[0].join("result.json").exists());
        let stdout = fs::read_to_string(result_dirs[0].join("stdout.txt")).unwrap();
        assert!(stdout.contains("extracted/sample/sample.log"));
        let snapshot = analysis_state::read_snapshot(&fixture.workspace).unwrap();
        assert_eq!(snapshot.state.budget.llm_calls, 1);
    }

    #[tokio::test]
    async fn plan_analysis_executes_stub_search_action_before_result() {
        let fixture = Fixture::new_with_log(TaskPhase::Extract, "INFO start\nWARN slow\n");
        let state = fixture.state();
        let mut task = fixture.task(TaskPhase::Extract);
        task.status = TaskStatus::Queued;
        task.phase = None;
        task.attempts = 0;
        state.tasks.create(task.clone()).await.unwrap();

        execute(state.clone(), &task.task_id).await.unwrap();

        let completed = state.tasks.get(&task.task_id).await.unwrap();
        assert_eq!(completed.status, TaskStatus::Succeeded);
        let grep: GrepResults = serde_json::from_str(
            &fs::read_to_string(fixture.workspace.join("grep_results.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(grep.keywords, vec!["error", "timeout", "failed"]);
    }

    struct Fixture {
        root: PathBuf,
        workspace: PathBuf,
        task_id: String,
    }

    impl Fixture {
        fn new(phase: TaskPhase) -> Self {
            Self::new_with_log(phase, "INFO start\nERROR failed\n")
        }

        fn new_with_log(phase: TaskPhase, log: &str) -> Self {
            let suffix = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root =
                std::env::temp_dir().join(format!("logagent-executor-resume-{phase:?}-{suffix}"));
            let task_id = format!("task_resume_{}", phase_name(phase));
            let workspace = root.join("workspaces").join(&task_id);
            let raw_dir = workspace.join("raw/upl_1");
            fs::create_dir_all(&raw_dir).unwrap();
            fs::write(raw_dir.join("sample.log"), log).unwrap();
            Self {
                root,
                workspace,
                task_id,
            }
        }

        fn state(&self) -> Arc<AppState> {
            self.state_with_tools(ToolsSettings::default())
        }

        fn state_with_tools(&self, tools: ToolsSettings) -> Arc<AppState> {
            let config = Arc::new(AppConfig {
                server: ServerSettings {
                    bind: "127.0.0.1:0".to_string(),
                    public_base_url: "http://127.0.0.1:0".to_string(),
                    max_concurrent_tasks: 1,
                },
                auth: AuthSettings { api_keys: vec![] },
                storage: StorageSettings {
                    data_dir: self.root.clone(),
                    max_upload_bytes: 1024 * 1024,
                    max_chunk_bytes: 512 * 1024,
                },
                log_analyzer: LogAnalyzerSettings {
                    keywords: vec!["error".to_string()],
                    max_matches: 20,
                },
                tools,
                llm: LlmSettings {
                    provider: LlmProvider::Stub,
                    base_url: None,
                    api_key: None,
                    model: "stub".to_string(),
                    request_timeout_seconds: 1,
                    max_input_chars: 60_000,
                    max_output_tokens: 100,
                },
            });
            config.prepare_dirs().unwrap();
            AppState::new(config).unwrap()
        }

        fn write_tool(&self, filename: &str, content: &str) -> PathBuf {
            let path = self.root.join(filename);
            fs::write(&path, content).unwrap();
            let mut permissions = fs::metadata(&path).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&path, permissions).unwrap();
            path
        }

        fn task(&self, phase: TaskPhase) -> TaskRecord {
            let now = Utc::now();
            TaskRecord {
                schema_version: 4,
                task_id: self.task_id.clone(),
                source: TaskSource::Upload,
                upload_ids: vec!["upl_1".to_string()],
                inputs: vec![TaskInput {
                    upload_id: "upl_1".to_string(),
                    filename: "sample.log".to_string(),
                    size: 24,
                    raw_path: "raw/upl_1/sample.log".to_string(),
                }],
                source_url: None,
                instance_id: None,
                cluster_id: None,
                node_id: None,
                question: "why did it fail?".to_string(),
                status: TaskStatus::Running,
                phase: Some(phase),
                attempts: 1,
                error: None,
                manifest_path: None,
                grep_results_path: None,
                metadata_context_path: None,
                result_json_path: None,
                result_markdown_path: None,
                created_at: now,
                updated_at: now,
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn phase_name(phase: TaskPhase) -> &'static str {
        match phase {
            TaskPhase::SearchLogs => "search",
            TaskPhase::GenerateResult => "generate",
            _ => "other",
        }
    }
}
