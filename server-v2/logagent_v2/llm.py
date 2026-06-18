from __future__ import annotations

import json
import os
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any

from .case_memory import task_case_tool_descriptors
from .claude_contracts import (
    CLAUDE_MCP_CONFIG_PATH,
    CLAUDE_PROMPT_PATH,
    build_claude_mcp_config,
    build_claude_prompt,
)
from .code_evidence import (
    code_diff_tool_descriptor,
    code_evidence_available,
    code_evidence_tool_descriptor,
)
from .config import ClaudeCodePermissionProfile, Settings, claude_code_profile_for_mode
from .fetch import fetch_tool_descriptors
from .metadata import task_metadata_tool_descriptors
from .skills import skill_tool_descriptors
from .store import JsonObject
from .tools import runnable_configured_tool_ids

MAX_PROVIDER_RESPONSE_BYTES = 1024 * 1024
MAX_PROVIDER_PREVIEW_CHARS = 20000
SESSION_TEXT_INPUT_REF = "session_text_input.json#question"
PROVIDER_AUDIT_RESPONSE_HEADERS = {
    "x-request-id": "xRequestId",
    "request-id": "requestId",
    "openai-request-id": "openaiRequestId",
    "anthropic-request-id": "anthropicRequestId",
    "x-correlation-id": "xCorrelationId",
    "x-amzn-requestid": "xAmznRequestId",
    "cf-ray": "cfRay",
    "openai-processing-ms": "openaiProcessingMs",
}
PROVIDER_REQUEST_ID_HEADER_KEYS = (
    "xRequestId",
    "requestId",
    "openaiRequestId",
    "anthropicRequestId",
    "xCorrelationId",
    "xAmznRequestId",
    "cfRay",
)
_DEBUG_LOCK = threading.Lock()
_DEBUG_LOG_RESPONSES = False


def debug_log_responses() -> bool:
    with _DEBUG_LOCK:
        return _DEBUG_LOG_RESPONSES


def set_debug_log_responses(enabled: bool) -> bool:
    global _DEBUG_LOG_RESPONSES
    with _DEBUG_LOCK:
        _DEBUG_LOG_RESPONSES = bool(enabled)
        return _DEBUG_LOG_RESPONSES


def log_provider_response_content(content: str) -> None:
    if debug_log_responses():
        print(f"[logagent-v2] agent provider response content: {content}", file=sys.stderr)


def generate_agent_final_answer(
    settings: Settings,
    workspace: JsonObject,
    evidence_bundle: JsonObject,
    tool_observations: list[JsonObject] | None = None,
) -> JsonObject | None:
    result = generate_agent_provider_result(
        settings, workspace, evidence_bundle, tool_observations
    )
    if result.get("status") == "skipped":
        return None
    if result.get("status") != "completed":
        error = result.get("error") if isinstance(result.get("error"), dict) else {}
        raise ValueError(error.get("message") or "agent provider failed")
    final_answer = result.get("finalAnswer")
    if not isinstance(final_answer, dict):
        raise ValueError("agent provider did not return a final answer")
    return final_answer


def generate_agent_provider_result(
    settings: Settings,
    workspace: JsonObject,
    evidence_bundle: JsonObject,
    tool_observations: list[JsonObject] | None = None,
) -> JsonObject:
    request_payload = build_agent_provider_request(
        settings, workspace, evidence_bundle, tool_observations
    )
    return execute_agent_provider_request(settings, request_payload)


def build_agent_provider_request(
    settings: Settings,
    workspace: JsonObject,
    evidence_bundle: JsonObject,
    tool_observations: list[JsonObject] | None = None,
    interaction_context: JsonObject | None = None,
) -> JsonObject:
    provider = (settings.agent_provider or "stub").lower()
    prompt = build_agent_prompt(
        settings, workspace, evidence_bundle, tool_observations, interaction_context
    )
    allowed_refs = allowed_evidence_refs(evidence_bundle, tool_observations)
    if provider == "stub":
        return {
            "provider": "stub",
            "model": None,
            "transport": {"type": "local_stub"},
            "payload": {"prompt": prompt},
            "allowedEvidenceRefs": allowed_refs,
        }
    if provider == "openai_compatible":
        url = None
        if settings.agent_base_url:
            url = urllib.parse.urljoin(
                settings.agent_base_url.rstrip("/") + "/", "chat/completions"
            )
        return {
            "provider": "openai_compatible",
            "model": settings.agent_model,
            "transport": {
                "type": "openai_chat_completions",
                "url": sanitize_url(url),
                "timeoutSeconds": settings.agent_timeout_seconds,
            },
            "payload": {
                "model": settings.agent_model,
                "temperature": 0,
                "max_tokens": settings.agent_max_output_tokens,
                "messages": [
                    {
                        "role": "system",
                        "content": (
                            "You are LogAgent V2. Return only one JSON object. Either request "
                            "allowed tool calls using the provided protocol or return the final "
                            "answer schema. Use only provided evidence refs."
                        ),
                    },
                    {
                        "role": "user",
                        "content": prompt,
                    },
                ],
            },
            "allowedEvidenceRefs": allowed_refs,
        }
    if provider == "binary":
        return {
            "provider": "binary",
            "model": settings.agent_model or "binary-reserved",
            "transport": {
                "type": "local_binary",
                "binaryPathConfigured": settings.agent_binary_path is not None,
                "timeoutSeconds": settings.agent_timeout_seconds,
                "maxOutputBytes": settings.agent_binary_max_output_bytes,
            },
            "payload": {"prompt": prompt},
            "allowedEvidenceRefs": allowed_refs,
        }
    if provider == "claude_code":
        run_id = evidence_bundle.get("runId")
        analysis_mode = workspace.get("mode") or "diagnose"
        analysis_language = workspace.get("language") or "zh-CN"
        profile = claude_code_profile_for_mode(settings, analysis_mode)
        claude_prompt = (
            build_claude_prompt(
                str(run_id),
                analysis_mode=str(analysis_mode),
                analysis_language=str(analysis_language),
                permission_profile=profile,
            )
            if isinstance(run_id, str)
            else ""
        )
        resume_session_id = (
            interaction_context.get("claudeSessionId")
            if isinstance(interaction_context, dict)
            else None
        )
        return {
            "provider": "claude_code",
            "model": settings.agent_model or "claude-code-cli",
            "transport": {
                "type": "claude_code_cli",
                "commandPathConfigured": settings.claude_code_path is not None,
                "timeoutSeconds": settings.agent_timeout_seconds,
                "maxOutputBytes": settings.claude_code_max_output_bytes,
                "analysisMode": profile.name,
                "permissionProfile": profile.to_json(),
                "permissionMode": profile.permission_mode,
                "tools": profile.tools,
                "allowedTools": list(profile.allowed_tools),
                "disallowedTools": list(profile.disallowed_tools),
                "nativeToolPolicy": native_tool_policy(profile),
                "mcpConfigPath": CLAUDE_MCP_CONFIG_PATH,
                "promptPath": CLAUDE_PROMPT_PATH,
                "resumeSessionConfigured": isinstance(resume_session_id, str)
                and bool(resume_session_id.strip()),
            },
            "payload": {
                "prompt": claude_prompt,
                "runId": run_id,
                "resumeSessionId": resume_session_id,
                "promptDelivery": {
                    "mode": "stdin_file",
                    "largeContextVia": "mcp_resource",
                    "resource": "analysis_package",
                },
                "analysisMode": profile.name,
            },
            "allowedEvidenceRefs": allowed_refs,
        }
    return {
        "provider": provider,
        "model": settings.agent_model,
        "transport": {"type": "unsupported"},
        "payload": {"prompt": prompt},
        "allowedEvidenceRefs": allowed_refs,
    }


