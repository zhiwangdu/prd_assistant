from __future__ import annotations

import json
import re
from typing import Any

from .artifacts import write_artifact_bytes
from .config import Settings
from .fetch import fetch_text, redact_url
from .store import JsonObject, Store


FIELD_TYPE_LABELS = {
    0: "Unknown",
    1: "Integer",
    2: "Unsigned",
    3: "Float",
    4: "String",
    5: "Boolean",
    6: "Tag",
    7: "Unknown",
}

METADATA_CONTEXT_MAX_INSTANCES = 3
METADATA_CONTEXT_MAX_NODES = 10
METADATA_CONTEXT_MAX_DATABASES = 12
METADATA_CONTEXT_MAX_RETENTION_POLICIES = 4
METADATA_CONTEXT_MAX_MEASUREMENTS = 8
METADATA_CONTEXT_MAX_FIELDS = 16
METADATA_TOKEN_RE = re.compile(r"[a-z0-9_.:-]+|[\u4e00-\u9fff]{2,}")


def import_metadata(
    store: Store,
    instance_id: str,
    template_type: str,
    content: str,
    remark: str | None = None,
) -> JsonObject:
    template_type = template_type.lower()
    raw = parse_metadata_content(template_type, content)
    snapshot = normalize_metadata_snapshot(instance_id, template_type, raw, remark)
    instance = store.upsert_metadata_instance(
        instance_id=instance_id,
        remark=remark,
        template_type=template_type,
        snapshot=snapshot,
        raw=raw,
    )
    return {"instance": instance, "snapshot": snapshot}


def preview_metadata_import(
    store: Store,
    instance_id: str,
    template_type: str,
    content: str,
    remark: str | None = None,
    source_url: str | None = None,
) -> JsonObject:
    template_type = template_type.lower()
    raw = parse_metadata_content(template_type, content)
    snapshot = normalize_metadata_snapshot(instance_id, template_type, raw, remark)
    draft = store.create_metadata_import(
        instance_id=instance_id,
        remark=remark,
        template_type=template_type,
        snapshot=snapshot,
        raw=raw,
        source_url=redact_url(source_url) if source_url else None,
    )
    return {"import": metadata_import_preview(draft), "snapshot": snapshot}


def preview_metadata_import_from_url(
    settings: Settings,
    store: Store,
    instance_id: str,
    template_type: str,
    url: str,
    remark: str | None = None,
) -> JsonObject:
    fetched = fetch_text(settings, url)
    result = preview_metadata_import(
        store=store,
        instance_id=instance_id,
        template_type=template_type,
        content=fetched["content"],
        remark=remark,
        source_url=url,
    )
    result["fetch"] = {
        "url": fetched["url"],
        "statusCode": fetched["statusCode"],
        "sizeBytes": fetched["sizeBytes"],
    }
    return result


def import_metadata_from_url(
    settings: Settings,
    store: Store,
    instance_id: str,
    template_type: str,
    url: str,
    remark: str | None = None,
) -> JsonObject:
    preview = preview_metadata_import_from_url(
        settings=settings,
        store=store,
        instance_id=instance_id,
        template_type=template_type,
        url=url,
        remark=remark,
    )
    confirmed = confirm_metadata_import(store, preview["import"]["importId"])
    confirmed["fetch"] = preview["fetch"]
    return confirmed


def confirm_metadata_import(store: Store, import_id: str) -> JsonObject:
    draft = store.get_metadata_import(import_id)
    instance = store.upsert_metadata_instance(
        instance_id=draft["instanceId"],
        remark=draft.get("remark"),
        template_type=draft["templateType"],
        snapshot=draft["snapshot"],
        raw=draft["raw"],
    )
    confirmed = store.update_metadata_import_status(import_id, "confirmed")
    return {
        "import": metadata_import_preview(confirmed),
        "instance": instance,
        "snapshot": draft["snapshot"],
    }


