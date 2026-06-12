use std::sync::Arc;

use tokio::sync::Semaphore;
use tracing::{error, info, warn};

use crate::{
    app::AppState,
    domain::{
        contracts::{
            ActionKind, ActionRisk, AgentAction, EvidenceProvider, EvidenceRef, TaskContext,
        },
        models::{
            AnalysisResult, Confidence, GrepResults, Manifest, ResultOutput, RootCause,
            SystemContextBundle, TaskKind, TaskPhase, TaskRecord,
        },
    },
    pipeline::{
        extract_task, generate_task_result, persist_final_answer_decision_result,
        prepare_pipeline_run, read_optional_json, read_tool_results, search_task,
    },
    services::metadata::TaskMetadataContext,
    services::{
        agent_backend::{AgentBackendDecisionInput, ClaudeSessionOutcome},
        agent_contracts::{write_agent_contracts, AgentContractInput},
    },
    stores::analysis_state,
    support::{config::AnalysisSettings, id::next_id},
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
        info!(task_id = %task_id, "task enqueued");
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
            info!(task_id = %task_id, "task execution started");
            if let Err(err) = execute(state.clone(), &task_id).await {
                error!(task_id, "task execution failed: {err}");
            }
        });
    }
}

async fn execute(state: Arc<AppState>, task_id: &str) -> anyhow::Result<()> {
    let initial_phase = state
        .tasks
        .get(task_id)
        .await
        .map(|task| match task.task_kind {
            TaskKind::LogAnalysis => TaskPhase::Extract,
            TaskKind::ToolRun => TaskPhase::RunTool,
        })
        .unwrap_or(TaskPhase::Extract);
    let mut task = match state.tasks.start_attempt(task_id, initial_phase).await {
        Ok(record) => record,
        Err(err) => {
            warn!(task_id, "skipping task that is no longer queued: {err}");
            return Ok(());
        }
    };
    info!(
        task_id = %task.task_id,
        task_kind = ?task.task_kind,
        phase = ?task.phase,
        attempt = task.attempts,
        "task attempt started"
    );
    sync_session_status(&state, &task).await;

    loop {
        let phase = task
            .phase
            .ok_or_else(|| anyhow::anyhow!("running task {task_id} has no phase"))?;
        task = match dispatch_phase(state.clone(), task, phase).await {
            Ok(DispatchOutcome::Continue(task)) => task,
            Ok(DispatchOutcome::Complete) => return Ok(()),
            Err(err) => {
                let workspace = state.config.storage.workspace_dir(task_id);
                error!(
                    task_id = %task_id,
                    phase = ?phase,
                    error = %err,
                    "task phase failed"
                );
                if let Err(record_err) =
                    analysis_state::record_failure(&workspace, Some(phase), err.to_string())
                {
                    warn!(task_id, "failed to record analysis failure: {record_err}");
                }
                let failed = state
                    .tasks
                    .fail(task_id, Some(phase), err.to_string())
                    .await?;
                sync_session_status(&state, &failed).await;
                info!(
                    task_id = %task_id,
                    phase = ?phase,
                    "task marked failed"
                );
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
    info!(
        task_id = %task.task_id,
        phase = ?phase,
        task_kind = ?task.task_kind,
        "task phase started"
    );
    // Phase dispatch is intentionally driven by persisted task.phase so recovered tasks
    // resume at the last durable boundary instead of replaying the whole pipeline.
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
            if task.task_kind == TaskKind::ToolRun {
                let result_path =
                    crate::services::tools::run_tool_task(state.config.clone(), task.clone())
                        .await?
                        .display()
                        .to_string();
                let completed = state
                    .tasks
                    .succeed_tool_run(&task.task_id, TaskPhase::RunTool, result_path)
                    .await?;
                sync_session_status(&state, &completed).await;
                info!(
                    task_id = %completed.task_id,
                    tool_id = ?completed.tool_id,
                    "tool run task succeeded"
                );
                return Ok(DispatchOutcome::Complete);
            }
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
            let completed =
                complete_successful_log_analysis(&state, &task, TaskPhase::GenerateResult, result)
                    .await?;
            sync_session_status(&state, &completed).await;
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

    loop {
        info!(
            task_id = %task.task_id,
            phase = ?TaskPhase::PlanAnalysis,
            "planning analysis round"
        );
        let manifest = read_json::<Manifest>(&workspace.join("manifest.json")).await?;
        let grep = read_json::<GrepResults>(&workspace.join("grep_results.json")).await?;
        let tool_results = read_tool_results(&workspace).await?;
        let case_context =
            read_optional_json::<serde_json::Value>(&workspace.join("case_context.json")).await?;
        let system_context = match task.system_context_path.as_deref() {
            Some(path) if std::path::Path::new(path) == workspace.join("system_context.json") => {
                Some(
                    read_json::<SystemContextBundle>(&workspace.join("system_context.json"))
                        .await?,
                )
            }
            Some(_) => anyhow::bail!("task contains invalid systemContextPath"),
            None => {
                read_optional_json::<SystemContextBundle>(&workspace.join("system_context.json"))
                    .await?
            }
        };
        if let Some(reason) = analysis_budget_exhausted(&workspace, &state.config.analysis)? {
            warn!(
                task_id = %task.task_id,
                reason = %reason,
                "analysis budget exhausted"
            );
            return complete_with_budget_limited_result(state, &task, &grep, reason).await;
        }
        let metadata_context = match task.metadata_context_path.as_deref() {
            Some(path) if std::path::Path::new(path) == workspace.join("metadata_context.json") => {
                Some(
                    read_json::<TaskMetadataContext>(&workspace.join("metadata_context.json"))
                        .await?,
                )
            }
            Some(_) => anyhow::bail!("task contains invalid metadataContextPath"),
            None => None,
        };
        let snapshot = analysis_state::read_snapshot(&workspace)?;
        let system_context_value = system_context
            .as_ref()
            .map(serde_json::to_value)
            .transpose()?;
        let contracts_existed = workspace.join("analysis_package.json").exists();
        write_agent_contracts(
            &workspace,
            AgentContractInput {
                task: &task,
                manifest: &manifest,
                grep_results: &grep,
                metadata_context: metadata_context.as_ref(),
                system_context: system_context_value.as_ref(),
                case_context: case_context.as_ref(),
                tool_results: &tool_results,
                analysis_snapshot: &snapshot,
                claude_code: &state.config.claude_code,
                mcp: &state.config.mcp,
                config_path: &state.config.config_path,
                analysis_mode: task.analysis_mode,
                tools: &state.config.tools,
            },
        )
        .await?;
        if !contracts_existed {
            analysis_state::record_agent_contracts(&workspace, "claude_code")?;
            info!(
                task_id = %task.task_id,
                "agent contracts written"
            );
        }
        let call_id = next_id("agentcall");
        let backend_id = "claude_code".to_string();
        analysis_state::record_llm_call_started(
            &workspace,
            TaskPhase::PlanAnalysis,
            call_id.clone(),
            "agent_backend_decision".to_string(),
            1,
            backend_id.clone(),
        )?;
        let decision = state
            .agent_backends
            .decide_next(AgentBackendDecisionInput {
                workspace: &workspace,
                analysis_mode: task.analysis_mode,
                grep_results: &grep,
                case_context: case_context.as_ref(),
                tool_results: &tool_results,
            })
            .await?;
        analysis_state::record_llm_call_completed(
            &workspace,
            TaskPhase::PlanAnalysis,
            call_id,
            "agent_backend_decision".to_string(),
            1,
            backend_id,
        )?;
        let claude_session_id =
            read_optional_json::<serde_json::Value>(&workspace.join("claude_session.json"))
                .await?
                .and_then(|value| {
                    value
                        .get("claudeSessionId")
                        .and_then(|value| value.as_str())
                        .map(ToString::to_string)
                });
        let permission_profile = state
            .config
            .claude_code
            .permission_profiles
            .get(&task.analysis_mode)
            .map(|profile| profile.name.clone())
            .unwrap_or_else(|| task.analysis_mode.as_str().to_string());
        analysis_state::record_claude_session_artifacts(
            &workspace,
            claude_session_id,
            task.analysis_mode.as_str().to_string(),
            permission_profile,
        )?;
        record_claude_outcome(&workspace, &decision)?;
        if let Some(waiting_action) = read_mcp_waiting_marker(&task, &workspace).await? {
            info!(
                task_id = %task.task_id,
                action_id = %waiting_action.action_id,
                action_kind = ?waiting_action.kind,
                "MCP waiting marker converted to task action"
            );
            return wait_for_agent_action(state.clone(), &task, waiting_action).await;
        }
        match decision {
            ClaudeSessionOutcome::FinalAnswer { result } => {
                info!(
                    task_id = %task.task_id,
                    "Claude Code returned final answer"
                );
                let result = result.into_result(&grep, case_context.as_ref(), &tool_results)?;
                let output = persist_final_answer_decision_result(&workspace, result).await?;
                let completed = complete_successful_log_analysis(
                    &state,
                    &task,
                    TaskPhase::PlanAnalysis,
                    output,
                )
                .await?;
                sync_session_status(&state, &completed).await;
                return Ok(DispatchOutcome::Complete);
            }
            ClaudeSessionOutcome::WaitingForUser { prompt } => {
                info!(
                    task_id = %task.task_id,
                    question_id = ?prompt.question_id,
                    "Claude Code requested user input"
                );
                let action = ask_user_action_from_prompt(&task, prompt);
                return wait_for_agent_action(state.clone(), &task, action).await;
            }
            ClaudeSessionOutcome::WaitingForApproval { approval } => {
                info!(
                    task_id = %task.task_id,
                    action_id = ?approval.action_id,
                    action_type = %approval.action_type,
                    "Claude Code requested approval"
                );
                let action = approval_action_from_request(&task, approval);
                return wait_for_agent_action(state.clone(), &task, action).await;
            }
        }
    }
}

async fn read_mcp_waiting_marker(
    task: &TaskRecord,
    workspace: &std::path::Path,
) -> anyhow::Result<Option<AgentAction>> {
    // MCP tools cannot mutate TaskStore directly; they leave this marker for the executor
    // to validate and translate into the persisted WAITING_* task states.
    let marker_path = workspace.join("mcp_waiting_request.json");
    let Some(marker) = read_optional_json::<serde_json::Value>(&marker_path).await? else {
        return Ok(None);
    };
    let _ = tokio::fs::remove_file(&marker_path).await;
    let status = marker
        .get("runtimeStatus")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let request = marker
        .get("request")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let action = match status {
        "waiting_for_user" => {
            let prompt = crate::services::agent_backend::ClaudeUserPrompt {
                question_id: request
                    .get("questionId")
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string),
                question: request
                    .get("question")
                    .and_then(|value| value.as_str())
                    .unwrap_or("Please provide the requested diagnostic detail.")
                    .to_string(),
                reason: request
                    .get("reason")
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string),
                required: request
                    .get("required")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(true),
                answer_format: request
                    .get("answerFormat")
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string),
            };
            ask_user_action_from_prompt(task, prompt)
        }
        "waiting_for_approval" => {
            let approval = crate::services::agent_backend::ClaudeApprovalRequest {
                action_id: request
                    .get("actionId")
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string),
                action_type: request
                    .get("actionType")
                    .and_then(|value| value.as_str())
                    .unwrap_or("collect_environment")
                    .to_string(),
                reason: request
                    .get("reason")
                    .and_then(|value| value.as_str())
                    .unwrap_or("Claude Code requested an approval-gated action.")
                    .to_string(),
                input: request
                    .get("input")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({ "scope": "default" })),
                evidence_refs: request
                    .get("evidenceRefs")
                    .and_then(|value| value.as_array())
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|value| value.as_str().map(ToString::to_string))
                            .collect()
                    })
                    .unwrap_or_default(),
            };
            approval_action_from_request(task, approval)
        }
        _ => return Ok(None),
    };
    Ok(Some(action))
}

