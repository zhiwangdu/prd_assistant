from __future__ import annotations

import json
import subprocess
from fnmatch import fnmatchcase
from hashlib import sha256
from pathlib import Path

from .artifacts import resolve_artifact_path, write_artifact_bytes
from .config import Settings, ToolDefinition
from .evidence import TextFile, collect_text_files
from .store import JsonObject, Store


def tool_descriptors(settings: Settings) -> list[JsonObject]:
    return [
        {
            "toolId": tool.id,
            "displayName": tool.display_name,
            "enabled": tool.enabled,
            "backend": "subprocess",
            "readOnly": True,
            "editable": False,
            "runnable": tool.enabled,
            "maxInputFiles": tool.max_input_files,
            "match": {
                "filePatterns": list(tool.match_file_patterns),
                "keywords": list(tool.match_keywords),
            },
            "paramsSchema": {
                "type": "object",
                "properties": {
                    "toolId": {"const": tool.id},
                    "inputFiles": {
                        "type": "array",
                        "items": {"type": "string"},
                    },
                },
                "required": ["toolId"],
                "additionalProperties": False,
            },
        }
        for tool in settings.tools
    ]


def get_tool(settings: Settings, tool_id: str) -> ToolDefinition:
    for tool in settings.tools:
        if tool.id == tool_id:
            return tool
    raise ValueError(f"unknown tool {tool_id}")


def run_configured_tool(
    settings: Settings,
    store: Store,
    workspace_id: str,
    run_id: str,
    tool_id: str,
) -> JsonObject:
    tool = get_tool(settings, tool_id)
    if not tool.enabled:
        raise ValueError(f"tool {tool_id} is disabled")
    input_entries = select_tool_inputs(settings, store, workspace_id, run_id, tool)
    if tool_requires_input(tool) and not input_entries:
        raise ValueError(f"tool {tool_id} requires an input_file but no tool input matched")
    if input_entries:
        runs = [
            run_single_configured_tool(settings, store, workspace_id, run_id, tool, entry)
            for entry in input_entries[: tool.max_input_files]
        ]
        primary = runs[0]
        if len(runs) == 1:
            return primary
        return {
            **primary,
            "results": [item["result"] for item in runs],
            "artifacts": [item["artifact"] for item in runs],
            "evidenceItems": [item["evidence"] for item in runs],
        }
    return run_single_configured_tool(settings, store, workspace_id, run_id, tool, None)


def run_single_configured_tool(
    settings: Settings,
    store: Store,
    workspace_id: str,
    run_id: str,
    tool: ToolDefinition,
    input_entry: JsonObject | None,
) -> JsonObject:
    command = Path(tool.command)
    if not command.is_absolute():
        raise ValueError(f"tool {tool.id} command must be an absolute path")
    input_file = resolve_tool_input_path(settings, store, input_entry) if input_entry else None
    action_id = tool_action_id(tool, input_entry)
    argv = [str(command), *format_tool_args(tool.args, input_file, action_id)]
    try:
        completed = subprocess.run(
            argv,
            check=False,
            capture_output=True,
            timeout=tool.timeout_seconds,
        )
        timed_out = False
    except subprocess.TimeoutExpired as error:
        completed = error
        timed_out = True

    stdout = (completed.stdout or b"")[: tool.max_output_bytes]
    stderr = (completed.stderr or b"")[: tool.max_output_bytes]
    parsed_stdout = parse_json(stdout)
    result = {
        "schemaVersion": 1,
        "toolId": tool.id,
        "displayName": tool.display_name,
        "actionId": action_id,
        "inputFile": input_entry.get("path") if input_entry else None,
        "inputKind": input_entry.get("inputKind") if input_entry else None,
        "argv": argv,
        "timedOut": timed_out,
        "exitCode": None if timed_out else int(completed.returncode),
        "stdoutPreview": stdout.decode("utf-8", errors="replace"),
        "stderrPreview": stderr.decode("utf-8", errors="replace"),
        "parsedStdout": parsed_stdout,
        "summary": summary_from_stdout(parsed_stdout, stdout, timed_out),
        "findings": findings_from_stdout(parsed_stdout),
    }
    artifact = write_artifact_bytes(
        settings=settings,
        store=store,
        workspace_id=workspace_id,
        filename=f"{action_id}_result.json",
        data=json.dumps(result, ensure_ascii=True, indent=2).encode("utf-8"),
        content_type="application/json",
        schema_name="logagent.v2.tool_result.v1",
        preview={
            "toolId": tool.id,
            "exitCode": result["exitCode"],
            "timedOut": timed_out,
            "findingCount": len(result["findings"]),
        },
    )
    evidence = store.create_evidence(
        workspace_id=workspace_id,
        run_id=run_id,
        kind="tool_result",
        final_allowed=True,
        summary=f"Tool {tool.id} completed with exitCode={result['exitCode']}.",
        artifact_id=artifact["id"],
        payload={
            "artifactId": artifact["id"],
            "toolId": tool.id,
            "actionId": action_id,
            "inputFile": result["inputFile"],
            "exitCode": result["exitCode"],
            "timedOut": timed_out,
            "findingCount": len(result["findings"]),
            "evidenceRefPrefix": f"tool_results/{action_id}/result.json#findings/",
        },
    )
    return {"result": result, "artifact": artifact, "evidence": evidence}