def metadata_import_preview(draft: JsonObject) -> JsonObject:
    snapshot = draft.get("snapshot", {})
    cluster = snapshot.get("cluster", {}) if isinstance(snapshot, dict) else {}
    return {
        "importId": draft["importId"],
        "instanceId": draft["instanceId"],
        "templateType": draft["templateType"],
        "remark": draft.get("remark"),
        "status": draft["status"],
        "sourceUrl": draft.get("sourceUrl"),
        "nodeCount": len(cluster.get("nodes", [])),
        "databaseCount": len(cluster.get("databases", [])),
        "createdAt": draft["createdAt"],
        "updatedAt": draft["updatedAt"],
    }


def parse_metadata_content(template_type: str, content: str) -> JsonObject:
    if template_type in {"json", "opengemini"}:
        value = json.loads(content)
    elif template_type == "yaml":
        value = parse_yaml(content)
    else:
        raise ValueError(f"unsupported metadata templateType {template_type}")
    if not isinstance(value, dict):
        raise ValueError("metadata content must decode to an object")
    return value


def parse_yaml(content: str) -> JsonObject:
    try:
        import yaml  # type: ignore[import-not-found]
    except Exception as error:
        raise ValueError("YAML metadata import requires PyYAML") from error
    value = yaml.safe_load(content)
    if not isinstance(value, dict):
        raise ValueError("YAML metadata content must decode to an object")
    return value


def normalize_metadata_snapshot(
    instance_id: str,
    template_type: str,
    raw: JsonObject,
    remark: str | None,
) -> JsonObject:
    if template_type == "opengemini":
        return normalize_opengemini_snapshot(instance_id, raw, remark)
    return normalize_generic_snapshot(instance_id, template_type, raw, remark)


def normalize_generic_snapshot(
    instance_id: str,
    template_type: str,
    raw: JsonObject,
    remark: str | None,
) -> JsonObject:
    raw_instance = raw.get("instance") if isinstance(raw.get("instance"), dict) else raw
    raw_cluster = raw.get("cluster") if isinstance(raw.get("cluster"), dict) else {}
    nodes = ensure_list(raw.get("nodes") or raw_cluster.get("nodes"))
    databases = ensure_list(raw.get("databases") or raw_cluster.get("databases"))
    instance = {
        "instanceId": instance_id,
        "remark": remark,
        "clusterId": str(
            raw_instance.get("clusterId") or raw_cluster.get("clusterId") or instance_id
        ),
        "product": raw_instance.get("product") or raw_cluster.get("product") or template_type,
        "version": raw_instance.get("version") or raw_cluster.get("version"),
        "environment": raw_instance.get("environment") or raw_cluster.get("environment"),
        "region": raw_instance.get("region"),
        "owner": raw_instance.get("owner"),
        "tags": raw_instance.get("tags") if isinstance(raw_instance.get("tags"), dict) else {},
    }
    cluster = {
        "clusterId": instance["clusterId"],
        "name": raw_cluster.get("name") or instance_id,
        "product": instance["product"],
        "version": instance["version"],
        "environment": instance["environment"],
        "nodes": [
            normalize_generic_node(instance_id, index, node) for index, node in enumerate(nodes)
        ],
        "databases": [normalize_database(name, value) for name, value in named_values(databases)],
        "partitionViews": ensure_list(
            raw.get("partitionViews") or raw_cluster.get("partitionViews")
        ),
    }
    return {
        "schemaVersion": 1,
        "templateType": template_type,
        "instance": instance,
        "cluster": cluster,
    }


