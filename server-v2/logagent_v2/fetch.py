from __future__ import annotations

import json
import re
import shlex
import time
import urllib.error
import urllib.parse
import urllib.request
from hashlib import sha256
from typing import Any

from .artifacts import write_artifact_bytes
from .config import Settings, format_fetch_host
from .ids import new_id
from .store import JsonObject, Store


FETCH_METHODS = {"GET", "POST", "PUT", "PATCH", "DELETE", "HEAD"}
SENSITIVE_HEADER_NAMES = {"authorization", "cookie", "x-api-key", "x-auth-token"}
SENSITIVE_QUERY_TOKENS = ("token", "secret", "password", "api_key", "apikey", "session")
CONTROLLED_HEADERS = {"host", "content-length", "transfer-encoding", "connection"}
REDACTED = "__REDACTED__"
REDIRECT_STATUSES = {301, 302, 303, 307, 308}
FETCH_TOOL_ID = "logagent.fetch"
SENSITIVE_ASSIGNMENT_RE = re.compile(
    r"(?i)((?:[A-Za-z0-9_-]*(?:token|secret|password|api_key|apikey|session)"
    r"[A-Za-z0-9_-]*)\s*[=:]\s*)([^&\s,}]+)"
)


def normalize_fetch_endpoint(value: JsonObject) -> JsonObject:
    method = str(value.get("method") or "GET").upper()
    if method not in FETCH_METHODS:
        raise ValueError(f"unsupported fetch method {method}")
    url = str(value.get("url") or "").strip()
    if not url:
        raise ValueError("fetch endpoint url is required")
    headers = normalize_headers(value.get("headers") or {})
    return {
        "name": str(value.get("name") or url)[:200],
        "method": method,
        "url": url,
        "headers": headers,
        "body": value.get("body") if isinstance(value.get("body"), str) else None,
        "enabled": bool(value.get("enabled", True)),
        "followRedirects": bool(value.get("followRedirects", False)),
    }


def preview_curl_import(curl: str) -> JsonObject:
    endpoint = endpoint_from_curl(curl, name="Preview", enabled=True)
    return {
        "schemaVersion": 1,
        "endpoint": public_fetch_endpoint(endpoint),
        "detectedSensitiveFields": detected_sensitive_fields(endpoint),
        "unsupportedWarnings": [],
    }


def endpoint_from_curl(
    curl: str,
    name: str | None = None,
    enabled: bool = True,
) -> JsonObject:
    parsed = parse_curl(curl)
    method = parsed["method"] or ("POST" if parsed.get("body") is not None else "GET")
    endpoint = {
        "name": name or default_fetch_name(parsed["url"]),
        "method": method,
        "url": parsed["url"],
        "headers": parsed["headers"],
        "body": parsed.get("body"),
        "enabled": enabled,
        "followRedirects": bool(parsed.get("followRedirects", False)),
    }
    return normalize_fetch_endpoint(endpoint)


