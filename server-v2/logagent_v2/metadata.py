from __future__ import annotations

import json
import re
from hashlib import sha256
from typing import Any

from .artifacts import write_artifact_bytes
from .config import Settings
from .fetch import fetch_text, redact_url
from .store import JsonObject, Store, now_iso


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
METADATA_QUERY_FILTERS = {
    "overview": (),
    "nodes": ("nodeId",),
    "databases": ("database",),
    "retention_policies": ("database", "retentionPolicy"),
    "measurements": ("database", "retentionPolicy", "measurement"),
    "fields": ("database", "retentionPolicy", "measurement"),
    "shard_groups": ("database", "retentionPolicy", "ownerNodeId", "ptId", "shardId"),
    "shards": ("database", "retentionPolicy", "ownerNodeId", "ptId", "shardId"),
    "index_groups": ("database", "retentionPolicy", "ownerNodeId", "ptId", "indexId"),
    "indexes": ("database", "retentionPolicy", "ownerNodeId", "ptId", "indexId"),
    "partition_views": ("database", "ownerNodeId", "ptId"),
}
METADATA_QUERY_FILTER_NAMES = {
    "database",
    "retentionPolicy",
    "measurement",
    "nodeId",
    "ownerNodeId",
    "ptId",
    "shardId",
    "indexId",
}


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


def fetch_metadata_snapshot_from_url(
    settings: Settings,
    instance_id: str,
    template_type: str,
    url: str,
    remark: str | None = None,
) -> JsonObject:
    template_type = template_type.lower()
    fetched = fetch_text(settings, url)
    raw = parse_metadata_content(template_type, fetched["content"])
    snapshot = normalize_metadata_snapshot(instance_id, template_type, raw, remark)
    cluster = snapshot.get("cluster")
    if not isinstance(cluster, dict):
        raise ValueError("metadata snapshot has no cluster")
    nodes = cluster.get("nodes") if isinstance(cluster.get("nodes"), list) else []
    return {
        "instance": snapshot.get("instance"),
        "cluster": cluster,
        "nodes": nodes,
        "snapshot": snapshot,
        "fetch": {
            "url": fetched["url"],
            "statusCode": fetched["statusCode"],
            "sizeBytes": fetched["sizeBytes"],
        },
    }


def get_metadata_cluster(store: Store, cluster_id: str) -> JsonObject:
    for instance in store.list_metadata_instances():
        snapshot = store.get_metadata_snapshot(instance["instanceId"])
        cluster = snapshot.get("cluster")
        if isinstance(cluster, dict) and cluster.get("clusterId") == cluster_id:
            return cluster
    raise KeyError(f"unknown metadata cluster {cluster_id}")


def list_metadata_cluster_nodes(store: Store, cluster_id: str) -> list[JsonObject]:
    cluster = get_metadata_cluster(store, cluster_id)
    nodes = cluster.get("nodes")
    return [node for node in ensure_list(nodes) if isinstance(node, dict)]


def refresh_metadata_instance(store: Store, instance_id: str) -> JsonObject:
    current = store.get_metadata_instance(instance_id)
    raw = current.get("raw")
    if not isinstance(raw, dict) or not raw:
        raise ValueError("metadata instance has no raw JSON snapshot")
    template_type = str(current["templateType"]).lower()
    remark = current.get("remark")
    snapshot = normalize_metadata_snapshot(instance_id, template_type, raw, remark)
    instance = store.upsert_metadata_instance(
        instance_id=instance_id,
        remark=remark,
        template_type=template_type,
        snapshot=snapshot,
        raw=raw,
    )
    return {"instance": instance, "snapshot": snapshot}


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
        "indexGroups": ensure_list(rp.get("IndexGroups") or rp.get("indexGroups")),
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