def execute_agent_provider_request(settings: Settings, request_payload: JsonObject) -> JsonObject:
    provider = str(request_payload.get("provider") or "stub").lower()
    if provider == "stub":
        return {
            "provider": "stub",
            "model": None,
            "status": "skipped",
            "reason": "local stub final answer will be generated by AgentRuntime",
        }
    if provider == "openai_compatible":
        return call_openai_compatible(settings, request_payload)
    if provider == "binary":
        return call_binary_provider(settings, request_payload)
    if provider == "claude_code":
        return call_claude_code_provider(settings, request_payload)
    return failed_provider_result(
        provider=provider,
        model=request_payload.get("model"),
        stage="configuration",
        error_type="ValueError",
        message=f"unsupported LOGAGENT_V2_AGENT_PROVIDER {settings.agent_provider}",
    )


def call_openai_compatible(
    settings: Settings,
    request_payload: JsonObject,
) -> JsonObject:
    if not settings.agent_base_url:
        return failed_provider_result(
            provider="openai_compatible",
            model=request_payload.get("model"),
            stage="configuration",
            error_type="ValueError",
            message="LOGAGENT_V2_AGENT_BASE_URL is required",
        )
    if not settings.agent_model:
        return failed_provider_result(
            provider="openai_compatible",
            model=request_payload.get("model"),
            stage="configuration",
            error_type="ValueError",
            message="LOGAGENT_V2_AGENT_MODEL is required",
        )
    url = urllib.parse.urljoin(settings.agent_base_url.rstrip("/") + "/", "chat/completions")
    payload = request_payload.get("payload")
    if not isinstance(payload, dict):
        payload = {}
    headers = {"Content-Type": "application/json"}
    if settings.agent_api_key:
        headers["Authorization"] = f"Bearer {settings.agent_api_key}"
    request = urllib.request.Request(
        url,
        data=json.dumps(payload, ensure_ascii=True).encode("utf-8"),
        headers=headers,
        method="POST",
    )
    try:
        with urllib.request.urlopen(
            request, timeout=settings.agent_timeout_seconds
        ) as response:
            response_headers = provider_response_headers(response.headers)
            raw = response.read(MAX_PROVIDER_RESPONSE_BYTES)
            http_status = response.status
    except urllib.error.HTTPError as error:
        body = error.read(4096).decode("utf-8", errors="replace")
        response_payload: JsonObject = {"httpStatus": error.code, "bodyPreview": body}
        add_provider_header_audit(
            response_payload,
            provider_response_headers(error.headers),
        )
        error_classification = provider_http_error_classification(error.code)
        return failed_provider_result(
            provider="openai_compatible",
            model=settings.agent_model,
            stage="http",
            error_type="HTTPError",
            message=f"agent provider returned HTTP {error.code}: {body}",
            response=response_payload,
            classification=error_classification["classification"],
            retryable=error_classification["retryable"],
            http_status=error.code,
        )
    except urllib.error.URLError as error:
        return failed_provider_result(
            provider="openai_compatible",
            model=settings.agent_model,
            stage="transport",
            error_type=error.__class__.__name__,
            message=str(error),
            classification="network_error",
            retryable=True,
        )
    except Exception as error:
        return failed_provider_result(
            provider="openai_compatible",
            model=settings.agent_model,
            stage="transport",
            error_type=error.__class__.__name__,
            message=str(error),
            classification="transport_error",
            retryable=True,
        )

    raw_text = raw.decode("utf-8", errors="replace")
    response_payload: JsonObject = {
        "httpStatus": http_status,
        "bodyPreview": raw_text[:MAX_PROVIDER_PREVIEW_CHARS],
    }
    add_provider_header_audit(response_payload, response_headers)
    try:
        decoded = json.loads(raw_text)
        if isinstance(decoded, dict):
            response_payload["json"] = decoded
            add_openai_response_audit(response_payload, decoded)
        content = extract_chat_content(decoded)
        log_provider_response_content(content)
        response_payload["contentPreview"] = content[:MAX_PROVIDER_PREVIEW_CHARS]
        final_answer = parse_final_answer_content(content)
    except Exception as error:
        return failed_provider_result(
            provider="openai_compatible",
            model=settings.agent_model,
            stage="parse",
            error_type=error.__class__.__name__,
            message=str(error),
            response=response_payload,
        )
    return {
        "provider": "openai_compatible",
        "model": settings.agent_model,
        "status": "completed",
        "response": response_payload,
        "finalAnswer": final_answer,
    }