def parse_curl(curl: str) -> JsonObject:
    normalized = curl.replace("\\\r\n", " ").replace("\\\n", " ").replace("\\\r", " ")
    normalized = normalized.strip()
    if normalized.startswith("$"):
        normalized = normalized[1:].lstrip()
    if "`" in normalized or normalized.startswith("curl.exe "):
        raise ValueError("only bash-style curl commands are supported")
    try:
        argv = shlex.split(normalized)
    except ValueError as error:
        raise ValueError(f"failed to parse bash curl command: {error}") from error
    if not argv or not argv[0].endswith("curl"):
        raise ValueError("curl command must start with curl")

    method: str | None = None
    url: str | None = None
    headers: JsonObject = {}
    body: str | None = None
    follow_redirects = False
    index = 1
    while index < len(argv):
        token = argv[index]

        def set_url(value: str) -> None:
            nonlocal url
            if url is not None:
                raise ValueError("curl import contains more than one URL")
            url = value

        def next_value(flag: str) -> str:
            nonlocal index
            index += 1
            if index >= len(argv):
                raise ValueError(f"{flag} requires a value")
            return argv[index]

        if token == "--url":
            set_url(next_value(token))
        elif token in {"-X", "--request"}:
            method = next_value(token)
        elif token in {"-H", "--header"}:
            name, value = parse_header(next_value(token))
            headers[name] = value
        elif token in {"-d", "--data", "--data-raw", "--data-binary", "--data-ascii"}:
            body = next_value(token)
        elif token in {"-b", "--cookie"}:
            headers["Cookie"] = next_value(token)
        elif token in {"-A", "--user-agent"}:
            headers["User-Agent"] = next_value(token)
        elif token in {"-e", "--referer"}:
            headers["Referer"] = next_value(token)
        elif token in {"-L", "--location"}:
            follow_redirects = True
        elif token == "--compressed":
            pass
        elif token in {"-I", "--head"}:
            method = "HEAD"
        elif token.startswith("--url="):
            set_url(token.removeprefix("--url="))
        elif token.startswith("--request="):
            method = token.removeprefix("--request=")
        elif token.startswith("--header="):
            name, value = parse_header(token.removeprefix("--header="))
            headers[name] = value
        elif token.startswith("--data="):
            body = token.removeprefix("--data=")
        elif token.startswith("--data-raw="):
            body = token.removeprefix("--data-raw=")
        elif token.startswith("--data-binary="):
            body = token.removeprefix("--data-binary=")
        elif token.startswith("--user-agent="):
            headers["User-Agent"] = token.removeprefix("--user-agent=")
        elif token.startswith("--referer="):
            headers["Referer"] = token.removeprefix("--referer=")
        elif token.startswith("-X") and len(token) > 2:
            method = token[2:]
        elif token.startswith("-H") and len(token) > 2:
            name, value = parse_header(token[2:])
            headers[name] = value
        elif token.startswith("-d") and len(token) > 2:
            body = token[2:]
        elif token.startswith("-b") and len(token) > 2:
            headers["Cookie"] = token[2:]
        elif token.startswith("-A") and len(token) > 2:
            headers["User-Agent"] = token[2:]
        elif token.startswith("-e") and len(token) > 2:
            headers["Referer"] = token[2:]
        elif token.startswith("-"):
            raise ValueError(
                f"unsupported curl flag {token}; supported flags are -X, -H, --data, "
                "--cookie, --url, --user-agent, --referer, --compressed, --head and "
                "--location"
            )
        else:
            set_url(token)
        index += 1
    if not url:
        raise ValueError("curl import is missing URL")
    validate_http_url(url)
    return {
        "method": method,
        "url": url,
        "headers": headers,
        "body": body,
        "followRedirects": follow_redirects,
    }


def parse_header(value: str) -> tuple[str, str]:
    if ":" not in value:
        raise ValueError("header must use Name: value syntax")
    name, header_value = value.split(":", 1)
    name = name.strip()
    if not name:
        raise ValueError("header name must not be empty")
    return name, header_value.lstrip()


def validate_http_url(url: str) -> None:
    parsed = urllib.parse.urlsplit(url)
    if parsed.scheme not in {"http", "https"} or not parsed.netloc:
        raise ValueError("curl URL must be absolute http/https URL")


def default_fetch_name(url: str) -> str:
    parsed = urllib.parse.urlsplit(url)
    return (parsed.hostname or "Imported Fetch")[:200]


def detected_sensitive_fields(endpoint: JsonObject) -> list[JsonObject]:
    fields = []
    parsed = urllib.parse.urlsplit(endpoint["url"])
    for key, _ in urllib.parse.parse_qsl(parsed.query, keep_blank_values=True):
        if is_sensitive_name(key):
            fields.append({"location": "query", "name": key})
    for key in endpoint.get("headers", {}):
        if is_sensitive_header(str(key)):
            fields.append({"location": "header", "name": str(key)})
    body = endpoint.get("body")
    if isinstance(body, str):
        fields.extend(detect_sensitive_body_fields(body))
    return fields


def detect_sensitive_body_fields(body: str) -> list[JsonObject]:
    stripped = body.strip()
    fields = []
    try:
        value = json.loads(stripped)
    except Exception:
        value = None
    if isinstance(value, dict):
        for key in value:
            if is_sensitive_name(str(key)):
                fields.append({"location": "body", "name": str(key)})
        return fields
    if "=" in stripped and "\n" not in stripped:
        for part in stripped.split("&"):
            if not part:
                continue
            key = part.split("=", 1)[0]
            if is_sensitive_name(key):
                fields.append({"location": "body", "name": key})
    return fields


