from __future__ import annotations

import json
import urllib.error
import urllib.parse
import urllib.request
from typing import Any

from .agent_graph import graph_runtime_metadata
from .config import Settings
from .llm import (
    MAX_PROVIDER_RESPONSE_BYTES,
    call_binary_provider,
    extract_chat_content,
    log_provider_response_content,
    validate_binary_path,
)
from .store import JsonObject

MAX_SETTINGS_MESSAGE_CHARS = 20000


def llm_settings_summary(settings: Settings) -> JsonObject:
    return {
        "provider": normalized_provider(settings),
        "configuredModel": configured_model(settings),
        "maxInputChars": MAX_SETTINGS_MESSAGE_CHARS,
        "maxOutputTokens": settings.agent_max_output_tokens,
        "requestTimeoutSeconds": settings.agent_timeout_seconds,
        "baseUrlConfigured": bool(settings.agent_base_url),
        "apiKeyConfigured": bool(settings.agent_api_key),
        "binaryPathConfigured": bool(settings.agent_binary_path),
        "binaryMaxOutputBytes": settings.agent_binary_max_output_bytes,
    }


def list_agent_models(settings: Settings) -> JsonObject:
    provider = normalized_provider(settings)
    model = configured_model(settings)
    if provider == "stub":
        return {
            "provider": "stub",
            "configuredModel": model,
            "models": [model],
            "raw": {"data": [{"id": model, "object": "model"}]},
        }
    if provider == "openai_compatible":
        url = openai_base_url(settings, "models")
        headers = auth_headers(settings)
        request = urllib.request.Request(url, headers=headers, method="GET")
        with urllib.request.urlopen(request, timeout=settings.agent_timeout_seconds) as response:
            raw = response.read(MAX_PROVIDER_RESPONSE_BYTES)
            decoded = json.loads(raw.decode("utf-8", errors="replace"))
        return {
            "provider": provider,
            "configuredModel": model,
            "models": extract_model_ids(decoded),
            "raw": decoded,
        }
    if provider == "binary":
        return {
            "provider": "binary",
            "configuredModel": model,
            "models": [model],
            "raw": {"data": [{"id": model, "object": "model"}]},
        }
    raise ValueError(f"unsupported LOGAGENT_V2_AGENT_PROVIDER {settings.agent_provider}")


def test_agent_chat(settings: Settings, message: str) -> JsonObject:
    provider = normalized_provider(settings)
    model = configured_model(settings)
    if provider == "stub":
        response = f"stub agent provider acknowledged: {message}"
        return {"provider": "stub", "model": model, "response": response}
    if provider == "openai_compatible":
        if not settings.agent_model:
            raise ValueError("LOGAGENT_V2_AGENT_MODEL is required")
        url = openai_base_url(settings, "chat/completions")
        payload = {
            "model": settings.agent_model,
            "temperature": 0,
            "max_tokens": settings.agent_max_output_tokens,
            "messages": [
                {
                    "role": "system",
                    "content": (
                        "You are a LogAgent V2 settings connectivity test. "
                        "Reply briefly with the user message acknowledged."
                    ),
                },
                {"role": "user", "content": message},
            ],
        }
        request = urllib.request.Request(
            url,
            data=json.dumps(payload, ensure_ascii=True).encode("utf-8"),
            headers={**auth_headers(settings), "Content-Type": "application/json"},
            method="POST",
        )
        with urllib.request.urlopen(request, timeout=settings.agent_timeout_seconds) as response:
            raw = response.read(MAX_PROVIDER_RESPONSE_BYTES)
        decoded = json.loads(raw.decode("utf-8", errors="replace"))
        content = extract_chat_content(decoded)
        log_provider_response_content(content)
        return {"provider": provider, "model": settings.agent_model, "response": content}
    if provider == "binary":
        prompt = json.dumps(
            {
                "task": "settings_connectivity_test",
                "message": message,
                "requiredResponse": {
                    "summary": "string",
                    "symptoms": ["string"],
                    "likelyRootCauses": [],
                    "nextChecks": ["string"],
                    "fixSuggestions": ["string"],
                    "missingInformation": ["string"],
                    "confidence": "low|medium|high",
                    "evidenceRefs": [],
                },
            },
            ensure_ascii=True,
        )
        result = call_binary_provider(
            settings,
            {
                "provider": "binary",
                "model": model,
                "payload": {"prompt": prompt},
            },
        )
        if result.get("status") != "completed":
            error = result.get("error") if isinstance(result.get("error"), dict) else {}
            raise ValueError(error.get("message") or "binary provider failed")
        final_answer = result.get("finalAnswer")
        response = final_answer.get("summary") if isinstance(final_answer, dict) else ""
        return {"provider": provider, "model": model, "response": response, "raw": result}
    raise ValueError(f"unsupported LOGAGENT_V2_AGENT_PROVIDER {settings.agent_provider}")