def call_binary_provider(
    settings: Settings,
    request_payload: JsonObject,
) -> JsonObject:
    binary_path = settings.agent_binary_path
    model = request_payload.get("model") or settings.agent_model or "binary-reserved"
    if binary_path is None:
        return failed_provider_result(
            provider="binary",
            model=model,
            stage="configuration",
            error_type="ValueError",
            message="LOGAGENT_V2_AGENT_BINARY_PATH is required",
        )
    validation_error = validate_binary_path(binary_path)
    if validation_error is not None:
        return failed_provider_result(
            provider="binary",
            model=model,
            stage="configuration",
            error_type="ValueError",
            message=validation_error,
        )
    payload = request_payload.get("payload")
    prompt = payload.get("prompt") if isinstance(payload, dict) else None
    if not isinstance(prompt, str) or not prompt.strip():
        return failed_provider_result(
            provider="binary",
            model=model,
            stage="configuration",
            error_type="ValueError",
            message="binary provider prompt is required",
        )
    started = time.monotonic()
    try:
        completed = subprocess.run(
            [binary_path.as_posix(), "run", prompt],
            capture_output=True,
            timeout=settings.agent_timeout_seconds,
            check=False,
        )
    except subprocess.TimeoutExpired:
        return failed_provider_result(
            provider="binary",
            model=model,
            stage="timeout",
            error_type="TimeoutExpired",
            message=f"binary provider timed out after {settings.agent_timeout_seconds}s",
            response={"timeoutSeconds": settings.agent_timeout_seconds, "cmd": binary_argv_preview(binary_path)},
        )
    except OSError as error:
        return failed_provider_result(
            provider="binary",
            model=model,
            stage="transport",
            error_type=error.__class__.__name__,
            message=f"failed to start binary provider: {error}",
            response={"cmd": binary_argv_preview(binary_path)},
        )
    duration_ms = int((time.monotonic() - started) * 1000)
    stdout = completed.stdout
    stderr = completed.stderr
    response_payload: JsonObject = {
        "exitCode": completed.returncode,
        "durationMs": duration_ms,
        "cmd": binary_argv_preview(binary_path),
        "stderrPreview": stderr[:MAX_PROVIDER_PREVIEW_CHARS].decode("utf-8", errors="replace"),
    }
    if completed.returncode != 0:
        return failed_provider_result(
            provider="binary",
            model=model,
            stage="process",
            error_type="NonZeroExit",
            message=f"binary provider exited with code {completed.returncode}",
            response=response_payload,
        )
    if len(stdout) > settings.agent_binary_max_output_bytes:
        return failed_provider_result(
            provider="binary",
            model=model,
            stage="output",
            error_type="OutputTooLarge",
            message=(
                "binary provider stdout exceeded "
                f"{settings.agent_binary_max_output_bytes} bytes"
            ),
            response=response_payload,
        )
    try:
        raw_text = stdout.decode("utf-8")
    except UnicodeDecodeError as error:
        return failed_provider_result(
            provider="binary",
            model=model,
            stage="decode",
            error_type=error.__class__.__name__,
            message=str(error),
            response=response_payload,
        )
    response_payload["stdoutPreview"] = raw_text[:MAX_PROVIDER_PREVIEW_CHARS]
    try:
        log_provider_response_content(raw_text)
        final_answer = parse_final_answer_content(raw_text)
    except Exception as error:
        return failed_provider_result(
            provider="binary",
            model=model,
            stage="parse",
            error_type=error.__class__.__name__,
            message=str(error),
            response=response_payload,
        )
    return {
        "provider": "binary",
        "model": model,
        "status": "completed",
        "response": response_payload,
        "finalAnswer": final_answer,
    }