def normalize_headers(value: Any) -> JsonObject:
    if not isinstance(value, dict):
        raise ValueError("fetch endpoint headers must be an object")
    headers: JsonObject = {}
    for key, item in value.items():
        name = str(key).strip()
        if not name:
            continue
        if name.lower() in CONTROLLED_HEADERS:
            raise ValueError(f"fetch endpoint header {name} is controlled by Server")
        headers[name] = str(item)
    return headers


def normalize_fetch_run_params(value: JsonObject) -> JsonObject:
    if not isinstance(value, dict):
        raise ValueError("logagent.fetch params must be an object")
    unknown = sorted(set(value) - {"endpointId", "fetchId", "variables", "headers", "body"})
    if unknown:
        raise ValueError(f"logagent.fetch received unsupported params: {', '.join(unknown)}")
    endpoint_id = value.get("endpointId") or value.get("fetchId")
    if not isinstance(endpoint_id, str) or not endpoint_id.strip():
        raise ValueError("logagent.fetch requires endpointId or fetchId")

    normalized: JsonObject = {
        "endpointId": endpoint_id.strip(),
        "variables": normalize_fetch_variables(value.get("variables") or {}),
        "headers": normalize_fetch_runtime_headers(value.get("headers") or {}),
    }
    if "body" in value and value.get("body") is not None:
        body = value["body"]
        if not isinstance(body, str):
            raise ValueError("logagent.fetch body override must be a string")
        normalized["body"] = body
    return normalized


def normalize_fetch_variables(value: Any) -> JsonObject:
    if not isinstance(value, dict):
        raise ValueError("logagent.fetch variables must be an object")
    variables: JsonObject = {}
    for key, item in value.items():
        if not isinstance(key, str):
            raise ValueError("logagent.fetch variable names must be strings")
        validate_fetch_variable_name(key)
        if not isinstance(item, str):
            raise ValueError(f"logagent.fetch variable {key} must be a string")
        variables[key] = item
    return variables


def normalize_fetch_runtime_headers(value: Any) -> JsonObject:
    if not isinstance(value, dict):
        raise ValueError("logagent.fetch headers must be an object")
    headers: JsonObject = {}
    for key, item in value.items():
        if not isinstance(key, str):
            raise ValueError("logagent.fetch header names must be strings")
        name = key.strip()
        if not name:
            continue
        if name.lower() in CONTROLLED_HEADERS:
            raise ValueError(f"logagent.fetch header override {name} is controlled by Server")
        if not isinstance(item, str):
            raise ValueError(f"logagent.fetch header {name} must be a string")
        headers[name] = item
    return headers


def validate_fetch_variable_name(name: str) -> None:
    if not name or not all(char.isascii() and (char.isalnum() or char == "_") for char in name):
        raise ValueError(f"invalid fetch variable name {name}")


def apply_fetch_variables(template: str, variables: JsonObject) -> str:
    output = template
    for key, value in variables.items():
        output = output.replace("{" + key + "}", value)
    if "{" in output or "}" in output:
        raise ValueError("fetch URL template contains unresolved variables")
    return output


def prepare_fetch_endpoint(endpoint: JsonObject, run_params: JsonObject) -> JsonObject:
    variables = run_params.get("variables") or {}
    prepared = dict(endpoint)
    prepared["url"] = apply_fetch_variables(endpoint["url"], variables)
    headers = dict(endpoint.get("headers", {}))
    headers.update(run_params.get("headers") or {})
    prepared["headers"] = headers
    if "body" in run_params:
        prepared["body"] = run_params["body"]
    return prepared


def validate_fetch_request_size(settings: Settings, endpoint: JsonObject) -> None:
    body = endpoint.get("body")
    if not isinstance(body, str):
        return
    if len(body.encode("utf-8")) > settings.fetch_max_request_bytes:
        raise ValueError("fetch request body exceeds LOGAGENT_V2_FETCH_MAX_REQUEST_BYTES")


def redact_variables(variables: JsonObject) -> JsonObject:
    return {
        str(key): REDACTED if is_sensitive_name(str(key)) else str(value)
        for key, value in variables.items()
    }


def public_fetch_endpoint(endpoint: JsonObject) -> JsonObject:
    result = dict(endpoint)
    result["url"] = redact_url(result["url"])
    result["headers"] = redact_headers(result.get("headers", {}))
    if result.get("body"):
        result["bodyPreview"] = redact_body_preview(str(result["body"])[:500])
    result.pop("body", None)
    return result