def normalize_opengemini_snapshot(
    instance_id: str,
    raw: JsonObject,
    remark: str | None,
) -> JsonObject:
    source_cluster_id = raw.get("ClusterID") or raw.get("clusterId")
    nodes = []
    nodes.extend(normalize_opengemini_nodes(instance_id, "meta", raw.get("MetaNodes")))
    nodes.extend(normalize_opengemini_nodes(instance_id, "data", raw.get("DataNodes")))
    nodes.extend(normalize_opengemini_nodes(instance_id, "sql", raw.get("SqlNodes")))
    databases = [
        normalize_database(name, value)
        for name, value in named_values(raw.get("Databases") or raw.get("databases") or [])
    ]
    instance = {
        "instanceId": instance_id,
        "remark": remark,
        "clusterId": instance_id,
        "product": "opengemini",
        "version": raw.get("Version") or raw.get("version"),
        "environment": raw.get("Environment") or raw.get("environment"),
        "region": raw.get("Region") or raw.get("region"),
        "owner": raw.get("Owner") or raw.get("owner"),
        "tags": {"sourceClusterId": str(source_cluster_id)} if source_cluster_id else {},
    }
    cluster = {
        "clusterId": instance_id,
        "name": raw.get("Name") or instance_id,
        "product": "opengemini",
        "version": instance["version"],
        "environment": instance["environment"],
        "nodes": nodes,
        "databases": databases,
        "partitionViews": normalize_partition_views(raw.get("PtView") or raw.get("PtViews")),
        "labels": {
            "term": raw.get("Term"),
            "numOfShards": raw.get("NumOfShards"),
        },
    }
    return {
        "schemaVersion": 1,
        "templateType": "opengemini",
        "instance": instance,
        "cluster": cluster,
    }


def normalize_opengemini_nodes(
    instance_id: str, role: str, value: Any
) -> list[JsonObject]:
    nodes = []
    for index, node in enumerate(ensure_list(value)):
        if not isinstance(node, dict):
            continue
        raw_id = node.get("ID") or node.get("NodeID") or node.get("nodeId") or index
        nodes.append(
            {
                "nodeId": f"{instance_id}:{role}-{raw_id}",
                "instanceId": instance_id,
                "hostname": node.get("Host") or node.get("TCPHost") or node.get("host"),
                "host": node.get("Host") or node.get("TCPHost") or node.get("RPCAddr"),
                "role": role,
                "zone": node.get("Az") or node.get("Zone"),
                "status": node.get("Status") or node.get("status"),
                "labels": {
                    key: value
                    for key, value in node.items()
                    if key not in {"Host", "TCPHost", "RPCAddr", "Az", "Zone", "Status"}
                },
            }
        )
    return nodes


def normalize_database(name: str, value: Any) -> JsonObject:
    db = value if isinstance(value, dict) else {}
    retention_policies = [
        normalize_retention_policy(rp_name, rp_value)
        for rp_name, rp_value in named_values(
            db.get("RetentionPolicies") or db.get("retentionPolicies") or []
        )
    ]
    return {
        "name": str(db.get("Name") or db.get("name") or name),
        "defaultRetentionPolicy": db.get("DefaultRetentionPolicy")
        or db.get("defaultRetentionPolicy")
        or db.get("DefaultRP"),
        "replicaN": db.get("ReplicaN") or db.get("replicaN"),
        "retentionPolicies": retention_policies,
    }


def normalize_retention_policy(name: str, value: Any) -> JsonObject:
    rp = value if isinstance(value, dict) else {}
    measurements = [
        normalize_measurement(measurement_name, measurement_value)
        for measurement_name, measurement_value in named_values(
            rp.get("Measurements") or rp.get("measurements") or []
        )
    ]
    return {
        "name": str(rp.get("Name") or rp.get("name") or name),
        "replicaN": rp.get("ReplicaN") or rp.get("replicaN"),
        "duration": rp.get("Duration") or rp.get("duration"),
        "shardGroupDuration": rp.get("ShardGroupDuration") or rp.get("shardGroupDuration"),
        "measurements": measurements,
        "shardGroups": ensure_list(rp.get("ShardGroups") or rp.get("shardGroups")),
    }


