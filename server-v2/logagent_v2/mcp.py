from __future__ import annotations

from .store import Store


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

