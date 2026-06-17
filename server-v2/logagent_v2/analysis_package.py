from __future__ import annotations

import json

from .artifacts import resolve_artifact_path, write_artifact_bytes
from .config import Settings
from .store import JsonObject, Store


MAX_PACKAGE_FILES = 50
MAX_PACKAGE_MATCHES = 50
MAX_CONTEXT_RESOURCES = 10
MAX_PACKAGE_ARTIFACTS = 50
SESSION_TEXT_INPUT_REF = "session_text_input.json#question"


def persist_analysis_package(
    settings: Settings,
    store: Store,
    workspace_id: str,
    run_id: str,
    evidence_bundle: JsonObject,
) -> JsonObject:
    package = build_analysis_package(settings, store, workspace_id, run_id, evidence_bundle)
    data = json.dumps(package, ensure_ascii=True, indent=2).encode("utf-8")
    artifact = write_artifact_bytes(
        settings=settings,
        store=store,
        workspace_id=workspace_id,
        filename="analysis_package.json",
        data=data,
        content_type="application/json",
        schema_name="logagent.v2.analysis_package.v1",
        preview={
            "fileCount": package["manifest"]["fileCount"],
            "matchCount": package["grepResults"]["totalMatches"],
            "allowedEvidenceRefCount": len(package["allowedEvidenceRefs"]),
        },
    )
    store.create_evidence(
        workspace_id=workspace_id,
        run_id=run_id,
        kind="analysis_package",
        final_allowed=False,
        summary="Analysis package captured bounded task context for the Agent loop.",
        artifact_id=artifact["id"],
        payload={"artifactId": artifact["id"], "path": "analysis_package.json"},
    )
    return {"package": package, "artifact": artifact}


def build_analysis_package(
    settings: Settings,
    store: Store,
    workspace_id: str,
    run_id: str,
    evidence_bundle: JsonObject,
) -> JsonObject:
    workspace = store.get_workspace(workspace_id)
    run = store.get_run(run_id)
    manifest = evidence_bundle["manifest"]
    grep_results = evidence_bundle["grepResults"]
    matches = grep_results.get("matches", [])[:MAX_PACKAGE_MATCHES]
    return {
        "schemaVersion": 1,
        "workspace": {
            "workspaceId": workspace_id,
            "title": workspace.get("title"),
            "question": workspace.get("question"),
            "sourceUrl": workspace.get("sourceUrl"),
            "instanceId": workspace.get("instanceId"),
            "nodeId": workspace.get("nodeId"),
            "mode": workspace.get("mode"),
            "language": workspace.get("language"),
            "systemContextIds": workspace.get("systemContextIds", []),
            "skillIds": workspace.get("skillIds", []),
            "uploadIds": workspace.get("uploadIds", []),
        },
        "run": {
            "runId": run_id,
            "status": run.get("status"),
            "phase": run.get("phase"),
            "budget": run.get("budget", {}),
        },
        "analysisState": analysis_state_outline(store, run_id),
        "resources": task_resource_index(run_id),
        "artifactIndex": artifact_index_outline(store, run_id),
        "manifest": manifest_outline(manifest),
        "grepResults": grep_outline(grep_results, matches),
        "toolInputIndex": tool_input_outline(evidence_bundle.get("toolInputIndex")),
        "toolResults": evidence_bundle.get("toolResults", []),
        "systemContext": context_outline(settings, store, run_id, "system_context"),
        "metadataContext": context_outline(settings, store, run_id, "metadata_context"),
        "backgroundEvidence": evidence_bundle.get("backgroundEvidence", []),
        "allowedEvidenceRefs": allowed_evidence_refs(
            matches,
            evidence_bundle.get("toolResults"),
            code_evidence_refs(settings, store, run_id),
        ),
        "finalEvidencePolicy": {
            "allowed": [
                "session_text_input.json#question",
                "grep_results.json#matches/<index>",
                "log_searches/<id>.json#matches/<index>",
                "log_slices/<id>.json#lines",
                "case_context.json#cases/<index>",
                "tool_results/<action_id>/result.json#findings/<index>",
                "tool_results/<action_id>/result.json#response",
                "code_evidence/<action_id>.json#matches/<index>",
            ],
            "backgroundOnlyKinds": [
                "manifest",
                "system_context",
                "metadata_context",
                "analysis_package",
                "environment_evidence",
                "metadata_slice",
                "skill_reference",
            ],
        },
    }