def agent_backends_summary(settings: Settings) -> JsonObject:
    provider = normalized_provider(settings)
    graph_runtime = graph_runtime_metadata()
    return {
        "defaultBackend": "logagent_v2_agent",
        "backends": [
            {
                "id": "logagent_v2_agent",
                "backendType": "langgraph_oriented_agent",
                "graphRuntime": graph_runtime,
                "enabled": True,
                "defaultBackend": True,
                "commandConfigured": agent_backend_configured(settings),
                "timeoutSeconds": settings.agent_timeout_seconds,
                "maxInputBytes": 0,
                "maxOutputBytes": MAX_PROVIDER_RESPONSE_BYTES,
                "executionMode": f"{provider}_tool_loop",
                "defaultMode": "diagnose",
                "permissionProfile": "server_owned_readonly_tools",
            }
        ],
    }


def agent_backend_diagnostic(settings: Settings, backend_id: str) -> JsonObject:
    if backend_id != "logagent_v2_agent":
        raise ValueError(f"unknown V2 agent backend {backend_id}")
    provider = normalized_provider(settings)
    graph_runtime = graph_runtime_metadata()
    details = [
        (
            "V2 runs execute inside the FastAPI worker through a LangGraph "
            f"{graph_runtime['graph']} state graph."
        ),
        f"Provider={provider}, timeout={settings.agent_timeout_seconds}s, "
        f"maxRounds={settings.agent_max_rounds}.",
    ]
    if provider == "stub":
        details.append("Stub provider is local and requires no external configuration.")
        status = "configured"
    elif provider == "openai_compatible":
        missing = []
        if not settings.agent_base_url:
            missing.append("LOGAGENT_V2_AGENT_BASE_URL")
        if not settings.agent_model:
            missing.append("LOGAGENT_V2_AGENT_MODEL")
        if missing:
            raise ValueError(f"missing required Agent provider setting(s): {', '.join(missing)}")
        details.append("OpenAI-compatible provider has base URL and model configured.")
        if settings.agent_api_key:
            details.append("API key is configured through environment and is not returned.")
        else:
            details.append("API key is not configured; this is only valid for unauthenticated endpoints.")
        status = "configured"
    elif provider == "binary":
        if settings.agent_binary_path is None:
            raise ValueError("missing required Agent provider setting(s): LOGAGENT_V2_AGENT_BINARY_PATH")
        validation_error = validate_binary_path(settings.agent_binary_path)
        if validation_error is not None:
            raise ValueError(validation_error)
        details.append("Binary provider path is valid and invoked without a shell.")
        details.append(
            f"Binary max output bytes={settings.agent_binary_max_output_bytes}."
        )
        status = "configured"
    else:
        raise ValueError(f"unsupported LOGAGENT_V2_AGENT_PROVIDER {settings.agent_provider}")
    return {
        "backendId": "logagent_v2_agent",
        "backendType": "langgraph_oriented_agent",
        "enabled": True,
        "status": status,
        "executionMode": f"{provider}_tool_loop",
        "graphRuntime": graph_runtime,
        "details": details,
    }


