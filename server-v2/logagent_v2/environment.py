from __future__ import annotations

from .config import Settings
from .evidence import write_json_artifact
from .store import JsonObject, Store, now_iso


ENVIRONMENT_ACTION_TYPE = "collect_environment"
ENVIRONMENT_EVIDENCE_KIND = "environment_evidence"


def persist_approved_environment_evidence(
    settings: Settings,
    store: Store,
    action: JsonObject,
) -> JsonObject | None:
    """Record the V1-compatible mock environment artifact after approval.

    The current Rust server only records a mock environment evidence file after
    a collect_environment approval. V2 keeps that explicit MOCK marker so the
    later real SSH/SCP collector can replace this function without changing the
    user-facing action flow.
    """

    if action.get("kind") != "approval" or action.get("status") != "approved":
        return None
    payload = action.get("payload") or {}
    if payload.get("actionType") != ENVIRONMENT_ACTION_TYPE:
        return None

    run_id = action["run_id"]
    for evidence in store.list_evidence(run_id):
        if (
            evidence.get("kind") == ENVIRONMENT_EVIDENCE_KIND
            and evidence.get("payload", {}).get("actionId") == action["id"]
        ):
            return evidence

    run = store.get_run(run_id)
    artifact_path = f"environment_evidence/{action['id']}/result.json"
    result = {
        "schemaVersion": 1,
        "actionId": action["id"],
        "status": "MOCK",
        "summary": "mock environment evidence captured after user approval",
        "input": payload.get("input") if isinstance(payload.get("input"), dict) else {},
        "createdAt": now_iso(),
        "finalEvidenceAllowed": False,
    }
    artifact = write_json_artifact(
        settings=settings,
        store=store,
        workspace_id=run["workspace_id"],
        filename=f"{action['id']}_environment_result.json",
        value=result,
        schema_name="logagent.v2.environment_evidence.v1",
    )
    return store.create_evidence(
        workspace_id=run["workspace_id"],
        run_id=run_id,
        kind=ENVIRONMENT_EVIDENCE_KIND,
        final_allowed=False,
        summary=result["summary"],
        artifact_id=artifact["id"],
        payload={
            "artifactId": artifact["id"],
            "path": artifact_path,
            "actionId": action["id"],
            "status": result["status"],
            "finalEvidenceAllowed": False,
        },
    )
