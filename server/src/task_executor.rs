use std::sync::Arc;

use tokio::sync::Semaphore;
use tracing::{error, warn};

use crate::{
    contracts::{EvidenceProvider, TaskContext},
    models::{GrepResults, Manifest, TaskPhase, TaskRecord},
    pipeline::{extract_task, generate_task_result, prepare_pipeline_run, search_task},
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
            extract_task(state.config.clone(), task.clone()).await?;
            continue_with(
                &state,
                &task.task_id,
                TaskPhase::Extract,
                TaskPhase::SearchLogs,
            )
            .await
        }
        TaskPhase::SearchLogs => {
            search_task(state.config.clone(), &task.task_id).await?;
            continue_with(
                &state,
                &task.task_id,
                TaskPhase::SearchLogs,
                TaskPhase::RunTool,
            )
            .await
        }
        TaskPhase::RunTool => {
            run_tool_phase(state.clone(), &task).await?;
            continue_with(
                &state,
                &task.task_id,
                TaskPhase::RunTool,
                TaskPhase::GenerateResult,
            )
            .await
        }
        TaskPhase::GenerateResult => {
            let result =
                generate_task_result(state.config.clone(), state.llm.clone(), task.clone()).await?;
            let workspace = state.config.storage.workspace_dir(&task.task_id);
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
        TaskPhase::PlanAnalysis => {
            anyhow::bail!("task phase {phase:?} is not executable in this release")
        }
    }
}

async fn run_tool_phase(state: Arc<AppState>, task: &TaskRecord) -> anyhow::Result<()> {
    let workspace = state.config.storage.workspace_dir(&task.task_id);
    let manifest = read_json::<Manifest>(&workspace.join("manifest.json")).await?;
    let grep = read_json::<GrepResults>(&workspace.join("grep_results.json")).await?;
    let context = TaskContext::from_record(task, workspace, None);
    for action in state.tool_runner.rule_based_actions(&manifest, &grep) {
        state.tool_runner.execute(&context, &action).await?;
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
        assert!(fixture
            .workspace
            .join("tool_results/act_tool_fake/result.json")
            .exists());
        let stdout = fs::read_to_string(
            fixture
                .workspace
                .join("tool_results/act_tool_fake/stdout.txt"),
        )
        .unwrap();
        assert!(stdout.contains("extracted/sample/sample.log"));
    }

    struct Fixture {
        root: PathBuf,
        workspace: PathBuf,
        task_id: String,
    }

    impl Fixture {
        fn new(phase: TaskPhase) -> Self {
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
            fs::write(raw_dir.join("sample.log"), "INFO start\nERROR failed\n").unwrap();
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