def normalize_measurement(name: str, value: Any) -> JsonObject:
    measurement = value if isinstance(value, dict) else {}
    schema = measurement.get("Schema") or measurement.get("schema") or {}
    return {
        "name": str(measurement.get("Name") or measurement.get("name") or name),
        "versionName": measurement.get("VersionName") or measurement.get("versionName"),
        "shardKeyType": measurement.get("ShardKeyType") or measurement.get("shardKeyType"),
        "schema": normalize_schema(schema),
        "engineType": measurement.get("EngineType") or measurement.get("engineType"),
    }


def normalize_schema(schema: Any) -> list[JsonObject]:
    fields = []
    if isinstance(schema, dict):
        iterable = schema.items()
    else:
        iterable = (
            (item.get("name"), item) for item in ensure_list(schema) if isinstance(item, dict)
        )
    for field_name, field_value in iterable:
        if field_name is None:
            continue
        typ, end_time = parse_field_type(field_value)
        fields.append(
            {
                "name": str(field_name),
                "typ": typ,
                "typeLabel": FIELD_TYPE_LABELS.get(typ, "Unknown"),
                "endTime": end_time,
            }
        )
    return fields


def parse_field_type(value: Any) -> tuple[int, Any]:
    direct = coerce_int(value)
    if direct is not None:
        return direct, None
    if not isinstance(value, dict):
        return 0, None
    for key in ("Typ", "Type", "type", "typ"):
        parsed = coerce_int(value.get(key))
        if parsed is not None:
            return parsed, value.get("EndTime") or value.get("endTime")
    return 0, value.get("EndTime") or value.get("endTime")


def coerce_int(value: Any) -> int | None:
    if isinstance(value, bool):
        return None
    if isinstance(value, int):
        return value
    if isinstance(value, str) and value.strip().isdigit():
        return int(value.strip())
    return None


def normalize_partition_views(value: Any) -> list[JsonObject]:
    views = []
    for item in ensure_list(value):
        if isinstance(item, dict):
            views.append(item)
    return views


def ensure_list(value: Any) -> list[Any]:
    if value is None:
        return []
    if isinstance(value, list):
        return value
    if isinstance(value, dict):
        return list(value.values())
    return []


def named_values(value: Any) -> list[tuple[str, Any]]:
    if isinstance(value, dict):
        return [(str(name), item) for name, item in value.items()]
    result = []
    for index, item in enumerate(ensure_list(value)):
        if isinstance(item, dict):
            name = item.get("Name") or item.get("name") or str(index)
        else:
            name = str(index)
        result.append((str(name), item))
    return result


def normalize_generic_node(instance_id: str, index: int, value: Any) -> JsonObject:
    node = value if isinstance(value, dict) else {}
    return {
        "nodeId": str(node.get("nodeId") or node.get("id") or f"{instance_id}:node-{index}"),
        "instanceId": instance_id,
        "hostname": node.get("hostname") or node.get("host"),
        "host": node.get("host"),
        "role": node.get("role"),
        "zone": node.get("zone"),
        "status": node.get("status"),
        "labels": node.get("labels") if isinstance(node.get("labels"), dict) else {},
    }


def build_metadata_context(
    store: Store,
    workspace_id: str,
    run_id: str,
    max_instances: int = METADATA_CONTEXT_MAX_INSTANCES,
) -> JsonObject:
    workspace = store.get_workspace(workspace_id)
    instances = store.list_metadata_instances()
    selected = select_metadata_instances(
        store=store,
        instances=instances,
        question=workspace.get("question", ""),
        task_mode=workspace.get("mode", ""),
        max_instances=max_instances,
    )
    return {
        "schemaVersion": 1,
        "workspaceId": workspace_id,
        "runId": run_id,
        "selection": {
            "mode": "auto",
            "totalInstances": len(instances),
            "selectedInstances": len(selected),
            "maxInstances": max_instances,
        },
        "resources": selected,
        "finalEvidenceAllowed": False,
    }