def select_tool_inputs(
    settings: Settings,
    store: Store,
    workspace_id: str,
    run_id: str,
    tool: ToolDefinition,
) -> list[JsonObject]:
    if not tool_requires_input(tool):
        return []
    index = latest_tool_input_index(settings, store, run_id)
    inputs = index.get("inputs") if isinstance(index, dict) else None
    selected = []
    if isinstance(inputs, list):
        for entry in inputs:
            if not isinstance(entry, dict):
                continue
            tool_ids = entry.get("toolIds")
            if not isinstance(tool_ids, list) or tool.id not in tool_ids:
                continue
            if not isinstance(entry.get("artifactId"), str):
                continue
            selected.append(entry)
            if len(selected) >= tool.max_input_files:
                break
    if selected:
        return selected
    return select_fallback_tool_inputs(settings, store, workspace_id, run_id, tool)


def select_fallback_tool_inputs(
    settings: Settings,
    store: Store,
    workspace_id: str,
    run_id: str,
    tool: ToolDefinition,
) -> list[JsonObject]:
    uploads = store.list_uploads(workspace_id)
    text_files = collect_text_files(settings, uploads)
    files_by_path = {text_file.path: text_file for text_file in text_files}
    selected_paths: list[str] = []

    for text_file in text_files:
        if len(selected_paths) >= tool.max_input_files:
            break
        if any(
            matches_file_pattern(pattern, text_file.path, text_file.source_filename)
            for pattern in tool.match_file_patterns
        ):
            push_selected_path(selected_paths, text_file.path)

    if len(selected_paths) < tool.max_input_files and tool.match_keywords:
        grep_results = latest_initial_grep_results(settings, store, run_id)
        for match in grep_results.get("matches", []):
            if len(selected_paths) >= tool.max_input_files:
                break
            if not isinstance(match, dict):
                continue
            path = match.get("path")
            text = match.get("text")
            if not isinstance(path, str) or path not in files_by_path:
                continue
            if not isinstance(text, str) or not matches_any_keyword(text, tool.match_keywords):
                continue
            push_selected_path(selected_paths, path)

    return [
        materialize_fallback_tool_input(settings, store, workspace_id, tool, files_by_path[path])
        for path in selected_paths[: tool.max_input_files]
    ]


def latest_tool_input_index(settings: Settings, store: Store, run_id: str) -> JsonObject:
    candidates = [
        item for item in store.list_evidence(run_id) if item["kind"] == "tool_input_index"
    ]
    if not candidates:
        return {}
    artifact_id = candidates[-1].get("artifact_id")
    if not artifact_id:
        return {}
    artifact = store.get_artifact(artifact_id)
    path = resolve_artifact_path(settings, artifact["relative_path"])
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except Exception:
        return {}
    return value if isinstance(value, dict) else {}


def latest_initial_grep_results(settings: Settings, store: Store, run_id: str) -> JsonObject:
    candidates = [
        item
        for item in store.list_evidence(run_id)
        if item["kind"] == "log_search" and item["payload"].get("path") == "grep_results.json"
    ]
    if not candidates:
        return {}
    artifact_id = candidates[-1].get("artifact_id")
    if not artifact_id:
        return {}
    artifact = store.get_artifact(artifact_id)
    path = resolve_artifact_path(settings, artifact["relative_path"])
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except Exception:
        return {}
    return value if isinstance(value, dict) else {}


