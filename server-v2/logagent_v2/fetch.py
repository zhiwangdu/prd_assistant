from __future__ import annotations

import json
import re
import shlex
import time
import urllib.error
import urllib.parse
import urllib.request
from typing import Any

from .artifacts import write_artifact_bytes
from .config import Settings
from .ids import new_id
from .store import JsonObject, Store


FETCH_METHODS = {"GET", "POST", "PUT", "PATCH", "DELETE", "HEAD"}
SENSITIVE_HEADER_NAMES = {"authorization", "cookie", "x-api-key", "x-auth-token"}
SENSITIVE_QUERY_TOKENS = ("token", "secret", "password", "api_key", "apikey", "session")
CONTROLLED_HEADERS = {"host", "content-length", "transfer-encoding", "connection"}
REDACTED = "__REDACTED__"
REDIRECT_STATUSES = {301, 302, 303, 307, 308}
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
    }
    return normalize_fetch_endpoint(endpoint)


def parse_curl(curl: str) -> JsonObject:
    normalized = curl.replace("\\\r\n", " ").replace("\\\n", " ").replace("\\\r", " ")
    normalized = normalized.strip()
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
    index = 1
    while index < len(argv):
        token = argv[index]

        def next_value(flag: str) -> str:
            nonlocal index
            index += 1
            if index >= len(argv):
                raise ValueError(f"{flag} requires a value")
            return argv[index]

        if token in {"-X", "--request"}:
            method = next_value(token)
        elif token in {"-H", "--header"}:
            name, value = parse_header(next_value(token))
            headers[name] = value
        elif token in {"-d", "--data", "--data-raw", "--data-binary", "--data-ascii"}:
            body = next_value(token)
        elif token in {"-b", "--cookie"}:
            headers["Cookie"] = next_value(token)
        elif token in {"-L", "--location", "--compressed"}:
            pass
        elif token in {"-I", "--head"}:
            method = "HEAD"
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
        elif token.startswith("-X") and len(token) > 2:
            method = token[2:]
        elif token.startswith("-H") and len(token) > 2:
            name, value = parse_header(token[2:])
            headers[name] = value
        elif token.startswith("-d") and len(token) > 2:
            body = token[2:]
        elif token.startswith("-b") and len(token) > 2:
            headers["Cookie"] = token[2:]
        elif token.startswith("-"):
            raise ValueError(
                f"unsupported curl flag {token}; supported flags are -X, -H, --data, "
                "--cookie, --compressed, --head and --location"
            )
        else:
            if url is not None:
                raise ValueError("curl import contains more than one URL")
            url = token
        index += 1
    if not url:
        raise ValueError("curl import is missing URL")
    validate_http_url(url)
    return {"method": method, "url": url, "headers": headers, "body": body}


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


def public_fetch_endpoint(endpoint: JsonObject) -> JsonObject:
    result = dict(endpoint)
    result["url"] = redact_url(result["url"])
    result["headers"] = redact_headers(result.get("headers", {}))
    if result.get("body"):
        result["bodyPreview"] = redact_body_preview(str(result["body"])[:500])
    result.pop("body", None)
    return result


def fetch_catalog_descriptor(settings: Settings) -> JsonObject:
    return {
        "toolId": "logagent.fetch",
        "displayName": "Fetch endpoint",
        "source": "built_in",
        "backend": "fetch",
        "readOnly": True,
        "editable": False,
        "exportable": False,
        "runnable": settings.fetch_enabled,
        "enabled": settings.fetch_enabled,
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
                "properties": {"endpointId": {"type": "string", "minLength": 1}},
                "required": ["endpointId"],
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
        return {
            "enabled": settings.fetch_enabled,
            "endpoints": [
                public_fetch_endpoint(endpoint)
                for endpoint in store.list_fetch_endpoints()
                if endpoint["enabled"]
            ],
        }
    if name == "logagent.fetch":
        endpoint_id = arguments.get("endpointId")
        if not isinstance(endpoint_id, str) or not endpoint_id:
            raise ValueError("endpointId is required")
        return execute_fetch_endpoint(settings, store, run["workspace_id"], run["id"], endpoint_id)
    raise ValueError(f"unsupported fetch tool {name}")


def execute_fetch_endpoint(
    settings: Settings,
    store: Store,
    workspace_id: str,
    run_id: str,
    endpoint_id: str,
) -> JsonObject:
    if not settings.fetch_enabled:
        raise ValueError("fetch is disabled")
    endpoint = store.get_fetch_endpoint(endpoint_id)
    if not endpoint["enabled"]:
        raise ValueError(f"fetch endpoint {endpoint_id} is disabled")
    validate_url_allowed(settings, endpoint["url"])
    action_id = new_id("fetchact")
    started = time.monotonic()
    status = "OK"
    error = None
    response: JsonObject
    try:
        response = perform_http_request(settings, endpoint)
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
    ref = f"tool_results/{action_id}/result.json#response"
    result = {
        "schemaVersion": 1,
        "tool": "logagent.fetch",
        "toolId": "logagent.fetch",
        "actionId": action_id,
        "endpointId": endpoint_id,
        "status": status,
        "summary": fetch_summary(endpoint, response, status, error),
        "request": {
            "name": endpoint["name"],
            "method": endpoint["method"],
            "url": redact_url(endpoint["url"]),
            "headers": redact_headers(endpoint.get("headers", {})),
            "bodyPreview": redact_body_preview((endpoint.get("body") or "")[:500]),
        },
        "response": response,
        "durationMs": duration_ms,
        "error": error,
        "evidenceRef": ref,
    }
    artifact = write_artifact_bytes(
        settings=settings,
        store=store,
        workspace_id=workspace_id,
        filename=f"{action_id}_fetch_result.json",
        data=json.dumps(result, ensure_ascii=True, indent=2).encode("utf-8"),
        content_type="application/json",
        schema_name="logagent.v2.fetch_result.v1",
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
            "ref": ref,
        },
    )
    return {"result": result, "artifact": artifact, "evidence": evidence}


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
    for redirect_count in range(settings.fetch_max_redirects + 1):
        validate_url_allowed(settings, url)
        request = urllib.request.Request(url, data=data, headers=headers, method=method)
        try:
            with opener.open(request, timeout=settings.fetch_timeout_seconds) as response:
                return response_from_http(settings, response, int(response.status), url, redirects)
        except urllib.error.HTTPError as error:
            status_code = int(error.code)
            if status_code not in REDIRECT_STATUSES:
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
        "httpOk": 200 <= status_code < 400,
        "finalUrl": redact_url(final_url),
        "redirectCount": len(redirects or []),
        "redirects": redirects or [],
        "headers": redact_headers(dict(response.headers.items())),
        "bodyPreview": raw[:4000].decode("utf-8", errors="replace"),
        "bodyTruncated": truncated,
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
    allowed = {item.lower() for item in settings.fetch_allowed_hosts}
    if host not in allowed and netloc not in allowed:
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