def metadata_context_outline(store: Store, context: JsonObject) -> JsonObject:
    counts = metadata_section_counts(store, context)
    resources = selected_metadata_resources(context)
    first = resources[0] if resources else {}
    return {
        "schemaVersion": 1,
        "kind": "metadata_context_outline",
        "metadataContextPath": "metadata_context.json",
        "fullContextInPackage": False,
        "fullContextAccess": {
            "tool": "logagent.query_metadata",
            "resource": "logagent-v2://run/<run_id>/metadata_context",
            "note": (
                "resources/read metadata_context returns the run outline; use "
                "logagent.query_metadata for bounded metadata slices."
            ),
        },
        "selection": context.get("selection", {}),
        "selected": {
            "instanceId": first.get("instanceId"),
            "instanceIds": [item.get("instanceId") for item in resources],
        },
        "product": first.get("product"),
        "version": first.get("version"),
        "environment": first.get("environment"),
        "resources": resources,
        "counts": {
            "nodes": counts["nodes"],
            "databases": counts["databases"],
            "retentionPolicies": counts["retention_policies"],
            "measurements": counts["measurements"],
            "fields": counts["fields"],
            "shardGroups": counts["shard_groups"],
            "shards": counts["shards"],
            "indexGroups": counts["index_groups"],
            "indexes": counts["indexes"],
            "partitionViews": counts["partition_views"],
        },
        "sections": {
            section: metadata_section_outline(counts[section], filters)
            for section, filters in METADATA_QUERY_FILTERS.items()
        },
        "finalEvidenceAllowed": False,
    }


def query_metadata_context(
    store: Store,
    context: JsonObject,
    arguments: JsonObject,
) -> JsonObject:
    query = parse_metadata_slice_query(arguments)
    all_items = metadata_items_for_section(store, context, query)
    total = len(all_items)
    cursor = query["cursor"]
    limit = query["limit"]
    if cursor > total:
        raise ValueError("cursor is beyond the result set")
    end = min(cursor + limit, total)
    return {
        "schemaVersion": 1,
        "section": query["section"],
        "filters": {
            name: query["filters"].get(name)
            for name in sorted(METADATA_QUERY_FILTER_NAMES)
        },
        "limit": limit,
        "cursor": str(cursor) if cursor else None,
        "total": total,
        "nextCursor": str(end) if end < total else None,
        "truncated": end < total,
        "items": all_items[cursor:end],
    }


def parse_metadata_slice_query(arguments: JsonObject) -> JsonObject:
    if not isinstance(arguments, dict):
        raise ValueError("metadata query arguments must be an object")
    allowed_keys = {"section", "limit", "cursor"} | METADATA_QUERY_FILTER_NAMES
    unknown = sorted(key for key in arguments if key not in allowed_keys)
    if unknown:
        raise ValueError(f"unsupported metadata query field {unknown[0]}")
    section = arguments.get("section")
    if not isinstance(section, str) or not section.strip():
        raise ValueError("section is required")
    section = section.strip()
    if section not in METADATA_QUERY_FILTERS:
        raise ValueError(f"unsupported metadata section {section}")
    limit = coerce_limit(arguments.get("limit"))
    cursor = coerce_cursor(arguments.get("cursor"))
    filters = {
        "database": optional_filter_string(arguments.get("database"), "database"),
        "retentionPolicy": optional_filter_string(
            arguments.get("retentionPolicy"), "retentionPolicy"
        ),
        "measurement": optional_filter_string(arguments.get("measurement"), "measurement"),
        "nodeId": optional_filter_string(arguments.get("nodeId"), "nodeId"),
        "ownerNodeId": optional_filter_int(arguments.get("ownerNodeId"), "ownerNodeId"),
        "ptId": optional_filter_int(arguments.get("ptId"), "ptId"),
        "shardId": optional_filter_int(arguments.get("shardId"), "shardId"),
        "indexId": optional_filter_int(arguments.get("indexId"), "indexId"),
    }
    for name, value in filters.items():
        if value is not None and name not in METADATA_QUERY_FILTERS[section]:
            raise ValueError(f"filter {name} is not supported for metadata section {section}")
    return {
        "section": section,
        "limit": limit,
        "cursor": cursor,
        "filters": filters,
    }


def coerce_limit(value: Any) -> int:
    if value is None:
        return 50
    if isinstance(value, bool) or not isinstance(value, int):
        raise ValueError("limit must be an integer")
    if value < 1 or value > 200:
        raise ValueError("limit must be between 1 and 200")
    return value


