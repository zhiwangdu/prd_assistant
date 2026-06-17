from __future__ import annotations

import json
from typing import Any

from .artifacts import write_artifact_bytes
from .config import Settings
from .ids import new_id
from .results import latest_evidence, read_text_artifact
from .store import JsonObject, Store, now_iso


MCP_CALLS_FILENAME = "mcp_calls.jsonl"


def persist_mcp_call(
    settings: Settings,
    store: Store,
    run: JsonObject,
    name: str,
    arguments: JsonObject,
    status: str,
    result: JsonObject,
    evidence_refs: list[str] | None = None,
) -> JsonObject:
    existing_text = read_mcp_calls_text(settings, store, run["id"])
    existing_calls = parse_mcp_calls_text(existing_text)
    record = {
        "schemaVersion": 1,
        "callId": new_id("mcpcall"),
        "createdAt": now_iso(),
        "name": name,
        "arguments": arguments,
        "status": status,
        "result": result,
        "evidenceRefs": evidence_refs or evidence_refs_from_result(result),
    }
    next_text = existing_text
    if next_text and not next_text.endswith("\n"):
        next_text += "\n"
    next_text += json.dumps(record, ensure_ascii=True, separators=(",", ":")) + "\n"
    data = next_text.encode("utf-8")
    artifact = write_artifact_bytes(
        settings=settings,
        store=store,
        workspace_id=run["workspace_id"],
        filename=MCP_CALLS_FILENAME,
        data=data,
        content_type="application/x-ndjson",
        schema_name="logagent.v2.mcp_calls.v1",
        preview={
            "filename": MCP_CALLS_FILENAME,
            "callCount": len(existing_calls) + 1,
            "lastName": name,
            "lastStatus": status,
            "sizeBytes": len(data),
        },
    )
    evidence = store.create_evidence(
        workspace_id=run["workspace_id"],
        run_id=run["id"],
        kind="mcp_calls",
        final_allowed=False,
        summary=f"Task MCP call audit captured {len(existing_calls) + 1} call(s).",
        artifact_id=artifact["id"],
        payload={
            "artifactId": artifact["id"],
            "path": MCP_CALLS_FILENAME,
            "callCount": len(existing_calls) + 1,
            "lastName": name,
            "lastStatus": status,
        },
    )
    return {"record": record, "artifact": artifact, "evidence": evidence}


def read_mcp_calls(settings: Settings, store: Store, run_id: str) -> JsonObject:
    calls = parse_mcp_calls_text(read_mcp_calls_text(settings, store, run_id))
    return {
        "schemaVersion": 1,
        "kind": "mcp_calls",
        "runId": run_id,
        "path": MCP_CALLS_FILENAME,
        "callCount": len(calls),
        "calls": calls,
        "finalEvidenceAllowed": False,
    }


def read_mcp_calls_text(settings: Settings, store: Store, run_id: str) -> str:
    try:
        evidence = latest_evidence(store, run_id, "mcp_calls")
        return read_text_artifact(settings, store, evidence["artifact_id"])
    except Exception:
        return ""


def parse_mcp_calls_text(text: str) -> list[JsonObject]:
    calls = []
    for line in text.splitlines():
        if not line.strip():
            continue
        value = json.loads(line)
        if isinstance(value, dict):
            calls.append(value)
    return calls


def evidence_refs_from_result(value: Any) -> list[str]:
    refs: list[str] = []

    def visit(item: Any) -> None:
        if isinstance(item, dict):
            for key, child in item.items():
                if key == "evidenceRefs" and isinstance(child, list):
                    refs.extend(ref for ref in child if isinstance(ref, str))
                elif key in {"backgroundRef", "ref"} and isinstance(child, str):
                    refs.append(child)
                else:
                    visit(child)
        elif isinstance(item, list):
            for child in item:
                visit(child)

    visit(value)
    return list(dict.fromkeys(refs))

