from __future__ import annotations

import json
import subprocess
from hashlib import sha256
from pathlib import Path

from .artifacts import resolve_artifact_path, write_artifact_bytes
from .config import Settings, ToolDefinition
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
    input_entries = select_tool_inputs(settings, store, run_id, tool)
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
    run_id: str,
    tool: ToolDefinition,
) -> list[JsonObject]:
    if not tool_requires_input(tool):
        return []
    index = latest_tool_input_index(settings, store, run_id)
    inputs = index.get("inputs") if isinstance(index, dict) else None
    if not isinstance(inputs, list):
        return []
    selected = []
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
    return selected


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


def parse_json(data: bytes) -> JsonObject | None:
    if not data.strip():
        return None
    try:
        value = json.loads(data.decode("utf-8"))
    except Exception:
        return None
    return value if isinstance(value, dict) else None


def summary_from_stdout(parsed: JsonObject | None, stdout: bytes, timed_out: bool) -> str:
    if timed_out:
        return "Tool timed out."
    if parsed and isinstance(parsed.get("summary"), str):
        return parsed["summary"]
    preview = stdout.decode("utf-8", errors="replace").strip()
    return preview[:500] if preview else "Tool produced no stdout."


def findings_from_stdout(parsed: JsonObject | None) -> list[JsonObject]:
    if not parsed:
        return []
    findings = parsed.get("findings")
    if not isinstance(findings, list):
        return []
    return [item for item in findings if isinstance(item, dict)]