def call_claude_code_provider(
    settings: Settings,
    request_payload: JsonObject,
) -> JsonObject:
    claude_path = settings.claude_code_path
    model = request_payload.get("model") or settings.agent_model or "claude-code-cli"
    if claude_path is None:
        return failed_provider_result(
            provider="claude_code",
            model=model,
            stage="configuration",
            error_type="ValueError",
            message=(
                "LOGAGENT_V2_CLAUDE_CODE_PATH or LOGAGENT_CLAUDE_CODE_PATH is required"
            ),
        )
    validation_error = validate_claude_code_path(claude_path)
    if validation_error is not None:
        return failed_provider_result(
            provider="claude_code",
            model=model,
            stage="configuration",
            error_type="ValueError",
            message=validation_error,
        )
    payload = request_payload.get("payload")
    prompt = payload.get("prompt") if isinstance(payload, dict) else None
    run_id = payload.get("runId") if isinstance(payload, dict) else None
    analysis_mode = payload.get("analysisMode") if isinstance(payload, dict) else None
    profile = claude_code_profile_for_mode(settings, analysis_mode)
    resume_session_id = (
        payload.get("resumeSessionId") if isinstance(payload, dict) else None
    )
    if isinstance(resume_session_id, str):
        resume_session_id = resume_session_id.strip() or None
    else:
        resume_session_id = None
    if not isinstance(run_id, str) or not run_id.strip():
        return failed_provider_result(
            provider="claude_code",
            model=model,
            stage="configuration",
            error_type="ValueError",
            message="claude_code provider runId is required",
        )
    if not isinstance(prompt, str) or not prompt.strip():
        prompt = build_claude_prompt(
            run_id,
            analysis_mode=profile.name,
            permission_profile=profile,
        )
    session_dir = claude_code_session_dir(settings, run_id)
    session_dir.mkdir(parents=True, exist_ok=True)
    (session_dir / CLAUDE_PROMPT_PATH).write_text(prompt, encoding="utf-8")
    (session_dir / CLAUDE_MCP_CONFIG_PATH).write_text(
        json.dumps(build_claude_mcp_config(settings, run_id), ensure_ascii=True, indent=2),
        encoding="utf-8",
    )
    env = os.environ.copy()
    env["LOGAGENT_V2_API_KEY"] = settings.api_key
    started = time.monotonic()
    try:
        completed = subprocess.run(
            claude_code_argv(settings, claude_path, profile, resume_session_id),
            input=prompt.encode("utf-8"),
            cwd=session_dir,
            env=env,
            capture_output=True,
            timeout=settings.agent_timeout_seconds,
            check=False,
        )
    except subprocess.TimeoutExpired:
        return failed_provider_result(
            provider="claude_code",
            model=model,
            stage="timeout",
            error_type="TimeoutExpired",
            message=f"Claude Code provider timed out after {settings.agent_timeout_seconds}s",
            response={
                "timeoutSeconds": settings.agent_timeout_seconds,
                "cmd": claude_code_argv_preview(
                    settings,
                    profile,
                    bool(resume_session_id),
                ),
                "workDir": "<claude_session_dir>",
            },
        )
    except OSError as error:
        return failed_provider_result(
            provider="claude_code",
            model=model,
            stage="transport",
            error_type=error.__class__.__name__,
            message=f"failed to start Claude Code provider: {error}",
            response={
                "cmd": claude_code_argv_preview(
                    settings,
                    profile,
                    bool(resume_session_id),
                ),
                "workDir": "<claude_session_dir>",
            },
        )
    duration_ms = int((time.monotonic() - started) * 1000)
    stdout = completed.stdout
    stderr = completed.stderr
    response_payload: JsonObject = {
        "exitCode": completed.returncode,
        "durationMs": duration_ms,
        "cmd": claude_code_argv_preview(settings, profile, bool(resume_session_id)),
        "workDir": "<claude_session_dir>",
        "promptPath": CLAUDE_PROMPT_PATH,
        "mcpConfigPath": CLAUDE_MCP_CONFIG_PATH,
        "analysisMode": profile.name,
        "permissionProfile": profile.name,
        "nativeToolPolicy": native_tool_policy(profile),
        "stderrPreview": stderr[:MAX_PROVIDER_PREVIEW_CHARS].decode(
            "utf-8", errors="replace"
        ),
    }
    if completed.returncode != 0:
        return failed_provider_result(
            provider="claude_code",
            model=model,
            stage="process",
            error_type="NonZeroExit",
            message=f"Claude Code provider exited with code {completed.returncode}",
            response=response_payload,
        )
    if len(stdout) > settings.claude_code_max_output_bytes:
        return failed_provider_result(
            provider="claude_code",
            model=model,
            stage="output",
            error_type="OutputTooLarge",
            message=(
                "Claude Code provider stdout exceeded "
                f"{settings.claude_code_max_output_bytes} bytes"
            ),
            response=response_payload,
        )
    try:
        raw_text = stdout.decode("utf-8")
    except UnicodeDecodeError as error:
        return failed_provider_result(
            provider="claude_code",
            model=model,
            stage="decode",
            error_type=error.__class__.__name__,
            message=str(error),
            response=response_payload,
        )
    response_payload["stdoutPreview"] = raw_text[:MAX_PROVIDER_PREVIEW_CHARS]
    try:
        log_provider_response_content(raw_text)
        parsed = parse_claude_session_output(raw_text)
    except Exception as error:
        return failed_provider_result(
            provider="claude_code",
            model=model,
            stage="parse",
            error_type=error.__class__.__name__,
            message=str(error),
            response=response_payload,
        )
    response_payload["runtimeStatus"] = parsed["runtimeStatus"]
    if parsed.get("sessionId"):
        response_payload["sessionId"] = parsed["sessionId"]
    if resume_session_id:
        response_payload["resumedSessionId"] = resume_session_id
    if parsed.get("usage") is not None:
        response_payload["usage"] = parsed["usage"]
    if parsed.get("cost") is not None:
        response_payload["cost"] = parsed["cost"]
    if parsed["runtimeStatus"] in {"waiting_for_user", "waiting_for_approval"}:
        return {
            "provider": "claude_code",
            "model": model,
            "status": "completed",
            "response": response_payload,
            "finalAnswer": claude_waiting_to_tool_calls(parsed),
        }
    return {
        "provider": "claude_code",
        "model": model,
        "status": "completed",
        "response": response_payload,
        "finalAnswer": parsed["finalAnswer"],
    }


def validate_binary_path(binary_path: Path) -> str | None:
    if not binary_path.is_absolute():
        return "LOGAGENT_V2_AGENT_BINARY_PATH must be an absolute path"
    if not binary_path.is_file():
        return "LOGAGENT_V2_AGENT_BINARY_PATH is not a regular file"
    if not os.access(binary_path, os.X_OK):
        return "LOGAGENT_V2_AGENT_BINARY_PATH is not executable"
    return None


def binary_argv_preview(_binary_path: Path) -> list[str]:
    return ["<binary_path>", "run", "<prompt>"]


def native_tool_policy(profile: ClaudeCodePermissionProfile) -> JsonObject:
    return {
        "permissionMode": profile.permission_mode,
        "tools": profile.tools,
        "allowedTools": list(profile.allowed_tools),
        "disallowedTools": list(profile.disallowed_tools),
        "nativeBash": profile.native_bash,
        "nativeEdit": profile.native_edit,
        "worktreeRequired": profile.worktree_required,
    }


def validate_claude_code_path(claude_path: Path) -> str | None:
    if not claude_path.is_absolute():
        return "LOGAGENT_V2_CLAUDE_CODE_PATH must be an absolute path"
    if not claude_path.is_file():
        return "LOGAGENT_V2_CLAUDE_CODE_PATH is not a regular file"
    if not os.access(claude_path, os.X_OK):
        return "LOGAGENT_V2_CLAUDE_CODE_PATH is not executable"
    return None