def coerce_cursor(value: Any) -> int:
    if value is None:
        return 0
    parsed = coerce_int_value(value)
    if parsed is None or parsed < 0:
        raise ValueError("cursor must be a non-negative integer offset")
    return parsed


def optional_filter_string(value: Any, name: str) -> str | None:
    if value is None:
        return None
    if not isinstance(value, str):
        raise ValueError(f"{name} must be a string")
    stripped = value.strip()
    return stripped or None


def optional_filter_int(value: Any, name: str) -> int | None:
    if value is None:
        return None
    parsed = coerce_int_value(value)
    if parsed is None or parsed < 0:
        raise ValueError(f"{name} must be an unsigned integer")
    return parsed


def metadata_section_outline(count: int, filters: tuple[str, ...]) -> JsonObject:
    return {
        "available": count > 0,
        "count": count,
        "query": {
            "tool": "logagent.query_metadata",
            "limitMax": 200,
            "filters": list(filters),
        },
    }


def metadata_section_counts(store: Store, context: JsonObject) -> dict[str, int]:
    counts = {"overview": 1}
    for section in METADATA_QUERY_FILTERS:
        if section == "overview":
            continue
        counts[section] = len(
            metadata_items_for_section(
                store,
                context,
                {"section": section, "filters": {}, "limit": 200, "cursor": 0},
            )
        )
    return counts | {"overview": len(metadata_overview_items(context))}


def metadata_items_for_section(
    store: Store,
    context: JsonObject,
    query: JsonObject,
) -> list[JsonObject]:
    section = query["section"]
    if section == "overview":
        return metadata_overview_items(context)
    if section == "nodes":
        return metadata_node_items(store, context, query)
    if section == "databases":
        return metadata_database_items(store, context, query)
    if section == "retention_policies":
        return metadata_retention_policy_items(store, context, query)
    if section == "measurements":
        return metadata_measurement_items(store, context, query)
    if section == "fields":
        return metadata_field_items(store, context, query)
    if section == "shard_groups":
        return metadata_shard_group_items(store, context, query)
    if section == "shards":
        return metadata_shard_items(store, context, query)
    if section == "index_groups":
        return metadata_index_group_items(store, context, query)
    if section == "indexes":
        return metadata_index_items(store, context, query)
    if section == "partition_views":
        return metadata_partition_view_items(store, context, query)
    raise ValueError(f"unsupported metadata section {section}")


def metadata_overview_items(context: JsonObject) -> list[JsonObject]:
    return [
        {
            "schemaVersion": context.get("schemaVersion"),
            "selection": context.get("selection", {}),
            "resourceCount": len(selected_metadata_resources(context)),
            "resources": selected_metadata_resources(context),
            "finalEvidenceAllowed": False,
        }
    ]


def metadata_node_items(store: Store, context: JsonObject, query: JsonObject) -> list[JsonObject]:
    items = []
    for _resource, snapshot in selected_metadata_snapshots(store, context):
        for node in ensure_list(snapshot.get("cluster", {}).get("nodes")):
            if not isinstance(node, dict):
                continue
            if not filter_string_matches(node.get("nodeId"), query, "nodeId"):
                continue
            items.append(dict(node))
    return items


def metadata_database_items(
    store: Store, context: JsonObject, query: JsonObject
) -> list[JsonObject]:
    items = []
    for resource, snapshot in selected_metadata_snapshots(store, context):
        for database in cluster_databases(snapshot):
            name = database.get("name")
            if not filter_string_matches(name, query, "database"):
                continue
            policies = ensure_list(database.get("retentionPolicies"))
            items.append(
                {
                    "instanceId": resource.get("instanceId"),
                    "name": name,
                    "defaultRetentionPolicy": database.get("defaultRetentionPolicy"),
                    "replicaN": database.get("replicaN"),
                    "retentionPolicyCount": len(policies),
                }
            )
    return items