def materialize_fallback_tool_input(
    settings: Settings,
    store: Store,
    workspace_id: str,
    tool: ToolDefinition,
    text_file: TextFile,
) -> JsonObject:
    virtual_path = fallback_virtual_path(text_file.path)
    data = text_file.text.encode("utf-8")
    digest = sha256(virtual_path.encode("utf-8")).hexdigest()[:12]
    artifact = write_artifact_bytes(
        settings=settings,
        store=store,
        workspace_id=workspace_id,
        filename=f"{safe_action_segment(tool.id)}_{digest}.txt",
        data=data,
        content_type="text/plain",
        schema_name="logagent.v2.tool_input.text_file.v1",
        preview={
            "path": virtual_path,
            "toolId": tool.id,
            "sourcePath": text_file.path,
            "sizeBytes": len(data),
        },
    )
    return {
        "path": virtual_path,
        "inputKind": "text_file",
        "scope": "manifest_fallback",
        "toolIds": [tool.id],
        "sourceFiles": [text_file.path],
        "sourceUploadId": text_file.source_upload_id,
        "sourceFilename": text_file.source_filename,
        "lineCount": len(text_file.text.splitlines()),
        "artifactId": artifact["id"],
        "artifactRelativePath": artifact["relative_path"],
    }


def fallback_virtual_path(manifest_path: str) -> str:
    if manifest_path.startswith("extracted/") or manifest_path.startswith("tool_inputs/"):
        return manifest_path
    return f"extracted/{manifest_path}"


def matches_file_pattern(pattern: str, path: str, source_filename: str) -> bool:
    lowered_pattern = pattern.lower()
    lowered_path = path.lower()
    lowered_name = Path(path).name.lower()
    lowered_source = source_filename.lower()
    return (
        lowered_pattern == "*"
        or fnmatchcase(lowered_path, lowered_pattern)
        or fnmatchcase(lowered_name, lowered_pattern)
        or fnmatchcase(lowered_source, lowered_pattern)
    )


def matches_any_keyword(text: str, keywords: tuple[str, ...]) -> bool:
    lowered = text.lower()
    return any(keyword.lower() in lowered for keyword in keywords)


def push_selected_path(selected_paths: list[str], path: str) -> None:
    if path not in selected_paths:
        selected_paths.append(path)


def resolve_tool_input_path(
    settings: Settings, store: Store, input_entry: JsonObject
) -> Path:
    artifact = store.get_artifact(str(input_entry["artifactId"]))
    path = resolve_artifact_path(settings, artifact["relative_path"])
    if not path.is_file():
        raise ValueError(f"tool input artifact is missing for {input_entry.get('path')}")
    return path


def format_tool_args(args: tuple[str, ...], input_file: Path | None, action_id: str) -> list[str]:
    result = []
    for arg in args:
        if "{input_file}" in arg and input_file is None:
            raise ValueError("tool arg requires {input_file} but no input file is selected")
        formatted = arg.replace("{input_file}", str(input_file) if input_file else "")
        formatted = formatted.replace("{action_id}", action_id)
        result.append(formatted)
    return result


def tool_requires_input(tool: ToolDefinition) -> bool:
    return any("{input_file}" in arg for arg in tool.args)


def tool_action_id(tool: ToolDefinition, input_entry: JsonObject | None) -> str:
    if input_entry is None:
        return tool.id
    digest = sha256(str(input_entry.get("path", "")).encode("utf-8")).hexdigest()[:12]
    return f"{safe_action_segment(tool.id)}_{digest}"


def safe_action_segment(value: str) -> str:
    result = "".join(char if char.isalnum() or char in "._-" else "_" for char in value)
    return result[:80] or "tool"


def parse_json(data: bytes) -> object | None:
    if not data.strip():
        return None
    try:
        value = json.loads(data.decode("utf-8"))
    except Exception:
        return None
    return value


def summary_from_stdout(parsed: object | None, stdout: bytes, timed_out: bool) -> str:
    if timed_out:
        return "Tool timed out."
    if isinstance(parsed, dict):
        if is_influxql_report(parsed):
            return influxql_report_summary(parsed)
        if is_influxql_compare_report(parsed):
            return influxql_compare_summary(parsed)
        summary = string_field(parsed, ("summary", "message", "title"))
        if summary:
            return summary
    if isinstance(parsed, str) and parsed.strip():
        return parsed.strip()[:500]
    preview = stdout.decode("utf-8", errors="replace").strip()
    return preview[:500] if preview else "Tool produced no stdout."


