from __future__ import annotations

import json
import re
from typing import Any

from .artifacts import resolve_artifact_path
from .config import Settings
from .store import JsonObject, Store


CONFIDENCE_VALUES = {"low", "medium", "high"}
LOG_MATCH_RE = re.compile(
    r"^(grep_results\.json|log_searches/[A-Za-z0-9_-]+\.json)#matches/(\d+)$"
)
LOG_SLICE_RE = re.compile(r"^(log_slices/[A-Za-z0-9_-]+\.json)#lines$")
TOOL_FINDING_RE = re.compile(
    r"^(tool_results/[A-Za-z0-9_.-]+/result\.json)#findings/(\d+)$"
)
FETCH_RESPONSE_RE = re.compile(r"^(tool_results/[A-Za-z0-9_.-]+/result\.json)#response$")
CASE_CONTEXT_RE = re.compile(r"^case_context\.json#cases/(\d+)$")
CASE_ID_RE = re.compile(r"(case_[A-Za-z0-9_]+)")
CODE_EVIDENCE_RE = re.compile(r"^(code_evidence/[A-Za-z0-9_-]+\.json)#matches/(\d+)$")
SESSION_TEXT_INPUT_REF = "session_text_input.json#question"


class FinalAnswerValidationError(ValueError):
    pass


def normalize_and_validate_final_answer(
    settings: Settings,
    store: Store,
    run_id: str,
    final_answer: JsonObject,
) -> JsonObject:
    normalized = normalize_final_answer(final_answer)
    normalized = normalize_case_refs(settings, store, run_id, normalized)
    validate_evidence_refs(settings, store, run_id, collect_evidence_refs(normalized))
    return normalized


def normalize_final_answer(value: Any) -> JsonObject:
    if not isinstance(value, dict):
        raise FinalAnswerValidationError("final answer must be an object")

    summary = value.get("summary")
    if not isinstance(summary, str) or not summary.strip():
        raise FinalAnswerValidationError("final answer summary must be a non-empty string")

    confidence = value.get("confidence")
    if confidence not in CONFIDENCE_VALUES:
        raise FinalAnswerValidationError("final answer confidence must be low, medium, or high")

    normalized = dict(value)
    normalized["summary"] = summary.strip()
    normalized["confidence"] = confidence
    for field in ("symptoms", "nextChecks", "fixSuggestions", "missingInformation"):
        normalized[field] = normalize_string_list(value.get(field, []), field)
    normalized["evidenceRefs"] = normalize_string_list(
        value.get("evidenceRefs", []), "evidenceRefs"
    )
    normalized["likelyRootCauses"] = normalize_root_causes(value.get("likelyRootCauses", []))
    return normalized


def normalize_string_list(value: Any, field: str) -> list[str]:
    if value is None:
        return []
    if isinstance(value, str):
        text = value.strip()
        return [text] if text else []
    if not isinstance(value, list):
        raise FinalAnswerValidationError(f"{field} must be an array of strings")
    result: list[str] = []
    for item in value:
        if not isinstance(item, str):
            raise FinalAnswerValidationError(f"{field} must contain only strings")
        text = item.strip()
        if text:
            result.append(text)
    return result


def normalize_root_causes(value: Any) -> list[JsonObject]:
    if value is None:
        return []
    if not isinstance(value, list):
        raise FinalAnswerValidationError("likelyRootCauses must be an array")
    result: list[JsonObject] = []
    for index, item in enumerate(value):
        if not isinstance(item, dict):
            raise FinalAnswerValidationError(f"likelyRootCauses[{index}] must be an object")
        cause = item.get("cause")
        if not isinstance(cause, str) or not cause.strip():
            raise FinalAnswerValidationError(
                f"likelyRootCauses[{index}].cause must be a non-empty string"
            )
        normalized = dict(item)
        normalized["cause"] = cause.strip()
        normalized["evidenceRefs"] = normalize_string_list(
            item.get("evidenceRefs", []), f"likelyRootCauses[{index}].evidenceRefs"
        )
        result.append(normalized)
    return result


def collect_evidence_refs(final_answer: JsonObject) -> list[str]:
    refs = list(final_answer.get("evidenceRefs", []))
    for root_cause in final_answer.get("likelyRootCauses", []):
        refs.extend(root_cause.get("evidenceRefs", []))
    return list(dict.fromkeys(refs))


def validate_evidence_refs(
    settings: Settings,
    store: Store,
    run_id: str,
    refs: list[str],
) -> None:
    if not refs:
        return
    evidence_items = store.list_evidence(run_id)
    for ref in refs:
        if not is_valid_ref(settings, store, evidence_items, ref):
            raise FinalAnswerValidationError(f"invalid or unsupported final evidence ref: {ref}")