def claude_code_session_dir(settings: Settings, run_id: str) -> Path:
    return settings.tmp_dir / "claude_sessions" / safe_session_segment(run_id)


def safe_session_segment(value: str) -> str:
    return "".join(
        char if char.isascii() and (char.isalnum() or char in {"_", "-"}) else "_"
        for char in value
    )[:160] or "run"


def claude_code_argv(
    settings: Settings,
    claude_path: Path,
    profile: ClaudeCodePermissionProfile,
    resume_session_id: str | None = None,
) -> list[str]:
    argv = [
        claude_path.as_posix(),
        "--print",
        "--output-format",
        "json",
        "--json-schema",
        json.dumps(claude_session_json_schema(), ensure_ascii=True),
        "--mcp-config",
        CLAUDE_MCP_CONFIG_PATH,
        "--strict-mcp-config",
        "--permission-mode",
        profile.permission_mode,
        "--tools",
        profile.tools,
    ]
    if profile.allowed_tools:
        argv.extend(["--allowedTools", ",".join(profile.allowed_tools)])
    if profile.disallowed_tools:
        argv.extend(["--disallowedTools", ",".join(profile.disallowed_tools)])
    if resume_session_id:
        argv.extend(["--resume", resume_session_id])
    return argv


def claude_code_argv_preview(
    settings: Settings,
    profile: ClaudeCodePermissionProfile,
    include_resume: bool = False,
) -> list[str]:
    argv = [
        "<claude_code_path>",
        "--print",
        "--output-format",
        "json",
        "--json-schema",
        "<json_schema>",
        "--mcp-config",
        CLAUDE_MCP_CONFIG_PATH,
        "--strict-mcp-config",
        "--permission-mode",
        profile.permission_mode,
        "--tools",
        profile.tools,
    ]
    if profile.allowed_tools:
        argv.extend(["--allowedTools", ",".join(profile.allowed_tools)])
    if profile.disallowed_tools:
        argv.extend(["--disallowedTools", ",".join(profile.disallowed_tools)])
    if include_resume:
        argv.extend(["--resume", "<session_id>"])
    return argv


def claude_session_json_schema() -> JsonObject:
    final_answer_schema: JsonObject = {
        "type": "object",
        "additionalProperties": True,
        "properties": {
            "summary": {"type": "string"},
            "symptoms": {"type": "array", "items": {"type": "string"}},
            "likelyRootCauses": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": True,
                    "properties": {
                        "cause": {"type": "string"},
                        "evidenceRefs": {
                            "type": "array",
                            "items": {"type": "string"},
                        },
                    },
                    "required": ["cause", "evidenceRefs"],
                },
            },
            "nextChecks": {"type": "array", "items": {"type": "string"}},
            "fixSuggestions": {"type": "array", "items": {"type": "string"}},
            "missingInformation": {"type": "array", "items": {"type": "string"}},
            "confidence": {"type": "string", "enum": ["low", "medium", "high"]},
            "evidenceRefs": {"type": "array", "items": {"type": "string"}},
        },
        "required": [
            "summary",
            "symptoms",
            "likelyRootCauses",
            "nextChecks",
            "fixSuggestions",
            "missingInformation",
            "confidence",
            "evidenceRefs",
        ],
    }
    return {
        "type": "object",
        "additionalProperties": True,
        "properties": {
            "runtimeStatus": {
                "type": "string",
                "enum": [
                    "completed",
                    "succeeded",
                    "final_answer",
                    "waiting_for_user",
                    "waiting_for_approval",
                ],
            },
            "finalAnswer": final_answer_schema,
            "pendingPrompt": {"type": ["object", "string"]},
            "pendingApproval": {"type": ["object", "string"]},
        },
        "required": ["runtimeStatus"],
    }


def parse_claude_session_output(content: str) -> JsonObject:
    decoded = json.loads(content.strip())
    if not isinstance(decoded, dict):
        raise ValueError("Claude Code output must be a JSON object")
    if decoded.get("is_error") is True:
        result = decoded.get("result")
        raise ValueError(f"Claude Code returned an error: {result}")
    for key in ("structured_output", "structuredOutput", "result"):
        if key in decoded:
            candidate = decoded[key]
            break
    else:
        candidate = decoded
    structured = decode_claude_structured_output(candidate)
    runtime_status = structured.get("runtimeStatus") or structured.get("runtime_status")
    session_id = decoded.get("session_id") or decoded.get("sessionId")
    telemetry = claude_envelope_telemetry(decoded)
    if runtime_status in {"completed", "succeeded", "final_answer"}:
        final_answer = (
            structured["finalAnswer"]
            if "finalAnswer" in structured
            else structured.get("final_answer")
        )
        if not isinstance(final_answer, dict):
            raise ValueError("Claude Code completed output requires finalAnswer")
        return {
            "runtimeStatus": "completed",
            "finalAnswer": final_answer,
            "sessionId": session_id,
            **telemetry,
        }
    if runtime_status == "waiting_for_user":
        pending = (
            structured["pendingPrompt"]
            if "pendingPrompt" in structured
            else structured.get("pending_prompt")
        )
        if pending is None:
            raise ValueError("Claude Code waiting_for_user output requires pendingPrompt")
        return {
            "runtimeStatus": "waiting_for_user",
            "pendingPrompt": pending,
            "sessionId": session_id,
            **telemetry,
        }
    if runtime_status == "waiting_for_approval":
        pending = (
            structured["pendingApproval"]
            if "pendingApproval" in structured
            else structured.get("pending_approval")
        )
        if pending is None:
            raise ValueError(
                "Claude Code waiting_for_approval output requires pendingApproval"
            )
        return {
            "runtimeStatus": "waiting_for_approval",
            "pendingApproval": pending,
            "sessionId": session_id,
            **telemetry,
        }
    raise ValueError(f"unsupported Claude Code runtimeStatus: {runtime_status}")