def findings_from_stdout(parsed: object | None) -> list[JsonObject]:
    if not parsed:
        return []
    if isinstance(parsed, dict):
        if is_influxql_report(parsed):
            return influxql_report_findings(parsed)
        if is_influxql_compare_report(parsed):
            return influxql_compare_findings(parsed)
        for key in ("findings", "issues", "diagnostics"):
            findings = parsed.get(key)
            if isinstance(findings, list):
                return parse_findings_value(findings)
        return []
    if isinstance(parsed, list):
        return parse_findings_value(parsed)
    return []


def is_influxql_report(value: JsonObject) -> bool:
    return (
        "total_records" in value
        and "total_statements" in value
        and isinstance(value.get("fingerprints"), list)
    )


def is_influxql_compare_report(value: JsonObject) -> bool:
    return "batch_a" in value and "batch_b" in value and "statement_delta" in value


def influxql_report_summary(value: JsonObject) -> str:
    total_records = int_field(value, "total_records") or 0
    records_in_window = int_field(value, "records_in_window") or 0
    total_statements = int_field(value, "total_statements") or 0
    parse_errors = int_field(value, "parse_error_count") or 0
    summary = (
        "influxql report: "
        f"records={total_records}, recordsInWindow={records_in_window}, "
        f"statements={total_statements}, parseErrors={parse_errors}"
    )
    rule_summary = influxql_rule_summary(value)
    if rule_summary:
        summary += f", specialRules={rule_summary}"
    return summary


def influxql_compare_summary(value: JsonObject) -> str:
    statement_delta = number_to_string(value.get("statement_delta")) or "0"
    qps_delta = number_to_string(value.get("qps_delta")) or "0"
    return (
        "influxql compare report: "
        f"statementDelta={statement_delta}, qpsDelta={qps_delta}, "
        f"batchA={compare_batch_summary(value.get('batch_a'))}, "
        f"batchB={compare_batch_summary(value.get('batch_b'))}"
    )


def compare_batch_summary(value: object) -> str:
    if not isinstance(value, dict):
        return "unknown"
    statements = number_to_string(value.get("total_statements")) or "0"
    parse_errors = number_to_string(value.get("parse_error_count")) or "0"
    qps = number_to_string(value.get("qps")) or "0"
    duration = number_to_string(value.get("effective_duration_seconds")) or "0"
    return (
        f"statements={statements}, parseErrors={parse_errors}, "
        f"qps={qps}, durationSeconds={duration}"
    )


def influxql_report_findings(value: JsonObject) -> list[JsonObject]:
    findings: list[JsonObject] = []
    findings.extend(influxql_special_rule_findings(value))
    findings.extend(influxql_parse_error_findings(value))
    findings.extend(influxql_realtime_findings(value))
    findings.extend(influxql_fingerprint_findings(value))
    return findings


def influxql_compare_findings(value: JsonObject) -> list[JsonObject]:
    findings: list[JsonObject] = []
    findings.extend(compare_fingerprint_findings(value, "new_fingerprints", "new fingerprint"))
    findings.extend(
        compare_fingerprint_findings(value, "removed_fingerprints", "removed fingerprint")
    )
    findings.extend(
        compare_fingerprint_findings(value, "changed_fingerprints", "changed fingerprint")
    )
    findings.extend(compare_rule_delta_findings(value))
    return findings


def influxql_rule_summary(value: JsonObject) -> str:
    rules = value.get("special_rules")
    if not isinstance(rules, list):
        return ""
    parts = []
    for item in rules[:8]:
        if not isinstance(item, dict):
            continue
        name = string_field(item, ("rule",))
        if not name:
            continue
        count = number_to_string(item.get("count")) or "0"
        parts.append(f"{name}:{count}")
    return ", ".join(parts)


def influxql_special_rule_findings(value: JsonObject) -> list[JsonObject]:
    rules = value.get("special_rules")
    if not isinstance(rules, list):
        return []
    findings = []
    for item in rules[:12]:
        if not isinstance(item, dict):
            continue
        name = string_field(item, ("rule",))
        if not name:
            continue
        count = number_to_string(item.get("count")) or "0"
        fingerprints = item.get("fingerprints")
        fingerprint_count = len(fingerprints) if isinstance(fingerprints, list) else 0
        findings.append(
            {
                "severity": influxql_rule_severity(name),
                "message": (
                    f"rule {name} matched {count} statement(s) across "
                    f"{fingerprint_count} fingerprint(s): {influxql_rule_description(name)}"
                ),
            }
        )
    return findings