def mcp_fetch_endpoint(endpoint: JsonObject) -> JsonObject:
    public = public_fetch_endpoint(endpoint)
    credential_set = public.get("credentialSet")
    credential_version = (
        credential_set.get("updatedAt") if isinstance(credential_set, dict) else None
    )
    public.update(
        {
            "fetchId": endpoint["id"],
            "description": endpoint.get("description", ""),
            "tags": endpoint.get("tags", []),
            "urlTemplate": public["url"],
            "credentialVersion": credential_version,
        }
    )
    return public


def endpoint_with_credential_summary(store: Store, endpoint: JsonObject) -> JsonObject:
    result = dict(endpoint)
    credential = store.get_fetch_credential_set(endpoint["id"])
    result["hasCredentials"] = credential is not None
    if credential is not None:
        result["credentialSet"] = {
            "id": credential["id"],
            "redacted": credential["redacted"],
            "updatedAt": credential["updatedAt"],
        }
    return result


def endpoint_for_storage(endpoint: JsonObject) -> JsonObject:
    if not detected_sensitive_fields(endpoint):
        return dict(endpoint)
    stored = dict(endpoint)
    stored["url"] = redact_url(endpoint["url"])
    stored["headers"] = redact_headers(endpoint.get("headers", {}))
    if endpoint.get("body") is not None:
        stored["body"] = redact_body_preview(str(endpoint.get("body")))
    return stored


def validate_fetch_credentials_available(settings: Settings, endpoint: JsonObject) -> None:
    if detected_sensitive_fields(endpoint):
        fetch_fernet(settings)


def persist_fetch_credentials(
    settings: Settings,
    store: Store,
    endpoint_id: str,
    endpoint: JsonObject,
) -> None:
    sensitive_fields = detected_sensitive_fields(endpoint)
    if not sensitive_fields:
        store.delete_fetch_credential_set(endpoint_id)
        return
    encrypted = encrypt_json(settings, {"schemaVersion": 1, "endpoint": endpoint})
    redacted = {
        "schemaVersion": 1,
        "detectedSensitiveFields": sensitive_fields,
        "endpoint": public_fetch_endpoint(endpoint),
    }
    store.upsert_fetch_credential_set(endpoint_id, encrypted, redacted)


def hydrate_fetch_endpoint(
    settings: Settings,
    store: Store,
    endpoint: JsonObject,
) -> JsonObject:
    credential = store.get_fetch_credential_set(endpoint["id"])
    if credential is None:
        return dict(endpoint)
    decrypted = decrypt_json(settings, credential["encrypted"])
    secret_endpoint = decrypted.get("endpoint")
    if not isinstance(secret_endpoint, dict):
        raise ValueError("fetch credential set is invalid")
    hydrated = dict(endpoint)
    for key in ("name", "method", "url", "headers", "body", "enabled", "followRedirects"):
        if key in secret_endpoint:
            hydrated[key] = secret_endpoint[key]
    return hydrated


def encrypt_json(settings: Settings, value: JsonObject) -> str:
    fernet = fetch_fernet(settings)
    data = json.dumps(value, ensure_ascii=True, sort_keys=True).encode("utf-8")
    return fernet.encrypt(data).decode("utf-8")


def decrypt_json(settings: Settings, encrypted: str) -> JsonObject:
    fernet = fetch_fernet(settings)
    try:
        data = fernet.decrypt(encrypted.encode("utf-8"))
    except Exception as error:
        raise ValueError("failed to decrypt fetch credential set") from error
    value = json.loads(data.decode("utf-8"))
    if not isinstance(value, dict):
        raise ValueError("fetch credential set did not decrypt to an object")
    return value


def fetch_fernet(settings: Settings):
    if not settings.fetch_secret_key:
        raise ValueError(
            "LOGAGENT_V2_FETCH_SECRET_KEY is required for sensitive Fetch credentials"
        )
    try:
        from cryptography.fernet import Fernet
    except Exception as error:
        raise ValueError("cryptography package is required for Fetch credential encryption") from error
    try:
        return Fernet(settings.fetch_secret_key.encode("utf-8"))
    except Exception as error:
        raise ValueError("LOGAGENT_V2_FETCH_SECRET_KEY must be a valid Fernet key") from error


