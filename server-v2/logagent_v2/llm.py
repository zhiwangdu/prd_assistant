from __future__ import annotations

import json
import urllib.error
import urllib.parse
import urllib.request
from typing import Any

from .config import Settings
from .store import JsonObject


def generate_agent_final_answer(
    settings: Settings,
    workspace: JsonObject,
    evidence_bundle: JsonObject,
) -> JsonObject | None:
    provider = (settings.agent_provider or "stub").lower()
    if provider == "stub":
        return None
    if provider == "openai_compatible":
        return call_openai_compatible(settings, workspace, evidence_bundle)
    raise ValueError(f"unsupported LOGAGENT_V2_AGENT_PROVIDER {settings.agent_provider}")


def call_openai_compatible(
    settings: Settings,
    workspace: JsonObject,
    evidence_bundle: JsonObject,
) -> JsonObject:
    if not settings.agent_base_url:
        raise ValueError("LOGAGENT_V2_AGENT_BASE_URL is required")
    if not settings.agent_model:
        raise ValueError("LOGAGENT_V2_AGENT_MODEL is required")
    url = urllib.parse.urljoin(settings.agent_base_url.rstrip("/") + "/", "chat/completions")
    payload = {
        "model": settings.agent_model,
        "temperature": 0,
        "messages": [
            {
                "role": "system",
                "content": (
                    "You are LogAgent V2. Return only one JSON object matching the "
                    "final answer schema. Use only provided evidence refs."
                ),
            },
            {
                "role": "user",
                "content": build_agent_prompt(workspace, evidence_bundle),
            },
        ],
    }
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
            raw = response.read(1024 * 1024)
    except urllib.error.HTTPError as error:
        body = error.read(4096).decode("utf-8", errors="replace")
        raise ValueError(f"agent provider returned HTTP {error.code}: {body}") from error
    decoded = json.loads(raw.decode("utf-8"))
    content = extract_chat_content(decoded)
    return parse_final_answer_content(content)


def build_agent_prompt(workspace: JsonObject, evidence_bundle: JsonObject) -> str:
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