async fn complete_successful_log_analysis(
    state: &AppState,
    task: &TaskRecord,
    phase: TaskPhase,
    output: ResultOutput,
) -> anyhow::Result<TaskRecord> {
    let workspace = state.config.storage.workspace_dir(&task.task_id);
    info!(
        task_id = %task.task_id,
        phase = ?phase,
        result_json_path = %output.result_json_path.display(),
        "completing successful log analysis task"
    );
    let result: AnalysisResult = read_json(&output.result_json_path).await?;
    let manifest: Manifest = read_json(&workspace.join("manifest.json")).await?;
    let metadata_context = match task.metadata_context_path.as_deref() {
        Some(path) if std::path::Path::new(path) == workspace.join("metadata_context.json") => {
            Some(read_json::<TaskMetadataContext>(&workspace.join("metadata_context.json")).await?)
        }
        Some(_) => None,
        None => None,
    };
    let alias = match state
        .llm
        .generate_task_alias(
            &task.question,
            &manifest,
            &result,
            metadata_context.as_ref(),
        )
        .await
    {
        Ok(alias) => alias,
        Err(err) => {
            warn!(
                task_id = task.task_id,
                "task alias generation failed: {err}"
            );
            crate::services::llm_gateway::fallback_task_alias(&result, &task.question)
        }
    };
    state
        .tasks
        .succeed(
            &task.task_id,
            phase,
            workspace.join("manifest.json").display().to_string(),
            workspace.join("grep_results.json").display().to_string(),
            output.result_json_path.display().to_string(),
            output.result_markdown_path.display().to_string(),
            Some(alias),
        )
        .await
}

