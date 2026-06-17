from __future__ import annotations

from typing import Any

from .ids import new_id
from .store import JsonObject, Store, now_iso


RESOURCE_KINDS = {
    "prompt_pack",
    "architecture_doc",
    "runbook",
    "glossary",
    "tool_capability",
    "knowledge_note",
    "diagnostic_skill",
}
METADATA_KIND = "metadata_instance"
SCOPES = {"global", "log_analysis", "tool_run", "case_import"}
USER_CONTENT_TYPES = {"markdown", "plain_text", "json"}
VERSION_STATUSES = {"draft", "active", "archived"}
DEFAULT_PROMPT_POLICY = {
    "includeByDefault": True,
    "maxChars": 4000,
    "priority": 0,
    "allowedTaskKinds": [],
}


def create_system_context_resource(store: Store, payload: JsonObject) -> JsonObject:
    kind = require_choice(payload.get("kind"), RESOURCE_KINDS, "kind")
    title = clean_required(payload.get("title"), "title", max_chars=200)
    content = clean_required(payload.get("content"), "content", max_chars=200_000)
    now = now_iso()
    version_id = new_id("ctxver")
    record = {
        "schemaVersion": 1,
        "contextId": new_id("ctx"),
        "kind": kind,
        "title": title,
        "description": clean_optional(payload.get("description"), max_chars=2000),
        "scope": require_choice(payload.get("scope") or "log_analysis", SCOPES, "scope"),
        "enabled": bool(payload.get("enabled", True)),
        "tags": normalize_tags(payload.get("tags")),
        "product": clean_optional(payload.get("product"), max_chars=120),
        "version": clean_optional(payload.get("version"), max_chars=120),
        "environment": clean_optional(payload.get("environment"), max_chars=120),
        "activeVersionId": version_id,
        "versions": [
            {
                "versionId": version_id,
                "revision": 1,
                "status": "active",
                "contentType": require_choice(
                    payload.get("contentType"), USER_CONTENT_TYPES, "contentType"
                ),
                "content": content,
                "summary": clean_optional(payload.get("summary"), max_chars=2000),
                "promptPolicy": normalize_prompt_policy(payload.get("promptPolicy")),
                "createdAt": now,
                "updatedAt": now,
            }
        ],
        "createdAt": now,
        "updatedAt": now,
    }
    return store.upsert_system_context_resource(record)


def list_system_context_resource_summaries(store: Store) -> list[JsonObject]:
    resources = [
        resource_summary(resource, "system_context")
        for resource in store.list_system_context_resources()
    ]
    resources.extend(metadata_resource_summaries(store))
    resources.sort(key=lambda item: (str(item.get("kind")), str(item.get("title"))))
    return resources


def get_system_context_resource(store: Store, context_id: str) -> JsonObject:
    validate_context_id(context_id)
    if context_id.startswith("meta_"):
        raise KeyError(f"metadata adapter {context_id} is summary-only")
    return store.get_system_context_resource(context_id)


def patch_system_context_resource(
    store: Store, context_id: str, updates: JsonObject
) -> JsonObject:
    record = get_system_context_resource(store, context_id)
    for field in (
        "title",
        "description",
        "scope",
        "enabled",
        "tags",
        "product",
        "version",
        "environment",
    ):
        if field not in updates:
            continue
        value = updates[field]
        if field == "title":
            record[field] = clean_required(value, field, max_chars=200)
        elif field == "description":
            record[field] = clean_optional(value, max_chars=2000)
        elif field == "scope":
            record[field] = require_choice(value, SCOPES, field)
        elif field == "enabled":
            record[field] = bool(value)
        elif field == "tags":
            record[field] = normalize_tags(value)
        else:
            record[field] = clean_optional(value, max_chars=120)
    record["updatedAt"] = now_iso()
    return store.upsert_system_context_resource(record)


