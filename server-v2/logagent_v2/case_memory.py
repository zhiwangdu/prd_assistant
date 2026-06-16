from __future__ import annotations

import json
from typing import Any

from .artifacts import write_artifact_bytes
from .config import Settings
from .ids import new_id
from .store import JsonObject, Store, case_searchable_text, now_iso


CASE_REQUIRED_FIELDS = ("title", "symptom", "rootCause", "solution")


def create_manual_case(store: Store, payload: JsonObject) -> JsonObject:
    record = base_case_record("manual", payload)
    for field in CASE_REQUIRED_FIELDS:
        require_non_empty(record, field)
    return store.create_case(record, case_searchable_text(record))


def create_task_case(store: Store, run_id: str, payload: JsonObject) -> JsonObject:
    existing = store.find_case_by_task(run_id)
    if existing is not None:
        return existing
    run = store.get_run(run_id)
    if run["status"] != "succeeded" or not run.get("finalAnswer"):
        raise ValueError("only succeeded runs with finalAnswer can be saved as cases")
    final_answer = run["finalAnswer"]
    derived = {
        "title": final_answer.get("summary", "")[:180],
        "symptom": "\n".join(final_answer.get("symptoms", [])),
        "rootCause": "\n".join(
            item.get("cause", "")
            for item in final_answer.get("likelyRootCauses", [])
            if isinstance(item, dict)
        ),
        "solution": "\n".join(final_answer.get("fixSuggestions", [])),
        "evidenceRefs": collect_final_evidence_refs(final_answer),
    }
    derived.update({key: value for key, value in payload.items() if value is not None})
    record = base_case_record("task", derived)
    record["taskId"] = run_id
    record["sourceResultPath"] = f"runs/{run_id}/finalAnswer"
    for field in CASE_REQUIRED_FIELDS:
        require_non_empty(record, field)
    return store.create_case(record, case_searchable_text(record))


def update_case(store: Store, case_id: str, payload: JsonObject) -> JsonObject:
    current = store.get_case(case_id)
    updates = normalize_case_updates(payload)
    merged = dict(current)
    merged.update({key: value for key, value in updates.items() if value is not None})
    for field in CASE_REQUIRED_FIELDS:
        require_non_empty(merged, field)
    return store.update_case(case_id, updates, case_searchable_text(merged))


def base_case_record(source_type: str, payload: JsonObject) -> JsonObject:
    ts = now_iso()
    return {
        "schemaVersion": 2,
        "caseId": new_id("case"),
        "sourceType": source_type,
        "product": optional_string(payload.get("product")),
        "version": optional_string(payload.get("version")),
        "environment": optional_string(payload.get("environment")),
        "instanceId": optional_string(payload.get("instanceId")),
        "nodeId": optional_string(payload.get("nodeId")),
        "title": required_string_value(payload.get("title")),
        "symptom": required_string_value(payload.get("symptom")),
        "rootCause": required_string_value(payload.get("rootCause")),
        "solution": required_string_value(payload.get("solution")),
        "evidenceRefs": normalize_string_list(payload.get("evidenceRefs", [])),
        "enabled": bool(payload.get("enabled", True)),
        "createdAt": ts,
        "updatedAt": ts,
    }


def normalize_case_updates(payload: JsonObject) -> JsonObject:
    updates: JsonObject = {}
    for field in (
        "title",
        "symptom",
        "rootCause",
        "solution",
        "product",
        "version",
        "environment",
        "instanceId",
        "nodeId",
    ):
        if field in payload:
            updates[field] = optional_string(payload.get(field))
    if "evidenceRefs" in payload:
        updates["evidenceRefs"] = normalize_string_list(payload.get("evidenceRefs"))
    if "enabled" in payload:
        updates["enabled"] = bool(payload.get("enabled"))
    return updates


def collect_final_evidence_refs(final_answer: JsonObject) -> list[str]:
    refs = list(final_answer.get("evidenceRefs", []))
    for root_cause in final_answer.get("likelyRootCauses", []):
        if isinstance(root_cause, dict):
            refs.extend(root_cause.get("evidenceRefs", []))
    return list(dict.fromkeys(ref for ref in refs if isinstance(ref, str)))


def required_string_value(value: Any) -> str:
    return value.strip() if isinstance(value, str) else ""


def optional_string(value: Any) -> str | None:
    if not isinstance(value, str) or not value.strip():
        return None
    return value.strip()


def normalize_string_list(value: Any) -> list[str]:
    if value is None:
        return []
    if isinstance(value, str):
        value = [value]
    if not isinstance(value, list):
        raise ValueError("evidenceRefs must be an array of strings")
    return [item.strip() for item in value if isinstance(item, str) and item.strip()]


def require_non_empty(record: JsonObject, field: str) -> None:
    value = record.get(field)
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"case {field} is required")


def case_tool_descriptors() -> list[JsonObject]:
    return [
        {
            "name": "logagent.search_cases",
            "description": "Search confirmed V2 cases by keywords.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 20},
                    "includeDisabled": {"type": "boolean"},
                },
                "additionalProperties": False,
            },
        },
        {
            "name": "logagent.get_case",
            "description": "Read one confirmed V2 case by caseId.",
            "inputSchema": {
                "type": "object",
                "properties": {"caseId": {"type": "string", "minLength": 1}},
                "required": ["caseId"],
                "additionalProperties": False,
            },
        },
    ]


def call_case_tool(
    settings: Settings | None,
    store: Store,
    run: JsonObject | None,
    name: str,
    arguments: JsonObject,
) -> JsonObject:
    if name == "logagent.search_cases":
        value = {
            "cases": store.search_cases(
                query=optional_string(arguments.get("query")),
                limit=int(arguments.get("limit", 5)),
                include_disabled=bool(arguments.get("includeDisabled", False)),
            ),
            "finalEvidenceAllowed": False,
        }
    elif name == "logagent.get_case":
        value = {"case": store.get_case(require_string(arguments, "caseId"))}
    else:
        raise ValueError(f"unsupported case tool {name}")
    if settings is not None and run is not None:
        persist_case_context(settings, store, run, name, value)
    return value


def persist_case_context(
    settings: Settings,
    store: Store,
    run: JsonObject,
    tool_name: str,
    value: JsonObject,
) -> None:
    data = json.dumps(value, ensure_ascii=True, indent=2).encode("utf-8")
    artifact = write_artifact_bytes(
        settings=settings,
        store=store,
        workspace_id=run["workspace_id"],
        filename=f"{tool_name.removeprefix('logagent.').replace('.', '_')}.json",
        data=data,
        content_type="application/json",
        schema_name="logagent.v2.case_context.v1",
        preview={"tool": tool_name, "sizeBytes": len(data)},
    )
    store.create_evidence(
        workspace_id=run["workspace_id"],
        run_id=run["id"],
        kind="case_context",
        final_allowed=False,
        summary=f"Historical Case background from {tool_name}.",
        artifact_id=artifact["id"],
        payload={"artifactId": artifact["id"], "tool": tool_name},
    )


def require_string(arguments: JsonObject, field: str) -> str:
    value = arguments.get(field)
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"{field} is required")
    return value.strip()
