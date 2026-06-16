from __future__ import annotations

import json

from .artifacts import resolve_artifact_path
from .config import Settings
from .evidence import run_log_search
from .store import Store


def task_mcp_response(settings: Settings, store: Store, run_id: str, request: dict) -> dict:
    method = request.get("method")
    request_id = request.get("id")
    try:
        run = store.get_run(run_id)
        if method == "initialize":
            result = {
                "protocolVersion": "2025-06-18",
                "capabilities": {"resources": {}, "tools": {}},
                "serverInfo": {"name": "logagent-v2-task", "version": "0.1.0"},
            }
        elif method == "resources/list":
            result = {"resources": task_resources(run)}
        elif method == "resources/read":
            uri = request.get("params", {}).get("uri")
            result = read_task_resource(settings, store, run, uri)
        elif method == "tools/list":
            result = {"tools": [search_logs_descriptor()]}
        elif method == "tools/call":
            result = call_task_tool(settings, store, run, request.get("params", {}))
        else:
            raise ValueError(f"unsupported MCP method {method}")
        return {"jsonrpc": "2.0", "id": request_id, "result": result}
    except Exception as error:
        return {
            "jsonrpc": "2.0",
            "id": request_id,
            "error": {"code": -32000, "message": str(error)},
        }


def readonly_mcp_response(store: Store, request: dict) -> dict:
    method = request.get("method")
    request_id = request.get("id")
    try:
        if method == "initialize":
            result = {
                "protocolVersion": "2025-06-18",
                "capabilities": {"resources": {}, "tools": {}},
                "serverInfo": {"name": "logagent-v2-readonly", "version": "0.1.0"},
            }
        elif method == "resources/list":
            result = {
                "resources": [
                    {
                        "uri": "logagent-v2://tools/catalog",
                        "name": "tools_catalog",
                        "description": "V2 tool catalog placeholder",
                        "mimeType": "application/json",
                    }
                ]
            }
        elif method == "tools/list":
            result = {
                "tools": [
                    {
                        "name": "logagent.list_tools",
                        "description": "List V2 tool descriptors.",
                        "inputSchema": {"type": "object", "additionalProperties": False},
                    }
                ]
            }
        elif method == "tools/call":
            name = request.get("params", {}).get("name")
            if name != "logagent.list_tools":
                raise ValueError(f"unsupported readonly tool {name}")
            result = {"content": [{"type": "text", "text": "[]"}]}
        elif method == "resources/read":
            uri = request.get("params", {}).get("uri")
            if uri != "logagent-v2://tools/catalog":
                raise ValueError(f"unsupported readonly resource {uri}")
            result = {
                "contents": [
                    {
                        "uri": uri,
                        "mimeType": "application/json",
                        "text": "[]",
                    }
                ]
            }
        else:
            raise ValueError(f"unsupported MCP method {method}")
        return {"jsonrpc": "2.0", "id": request_id, "result": result}
    except Exception as error:
        return {
            "jsonrpc": "2.0",
            "id": request_id,
            "error": {"code": -32000, "message": str(error)},
        }


def task_resources(run: dict) -> list[dict]:
    run_id = run["id"]
    return [
        resource(run_id, "summary", "Run summary"),
        resource(run_id, "evidence", "Evidence index"),
        resource(run_id, "manifest", "Initial manifest"),
        resource(run_id, "grep_results", "Initial grep results"),
    ]


def resource(run_id: str, name: str, description: str) -> dict:
    return {
        "uri": f"logagent-v2://run/{run_id}/{name}",
        "name": name,
        "description": description,
        "mimeType": "application/json",
    }


def read_task_resource(settings: Settings, store: Store, run: dict, uri: str) -> dict:
    prefix = f"logagent-v2://run/{run['id']}/"
    if not isinstance(uri, str) or not uri.startswith(prefix):
        raise ValueError("resource URI does not belong to this run")
    name = uri.removeprefix(prefix)
    if name == "summary":
        value = {
            "run": run,
            "workspace": store.get_workspace(run["workspace_id"]),
        }
    elif name == "evidence":
        value = {"evidence": store.list_evidence(run["id"])}
    elif name == "manifest":
        value = read_latest_evidence_artifact(settings, store, run["id"], "manifest")
    elif name == "grep_results":
        value = read_initial_grep_artifact(settings, store, run["id"])
    else:
        raise ValueError(f"unsupported task resource {name}")
    return {
        "contents": [
            {
                "uri": uri,
                "mimeType": "application/json",
                "text": json.dumps(value, ensure_ascii=True, indent=2),
            }
        ]
    }


def read_latest_evidence_artifact(
    settings: Settings, store: Store, run_id: str, kind: str
) -> dict:
    candidates = [item for item in store.list_evidence(run_id) if item["kind"] == kind]
    if not candidates:
        raise ValueError(f"no {kind} evidence exists for run {run_id}")
    artifact_id = candidates[-1]["artifact_id"]
    if not artifact_id:
        raise ValueError(f"{kind} evidence has no artifact")
    return read_artifact_json(settings, store, artifact_id)


def read_initial_grep_artifact(settings: Settings, store: Store, run_id: str) -> dict:
    candidates = [
        item
        for item in store.list_evidence(run_id)
        if item["kind"] == "log_search" and item["payload"].get("path") == "grep_results.json"
    ]
    if not candidates:
        raise ValueError(f"no initial grep evidence exists for run {run_id}")
    artifact_id = candidates[-1]["artifact_id"]
    return read_artifact_json(settings, store, artifact_id)


def read_artifact_json(settings: Settings, store: Store, artifact_id: str) -> dict:
    artifact = store.get_artifact(artifact_id)
    path = resolve_artifact_path(settings, artifact["relative_path"])
    return json.loads(path.read_text(encoding="utf-8"))


def search_logs_descriptor() -> dict:
    return {
        "name": "logagent.search_logs",
        "description": "Search current Workspace uploads for keywords.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "keywords": {
                    "type": "array",
                    "items": {"type": "string", "minLength": 1},
                    "minItems": 1,
                    "maxItems": 16,
                }
            },
            "required": ["keywords"],
            "additionalProperties": False,
        },
    }


def call_task_tool(settings: Settings, store: Store, run: dict, params: dict) -> dict:
    name = params.get("name")
    arguments = params.get("arguments") or {}
    if name != "logagent.search_logs":
        raise ValueError(f"unsupported task tool {name}")
    keywords = arguments.get("keywords")
    if not isinstance(keywords, list) or not keywords:
        raise ValueError("logagent.search_logs requires non-empty keywords array")
    normalized = []
    for keyword in keywords[:16]:
        if not isinstance(keyword, str) or not keyword.strip():
            raise ValueError("keywords must be non-empty strings")
        normalized.append(keyword.strip()[:128])
    result = run_log_search(settings, store, run["workspace_id"], run["id"], normalized)
    text = json.dumps(
        {
            "search": result["search"],
            "evidence": result["evidence"],
        },
        ensure_ascii=True,
        indent=2,
    )
    return {"content": [{"type": "text", "text": text}]}