def select_metadata_instances(
    store: Store,
    instances: list[JsonObject],
    question: str,
    task_mode: str,
    max_instances: int,
) -> list[JsonObject]:
    scored: list[tuple[int, int, str, JsonObject]] = []
    for index, instance in enumerate(instances):
        snapshot = store.get_metadata_snapshot(instance["instanceId"])
        score, match_reasons = metadata_match_score(instance, snapshot, question, task_mode)
        if score > 0:
            reason = "auto"
        elif len(instances) == 1:
            reason = "default_single"
        else:
            continue
        scored.append(
            (
                score,
                index,
                reason,
                metadata_context_resource(instance, snapshot, reason, score, match_reasons),
            )
        )
    scored.sort(key=lambda item: (-item[0], item[1]))
    return [item[3] for item in scored[: max(0, max_instances)]]


def metadata_context_resource(
    instance_summary: JsonObject,
    snapshot: JsonObject,
    selection_reason: str,
    match_score: int,
    match_reasons: list[str],
) -> JsonObject:
    instance = snapshot.get("instance", {}) if isinstance(snapshot.get("instance"), dict) else {}
    cluster = snapshot.get("cluster", {}) if isinstance(snapshot.get("cluster"), dict) else {}
    nodes = cluster.get("nodes", []) if isinstance(cluster.get("nodes"), list) else []
    databases = cluster.get("databases", []) if isinstance(cluster.get("databases"), list) else []
    return {
        "kind": "metadata_instance",
        "instanceId": instance_summary["instanceId"],
        "templateType": instance_summary.get("templateType"),
        "selectionReason": selection_reason,
        "matchScore": match_score,
        "matchReasons": match_reasons[:8],
        "remark": instance_summary.get("remark") or instance.get("remark"),
        "product": instance.get("product") or instance_summary.get("product"),
        "version": instance.get("version") or instance_summary.get("version"),
        "environment": instance.get("environment") or instance_summary.get("environment"),
        "cluster": {
            "clusterId": cluster.get("clusterId"),
            "name": cluster.get("name"),
            "product": cluster.get("product"),
            "version": cluster.get("version"),
            "environment": cluster.get("environment"),
            "nodeCount": len(nodes),
            "databaseCount": len(databases),
            "nodes": [node_outline(node) for node in nodes[:METADATA_CONTEXT_MAX_NODES]],
            "databases": [
                database_outline(database)
                for database in databases[:METADATA_CONTEXT_MAX_DATABASES]
            ],
        },
        "finalEvidenceAllowed": False,
    }


def node_outline(node: JsonObject) -> JsonObject:
    return {
        "nodeId": node.get("nodeId"),
        "hostname": node.get("hostname"),
        "host": node.get("host"),
        "role": node.get("role"),
        "zone": node.get("zone"),
        "status": node.get("status"),
    }


def database_outline(database: JsonObject) -> JsonObject:
    policies = (
        database.get("retentionPolicies", [])
        if isinstance(database.get("retentionPolicies"), list)
        else []
    )
    return {
        "name": database.get("name"),
        "defaultRetentionPolicy": database.get("defaultRetentionPolicy"),
        "replicaN": database.get("replicaN"),
        "retentionPolicyCount": len(policies),
        "retentionPolicies": [
            retention_policy_outline(policy)
            for policy in policies[:METADATA_CONTEXT_MAX_RETENTION_POLICIES]
        ],
    }


def retention_policy_outline(policy: JsonObject) -> JsonObject:
    measurements = (
        policy.get("measurements", []) if isinstance(policy.get("measurements"), list) else []
    )
    shard_groups = (
        policy.get("shardGroups", []) if isinstance(policy.get("shardGroups"), list) else []
    )
    return {
        "name": policy.get("name"),
        "replicaN": policy.get("replicaN"),
        "duration": policy.get("duration"),
        "shardGroupDuration": policy.get("shardGroupDuration"),
        "measurementCount": len(measurements),
        "shardGroupCount": len(shard_groups),
        "measurements": [
            measurement_outline(measurement)
            for measurement in measurements[:METADATA_CONTEXT_MAX_MEASUREMENTS]
        ],
    }


