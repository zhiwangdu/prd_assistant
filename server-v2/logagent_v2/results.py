from __future__ import annotations

import json

from .artifacts import resolve_artifact_path, write_artifact_bytes
from .config import Settings
from .store import JsonObject, Store, now_iso


def persist_run_result(
    settings: Settings,
    store: Store,
    workspace_id: str,
    run_id: str,
    final_answer: JsonObject,
) -> JsonObject:
    result_doc = {
        "schemaVersion": 1,
        "runId": run_id,
        "createdAt": now_iso(),
        "finalAnswer": final_answer,
    }
    json_artifact = write_artifact_bytes(
        settings=settings,
        store=store,
        workspace_id=workspace_id,
        filename="result.json",
        data=json.dumps(result_doc, ensure_ascii=True, indent=2).encode("utf-8"),
        content_type="application/json",
        schema_name="logagent.v2.result.v1",
        preview=result_preview(final_answer),
    )
    json_evidence = store.create_evidence(
        workspace_id=workspace_id,
        run_id=run_id,
        kind="result",
        final_allowed=False,
        summary="Final result JSON artifact.",
        artifact_id=json_artifact["id"],
        payload={"artifactId": json_artifact["id"], "path": "result.json"},
    )
    markdown = render_result_markdown(final_answer)
    markdown_artifact = write_artifact_bytes(
        settings=settings,
        store=store,
        workspace_id=workspace_id,
        filename="result.md",
        data=markdown.encode("utf-8"),
        content_type="text/markdown",
        schema_name="logagent.v2.result_markdown.v1",
        preview=result_preview(final_answer),
    )
    markdown_evidence = store.create_evidence(
        workspace_id=workspace_id,
        run_id=run_id,
        kind="result_markdown",
        final_allowed=False,
        summary="Final result Markdown artifact.",
        artifact_id=markdown_artifact["id"],
        payload={"artifactId": markdown_artifact["id"], "path": "result.md"},
    )
    return {
        "result": result_doc,
        "jsonArtifact": json_artifact,
        "jsonEvidence": json_evidence,
        "markdownArtifact": markdown_artifact,
        "markdownEvidence": markdown_evidence,
    }


def get_run_result(settings: Settings, store: Store, run_id: str) -> JsonObject:
    run = store.get_run(run_id)
    if not run.get("finalAnswer"):
        raise ValueError(f"run {run_id} has no final result")
    json_evidence = latest_evidence(store, run_id, "result")
    markdown_evidence = latest_evidence(store, run_id, "result_markdown")
    result = read_json_artifact(settings, store, json_evidence["artifact_id"])
    return {
        "run": run,
        "finalAnswer": run["finalAnswer"],
        "result": result,
        "artifacts": {
            "json": store.get_artifact(json_evidence["artifact_id"]),
            "markdown": store.get_artifact(markdown_evidence["artifact_id"]),
        },
        "evidence": {
            "json": json_evidence,
            "markdown": markdown_evidence,
        },
    }


def latest_evidence(store: Store, run_id: str, kind: str) -> JsonObject:
    candidates = [item for item in store.list_evidence(run_id) if item["kind"] == kind]
    if not candidates:
        raise ValueError(f"no {kind} evidence exists for run {run_id}")
    artifact_id = candidates[-1].get("artifact_id")
    if not artifact_id:
        raise ValueError(f"{kind} evidence has no artifact")
    return candidates[-1]


def read_json_artifact(settings: Settings, store: Store, artifact_id: str) -> JsonObject:
    artifact = store.get_artifact(artifact_id)
    path = resolve_artifact_path(settings, artifact["relative_path"])
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError("result artifact is not a JSON object")
    return value


def read_text_artifact(settings: Settings, store: Store, artifact_id: str) -> str:
    artifact = store.get_artifact(artifact_id)
    path = resolve_artifact_path(settings, artifact["relative_path"])
    return path.read_text(encoding="utf-8")


def render_result_markdown(final_answer: JsonObject) -> str:
    lines = ["# LogAgent Result", ""]
    lines.extend(["## Summary", "", str(final_answer.get("summary", "")).strip(), ""])
    lines.extend(["## Confidence", "", str(final_answer.get("confidence", "")).strip(), ""])
    append_string_list(lines, "Symptoms", final_answer.get("symptoms", []))
    append_root_causes(lines, final_answer.get("likelyRootCauses", []))
    append_string_list(lines, "Next Checks", final_answer.get("nextChecks", []))
    append_string_list(lines, "Fix Suggestions", final_answer.get("fixSuggestions", []))
    append_string_list(lines, "Missing Information", final_answer.get("missingInformation", []))
    append_string_list(lines, "Evidence Refs", final_answer.get("evidenceRefs", []))
    return "\n".join(lines).rstrip() + "\n"


def append_string_list(lines: list[str], title: str, values: object) -> None:
    lines.extend([f"## {title}", ""])
    if not isinstance(values, list) or not values:
        lines.extend(["- None", ""])
        return
    for value in values:
        lines.append(f"- {value}")
    lines.append("")


def append_root_causes(lines: list[str], values: object) -> None:
    lines.extend(["## Likely Root Causes", ""])
    if not isinstance(values, list) or not values:
        lines.extend(["- None", ""])
        return
    for value in values:
        if not isinstance(value, dict):
            continue
        cause = value.get("cause", "")
        refs = value.get("evidenceRefs", [])
        suffix = ""
        if isinstance(refs, list) and refs:
            suffix = " (" + ", ".join(str(ref) for ref in refs) + ")"
        lines.append(f"- {cause}{suffix}")
    lines.append("")


def result_preview(final_answer: JsonObject) -> JsonObject:
    return {
        "summary": str(final_answer.get("summary", ""))[:300],
        "confidence": final_answer.get("confidence"),
        "evidenceRefCount": len(final_answer.get("evidenceRefs", []))
        if isinstance(final_answer.get("evidenceRefs"), list)
        else 0,
    }