def metadata_retention_policy_items(
    store: Store, context: JsonObject, query: JsonObject
) -> list[JsonObject]:
    items = []
    for resource, _snapshot, database, policy in selected_retention_policies(
        store, context, query
    ):
        measurements = ensure_list(policy.get("measurements"))
        shard_groups = ensure_list(policy.get("shardGroups"))
        index_groups = ensure_list(policy.get("indexGroups"))
        items.append(
            {
                "instanceId": resource.get("instanceId"),
                "database": database.get("name"),
                "name": policy.get("name"),
                "replicaN": policy.get("replicaN"),
                "duration": policy.get("duration"),
                "shardGroupDuration": policy.get("shardGroupDuration"),
                "measurementCount": len(measurements),
                "shardGroupCount": len(shard_groups),
                "indexGroupCount": len(index_groups),
            }
        )
    return items


def metadata_measurement_items(
    store: Store, context: JsonObject, query: JsonObject
) -> list[JsonObject]:
    items = []
    for resource, _snapshot, database, policy, measurement in selected_measurements(
        store, context, query
    ):
        fields = ensure_list(measurement.get("schema"))
        items.append(
            {
                "instanceId": resource.get("instanceId"),
                "database": database.get("name"),
                "retentionPolicy": policy.get("name"),
                "name": measurement.get("name"),
                "versionName": measurement.get("versionName"),
                "shardKeyType": measurement.get("shardKeyType"),
                "engineType": measurement.get("engineType"),
                "fieldCount": len(fields),
            }
        )
    return items


def metadata_field_items(
    store: Store, context: JsonObject, query: JsonObject
) -> list[JsonObject]:
    items = []
    for resource, _snapshot, database, policy, measurement in selected_measurements(
        store, context, query
    ):
        for field in ensure_list(measurement.get("schema")):
            if not isinstance(field, dict):
                continue
            items.append(
                {
                    "instanceId": resource.get("instanceId"),
                    "database": database.get("name"),
                    "retentionPolicy": policy.get("name"),
                    "measurement": measurement.get("name"),
                    "name": field.get("name"),
                    "typ": field.get("typ"),
                    "typeLabel": field.get("typeLabel"),
                    "endTime": field.get("endTime"),
                }
            )
    return items


def metadata_shard_group_items(
    store: Store, context: JsonObject, query: JsonObject
) -> list[JsonObject]:
    items = []
    for resource, snapshot, database, policy in selected_retention_policies(
        store, context, query
    ):
        for group in ensure_list(policy.get("shardGroups")):
            if not isinstance(group, dict):
                continue
            shard_ids = shard_ids_for_group(group)
            owners = owner_ids_for_item(group)
            if not filter_int_contains(shard_ids, query, "shardId"):
                continue
            if not pt_owner_filters_match(snapshot, database.get("name"), owners, query):
                continue
            items.append(
                {
                    "instanceId": resource.get("instanceId"),
                    "database": database.get("name"),
                    "retentionPolicy": policy.get("name"),
                    "id": coerce_int_value(first_value(group, "id", "ID")),
                    "startTime": first_value(group, "startTime", "StartTime"),
                    "endTime": first_value(group, "endTime", "EndTime"),
                    "shardIds": shard_ids,
                    "owners": owners,
                    "shardCount": len(shard_ids),
                    "deletedAt": first_value(group, "deletedAt", "DeletedAt"),
                    "truncatedAt": first_value(group, "truncatedAt", "TruncatedAt"),
                    "engineType": first_value(group, "engineType", "EngineType"),
                    "version": first_value(group, "version", "Version"),
                }
            )
    return items


def metadata_shard_items(store: Store, context: JsonObject, query: JsonObject) -> list[JsonObject]:
    items = []
    for resource, snapshot, database, policy in selected_retention_policies(
        store, context, query
    ):
        for group in ensure_list(policy.get("shardGroups")):
            if not isinstance(group, dict):
                continue
            group_owners = owner_ids_for_item(group)
            for shard in shards_for_group(group):
                shard_id = coerce_int_value(first_value(shard, "id", "ID"))
                if not filter_int_matches(shard_id, query, "shardId"):
                    continue
                owners = owner_ids_for_item(shard) or group_owners
                if not pt_owner_filters_match(snapshot, database.get("name"), owners, query):
                    continue
                items.append(
                    {
                        "instanceId": resource.get("instanceId"),
                        "database": database.get("name"),
                        "retentionPolicy": policy.get("name"),
                        "shardGroupId": coerce_int_value(first_value(group, "id", "ID")),
                        "id": shard_id,
                        "owners": owners,
                        "min": first_value(shard, "min", "Min"),
                        "max": first_value(shard, "max", "Max"),
                        "tier": first_value(shard, "tier", "Tier"),
                        "indexId": first_value(shard, "indexId", "IndexID", "IndexId"),
                        "readOnly": first_value(shard, "readOnly", "ReadOnly"),
                        "markDelete": first_value(shard, "markDelete", "MarkDelete"),
                    }
                )
    return items


