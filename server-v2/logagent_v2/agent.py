from __future__ import annotations

from .store import JsonObject, Store


class AgentRuntime:
    """Small V2 agent runtime stub.

    This class is the stable execution seam for LangGraph. The current slice
    records durable events and produces a low-confidence final answer so the
    API, store, worker, and UI contract can be exercised before model/provider
    integration lands.
    """

    def __init__(self, store: Store):
        self.store = store

    def run_analysis(self, workspace_id: str, run_id: str) -> JsonObject:
        workspace = self.store.get_workspace(workspace_id)
        self.store.update_run_status(run_id, "running", "agent_round")
        self.store.create_evidence(
            workspace_id=workspace_id,
            run_id=run_id,
            kind="user_question",
            final_allowed=True,
            summary="User question captured as initial evidence.",
            payload={"question": workspace["question"]},
        )
        final_answer = {
            "summary": "V2 analysis runtime is initialized. Full LangGraph model reasoning is not wired yet.",
            "symptoms": [],
            "likelyRootCauses": [],
            "nextChecks": [
                "Migrate log extraction and search into V2 evidence pipeline.",
                "Wire LangGraph model provider and MCP tool gateway.",
            ],
            "fixSuggestions": [],
            "missingInformation": ["No log evidence has been analyzed by V2 yet."],
            "confidence": "low",
            "evidenceRefs": [],
        }
        self.store.update_run_status(run_id, "succeeded", "finish", final_answer)
        return final_answer