def analysis_state_outline(store: Store, run_id: str) -> JsonObject:
    timeline = store.list_timeline(run_id)
    user_messages = [
        {
            "questionId": event.get("payload", {}).get("questionId"),
            "message": event.get("payload", {}).get("message"),
            "resumeMode": event.get("payload", {}).get("resumeMode"),
            "createdAt": event.get("created_at"),
        }
        for event in timeline
        if event.get("kind") == "user.message"
        and isinstance(event.get("payload"), dict)
        and isinstance(event["payload"].get("message"), str)
    ]
    actions = store.list_actions(run_id)
    pending_actions = [
        {
            "id": action.get("id"),
            "kind": action.get("kind"),
            "payload": action.get("payload", {}),
            "createdAt": action.get("created_at"),
        }
        for action in actions
        if action.get("status") == "pending"
    ]
    action_results = [
        {
            "id": action.get("id"),
            "kind": action.get("kind"),
            "status": action.get("status"),
            "result": action.get("result"),
            "updatedAt": action.get("updated_at"),
        }
        for action in actions
        if action.get("status") != "pending"
    ]
    finalize_requested = (
        bool(user_messages) and user_messages[-1].get("resumeMode") == "finalize"
    )
    return {
        "finalizeRequested": finalize_requested,
        "resumeDirective": (
            "finalize_with_current_evidence" if finalize_requested else None
        ),
        "recentUserMessages": user_messages[-10:],
        "pendingActions": pending_actions[-10:],
        "actionResults": action_results[-10:],
    }


def task_resource_index(run_id: str) -> list[JsonObject]:
    names = [
        "summary",
        "artifact_index",
        "evidence",
        "manifest",
        "grep_results",
        "system_context",
        "metadata_context",
        "environment_evidence",
        "analysis_package",
        "analysis_state",
        "agent_request",
        "agent_response",
        "claude_mcp_config",
        "claude_session",
        "case_context",
        "code_evidence",
        "tool_results",
        "mcp_calls",
        "result",
        "result_markdown",
    ]
    return [
        {
            "uri": f"logagent-v2://run/{run_id}/{name}",
            "name": name,
            "mimeType": "application/json",
        }
        for name in names
    ]


def artifact_index_outline(store: Store, run_id: str) -> JsonObject:
    run_artifacts = store.list_run_artifacts(run_id)
    artifacts: list[JsonObject] = []
    for upload in run_artifacts["uploads"]:
        artifacts.append(
            {
                "path": f"uploads/{upload['upload_id']}/{upload['filename']}",
                "source": "upload",
                "artifactId": upload["artifact_id"],
                "sizeBytes": upload["size_bytes"],
                "contentType": upload["content_type"],
            }
        )
    for item in run_artifacts["evidenceArtifacts"]:
        payload = item.get("evidence_payload") or {}
        artifacts.append(
            {
                "path": payload.get("path") or item["relative_path"],
                "source": "evidence",
                "artifactId": item["artifact_id"],
                "evidenceKind": item["evidence_kind"],
                "finalAllowed": item["final_allowed"],
                "sizeBytes": item["size_bytes"],
                "contentType": item["content_type"],
            }
        )
    for item in run_artifacts.get("supportArtifacts", []):
        artifacts.append(
            {
                "path": item.get("logical_path") or item["relative_path"],
                "source": "support",
                "artifactId": item["artifact_id"],
                "role": item.get("role"),
                "actionId": item.get("action_id"),
                "sizeBytes": item["size_bytes"],
                "contentType": item["content_type"],
            }
        )
    return {
        "artifactCount": len(artifacts),
        "supportArtifactCount": len(run_artifacts.get("supportArtifacts", [])),
        "truncated": len(artifacts) > MAX_PACKAGE_ARTIFACTS,
        "artifacts": artifacts[:MAX_PACKAGE_ARTIFACTS],
    }


def allowed_evidence_refs(
    matches: list[JsonObject],
    tool_results: object | None = None,
    code_refs: list[str] | None = None,
) -> list[str]:
    refs = [
        match["ref"]
        for match in matches
        if isinstance(match, dict) and isinstance(match.get("ref"), str)
    ]
    tool_refs: list[str] = []
    if isinstance(tool_results, list):
        for result in tool_results:
            if not isinstance(result, dict):
                continue
            final_refs = result.get("finalEvidenceRefs")
            if not isinstance(final_refs, list):
                continue
            tool_refs.extend(ref for ref in final_refs if isinstance(ref, str))
    return list(dict.fromkeys([SESSION_TEXT_INPUT_REF, *refs, *tool_refs, *(code_refs or [])]))


def code_evidence_refs(settings: Settings, store: Store, run_id: str) -> list[str]:
    refs: list[str] = []
    for evidence in store.list_evidence(run_id):
        if evidence.get("kind") != "code_evidence" or not evidence.get("final_allowed"):
            continue
        artifact_id = evidence.get("artifact_id")
        if not isinstance(artifact_id, str):
            continue
        try:
            artifact = store.get_artifact(artifact_id)
            path = resolve_artifact_path(settings, artifact["relative_path"])
            value = json.loads(path.read_text(encoding="utf-8"))
        except Exception:
            continue
        matches = value.get("matches") if isinstance(value, dict) else None
        if not isinstance(matches, list):
            continue
        for match in matches:
            if isinstance(match, dict) and isinstance(match.get("ref"), str):
                refs.append(match["ref"])
    return list(dict.fromkeys(refs))