def measurement_outline(measurement: JsonObject) -> JsonObject:
    fields = (
        measurement.get("schema", []) if isinstance(measurement.get("schema"), list) else []
    )
    return {
        "name": measurement.get("name"),
        "versionName": measurement.get("versionName"),
        "engineType": measurement.get("engineType"),
        "fieldCount": len(fields),
        "fields": [field_outline(field) for field in fields[:METADATA_CONTEXT_MAX_FIELDS]],
    }


def field_outline(field: JsonObject) -> JsonObject:
    return {
        "name": field.get("name"),
        "typ": field.get("typ"),
        "typeLabel": field.get("typeLabel"),
    }


def metadata_match_score(
    instance_summary: JsonObject,
    snapshot: JsonObject,
    question: str,
    task_mode: str,
) -> tuple[int, list[str]]:
    haystack = f"{question}\n{task_mode}".lower()
    score = 0
    reasons: list[str] = []
    instance = snapshot.get("instance", {}) if isinstance(snapshot.get("instance"), dict) else {}
    cluster = snapshot.get("cluster", {}) if isinstance(snapshot.get("cluster"), dict) else {}

    weighted_terms = [
        ("instanceId", instance_summary.get("instanceId"), 10),
        ("remark", instance_summary.get("remark") or instance.get("remark"), 5),
        ("product", instance.get("product") or instance_summary.get("product"), 4),
        ("environment", instance.get("environment") or instance_summary.get("environment"), 4),
        ("version", instance.get("version") or instance_summary.get("version"), 3),
        ("cluster", cluster.get("name") or cluster.get("clusterId"), 4),
    ]
    for label, value, weight in weighted_terms:
        matched = score_term(value, haystack)
        if matched:
            score += weight
            reasons.append(f"{label}:{matched}")

    for node in cluster.get("nodes", []) if isinstance(cluster.get("nodes"), list) else []:
        for label, value, weight in [
            ("node", node.get("nodeId"), 4),
            ("host", node.get("host") or node.get("hostname"), 4),
            ("role", node.get("role"), 2),
            ("status", node.get("status"), 1),
        ]:
            matched = score_term(value, haystack)
            if matched:
                score += weight
                reasons.append(f"{label}:{matched}")

    for database in (
        cluster.get("databases", []) if isinstance(cluster.get("databases"), list) else []
    ):
        matched_db = score_term(database.get("name"), haystack)
        if matched_db:
            score += 5
            reasons.append(f"database:{matched_db}")
        policies = (
            database.get("retentionPolicies", [])
            if isinstance(database.get("retentionPolicies"), list)
            else []
        )
        for policy in policies:
            matched_policy = score_term(policy.get("name"), haystack)
            if matched_policy:
                score += 2
                reasons.append(f"retentionPolicy:{matched_policy}")
            measurements = (
                policy.get("measurements", [])
                if isinstance(policy.get("measurements"), list)
                else []
            )
            for measurement in measurements:
                matched_measurement = score_term(measurement.get("name"), haystack)
                if matched_measurement:
                    score += 5
                    reasons.append(f"measurement:{matched_measurement}")
                fields = (
                    measurement.get("schema", [])
                    if isinstance(measurement.get("schema"), list)
                    else []
                )
                for field in fields:
                    matched_field = score_term(field.get("name"), haystack)
                    if matched_field:
                        score += 2
                        reasons.append(f"field:{matched_field}")
    return score, dedupe_preserve_order(reasons)


