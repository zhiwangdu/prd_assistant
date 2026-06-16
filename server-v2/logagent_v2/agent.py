from __future__ import annotations

import json

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
from .llm import (
    agent_allowed_tool_names,
    build_agent_provider_request,
    execute_agent_provider_request,
)
from .mcp import call_task_tool
from .metadata import persist_metadata_context
from .skills import persist_system_context
from .store import JsonObject, Store

MAX_TOOL_CALLS_PER_ROUND = 4


class AgentRuntime:
    """V2 Agent execution boundary.

    The runtime records durable audit artifacts around each provider round,
    executes a bounded set of Server-owned read-only tools, and validates the
    final answer before the run can succeed. The default provider remains the
    deterministic local stub.
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
        tool_observations: list[JsonObject] = []
        rounds: list[JsonObject] = []
        last_provider_request: JsonObject | None = None
        last_base_round: JsonObject | None = None
        last_request_artifact_id: str | None = None

        for attempt in range(1, self.settings.agent_max_rounds + 1):
            provider_request = build_agent_provider_request(
                self.settings, workspace, evidence_bundle, tool_observations
            )
            last_provider_request = provider_request
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
            last_request_artifact_id = request_artifact_id
            base_round = {
                "attempt": attempt,
                "provider": provider_request.get("provider"),
                "model": provider_request.get("model"),
                "requestArtifactId": request_artifact_id,
                "allowedEvidenceRefCount": len(provider_request.get("allowedEvidenceRefs", [])),
                "toolObservationCount": len(tool_observations),
            }
            last_base_round = base_round
            rounds.append({**base_round, "status": "requested"})
            self._persist_state(
                workspace_id,
                run_id,
                status="running",
                phase="agent_round",
                rounds=rounds,
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
                        rounds,
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
                    raise ValueError("agent provider did not return a JSON object")

                if is_tool_call_request(raw_final_answer):
                    tool_calls = normalize_tool_calls(
                        raw_final_answer,
                        allowed_tool_names=agent_allowed_tool_names(self.settings),
                    )
                    observations = self._execute_tool_calls(run_id, attempt, tool_calls)
                    tool_observations.extend(observations)
                    provider_response = {
                        **provider_response,
                        "toolCalls": tool_calls,
                        "toolObservations": observations,
                        "validation": {"status": "tool_calls_executed"},
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
                    rounds[-1] = {
                        **base_round,
                        "status": "tool_calls_executed",
                        "responseArtifactId": response_audit["artifact"]["id"],
                        "toolCallCount": len(tool_calls),
                        "validation": {"status": "tool_calls_executed"},
                    }
                    self._persist_state(
                        workspace_id,
                        run_id,
                        status="running",
                        phase="agent_round",
                        rounds=rounds,
                        final_answer_status="pending",
                    )
                    continue

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
                        rounds,
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
                rounds[-1] = {
                    **base_round,
                    "status": "completed",
                    "responseArtifactId": response_audit["artifact"]["id"],
                    "validation": {"status": "passed"},
                }
                self._persist_state(
                    workspace_id,
                    run_id,
                    status="succeeded",
                    phase="finish",
                    rounds=rounds,
                    final_answer_status="validated",
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
                        rounds,
                        base_round,
                        response_audit["artifact"]["id"],
                        provider_response,
                    )
                raise

        error = ValueError(
            f"agent reached LOGAGENT_V2_AGENT_MAX_ROUNDS={self.settings.agent_max_rounds}"
        )
        provider_response = {
            **failed_agent_response(last_provider_request or {}, error),
            "validation": {"status": "not_run"},
        }
        response_audit = persist_agent_response(
            settings=self.settings,
            store=self.store,
            workspace_id=workspace_id,
            run_id=run_id,
            attempt=self.settings.agent_max_rounds,
            provider_response=provider_response,
            request_artifact_id=last_request_artifact_id,
        )
        self._persist_failed_state(
            workspace_id,
            run_id,
            rounds,
            last_base_round or {},
            response_audit["artifact"]["id"],
            provider_response,
        )
        raise error

    def _persist_state(
        self,
        workspace_id: str,
        run_id: str,
        status: str,
        phase: str,
        rounds: list[JsonObject],
        final_answer_status: str | None = None,
    ) -> None:
        state: JsonObject = {
            "status": status,
            "phase": phase,
            "rounds": rounds,
        }
        if final_answer_status is not None:
            state["finalAnswerStatus"] = final_answer_status
        persist_analysis_state(
            settings=self.settings,
            store=self.store,
            workspace_id=workspace_id,
            run_id=run_id,
            state=state,
        )

    def _persist_failed_state(
        self,
        workspace_id: str,
        run_id: str,
        rounds: list[JsonObject],
        base_round: JsonObject,
        response_artifact_id: str,
        provider_response: JsonObject,
    ) -> None:
        if rounds:
            rounds[-1] = {
                **base_round,
                "status": "failed",
                "responseArtifactId": response_artifact_id,
                "error": provider_response.get("error"),
                "validation": provider_response.get("validation"),
            }
        self._persist_state(
            workspace_id,
            run_id,
            status="failed",
            phase="agent_round",
            rounds=rounds,
            final_answer_status="invalid",
        )

    def _execute_tool_calls(
        self,
        run_id: str,
        attempt: int,
        tool_calls: list[JsonObject],
    ) -> list[JsonObject]:
        run = self.store.get_run(run_id)
        observations = []
        for index, tool_call in enumerate(tool_calls):
            name = tool_call["name"]
            arguments = tool_call["arguments"]
            result = call_task_tool(
                self.settings,
                self.store,
                run,
                {"name": name, "arguments": arguments},
            )
            observations.append(
                {
                    "toolCallId": f"round_{attempt}_call_{index}",
                    "name": name,
                    "arguments": arguments,
                    "result": parse_tool_result(result),
                }
            )
        return observations

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
                "Use task MCP log search and log slice tools for iterative investigation.",
            ],
            "fixSuggestions": [],
            "missingInformation": [
                "Full automatic domain-tool and Case follow-up are not wired yet."
            ],
            "confidence": "low",
            "evidenceRefs": [match["ref"] for match in top],
            "question": workspace["question"],
        }


def is_tool_call_request(value: JsonObject) -> bool:
    return value.get("type") == "tool_calls" or isinstance(value.get("toolCalls"), list)


def normalize_tool_calls(
    value: JsonObject,
    allowed_tool_names: set[str],
) -> list[JsonObject]:
    raw_calls = value.get("toolCalls")
    if not isinstance(raw_calls, list) or not raw_calls:
        raise ValueError("agent tool_calls response requires non-empty toolCalls")
    if len(raw_calls) > MAX_TOOL_CALLS_PER_ROUND:
        raise ValueError(f"agent requested too many tool calls: {len(raw_calls)}")
    tool_calls = []
    for index, item in enumerate(raw_calls):
        if not isinstance(item, dict):
            raise ValueError(f"toolCalls[{index}] must be an object")
        name = item.get("name")
        if name not in allowed_tool_names:
            raise ValueError(f"unsupported agent tool call: {name}")
        arguments = item.get("arguments")
        if arguments is None:
            arguments = {}
        if not isinstance(arguments, dict):
            raise ValueError(f"toolCalls[{index}].arguments must be an object")
        tool_calls.append({"name": name, "arguments": arguments})
    return tool_calls


def parse_tool_result(result: JsonObject) -> JsonObject:
    content = result.get("content")
    if not isinstance(content, list):
        return {"content": result}
    texts = [
        item.get("text")
        for item in content
        if isinstance(item, dict) and isinstance(item.get("text"), str)
    ]
    if not texts:
        return {"content": content}
    try:
        decoded = json.loads(texts[0])
    except json.JSONDecodeError:
        return {"contentPreview": texts[0][:4000]}
    if isinstance(decoded, dict):
        return decoded
    return {"content": decoded}
