from __future__ import annotations

import json

from .artifacts import resolve_artifact_path
from .case_memory import call_case_tool, case_tool_descriptors
from .config import Settings
from .evidence import get_log_slice, run_log_search
from .fetch import call_fetch_tool, fetch_tool_descriptors
from .metadata import call_metadata_tool, metadata_tool_descriptors
from .results import latest_evidence, read_text_artifact
from .settings_api import domain_adapter_summaries
from .skills import (
    get_skill,
    list_skills,
    preview_system_context,
    read_readonly_skill_reference,
    read_task_skill_reference,
    skill_tool_descriptors,
)
from .store import Store
from .tools import run_configured_tool, tool_descriptors


METADATA_TOOL_NAMES = {tool["name"] for tool in metadata_tool_descriptors()}
CASE_TOOL_NAMES = {tool["name"] for tool in case_tool_descriptors()}
SKILL_TOOL_NAMES = {tool["name"] for tool in skill_tool_descriptors()}
FETCH_TOOL_NAMES = {tool["name"] for tool in fetch_tool_descriptors()}


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
            result = {
                "tools": [
                    search_logs_descriptor(),
                    get_log_slice_descriptor(),
                    run_domain_tool_descriptor(settings),
                    request_user_input_descriptor(),
                    request_approval_descriptor(),
                    *metadata_tool_descriptors(),
                    *case_tool_descriptors(),
                    *skill_tool_descriptors(),
                    *fetch_tool_descriptors(),
                ]
            }
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