def metadata_index_group_items(
    store: Store, context: JsonObject, query: JsonObject
) -> list[JsonObject]:
    items = []
    for resource, snapshot, database, policy in selected_retention_policies(
        store, context, query
    ):
        for group in ensure_list(policy.get("indexGroups")):
            if not isinstance(group, dict):
                continue
            indexes = indexes_for_group(group)
            index_ids = [
                item_id
                for item_id in (
                    coerce_int_value(first_value(index, "id", "ID")) for index in indexes
                )
                if item_id is not None
            ]
            owners = owner_ids_for_item(group)
            if not filter_int_contains(index_ids, query, "indexId"):
                continue
            if not pt_owner_filters_match(snapshot, database.get("name"), owners, query):
                continue
            items.append(
                {
                    "instanceId": resource.get("instanceId"),
                    "database": database.get("name"),
                    "retentionPolicy": policy.get("name"),
                    "id": coerce_int_value(first_value(group, "id", "ID")),
                    "startTime": first_value(group, "startTime", "StartTime"),
                    "endTime": first_value(group, "endTime", "EndTime"),
                    "deletedAt": first_value(group, "deletedAt", "DeletedAt"),
                    "engineType": first_value(group, "engineType", "EngineType"),
                    "owners": owners,
                    "indexCount": len(indexes),
                }
            )
    return items


def metadata_index_items(store: Store, context: JsonObject, query: JsonObject) -> list[JsonObject]:
    items = []
    for resource, snapshot, database, policy in selected_retention_policies(
        store, context, query
    ):
        for group in ensure_list(policy.get("indexGroups")):
            if not isinstance(group, dict):
                continue
            group_owners = owner_ids_for_item(group)
            for index in indexes_for_group(group):
                index_id = coerce_int_value(first_value(index, "id", "ID"))
                if not filter_int_matches(index_id, query, "indexId"):
                    continue
                owners = owner_ids_for_item(index) or group_owners
                if not pt_owner_filters_match(snapshot, database.get("name"), owners, query):
                    continue
                items.append(
                    {
                        "instanceId": resource.get("instanceId"),
                        "database": database.get("name"),
                        "retentionPolicy": policy.get("name"),
                        "indexGroupId": coerce_int_value(first_value(group, "id", "ID")),
                        "id": index_id,
                        "tier": first_value(index, "tier", "Tier"),
                        "owners": owners,
                        "markDelete": first_value(index, "markDelete", "MarkDelete"),
                    }
                )
    return items


def metadata_partition_view_items(
    store: Store, context: JsonObject, query: JsonObject
) -> list[JsonObject]:
    items = []
    for resource, snapshot in selected_metadata_snapshots(store, context):
        for view in partition_views(snapshot):
            database = partition_view_database(view)
            pt_id = partition_view_pt_id(view)
            owner_node_id = partition_view_owner_node_id(view)
            if not filter_string_matches(database, query, "database"):
                continue
            if not filter_int_matches(pt_id, query, "ptId"):
                continue
            if not filter_int_matches(owner_node_id, query, "ownerNodeId"):
                continue
            items.append(
                {
                    "instanceId": resource.get("instanceId"),
                    "database": database,
                    "ptId": pt_id,
                    "ownerNodeId": owner_node_id,
                    "status": first_value(view, "status", "Status"),
                    "statusText": first_value(view, "statusText", "StatusText"),
                    "version": first_value(view, "version", "Version"),
                    "replicaGroupId": first_value(view, "replicaGroupId", "RGID"),
                }
            )
    return items


