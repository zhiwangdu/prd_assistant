from __future__ import annotations

import json

from .artifacts import write_artifact_bytes
from .config import Settings
from .store import JsonObject, Store, now_iso


def persist_agent_request(
    settings: Settings,
    store: Store,
    workspace_id: str,
    run_id: str,
    attempt: int,
    provider_request: JsonObject,
    analysis_package_artifact_id: str | None,
) -> JsonObject:
    document = {
        "schemaVersion": 1,
        "kind": "agent_request",
        "runId": run_id,
        "attempt": attempt,
        "createdAt": now_iso(),
        "provider": provider_request.get("provider"),
        "model": provider_request.get("model"),
        "transport": provider_request.get("transport", {}),
        "allowedEvidenceRefs": provider_request.get("allowedEvidenceRefs", []),
        "analysisPackageArtifactId": analysis_package_artifact_id,
        "payload": provider_request.get("payload", {}),
    }
    return persist_agent_audit_artifact(
        settings=settings,
        store=store,
        workspace_id=workspace_id,
        run_id=run_id,
        kind="agent_request",
        filename="agent_request.json",
        schema_name="logagent.v2.agent_request.v1",
        document=document,
        summary="Agent round request captured before provider execution.",
        preview={
            "attempt": attempt,
            "provider": document["provider"],
            "model": document["model"],
            "allowedEvidenceRefCount": len(document["allowedEvidenceRefs"]),
        },
    )


def persist_agent_response(
    settings: Settings,
    store: Store,
    workspace_id: str,
    run_id: str,
    attempt: int,
    provider_response: JsonObject,
    request_artifact_id: str | None,
) -> JsonObject:
    document = {
        "schemaVersion": 1,
        "kind": "agent_response",
        "runId": run_id,
        "attempt": attempt,
        "createdAt": now_iso(),
        "provider": provider_response.get("provider"),
        "model": provider_response.get("model"),
        "status": provider_response.get("status"),
        "requestArtifactId": request_artifact_id,
        "response": provider_response.get("response"),
        "reason": provider_response.get("reason"),
        "error": provider_response.get("error"),
        "finalAnswer": provider_response.get("finalAnswer"),
        "toolCalls": provider_response.get("toolCalls"),
        "toolObservations": provider_response.get("toolObservations"),
        "validatedFinalAnswer": provider_response.get("validatedFinalAnswer"),
        "validation": provider_response.get("validation"),
    }
    return persist_agent_audit_artifact(
        settings=settings,
        store=store,
        workspace_id=workspace_id,
        run_id=run_id,
        kind="agent_response",
        filename="agent_response.json",
        schema_name="logagent.v2.agent_response.v1",
        document=without_none(document),
        summary="Agent round response captured after provider execution.",
        preview={
            "attempt": attempt,
            "provider": document["provider"],
            "model": document["model"],
            "status": document["status"],
            "validationStatus": (document.get("validation") or {}).get("status")
            if isinstance(document.get("validation"), dict)
            else None,
        },
    )


def persist_analysis_state(
    settings: Settings,
    store: Store,
    workspace_id: str,
    run_id: str,
    state: JsonObject,
) -> JsonObject:
    document = {
        "schemaVersion": 1,
        "kind": "analysis_state",
        "runId": run_id,
        "updatedAt": now_iso(),
        **state,
    }
    return persist_agent_audit_artifact(
        settings=settings,
        store=store,
        workspace_id=workspace_id,
        run_id=run_id,
        kind="analysis_state",
        filename="analysis_state.json",
        schema_name="logagent.v2.analysis_state.v1",
        document=document,
        summary=f"Analysis state snapshot captured with status {document.get('status')}.",
        preview={
            "status": document.get("status"),
            "phase": document.get("phase"),
            "roundCount": len(document.get("rounds", []))
            if isinstance(document.get("rounds"), list)
            else 0,
        },
    )


def persist_agent_audit_artifact(
    settings: Settings,
    store: Store,
    workspace_id: str,
    run_id: str,
    kind: str,
    filename: str,
    schema_name: str,
    document: JsonObject,
    summary: str,
    preview: JsonObject,
) -> JsonObject:
    data = json.dumps(document, ensure_ascii=True, indent=2).encode("utf-8")
    artifact = write_artifact_bytes(
        settings=settings,
        store=store,
        workspace_id=workspace_id,
        filename=filename,
        data=data,
        content_type="application/json",
        schema_name=schema_name,
        preview=preview,
    )
    evidence = store.create_evidence(
        workspace_id=workspace_id,
        run_id=run_id,
        kind=kind,
        final_allowed=False,
        summary=summary,
        artifact_id=artifact["id"],
        payload={
            "artifactId": artifact["id"],
            "path": filename,
            "attempt": document.get("attempt"),
            "provider": document.get("provider"),
            "status": document.get("status"),
        },
    )
    return {"document": document, "artifact": artifact, "evidence": evidence}


def failed_agent_response(
    provider_request: JsonObject,
    error: Exception,
    stage: str = "agent_runtime",
) -> JsonObject:
    return {
        "provider": provider_request.get("provider"),
        "model": provider_request.get("model"),
        "status": "failed",
        "error": {
            "stage": stage,
            "type": error.__class__.__name__,
            "message": str(error)[:4000],
        },
    }


def without_none(value: JsonObject) -> JsonObject:
    return {key: item for key, item in value.items() if item is not None}
