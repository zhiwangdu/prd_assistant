from __future__ import annotations

import json
import re
from hashlib import sha256
from typing import Any

from .artifacts import write_artifact_bytes
from .config import Settings
from .ids import new_id
from .store import JsonObject, Store, case_searchable_text, now_iso


CASE_REQUIRED_FIELDS = ("title", "symptom", "rootCause", "solution")

CASE_IMPORT_SECTION_ALIASES = {
    "title": {"title", "case title", "标题", "案例标题"},
    "symptom": {"symptom", "symptoms", "现象", "故障现象", "问题现象"},
    "rootCause": {
        "root cause",
        "rootcause",
        "cause",
        "原因",
        "根因",
        "根本原因",
    },
    "solution": {"solution", "fix", "resolution", "解决方案", "修复", "处理方案"},
    "product": {"product", "产品"},
    "version": {"version", "版本"},
    "environment": {"environment", "env", "环境"},
    "instanceId": {"instance", "instanceid", "instance id", "实例", "实例id"},
    "nodeId": {"node", "nodeid", "node id", "节点", "节点id"},
    "evidenceRefs": {"evidencerefs", "evidence refs", "evidence", "证据", "证据引用"},
}


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


def preview_case_import(
    store: Store,
    content: str,
    filename: str | None = None,
) -> JsonObject:
    draft = draft_case_from_text(content)
    validation_errors = validate_case_draft(draft)
    case_import = store.create_case_import(
        source_text=content,
        filename=filename,
        draft=draft,
        validation_errors=validation_errors,
    )
    return {"import": case_import_preview(case_import)}


def confirm_case_import(
    store: Store,
    import_id: str,
    overrides: JsonObject | None = None,
) -> JsonObject:
    case_import = store.get_case_import(import_id)
    if case_import["status"] == "confirmed" and case_import.get("caseId"):
        return {
            "import": case_import_preview(case_import),
            "case": store.get_case(case_import["caseId"]),
        }
    draft = dict(case_import.get("draft", {}))
    draft.update(normalize_case_import_overrides(overrides or {}))
    validation_errors = validate_case_draft(draft)
    if validation_errors:
        store.update_case_import(
            import_id,
            status="previewed",
            draft=draft,
            validation_errors=validation_errors,
        )
        raise ValueError("case import draft is incomplete: " + "; ".join(validation_errors))
    case = create_manual_case(store, draft)
    confirmed = store.update_case_import(
        import_id,
        status="confirmed",
        draft=draft,
        validation_errors=[],
        case_id=case["caseId"],
    )
    return {"import": case_import_preview(confirmed), "case": case}


def update_case_import_draft(
    store: Store,
    import_id: str,
    overrides: JsonObject,
) -> JsonObject:
    case_import = store.get_case_import(import_id)
    if case_import["status"] == "confirmed":
        raise ValueError("case import draft is already confirmed")
    draft = dict(case_import.get("draft", {}))
    draft.update(normalize_case_import_overrides(overrides or {}))
    validation_errors = validate_case_draft(draft)
    updated = store.update_case_import(
        import_id,
        status="previewed",
        draft=draft,
        validation_errors=validation_errors,
    )
    return {"import": case_import_preview(updated)}


def append_case_import_message(store: Store, import_id: str, message: str) -> JsonObject:
    case_import = store.get_case_import(import_id)
    if case_import["status"] == "confirmed":
        raise ValueError("case import draft is already confirmed")
    content = message.strip()
    if not content:
        raise ValueError("message must not be empty")
    messages = list(case_import.get("messages", []))
    messages.append({"role": "user", "content": content, "createdAt": now_iso()})
    combined_text = combine_case_import_text(case_import["sourceText"], messages)
    reparsed = draft_case_from_text(combined_text)
    draft = dict(case_import.get("draft", {}))
    draft.update({key: value for key, value in reparsed.items() if key != "_freeText"})
    validation_errors = validate_case_draft(draft)
    if validation_errors:
        messages.append(
            {
                "role": "assistant",
                "content": default_case_import_question(validation_errors),
                "createdAt": now_iso(),
            }
        )
    updated = store.update_case_import(
        import_id,
        status="previewed",
        draft=draft,
        validation_errors=validation_errors,
        messages=messages,
    )
    return {"import": case_import_preview(updated)}


