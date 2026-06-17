from __future__ import annotations

from .store import JsonObject

AGENT_GRAPH_NODES = (
    "collect_initial_evidence",
    "agent_round",
    "finalize_result",
)


def graph_runtime_metadata() -> JsonObject:
    return {
        "engine": "langgraph",
        "graph": "logagent_v2_analysis",
        "nodes": list(AGENT_GRAPH_NODES),
    }
