from __future__ import annotations

import json

from .artifacts import resolve_artifact_path, write_artifact_bytes
from .config import Settings
from .store import JsonObject, Store


MAX_PACKAGE_FILES = 50
MAX_PACKAGE_MATCHES = 50
MAX_CONTEXT_RESOURCES = 10
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
        "resources": task_resource_index(run_id),
        "manifest": manifest_outline(manifest),
        "grepResults": grep_outline(grep_results, matches),
        "toolInputIndex": tool_input_outline(evidence_bundle.get("toolInputIndex")),
        "systemContext": context_outline(settings, store, run_id, "system_context"),
        "metadataContext": context_outline(settings, store, run_id, "metadata_context"),
        "backgroundEvidence": evidence_bundle.get("backgroundEvidence", []),
        "allowedEvidenceRefs": allowed_evidence_refs(matches),
        "finalEvidencePolicy": {
            "allowed": [
                "session_text_input.json#question",
                "grep_results.json#matches/<index>",
                "log_searches/<id>.json#matches/<index>",
                "log_slices/<id>.json#lines",
                "case_context.json#cases/<index>",
                "tool_results/<action_id>/result.json#findings/<index>",
                "tool_results/<action_id>/result.json#response",
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
        "case_context",
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


def allowed_evidence_refs(matches: list[JsonObject]) -> list[str]:
    refs = [
        match["ref"]
        for match in matches
        if isinstance(match, dict) and isinstance(match.get("ref"), str)
    ]
    return [SESSION_TEXT_INPUT_REF, *refs]


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
