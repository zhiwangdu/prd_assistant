from __future__ import annotations

from .store import JsonObject

AGENT_GRAPH_NODES = (
    "collect_initial_evidence",
    "prepare_agent_request",
    "call_agent_provider",
    "execute_tool_calls",
    "validate_final_answer",
    "finalize_result",
)


def graph_runtime_metadata() -> JsonObject:
    return {
        "engine": "langgraph",
        "graph": "logagent_v2_analysis",
        "nodes": list(AGENT_GRAPH_NODES),
    }
