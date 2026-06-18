from __future__ import annotations

import json
import time
from hashlib import sha256
from typing import TypedDict

from langgraph.graph import END, START, StateGraph

from .agent_audit import (
    failed_agent_response,
    persist_agent_request,
    persist_agent_response,
    persist_analysis_state,
)
from .agent_graph import graph_runtime_metadata
from .alias import generate_run_alias
from .analysis_package import persist_analysis_package
from .artifacts import resolve_artifact_path
from .config import Settings
from .claude_contracts import persist_claude_contracts, persist_claude_runtime_session
from .evidence import SESSION_TEXT_INPUT_REF, build_initial_evidence, persist_session_text_input
from .final_answer import normalize_and_validate_final_answer
from .llm import (
    agent_allowed_tool_names,
    build_agent_provider_request,
    execute_agent_provider_request,
)
from .mcp import call_task_tool
from .mcp_audit import persist_mcp_call, read_mcp_calls
from .metadata import persist_metadata_context
from .results import persist_run_result
from .skills import persist_system_context
from .store import JsonObject, Store
from .tools import configured_tool_results_outline, run_matching_configured_tools

MAX_TOOL_CALLS_PER_ROUND = 4


class AgentGraphState(TypedDict, total=False):
    workspaceId: str
    runId: str
    workspace: JsonObject
    evidenceBundle: JsonObject
    analysisPackageArtifactId: str
    interactionContext: JsonObject
    toolObservations: list[JsonObject]
    rounds: list[JsonObject]
    attempt: int
    providerRequest: JsonObject
    providerResponse: JsonObject
    requestArtifactId: str
    responseArtifactId: str
    startedAtMonotonic: float
    baseRound: JsonObject
    toolCalls: list[JsonObject]
    waitingStatus: str
    finalAnswer: JsonObject
    runtimeStatus: str
    result: JsonObject


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
        graph = self._build_agent_graph()
        state = graph.invoke({"workspaceId": workspace_id, "runId": run_id})
        result = state.get("result")
        if isinstance(result, dict):
            return result
        raise ValueError("agent graph finished without a result")

    def _build_agent_graph(self):
        graph = StateGraph(AgentGraphState)
        graph.add_node("collect_initial_evidence", self._graph_collect_initial_evidence)
        graph.add_node("prepare_agent_request", self._graph_prepare_agent_request)
        graph.add_node("call_agent_provider", self._graph_call_agent_provider)
        graph.add_node("execute_tool_calls", self._graph_execute_tool_calls)
        graph.add_node("validate_final_answer", self._graph_validate_final_answer)
        graph.add_node("finalize_result", self._graph_finalize_result)
        graph.add_edge(START, "collect_initial_evidence")
        graph.add_edge("collect_initial_evidence", "prepare_agent_request")
        graph.add_conditional_edges(
            "prepare_agent_request",
            self._graph_after_prepare_agent_request,
            {
                "call_agent_provider": "call_agent_provider",
                "validate_final_answer": "validate_final_answer",
            },
        )
        graph.add_conditional_edges(
            "call_agent_provider",
            self._graph_after_provider_call,
            {
                "execute_tool_calls": "execute_tool_calls",
                "validate_final_answer": "validate_final_answer",
            },
        )
        graph.add_conditional_edges(
            "execute_tool_calls",
            self._graph_after_tool_calls,
            {"prepare_agent_request": "prepare_agent_request", "end": END},
        )
        graph.add_edge("validate_final_answer", "finalize_result")
        graph.add_edge("finalize_result", END)
        return graph.compile(name="logagent_v2_analysis")

    def _graph_collect_initial_evidence(self, state: AgentGraphState) -> AgentGraphState:
        workspace_id = state["workspaceId"]
        run_id = state["runId"]
        workspace = self.store.get_workspace(workspace_id)
        self.store.update_run_status(run_id, "running", "collect_initial_evidence")
        persist_session_text_input(
            self.settings,
            self.store,
            workspace_id,
            run_id,
            workspace["question"],
        )
        persist_system_context(self.settings, self.store, workspace_id, run_id)
        persist_metadata_context(self.settings, self.store, workspace_id, run_id)
        evidence_bundle = build_initial_evidence(
            self.settings,
            self.store,
            workspace_id,
            run_id,
        )
        evidence_bundle["workspaceId"] = workspace_id
        evidence_bundle["runId"] = run_id
        self.store.update_run_status(run_id, "running", "run_tool")
        auto_tool_results = run_matching_configured_tools(
            self.settings,
            self.store,
            workspace_id,
            run_id,
        )
        evidence_bundle["toolResults"] = configured_tool_results_outline(auto_tool_results)
        self.store.update_run_status(run_id, "running", "collect_initial_evidence")
        analysis_package = persist_analysis_package(
            self.settings,
            self.store,
            workspace_id,
            run_id,
            evidence_bundle,
        )
        persist_claude_contracts(
            self.settings,
            self.store,
            workspace_id,
            run_id,
            analysis_package["artifact"]["id"],
        )
        self.store.update_run_status(run_id, "running", "agent_round")
        return {
            "workspace": workspace,
            "evidenceBundle": evidence_bundle,
            "analysisPackageArtifactId": analysis_package["artifact"]["id"],
            "interactionContext": self._interaction_context(run_id),
            "toolObservations": [],
            "rounds": [],
            "attempt": 0,
            "startedAtMonotonic": time.monotonic(),
            "runtimeStatus": "prepare_agent_request",
        }

    def _graph_prepare_agent_request(self, state: AgentGraphState) -> AgentGraphState:
        workspace_id = state["workspaceId"]
        run_id = state["runId"]
        workspace = state["workspace"]
        evidence_bundle = state["evidenceBundle"]
        current_attempt = int(state.get("attempt", 0))
        rounds = list(state.get("rounds") or [])
        interaction_context = self._interaction_context(run_id)
        tool_observations = list(state.get("toolObservations") or [])
        budget_reason = self._analysis_budget_exhausted(
            run_id=run_id,
            completed_attempts=current_attempt,
            tool_observations=tool_observations,
            rounds=rounds,
            started_at_monotonic=state.get("startedAtMonotonic"),
        )
        if budget_reason:
            return self._prepare_budget_limited_result(
                workspace_id=workspace_id,
                run_id=run_id,
                workspace=workspace,
                evidence_bundle=evidence_bundle,
                interaction_context=interaction_context,
                tool_observations=tool_observations,
                rounds=rounds,
                attempt=max(1, current_attempt),
                reason=budget_reason,
                analysis_package_artifact_id=state.get("analysisPackageArtifactId"),
            )
        attempt = current_attempt + 1
        provider_request = build_agent_provider_request(
            self.settings,
            workspace,
            evidence_bundle,
            tool_observations,
            interaction_context,
        )
        request_audit = persist_agent_request(
            settings=self.settings,
            store=self.store,
            workspace_id=workspace_id,
            run_id=run_id,
            attempt=attempt,
            provider_request=provider_request,
            analysis_package_artifact_id=state.get("analysisPackageArtifactId"),
        )
        request_artifact_id = request_audit["artifact"]["id"]
        base_round = {
            "attempt": attempt,
            "provider": provider_request.get("provider"),
            "model": provider_request.get("model"),
            "requestArtifactId": request_artifact_id,
            "allowedEvidenceRefCount": len(provider_request.get("allowedEvidenceRefs", [])),
            "toolObservationCount": len(tool_observations),
        }
        rounds.append({**base_round, "status": "requested"})
        self._persist_state(
            workspace_id,
            run_id,
            status="running",
            phase="agent_round",
            rounds=rounds,
        )
        return {
            "attempt": attempt,
            "interactionContext": interaction_context,
            "toolObservations": tool_observations,
            "providerRequest": provider_request,
            "requestArtifactId": request_artifact_id,
            "baseRound": base_round,
            "rounds": rounds,
            "runtimeStatus": "call_agent_provider",
        }

    def _graph_after_prepare_agent_request(self, state: AgentGraphState) -> str:
        if state.get("runtimeStatus") == "validate_final_answer":
            return "validate_final_answer"
        return "call_agent_provider"

    def _graph_call_agent_provider(self, state: AgentGraphState) -> AgentGraphState:
        workspace_id = state["workspaceId"]
        run_id = state["runId"]
        provider_request = state["providerRequest"]
        attempt = state["attempt"]
        request_artifact_id = state["requestArtifactId"]
        rounds = list(state.get("rounds") or [])
        base_round = state.get("baseRound") or {}
        try:
            provider_response = execute_agent_provider_request(self.settings, provider_request)
        except Exception as error:
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
            self._persist_claude_runtime_session(
                workspace_id, run_id, attempt, provider_response, response_audit
            )
            self._persist_failed_state(
                workspace_id,
                run_id,
                rounds,
                base_round,
                response_audit["artifact"]["id"],
                provider_response,
            )
            raise
        if provider_response.get("status") == "skipped":
            provider_response = {
                **provider_response,
                "status": "completed",
                "finalAnswer": self._stub_final_answer(
                    state["workspace"],
                    state["evidenceBundle"],
                    state.get("interactionContext"),
                ),
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
            self._persist_claude_runtime_session(
                workspace_id, run_id, attempt, provider_response, response_audit
            )
            self._persist_failed_state(
                workspace_id,
                run_id,
                rounds,
                base_round,
                response_audit["artifact"]["id"],
                provider_response,
            )
            error = provider_response.get("error")
            message = error.get("message") if isinstance(error, dict) else None
            raise ValueError(message or "agent provider failed")
        raw_final_answer = provider_response.get("finalAnswer")
        if not isinstance(raw_final_answer, dict):
            error = ValueError("agent provider did not return a JSON object")
            provider_response = {
                **provider_response,
                "validation": {
                    "status": "failed",
                    "type": error.__class__.__name__,
                    "message": str(error),
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
            self._persist_claude_runtime_session(
                workspace_id, run_id, attempt, provider_response, response_audit
            )
            self._persist_failed_state(
                workspace_id,
                run_id,
                rounds,
                base_round,
                response_audit["artifact"]["id"],
                provider_response,
            )
            raise error
        if is_tool_call_request(raw_final_answer):
            try:
                tool_calls = normalize_tool_calls(
                    raw_final_answer,
                    allowed_tool_names=agent_allowed_tool_names(
                        self.settings,
                        state.get("interactionContext"),
                    ),
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
                self._persist_claude_runtime_session(
                    workspace_id, run_id, attempt, provider_response, response_audit
                )
                self._persist_failed_state(
                    workspace_id,
                    run_id,
                    rounds,
                    base_round,
                    response_audit["artifact"]["id"],
                    provider_response,
                )
                raise
            repeated_reason = self._repeated_action_reason(run_id, tool_calls)
            if repeated_reason:
                blocked_response = provider_response.get("response")
                if not isinstance(blocked_response, dict):
                    blocked_response = {}
                return {
                    "providerResponse": {
                        **provider_response,
                        "toolCalls": tool_calls,
                        "reason": repeated_reason,
                        "response": {
                            **blocked_response,
                            "type": "repeated_action_blocked",
                            "reason": repeated_reason,
                        },
                        "finalAnswer": self._budget_limited_final_answer(
                            state["workspace"],
                            state["evidenceBundle"],
                            state.get("interactionContext") or {},
                            repeated_reason,
                        ),
                    },
                    "baseRound": {
                        **base_round,
                        "budgetLimited": True,
                        "reason": repeated_reason,
                    },
                    "runtimeStatus": "validate_final_answer",
                }
            return {
                "providerResponse": provider_response,
                "toolCalls": tool_calls,
                "runtimeStatus": "execute_tool_calls",
            }
        return {
            "providerResponse": provider_response,
            "runtimeStatus": "validate_final_answer",
        }

    def _graph_after_provider_call(self, state: AgentGraphState) -> str:
        if state.get("runtimeStatus") == "execute_tool_calls":
            return "execute_tool_calls"
        return "validate_final_answer"

    def _graph_execute_tool_calls(self, state: AgentGraphState) -> AgentGraphState:
        workspace_id = state["workspaceId"]
        run_id = state["runId"]
        attempt = state["attempt"]
        rounds = list(state.get("rounds") or [])
        base_round = state.get("baseRound") or {}
        tool_calls = list(state.get("toolCalls") or [])
        existing_action_count = executed_tool_action_count(state.get("toolObservations") or [])
        remaining_actions = max(0, self.settings.agent_max_actions - existing_action_count)
        observations = self._execute_tool_calls(
            run_id,
            attempt,
            tool_calls,
            remaining_actions=remaining_actions,
        )
        waiting_status = waiting_status_from_observations(observations)
        tool_observations = [*(state.get("toolObservations") or []), *observations]
        executed_action_count = executed_tool_action_count(observations)
        action_budget_exhausted = (
            not waiting_status
            and len(tool_calls) > executed_action_count
            and existing_action_count + executed_action_count >= self.settings.agent_max_actions
        )
        validation = {"status": "tool_calls_executed"}
        if action_budget_exhausted:
            validation = {
                "status": "action_budget_exhausted",
                "reason": self._action_budget_reason(
                    existing_action_count + executed_action_count
                ),
            }
        provider_response = {
            **state["providerResponse"],
            "toolCalls": tool_calls,
            "toolObservations": observations,
            "validation": (
                {
                    "status": "paused",
                    "runtimeStatus": waiting_status,
                }
                if waiting_status
                else validation
            ),
        }
        token_usage = provider_token_usage(provider_response)
        response_audit = persist_agent_response(
            settings=self.settings,
            store=self.store,
            workspace_id=workspace_id,
            run_id=run_id,
            attempt=attempt,
            provider_response=provider_response,
            request_artifact_id=state["requestArtifactId"],
        )
        self._persist_claude_runtime_session(
            workspace_id, run_id, attempt, provider_response, response_audit
        )
        if waiting_status:
            round_update = {
                **base_round,
                "status": waiting_status,
                "responseArtifactId": response_audit["artifact"]["id"],
                "toolCallCount": len(tool_calls),
                "validation": {
                    "status": "paused",
                    "runtimeStatus": waiting_status,
                },
            }
            if token_usage:
                round_update["tokenUsage"] = token_usage
            rounds[-1] = round_update
            self._persist_state(
                workspace_id,
                run_id,
                status=waiting_status,
                phase=waiting_status,
                rounds=rounds,
                final_answer_status="waiting",
            )
            run = self.store.get_run(run_id)
            return {
                "toolObservations": tool_observations,
                "rounds": rounds,
                "waitingStatus": waiting_status,
                "runtimeStatus": waiting_status,
                "result": {
                    "graphRuntime": graph_runtime_metadata(),
                    "status": run["status"],
                    "phase": run["phase"],
                    "pendingActions": [
                        action
                        for action in self.store.list_actions(run_id)
                        if action.get("status") == "pending"
                    ],
                },
            }
        round_update = {
            **base_round,
            "status": validation["status"],
            "responseArtifactId": response_audit["artifact"]["id"],
            "toolCallCount": len(tool_calls),
            "validation": validation,
        }
        if token_usage:
            round_update["tokenUsage"] = token_usage
        rounds[-1] = round_update
        self._persist_state(
            workspace_id,
            run_id,
            status="running",
            phase="agent_round",
            rounds=rounds,
            final_answer_status="pending",
        )
        return {
            "toolObservations": tool_observations,
            "rounds": rounds,
            "responseArtifactId": response_audit["artifact"]["id"],
            "startedAtMonotonic": state.get("startedAtMonotonic"),
            "runtimeStatus": "prepare_agent_request",
        }

    def _graph_after_tool_calls(self, state: AgentGraphState) -> str:
        if state.get("waitingStatus") in {"waiting_for_user", "waiting_for_approval"}:
            return "end"
        return "prepare_agent_request"

    def _graph_validate_final_answer(self, state: AgentGraphState) -> AgentGraphState:
        workspace_id = state["workspaceId"]
        run_id = state["runId"]
        attempt = state["attempt"]
        rounds = list(state.get("rounds") or [])
        base_round = state.get("baseRound") or {}
        provider_response = state["providerResponse"]
        raw_final_answer = provider_response.get("finalAnswer")
        try:
            final_answer = normalize_and_validate_final_answer(
                self.settings,
                self.store,
                run_id,
                raw_final_answer,
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
                request_artifact_id=state["requestArtifactId"],
            )
            self._persist_claude_runtime_session(
                workspace_id, run_id, attempt, provider_response, response_audit
            )
            self._persist_failed_state(
                workspace_id,
                run_id,
                rounds,
                base_round,
                response_audit["artifact"]["id"],
                provider_response,
            )
            raise
        provider_response = {
            **provider_response,
            "validatedFinalAnswer": final_answer,
            "validation": (
                {
                    "status": "passed",
                    "budgetLimited": True,
                    "reason": base_round.get("reason"),
                }
                if base_round.get("budgetLimited")
                else {"status": "passed"}
            ),
        }
        token_usage = provider_token_usage(provider_response)
        response_audit = persist_agent_response(
            settings=self.settings,
            store=self.store,
            workspace_id=workspace_id,
            run_id=run_id,
            attempt=attempt,
            provider_response=provider_response,
            request_artifact_id=state["requestArtifactId"],
        )
        self._persist_claude_runtime_session(
            workspace_id, run_id, attempt, provider_response, response_audit
        )
        round_update = {
            **base_round,
            "status": "budget_limited" if base_round.get("budgetLimited") else "completed",
            "responseArtifactId": response_audit["artifact"]["id"],
            "validation": provider_response["validation"],
        }
        if token_usage:
            round_update["tokenUsage"] = token_usage
        rounds[-1] = round_update
        self._persist_state(
            workspace_id,
            run_id,
            status="succeeded",
            phase="finish",
            rounds=rounds,
            final_answer_status="validated",
        )
        return {
            "rounds": rounds,
            "responseArtifactId": response_audit["artifact"]["id"],
            "runtimeStatus": "final_answer_ready",
            "finalAnswer": final_answer,
        }

    def _graph_finalize_result(self, state: AgentGraphState) -> AgentGraphState:
        workspace_id = state["workspaceId"]
        run_id = state["runId"]
        workspace = state["workspace"]
        evidence_bundle = state["evidenceBundle"]
        final_answer = state["finalAnswer"]
        persist_run_result(self.settings, self.store, workspace_id, run_id, final_answer)
        alias = generate_run_alias(
            self.settings,
            workspace,
            final_answer,
            evidence_bundle,
        )
        self.store.update_run_status(run_id, "succeeded", "finish", final_answer, alias=alias)
        return {
            "runtimeStatus": "succeeded",
            "result": final_answer,
        }

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
            "graphRuntime": graph_runtime_metadata(),
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

    def _prepare_budget_limited_result(
        self,
        workspace_id: str,
        run_id: str,
        workspace: JsonObject,
        evidence_bundle: JsonObject,
        interaction_context: JsonObject,
        tool_observations: list[JsonObject],
        rounds: list[JsonObject],
        attempt: int,
        reason: str,
        analysis_package_artifact_id: str | None,
    ) -> AgentGraphState:
        provider_request = build_agent_provider_request(
            self.settings,
            workspace,
            evidence_bundle,
            tool_observations,
            interaction_context,
        )
        provider_request = {
            **provider_request,
            "provider": "budget_guard",
            "model": "logagent-v2-budget-guard",
            "transport": {"type": "internal"},
            "payload": {
                **provider_request.get("payload", {}),
                "budgetLimited": True,
                "reason": reason,
            },
        }
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
            "toolObservationCount": len(tool_observations),
            "budgetLimited": True,
            "reason": reason,
        }
        rounds.append({**base_round, "status": "budget_limited"})
        self._persist_state(
            workspace_id,
            run_id,
            status="running",
            phase="agent_round",
            rounds=rounds,
            final_answer_status="pending",
        )
        return {
            "attempt": attempt,
            "interactionContext": interaction_context,
            "toolObservations": tool_observations,
            "providerRequest": provider_request,
            "providerResponse": {
                "provider": provider_request["provider"],
                "model": provider_request["model"],
                "status": "completed",
                "reason": reason,
                "response": {
                    "type": "budget_limited",
                    "reason": reason,
                },
                "finalAnswer": self._budget_limited_final_answer(
                    workspace,
                    evidence_bundle,
                    interaction_context,
                    reason,
                ),
            },
            "requestArtifactId": request_artifact_id,
            "baseRound": base_round,
            "rounds": rounds,
            "runtimeStatus": "validate_final_answer",
        }

    def _analysis_budget_exhausted(
        self,
        run_id: str,
        completed_attempts: int,
        tool_observations: list[JsonObject],
        rounds: list[JsonObject],
        started_at_monotonic: object,
    ) -> str | None:
        if isinstance(started_at_monotonic, (int, float)):
            elapsed_seconds = int(max(0, time.monotonic() - started_at_monotonic))
            if elapsed_seconds >= self.settings.agent_max_runtime_seconds:
                return (
                    "analysis runtime budget exhausted: "
                    f"{elapsed_seconds}/{self.settings.agent_max_runtime_seconds} seconds"
                )
        if completed_attempts >= self.settings.agent_max_rounds:
            return (
                "analysis round budget exhausted: "
                f"{completed_attempts}/{self.settings.agent_max_rounds}"
            )
        if completed_attempts >= self.settings.agent_max_llm_calls:
            return (
                "LLM call budget exhausted: "
                f"{completed_attempts}/{self.settings.agent_max_llm_calls}"
            )
        token_count = total_token_usage(rounds)
        if token_count >= self.settings.agent_max_total_tokens:
            return (
                "token budget exhausted: "
                f"{token_count}/{self.settings.agent_max_total_tokens}"
            )
        user_prompt_count = user_input_action_count(self.store, run_id)
        if user_prompt_count >= self.settings.agent_max_user_prompts:
            return (
                "user prompt budget exhausted: "
                f"{user_prompt_count}/{self.settings.agent_max_user_prompts}"
            )
        approval_count = approval_action_count(self.store, run_id)
        if approval_count >= self.settings.agent_max_approvals:
            return (
                "approval budget exhausted: "
                f"{approval_count}/{self.settings.agent_max_approvals}"
            )
        action_count = executed_tool_action_count(tool_observations)
        if action_count >= self.settings.agent_max_actions:
            return self._action_budget_reason(action_count)
        return None

    def _action_budget_reason(self, action_count: int) -> str:
        return (
            "analysis action budget exhausted: "
            f"{action_count}/{self.settings.agent_max_actions}"
        )

    def _repeated_action_reason(
        self,
        run_id: str,
        tool_calls: list[JsonObject],
    ) -> str | None:
        counts: dict[str, int] = {}
        calls = read_mcp_calls(self.settings, self.store, run_id).get("calls", [])
        if isinstance(calls, list):
            for call in calls:
                if not isinstance(call, dict) or call.get("status") != "succeeded":
                    continue
                name = call.get("name")
                if not isinstance(name, str) or not name.startswith("logagent."):
                    continue
                fingerprint = tool_call_fingerprint(name, call.get("arguments") or {})
                counts[fingerprint] = counts.get(fingerprint, 0) + 1
        for tool_call in tool_calls:
            fingerprint = tool_call_fingerprint(tool_call["name"], tool_call["arguments"])
            count = counts.get(fingerprint, 0)
            if count >= self.settings.agent_max_repeated_action_fingerprints:
                return (
                    "repeated action fingerprint blocked: "
                    f"{fingerprint} {count}/"
                    f"{self.settings.agent_max_repeated_action_fingerprints}"
                )
            counts[fingerprint] = count + 1
        return None

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
            round_update = {
                **base_round,
                "status": "failed",
                "responseArtifactId": response_artifact_id,
                "error": provider_response.get("error"),
                "validation": provider_response.get("validation"),
            }
            token_usage = provider_token_usage(provider_response)
            if token_usage:
                round_update["tokenUsage"] = token_usage
            rounds[-1] = round_update
        self._persist_state(
            workspace_id,
            run_id,
            status="failed",
            phase="agent_round",
            rounds=rounds,
            final_answer_status="invalid",
        )

    def _persist_claude_runtime_session(
        self,
        workspace_id: str,
        run_id: str,
        attempt: int,
        provider_response: JsonObject,
        response_audit: JsonObject,
    ) -> None:
        artifact = response_audit.get("artifact")
        response_artifact_id = artifact.get("id") if isinstance(artifact, dict) else None
        if not isinstance(response_artifact_id, str):
            return
        persist_claude_runtime_session(
            settings=self.settings,
            store=self.store,
            workspace_id=workspace_id,
            run_id=run_id,
            attempt=attempt,
            provider_response=provider_response,
            response_artifact_id=response_artifact_id,
        )

    def _execute_tool_calls(
        self,
        run_id: str,
        attempt: int,
        tool_calls: list[JsonObject],
        *,
        remaining_actions: int,
    ) -> list[JsonObject]:
        run = self.store.get_run(run_id)
        observations = []
        for index, tool_call in enumerate(tool_calls[:remaining_actions]):
            name = tool_call["name"]
            arguments = tool_call["arguments"]
            result = call_task_tool(
                self.settings,
                self.store,
                run,
                {"name": name, "arguments": arguments},
            )
            parsed_result = parse_tool_result(result)
            persist_mcp_call(
                self.settings,
                self.store,
                run,
                name,
                arguments,
                "succeeded",
                parsed_result,
            )
            observations.append(
                {
                    "toolCallId": f"round_{attempt}_call_{index}",
                    "name": name,
                    "arguments": arguments,
                    "result": parsed_result,
                }
            )
            if waiting_status_from_observations(observations):
                break
        if not observations and tool_calls and remaining_actions <= 0:
            first_call = tool_calls[0]
            observations.append(
                {
                    "toolCallId": f"round_{attempt}_call_0",
                    "name": first_call["name"],
                    "arguments": first_call["arguments"],
                    "result": {
                        "error": {
                            "type": "budget_exhausted",
                            "message": self._action_budget_reason(
                                self.settings.agent_max_actions
                            ),
                        }
                    },
                }
            )
        return observations

    def _interaction_context(self, run_id: str) -> JsonObject:
        timeline = self.store.list_timeline(run_id)
        user_messages = [
            {
                "questionId": event.get("payload", {}).get("questionId"),
                "message": event.get("payload", {}).get("message"),
                "resumeMode": event.get("payload", {}).get("resumeMode"),
                "idempotencyKey": event.get("payload", {}).get("idempotencyKey"),
                "createdAt": event.get("created_at"),
            }
            for event in timeline
            if event.get("kind") == "user.message"
            and isinstance(event.get("payload"), dict)
            and isinstance(event["payload"].get("message"), str)
        ]
        actions = self.store.list_actions(run_id)
        action_results = [
            {
                "id": action.get("id"),
                "kind": action.get("kind"),
                "status": action.get("status"),
                "payload": action.get("payload", {}),
                "result": action.get("result"),
                "updatedAt": action.get("updated_at"),
            }
            for action in actions
            if action.get("status") != "pending"
        ]
        pending_actions = [
            {
                "id": action.get("id"),
                "kind": action.get("kind"),
                "payload": action.get("payload", {}),
                "createdAt": action.get("created_at"),
            }
            for action in actions
            if action.get("status") == "pending"
        ]
        context: JsonObject = {
            "userMessages": user_messages[-10:],
            "actionResults": action_results[-10:],
            "pendingActions": pending_actions[-10:],
        }
        claude_session_id = self._latest_claude_session_id(run_id)
        if claude_session_id:
            context["claudeSessionId"] = claude_session_id
        if user_messages and user_messages[-1].get("resumeMode") == "finalize":
            context["resumeDirective"] = "finalize_with_current_evidence"
        return context

    def _latest_claude_session_id(self, run_id: str) -> str | None:
        for evidence in reversed(self.store.list_evidence(run_id)):
            if evidence.get("kind") != "agent_response" or not evidence.get("artifact_id"):
                continue
            try:
                artifact = self.store.get_artifact(evidence["artifact_id"])
                path = resolve_artifact_path(self.settings, artifact["relative_path"])
                document = json.loads(path.read_text(encoding="utf-8"))
            except Exception:
                continue
            if document.get("provider") != "claude_code":
                continue
            response = document.get("response")
            session_id = response.get("sessionId") if isinstance(response, dict) else None
            if isinstance(session_id, str) and session_id.strip():
                return session_id.strip()
        return None

    def _stub_final_answer(
        self,
        workspace: JsonObject,
        evidence_bundle: JsonObject,
        interaction_context: JsonObject | None = None,
    ) -> JsonObject:
        interaction_context = interaction_context or {}
        user_messages = interaction_context.get("userMessages")
        last_message = (
            user_messages[-1] if isinstance(user_messages, list) and user_messages else None
        )
        manifest = evidence_bundle["manifest"]
        grep_results = evidence_bundle["grepResults"]
        matches = grep_results["matches"]
        if not manifest["files"]:
            missing_information = ["No current-task log evidence is available."]
            if interaction_context.get("resumeDirective") == "finalize_with_current_evidence":
                missing_information = []
            return {
                "summary": "V2 captured the question, but no supported text log files were uploaded.",
                "symptoms": [],
                "likelyRootCauses": [],
                "nextChecks": ["Upload .log/.txt files or supported .zip/.tar/.tar.gz packages."],
                "fixSuggestions": [],
                "missingInformation": missing_information,
                "confidence": "low",
                "evidenceRefs": [SESSION_TEXT_INPUT_REF],
                "userMessage": last_message.get("message") if isinstance(last_message, dict) else None,
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

    def _budget_limited_final_answer(
        self,
        workspace: JsonObject,
        evidence_bundle: JsonObject,
        interaction_context: JsonObject,
        reason: str,
    ) -> JsonObject:
        base = self._stub_final_answer(workspace, evidence_bundle, interaction_context)
        missing_information = [
            reason,
            *[
                item
                for item in base.get("missingInformation", [])
                if isinstance(item, str) and item.strip()
            ],
        ]
        next_checks = [
            "Resume the run with a higher Agent budget if deeper investigation is required.",
            *[
                item
                for item in base.get("nextChecks", [])
                if isinstance(item, str) and item.strip()
            ],
        ]
        return {
            **base,
            "summary": f"Analysis stopped because {reason}. {base['summary']}",
            "nextChecks": list(dict.fromkeys(next_checks)),
            "missingInformation": list(dict.fromkeys(missing_information)),
            "confidence": "low",
            "budgetLimited": True,
            "terminationReason": reason,
        }


def is_tool_call_request(value: JsonObject) -> bool:
    return value.get("type") == "tool_calls" or isinstance(value.get("toolCalls"), list)


def provider_token_usage(provider_response: JsonObject) -> JsonObject:
    usage = provider_response.get("usage")
    response = provider_response.get("response")
    if not isinstance(usage, dict) and isinstance(response, dict):
        usage = response.get("usage")
    if not isinstance(usage, dict):
        return {}
    total = token_count_from_usage(usage)
    if total <= 0:
        return {}
    return {"totalTokens": total, "raw": usage}


def token_count_from_usage(usage: JsonObject) -> int:
    for key in ("total_tokens", "totalTokens", "total"):
        value = usage.get(key)
        if isinstance(value, int) and not isinstance(value, bool) and value > 0:
            return value
    input_tokens = int_token_value(
        usage.get("input_tokens")
        or usage.get("prompt_tokens")
        or usage.get("inputTokens")
        or usage.get("promptTokens")
    )
    output_tokens = int_token_value(
        usage.get("output_tokens")
        or usage.get("completion_tokens")
        or usage.get("outputTokens")
        or usage.get("completionTokens")
    )
    return input_tokens + output_tokens


def int_token_value(value: object) -> int:
    if isinstance(value, int) and not isinstance(value, bool) and value > 0:
        return value
    return 0


def total_token_usage(rounds: list[JsonObject]) -> int:
    total = 0
    for round_item in rounds:
        usage = round_item.get("tokenUsage")
        if isinstance(usage, dict):
            total += int_token_value(usage.get("totalTokens"))
    return total


def user_input_action_count(store: Store, run_id: str) -> int:
    return action_kind_count(store, run_id, "user_input")


def approval_action_count(store: Store, run_id: str) -> int:
    return action_kind_count(store, run_id, "approval")


def action_kind_count(store: Store, run_id: str, kind: str) -> int:
    return sum(1 for action in store.list_actions(run_id) if action.get("kind") == kind)


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


def waiting_status_from_observations(observations: list[JsonObject]) -> str | None:
    for observation in observations:
        result = observation.get("result")
        if not isinstance(result, dict):
            continue
        runtime_status = result.get("runtimeStatus")
        if runtime_status in {"waiting_for_user", "waiting_for_approval"}:
            return str(runtime_status)
    return None


def executed_tool_action_count(observations: list[JsonObject]) -> int:
    count = 0
    for observation in observations:
        result = observation.get("result")
        if isinstance(result, dict):
            error = result.get("error")
            if isinstance(error, dict) and error.get("type") == "budget_exhausted":
                continue
        count += 1
    return count


def tool_call_fingerprint(name: str, arguments: JsonObject) -> str:
    payload = json.dumps(
        {"name": name, "arguments": arguments},
        ensure_ascii=True,
        sort_keys=True,
        separators=(",", ":"),
    )
    digest = sha256(payload.encode("utf-8")).hexdigest()[:16]
    return f"mcp_tool:{digest}"


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