def selected_retention_policies(
    store: Store,
    context: JsonObject,
    query: JsonObject,
) -> list[tuple[JsonObject, JsonObject, JsonObject, JsonObject]]:
    items = []
    for resource, snapshot in selected_metadata_snapshots(store, context):
        for database in cluster_databases(snapshot):
            if not filter_string_matches(database.get("name"), query, "database"):
                continue
            for policy in ensure_list(database.get("retentionPolicies")):
                if not isinstance(policy, dict):
                    continue
                if not filter_string_matches(policy.get("name"), query, "retentionPolicy"):
                    continue
                items.append((resource, snapshot, database, policy))
    return items


def selected_measurements(
    store: Store,
    context: JsonObject,
    query: JsonObject,
) -> list[tuple[JsonObject, JsonObject, JsonObject, JsonObject, JsonObject]]:
    items = []
    for resource, snapshot, database, policy in selected_retention_policies(
        store, context, query
    ):
        for measurement in ensure_list(policy.get("measurements")):
            if not isinstance(measurement, dict):
                continue
            if not filter_string_matches(measurement.get("name"), query, "measurement"):
                continue
            items.append((resource, snapshot, database, policy, measurement))
    return items


def selected_metadata_resources(context: JsonObject) -> list[JsonObject]:
    resources = context.get("resources", [])
    if not isinstance(resources, list):
        return []
    return [
        item
        for item in resources
        if isinstance(item, dict) and isinstance(item.get("instanceId"), str)
    ]


def selected_metadata_snapshots(
    store: Store, context: JsonObject
) -> list[tuple[JsonObject, JsonObject]]:
    snapshots = []
    for resource in selected_metadata_resources(context):
        instance_id = resource["instanceId"]
        try:
            snapshot = store.get_metadata_snapshot(instance_id)
        except KeyError:
            continue
        snapshots.append((resource, snapshot))
    return snapshots


def cluster_databases(snapshot: JsonObject) -> list[JsonObject]:
    cluster = snapshot.get("cluster", {})
    if not isinstance(cluster, dict):
        return []
    return [item for item in ensure_list(cluster.get("databases")) if isinstance(item, dict)]


def partition_views(snapshot: JsonObject) -> list[JsonObject]:
    cluster = snapshot.get("cluster", {})
    if not isinstance(cluster, dict):
        return []
    return [item for item in ensure_list(cluster.get("partitionViews")) if isinstance(item, dict)]


def filter_string_matches(value: Any, query: JsonObject, name: str) -> bool:
    expected = query.get("filters", {}).get(name)
    if expected is None:
        return True
    return str(value) == expected


def filter_int_matches(value: Any, query: JsonObject, name: str) -> bool:
    expected = query.get("filters", {}).get(name)
    if expected is None:
        return True
    return coerce_int_value(value) == expected


def filter_int_contains(values: list[int], query: JsonObject, name: str) -> bool:
    expected = query.get("filters", {}).get(name)
    if expected is None:
        return True
    return expected in values


def pt_owner_filters_match(
    snapshot: JsonObject,
    database: Any,
    pt_ids: list[int],
    query: JsonObject,
) -> bool:
    wanted_pt_id = query.get("filters", {}).get("ptId")
    wanted_owner_node_id = query.get("filters", {}).get("ownerNodeId")
    if wanted_pt_id is not None and wanted_pt_id not in pt_ids:
        return False
    if wanted_owner_node_id is None:
        return True
    if wanted_owner_node_id in pt_ids:
        return True
    for view in partition_views(snapshot):
        if database is not None and str(partition_view_database(view)) != str(database):
            continue
        if partition_view_pt_id(view) in pt_ids and (
            partition_view_owner_node_id(view) == wanted_owner_node_id
        ):
            return True
    return False


def shard_ids_for_group(group: JsonObject) -> list[int]:
    raw_ids = first_value(group, "shardIds", "ShardIds")
    ids = list_int_values(raw_ids)
    if ids:
        return ids
    return [
        shard_id
        for shard_id in (
            coerce_int_value(first_value(shard, "id", "ID")) for shard in shards_for_group(group)
        )
        if shard_id is not None
    ]