def fetch_catalog_descriptor(settings: Settings) -> JsonObject:
    return {
        "toolId": "logagent.fetch",
        "displayName": "Fetch endpoint",
        "description": "Run a managed HTTP endpoint imported from a browser DevTools curl command.",
        "source": "built_in",
        "tags": ["built-in", "fetch", "http", "manual-run"],
        "backend": "fetch",
        "readOnly": False,
        "editable": False,
        "exportable": False,
        "runnable": settings.fetch_enabled,
        "enabled": settings.fetch_enabled,
        "minFiles": 0,
        "maxFiles": 0,
        "acceptedSuffixes": [],
        "paramsSchema": {
            "type": "object",
            "properties": {
                "endpointId": {"type": "string"},
                "fetchId": {"type": "string"},
                "variables": {
                    "type": "object",
                    "additionalProperties": {"type": "string"},
                },
                "headers": {
                    "type": "object",
                    "additionalProperties": {"type": "string"},
                },
                "body": {"type": "string"},
            },
            "anyOf": [{"required": ["fetchId"]}, {"required": ["endpointId"]}],
            "additionalProperties": False,
        },
        "paramsTemplate": {"fetchId": "", "variables": {}, "headers": {}, "body": None},
        "outputViews": ["summary", "request", "response", "body_artifact"],
        "allowedHosts": list(settings.fetch_allowed_hosts),
    }


def fetch_tool_descriptors() -> list[JsonObject]:
    return [
        {
            "name": "logagent.list_fetch_endpoints",
            "description": "List configured fetch endpoints available to this task.",
            "inputSchema": {"type": "object", "additionalProperties": False},
        },
        {
            "name": "logagent.fetch",
            "description": "Run one configured fetch endpoint by endpointId.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "endpointId": {"type": "string", "minLength": 1},
                    "fetchId": {"type": "string", "minLength": 1},
                    "variables": {
                        "type": "object",
                        "additionalProperties": {"type": "string"},
                    },
                    "headers": {
                        "type": "object",
                        "additionalProperties": {"type": "string"},
                    },
                    "body": {"type": "string"},
                },
                "anyOf": [{"required": ["endpointId"]}, {"required": ["fetchId"]}],
                "additionalProperties": False,
            },
        },
    ]


def call_fetch_tool(
    settings: Settings,
    store: Store,
    run: JsonObject,
    name: str,
    arguments: JsonObject,
) -> JsonObject:
    if name == "logagent.list_fetch_endpoints":
        if not settings.fetch_enabled:
            raise ValueError("fetch is disabled by configuration")
        return {
            "schemaVersion": 1,
            "enabled": settings.fetch_enabled,
            "endpoints": [
                mcp_fetch_endpoint(endpoint_with_credential_summary(store, endpoint))
                for endpoint in store.list_fetch_endpoints()
                if endpoint["enabled"]
            ],
            "finalEvidenceAllowed": False,
        }
    if name == "logagent.fetch":
        run_params = normalize_fetch_run_params(arguments)
        executed = execute_fetch_endpoint(
            settings,
            store,
            run["workspace_id"],
            run["id"],
            run_params["endpointId"],
            run_params=run_params,
            action_id=stable_fetch_action_id(run_params),
        )
        return {**executed, **fetch_tool_compat_payload(executed)}
    raise ValueError(f"unsupported fetch tool {name}")