def claude_envelope_telemetry(decoded: JsonObject) -> JsonObject:
    result: JsonObject = {}
    usage = decoded.get("usage")
    if isinstance(usage, dict):
        result["usage"] = usage
    cost_usd = (
        decoded.get("total_cost_usd")
        if "total_cost_usd" in decoded
        else decoded.get("totalCostUsd", decoded.get("cost_usd"))
    )
    if isinstance(cost_usd, (int, float)) and not isinstance(cost_usd, bool):
        result["cost"] = {"usd": cost_usd}
    return result


def decode_claude_structured_output(candidate: Any) -> JsonObject:
    if isinstance(candidate, dict):
        return candidate
    if isinstance(candidate, str):
        text = candidate.strip()
        if text.startswith("```"):
            text = strip_json_fence(text)
        decoded = json.loads(text)
        if isinstance(decoded, dict):
            return decoded
    raise ValueError("Claude Code structured output must be a JSON object")


def claude_waiting_to_tool_calls(parsed: JsonObject) -> JsonObject:
    if parsed["runtimeStatus"] == "waiting_for_user":
        return {
            "type": "tool_calls",
            "toolCalls": [
                {
                    "name": "logagent.request_user_input",
                    "arguments": normalize_pending_prompt(parsed.get("pendingPrompt")),
                }
            ],
        }
    return {
        "type": "tool_calls",
        "toolCalls": [
            {
                "name": "logagent.request_approval",
                "arguments": normalize_pending_approval(parsed.get("pendingApproval")),
            }
        ],
    }


def normalize_pending_prompt(value: Any) -> JsonObject:
    if isinstance(value, str):
        return {"question": value}
    if not isinstance(value, dict):
        raise ValueError("pendingPrompt must be an object or string")
    question = value.get("question") or value.get("message") or value.get("prompt")
    if not isinstance(question, str) or not question.strip():
        raise ValueError("pendingPrompt requires question")
    arguments: JsonObject = {"question": question.strip()}
    optional_fields = {
        "questionId": value.get("questionId") or value.get("question_id"),
        "reason": value.get("reason"),
        "answerFormat": value.get("answerFormat") or value.get("answer_format"),
    }
    for key, item in optional_fields.items():
        if isinstance(item, str) and item.strip():
            arguments[key] = item.strip()
    if "required" in value:
        arguments["required"] = bool(value["required"])
    return arguments


def normalize_pending_approval(value: Any) -> JsonObject:
    if isinstance(value, str):
        return {"reason": value}
    if not isinstance(value, dict):
        raise ValueError("pendingApproval must be an object or string")
    reason = value.get("reason") or value.get("message") or value.get("summary")
    if not isinstance(reason, str) or not reason.strip():
        raise ValueError("pendingApproval requires reason")
    arguments: JsonObject = {"reason": reason.strip()}
    action_type = value.get("actionType") or value.get("action_type")
    if isinstance(action_type, str) and action_type.strip():
        arguments["actionType"] = action_type.strip()
    input_value = value.get("input")
    if isinstance(input_value, dict):
        arguments["input"] = input_value
    evidence_refs = value.get("evidenceRefs") or value.get("evidence_refs")
    if isinstance(evidence_refs, list):
        arguments["evidenceRefs"] = [
            item for item in evidence_refs if isinstance(item, str)
        ]
    return arguments


def build_agent_prompt(
    settings: Settings,
    workspace: JsonObject,
    evidence_bundle: JsonObject,
    tool_observations: list[JsonObject] | None = None,
    interaction_context: JsonObject | None = None,
) -> str:
    manifest = evidence_bundle.get("manifest", {})
    grep_results = evidence_bundle.get("grepResults", {})
    matches = grep_results.get("matches", [])
    evidence_preview = [
        {
            "ref": match.get("ref"),
            "path": match.get("path"),
            "lineNumber": match.get("lineNumber"),
            "text": match.get("text"),
        }
        for match in matches[:20]
        if isinstance(match, dict)
    ]
    prompt = {
        "question": workspace.get("question"),
        "mode": workspace.get("mode"),
        "language": workspace.get("language"),
        "manifest": {
            "fileCount": manifest.get("fileCount"),
            "uploadCount": manifest.get("uploadCount"),
        },
        "allowedEvidenceRefs": allowed_evidence_refs(evidence_bundle, tool_observations),
        "evidencePreview": evidence_preview,
        "backgroundEvidence": evidence_bundle.get("backgroundEvidence", []),
        "preRunToolResults": evidence_bundle.get("toolResults", []),
        "toolObservations": tool_observations or [],
        "interactionContext": interaction_context or {},
        "availableTools": agent_available_tools(settings, interaction_context),
        "resumePolicy": agent_resume_policy(interaction_context),
        "responseProtocol": {
            "finalAnswer": "Return the final answer JSON object directly.",
            "toolCalls": {
                "type": "tool_calls",
                "toolCalls": [
                    {
                        "name": "logagent.search_logs",
                        "arguments": {"keywords": ["timeout"]},
                    }
                ],
            },
        },
        "requiredSchema": {
            "summary": "string",
            "symptoms": ["string"],
            "likelyRootCauses": [{"cause": "string", "evidenceRefs": ["string"]}],
            "nextChecks": ["string"],
            "fixSuggestions": ["string"],
            "missingInformation": ["string"],
            "confidence": "low|medium|high",
            "evidenceRefs": ["string"],
        },
    }
    return json.dumps(prompt, ensure_ascii=True, indent=2)


def agent_allowed_tool_names(
    settings: Settings, interaction_context: JsonObject | None = None
) -> set[str]:
    return {tool["name"] for tool in agent_available_tools(settings, interaction_context)}