async fn wait_for_agent_action(
    state: Arc<AppState>,
    task: &TaskRecord,
    action: AgentAction,
) -> anyhow::Result<DispatchOutcome> {
    let workspace = state.config.storage.workspace_dir(&task.task_id);
    match action.kind {
        ActionKind::AskUser => {
            let question = action
                .input
                .get("question")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow::anyhow!("ask_user action is missing question"))?
                .to_string();
            let required = action
                .input
                .get("required")
                .and_then(|value| value.as_bool())
                .unwrap_or(true);
            let answer_format = action
                .input
                .get("answerFormat")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string);
            let question_id = action
                .input
                .get("questionId")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
                .unwrap_or_else(|| action.action_id.clone());
            analysis_state::record_pending_user_prompt(
                &workspace,
                &action,
                question_id.clone(),
                question,
                required,
                answer_format,
            )?;
            let waiting = state.tasks.wait_for_user(&task.task_id).await?;
            sync_session_status(&state, &waiting).await;
            info!(
                task_id = %task.task_id,
                action_id = %action.action_id,
                question_id = %question_id,
                "task is waiting for user input"
            );
            Ok(DispatchOutcome::Complete)
        }
        ActionKind::CollectEnvironment if action.risk == ActionRisk::RequiresApproval => {
            analysis_state::record_pending_approval(&workspace, &action)?;
            let waiting = state.tasks.wait_for_approval(&task.task_id).await?;
            sync_session_status(&state, &waiting).await;
            info!(
                task_id = %task.task_id,
                action_id = %action.action_id,
                action_kind = ?action.kind,
                "task is waiting for approval"
            );
            Ok(DispatchOutcome::Complete)
        }
        _ if action.risk == ActionRisk::RequiresApproval => {
            analysis_state::record_pending_approval(&workspace, &action)?;
            let waiting = state.tasks.wait_for_approval(&task.task_id).await?;
            sync_session_status(&state, &waiting).await;
            info!(
                task_id = %task.task_id,
                action_id = %action.action_id,
                action_kind = ?action.kind,
                "task is waiting for approval"
            );
            Ok(DispatchOutcome::Complete)
        }
        _ => anyhow::bail!("action {:?} cannot enter waiting state", action.kind),
    }
}

