from __future__ import annotations

from .agent_audit import (
    failed_agent_response,
    persist_agent_request,
    persist_agent_response,
    persist_analysis_state,
)
from .analysis_package import persist_analysis_package
from .config import Settings
from .evidence import build_initial_evidence
from .final_answer import normalize_and_validate_final_answer
from .llm import build_agent_provider_request, execute_agent_provider_request
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
        analysis_package = persist_analysis_package(
            self.settings,
            self.store,
            workspace_id,
            run_id,
            evidence_bundle,
        )
        self.store.update_run_status(run_id, "running", "agent_round")
        final_answer = self._run_agent_round(
            workspace=workspace,
            workspace_id=workspace_id,
            run_id=run_id,
            evidence_bundle=evidence_bundle,
            analysis_package_artifact_id=analysis_package["artifact"]["id"],
        )
        self.store.update_run_status(run_id, "succeeded", "finish", final_answer)
        return final_answer

    def _run_agent_round(
        self,
        workspace: JsonObject,
        workspace_id: str,
        run_id: str,
        evidence_bundle: JsonObject,
        analysis_package_artifact_id: str | None,
    ) -> JsonObject:
        attempt = 1
        provider_request = build_agent_provider_request(
            self.settings, workspace, evidence_bundle
        )
        request_audit = persist_agent_request(
            settings=self.settings,
            store=self.store,
            workspace_id=workspace_id,
            run_id=run_id,
            attempt=attempt,
            provider_request=provider_request,
            analysis_package_artifact_id=analysis_package_artifact_id,
        )
        request_artifact_id = request_audit["artifact"]["id"]
        base_round = {
            "attempt": attempt,
            "provider": provider_request.get("provider"),
            "model": provider_request.get("model"),
            "requestArtifactId": request_artifact_id,
            "allowedEvidenceRefCount": len(provider_request.get("allowedEvidenceRefs", [])),
        }
        persist_analysis_state(
            settings=self.settings,
            store=self.store,
            workspace_id=workspace_id,
            run_id=run_id,
            state={
                "status": "running",
                "phase": "agent_round",
                "rounds": [{**base_round, "status": "requested"}],
            },
        )

        response_audit: JsonObject | None = None
        state_failed = False
        try:
            provider_response = execute_agent_provider_request(
                self.settings, provider_request
            )
            if provider_response.get("status") == "skipped":
                provider_response = {
                    **provider_response,
                    "status": "completed",
                    "finalAnswer": self._stub_final_answer(workspace, evidence_bundle),
                }
            if provider_response.get("status") != "completed":
                provider_response = {
                    **provider_response,
                    "validation": {"status": "not_run"},
                }
                response_audit = persist_agent_response(
                    settings=self.settings,
                    store=self.store,
                    workspace_id=workspace_id,
                    run_id=run_id,
                    attempt=attempt,
                    provider_response=provider_response,
                    request_artifact_id=request_artifact_id,
                )
                self._persist_failed_state(
                    workspace_id,
                    run_id,
                    base_round,
                    response_audit["artifact"]["id"],
                    provider_response,
                )
                state_failed = True
                error = provider_response.get("error")
                message = error.get("message") if isinstance(error, dict) else None
                raise ValueError(message or "agent provider failed")

            raw_final_answer = provider_response.get("finalAnswer")
            if not isinstance(raw_final_answer, dict):
                raise ValueError("agent provider did not return a final answer")
            try:
                final_answer = normalize_and_validate_final_answer(
                    self.settings, self.store, run_id, raw_final_answer
                )
            except Exception as error:
                provider_response = {
                    **provider_response,
                    "validation": {
                        "status": "failed",
                        "type": error.__class__.__name__,
                        "message": str(error)[:4000],
                    },
                }
                response_audit = persist_agent_response(
                    settings=self.settings,
                    store=self.store,
                    workspace_id=workspace_id,
                    run_id=run_id,
                    attempt=attempt,
                    provider_response=provider_response,
                    request_artifact_id=request_artifact_id,
                )
                self._persist_failed_state(
                    workspace_id,
                    run_id,
                    base_round,
                    response_audit["artifact"]["id"],
                    provider_response,
                )
                state_failed = True
                raise

            provider_response = {
                **provider_response,
                "validatedFinalAnswer": final_answer,
                "validation": {"status": "passed"},
            }
            response_audit = persist_agent_response(
                settings=self.settings,
                store=self.store,
                workspace_id=workspace_id,
                run_id=run_id,
                attempt=attempt,
                provider_response=provider_response,
                request_artifact_id=request_artifact_id,
            )
            persist_analysis_state(
                settings=self.settings,
                store=self.store,
                workspace_id=workspace_id,
                run_id=run_id,
                state={
                    "status": "succeeded",
                    "phase": "finish",
                    "rounds": [
                        {
                            **base_round,
                            "status": "completed",
                            "responseArtifactId": response_audit["artifact"]["id"],
                            "validation": {"status": "passed"},
                        }
                    ],
                    "finalAnswerStatus": "validated",
                },
            )
            return final_answer
        except Exception as error:
            if response_audit is None:
                provider_response = {
                    **failed_agent_response(provider_request, error),
                    "validation": {"status": "not_run"},
                }
                response_audit = persist_agent_response(
                    settings=self.settings,
                    store=self.store,
                    workspace_id=workspace_id,
                    run_id=run_id,
                    attempt=attempt,
                    provider_response=provider_response,
                    request_artifact_id=request_artifact_id,
                )
            if not state_failed:
                self._persist_failed_state(
                    workspace_id,
                    run_id,
                    base_round,
                    response_audit["artifact"]["id"],
                    provider_response,
                )
            raise

    def _persist_failed_state(
        self,
        workspace_id: str,
        run_id: str,
        base_round: JsonObject,
        response_artifact_id: str,
        provider_response: JsonObject,
    ) -> None:
        persist_analysis_state(
            settings=self.settings,
            store=self.store,
            workspace_id=workspace_id,
            run_id=run_id,
            state={
                "status": "failed",
                "phase": "agent_round",
                "rounds": [
                    {
                        **base_round,
                        "status": "failed",
                        "responseArtifactId": response_artifact_id,
                        "error": provider_response.get("error"),
                        "validation": provider_response.get("validation"),
                    }
                ],
                "finalAnswerStatus": "invalid",
            },
        )

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
