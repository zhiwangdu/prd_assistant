from __future__ import annotations

import json
from typing import Any

from .artifacts import write_artifact_bytes
from .config import Settings
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