def execute_fetch_endpoint(
    settings: Settings,
    store: Store,
    workspace_id: str,
    run_id: str,
    endpoint_id: str,
    run_params: JsonObject | None = None,
    action_id: str | None = None,
) -> JsonObject:
    if not settings.fetch_enabled:
        raise ValueError("fetch is disabled")
    run_params = normalize_fetch_run_params(
        {"endpointId": endpoint_id, **(run_params or {})}
    )
    endpoint_id = run_params["endpointId"]
    endpoint = hydrate_fetch_endpoint(settings, store, store.get_fetch_endpoint(endpoint_id))
    if not endpoint["enabled"]:
        raise ValueError(f"fetch endpoint {endpoint_id} is disabled")
    credential = store.get_fetch_credential_set(endpoint_id)
    endpoint = prepare_fetch_endpoint(endpoint, run_params)
    validate_url_allowed(settings, endpoint["url"])
    validate_fetch_request_size(settings, endpoint)
    action_id = action_id or new_id("fetchact")
    started = time.monotonic()
    status = "OK"
    error = None
    response: JsonObject
    body_bytes = b""
    try:
        response = perform_http_request(settings, endpoint)
        raw_body = response.pop("_bodyBytes", b"")
        if isinstance(raw_body, bytes):
            body_bytes = raw_body
    except Exception as exc:
        status = "FAILED"
        error = str(exc)[:2000]
        response = {
            "statusCode": None,
            "httpOk": False,
            "headers": {},
            "bodyPreview": "",
            "bodyTruncated": False,
        }
    duration_ms = int((time.monotonic() - started) * 1000)
    result_path = f"tool_results/{action_id}/result.json"
    ref = f"{result_path}#response"
    logical_body_path = f"tool_results/{action_id}/response_body.bin"
    body_artifact = write_artifact_bytes(
        settings=settings,
        store=store,
        workspace_id=workspace_id,
        filename=f"{action_id}_response_body.bin",
        data=body_bytes,
        content_type="application/octet-stream",
        schema_name="logagent.v2.fetch_response_body.v1",
        preview={
            "tool": "logagent.fetch",
            "endpointId": endpoint_id,
            "actionId": action_id,
            "path": logical_body_path,
            "sizeBytes": len(body_bytes),
        },
    )
    response["bodyArtifactPath"] = logical_body_path
    response["bodyArtifactId"] = body_artifact["id"]
    response["bodyArtifactRelativePath"] = body_artifact["relative_path"]
    response["truncated"] = bool(response.get("bodyTruncated", False))
    result = {
        "schemaVersion": 3,
        "tool": "logagent.fetch",
        "toolId": "logagent.fetch",
        "actionId": action_id,
        "endpointId": endpoint_id,
        "fetchId": endpoint_id,
        "status": status,
        "exitCode": None,
        "command": [],
        "inputFile": None,
        "stdoutPath": "",
        "stderrPath": "",
        "summary": fetch_summary(endpoint, response, status, error),
        "findings": [],
        "httpOk": bool(response.get("httpOk", False)),
        "statusCode": response.get("statusCode"),
        "redirectCount": response.get("redirectCount", 0),
        "finalUrl": response.get("finalUrl"),
        "request": {
            "name": endpoint["name"],
            "method": endpoint["method"],
            "url": redact_url(endpoint["url"]),
            "headers": redact_headers(endpoint.get("headers", {})),
            "bodyPreview": redact_body_preview((endpoint.get("body") or "")[:500]),
            "variables": redact_variables(run_params.get("variables") or {}),
        },
        "response": response,
        "bodyArtifactPath": logical_body_path,
        "bodyArtifactId": body_artifact["id"],
        "bodyArtifactRelativePath": body_artifact["relative_path"],
        "truncated": bool(response.get("bodyTruncated", False)),
        "credentialVersion": credential["updatedAt"] if credential is not None else None,
        "durationMs": duration_ms,
        "error": error,
        "evidenceRef": ref,
        "evidenceRefs": [ref],
    }
    artifact = write_artifact_bytes(
        settings=settings,
        store=store,
        workspace_id=workspace_id,
        filename=f"{action_id}_fetch_result.json",
        data=json.dumps(result, ensure_ascii=True, indent=2).encode("utf-8"),
        content_type="application/json",
        schema_name="logagent.v2.fetch_result.v3",
        preview={
            "tool": "logagent.fetch",
            "endpointId": endpoint_id,
            "status": status,
            "statusCode": response.get("statusCode"),
        },
    )
    evidence = store.create_evidence(
        workspace_id=workspace_id,
        run_id=run_id,
        kind="fetch_result",
        final_allowed=True,
        summary=result["summary"],
        artifact_id=artifact["id"],
        payload={
            "artifactId": artifact["id"],
            "tool": "logagent.fetch",
            "actionId": action_id,
            "endpointId": endpoint_id,
            "bodyArtifactId": body_artifact["id"],
            "ref": ref,
        },
    )
    return {"result": result, "artifact": artifact, "evidence": evidence}


def stable_fetch_action_id(run_params: JsonObject) -> str:
    encoded = json.dumps(run_params, ensure_ascii=True, sort_keys=True, separators=(",", ":"))
    digest = sha256(encoded.encode("utf-8")).hexdigest()[:16]
    return f"act_fetch_{digest}"