def manifest_outline(manifest: JsonObject) -> JsonObject:
    return {
        "schemaVersion": manifest.get("schemaVersion"),
        "uploadCount": manifest.get("uploadCount"),
        "fileCount": manifest.get("fileCount"),
        "toolInputsPath": manifest.get("toolInputsPath"),
        "toolInputCount": manifest.get("toolInputCount", 0),
        "files": [
            {
                "path": item.get("path"),
                "sourceFilename": item.get("sourceFilename"),
                "sizeBytes": item.get("sizeBytes"),
                "logGroup": item.get("logGroup"),
                "nodePackage": item.get("nodePackage"),
            }
            for item in manifest.get("files", [])[:MAX_PACKAGE_FILES]
            if isinstance(item, dict)
        ],
    }


def grep_outline(grep_results: JsonObject, matches: list[JsonObject]) -> JsonObject:
    keyword_counts = grep_results.get("keywordCounts", {})
    return {
        "schemaVersion": grep_results.get("schemaVersion"),
        "keywords": grep_results.get("keywords", []),
        "keywordCounts": keyword_counts if isinstance(keyword_counts, dict) else {},
        "totalMatches": grep_results.get("totalMatches", 0),
        "truncated": grep_results.get("truncated", False),
        "matches": [
            {
                "ref": match.get("ref"),
                "path": match.get("path"),
                "lineNumber": match.get("lineNumber"),
                "keyword": match.get("keyword"),
                "text": match.get("text"),
            }
            for match in matches
            if isinstance(match, dict)
        ],
    }


def tool_input_outline(tool_input_index: JsonObject | None) -> JsonObject | None:
    if not isinstance(tool_input_index, dict):
        return None
    inputs = tool_input_index.get("inputs")
    if not isinstance(inputs, list):
        return None
    return {
        "path": tool_input_index.get("path"),
        "inputCount": len(inputs),
        "inputs": [
            {
                "path": item.get("path"),
                "inputKind": item.get("inputKind"),
                "scope": item.get("scope"),
                "toolIds": item.get("toolIds"),
                "sourceFiles": item.get("sourceFiles"),
                "recordCount": item.get("recordCount"),
            }
            for item in inputs[:MAX_CONTEXT_RESOURCES]
            if isinstance(item, dict)
        ],
    }


def context_outline(
    settings: Settings, store: Store, run_id: str, kind: str
) -> JsonObject | None:
    value = read_latest_context_artifact(settings, store, run_id, kind)
    if value is None:
        return None
    if kind == "system_context":
        resources = value.get("resources", [])
        system_resources = value.get("systemResources", [])
        return {
            "schemaVersion": value.get("schemaVersion"),
            "resourceCount": len(resources) if isinstance(resources, list) else 0,
            "systemResourceCount": len(system_resources)
            if isinstance(system_resources, list)
            else 0,
            "resources": [
                {
                    "kind": item.get("kind"),
                    "skillId": item.get("skillId"),
                    "selectionReason": item.get("selectionReason"),
                    "matchScore": item.get("matchScore"),
                    "summary": item.get("summary"),
                    "referenceCount": len(item.get("references", []))
                    if isinstance(item.get("references"), list)
                    else 0,
                }
                for item in resources[:MAX_CONTEXT_RESOURCES]
                if isinstance(item, dict)
            ],
            "systemResources": [
                {
                    "kind": item.get("kind"),
                    "contextId": item.get("contextId"),
                    "title": item.get("title"),
                    "summary": item.get("summary"),
                    "source": item.get("source"),
                }
                for item in system_resources[:MAX_CONTEXT_RESOURCES]
                if isinstance(item, dict)
            ],
        }
    if kind == "metadata_context":
        resources = value.get("resources", [])
        return {
            "schemaVersion": value.get("schemaVersion"),
            "selection": value.get("selection", {}),
            "resourceCount": len(resources) if isinstance(resources, list) else 0,
            "resources": [
                {
                    "kind": item.get("kind"),
                    "instanceId": item.get("instanceId"),
                    "selectionReason": item.get("selectionReason"),
                    "matchScore": item.get("matchScore"),
                    "product": item.get("product"),
                    "environment": item.get("environment"),
                    "cluster": {
                        "nodeCount": (item.get("cluster") or {}).get("nodeCount"),
                        "databaseCount": (item.get("cluster") or {}).get("databaseCount"),
                    }
                    if isinstance(item.get("cluster"), dict)
                    else {},
                }
                for item in resources[:MAX_CONTEXT_RESOURCES]
                if isinstance(item, dict)
            ],
        }
    return value


def read_latest_context_artifact(
    settings: Settings, store: Store, run_id: str, kind: str
) -> JsonObject | None:
    candidates = [item for item in store.list_evidence(run_id) if item["kind"] == kind]
    if not candidates:
        return None
    artifact_id = candidates[-1].get("artifact_id")
    if not artifact_id:
        return None
    artifact = store.get_artifact(artifact_id)
    path = resolve_artifact_path(settings, artifact["relative_path"])
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except Exception:
        return None
    return value if isinstance(value, dict) else None