def agent_available_tools(
    settings: Settings, interaction_context: JsonObject | None = None
) -> list[JsonObject]:
    tools = [
        search_logs_tool_descriptor(),
        get_log_slice_tool_descriptor(),
        *task_metadata_tool_descriptors(),
        *task_case_tool_descriptors(),
        *skill_tool_descriptors(),
    ]
    if code_evidence_available(settings):
        tools.append(code_evidence_tool_descriptor())
        tools.append(code_diff_tool_descriptor())
    if not agent_resume_policy(interaction_context)["finalizeWithCurrentEvidence"]:
        tools.extend(
            [request_user_input_tool_descriptor(), request_approval_tool_descriptor()]
        )
    if agent_domain_tool_ids(settings):
        tools.append(run_domain_tool_descriptor(settings))
    fetch_tools = fetch_tool_descriptors()
    tools.append(fetch_tools[0])
    if settings.fetch_enabled:
        tools.append(fetch_tools[1])
    return tools


def agent_resume_policy(interaction_context: JsonObject | None = None) -> JsonObject:
    finalize = (
        isinstance(interaction_context, dict)
        and interaction_context.get("resumeDirective") == "finalize_with_current_evidence"
    )
    return {
        "finalizeWithCurrentEvidence": finalize,
        "allowWaitingTools": not finalize,
        "instruction": (
            "The user asked to finalize with current evidence. Do not request more "
            "user input or approval-gated actions; return the final answer schema."
            if finalize
            else (
                "If essential information is missing, use logagent.request_user_input. "
                "If an approval-gated action is needed, use logagent.request_approval."
            )
        ),
    }


def request_user_input_tool_descriptor() -> JsonObject:
    return {
        "name": "logagent.request_user_input",
        "description": "Pause this run and ask the user for more information.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "questionId": {"type": "string", "minLength": 1},
                "question": {"type": "string", "minLength": 1},
                "reason": {"type": "string"},
                "required": {"type": "boolean", "default": True},
                "answerFormat": {"type": "string"},
            },
            "required": ["question"],
            "additionalProperties": False,
        },
    }


def request_approval_tool_descriptor() -> JsonObject:
    return {
        "name": "logagent.request_approval",
        "description": "Pause this run and request approval for a gated action.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "actionType": {"type": "string", "minLength": 1},
                "reason": {"type": "string", "minLength": 1},
                "input": {"type": "object"},
                "evidenceRefs": {"type": "array", "items": {"type": "string"}},
            },
            "required": ["reason"],
            "additionalProperties": False,
        },
    }


def search_logs_tool_descriptor() -> JsonObject:
    return {
        "name": "logagent.search_logs",
        "description": "Search current Workspace logs for one or more keywords.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "keywords": {
                    "type": "array",
                    "items": {"type": "string", "minLength": 1},
                    "minItems": 1,
                    "maxItems": 16,
                },
                "maxMatches": {"type": "integer", "minimum": 1, "maximum": 200},
            },
            "required": ["keywords"],
            "additionalProperties": False,
        },
    }


def get_log_slice_tool_descriptor() -> JsonObject:
    return {
        "name": "logagent.get_log_slice",
        "description": "Read bounded context lines from a current Workspace log path.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": {"type": "string", "minLength": 1},
                "lineNumber": {"type": "integer", "minimum": 1},
                "before": {"type": "integer", "minimum": 0, "maximum": 50, "default": 5},
                "after": {"type": "integer", "minimum": 0, "maximum": 50, "default": 5},
                "startLine": {"type": "integer", "minimum": 1},
                "endLine": {"type": "integer", "minimum": 1},
            },
            "required": ["path"],
            "anyOf": [
                {"required": ["lineNumber"]},
                {"required": ["startLine", "endLine"]},
            ],
            "additionalProperties": False,
        },
    }


def run_domain_tool_descriptor(settings: Settings) -> JsonObject:
    configured_tool_ids = agent_domain_tool_ids(settings)
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
            "anyOf": [
                {"required": ["toolId"]},
                {"required": ["tool", "inputFile"]},
            ],
            "additionalProperties": False,
        },
    }


def agent_domain_tool_ids(settings: Settings) -> list[str]:
    return runnable_configured_tool_ids(settings)


def allowed_evidence_refs(
    evidence_bundle: JsonObject,
    tool_observations: list[JsonObject] | None = None,
) -> list[str]:
    grep_results = evidence_bundle.get("grepResults", {})
    matches = grep_results.get("matches", [])
    if not isinstance(matches, list):
        initial_refs = [SESSION_TEXT_INPUT_REF]
    else:
        initial_refs = [
            SESSION_TEXT_INPUT_REF,
            *[
                match["ref"]
                for match in matches[:20]
                if isinstance(match, dict) and isinstance(match.get("ref"), str)
            ],
        ]
    refs: list[str] = []
    for ref in [
        *initial_refs,
        *collect_tool_evidence_refs(evidence_bundle.get("toolResults") or []),
        *tool_observation_evidence_refs(tool_observations or []),
    ]:
        if ref not in refs:
            refs.append(ref)
    return refs


def tool_observation_evidence_refs(tool_observations: list[JsonObject]) -> list[str]:
    refs: list[str] = []
    for observation in tool_observations:
        for ref in collect_tool_evidence_refs(observation):
            if ref not in refs:
                refs.append(ref)
    return refs


def collect_tool_evidence_refs(value: object) -> list[str]:
    refs: list[str] = []
    if isinstance(value, dict):
        for key, child in value.items():
            if key in {"evidenceRefs", "finalEvidenceRefs"} and isinstance(child, list):
                refs.extend(item for item in child if isinstance(item, str))
                continue
            if key in {"ref", "evidenceRef"} and isinstance(child, str):
                refs.append(child)
                continue
            refs.extend(collect_tool_evidence_refs(child))
    elif isinstance(value, list):
        for item in value:
            refs.extend(collect_tool_evidence_refs(item))
    return refs