def create_system_context_version(
    store: Store, context_id: str, payload: JsonObject
) -> JsonObject:
    record = get_system_context_resource(store, context_id)
    content = clean_required(payload.get("content"), "content", max_chars=200_000)
    activate = bool(payload.get("activate", True))
    now = now_iso()
    if activate:
        archive_active_versions(record)
    version = {
        "versionId": new_id("ctxver"),
        "revision": max(
            (int(item.get("revision", 0)) for item in record["versions"]),
            default=0,
        )
        + 1,
        "status": "active" if activate else "draft",
        "contentType": require_choice(
            payload.get("contentType"), USER_CONTENT_TYPES, "contentType"
        ),
        "content": content,
        "summary": clean_optional(payload.get("summary"), max_chars=2000),
        "promptPolicy": normalize_prompt_policy(payload.get("promptPolicy")),
        "createdAt": now,
        "updatedAt": now,
    }
    record["versions"].append(version)
    if activate:
        record["activeVersionId"] = version["versionId"]
    record["updatedAt"] = now
    return store.upsert_system_context_resource(record)


def patch_system_context_version(
    store: Store,
    context_id: str,
    version_id: str,
    updates: JsonObject,
) -> JsonObject:
    validate_version_id(version_id)
    record = get_system_context_resource(store, context_id)
    version = find_version(record, version_id)
    if updates.get("status") == "active":
        archive_active_versions(record)
    if "contentType" in updates:
        version["contentType"] = require_choice(
            updates["contentType"], USER_CONTENT_TYPES, "contentType"
        )
    if "content" in updates:
        version["content"] = clean_required(updates["content"], "content", max_chars=200_000)
    if "summary" in updates:
        version["summary"] = clean_optional(updates["summary"], max_chars=2000)
    if "promptPolicy" in updates:
        version["promptPolicy"] = normalize_prompt_policy(updates["promptPolicy"])
    if "status" in updates:
        version["status"] = require_choice(updates["status"], VERSION_STATUSES, "status")
    if version["status"] == "active":
        record["activeVersionId"] = version_id
    elif record.get("activeVersionId") == version_id:
        record["activeVersionId"] = next(
            (
                item["versionId"]
                for item in record["versions"]
                if item.get("status") == "active"
            ),
            None,
        )
    timestamp = now_iso()
    version["updatedAt"] = timestamp
    record["updatedAt"] = timestamp
    return store.upsert_system_context_resource(record)


def activate_system_context_version(
    store: Store, context_id: str, version_id: str
) -> JsonObject:
    return patch_system_context_version(
        store,
        context_id,
        version_id,
        {"status": "active"},
    )


def preview_system_context_resources(
    store: Store,
    context_ids: list[str] | None = None,
    task_kind: str = "log_analysis",
    product: str | None = None,
    version: str | None = None,
    environment: str | None = None,
    instance_id: str | None = None,
) -> JsonObject:
    task_kind = require_choice(task_kind, {"log_analysis", "tool_run"}, "taskKind")
    context_ids = list(dict.fromkeys(context_ids or []))
    for context_id in context_ids:
        validate_context_id(context_id)
    resources = resolve_resource_items(
        store,
        context_ids,
        task_kind,
        product,
        version,
        environment,
    )
    metadata_summaries = {
        str(item.get("contextId")): item for item in metadata_resource_summaries(store)
    }
    metadata_context_ids = [
        context_id for context_id in context_ids if context_id.startswith("meta_")
    ]
    if instance_id:
        metadata_context_ids.append(f"meta_{instance_id}")
    seen_resource_ids = {str(item.get("contextId")) for item in resources}
    for context_id in dict.fromkeys(metadata_context_ids):
        metadata = metadata_summaries.get(context_id)
        if metadata and context_id not in seen_resource_ids:
            resources.append(metadata_bundle_item(metadata))
            seen_resource_ids.add(context_id)
    return {
        "schemaVersion": 2,
        "resources": resources,
        "prompt": render_system_context_prompt(resources),
    }