def score_term(value: Any, haystack: str) -> str | None:
    if value is None:
        return None
    normalized = str(value).strip().lower()
    if not meaningful_metadata_term(normalized):
        return None
    if normalized in haystack:
        return normalized
    for token in metadata_match_terms(normalized):
        if token in haystack:
            return token
    return None


def metadata_match_terms(value: str) -> list[str]:
    return [token for token in METADATA_TOKEN_RE.findall(value) if meaningful_metadata_term(token)]


def meaningful_metadata_term(value: str) -> bool:
    if not value:
        return False
    if re.search(r"[\u4e00-\u9fff]", value):
        return len(value) >= 2
    return len(value) >= 3


def dedupe_preserve_order(values: list[str]) -> list[str]:
    seen = set()
    result = []
    for value in values:
        if value in seen:
            continue
        seen.add(value)
        result.append(value)
    return result


def persist_metadata_context(
    settings: Settings,
    store: Store,
    workspace_id: str,
    run_id: str,
) -> JsonObject:
    context = build_metadata_context(store=store, workspace_id=workspace_id, run_id=run_id)
    data = json.dumps(context, ensure_ascii=True, indent=2).encode("utf-8")
    artifact = write_artifact_bytes(
        settings=settings,
        store=store,
        workspace_id=workspace_id,
        filename="metadata_context.json",
        data=data,
        content_type="application/json",
        schema_name="logagent.v2.metadata_context.v1",
        preview={
            "resourceCount": len(context["resources"]),
            "totalInstances": context["selection"]["totalInstances"],
        },
    )
    store.create_evidence(
        workspace_id=workspace_id,
        run_id=run_id,
        kind="metadata_context",
        final_allowed=False,
        summary=f"Metadata Context captured {len(context['resources'])} instance outline(s).",
        artifact_id=artifact["id"],
        payload={
            "artifactId": artifact["id"],
            "path": "metadata_context.json",
            "resourceCount": len(context["resources"]),
        },
    )
    return {"context": context, "artifact": artifact}


def query_field_types(
    store: Store,
    instance_id: str,
    database: str,
    measurement: str,
    retention_policy: str | None = None,
    field: str | list[str] | None = None,
    tags_only: bool = False,
) -> JsonObject:
    snapshot = store.get_metadata_snapshot(instance_id)
    db = find_named(snapshot.get("cluster", {}).get("databases", []), database)
    if db is None:
        raise ValueError(f"database {database} not found")
    rp = find_retention_policy(db, retention_policy)
    if rp is None:
        raise ValueError("retention policy not found")
    mst = find_named(rp.get("measurements", []), measurement)
    if mst is None:
        raise ValueError(f"measurement {measurement} not found")
    wanted = normalize_field_filter(field)
    fields = []
    missing = set(wanted or [])
    for item in mst.get("schema", []):
        name = item.get("name")
        if wanted is not None and name not in wanted:
            continue
        if tags_only and item.get("typ") != 6:
            continue
        fields.append(item)
        missing.discard(name)
    return {
        "schemaVersion": 1,
        "instanceId": instance_id,
        "database": db.get("name"),
        "retentionPolicy": rp.get("name"),
        "measurement": mst.get("name"),
        "fields": fields,
        "missingFields": sorted(missing),
        "tagsOnly": tags_only,
        "finalEvidenceAllowed": False,
    }


def find_retention_policy(db: JsonObject, retention_policy: str | None) -> JsonObject | None:
    policies = db.get("retentionPolicies", [])
    if retention_policy:
        return find_named(policies, retention_policy)
    default_name = db.get("defaultRetentionPolicy")
    if default_name:
        found = find_named(policies, default_name)
        if found is not None:
            return found
    return policies[0] if policies else None


def find_named(items: list[JsonObject], name: str) -> JsonObject | None:
    for item in items:
        if item.get("name") == name:
            return item
    return None


def normalize_field_filter(field: str | list[str] | None) -> set[str] | None:
    if field is None:
        return None
    if isinstance(field, str):
        return {field}
    return {item for item in field if isinstance(item, str)}


