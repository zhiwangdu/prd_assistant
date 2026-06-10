use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::{
    contracts::EvidenceArtifact,
    models::{AnalysisResult, GrepResults, TaskPhase, TaskRecord},
};

const STATE_FILE: &str = "analysis_state.json";
const EVENTS_FILE: &str = "analysis_events.jsonl";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisState {
    pub schema_version: u32,
    pub task_id: String,
    pub revision: u64,
    pub question: String,
    pub status: AnalysisStatus,
    pub current_phase: Option<TaskPhase>,
    pub evidence: Vec<AnalysisEvidenceRecord>,
    pub actions: Vec<AnalysisActionRecord>,
    #[serde(default)]
    pub user_messages: Vec<UserMessageRecord>,
    #[serde(default)]
    pub pending_user_prompts: Vec<PendingUserPrompt>,
    #[serde(default)]
    pub pending_approvals: Vec<PendingApproval>,
    pub budget: AnalysisBudgetSnapshot,
    pub final_result_path: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AnalysisStatus {
    Running,
    WaitingForUser,
    WaitingForApproval,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisEvidenceRecord {
    pub evidence_type: AnalysisEvidenceType,
    pub artifact_path: String,
    pub action_id: Option<String>,
    pub summary: String,
    pub evidence_refs: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisEvidenceType {
    Manifest,
    LogSearch,
    ToolOutput,
    EnvironmentEvidence,
    FinalResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisActionRecord {
    pub action_id: String,
    pub action_type: String,
    pub fingerprint: String,
    pub status: AnalysisActionStatus,
    pub summary: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AnalysisActionStatus {
    WaitingForUser,
    WaitingForApproval,
    Succeeded,
    Failed,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMessageRecord {
    pub message_id: String,
    pub question_id: Option<String>,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingUserPrompt {
    pub question_id: String,
    pub action_id: String,
    pub question: String,
    pub reason: String,
    pub required: bool,
    pub answer_format: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingApproval {
    pub action_id: String,
    pub action_type: String,
    pub reason: String,
    pub risk: String,
    pub input: serde_json::Value,
    pub evidence_refs: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisBudgetSnapshot {
    pub rounds: u32,
    pub llm_calls: u32,
    pub actions: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisEvent {
    pub schema_version: u32,
    pub revision: u64,
    pub task_id: String,
    pub event_type: AnalysisEventType,
    pub phase: Option<TaskPhase>,
    pub action_id: Option<String>,
    pub message: String,
    pub evidence_refs: Vec<String>,
    pub artifact_path: Option<String>,
    pub details: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisEventType {
    AnalysisStarted,
    ModelDecision,
    LlmCallStarted,
    LlmCallCompleted,
    LlmCallSchemaRetry,
    EvidenceRecorded,
    UserPromptRequested,
    UserMessageReceived,
    ApprovalRequested,
    ApprovalDecisionRecorded,
    ActionCompleted,
    FinalResultGenerated,
    AnalysisFailed,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisSnapshotResponse {
    pub task_id: String,
    pub state_path: String,
    pub events_path: String,
    pub state: AnalysisState,
    pub events: Vec<AnalysisEvent>,
}

pub fn initialize(workspace: &Path, task: &TaskRecord) -> anyhow::Result<()> {
    fs::create_dir_all(workspace)?;
    let now = Utc::now();
    let state = AnalysisState {
        schema_version: 1,
        task_id: task.task_id.clone(),
        revision: 0,
        question: task.question.clone(),
        status: AnalysisStatus::Running,
        current_phase: task.phase,
        evidence: Vec::new(),
        actions: Vec::new(),
        user_messages: Vec::new(),
        pending_user_prompts: Vec::new(),
        pending_approvals: Vec::new(),
        budget: AnalysisBudgetSnapshot {
            rounds: 0,
            llm_calls: 0,
            actions: 0,
        },
        final_result_path: None,
        created_at: now,
        updated_at: now,
    };
    write_state(workspace, &state)?;
    write_events(workspace, &[])?;
    append_event(
        workspace,
        AnalysisEventType::AnalysisStarted,
        task.phase,
        None,
        "analysis state initialized".to_string(),
        Vec::new(),
        None,
        serde_json::json!({
            "attempt": task.attempts,
            "source": task.source,
        }),
        |state| {
            state.current_phase = task.phase;
            Ok(())
        },
    )
}

pub fn record_pending_user_prompt(
    workspace: &Path,
    action: &crate::domain::contracts::AgentAction,
    question_id: String,
    question: String,
    required: bool,
    answer_format: Option<String>,
) -> anyhow::Result<()> {
    let evidence_refs = action
        .evidence_refs
        .iter()
        .map(|reference| match &reference.selector {
            Some(selector) => format!("{}#{selector}", reference.artifact_path),
            None => reference.artifact_path.clone(),
        })
        .collect::<Vec<_>>();
    let prompt = PendingUserPrompt {
        question_id: question_id.clone(),
        action_id: action.action_id.clone(),
        question,
        reason: action.reason.clone(),
        required,
        answer_format,
        created_at: Utc::now(),
    };
    append_event(
        workspace,
        AnalysisEventType::UserPromptRequested,
        Some(TaskPhase::PlanAnalysis),
        Some(action.action_id.clone()),
        format!("waiting for user answer to {question_id}"),
        evidence_refs.clone(),
        None,
        serde_json::json!({
            "prompt": prompt,
            "action": action,
        }),
        |state| {
            state.status = AnalysisStatus::WaitingForUser;
            state.current_phase = Some(TaskPhase::PlanAnalysis);
            state.budget.actions = state.budget.actions.saturating_add(1);
            upsert_action(
                &mut state.actions,
                AnalysisActionRecord {
                    action_id: action.action_id.clone(),
                    action_type: "ask_user".to_string(),
                    fingerprint: action.fingerprint.clone(),
                    status: AnalysisActionStatus::WaitingForUser,
                    summary: format!("waiting for user answer to {question_id}"),
                    created_at: Utc::now(),
                },
            );
            state
                .pending_user_prompts
                .retain(|item| item.question_id != question_id);
            state.pending_user_prompts.push(prompt);
            Ok(())
        },
    )
}

pub fn record_user_message(
    workspace: &Path,
    message_id: String,
    question_id: Option<String>,
    content: String,
) -> anyhow::Result<()> {
    append_event(
        workspace,
        AnalysisEventType::UserMessageReceived,
        Some(TaskPhase::PlanAnalysis),
        question_id.clone(),
        match &question_id {
            Some(question_id) => format!("user answered {question_id}"),
            None => "user added message".to_string(),
        },
        Vec::new(),
        None,
        serde_json::json!({
            "messageId": message_id,
            "questionId": question_id,
            "content": content,
        }),
        |state| {
            state.status = AnalysisStatus::Running;
            state.current_phase = Some(TaskPhase::PlanAnalysis);
            if let Some(question_id) = &question_id {
                let action_id = state
                    .pending_user_prompts
                    .iter()
                    .find(|item| item.question_id == *question_id)
                    .map(|item| item.action_id.clone());
                state
                    .pending_user_prompts
                    .retain(|item| item.question_id != *question_id);
                if let Some(action_id) = action_id {
                    if let Some(action) = state
                        .actions
                        .iter_mut()
                        .find(|action| action.action_id == action_id)
                    {
                        action.status = AnalysisActionStatus::Succeeded;
                        action.summary = "user answered prompt".to_string();
                    }
                }
            } else {
                state.pending_user_prompts.clear();
            }
            if !state
                .user_messages
                .iter()
                .any(|message| message.message_id == message_id)
            {
                state.user_messages.push(UserMessageRecord {
                    message_id,
                    question_id,
                    content,
                    created_at: Utc::now(),
                });
            }
            Ok(())
        },
    )
}

pub fn record_pending_approval(
    workspace: &Path,
    action: &crate::domain::contracts::AgentAction,
) -> anyhow::Result<()> {
    let evidence_refs = action
        .evidence_refs
        .iter()
        .map(|reference| match &reference.selector {
            Some(selector) => format!("{}#{selector}", reference.artifact_path),
            None => reference.artifact_path.clone(),
        })
        .collect::<Vec<_>>();
    let approval = PendingApproval {
        action_id: action.action_id.clone(),
        action_type: format!("{:?}", action.kind),
        reason: action.reason.clone(),
        risk: format!("{:?}", action.risk),
        input: action.input.clone(),
        evidence_refs: evidence_refs.clone(),
        created_at: Utc::now(),
    };
    append_event(
        workspace,
        AnalysisEventType::ApprovalRequested,
        Some(TaskPhase::PlanAnalysis),
        Some(action.action_id.clone()),
        format!("approval required for {}", action.action_id),
        evidence_refs,
        None,
        serde_json::json!({
            "approval": approval,
            "action": action,
        }),
        |state| {
            state.status = AnalysisStatus::WaitingForApproval;
            state.current_phase = Some(TaskPhase::PlanAnalysis);
            state.budget.actions = state.budget.actions.saturating_add(1);
            upsert_action(
                &mut state.actions,
                AnalysisActionRecord {
                    action_id: action.action_id.clone(),
                    action_type: format!("{:?}", action.kind),
                    fingerprint: action.fingerprint.clone(),
                    status: AnalysisActionStatus::WaitingForApproval,
                    summary: format!("approval required: {}", action.reason),
                    created_at: Utc::now(),
                },
            );
            state
                .pending_approvals
                .retain(|item| item.action_id != action.action_id);
            state.pending_approvals.push(approval);
            Ok(())
        },
    )
}

pub fn record_approval_decision(
    workspace: &Path,
    action_id: &str,
    approved: bool,
    reason: Option<String>,
    idempotency_key: Option<String>,
) -> anyhow::Result<()> {
    append_event(
        workspace,
        AnalysisEventType::ApprovalDecisionRecorded,
        Some(TaskPhase::PlanAnalysis),
        Some(action_id.to_string()),
        if approved {
            format!("approved action {action_id}")
        } else {
            format!("rejected action {action_id}")
        },
        Vec::new(),
        None,
        serde_json::json!({
            "actionId": action_id,
            "approved": approved,
            "reason": reason,
            "idempotencyKey": idempotency_key,
        }),
        |state| {
            state.status = AnalysisStatus::Running;
            state.current_phase = Some(TaskPhase::PlanAnalysis);
            state
                .pending_approvals
                .retain(|item| item.action_id != action_id);
            if let Some(action) = state
                .actions
                .iter_mut()
                .find(|action| action.action_id == action_id)
            {
                action.status = if approved {
                    AnalysisActionStatus::Succeeded
                } else {
                    AnalysisActionStatus::Rejected
                };
                action.summary = reason.unwrap_or_else(|| {
                    if approved {
                        "action approved".to_string()
                    } else {
                        "action rejected".to_string()
                    }
                });
            }
            Ok(())
        },
    )
}

pub fn record_environment_artifact(
    workspace: &Path,
    action_id: &str,
    artifact_path: String,
    summary: String,
) -> anyhow::Result<()> {
    let evidence = AnalysisEvidenceRecord {
        evidence_type: AnalysisEvidenceType::EnvironmentEvidence,
        artifact_path: artifact_path.clone(),
        action_id: Some(action_id.to_string()),
        summary,
        evidence_refs: vec![artifact_path],
        created_at: Utc::now(),
    };
    append_evidence_event(
        workspace,
        "",
        TaskPhase::PlanAnalysis,
        format!("environment evidence recorded for {action_id}"),
        evidence,
        serde_json::json!({}),
    )
}

pub fn ensure_initialized(workspace: &Path, task: &TaskRecord) -> anyhow::Result<()> {
    if workspace.join(STATE_FILE).exists() {
        return Ok(());
    }
    initialize(workspace, task)
}

pub fn record_manifest(workspace: &Path, task_id: &str) -> anyhow::Result<()> {
    let evidence = AnalysisEvidenceRecord {
        evidence_type: AnalysisEvidenceType::Manifest,
        artifact_path: "manifest.json".to_string(),
        action_id: None,
        summary: "manifest generated from raw snapshot".to_string(),
        evidence_refs: vec!["manifest.json".to_string()],
        created_at: Utc::now(),
    };
    append_evidence_event(
        workspace,
        task_id,
        TaskPhase::Extract,
        "manifest evidence recorded".to_string(),
        evidence,
        serde_json::json!({}),
    )
}

pub fn record_log_search(workspace: &Path, grep: &GrepResults) -> anyhow::Result<()> {
    let evidence_refs = (0..grep.matches.len())
        .map(|index| format!("grep_results.json#matches/{index}"))
        .collect::<Vec<_>>();
    let evidence = AnalysisEvidenceRecord {
        evidence_type: AnalysisEvidenceType::LogSearch,
        artifact_path: "grep_results.json".to_string(),
        action_id: None,
        summary: format!("grep search recorded {} matches", grep.matches.len()),
        evidence_refs,
        created_at: Utc::now(),
    };
    append_evidence_event(
        workspace,
        "",
        TaskPhase::SearchLogs,
        format!("log search recorded {} matches", grep.matches.len()),
        evidence,
        serde_json::json!({
            "keywords": grep.keywords,
            "totalMatches": grep.total_matches,
        }),
    )
}

pub fn record_log_search_action(
    workspace: &Path,
    action: &crate::domain::contracts::AgentAction,
    grep: &GrepResults,
) -> anyhow::Result<()> {
    let evidence_refs = (0..grep.matches.len())
        .map(|index| format!("grep_results.json#matches/{index}"))
        .collect::<Vec<_>>();
    let evidence = AnalysisEvidenceRecord {
        evidence_type: AnalysisEvidenceType::LogSearch,
        artifact_path: "grep_results.json".to_string(),
        action_id: Some(action.action_id.clone()),
        summary: format!("search_logs action recorded {} matches", grep.matches.len()),
        evidence_refs,
        created_at: Utc::now(),
    };
    append_action_event(
        workspace,
        TaskPhase::PlanAnalysis,
        action.action_id.clone(),
        action.fingerprint.clone(),
        "search_logs".to_string(),
        AnalysisActionStatus::Succeeded,
        format!("search_logs action {} completed", action.action_id),
        evidence,
        serde_json::json!({
            "searchAction": action,
            "keywords": grep.keywords,
            "totalMatches": grep.total_matches,
        }),
    )
}

pub fn record_tool_artifact(
    workspace: &Path,
    action: &crate::domain::contracts::AgentAction,
    artifact: &EvidenceArtifact,
) -> anyhow::Result<()> {
    let evidence = AnalysisEvidenceRecord {
        evidence_type: AnalysisEvidenceType::ToolOutput,
        artifact_path: artifact.artifact_path.clone(),
        action_id: Some(action.action_id.clone()),
        summary: artifact.summary.details.join("; "),
        evidence_refs: vec![artifact.artifact_path.clone()],
        created_at: Utc::now(),
    };
    append_action_event(
        workspace,
        TaskPhase::RunTool,
        action.action_id.clone(),
        action.fingerprint.clone(),
        "run_tool".to_string(),
        AnalysisActionStatus::Succeeded,
        format!("tool action {} completed", action.action_id),
        evidence,
        serde_json::json!({
            "toolAction": action,
            "artifact": artifact,
        }),
    )
}

pub fn record_final_result(
    workspace: &Path,
    result_path: &Path,
    result: &AnalysisResult,
) -> anyhow::Result<()> {
    record_final_result_inner(workspace, result_path, result, true)
}

pub fn record_final_answer_decision_result(
    workspace: &Path,
    result_path: &Path,
    result: &AnalysisResult,
) -> anyhow::Result<()> {
    record_final_result_inner(workspace, result_path, result, false)
}

fn record_final_result_inner(
    workspace: &Path,
    result_path: &Path,
    result: &AnalysisResult,
    increment_llm_calls: bool,
) -> anyhow::Result<()> {
    let artifact_path = relative_to_workspace(workspace, result_path)?;
    let evidence_refs = result
        .likely_root_causes
        .iter()
        .flat_map(|cause| cause.evidence_refs.iter().cloned())
        .collect::<Vec<_>>();
    let evidence = AnalysisEvidenceRecord {
        evidence_type: AnalysisEvidenceType::FinalResult,
        artifact_path: artifact_path.clone(),
        action_id: None,
        summary: result.summary.clone(),
        evidence_refs: evidence_refs.clone(),
        created_at: Utc::now(),
    };
    append_evidence_event(
        workspace,
        "",
        TaskPhase::GenerateResult,
        "final result generated".to_string(),
        evidence,
        serde_json::json!({
            "confidence": result.confidence,
        }),
    )?;
    update_state(workspace, |state| {
        state.status = AnalysisStatus::Succeeded;
        state.current_phase = None;
        state.final_result_path = Some(artifact_path.clone());
        if increment_llm_calls {
            state.budget.llm_calls = state.budget.llm_calls.saturating_add(1);
        }
        Ok(())
    })?;
    let state = read_state(workspace)?;
    append_raw_event(
        workspace,
        &AnalysisEvent {
            schema_version: 1,
            revision: state.revision,
            task_id: state.task_id,
            event_type: AnalysisEventType::FinalResultGenerated,
            phase: Some(TaskPhase::GenerateResult),
            action_id: None,
            message: "final result persisted".to_string(),
            evidence_refs,
            artifact_path: Some(artifact_path),
            details: serde_json::json!({}),
            created_at: Utc::now(),
        },
    )
}

pub fn record_failure(
    workspace: &Path,
    phase: Option<TaskPhase>,
    message: String,
) -> anyhow::Result<()> {
    append_event(
        workspace,
        AnalysisEventType::AnalysisFailed,
        phase,
        None,
        message.clone(),
        Vec::new(),
        None,
        serde_json::json!({ "error": message }),
        |state| {
            state.status = AnalysisStatus::Failed;
            state.current_phase = phase;
            Ok(())
        },
    )
}

pub fn record_model_decision(
    workspace: &Path,
    phase: TaskPhase,
    action_id: Option<String>,
    message: String,
    evidence_refs: Vec<String>,
    details: serde_json::Value,
) -> anyhow::Result<()> {
    append_event(
        workspace,
        AnalysisEventType::ModelDecision,
        Some(phase),
        action_id,
        message,
        evidence_refs,
        None,
        details,
        |state| {
            state.current_phase = Some(phase);
            state.budget.rounds = state.budget.rounds.saturating_add(1);
            Ok(())
        },
    )
}

pub fn record_llm_call_started(
    workspace: &Path,
    phase: TaskPhase,
    call_id: String,
    call_kind: String,
    attempt: usize,
    model: String,
) -> anyhow::Result<()> {
    append_event(
        workspace,
        AnalysisEventType::LlmCallStarted,
        Some(phase),
        Some(call_id.clone()),
        format!("LLM {call_kind} call {call_id} attempt {attempt} started"),
        Vec::new(),
        None,
        serde_json::json!({
            "callId": call_id,
            "callKind": call_kind,
            "attempt": attempt,
            "model": model,
        }),
        |state| {
            state.current_phase = Some(phase);
            state.budget.llm_calls = state.budget.llm_calls.saturating_add(1);
            Ok(())
        },
    )
}

pub fn record_llm_call_completed(
    workspace: &Path,
    phase: TaskPhase,
    call_id: String,
    call_kind: String,
    attempt: usize,
    model: String,
) -> anyhow::Result<()> {
    append_event(
        workspace,
        AnalysisEventType::LlmCallCompleted,
        Some(phase),
        Some(call_id.clone()),
        format!("LLM {call_kind} call {call_id} attempt {attempt} completed"),
        Vec::new(),
        None,
        serde_json::json!({
            "callId": call_id,
            "callKind": call_kind,
            "attempt": attempt,
            "model": model,
        }),
        |state| {
            state.current_phase = Some(phase);
            Ok(())
        },
    )
}

pub fn record_llm_call_schema_retry(
    workspace: &Path,
    phase: TaskPhase,
    call_id: String,
    call_kind: String,
    attempt: usize,
    model: String,
    error: String,
) -> anyhow::Result<()> {
    append_event(
        workspace,
        AnalysisEventType::LlmCallSchemaRetry,
        Some(phase),
        Some(call_id.clone()),
        format!("LLM {call_kind} call {call_id} attempt {attempt} needs schema retry"),
        Vec::new(),
        None,
        serde_json::json!({
            "callId": call_id,
            "callKind": call_kind,
            "attempt": attempt,
            "model": model,
            "error": error,
        }),
        |state| {
            state.current_phase = Some(phase);
            Ok(())
        },
    )
}

pub fn read_snapshot(workspace: &Path) -> anyhow::Result<AnalysisSnapshotResponse> {
    let state = read_state(workspace)?;
    let events = read_events(workspace)?;
    Ok(AnalysisSnapshotResponse {
        task_id: state.task_id.clone(),
        state_path: workspace.join(STATE_FILE).display().to_string(),
        events_path: workspace.join(EVENTS_FILE).display().to_string(),
        state,
        events,
    })
}

fn append_evidence_event(
    workspace: &Path,
    task_id_hint: &str,
    phase: TaskPhase,
    message: String,
    evidence: AnalysisEvidenceRecord,
    details: serde_json::Value,
) -> anyhow::Result<()> {
    let evidence_refs = evidence.evidence_refs.clone();
    let artifact_path = Some(evidence.artifact_path.clone());
    append_event(
        workspace,
        AnalysisEventType::EvidenceRecorded,
        Some(phase),
        evidence.action_id.clone(),
        message,
        evidence_refs,
        artifact_path,
        details,
        |state| {
            if state.task_id.is_empty() && !task_id_hint.is_empty() {
                state.task_id = task_id_hint.to_string();
            }
            state.current_phase = Some(phase);
            upsert_evidence(&mut state.evidence, evidence);
            Ok(())
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn append_action_event(
    workspace: &Path,
    phase: TaskPhase,
    action_id: String,
    fingerprint: String,
    action_type: String,
    status: AnalysisActionStatus,
    summary: String,
    evidence: AnalysisEvidenceRecord,
    details: serde_json::Value,
) -> anyhow::Result<()> {
    let evidence_refs = evidence.evidence_refs.clone();
    let artifact_path = Some(evidence.artifact_path.clone());
    append_event(
        workspace,
        AnalysisEventType::ActionCompleted,
        Some(phase),
        Some(action_id.clone()),
        summary.clone(),
        evidence_refs,
        artifact_path,
        details,
        |state| {
            state.current_phase = Some(phase);
            state.budget.actions = state.budget.actions.saturating_add(1);
            upsert_action(
                &mut state.actions,
                AnalysisActionRecord {
                    action_id: action_id.clone(),
                    action_type,
                    fingerprint,
                    status,
                    summary,
                    created_at: Utc::now(),
                },
            );
            upsert_evidence(&mut state.evidence, evidence);
            Ok(())
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn append_event(
    workspace: &Path,
    event_type: AnalysisEventType,
    phase: Option<TaskPhase>,
    action_id: Option<String>,
    message: String,
    evidence_refs: Vec<String>,
    artifact_path: Option<String>,
    details: serde_json::Value,
    update: impl FnOnce(&mut AnalysisState) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    update_state(workspace, update)?;
    let state = read_state(workspace)?;
    append_raw_event(
        workspace,
        &AnalysisEvent {
            schema_version: 1,
            revision: state.revision,
            task_id: state.task_id,
            event_type,
            phase,
            action_id,
            message,
            evidence_refs,
            artifact_path,
            details,
            created_at: Utc::now(),
        },
    )
}

fn update_state(
    workspace: &Path,
    update: impl FnOnce(&mut AnalysisState) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    let mut state = read_state(workspace)?;
    update(&mut state)?;
    state.revision = state.revision.saturating_add(1);
    state.updated_at = Utc::now();
    write_state(workspace, &state)
}

fn upsert_evidence(records: &mut Vec<AnalysisEvidenceRecord>, next: AnalysisEvidenceRecord) {
    if let Some(existing) = records.iter_mut().find(|record| {
        record.artifact_path == next.artifact_path && record.action_id == next.action_id
    }) {
        *existing = next;
    } else {
        records.push(next);
    }
}

fn upsert_action(records: &mut Vec<AnalysisActionRecord>, next: AnalysisActionRecord) {
    if let Some(existing) = records
        .iter_mut()
        .find(|record| record.action_id == next.action_id)
    {
        *existing = next;
    } else {
        records.push(next);
    }
}

fn read_state(workspace: &Path) -> anyhow::Result<AnalysisState> {
    let path = workspace.join(STATE_FILE);
    let raw = fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&raw)?)
}

fn write_state(workspace: &Path, state: &AnalysisState) -> anyhow::Result<()> {
    let path = workspace.join(STATE_FILE);
    let temp = workspace.join(".analysis_state.json.tmp");
    fs::write(&temp, serde_json::to_vec_pretty(state)?)?;
    fs::rename(&temp, path)?;
    Ok(())
}

fn write_events(workspace: &Path, events: &[AnalysisEvent]) -> anyhow::Result<()> {
    let path = workspace.join(EVENTS_FILE);
    let temp = workspace.join(".analysis_events.jsonl.tmp");
    let mut file = fs::File::create(&temp)?;
    for event in events {
        serde_json::to_writer(&mut file, event)?;
        file.write_all(b"\n")?;
    }
    file.sync_all()?;
    fs::rename(&temp, path)?;
    Ok(())
}

fn append_raw_event(workspace: &Path, event: &AnalysisEvent) -> anyhow::Result<()> {
    let path = workspace.join(EVENTS_FILE);
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    serde_json::to_writer(&mut file, event)?;
    file.write_all(b"\n")?;
    file.flush()?;
    Ok(())
}

fn read_events(workspace: &Path) -> anyhow::Result<Vec<AnalysisEvent>> {
    let path = workspace.join(EVENTS_FILE);
    let raw = fs::read_to_string(path)?;
    raw.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| Ok(serde_json::from_str(line)?))
        .collect()
}

fn relative_to_workspace(workspace: &Path, path: &Path) -> anyhow::Result<String> {
    let relative = path.strip_prefix(workspace)?;
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{TaskInput, TaskSource, TaskStatus};
    use std::path::PathBuf;

    #[test]
    fn persists_state_and_events() {
        let workspace = temp_workspace("analysis-state");
        let task = task_record("task_analysis");
        initialize(&workspace, &task).unwrap();
        record_log_search(
            &workspace,
            &GrepResults {
                keywords: vec!["error".to_string()],
                total_matches: 1,
                matches: vec![crate::domain::models::GrepMatch {
                    file: "sample.log".to_string(),
                    line: 1,
                    keyword: "error".to_string(),
                    text: "ERROR failed".to_string(),
                }],
            },
        )
        .unwrap();

        let snapshot = read_snapshot(&workspace).unwrap();

        assert_eq!(snapshot.state.task_id, "task_analysis");
        assert_eq!(snapshot.state.revision, 2);
        assert_eq!(snapshot.state.evidence.len(), 1);
        assert_eq!(snapshot.events.len(), 2);
        assert_eq!(
            snapshot.state.evidence[0].evidence_refs,
            vec!["grep_results.json#matches/0"]
        );
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn records_llm_call_lifecycle_events() {
        let workspace = temp_workspace("analysis-llm-call");
        let task = task_record("task_analysis_llm");
        initialize(&workspace, &task).unwrap();
        record_llm_call_started(
            &workspace,
            TaskPhase::PlanAnalysis,
            "llmcall_1".to_string(),
            "action_decision".to_string(),
            1,
            "test-model".to_string(),
        )
        .unwrap();
        record_llm_call_schema_retry(
            &workspace,
            TaskPhase::PlanAnalysis,
            "llmcall_1".to_string(),
            "action_decision".to_string(),
            1,
            "test-model".to_string(),
            "missing field `type`".to_string(),
        )
        .unwrap();
        record_llm_call_completed(
            &workspace,
            TaskPhase::PlanAnalysis,
            "llmcall_1".to_string(),
            "action_decision".to_string(),
            2,
            "test-model".to_string(),
        )
        .unwrap();

        let snapshot = read_snapshot(&workspace).unwrap();

        assert_eq!(snapshot.state.budget.llm_calls, 1);
        assert!(snapshot
            .events
            .iter()
            .any(|event| event.event_type == AnalysisEventType::LlmCallStarted));
        let retry = snapshot
            .events
            .iter()
            .find(|event| event.event_type == AnalysisEventType::LlmCallSchemaRetry)
            .unwrap();
        assert_eq!(retry.action_id.as_deref(), Some("llmcall_1"));
        assert_eq!(retry.details["error"], "missing field `type`");
        assert!(snapshot
            .events
            .iter()
            .any(|event| event.event_type == AnalysisEventType::LlmCallCompleted));
        let _ = fs::remove_dir_all(workspace);
    }

    fn temp_workspace(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "logagent-{name}-{}",
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn task_record(task_id: &str) -> TaskRecord {
        let now = Utc::now();
        TaskRecord {
            schema_version: 4,
            task_id: task_id.to_string(),
            alias: None,
            session_id: Some("sess_test".to_string()),
            task_kind: crate::domain::models::TaskKind::LogAnalysis,
            source: TaskSource::Upload,
            upload_ids: vec!["upl_1".to_string()],
            inputs: vec![TaskInput {
                upload_id: "upl_1".to_string(),
                filename: "sample.log".to_string(),
                size: 1,
                raw_path: "raw/upl_1/sample.log".to_string(),
            }],
            source_url: None,
            tool_id: None,
            tool_params: serde_json::Value::Null,
            tool_result_path: None,
            instance_id: None,
            cluster_id: None,
            node_id: None,
            question: "why".to_string(),
            status: TaskStatus::Running,
            phase: Some(TaskPhase::Extract),
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