def resolve_resource_items(
    store: Store,
    context_ids: list[str],
    task_kind: str,
    product: str | None,
    version: str | None,
    environment: str | None,
) -> list[JsonObject]:
    explicit = set(context_ids)
    items = []
    for resource in store.list_system_context_resources():
        active = active_version(resource)
        if not active:
            continue
        include = resource.get("contextId") in explicit or (
            resource.get("enabled", True)
            and active.get("promptPolicy", {}).get("includeByDefault", True)
            and scope_allows_task(resource.get("scope"), task_kind)
            and policy_allows_task(active.get("promptPolicy", {}), task_kind)
            and metadata_filters_match(resource, product, version, environment)
        )
        if include:
            items.append(bundle_item(resource, active))
    items.sort(
        key=lambda item: (
            -int(item.get("promptPriority") or 0),
            str(item.get("title") or ""),
        )
    )
    return items


def metadata_resource_summaries(store: Store) -> list[JsonObject]:
    summaries = []
    for instance in store.list_metadata_instances():
        instance_id = instance["instanceId"]
        title = instance_id
        if instance.get("remark"):
            title = f"{instance_id} ({instance['remark']})"
        summaries.append(
            {
                "contextId": f"meta_{instance_id}",
                "kind": METADATA_KIND,
                "title": title,
                "description": (
                    f"Metadata adapter: nodes={instance.get('nodeCount', 0)} "
                    f"databases={instance.get('databaseCount', 0)}"
                ),
                "scope": "log_analysis",
                "enabled": True,
                "tags": ["metadata"],
                "product": instance.get("product"),
                "version": instance.get("version"),
                "environment": instance.get("environment"),
                "activeVersionId": None,
                "activeSummary": instance.get("templateType"),
                "contentType": "metadata_adapter",
                "source": "metadata_adapter",
                "updatedAt": instance.get("updated_at"),
            }
        )
    return summaries


def resource_summary(resource: JsonObject, source: str) -> JsonObject:
    active = active_version(resource)
    return {
        "contextId": resource.get("contextId"),
        "kind": resource.get("kind"),
        "title": resource.get("title"),
        "description": resource.get("description"),
        "scope": resource.get("scope"),
        "enabled": bool(resource.get("enabled", True)),
        "tags": resource.get("tags", []),
        "product": resource.get("product"),
        "version": resource.get("version"),
        "environment": resource.get("environment"),
        "activeVersionId": resource.get("activeVersionId"),
        "activeSummary": active.get("summary") if active else None,
        "contentType": active.get("contentType") if active else None,
        "source": source,
        "updatedAt": resource.get("updatedAt"),
    }


def bundle_item(resource: JsonObject, version: JsonObject) -> JsonObject:
    policy = normalize_prompt_policy(version.get("promptPolicy"))
    content = truncate_chars(str(version.get("content") or ""), int(policy["maxChars"]))
    return {
        "contextId": resource.get("contextId"),
        "versionId": version.get("versionId"),
        "kind": resource.get("kind"),
        "title": resource.get("title"),
        "contentType": version.get("contentType"),
        "summary": version.get("summary"),
        "content": content,
        "source": "system_context",
        "promptPriority": policy["priority"],
        "promptChars": policy["maxChars"],
    }


def metadata_bundle_item(summary: JsonObject) -> JsonObject:
    return {
        "contextId": summary.get("contextId"),
        "versionId": None,
        "kind": METADATA_KIND,
        "title": summary.get("title"),
        "contentType": "metadata_adapter",
        "summary": summary.get("description"),
        "content": summary.get("description") or "",
        "source": "metadata_adapter",
        "promptPriority": 50,
        "promptChars": 4000,
    }


def render_system_context_prompt(resources: list[JsonObject]) -> str:
    if not resources:
        return "no system context resources selected\n"
    lines = []
    for item in resources:
        lines.append(
            f"- [{item.get('kind')}] {item.get('title')} "
            f"source={item.get('source')} version={item.get('versionId') or '-'} "
            f"summary={item.get('summary') or '-'}"
        )
        content = str(item.get("content") or "").strip()
        if content:
            lines.append(content)
    return "\n".join(lines) + "\n"