def readonly_mcp_response(settings: Settings, store: Store, request: dict) -> dict:
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
                        "description": "Configured and built-in V2 tool catalog.",
                        "mimeType": "application/json",
                    },
                    {
                        "uri": "logagent-v2://metadata/instances",
                        "name": "metadata_instances",
                        "description": "Imported V2 metadata instances",
                        "mimeType": "application/json",
                    },
                    {
                        "uri": "logagent-v2://cases/recent",
                        "name": "cases_recent",
                        "description": "Recent enabled V2 cases",
                        "mimeType": "application/json",
                    },
                    {
                        "uri": "logagent-v2://skills",
                        "name": "skills",
                        "description": "Imported V2 diagnostic skills",
                        "mimeType": "application/json",
                    },
                    {
                        "uri": "logagent-v2://domain-adapters",
                        "name": "domain_adapters",
                        "description": "Built-in V2 domain adapter summaries",
                        "mimeType": "application/json",
                    },
                ]
            }
        elif method == "tools/list":
            result = {
                "tools": [
                    {
                        "name": "logagent.list_tools",
                        "description": "List V2 tool descriptors.",
                        "inputSchema": {"type": "object", "additionalProperties": False},
                    },
                    {
                        "name": "logagent.list_domain_adapters",
                        "description": "List built-in V2 domain adapter summaries.",
                        "inputSchema": {"type": "object", "additionalProperties": False},
                    },
                    *metadata_tool_descriptors(),
                    *case_tool_descriptors(),
                    *skill_tool_descriptors(),
                ]
            }
        elif method == "tools/call":
            name = request.get("params", {}).get("name")
            arguments = request.get("params", {}).get("arguments") or {}
            if name == "logagent.list_tools":
                result = {
                    "content": [
                        {
                            "type": "text",
                            "text": json.dumps(
                                tool_catalog(settings),
                                ensure_ascii=True,
                                indent=2,
                            ),
                        }
                    ]
                }
            elif name == "logagent.list_domain_adapters":
                result = {
                    "content": [
                        {
                            "type": "text",
                            "text": json.dumps(
                                {"domainAdapters": domain_adapter_summaries()},
                                ensure_ascii=True,
                                indent=2,
                            ),
                        }
                    ]
                }
            elif name in METADATA_TOOL_NAMES:
                value = call_metadata_tool(None, store, None, name, arguments)
                result = {
                    "content": [
                        {
                            "type": "text",
                            "text": json.dumps(value, ensure_ascii=True, indent=2),
                        }
                    ]
                }
            elif name in CASE_TOOL_NAMES:
                value = call_case_tool(None, store, None, name, arguments)
                result = {
                    "content": [
                        {
                            "type": "text",
                            "text": json.dumps(value, ensure_ascii=True, indent=2),
                        }
                    ]
                }
            elif name in SKILL_TOOL_NAMES:
                value = call_readonly_skill_tool(settings, name, arguments)
                result = {
                    "content": [
                        {
                            "type": "text",
                            "text": json.dumps(value, ensure_ascii=True, indent=2),
                        }
                    ]
                }
            else:
                raise ValueError(f"unsupported readonly tool {name}")
        elif method == "resources/read":
            uri = request.get("params", {}).get("uri")
            if uri == "logagent-v2://tools/catalog":
                value = tool_catalog(settings)
            elif uri == "logagent-v2://metadata/instances":
                value = {"instances": store.list_metadata_instances()}
            elif uri == "logagent-v2://cases/recent":
                value = {"cases": store.search_cases(query=None, limit=10)}
            elif uri == "logagent-v2://skills":
                value = {"skills": list_skills(settings)}
            elif uri == "logagent-v2://domain-adapters":
                value = {"domainAdapters": domain_adapter_summaries()}
            elif isinstance(uri, str) and uri.startswith("logagent-v2://skills/"):
                skill_id = uri.removeprefix("logagent-v2://skills/")
                value = get_skill(settings, skill_id)
            elif isinstance(uri, str) and uri.startswith(
                "logagent-v2://metadata/instances/"
            ) and uri.endswith("/snapshot"):
                instance_id = uri.removeprefix("logagent-v2://metadata/instances/").removesuffix(
                    "/snapshot"
                )
                value = store.get_metadata_snapshot(instance_id)
            else:
                raise ValueError(f"unsupported readonly resource {uri}")
            result = {
                "contents": [
                    {
                        "uri": uri,
                        "mimeType": "application/json",
                        "text": json.dumps(value, ensure_ascii=True, indent=2),
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


def tool_catalog(settings: Settings) -> dict:
    descriptors = tool_descriptors(settings)
    configured_tools = [
        {
            "toolId": tool["toolId"],
            "enabled": tool["enabled"],
            "timeoutSeconds": find_configured_tool_timeout(settings, tool["toolId"]),
            "maxInputFiles": tool.get("maxInputFiles", tool.get("maxFiles")),
            "configuredArgs": list(find_configured_tool_args(settings, tool["toolId"])),
            "match": tool.get("match", {"filePatterns": [], "keywords": []}),
        }
        for tool in descriptors
        if tool.get("source") == "configured"
    ]
    return {
        "schemaVersion": 1,
        "tools": descriptors,
        "configuredTools": configured_tools,
    }


def find_configured_tool_args(settings: Settings, tool_id: str) -> tuple[str, ...]:
    for tool in settings.tools:
        if tool.id == tool_id:
            return tool.args
    return ()


def find_configured_tool_timeout(settings: Settings, tool_id: str) -> int | None:
    for tool in settings.tools:
        if tool.id == tool_id:
            return tool.timeout_seconds
    return None


def task_resources(run: dict) -> list[dict]:
    run_id = run["id"]
    return [
        resource(run_id, "summary", "Run summary"),
        resource(run_id, "evidence", "Evidence index"),
        resource(run_id, "manifest", "Initial manifest"),
        resource(run_id, "grep_results", "Initial grep results"),
        resource(run_id, "system_context", "System Context snapshot"),
        resource(run_id, "metadata_context", "Metadata Context snapshot"),
        resource(run_id, "environment_evidence", "Latest approved environment evidence"),
        resource(run_id, "analysis_package", "Bounded Agent analysis package"),
        resource(run_id, "analysis_state", "Latest Analysis Agent state snapshot"),
        resource(run_id, "agent_request", "Latest Agent provider request"),
        resource(run_id, "agent_response", "Latest Agent provider response"),
        resource(run_id, "result", "Final result JSON artifact"),
        resource(run_id, "result_markdown", "Final result Markdown artifact", "text/markdown"),
    ]


def resource(
    run_id: str,
    name: str,
    description: str,
    mime_type: str = "application/json",
) -> dict:
    return {
        "uri": f"logagent-v2://run/{run_id}/{name}",
        "name": name,
        "description": description,
        "mimeType": mime_type,
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
    elif name == "system_context":
        value = read_latest_evidence_artifact(settings, store, run["id"], "system_context")
    elif name == "metadata_context":
        value = read_latest_evidence_artifact(settings, store, run["id"], "metadata_context")
    elif name == "environment_evidence":
        value = read_latest_evidence_artifact(settings, store, run["id"], "environment_evidence")
    elif name == "analysis_package":
        value = read_latest_evidence_artifact(settings, store, run["id"], "analysis_package")
    elif name == "analysis_state":
        value = read_latest_evidence_artifact(settings, store, run["id"], "analysis_state")
    elif name == "agent_request":
        value = read_latest_evidence_artifact(settings, store, run["id"], "agent_request")
    elif name == "agent_response":
        value = read_latest_evidence_artifact(settings, store, run["id"], "agent_response")
    elif name == "result":
        value = read_latest_evidence_artifact(settings, store, run["id"], "result")
    elif name == "result_markdown":
        evidence = latest_evidence(store, run["id"], "result_markdown")
        text = read_text_artifact(settings, store, evidence["artifact_id"])
        return {
            "contents": [
                {
                    "uri": uri,
                    "mimeType": "text/markdown",
                    "text": text,
                }
            ]
        }
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
    if name in METADATA_TOOL_NAMES:
        value = call_metadata_tool(settings, store, run, name, arguments)
        return {
            "content": [
                {
                    "type": "text",
                    "text": json.dumps(value, ensure_ascii=True, indent=2),
                }
            ]
        }
    if name in CASE_TOOL_NAMES:
        value = call_case_tool(settings, store, run, name, arguments)
        return {
            "content": [
                {
                    "type": "text",
                    "text": json.dumps(value, ensure_ascii=True, indent=2),
                }
            ]
        }
    if name in SKILL_TOOL_NAMES:
        value = call_task_skill_tool(settings, store, run, name, arguments)
        return {
            "content": [
                {
                    "type": "text",
                    "text": json.dumps(value, ensure_ascii=True, indent=2),
                }
            ]
        }
    if name in FETCH_TOOL_NAMES:
        value = call_fetch_tool(settings, store, run, name, arguments)
        return {
            "content": [
                {
                    "type": "text",
                    "text": json.dumps(value, ensure_ascii=True, indent=2),
                }
            ]
        }
    if name == "logagent.get_log_slice":
        return call_get_log_slice(settings, store, run, arguments)
    if name == "logagent.run_domain_tool":
        return call_run_domain_tool(settings, store, run, arguments)
    if name == "logagent.request_user_input":
        return call_request_user_input(store, run, arguments)
    if name == "logagent.request_approval":
        return call_request_approval(store, run, arguments)
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


def request_user_input_descriptor() -> dict:
    return {
        "name": "logagent.request_user_input",
        "description": "Pause this run and ask the user for more information.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "question": {"type": "string", "minLength": 1},
                "reason": {"type": "string"},
                "required": {"type": "boolean", "default": True},
                "answerFormat": {"type": "string"},
            },
            "required": ["question"],
            "additionalProperties": False,
        },
    }


def request_approval_descriptor() -> dict:
    return {
        "name": "logagent.request_approval",
        "description": "Pause this run and request approval for a gated action.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "actionType": {"type": "string", "minLength": 1},
                "reason": {"type": "string", "minLength": 1},
                "input": {"type": "object"},
            },
            "required": ["actionType", "reason"],
            "additionalProperties": False,
        },
    }


