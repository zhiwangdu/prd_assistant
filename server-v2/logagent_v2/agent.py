from __future__ import annotations

from .analysis_package import persist_analysis_package
from .config import Settings
from .evidence import build_initial_evidence
from .final_answer import normalize_and_validate_final_answer
from .llm import generate_agent_final_answer
from .metadata import persist_metadata_context
from .skills import persist_system_context
from .store import JsonObject, Store


class AgentRuntime:
    """Small V2 agent runtime stub.

    This class is the stable execution seam for LangGraph. The current slice
    records durable events and produces a low-confidence final answer so the
    API, store, worker, and UI contract can be exercised before model/provider
    integration lands.
    """

    def __init__(self, settings: Settings, store: Store):
        self.settings = settings
        self.store = store

    def run_analysis(self, workspace_id: str, run_id: str) -> JsonObject:
        workspace = self.store.get_workspace(workspace_id)
        self.store.update_run_status(run_id, "running", "collect_initial_evidence")
        self.store.create_evidence(
            workspace_id=workspace_id,
            run_id=run_id,
            kind="user_question",
            final_allowed=True,
            summary="User question captured as initial evidence.",
            payload={"question": workspace["question"]},
        )
        persist_system_context(self.settings, self.store, workspace_id, run_id)
        persist_metadata_context(self.settings, self.store, workspace_id, run_id)
        evidence_bundle = build_initial_evidence(
            self.settings,
            self.store,
            workspace_id,
            run_id,
        )
        persist_analysis_package(
            self.settings,
            self.store,
            workspace_id,
            run_id,
            evidence_bundle,
        )
        self.store.update_run_status(run_id, "running", "agent_round")
        final_answer = generate_agent_final_answer(self.settings, workspace, evidence_bundle)
        if final_answer is None:
            final_answer = self._stub_final_answer(workspace, evidence_bundle)
        final_answer = normalize_and_validate_final_answer(
            self.settings, self.store, run_id, final_answer
        )
        self.store.update_run_status(run_id, "succeeded", "finish", final_answer)
        return final_answer

    def _stub_final_answer(self, workspace: JsonObject, evidence_bundle: JsonObject) -> JsonObject:
        manifest = evidence_bundle["manifest"]
        grep_results = evidence_bundle["grepResults"]
        matches = grep_results["matches"]
        if not manifest["files"]:
            return {
                "summary": "V2 captured the question, but no supported text log files were uploaded.",
                "symptoms": [],
                "likelyRootCauses": [],
                "nextChecks": ["Upload .log/.txt files or supported .zip/.tar/.tar.gz packages."],
                "fixSuggestions": [],
                "missingInformation": ["No current-task log evidence is available."],
                "confidence": "low",
                "evidenceRefs": [],
            }
        if not matches:
            return {
                "summary": (
                    f"V2 indexed {manifest['fileCount']} text files, but the initial keyword "
                    "search found no suspicious lines."
                ),
                "symptoms": [],
                "likelyRootCauses": [],
                "nextChecks": [
                    "Run a targeted search with domain-specific keywords.",
                    "Wire the LangGraph model loop to plan follow-up MCP searches.",
                ],
                "fixSuggestions": [],
                "missingInformation": ["Initial grep evidence is empty."],
                "confidence": "low",
                "evidenceRefs": [],
            }
        top = matches[:3]
        return {
            "summary": (
                f"V2 indexed {manifest['fileCount']} text files and found "
                f"{grep_results['totalMatches']} initial keyword matches."
            ),
            "symptoms": [f"{match['path']}:{match['lineNumber']} {match['text']}" for match in top],
            "likelyRootCauses": [
                {
                    "cause": (
                        "Initial log evidence contains suspicious keywords. Full model reasoning "
                        "is not wired yet, so this is an evidence summary rather than root cause."
                    ),
                    "evidenceRefs": [top[0]["ref"]],
                }
            ],
            "nextChecks": [
                "Enable the OpenAI-compatible Agent provider for model reasoning.",
                "Add task MCP log search and log slice tools for iterative investigation.",
            ],
            "fixSuggestions": [],
            "missingInformation": [
                "Full multi-round Agent planning and automatic Tool/Case follow-up are not wired yet."
            ],
            "confidence": "low",
            "evidenceRefs": [match["ref"] for match in top],
            "question": workspace["question"],
        }