def fetch_tool_compat_payload(executed: JsonObject) -> JsonObject:
    result = executed.get("result") if isinstance(executed.get("result"), dict) else {}
    action_id = result.get("actionId")
    artifact_path = (
        f"tool_results/{action_id}/result.json" if isinstance(action_id, str) else None
    )
    response = result.get("response") if isinstance(result.get("response"), dict) else {}
    body_preview = response.get("bodyPreview")
    if isinstance(body_preview, str):
        body_preview = body_preview[:1200]
    else:
        body_preview = ""
    evidence_refs = result.get("evidenceRefs")
    if not isinstance(evidence_refs, list):
        evidence_ref = result.get("evidenceRef")
        evidence_refs = [evidence_ref] if isinstance(evidence_ref, str) else []
    return {
        "artifactPath": artifact_path,
        "statusCode": result.get("statusCode"),
        "httpOk": bool(result.get("httpOk", False)),
        "bodyPreview": body_preview,
        "evidenceRefs": evidence_refs,
    }


def perform_http_request(settings: Settings, endpoint: JsonObject) -> JsonObject:
    body = endpoint.get("body")
    data = (
        body.encode("utf-8")
        if body is not None and endpoint["method"] not in {"GET", "HEAD"}
        else None
    )
    opener = urllib.request.build_opener(NoRedirectHandler)
    method = endpoint["method"]
    url = endpoint["url"]
    headers = dict(endpoint.get("headers", {}))
    redirects: list[JsonObject] = []
    follow_redirects = bool(endpoint.get("followRedirects", False))
    for redirect_count in range(settings.fetch_max_redirects + 1):
        validate_url_allowed(settings, url)
        request = urllib.request.Request(url, data=data, headers=headers, method=method)
        try:
            with opener.open(request, timeout=settings.fetch_timeout_seconds) as response:
                return response_from_http(settings, response, int(response.status), url, redirects)
        except urllib.error.HTTPError as error:
            status_code = int(error.code)
            if status_code not in REDIRECT_STATUSES or not follow_redirects:
                return response_from_http(settings, error, status_code, url, redirects)
            location = error.headers.get("Location")
            if not location:
                return response_from_http(settings, error, status_code, url, redirects)
            if redirect_count >= settings.fetch_max_redirects:
                error.close()
                raise ValueError("fetch redirect limit exceeded")
            next_url = urllib.parse.urljoin(url, location)
            try:
                validate_url_allowed(settings, next_url)
            finally:
                error.close()
            redirects.append(
                {
                    "statusCode": status_code,
                    "from": redact_url(url),
                    "to": redact_url(next_url),
                }
            )
            headers = redirect_headers(headers, url, next_url)
            if status_code == 303 or (status_code in {301, 302} and method != "GET"):
                method = "GET"
                data = None
            url = next_url
    raise ValueError("fetch redirect limit exceeded")


def response_from_http(
    settings: Settings,
    response: Any,
    status_code: int,
    final_url: str,
    redirects: list[JsonObject] | None = None,
) -> JsonObject:
    raw = response.read(settings.fetch_max_response_bytes + 1)
    truncated = len(raw) > settings.fetch_max_response_bytes
    if truncated:
        raw = raw[: settings.fetch_max_response_bytes]
    return {
        "statusCode": status_code,
        "httpOk": 200 <= status_code < 300,
        "finalUrl": redact_url(final_url),
        "redirectCount": len(redirects or []),
        "redirects": redirects or [],
        "headers": redact_headers(dict(response.headers.items())),
        "bodyPreview": raw[:4000].decode("utf-8", errors="replace"),
        "bodyTruncated": truncated,
        "_bodyBytes": raw,
    }


def redirect_headers(headers: JsonObject, current_url: str, next_url: str) -> JsonObject:
    if same_origin(current_url, next_url):
        return dict(headers)
    return {
        key: value
        for key, value in headers.items()
        if not is_sensitive_header(str(key))
    }


def same_origin(left_url: str, right_url: str) -> bool:
    left = urllib.parse.urlsplit(left_url)
    right = urllib.parse.urlsplit(right_url)
    return (
        left.scheme.lower(),
        (left.hostname or "").lower(),
        left.port,
    ) == (
        right.scheme.lower(),
        (right.hostname or "").lower(),
        right.port,
    )


