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

from .case_memory import case_tool_descriptors
from .config import Settings
from .fetch import fetch_tool_descriptors
from .metadata import metadata_tool_descriptors
from .skills import skill_tool_descriptors
from .store import JsonObject
from .tools import tool_descriptors

MAX_PROVIDER_RESPONSE_BYTES = 1024 * 1024
MAX_PROVIDER_PREVIEW_CHARS = 20000
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
    allowed_refs = allowed_evidence_refs(evidence_bundle)
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
            raw = response.read(MAX_PROVIDER_RESPONSE_BYTES)
            http_status = response.status
    except urllib.error.HTTPError as error:
        body = error.read(4096).decode("utf-8", errors="replace")
        return failed_provider_result(
            provider="openai_compatible",
            model=settings.agent_model,
            stage="http",
            error_type="HTTPError",
            message=f"agent provider returned HTTP {error.code}: {body}",
            response={"httpStatus": error.code, "bodyPreview": body},
        )
    except urllib.error.URLError as error:
        return failed_provider_result(
            provider="openai_compatible",
            model=settings.agent_model,
            stage="transport",
            error_type=error.__class__.__name__,
            message=str(error),
        )
    except Exception as error:
        return failed_provider_result(
            provider="openai_compatible",
            model=settings.agent_model,
            stage="transport",
            error_type=error.__class__.__name__,
            message=str(error),
        )

    raw_text = raw.decode("utf-8", errors="replace")
    response_payload: JsonObject = {
        "httpStatus": http_status,
        "bodyPreview": raw_text[:MAX_PROVIDER_PREVIEW_CHARS],
    }
    try:
        decoded = json.loads(raw_text)
        if isinstance(decoded, dict):
            response_payload["json"] = decoded
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
        "allowedEvidenceRefs": [item["ref"] for item in evidence_preview if item.get("ref")],
        "evidencePreview": evidence_preview,
        "backgroundEvidence": evidence_bundle.get("backgroundEvidence", []),
        "toolObservations": tool_observations or [],
        "interactionContext": interaction_context or {},
        "availableTools": agent_available_tools(settings),
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


def agent_allowed_tool_names(settings: Settings) -> set[str]:
    return {tool["name"] for tool in agent_available_tools(settings)}


def agent_available_tools(settings: Settings) -> list[JsonObject]:
    tools = [
        search_logs_tool_descriptor(),
        get_log_slice_tool_descriptor(),
        *metadata_tool_descriptors(),
        *case_tool_descriptors(),
        *skill_tool_descriptors(),
    ]
    if any(
        tool["enabled"] and tool.get("source") == "configured"
        for tool in tool_descriptors(settings)
    ):
        tools.append(run_domain_tool_descriptor(settings))
    fetch_tools = fetch_tool_descriptors()
    tools.append(fetch_tools[0])
    if settings.fetch_enabled:
        tools.append(fetch_tools[1])
    return tools


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
                }
            },
            "required": ["keywords"],
            "additionalProperties": False,
        },
    }


def get_log_slice_tool_descriptor() -> JsonObject:
    return {
        "name": "logagent.get_log_slice",
        "description": "Read bounded context around a current Workspace log path.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": {"type": "string", "minLength": 1},
                "lineNumber": {"type": "integer", "minimum": 1},
                "before": {"type": "integer", "minimum": 0, "maximum": 50, "default": 5},
                "after": {"type": "integer", "minimum": 0, "maximum": 50, "default": 5},
            },
            "required": ["path", "lineNumber"],
            "additionalProperties": False,
        },
    }


def run_domain_tool_descriptor(settings: Settings) -> JsonObject:
    return {
        "name": "logagent.run_domain_tool",
        "description": "Run a configured read-only diagnostic tool by toolId.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "toolId": {
                    "type": "string",
                    "enum": [
                        tool["toolId"]
                        for tool in tool_descriptors(settings)
                        if tool["enabled"] and tool.get("source") == "configured"
                    ],
                },
                "params": {"type": "object"},
            },
            "required": ["toolId"],
            "additionalProperties": False,
        },
    }


def allowed_evidence_refs(evidence_bundle: JsonObject) -> list[str]:
    grep_results = evidence_bundle.get("grepResults", {})
    matches = grep_results.get("matches", [])
    if not isinstance(matches, list):
        return []
    return [
        match["ref"]
        for match in matches[:20]
        if isinstance(match, dict) and isinstance(match.get("ref"), str)
    ]


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
) -> JsonObject:
    result: JsonObject = {
        "provider": provider,
        "model": model,
        "status": "failed",
        "error": {
            "stage": stage,
            "type": error_type,
            "message": message[:4000],
        },
    }
    if response is not None:
        result["response"] = response
    return result


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