def active_version(resource: JsonObject) -> JsonObject | None:
    active_id = resource.get("activeVersionId")
    versions = resource.get("versions")
    if not isinstance(versions, list):
        return None
    for version in versions:
        if isinstance(version, dict) and version.get("versionId") == active_id:
            return version
    for version in versions:
        if isinstance(version, dict) and version.get("status") == "active":
            return version
    return None


def find_version(resource: JsonObject, version_id: str) -> JsonObject:
    for version in resource.get("versions", []):
        if isinstance(version, dict) and version.get("versionId") == version_id:
            return version
    raise KeyError(f"unknown version {version_id}")


def archive_active_versions(resource: JsonObject) -> None:
    for version in resource.get("versions", []):
        if isinstance(version, dict) and version.get("status") == "active":
            version["status"] = "archived"


def validate_context_id(context_id: str) -> None:
    if not valid_prefixed_id(context_id, "ctx_") and not valid_prefixed_id(
        context_id, "meta_"
    ):
        raise ValueError("invalid contextId")


def validate_version_id(version_id: str) -> None:
    if not valid_prefixed_id(version_id, "ctxver_"):
        raise ValueError("invalid versionId")


def valid_prefixed_id(value: str, prefix: str) -> bool:
    return value.startswith(prefix) and all(
        char.isascii() and (char.isalnum() or char in "_-") for char in value
    )


def normalize_prompt_policy(value: Any) -> JsonObject:
    policy = dict(DEFAULT_PROMPT_POLICY)
    if isinstance(value, dict):
        policy.update(value)
    policy["includeByDefault"] = bool(policy.get("includeByDefault", True))
    try:
        max_chars = int(policy.get("maxChars", 4000))
    except (TypeError, ValueError):
        max_chars = 4000
    policy["maxChars"] = max(200, min(max_chars, 20_000))
    try:
        policy["priority"] = int(policy.get("priority", 0))
    except (TypeError, ValueError):
        policy["priority"] = 0
    policy["allowedTaskKinds"] = [
        str(item)
        for item in policy.get("allowedTaskKinds", [])
        if str(item) in {"log_analysis", "tool_run"}
    ][:20]
    return policy


def normalize_tags(value: Any) -> list[str]:
    if not isinstance(value, list):
        return []
    seen = set()
    result = []
    for item in value:
        text = clean_optional(item, max_chars=64)
        if text and text.lower() not in seen:
            seen.add(text.lower())
            result.append(text)
    return result[:32]


def clean_required(value: Any, field: str, max_chars: int) -> str:
    text = clean_optional(value, max_chars=max_chars)
    if not text:
        raise ValueError(f"{field} is required")
    return text


def clean_optional(value: Any, max_chars: int) -> str | None:
    if value is None:
        return None
    text = str(value).strip()
    if not text:
        return None
    if len(text) > max_chars:
        raise ValueError("value is too long")
    return text


def require_choice(value: Any, choices: set[str], field: str) -> str:
    text = clean_required(value, field, max_chars=120)
    if text not in choices:
        raise ValueError(f"{field} must be one of {', '.join(sorted(choices))}")
    return text


def scope_allows_task(scope: Any, task_kind: str) -> bool:
    return scope == "global" or scope == task_kind


def policy_allows_task(policy: JsonObject, task_kind: str) -> bool:
    allowed = policy.get("allowedTaskKinds")
    return not allowed or task_kind in allowed


def metadata_filters_match(
    resource: JsonObject,
    product: str | None,
    version: str | None,
    environment: str | None,
) -> bool:
    return (
        optional_filter_matches(resource.get("product"), product)
        and optional_filter_matches(resource.get("version"), version)
        and optional_filter_matches(resource.get("environment"), environment)
    )


def optional_filter_matches(expected: Any, actual: str | None) -> bool:
    if expected in (None, ""):
        return True
    return bool(actual) and str(expected).lower() == str(actual).lower()


def truncate_chars(value: str, max_chars: int) -> str:
    if len(value) <= max_chars:
        return value
    return value[:max_chars] + "\n[truncated]"
