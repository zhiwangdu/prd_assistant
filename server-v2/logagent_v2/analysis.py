from __future__ import annotations

from .config import Settings
from .mcp import read_latest_evidence_artifact
from .results import get_run_result
from .store import JsonObject, Store


ANALYSIS_RESOURCE_KINDS = (
    "analysis_state",
    "analysis_package",
    "agent_request",
    "agent_response",
    "system_context",
    "metadata_context",
)


def get_run_analysis(settings: Settings, store: Store, run_id: str) -> JsonObject:
    run = store.get_run(run_id)
    value: JsonObject = {
        "run": run,
        "workspace": store.get_workspace(run["workspace_id"]),
        "timeline": store.list_timeline(run_id),
        "evidence": store.list_evidence(run_id),
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


def optional_latest_artifact(
    settings: Settings,
    store: Store,
    run_id: str,
    kind: str,
) -> JsonObject | None:
    try:
        return read_latest_evidence_artifact(settings, store, run_id, kind)
    except ValueError:
        return None