def case_import_preview(case_import: JsonObject) -> JsonObject:
    return {
        "importId": case_import["importId"],
        "status": case_import["status"],
        "filename": case_import.get("filename"),
        "caseId": case_import.get("caseId"),
        "draft": case_import.get("draft", {}),
        "validationErrors": case_import.get("validationErrors", []),
        "messages": case_import.get("messages", []),
        "sourceSizeBytes": case_import.get("sourceSizeBytes", 0),
        "createdAt": case_import["createdAt"],
        "updatedAt": case_import["updatedAt"],
    }


def draft_case_from_text(content: str) -> JsonObject:
    parsed_json = try_parse_case_json(content)
    if parsed_json is not None:
        return normalize_case_import_overrides(parsed_json)
    sections = parse_case_sections(content)
    draft = normalize_case_import_overrides(sections)
    free_text = sections.get("_freeText", "")
    non_empty_lines = [line.strip() for line in content.splitlines() if line.strip()]
    if not draft.get("title") and non_empty_lines:
        draft["title"] = non_empty_lines[0][:180]
    if not draft.get("symptom") and free_text:
        draft["symptom"] = free_text
    return draft


def combine_case_import_text(source_text: str, messages: list[JsonObject]) -> str:
    parts = [source_text.strip()]
    for message in messages:
        content = message.get("content")
        if isinstance(content, str) and content.strip():
            role = str(message.get("role") or "user")
            parts.append(f"{role} supplement:\n{content.strip()}")
    return "\n\n".join(part for part in parts if part)


def default_case_import_question(validation_errors: list[str]) -> str:
    fields = [error.removesuffix(" is required") for error in validation_errors]
    return "Please provide missing Case fields: " + ", ".join(fields)


def try_parse_case_json(content: str) -> JsonObject | None:
    stripped = content.strip()
    if not stripped.startswith("{"):
        return None
    try:
        value = json.loads(stripped)
    except json.JSONDecodeError:
        return None
    if not isinstance(value, dict):
        return None
    return value


def parse_case_sections(content: str) -> JsonObject:
    result: JsonObject = {"_freeText": ""}
    buffers: dict[str, list[str]] = {}
    current_key: str | None = None
    free_lines: list[str] = []
    for raw_line in content.splitlines():
        line = raw_line.rstrip()
        parsed = parse_case_section_line(line)
        if parsed is not None:
            current_key, value = parsed
            if value:
                buffers.setdefault(current_key, []).append(value)
            continue
        if current_key is not None:
            buffers.setdefault(current_key, []).append(line)
        elif line.strip():
            free_lines.append(line.strip())
    for key, lines in buffers.items():
        text = "\n".join(line.strip() for line in lines if line.strip()).strip()
        if not text:
            continue
        if key == "evidenceRefs":
            result[key] = split_evidence_refs(text)
        else:
            result[key] = text
    result["_freeText"] = "\n".join(free_lines).strip()
    return result


def parse_case_section_line(line: str) -> tuple[str, str] | None:
    stripped = line.strip().lstrip("#*- >").strip()
    if not stripped:
        return None
    key = canonical_case_import_key(stripped)
    if key is not None:
        return key, ""
    match = re.match(r"^([^:：]{1,40})[:：]\s*(.*)$", stripped)
    if not match:
        return None
    key = canonical_case_import_key(match.group(1))
    if key is None:
        return None
    return key, match.group(2).strip()


def canonical_case_import_key(label: str) -> str | None:
    normalized = re.sub(r"\s+", " ", label.strip().lower())
    normalized = normalized.removesuffix(":").removesuffix("：").strip()
    for key, aliases in CASE_IMPORT_SECTION_ALIASES.items():
        if normalized in aliases:
            return key
    return None