def call_request_user_input(store: Store, run: dict, arguments: dict) -> dict:
    question = arguments.get("question")
    if not isinstance(question, str) or not question.strip():
        raise ValueError("logagent.request_user_input requires question")
    action = store.create_action(
        run["id"],
        "user_input",
        {
            "question": question.strip(),
            "reason": arguments.get("reason"),
            "required": bool(arguments.get("required", True)),
            "answerFormat": arguments.get("answerFormat"),
        },
    )
    store.update_run_status(run["id"], "waiting_for_user", "waiting_for_user")
    return {
        "content": [
            {
                "type": "text",
                "text": json.dumps({"action": action}, ensure_ascii=True, indent=2),
            }
        ]
    }


def call_request_approval(store: Store, run: dict, arguments: dict) -> dict:
    action_type = arguments.get("actionType")
    reason = arguments.get("reason")
    if not isinstance(action_type, str) or not action_type.strip():
        raise ValueError("logagent.request_approval requires actionType")
    if not isinstance(reason, str) or not reason.strip():
        raise ValueError("logagent.request_approval requires reason")
    action = store.create_action(
        run["id"],
        "approval",
        {
            "actionType": action_type.strip(),
            "reason": reason.strip(),
            "input": arguments.get("input") if isinstance(arguments.get("input"), dict) else {},
        },
    )
    store.update_run_status(run["id"], "waiting_for_approval", "waiting_for_approval")
    return {
        "content": [
            {
                "type": "text",
                "text": json.dumps({"action": action}, ensure_ascii=True, indent=2),
            }
        ]
    }


def run_domain_tool_descriptor(settings: Settings) -> dict:
    configured_tool_ids = [
        tool["toolId"]
        for tool in tool_descriptors(settings)
        if tool["enabled"] and tool.get("source") == "configured"
    ]
    return {
        "name": "logagent.run_domain_tool",
        "description": "Run a configured read-only diagnostic tool by toolId or legacy tool/inputFile.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "toolId": {
                    "type": "string",
                    "enum": configured_tool_ids,
                },
                "tool": {
                    "type": "string",
                    "enum": configured_tool_ids,
                },
                "inputFile": {"type": "string"},
                "params": {"type": "object"},
            },
            "additionalProperties": False,
        },
    }