def shards_for_group(group: JsonObject) -> list[JsonObject]:
    shards = first_value(group, "shards", "Shards")
    if isinstance(shards, list):
        return [item for item in shards if isinstance(item, dict)]
    return [
        {"id": shard_id, "owners": owner_ids_for_item(group)}
        for shard_id in list_int_values(first_value(group, "shardIds", "ShardIds"))
    ]


def indexes_for_group(group: JsonObject) -> list[JsonObject]:
    indexes = first_value(group, "indexes", "Indexes")
    if isinstance(indexes, list):
        return [item for item in indexes if isinstance(item, dict)]
    return []


def owner_ids_for_item(item: JsonObject) -> list[int]:
    return list_int_values(first_value(item, "owners", "Owners", "ownerIds", "OwnerIds"))


def partition_view_database(view: JsonObject) -> Any:
    return first_value(view, "database", "Database", "db", "DB")


def partition_view_pt_id(view: JsonObject) -> int | None:
    return coerce_int_value(first_value(view, "ptId", "PtId", "PTID", "pt_id"))


def partition_view_owner_node_id(view: JsonObject) -> int | None:
    return coerce_int_value(
        first_value(view, "ownerNodeId", "OwnerNodeID", "OwnerNodeId", "owner")
    )


def first_value(item: JsonObject, *keys: str) -> Any:
    if not isinstance(item, dict):
        return None
    for key in keys:
        if key in item:
            return item.get(key)
    return None


def list_int_values(value: Any) -> list[int]:
    if value is None:
        return []
    if isinstance(value, list):
        values = value
    else:
        values = [value]
    parsed = []
    for item in values:
        number = coerce_int_value(item)
        if number is not None:
            parsed.append(number)
    return parsed


def coerce_int_value(value: Any) -> int | None:
    if isinstance(value, bool):
        return None
    if isinstance(value, int):
        return value
    if isinstance(value, str) and value.strip().isdigit():
        return int(value.strip())
    return None


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
    rp, default_retention_policy_used = find_retention_policy(db, retention_policy)
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
        "defaultRetentionPolicyUsed": default_retention_policy_used,
        "measurement": mst.get("name"),
        "fields": fields,
        "missingFields": sorted(missing),
        "tagsOnly": tags_only,
        "finalEvidenceAllowed": False,
    }


def find_retention_policy(
    db: JsonObject, retention_policy: str | None
) -> tuple[JsonObject | None, bool]:
    policies = db.get("retentionPolicies", [])
    if retention_policy:
        return find_named(policies, retention_policy), False
    default_name = db.get("defaultRetentionPolicy")
    if default_name:
        found = find_named(policies, default_name)
        if found is not None:
            return found, True
    return (policies[0], False) if policies else (None, False)


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


def task_metadata_tool_descriptors() -> list[JsonObject]:
    return [
        get_metadata_topology_descriptor(),
        query_metadata_descriptor(),
        *metadata_tool_descriptors(),
    ]


def get_metadata_topology_descriptor() -> JsonObject:
    return {
        "name": "logagent.get_metadata_topology",
        "description": (
            "Compatibility alias that returns the task metadata overview outline. "
            "Use logagent.query_metadata for bounded metadata slices."
        ),
        "inputSchema": {"type": "object", "additionalProperties": False},
    }


def query_metadata_descriptor() -> JsonObject:
    return {
        "name": "logagent.query_metadata",
        "description": (
            "Read a bounded, paged slice from the current task metadata context. "
            "Returned slices are background context, not final evidence."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "section": {
                    "type": "string",
                    "enum": list(METADATA_QUERY_FILTERS.keys()),
                },
                "database": {"type": "string"},
                "retentionPolicy": {"type": "string"},
                "measurement": {"type": "string"},
                "nodeId": {"type": "string"},
                "ownerNodeId": {"type": "integer", "minimum": 0},
                "ptId": {"type": "integer", "minimum": 0},
                "shardId": {"type": "integer", "minimum": 0},
                "indexId": {"type": "integer", "minimum": 0},
                "limit": {"type": "integer", "minimum": 1, "maximum": 200},
                "cursor": {"oneOf": [{"type": "string"}, {"type": "integer"}]},
            },
            "required": ["section"],
            "additionalProperties": False,
        },
    }


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