def metadata_tool_descriptors() -> list[JsonObject]:
    return [
        {
            "name": "logagent.list_metadata_instances",
            "description": "List imported V2 metadata instances.",
            "inputSchema": {"type": "object", "additionalProperties": False},
        },
        {
            "name": "logagent.get_metadata_snapshot",
            "description": "Read one imported metadata snapshot by instanceId.",
            "inputSchema": {
                "type": "object",
                "properties": {"instanceId": {"type": "string", "minLength": 1}},
                "required": ["instanceId"],
                "additionalProperties": False,
            },
        },
        metadata_field_types_descriptor("logagent.get_metadata_field_types", False),
        metadata_field_types_descriptor("logagent.get_metadata_tag_fields", True),
    ]


def metadata_field_types_descriptor(name: str, tags_only: bool) -> JsonObject:
    properties: JsonObject = {
        "instanceId": {"type": "string", "minLength": 1},
        "database": {"type": "string", "minLength": 1},
        "measurement": {"type": "string", "minLength": 1},
        "retentionPolicy": {"type": "string"},
    }
    if not tags_only:
        properties["field"] = {
            "oneOf": [
                {"type": "string"},
                {"type": "array", "items": {"type": "string"}},
            ]
        }
    return {
        "name": name,
        "description": "Query imported metadata field types.",
        "inputSchema": {
            "type": "object",
            "properties": properties,
            "required": ["instanceId", "database", "measurement"],
            "additionalProperties": False,
        },
    }


def call_metadata_tool(
    settings: Settings | None,
    store: Store,
    run: JsonObject | None,
    name: str,
    arguments: JsonObject,
) -> JsonObject:
    if name == "logagent.list_metadata_instances":
        value = {"instances": store.list_metadata_instances()}
    elif name == "logagent.get_metadata_snapshot":
        instance_id = require_string(arguments, "instanceId")
        value = store.get_metadata_snapshot(instance_id)
    elif name in {"logagent.get_metadata_field_types", "logagent.get_metadata_tag_fields"}:
        value = query_field_types(
            store=store,
            instance_id=require_string(arguments, "instanceId"),
            database=require_string(arguments, "database"),
            measurement=require_string(arguments, "measurement"),
            retention_policy=optional_string(arguments, "retentionPolicy"),
            field=arguments.get("field"),
            tags_only=name == "logagent.get_metadata_tag_fields",
        )
    else:
        raise ValueError(f"unsupported metadata tool {name}")
    if settings is not None and run is not None:
        persist_metadata_slice(settings, store, run, name, value)
    return value


def persist_metadata_slice(
    settings: Settings,
    store: Store,
    run: JsonObject,
    tool_name: str,
    value: JsonObject,
) -> None:
    data = json.dumps(value, ensure_ascii=True, indent=2).encode("utf-8")
    artifact = write_artifact_bytes(
        settings=settings,
        store=store,
        workspace_id=run["workspace_id"],
        filename=f"{tool_name.removeprefix('logagent.').replace('.', '_')}.json",
        data=data,
        content_type="application/json",
        schema_name="logagent.v2.metadata_slice.v1",
        preview={"tool": tool_name, "sizeBytes": len(data)},
    )
    store.create_evidence(
        workspace_id=run["workspace_id"],
        run_id=run["id"],
        kind="metadata_slice",
        final_allowed=False,
        summary=f"Metadata background slice from {tool_name}.",
        artifact_id=artifact["id"],
        payload={"artifactId": artifact["id"], "tool": tool_name},
    )


def require_string(arguments: JsonObject, field: str) -> str:
    value = arguments.get(field)
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"{field} is required")
    return value.strip()


def optional_string(arguments: JsonObject, field: str) -> str | None:
    value = arguments.get(field)
    if not isinstance(value, str) or not value.strip():
        return None
    return value.strip()