def fetch_text(settings: Settings, url: str) -> JsonObject:
    if not settings.fetch_enabled:
        raise ValueError("metadata URL fetch is disabled")
    validate_url_allowed(settings, url)
    request = urllib.request.Request(url, method="GET")
    opener = urllib.request.build_opener(NoRedirectHandler)
    try:
        with opener.open(request, timeout=settings.fetch_timeout_seconds) as response:
            status_code = int(response.status)
            raw = response.read(settings.fetch_max_response_bytes + 1)
    except urllib.error.HTTPError as error:
        status_code = int(error.code)
        raw = error.read(min(settings.fetch_max_response_bytes + 1, 4096))
        if 300 <= status_code < 400:
            raise ValueError(f"metadata URL fetch redirects are disabled: HTTP {status_code}")
        raise ValueError(
            f"metadata URL fetch returned HTTP {status_code}: "
            f"{raw[:500].decode('utf-8', errors='replace')}"
        ) from error
    truncated = len(raw) > settings.fetch_max_response_bytes
    if truncated:
        raise ValueError("metadata URL fetch response exceeds LOGAGENT_V2_FETCH_MAX_RESPONSE_BYTES")
    if not 200 <= status_code < 300:
        raise ValueError(f"metadata URL fetch returned HTTP {status_code}")
    return {
        "url": redact_url(url),
        "statusCode": status_code,
        "content": raw.decode("utf-8", errors="replace"),
        "sizeBytes": len(raw),
    }


class NoRedirectHandler(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):  # type: ignore[override]
        raise urllib.error.HTTPError(req.full_url, code, msg, headers, fp)


def validate_url_allowed(settings: Settings, url: str) -> None:
    parsed = urllib.parse.urlsplit(url)
    if parsed.scheme not in {"http", "https"}:
        raise ValueError("fetch only supports http/https URLs")
    if not parsed.hostname:
        raise ValueError("fetch URL must include host")
    host = parsed.hostname.lower()
    netloc = parsed.netloc.lower()
    try:
        port = parsed.port
    except ValueError as error:
        raise ValueError(f"invalid fetch URL port in {url}") from error
    if port is None:
        port = 443 if parsed.scheme == "https" else 80
    scheme_host_port = f"{parsed.scheme}://{format_fetch_host(host)}:{port}"
    allowed = {item.lower() for item in settings.fetch_allowed_hosts}
    if host not in allowed and netloc not in allowed and scheme_host_port not in allowed:
        raise ValueError(f"fetch host {parsed.netloc} is not in allowlist")


def redact_url(url: str) -> str:
    parsed = urllib.parse.urlsplit(url)
    query = urllib.parse.parse_qsl(parsed.query, keep_blank_values=True)
    redacted = [
        (key, REDACTED if is_sensitive_name(key) else value)
        for key, value in query
    ]
    return urllib.parse.urlunsplit(
        (
            parsed.scheme,
            parsed.netloc,
            parsed.path,
            urllib.parse.urlencode(redacted),
            parsed.fragment,
        )
    )


def redact_headers(headers: JsonObject) -> JsonObject:
    return {
        str(key): REDACTED if is_sensitive_header(str(key)) else str(value)
        for key, value in headers.items()
    }


def redact_text(value: str) -> str:
    if not value:
        return value
    stripped = value.strip()
    try:
        decoded = json.loads(stripped)
    except Exception:
        decoded = None
    if decoded is not None:
        return json.dumps(redact_json(decoded), ensure_ascii=True)

    if "=" in value:
        pairs = urllib.parse.parse_qsl(value, keep_blank_values=True)
        if pairs:
            return urllib.parse.urlencode(
                [
                    (key, REDACTED if is_sensitive_name(key) else item)
                    for key, item in pairs
                ]
            )

    return SENSITIVE_ASSIGNMENT_RE.sub(lambda match: match.group(1) + REDACTED, value)


def redact_body_preview(value: str) -> str:
    return redact_text(value)


def redact_json(value: Any) -> Any:
    if isinstance(value, dict):
        return {
            str(key): REDACTED if is_sensitive_name(str(key)) else redact_json(item)
            for key, item in value.items()
        }
    if isinstance(value, list):
        return [redact_json(item) for item in value]
    return value


def is_sensitive_header(name: str) -> bool:
    lowered = name.lower()
    return lowered in SENSITIVE_HEADER_NAMES or is_sensitive_name(lowered)


def is_sensitive_name(name: str) -> bool:
    lowered = name.lower()
    return any(token in lowered for token in SENSITIVE_QUERY_TOKENS)


def fetch_summary(
    endpoint: JsonObject, response: JsonObject, status: str, error: str | None
) -> str:
    if status == "FAILED":
        return f"Fetch {endpoint['name']} failed: {error}"
    return f"Fetch {endpoint['name']} returned HTTP {response.get('statusCode')}"