def influxql_parse_error_findings(value: JsonObject) -> list[JsonObject]:
    errors = value.get("parse_errors")
    if not isinstance(errors, list):
        return []
    findings = []
    for item in errors[:5]:
        if not isinstance(item, dict):
            continue
        message = string_field(item, ("error",))
        if not message:
            continue
        count = number_to_string(item.get("count")) or "0"
        findings.append(
            {
                "severity": "high",
                "message": f"parse error occurred {count} time(s): {message}",
            }
        )
    return findings


def influxql_realtime_findings(value: JsonObject) -> list[JsonObject]:
    realtime = value.get("realtime_query")
    if not isinstance(realtime, dict):
        return []
    total = int_field(realtime, "total") or 0
    if total <= 0:
        return []
    non_realtime = int_field(realtime, "non_realtime") or 0
    unknown = int_field(realtime, "unknown") or 0
    realtime_count = int_field(realtime, "realtime") or 0
    findings = []
    if non_realtime > 0:
        findings.append(
            {
                "severity": "medium",
                "message": (
                    "realtime query classification found "
                    f"{non_realtime}/{total} non-realtime select-like statement(s)"
                ),
            }
        )
    if unknown > 0:
        reason = first_realtime_sample_reason(realtime, "sample_unknown")
        reason_text = f"; sample reason: {reason}" if reason else ""
        findings.append(
            {
                "severity": "low",
                "message": (
                    "realtime query classification is unknown for "
                    f"{unknown}/{total} select-like statement(s); "
                    f"realtime={realtime_count}{reason_text}"
                ),
            }
        )
    return findings


def influxql_fingerprint_findings(value: JsonObject) -> list[JsonObject]:
    fingerprints = value.get("fingerprints")
    if not isinstance(fingerprints, list):
        return []
    findings = []
    for item in fingerprints[:5]:
        if not isinstance(item, dict):
            continue
        count = int_field(item, "count") or 0
        rules = string_list(item.get("rules"))
        if count <= 1 and not rules:
            continue
        statement_type = string_field(item, ("statement_type",)) or ""
        normalized = string_field(item, ("normalized_query",)) or ""
        rule_text = ", ".join(rules) if rules else "none"
        findings.append(
            {
                "severity": "low",
                "message": (
                    f"fingerprint {statement_type} occurred {count} time(s), "
                    f"rules=[{rule_text}], normalized={normalized}"
                ),
            }
        )
    return findings


def compare_fingerprint_findings(value: JsonObject, key: str, label: str) -> list[JsonObject]:
    items = value.get(key)
    if not isinstance(items, list):
        return []
    findings = []
    for item in items[:8]:
        if not isinstance(item, dict):
            continue
        status = string_field(item, ("status",)) or label
        statement_type = string_field(item, ("statement_type",)) or "unknown"
        normalized = (
            string_field(item, ("normalized_query",))
            or string_field(item, ("fingerprint",))
            or "unknown"
        )
        count_a = number_to_string(item.get("count_a")) or "0"
        count_b = number_to_string(item.get("count_b")) or "0"
        count_delta = number_to_string(item.get("count_delta")) or "0"
        qps_a = number_to_string(item.get("qps_a")) or "0"
        qps_b = number_to_string(item.get("qps_b")) or "0"
        qps_delta = number_to_string(item.get("qps_delta")) or "0"
        rules = ",".join(string_list(item.get("rules"))) or "-"
        findings.append(
            {
                "severity": compare_fingerprint_severity(key, count_delta),
                "message": (
                    f"{label}: status={status}, statementType={statement_type}, "
                    f"count={count_a}->{count_b} (delta={count_delta}), "
                    f"qps={qps_a}->{qps_b} (delta={qps_delta}), "
                    f"rules={rules}, query={normalized}"
                ),
            }
        )
    return findings


