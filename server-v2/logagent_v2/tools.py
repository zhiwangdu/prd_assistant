from __future__ import annotations

import json
import subprocess
from pathlib import Path

from .artifacts import write_artifact_bytes
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
            "paramsSchema": {
                "type": "object",
                "properties": {"toolId": {"const": tool.id}},
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
    command = Path(tool.command)
    if not command.is_absolute():
        raise ValueError(f"tool {tool_id} command must be an absolute path")
    argv = [str(command), *tool.args]
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
        filename=f"{tool.id}_result.json",
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
            "exitCode": result["exitCode"],
            "timedOut": timed_out,
            "findingCount": len(result["findings"]),
            "evidenceRefPrefix": f"tool_results/{tool.id}/result.json#findings/",
        },
    )
    return {"result": result, "artifact": artifact, "evidence": evidence}


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