def normalize_case_import_overrides(payload: JsonObject) -> JsonObject:
    normalized: JsonObject = {}
    alias_map = {
        "root_cause": "rootCause",
        "root cause": "rootCause",
        "rootcause": "rootCause",
        "evidence_refs": "evidenceRefs",
        "evidence refs": "evidenceRefs",
        "instance_id": "instanceId",
        "instance id": "instanceId",
        "node_id": "nodeId",
        "node id": "nodeId",
    }
    for key, value in payload.items():
        canonical = alias_map.get(str(key).strip().lower(), key)
        if canonical in (
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
            text = optional_string(value)
            if text is not None:
                normalized[canonical] = text
        elif canonical == "evidenceRefs":
            normalized[canonical] = normalize_string_list(value)
        elif canonical == "enabled":
            normalized[canonical] = bool(value)
    return normalized


def validate_case_draft(draft: JsonObject) -> list[str]:
    errors = []
    for field in CASE_REQUIRED_FIELDS:
        value = draft.get(field)
        if not isinstance(value, str) or not value.strip():
            errors.append(f"{field} is required")
    return errors


def split_evidence_refs(value: str) -> list[str]:
    parts = re.split(r"[\n,，]+", value)
    return normalize_string_list([part.strip() for part in parts])


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
                    "limit": {"type": "integer", "minimum": 1, "maximum": 50},
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


def task_case_tool_descriptors() -> list[JsonObject]:
    return [
        {
            "name": "logagent.recall_cases",
            "description": "Recall active enabled cases from LogAgent memory.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {"type": "string", "minLength": 1},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 20},
                },
                "required": ["query"],
                "additionalProperties": False,
            },
        },
        *case_tool_descriptors(),
    ]


def call_case_tool(
    settings: Settings | None,
    store: Store,
    run: JsonObject | None,
    name: str,
    arguments: JsonObject,
) -> JsonObject:
    if name in {"logagent.search_cases", "logagent.recall_cases"}:
        query = optional_string(arguments.get("query"))
        if name == "logagent.recall_cases" and query is None:
            raise ValueError("query is required")
        limit = case_tool_limit(
            arguments.get("limit"),
            maximum=20 if name == "logagent.recall_cases" else 50,
        )
        value = {
            "cases": store.search_cases(
                query=query,
                limit=limit,
                include_disabled=False
                if name == "logagent.recall_cases"
                else bool(arguments.get("includeDisabled", False)),
            ),
            "finalEvidenceAllowed": False,
        }
        value["caseCount"] = len(value["cases"])
        if name == "logagent.recall_cases":
            artifact_path = f"case_recall/recall_{stable_case_digest(arguments)}.json"
            value["artifactPath"] = artifact_path
            value["backgroundRef"] = f"{artifact_path}#cases"
            value["evidenceRefs"] = [
                f"{artifact_path}#cases/{index}" for index, _ in enumerate(value["cases"])
            ]
    elif name == "logagent.get_case":
        value = {"case": store.get_case(require_string(arguments, "caseId"))}
    else:
        raise ValueError(f"unsupported case tool {name}")
    if settings is not None and run is not None:
        persist_case_context(settings, store, run, name, value)
    return value


def case_tool_limit(value: object, *, maximum: int) -> int:
    if value is None:
        return 5
    if isinstance(value, bool) or not isinstance(value, int):
        raise ValueError("case search limit must be an integer")
    return max(1, min(value, maximum))


def persist_case_context(
    settings: Settings,
    store: Store,
    run: JsonObject,
    tool_name: str,
    value: JsonObject,
) -> None:
    data = json.dumps(value, ensure_ascii=True, indent=2).encode("utf-8")
    filename = str(value.get("artifactPath") or "").rsplit("/", 1)[-1]
    if not filename:
        filename = f"{tool_name.removeprefix('logagent.').replace('.', '_')}.json"
    artifact = write_artifact_bytes(
        settings=settings,
        store=store,
        workspace_id=run["workspace_id"],
        filename=filename,
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
        payload={
            "artifactId": artifact["id"],
            "tool": tool_name,
            "path": value.get("artifactPath"),
            "backgroundRef": value.get("backgroundRef"),
        },
    )


def stable_case_digest(value: JsonObject) -> str:
    data = json.dumps(value, ensure_ascii=True, sort_keys=True, separators=(",", ":"))
    return sha256(data.encode("utf-8")).hexdigest()[:16]


def require_string(arguments: JsonObject, field: str) -> str:
    value = arguments.get(field)
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"{field} is required")
    return value.strip()
