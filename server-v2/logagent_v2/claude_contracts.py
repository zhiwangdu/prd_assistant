from __future__ import annotations

import json

from .artifacts import write_artifact_bytes
from .config import Settings
from .store import JsonObject, Store, now_iso


CLAUDE_PROMPT_PATH = "claude_prompt.md"
CLAUDE_MCP_CONFIG_PATH = "claude_mcp_config.json"
CLAUDE_SESSION_PATH = "claude_session.json"


def persist_claude_contracts(
    settings: Settings,
    store: Store,
    workspace_id: str,
    run_id: str,
    analysis_package_artifact_id: str | None,
) -> JsonObject:
    prompt = build_claude_prompt(run_id)
    prompt_artifact = write_artifact_bytes(
        settings=settings,
        store=store,
        workspace_id=workspace_id,
        filename=CLAUDE_PROMPT_PATH,
        data=prompt.encode("utf-8"),
        content_type="text/markdown",
        schema_name="logagent.v2.claude_prompt.v1",
        preview={
            "path": CLAUDE_PROMPT_PATH,
            "runId": run_id,
            "analysisPackageArtifactId": analysis_package_artifact_id,
        },
    )
    prompt_evidence = store.create_evidence(
        workspace_id=workspace_id,
        run_id=run_id,
        kind="claude_prompt",
        final_allowed=False,
        summary="Claude Code startup prompt contract captured.",
        payload={
            "artifactId": prompt_artifact["id"],
            "path": CLAUDE_PROMPT_PATH,
            "analysisPackageArtifactId": analysis_package_artifact_id,
        },
        artifact_id=prompt_artifact["id"],
    )

    mcp_config = build_claude_mcp_config(settings, run_id)
    config_artifact = write_artifact_bytes(
        settings=settings,
        store=store,
        workspace_id=workspace_id,
        filename=CLAUDE_MCP_CONFIG_PATH,
        data=json.dumps(mcp_config, ensure_ascii=True, indent=2).encode("utf-8"),
        content_type="application/json",
        schema_name="logagent.v2.claude_mcp_config.v1",
        preview={
            "path": CLAUDE_MCP_CONFIG_PATH,
            "runId": run_id,
            "server": "logagent",
            "transport": "http",
        },
    )
    config_evidence = store.create_evidence(
        workspace_id=workspace_id,
        run_id=run_id,
        kind="claude_mcp_config",
        final_allowed=False,
        summary="Claude Code MCP config contract captured.",
        payload={
            "artifactId": config_artifact["id"],
            "path": CLAUDE_MCP_CONFIG_PATH,
            "authEnv": "LOGAGENT_V2_API_KEY",
        },
        artifact_id=config_artifact["id"],
    )

    session = build_claude_session_contract(
        settings,
        run_id,
        analysis_package_artifact_id=analysis_package_artifact_id,
    )
    session_artifact = write_artifact_bytes(
        settings=settings,
        store=store,
        workspace_id=workspace_id,
        filename=CLAUDE_SESSION_PATH,
        data=json.dumps(session, ensure_ascii=True, indent=2).encode("utf-8"),
        content_type="application/json",
        schema_name="logagent.v2.claude_session.v1",
        preview={
            "path": CLAUDE_SESSION_PATH,
            "runId": run_id,
            "runtimeStatus": session["runtimeStatus"],
            "providerRuntime": session["providerRuntime"],
        },
    )
    session_evidence = store.create_evidence(
        workspace_id=workspace_id,
        run_id=run_id,
        kind="claude_session",
        final_allowed=False,
        summary="Claude Code session contract captured.",
        payload={
            "artifactId": session_artifact["id"],
            "path": CLAUDE_SESSION_PATH,
            "runtimeStatus": session["runtimeStatus"],
        },
        artifact_id=session_artifact["id"],
    )

    return {
        "prompt": {"artifact": prompt_artifact, "evidence": prompt_evidence},
        "mcpConfig": {"artifact": config_artifact, "evidence": config_evidence},
        "session": {"artifact": session_artifact, "evidence": session_evidence},
    }


def build_claude_mcp_config(settings: Settings, run_id: str) -> JsonObject:
    return {
        "mcpServers": {
            "logagent": {
                "type": "http",
                "url": f"{server_base_url(settings)}/api/v2/mcp/task/{run_id}",
                "headers": {
                    "Authorization": "Bearer ${LOGAGENT_V2_API_KEY}",
                },
            }
        },
        "notes": {
            "auth": (
                "LOGAGENT_V2_API_KEY is referenced as an environment placeholder; "
                "the resolved API key is not written to this artifact."
            ),
        },
    }


def build_claude_prompt(run_id: str) -> str:
    return "\n".join(
        [
            "You are Claude Code running as the LogAgent V2 diagnostic layer.",
            "",
            "Use the configured LogAgent MCP server for task evidence. Start with "
            "`resources/list`, then read the `analysis_package` resource for this run.",
            "",
            f"Run id: `{run_id}`",
            "",
            "Use LogAgent MCP tools for log search, log slices, metadata, case recall, "
            "skill references, fetch, and configured domain tools. Do not invent "
            "evidence refs.",
            "",
            "If `analysis_package.analysisState.finalizeRequested` is true, do not "
            "request more user input; return the best final answer possible from "
            "current evidence.",
            "",
            "Return exactly one JSON object following the LogAgent final answer or "
            "waiting-state protocol.",
            "",
        ]
    )


def build_claude_session_contract(
    settings: Settings,
    run_id: str,
    analysis_package_artifact_id: str | None,
) -> JsonObject:
    return {
        "schemaVersion": 1,
        "runtimeStatus": "contract_ready",
        "runId": run_id,
        "providerRuntime": settings.agent_provider,
        "createdAt": now_iso(),
        "analysisPackageArtifactId": analysis_package_artifact_id,
        "mcpConfigPath": CLAUDE_MCP_CONFIG_PATH,
        "promptPath": CLAUDE_PROMPT_PATH,
        "note": claude_session_note(settings),
    }


def server_base_url(settings: Settings) -> str:
    host = settings.host
    if host in {"0.0.0.0", "::", ""}:
        host = "127.0.0.1"
    if ":" in host and not host.startswith("["):
        host = f"[{host}]"
    return f"http://{host}:{settings.port}"


def claude_session_note(settings: Settings) -> str:
    if settings.agent_provider == "claude_code":
        return (
            "V2 generated Claude Code task contracts and will launch the configured "
            "Claude Code CLI provider for Agent rounds."
        )
    return (
        "V2 generated Claude Code-compatible task contracts. The in-process Agent "
        "provider loop may execute instead of launching Claude Code CLI."
    )