def sanitize_url(url: str | None) -> str | None:
    if not url:
        return None
    parsed = urllib.parse.urlsplit(url)
    hostname = parsed.hostname or ""
    netloc = hostname
    if parsed.port is not None:
        netloc = f"{netloc}:{parsed.port}"
    return urllib.parse.urlunsplit((parsed.scheme, netloc, parsed.path, "", ""))


def failed_provider_result(
    provider: str,
    model: object,
    stage: str,
    error_type: str,
    message: str,
    response: JsonObject | None = None,
    classification: str | None = None,
    retryable: bool | None = None,
    http_status: int | None = None,
) -> JsonObject:
    default_metadata = provider_failure_classification(stage, error_type)
    if classification is None:
        classification = default_metadata.get("classification")
    if retryable is None:
        retryable = default_metadata.get("retryable")
    error_payload: JsonObject = {
        "stage": stage,
        "type": error_type,
        "message": message[:4000],
    }
    if classification is not None:
        error_payload["classification"] = classification
    if retryable is not None:
        error_payload["retryable"] = retryable
    if http_status is not None:
        error_payload["httpStatus"] = http_status
    result: JsonObject = {
        "provider": provider,
        "model": model,
        "status": "failed",
        "error": error_payload,
    }
    if response is not None:
        result["response"] = response
    return result


def provider_failure_classification(stage: str, error_type: str) -> JsonObject:
    if stage == "configuration":
        return {"classification": "configuration_error", "retryable": False}
    if stage == "timeout":
        return {"classification": "provider_timeout", "retryable": True}
    if stage == "transport":
        return {"classification": "transport_error", "retryable": True}
    if stage == "process":
        return {"classification": "provider_process_error", "retryable": False}
    if stage == "output":
        if error_type == "OutputTooLarge":
            return {"classification": "output_too_large", "retryable": False}
        return {"classification": "provider_output_error", "retryable": False}
    if stage == "decode":
        return {"classification": "output_decode_error", "retryable": False}
    if stage == "parse":
        return {"classification": "output_parse_error", "retryable": False}
    if stage == "http":
        return {"classification": "provider_http_error", "retryable": False}
    return {"classification": "provider_error", "retryable": False}


def provider_http_error_classification(status_code: int) -> JsonObject:
    if status_code in {401, 403}:
        return {"classification": "authentication_failed", "retryable": False}
    if status_code == 429:
        return {"classification": "rate_limited", "retryable": True}
    if status_code in {408, 504}:
        return {"classification": "provider_timeout", "retryable": True}
    if status_code == 413:
        return {"classification": "input_too_large", "retryable": False}
    if 500 <= status_code <= 599:
        return {"classification": "provider_server_error", "retryable": True}
    if 400 <= status_code <= 499:
        return {"classification": "provider_client_error", "retryable": False}
    return {"classification": "provider_http_error", "retryable": False}


def provider_response_headers(headers: Any) -> JsonObject:
    result: JsonObject = {}
    if headers is None:
        return result
    for source_name, output_name in PROVIDER_AUDIT_RESPONSE_HEADERS.items():
        value = headers.get(source_name) if hasattr(headers, "get") else None
        if isinstance(value, str) and value.strip():
            result[output_name] = value.strip()[:200]
    return result


def add_provider_header_audit(response_payload: JsonObject, headers: JsonObject) -> None:
    if not headers:
        return
    response_payload["providerRequestHeaders"] = headers
    if response_payload.get("providerRequestId"):
        return
    for key in PROVIDER_REQUEST_ID_HEADER_KEYS:
        value = headers.get(key)
        if isinstance(value, str) and value:
            response_payload["providerRequestId"] = value
            return


def add_openai_response_audit(response_payload: JsonObject, decoded: JsonObject) -> None:
    response_id = decoded.get("id")
    if isinstance(response_id, str) and response_id.strip():
        response_payload["providerResponseId"] = response_id.strip()
        response_payload.setdefault("providerRequestId", response_id.strip())
    response_model = decoded.get("model")
    if isinstance(response_model, str) and response_model.strip():
        response_payload["responseModel"] = response_model.strip()
    system_fingerprint = decoded.get("system_fingerprint")
    if isinstance(system_fingerprint, str) and system_fingerprint.strip():
        response_payload["systemFingerprint"] = system_fingerprint.strip()
    usage = decoded.get("usage")
    if isinstance(usage, dict):
        response_payload["usage"] = usage
    finish_reason = first_choice_finish_reason(decoded)
    if finish_reason is not None:
        response_payload["finishReason"] = finish_reason


def first_choice_finish_reason(decoded: JsonObject) -> str | None:
    choices = decoded.get("choices")
    if not isinstance(choices, list) or not choices:
        return None
    first = choices[0]
    if not isinstance(first, dict):
        return None
    finish_reason = first.get("finish_reason")
    if isinstance(finish_reason, str):
        return finish_reason
    return None


def extract_chat_content(response: Any) -> str:
    if not isinstance(response, dict):
        raise ValueError("agent provider response must be an object")
    choices = response.get("choices")
    if not isinstance(choices, list) or not choices:
        raise ValueError("agent provider response has no choices")
    first = choices[0]
    if not isinstance(first, dict):
        raise ValueError("agent provider choice is invalid")
    message = first.get("message")
    if not isinstance(message, dict) or not isinstance(message.get("content"), str):
        raise ValueError("agent provider message content is missing")
    return message["content"]


def parse_final_answer_content(content: str) -> JsonObject:
    stripped = content.strip()
    if stripped.startswith("```"):
        stripped = strip_json_fence(stripped)
    value = json.loads(stripped)
    if not isinstance(value, dict):
        raise ValueError("agent final answer must be a JSON object")
    return value


def strip_json_fence(value: str) -> str:
    lines = value.splitlines()
    if lines and lines[0].strip().startswith("```"):
        lines = lines[1:]
    if lines and lines[-1].strip() == "```":
        lines = lines[:-1]
    return "\n".join(lines).strip()