fn analysis_budget_exhausted(
    workspace: &std::path::Path,
    settings: &AnalysisSettings,
) -> anyhow::Result<Option<String>> {
    let snapshot = analysis_state::read_snapshot(workspace)?;
    let budget = snapshot.state.budget;
    if budget.rounds >= settings.max_rounds {
        return Ok(Some(format!(
            "analysis round budget exhausted: {}/{}",
            budget.rounds, settings.max_rounds
        )));
    }
    if budget.llm_calls >= settings.max_llm_calls {
        return Ok(Some(format!(
            "LLM call budget exhausted: {}/{}",
            budget.llm_calls, settings.max_llm_calls
        )));
    }
    Ok(None)
}

async fn complete_with_budget_limited_result(
    state: Arc<AppState>,
    task: &TaskRecord,
    grep: &GrepResults,
    reason: String,
) -> anyhow::Result<DispatchOutcome> {
    let workspace = state.config.storage.workspace_dir(&task.task_id);
    warn!(
        task_id = %task.task_id,
        reason = %reason,
        "completing task with budget-limited result"
    );
    let result = budget_limited_result(&task.question, grep, &reason);
    let output = persist_final_answer_decision_result(&workspace, result).await?;
    let completed =
        complete_successful_log_analysis(&state, task, TaskPhase::PlanAnalysis, output).await?;
    sync_session_status(&state, &completed).await;
    Ok(DispatchOutcome::Complete)
}