def compare_rule_delta_findings(value: JsonObject) -> list[JsonObject]:
    items = value.get("rule_deltas")
    if not isinstance(items, list):
        return []
    findings = []
    for item in items[:8]:
        if not isinstance(item, dict):
            continue
        rule = string_field(item, ("rule",)) or "unknown"
        count_a = number_to_string(item.get("count_a")) or "0"
        count_b = number_to_string(item.get("count_b")) or "0"
        count_delta = number_to_string(item.get("count_delta")) or "0"
        qps_a = number_to_string(item.get("qps_a")) or "0"
        qps_b = number_to_string(item.get("qps_b")) or "0"
        qps_delta = number_to_string(item.get("qps_delta")) or "0"
        findings.append(
            {
                "severity": compare_delta_severity(count_delta),
                "message": (
                    f"rule delta: rule={rule}, count={count_a}->{count_b} "
                    f"(delta={count_delta}), qps={qps_a}->{qps_b} "
                    f"(delta={qps_delta})"
                ),
            }
        )
    return findings


def first_realtime_sample_reason(value: JsonObject, key: str) -> str | None:
    samples = value.get(key)
    if not isinstance(samples, list) or not samples:
        return None
    first = samples[0]
    if not isinstance(first, dict):
        return None
    return string_field(first, ("reason",))


def influxql_rule_severity(rule: str) -> str:
    if rule in {"write_or_destructive", "large_limit", "no_time_filter"}:
        return "high"
    if rule in {"group_by_high_cardinality_risk", "not_realtime_query"}:
        return "medium"
    return "low"


def influxql_rule_description(rule: str) -> str:
    descriptions = {
        "no_time_filter": "SELECT has no explicit time predicate",
        "has_regex": "query uses regex matching or regex measurement/source",
        "has_wildcard": "query uses wildcard selection, grouping, or metadata scope",
        "large_limit": "LIMIT or SLIMIT is greater than or equal to the configured threshold",
        "group_by_high_cardinality_risk": (
            "non-time GROUP BY dimensions exceed the configured threshold"
        ),
        "meta_query": "metadata or explain query",
        "write_or_destructive": "query writes data or performs destructive changes",
        "not_realtime_query": "select-like query is explicitly non-realtime",
    }
    return descriptions.get(rule, "unrecognized analyzer rule")


def compare_fingerprint_severity(key: str, count_delta: str) -> str:
    if key == "removed_fingerprints":
        return "low"
    if key == "new_fingerprints":
        return "high"
    return compare_delta_severity(count_delta)


def compare_delta_severity(count_delta: str) -> str:
    try:
        value = float(count_delta.strip())
    except ValueError:
        return "medium"
    if value > 0:
        return "high"
    if value < 0:
        return "low"
    return "medium"


def parse_findings_value(items: list[object]) -> list[JsonObject]:
    findings = []
    for item in items:
        finding = parse_finding_value(item)
        if finding:
            findings.append(finding)
    return findings


def parse_finding_value(value: object) -> JsonObject | None:
    if isinstance(value, str) and value.strip():
        return {"message": value.strip()}
    if not isinstance(value, dict):
        return None
    message = string_field(
        value,
        ("message", "summary", "description", "detail", "title", "cause"),
    )
    if not message:
        return None
    finding: JsonObject = {"message": message}
    severity = string_field(value, ("severity", "level", "status"))
    if severity:
        finding["severity"] = severity
    file = string_field(value, ("file", "path", "filename"))
    if file:
        finding["file"] = file
    line = number_field(value, ("line", "lineNumber", "startLine"))
    if line is not None:
        finding["line"] = line
    return finding


def string_field(value: JsonObject, keys: tuple[str, ...]) -> str | None:
    for key in keys:
        item = value.get(key)
        if isinstance(item, str) and item.strip():
            return item.strip()
    return None


def number_field(value: JsonObject, keys: tuple[str, ...]) -> int | None:
    for key in keys:
        item = value.get(key)
        if isinstance(item, int):
            return item
        if isinstance(item, float):
            return int(item)
        if isinstance(item, str):
            try:
                return int(item.strip())
            except ValueError:
                continue
    return None


def int_field(value: JsonObject, key: str) -> int | None:
    return number_field(value, (key,))


def number_to_string(value: object) -> str | None:
    if isinstance(value, bool) or value is None:
        return None
    if isinstance(value, int):
        return str(value)
    if isinstance(value, float):
        return str(int(value)) if value.is_integer() else str(value)
    if isinstance(value, str) and value.strip():
        return value.strip()
    return None


def string_list(value: object) -> list[str]:
    if not isinstance(value, list):
        return []
    return [item.strip() for item in value if isinstance(item, str) and item.strip()]