def is_valid_ref(
    settings: Settings,
    store: Store,
    evidence_items: list[JsonObject],
    ref: str,
) -> bool:
    if not isinstance(ref, str) or not ref.strip():
        return False

    if ref == SESSION_TEXT_INPUT_REF:
        return any(
            item["kind"] == "user_question"
            and item["final_allowed"]
            and item["payload"].get("ref") == SESSION_TEXT_INPUT_REF
            and artifact_question_exists(settings, store, item)
            for item in evidence_items
        )

    case_context = CASE_CONTEXT_RE.match(ref)
    if case_context:
        index = int(case_context.group(1))
        return latest_case_exists(settings, store, evidence_items, index)

    log_match = LOG_MATCH_RE.match(ref)
    if log_match:
        path = log_match.group(1)
        index = int(log_match.group(2))
        return any(
            item["kind"] == "log_search"
            and item["final_allowed"]
            and item["payload"].get("path") == path
            and artifact_match_exists(settings, store, item, "matches", index)
            for item in evidence_items
        )

    log_slice = LOG_SLICE_RE.match(ref)
    if log_slice:
        path = log_slice.group(1)
        return any(
            item["kind"] == "log_slice"
            and item["final_allowed"]
            and item["payload"].get("path") == path
            and artifact_ref_exists(settings, store, item, "ref", ref)
            for item in evidence_items
        )

    tool_finding = TOOL_FINDING_RE.match(ref)
    if tool_finding:
        prefix = f"{tool_finding.group(1)}#findings/"
        index = int(tool_finding.group(2))
        return any(
            item["kind"] == "tool_result"
            and item["final_allowed"]
            and item["payload"].get("evidenceRefPrefix") == prefix
            and artifact_match_exists(settings, store, item, "findings", index)
            for item in evidence_items
        )

    fetch_response = FETCH_RESPONSE_RE.match(ref)
    if fetch_response:
        return any(
            item["kind"] == "fetch_result"
            and item["final_allowed"]
            and item["payload"].get("ref") == ref
            and artifact_response_exists(settings, store, item)
            for item in evidence_items
        )

    code_evidence = CODE_EVIDENCE_RE.match(ref)
    if code_evidence:
        path = code_evidence.group(1)
        index = int(code_evidence.group(2))
        return any(
            item["kind"] == "code_evidence"
            and item["final_allowed"]
            and item["payload"].get("path") == path
            and artifact_match_exists(settings, store, item, "matches", index)
            for item in evidence_items
        )

    return False


def normalize_case_refs(
    settings: Settings,
    store: Store,
    run_id: str,
    final_answer: JsonObject,
) -> JsonObject:
    value = dict(final_answer)
    value["evidenceRefs"] = [
        normalize_case_ref(settings, store, run_id, ref)
        for ref in value.get("evidenceRefs", [])
    ]
    root_causes = []
    for root_cause in value.get("likelyRootCauses", []):
        if not isinstance(root_cause, dict):
            root_causes.append(root_cause)
            continue
        normalized = dict(root_cause)
        normalized["evidenceRefs"] = [
            normalize_case_ref(settings, store, run_id, ref)
            for ref in normalized.get("evidenceRefs", [])
        ]
        root_causes.append(normalized)
    value["likelyRootCauses"] = root_causes
    return value


def normalize_case_ref(
    settings: Settings,
    store: Store,
    run_id: str,
    ref: str,
) -> str:
    if CASE_CONTEXT_RE.match(ref):
        return ref
    match = CASE_ID_RE.search(ref)
    if not match:
        return ref
    index = find_case_index(settings, store, run_id, match.group(1))
    return f"case_context.json#cases/{index}" if index is not None else ref


def find_case_index(
    settings: Settings,
    store: Store,
    run_id: str,
    case_id: str,
) -> int | None:
    for evidence in reversed(store.list_evidence(run_id)):
        if evidence["kind"] != "case_context" or not evidence.get("artifact_id"):
            continue
        cases = read_evidence_artifact(settings, store, evidence).get("cases")
        if not isinstance(cases, list):
            continue
        for index, item in enumerate(cases):
            if isinstance(item, dict) and item.get("caseId") == case_id:
                return index
    return None


def artifact_match_exists(
    settings: Settings,
    store: Store,
    evidence: JsonObject,
    field: str,
    index: int,
) -> bool:
    value = read_evidence_artifact(settings, store, evidence).get(field)
    return isinstance(value, list) and 0 <= index < len(value)


def artifact_ref_exists(
    settings: Settings,
    store: Store,
    evidence: JsonObject,
    field: str,
    ref: str,
) -> bool:
    return read_evidence_artifact(settings, store, evidence).get(field) == ref


def artifact_response_exists(settings: Settings, store: Store, evidence: JsonObject) -> bool:
    return isinstance(read_evidence_artifact(settings, store, evidence).get("response"), dict)


def artifact_case_exists(
    settings: Settings,
    store: Store,
    evidence: JsonObject,
    index: int,
) -> bool:
    cases = read_evidence_artifact(settings, store, evidence).get("cases")
    if not isinstance(cases, list) or not 0 <= index < len(cases):
        return False
    item = cases[index]
    if not isinstance(item, dict):
        return False
    return item.get("enabled", True) is not False


def latest_case_exists(
    settings: Settings,
    store: Store,
    evidence_items: list[JsonObject],
    index: int,
) -> bool:
    for evidence in reversed(evidence_items):
        if evidence["kind"] == "case_context":
            return artifact_case_exists(settings, store, evidence, index)
    return False


def artifact_question_exists(settings: Settings, store: Store, evidence: JsonObject) -> bool:
    question = read_evidence_artifact(settings, store, evidence).get("question")
    return isinstance(question, str) and bool(question.strip())


def read_evidence_artifact(settings: Settings, store: Store, evidence: JsonObject) -> JsonObject:
    artifact_id = evidence.get("artifact_id")
    if not artifact_id:
        return {}
    artifact = store.get_artifact(artifact_id)
    path = resolve_artifact_path(settings, artifact["relative_path"])
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except Exception:
        return {}
    return value if isinstance(value, dict) else {}
