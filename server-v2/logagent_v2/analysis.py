from __future__ import annotations

from .config import Settings
from .mcp import (
    build_task_artifact_index,
    read_initial_grep_artifact,
    read_latest_evidence_artifact,
    read_task_case_context,
    read_task_tool_results,
)
from .mcp_audit import read_mcp_calls
from .results import get_run_result
from .store import JsonObject, Store


ANALYSIS_RESOURCE_KINDS = (
    "artifact_index",
    "analysis_state",
    "analysis_package",
    "agent_request",
    "agent_response",
    "claude_mcp_config",
    "claude_session",
    "case_context",
    "tool_results",
    "mcp_calls",
    "system_context",
    "metadata_context",
    "environment_evidence",
)


RUN_ARTIFACT_KINDS = {
    "manifest": ("manifestPath", "manifest", "manifest.json"),
    "metadata_context": (
        "metadataContextPath",
        "metadataContext",
        "metadata_context.json",
    ),
    "system_context": ("systemContextPath", "systemContext", "system_context.json"),
    "analysis_package": (
        "analysisPackagePath",
        "analysisPackage",
        "analysis_package.json",
    ),
    "agent_response": ("agentResponsePath", "agentResponse", "agent_response.json"),
    "claude_mcp_config": (
        "claudeMcpConfigPath",
        "claudeMcpConfig",
        "claude_mcp_config.json",
    ),
    "claude_session": ("claudeSessionPath", "claudeSession", "claude_session.json"),
    "analysis_state": ("analysisStatePath", "analysisState", "analysis_state.json"),
    "user_question": ("textInputPath", "textInput", "session_text_input.json"),
}


def get_run_analysis(settings: Settings, store: Store, run_id: str) -> JsonObject:
    run = store.get_run(run_id)
    actions = store.list_actions(run_id)
    value: JsonObject = {
        "run": run,
        "workspace": store.get_workspace(run["workspace_id"]),
        "timeline": store.list_timeline(run_id),
        "evidence": store.list_evidence(run_id),
        "actions": actions,
        "pendingActions": [
            action for action in actions if action.get("status") == "pending"
        ],
        "artifacts": store.list_run_artifacts(run_id),
        "resources": {},
        "result": None,
    }
    resources = value["resources"]
    for kind in ANALYSIS_RESOURCE_KINDS:
        resources[kind] = optional_latest_artifact(settings, store, run_id, kind)
    if run.get("finalAnswer"):
        try:
            value["result"] = get_run_result(settings, store, run_id)
        except ValueError:
            value["result"] = None
    return value


def get_run_artifacts(settings: Settings, store: Store, run_id: str) -> JsonObject:
    value = store.list_run_artifacts(run_id)
    run = value["run"]
    value["taskId"] = run_id
    value["artifactIndex"] = build_task_artifact_index(store, run)

    for kind, (path_field, value_field, logical_path) in RUN_ARTIFACT_KINDS.items():
        artifact = optional_latest_artifact(settings, store, run_id, kind)
        value[path_field] = logical_path if artifact is not None else None
        value[value_field] = artifact

    try:
        grep_results = read_initial_grep_artifact(settings, store, run_id)
    except ValueError:
        grep_results = None
    value["grepResultsPath"] = "grep_results.json" if grep_results is not None else None
    value["grepResults"] = grep_results

    case_context = read_task_case_context(settings, store, run)
    value["caseContextPath"] = "case_context.json"
    value["caseContext"] = case_context

    mcp_calls = read_mcp_calls(settings, store, run_id)
    value["mcpCallsPath"] = (
        "mcp_calls.jsonl" if int(mcp_calls.get("callCount", 0) or 0) > 0 else None
    )
    value["mcpCalls"] = mcp_calls.get("calls", [])

    tool_results = read_task_tool_results(settings, store, run)
    value["toolResults"] = tool_results.get("toolResults", [])
    value["toolResultCount"] = tool_results.get("toolResultCount", 0)
    return value


def optional_latest_artifact(
    settings: Settings,
    store: Store,
    run_id: str,
    kind: str,
) -> JsonObject | None:
    try:
        run = store.get_run(run_id)
        if kind == "artifact_index":
            return build_task_artifact_index(store, run)
        if kind == "case_context":
            return read_task_case_context(settings, store, run)
        if kind == "tool_results":
            return read_task_tool_results(settings, store, run)
        if kind == "mcp_calls":
            return read_mcp_calls(settings, store, run_id)
        return read_latest_evidence_artifact(settings, store, run_id, kind)
    except ValueError:
        return None