def call_run_domain_tool(settings: Settings, store: Store, run: dict, arguments: dict) -> dict:
    tool_id = arguments.get("toolId") or arguments.get("tool")
    if not isinstance(tool_id, str) or not tool_id:
        raise ValueError("logagent.run_domain_tool requires toolId")
    params = arguments.get("params")
    if params is not None and not isinstance(params, dict):
        raise ValueError("logagent.run_domain_tool params must be an object")
    run_params = dict(params or {})
    input_file = arguments.get("inputFile")
    if input_file is not None:
        if not isinstance(input_file, str):
            raise ValueError("logagent.run_domain_tool inputFile must be a string")
        existing = run_params.get("inputFiles")
        if existing is None:
            run_params["inputFiles"] = [input_file]
        elif isinstance(existing, list):
            run_params["inputFiles"] = [input_file, *existing]
        else:
            run_params["inputFiles"] = [input_file, existing]
    result = run_configured_tool(
        settings,
        store,
        run["workspace_id"],
        run["id"],
        tool_id,
        params=run_params,
    )
    payload = {
        "result": result["result"],
        "evidence": result["evidence"],
    }
    if "results" in result:
        payload["results"] = result["results"]
        payload["evidenceItems"] = result["evidenceItems"]
    text = json.dumps(
        payload,
        ensure_ascii=True,
        indent=2,
    )
    return {"content": [{"type": "text", "text": text}]}


def get_log_slice_descriptor() -> dict:
    return {
        "name": "logagent.get_log_slice",
        "description": "Read bounded context lines around a current Workspace log path.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": {"type": "string", "minLength": 1},
                "lineNumber": {"type": "integer", "minimum": 1},
                "before": {"type": "integer", "minimum": 0, "maximum": 50, "default": 5},
                "after": {"type": "integer", "minimum": 0, "maximum": 50, "default": 5},
            },
            "required": ["path", "lineNumber"],
            "additionalProperties": False,
        },
    }


def call_get_log_slice(settings: Settings, store: Store, run: dict, arguments: dict) -> dict:
    path = arguments.get("path")
    line_number = arguments.get("lineNumber")
    if not isinstance(path, str) or not path:
        raise ValueError("logagent.get_log_slice requires path")
    if not isinstance(line_number, int):
        raise ValueError("logagent.get_log_slice requires integer lineNumber")
    result = get_log_slice(
        settings=settings,
        store=store,
        workspace_id=run["workspace_id"],
        run_id=run["id"],
        path=path,
        line_number=line_number,
        before=int(arguments.get("before", 5)),
        after=int(arguments.get("after", 5)),
    )
    text = json.dumps(
        {
            "slice": result["slice"],
            "evidence": result["evidence"],
        },
        ensure_ascii=True,
        indent=2,
    )
    return {"content": [{"type": "text", "text": text}]}


def call_readonly_skill_tool(settings: Settings, name: str, arguments: dict) -> dict:
    if name == "logagent.list_skills":
        return {"skills": list_skills(settings)}
    if name == "logagent.get_skill":
        return get_skill(settings, require_arg_string(arguments, "skillId"))
    if name == "logagent.get_skill_reference":
        return read_readonly_skill_reference(
            settings=settings,
            skill_id=require_arg_string(arguments, "skillId"),
            reference_id=optional_arg_string(arguments, "referenceId"),
            path=optional_arg_string(arguments, "path"),
        )
    if name == "logagent.preview_system_context":
        skill_ids = arguments.get("skillIds")
        if skill_ids is not None and not isinstance(skill_ids, list):
            raise ValueError("skillIds must be an array")
        return preview_system_context(settings, skill_ids)
    raise ValueError(f"unsupported skill tool {name}")


def call_task_skill_tool(
    settings: Settings, store: Store, run: dict, name: str, arguments: dict
) -> dict:
    if name == "logagent.get_skill_reference":
        return read_task_skill_reference(
            settings=settings,
            store=store,
            run_id=run["id"],
            skill_id=require_arg_string(arguments, "skillId"),
            reference_id=optional_arg_string(arguments, "referenceId"),
            path=optional_arg_string(arguments, "path"),
        )
    if name == "logagent.list_skills":
        return {"skills": list_skills(settings)}
    if name == "logagent.get_skill":
        return get_skill(settings, require_arg_string(arguments, "skillId"))
    if name == "logagent.preview_system_context":
        return preview_system_context(settings, arguments.get("skillIds") or [])
    raise ValueError(f"unsupported skill tool {name}")


def require_arg_string(arguments: dict, name: str) -> str:
    value = arguments.get(name)
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"{name} is required")
    return value.strip()


def optional_arg_string(arguments: dict, name: str) -> str | None:
    value = arguments.get(name)
    if not isinstance(value, str) or not value.strip():
        return None
    return value.strip()