fn budget_limited_result(question: &str, grep: &GrepResults, reason: &str) -> AnalysisResult {
    let symptoms = grep
        .matches
        .iter()
        .take(3)
        .map(|item| format!("{}:{} {}", item.file, item.line, item.text))
        .collect::<Vec<_>>();
    let likely_root_causes = if grep.matches.is_empty() {
        Vec::new()
    } else {
        vec![RootCause {
            cause: "分析在预算或重复动作防护下提前终止，当前只能确认已有日志异常与问题相关"
                .to_string(),
            evidence_refs: vec!["grep_results.json#matches/0".to_string()],
        }]
    };
    AnalysisResult {
        schema_version: 1,
        summary: format!("分析已受控终止：{reason}。用户问题：{}", question.trim()),
        symptoms,
        likely_root_causes,
        next_checks: vec![
            "提高 analysis 预算后重新分析，或补充更精确的问题和时间窗口".to_string(),
            "检查 analysis_events.jsonl 中已执行动作和被阻止的重复 fingerprint".to_string(),
        ],
        fix_suggestions: vec!["当前结果置信度较低，建议先补充证据后再实施修复".to_string()],
        missing_information: vec![reason.to_string()],
        confidence: Confidence::Low,
    }
}

fn ask_user_action_from_prompt(
    task: &TaskRecord,
    prompt: crate::services::agent_backend::ClaudeUserPrompt,
) -> AgentAction {
    let input = serde_json::json!({
        "questionId": prompt.question_id,
        "question": prompt.question,
        "answerFormat": prompt.answer_format,
        "required": prompt.required,
    });
    let action_id = input
        .get("questionId")
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
        .unwrap_or_else(|| format!("act_ask_user_{}", stable_json_hash(&input)));
    let fingerprint = format!(
        "task:{}:ask_user:{}",
        task.task_id,
        stable_json_hash(&serde_json::json!({"question": input.get("question")}))
    );
    AgentAction {
        schema_version: 1,
        action_id,
        kind: ActionKind::AskUser,
        reason: prompt
            .reason
            .unwrap_or_else(|| "Claude Code requested additional user input".to_string()),
        evidence_refs: Vec::new(),
        input,
        risk: ActionRisk::SafeReadOnly,
        fingerprint,
    }
}

fn approval_action_from_request(
    task: &TaskRecord,
    approval: crate::services::agent_backend::ClaudeApprovalRequest,
) -> AgentAction {
    let kind = match approval.action_type.as_str() {
        "collect_environment" => ActionKind::CollectEnvironment,
        _ => ActionKind::CollectEnvironment,
    };
    let input = if approval.input.is_null() {
        serde_json::json!({ "scope": "default" })
    } else {
        approval.input
    };
    let action_id = approval
        .action_id
        .unwrap_or_else(|| format!("act_approval_{}", stable_json_hash(&input)));
    let fingerprint = format!(
        "task:{}:approval:{}",
        task.task_id,
        stable_json_hash(&serde_json::json!({"actionType": kind, "input": input.clone()}))
    );
    AgentAction {
        schema_version: 1,
        action_id,
        kind,
        reason: approval.reason,
        evidence_refs: approval
            .evidence_refs
            .into_iter()
            .map(parse_evidence_ref)
            .collect(),
        input,
        risk: ActionRisk::RequiresApproval,
        fingerprint,
    }
}