def domain_adapter_summaries() -> list[JsonObject]:
    return [
        {
            "id": "opengemini_influxdb",
            "displayName": "openGemini / InfluxDB",
            "status": "active",
            "products": ["openGemini", "InfluxDB"],
            "evidenceKinds": [
                "metadata_context",
                "log_search",
                "tool_result",
                "fetch_result",
            ],
            "plannedTools": [
                "influxql_analyzer",
                "flux_query_analyzer",
                "opengemini_storage_analyzer",
                "influxdb_storage_analyzer",
            ],
            "notes": [
                "Active adapter for Metadata topology, query analyzers, and storage analyzers.",
                "Tool execution remains constrained by V2 tool descriptors and task MCP validation.",
            ],
        },
        {
            "id": "cassandra",
            "displayName": "Cassandra",
            "status": "skeleton",
            "products": ["Cassandra"],
            "evidenceKinds": ["log_search", "system_context"],
            "plannedTools": ["nodetool", "sstablemetadata"],
            "notes": [
                "Skeleton adapter for ring ownership, repair, compaction, tombstones, and latency evidence."
            ],
        },
        {
            "id": "rocksdb",
            "displayName": "RocksDB",
            "status": "skeleton",
            "products": ["RocksDB"],
            "evidenceKinds": ["log_search", "system_context"],
            "plannedTools": ["ldb", "sst_dump"],
            "notes": [
                "Skeleton adapter for LOG, MANIFEST, OPTIONS, SST, compaction, flush, and write-stall evidence."
            ],
        },
    ]


def test_response(callable_result: Any) -> JsonObject:
    try:
        return {"ok": True, "result": callable_result(), "error": None}
    except (urllib.error.HTTPError, urllib.error.URLError, TimeoutError, ValueError) as error:
        return {"ok": False, "result": None, "error": str(error)}
    except Exception as error:
        return {"ok": False, "result": None, "error": f"{error.__class__.__name__}: {error}"}


test_agent_chat.__test__ = False
test_response.__test__ = False


def validate_settings_message(message: str) -> str:
    normalized = message.strip()
    if not normalized:
        raise ValueError("message must not be empty")
    if len(normalized) > MAX_SETTINGS_MESSAGE_CHARS:
        raise ValueError(f"message exceeds max input chars {MAX_SETTINGS_MESSAGE_CHARS}")
    return normalized


def normalized_provider(settings: Settings) -> str:
    return (settings.agent_provider or "stub").lower()


def configured_model(settings: Settings) -> str:
    if settings.agent_model:
        return settings.agent_model
    if normalized_provider(settings) == "stub":
        return "stub"
    if normalized_provider(settings) == "binary":
        return "binary-reserved"
    return ""


def agent_backend_configured(settings: Settings) -> bool:
    provider = normalized_provider(settings)
    if provider == "stub":
        return True
    if provider == "openai_compatible":
        return bool(settings.agent_base_url and settings.agent_model)
    if provider == "binary":
        return bool(
            settings.agent_binary_path
            and validate_binary_path(settings.agent_binary_path) is None
        )
    return False


def openai_base_url(settings: Settings, suffix: str) -> str:
    if not settings.agent_base_url:
        raise ValueError("LOGAGENT_V2_AGENT_BASE_URL is required")
    return urllib.parse.urljoin(settings.agent_base_url.rstrip("/") + "/", suffix)


def auth_headers(settings: Settings) -> dict[str, str]:
    if not settings.agent_api_key:
        return {}
    return {"Authorization": f"Bearer {settings.agent_api_key}"}


def extract_model_ids(decoded: Any) -> list[str]:
    if not isinstance(decoded, dict):
        return []
    data = decoded.get("data")
    if not isinstance(data, list):
        return []
    models = []
    for item in data:
        if isinstance(item, dict) and isinstance(item.get("id"), str):
            models.append(item["id"])
    return models
