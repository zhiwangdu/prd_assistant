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