fn record_claude_outcome(
    workspace: &std::path::Path,
    outcome: &ClaudeSessionOutcome,
) -> anyhow::Result<()> {
    let (action_id, message, evidence_refs, details) = match outcome {
        ClaudeSessionOutcome::FinalAnswer { result } => (
            None,
            "Claude Code session returned final answer".to_string(),
            result
                .likely_root_causes
                .iter()
                .flat_map(|cause| cause.evidence_refs.iter().cloned())
                .collect(),
            serde_json::json!({ "finalAnswer": result }),
        ),
        ClaudeSessionOutcome::WaitingForUser { prompt } => (
            prompt.question_id.clone(),
            "Claude Code session requested user input".to_string(),
            Vec::new(),
            serde_json::json!({ "pendingPrompt": prompt }),
        ),
        ClaudeSessionOutcome::WaitingForApproval { approval } => (
            approval.action_id.clone(),
            "Claude Code session requested approval".to_string(),
            approval.evidence_refs.clone(),
            serde_json::json!({ "pendingApproval": approval }),
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
    let actions = state.tool_runner.rule_based_actions(&manifest, &grep);
    info!(
        task_id = %task.task_id,
        action_count = actions.len(),
        "rule-based tool actions selected"
    );
    for action in actions {
        info!(
            task_id = %task.task_id,
            action_id = %action.action_id,
            "executing rule-based tool action"
        );
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
    info!(
        task_id = %task_id,
        from_phase = ?current,
        to_phase = ?next,
        "task phase completed"
    );
    sync_session_status(state, &task).await;
    Ok(DispatchOutcome::Continue(task))
}

async fn sync_session_status(state: &AppState, task: &TaskRecord) {
    if let Err(err) = state.sessions.sync_task_status(task).await {
        warn!(
            task_id = %task.task_id,
            session_id = ?task.session_id,
            "failed to sync session status: {err}"
        );
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        fs,
        os::unix::fs::PermissionsExt,
        path::PathBuf,
        sync::Arc,
        time::{SystemTime, UNIX_EPOCH},
    };

    use chrono::Utc;

    use super::*;
    use crate::{
        domain::models::{TaskInput, TaskSource, TaskStatus},
        pipeline::{extract_task, prepare_pipeline_run, search_task},
        support::config::{
            AnalysisMode, AnalysisSettings, AppConfig, AuthSettings, ClaudeCodeSettings,
            EmbeddingSettings, LlmProvider, LlmSettings, LogAnalyzerSettings, McpSettings,
            PermissionProfileSettings, ServerSettings, StorageSettings, ToolMatchSettings,
            ToolSettings, ToolsSettings,
        },
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
        assert_eq!(snapshot.state.budget.rounds, 1);
        assert_eq!(snapshot.state.budget.llm_calls, 1);
        assert!(fixture.workspace.join("agent_response.json").exists());
    }

    #[tokio::test]
    async fn plan_analysis_completes_claude_code_session() {
        let fixture = Fixture::new_with_log(TaskPhase::Extract, "INFO start\nWARN slow\n");
        let adapter = fixture.write_tool(
            "claude",
            r#"#!/usr/bin/env bash
cat <<'JSON'
{"structured_output":{"runtimeStatus":"completed","finalAnswer":{"summary":"session summary","symptoms":["slow warning"],"likelyRootCauses":[],"nextChecks":["inspect warning"],"fixSuggestions":[],"missingInformation":["no error evidence"],"confidence":"low"}},"session_id":"sess-claude-direct"}
JSON
"#,
        );
        let state = fixture.state_with_adapter(adapter);
        let mut task = fixture.task(TaskPhase::Extract);
        task.status = TaskStatus::Queued;
        task.phase = None;
        task.attempts = 0;
        state.tasks.create(task.clone()).await.unwrap();

        execute(state.clone(), &task.task_id).await.unwrap();

        let completed = state.tasks.get(&task.task_id).await.unwrap();
        assert_eq!(completed.status, TaskStatus::Succeeded);
        let snapshot = analysis_state::read_snapshot(&fixture.workspace).unwrap();
        assert_eq!(snapshot.state.budget.rounds, 1);
        assert_eq!(
            snapshot.state.claude_session_id.as_deref(),
            Some("sess-claude-direct")
        );
        assert!(fixture.workspace.join("claude_mcp_config.json").exists());
        assert!(fixture.workspace.join("claude_session.json").exists());
        let result: AnalysisResult = serde_json::from_str(
            &fs::read_to_string(fixture.workspace.join("result.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(result.summary, "session summary");
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
            let adapter = self.write_tool(
                "claude",
                r#"#!/usr/bin/env bash
cat <<'JSON'
{"structured_output":{"runtimeStatus":"completed","finalAnswer":{"summary":"mock summary","symptoms":["failure"],"likelyRootCauses":[{"cause":"log contains an error","evidenceRefs":["grep_results.json#matches/0"]}],"nextChecks":["check logs"],"fixSuggestions":["fix error"],"missingInformation":[],"confidence":"high"}},"usage":{"inputTokens":12,"outputTokens":8},"cost":{"usd":0.002},"session_id":"sess-claude-test"}
JSON
"#,
            );
            self.state_with_tools_and_adapter(tools, adapter)
        }

        fn state_with_adapter(&self, adapter: PathBuf) -> Arc<AppState> {
            self.state_with_tools_and_adapter(ToolsSettings::default(), adapter)
        }

        fn state_with_tools_and_adapter(
            &self,
            tools: ToolsSettings,
            adapter: PathBuf,
        ) -> Arc<AppState> {
            let config = Arc::new(AppConfig {
                config_path: self.root.join("logagent-test.yaml"),
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
                skills: crate::support::config::SkillSettings {
                    enabled: false,
                    roots: Vec::new(),
                    max_skill_chars: 4000,
                    max_reference_chars: 20_000,
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
                    binary_path: None,
                    binary_max_output_bytes: 1024 * 1024,
                    model: "stub".to_string(),
                    request_timeout_seconds: 1,
                    max_input_chars: 60_000,
                    max_output_tokens: 100,
                },
                claude_code: test_claude_code_settings(adapter),
                mcp: McpSettings::default(),
                analysis: AnalysisSettings {
                    max_rounds: 4,
                    max_llm_calls: 4,
                    max_actions: 6,
                    max_repeated_action_fingerprints: 1,
                },
                embedding: EmbeddingSettings {
                    enabled: false,
                    provider: "openai_compatible".to_string(),
                    model: "text-embedding-3-small".to_string(),
                    api_key_env: None,
                    store: "sqlite".to_string(),
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
                alias: None,
                session_id: Some("sess_test".to_string()),
                task_kind: TaskKind::LogAnalysis,
                analysis_mode: AnalysisMode::Diagnose,
                source: TaskSource::Upload,
                upload_ids: vec!["upl_1".to_string()],
                inputs: vec![TaskInput {
                    upload_id: "upl_1".to_string(),
                    filename: "sample.log".to_string(),
                    size: 24,
                    raw_path: "raw/upl_1/sample.log".to_string(),
                }],
                source_url: None,
                tool_id: None,
                tool_params: serde_json::Value::Null,
                tool_result_path: None,
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
                system_context_path: None,
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

    fn test_claude_code_settings(command_path: PathBuf) -> ClaudeCodeSettings {
        ClaudeCodeSettings {
            command_path,
            default_mode: AnalysisMode::Diagnose,
            max_session_seconds: 5,
            max_output_bytes: 1024 * 1024,
            permission_profiles: BTreeMap::from([(
                AnalysisMode::Diagnose,
                PermissionProfileSettings {
                    name: "diagnose".to_string(),
                    permission_mode: "dontAsk".to_string(),
                    tools: String::new(),
                    allowed_tools: vec![
                        crate::support::config::LOGAGENT_MCP_ALLOWED_TOOL_GLOB.to_string()
                    ],
                    disallowed_tools: vec!["Bash".to_string(), "Edit".to_string()],
                    native_bash: false,
                    native_edit: false,
                    worktree_required: false,
                },
            )]),
        }
    }
}