def call_task_metadata_tool(
    settings: Settings,
    store: Store,
    run: JsonObject,
    name: str,
    arguments: JsonObject,
    context: JsonObject,
) -> JsonObject:
    if name == "logagent.get_metadata_topology":
        return metadata_context_outline(store, context)
    if name == "logagent.query_metadata":
        value = query_metadata_context(store, context, arguments)
        slice_id = f"slice_{stable_json_digest(arguments)}"
        artifact_path = f"metadata_slices/{slice_id}.json"
        value = {
            **value,
            "metadataContextPath": "metadata_context.json",
            "artifactPath": artifact_path,
            "backgroundRef": f"{artifact_path}#items",
            "finalEvidenceAllowed": False,
        }
        persist_metadata_query_slice(settings, store, run, name, value, artifact_path)
        return value
    return call_metadata_tool(settings, store, run, name, arguments)


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
        snapshot = store.get_metadata_snapshot(instance_id)
        value = {**snapshot, "snapshot": snapshot}
    elif name in {"logagent.get_metadata_field_types", "logagent.get_metadata_tag_fields"}:
        raw_value = query_field_types(
            store=store,
            instance_id=require_string(arguments, "instanceId"),
            database=require_string(arguments, "database"),
            measurement=require_string(arguments, "measurement"),
            retention_policy=optional_string(arguments, "retentionPolicy"),
            field=arguments.get("field"),
            tags_only=name == "logagent.get_metadata_tag_fields",
        )
        if settings is not None and run is not None:
            return metadata_field_tool_task_payload(
                settings=settings,
                store=store,
                run=run,
                tool_name=name,
                arguments=arguments,
                value=raw_value,
            )
        return {**raw_value, "result": raw_value}
    else:
        raise ValueError(f"unsupported metadata tool {name}")
    if settings is not None and run is not None:
        persist_metadata_slice(settings, store, run, name, value)
    return value


def metadata_field_tool_task_payload(
    settings: Settings,
    store: Store,
    run: JsonObject,
    tool_name: str,
    arguments: JsonObject,
    value: JsonObject,
) -> JsonObject:
    prefix = (
        "tag_fields"
        if tool_name == "logagent.get_metadata_tag_fields"
        else "field_types"
    )
    artifact_path = f"metadata_slices/{prefix}_{stable_json_digest(arguments)}.json"
    background_ref = f"{artifact_path}#fields"
    result = {
        **value,
        "artifactPath": artifact_path,
        "backgroundRef": background_ref,
        "createdAt": now_iso(),
        "finalEvidenceAllowed": False,
    }
    persist_metadata_query_slice(settings, store, run, tool_name, result, artifact_path)
    return {
        **value,
        "artifactPath": artifact_path,
        "backgroundRef": background_ref,
        "evidenceRefs": [background_ref],
        "finalEvidenceAllowed": False,
        "result": result,
    }


def persist_metadata_query_slice(
    settings: Settings,
    store: Store,
    run: JsonObject,
    tool_name: str,
    value: JsonObject,
    artifact_path: str,
) -> None:
    data = json.dumps(value, ensure_ascii=True, indent=2).encode("utf-8")
    artifact = write_artifact_bytes(
        settings=settings,
        store=store,
        workspace_id=run["workspace_id"],
        filename=artifact_path.rsplit("/", 1)[-1],
        data=data,
        content_type="application/json",
        schema_name="logagent.v2.metadata_slice.v1",
        preview={"tool": tool_name, "path": artifact_path, "sizeBytes": len(data)},
    )
    store.create_evidence(
        workspace_id=run["workspace_id"],
        run_id=run["id"],
        kind="metadata_slice",
        final_allowed=False,
        summary=f"Metadata background slice from {tool_name}.",
        artifact_id=artifact["id"],
        payload={
            "artifactId": artifact["id"],
            "tool": tool_name,
            "path": artifact_path,
            "backgroundRef": value.get("backgroundRef"),
        },
    )


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


def stable_json_digest(value: JsonObject) -> str:
    data = json.dumps(value, ensure_ascii=True, sort_keys=True, separators=(",", ":"))
    return sha256(data.encode("utf-8")).hexdigest()[:16]


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
