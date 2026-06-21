from __future__ import annotations

from collections.abc import Iterable, Sequence
import base64
from datetime import UTC, datetime
import email.utils
import hmac
import json
import os
import re
import shutil
import subprocess
import time
import urllib.error
import urllib.parse
import urllib.request
from fnmatch import fnmatchcase
from hashlib import sha256
from pathlib import Path

from .artifacts import resolve_artifact_path, write_artifact_bytes
from .config import Settings, ToolDefinition
from .evidence import (
    TextFile,
    build_manifest,
    collect_text_files,
    grep_text_files,
    materialize_tool_inputs,
    resolve_text_file_selector,
    search_keywords,
    write_json_artifact,
)
from .fetch import fetch_catalog_descriptor
from .metadata import (
    metadata_field_filter_schema,
    normalize_field_filter_value,
    query_field_types,
)
from .store import JsonObject, Store

PARAM_PLACEHOLDER_RE = re.compile(r"\{params\.([A-Za-z0-9_]+)\}")
UNSUPPORTED_PLACEHOLDER_RE = re.compile(r"\{[A-Za-z_][A-Za-z0-9_.]*\}")
PREPROCESS_LOG_PACKAGE_ID = "logagent.preprocess_log_package"
PPROF_ANALYZER_ID = "pprof_analyzer"
HUAWEI_PACKAGE_SYNC_TOOL_ID = "logagent.huawei_cloud_package_sync"
METADATA_LIST_INSTANCES_ID = "logagent.list_metadata_instances"
METADATA_GET_SNAPSHOT_ID = "logagent.get_metadata_snapshot"
METADATA_GET_FIELD_TYPES_ID = "logagent.get_metadata_field_types"
METADATA_GET_TAG_FIELDS_ID = "logagent.get_metadata_tag_fields"
METADATA_TOOL_IDS = {
    METADATA_LIST_INSTANCES_ID,
    METADATA_GET_SNAPSHOT_ID,
    METADATA_GET_FIELD_TYPES_ID,
    METADATA_GET_TAG_FIELDS_ID,
}
STORAGE_TOOL_IDS = {"opengemini_storage_analyzer", "influxdb_storage_analyzer"}
SOURCE_BUILT_ANALYZERS: tuple[tuple[str, str], ...] = (
    ("flux_query_analyzer", "Flux Query Analyzer"),
    ("influxql_analyzer", "InfluxQL Analyzer"),
    ("opengemini_storage_analyzer", "openGemini Storage Analyzer"),
    ("influxdb_storage_analyzer", "InfluxDB Storage Analyzer"),
)
MAX_HUAWEI_QUERY_ROWS = 200


def tool_descriptors(settings: Settings) -> list[JsonObject]:
    descriptors = [
        configured_tool_descriptor(tool)
        for tool in settings.tools
    ]
    descriptors.extend(built_in_tool_descriptors(settings))
    return descriptors


def configured_tool_descriptor(tool: ToolDefinition) -> JsonObject:
    command_state = command_file_state(tool.command)
    runnable = bool(tool.enabled and command_state["executable"])
    return {
        "toolId": tool.id,
        "displayName": tool.display_name,
        "description": (
            f"Configured Tool Runner command with up to {tool.max_input_files} input file(s)."
        ),
        "source": "configured",
        "tags": ["configured", "manual-run", "tool-runner", "external"],
        "enabled": tool.enabled,
        "backend": "command",
        "readOnly": False,
        "editable": True,
        "exportable": runnable,
        "runnable": runnable,
        "commandState": command_state,
        "minFiles": 1,
        "maxFiles": tool.max_input_files,
        "maxInputFiles": tool.max_input_files,
        "match": {
            "filePatterns": list(tool.match_file_patterns),
            "keywords": list(tool.match_keywords),
        },
        "acceptedSuffixes": list(tool.match_file_patterns),
        "paramsSchema": configured_tool_params_schema(tool),
        "paramsTemplate": configured_tool_params_template(tool),
        "outputViews": ["summary", "findings", "stdout", "stderr"],
    }


def command_file_state(command: str) -> JsonObject:
    normalized = command.strip()
    if not normalized:
        return {
            "path": command,
            "absolute": False,
            "exists": False,
            "executable": False,
            "reason": "command_missing",
        }

    path = Path(normalized)
    absolute = path.is_absolute()
    exists = bool(absolute and path.is_file())
    executable = bool(exists and os.access(path, os.X_OK))
    reason = None
    if not absolute:
        reason = "command_not_absolute"
    elif not exists:
        reason = "command_file_not_found"
    elif not executable:
        reason = "command_not_executable"
    return {
        "path": normalized,
        "absolute": absolute,
        "exists": exists,
        "executable": executable,
        "reason": reason,
    }


def tool_catalog(settings: Settings) -> JsonObject:
    descriptors = tool_descriptors(settings)
    return {
        "schemaVersion": 1,
        "tools": descriptors,
        "toolPlugins": descriptors,
        "configuredTools": configured_tool_summaries(settings, descriptors),
        "sourceBuiltAnalyzers": source_built_analyzer_summaries(settings, descriptors),
    }


def configured_tool_summaries(
    settings: Settings,
    descriptors: list[JsonObject] | None = None,
) -> list[JsonObject]:
    return [
        {
            "toolId": tool["toolId"],
            "enabled": tool["enabled"],
            "timeoutSeconds": find_configured_tool_timeout(settings, tool["toolId"]),
            "maxInputFiles": tool.get("maxInputFiles", tool.get("maxFiles")),
            "configuredArgs": list(find_configured_tool_args(settings, tool["toolId"])),
            "match": tool.get("match", {"filePatterns": [], "keywords": []}),
        }
        for tool in (descriptors or tool_descriptors(settings))
        if tool.get("source") == "configured"
    ]


def runnable_configured_tool_ids(settings: Settings) -> list[str]:
    return [
        tool["toolId"]
        for tool in tool_descriptors(settings)
        if tool["enabled"]
        and tool.get("runnable")
        and tool.get("source") == "configured"
        and not tool.get("manualOnly")
    ]


def source_built_analyzer_summaries(
    settings: Settings,
    descriptors: list[JsonObject] | None = None,
) -> list[JsonObject]:
    descriptor_by_id = {
        str(tool.get("toolId")): tool for tool in (descriptors or tool_descriptors(settings))
    }
    configured_by_id = {tool.id: tool for tool in settings.tools}
    summaries: list[JsonObject] = []
    for tool_id, display_name in SOURCE_BUILT_ANALYZERS:
        descriptor = descriptor_by_id.get(tool_id)
        configured = configured_by_id.get(tool_id)
        enabled = bool(descriptor.get("enabled")) if descriptor else False
        runnable = bool(descriptor.get("runnable")) if descriptor else False
        command_state = command_file_state(configured.command) if configured else None
        if configured is None:
            status = "missing"
            status_reason = "not_registered"
        elif not configured.enabled:
            status = "disabled"
            status_reason = "disabled"
        elif command_state and not command_state["executable"]:
            status = "unavailable"
            status_reason = command_state.get("reason") or "command_unavailable"
        else:
            status = "registered"
            status_reason = None
        summaries.append(
            {
                "toolId": tool_id,
                "displayName": display_name,
                "registered": configured is not None,
                "enabled": enabled,
                "runnable": runnable,
                "status": status,
                "statusReason": status_reason,
                "commandPath": configured.command if configured is not None else None,
                "commandExists": (
                    bool(command_state["exists"]) if command_state is not None else False
                ),
                "commandExecutable": (
                    bool(command_state["executable"]) if command_state is not None else False
                ),
                "timeoutSeconds": (
                    find_configured_tool_timeout(settings, tool_id)
                    if configured is not None
                    else None
                ),
                "maxInputFiles": (
                    descriptor.get("maxInputFiles") if descriptor is not None else None
                ),
            }
        )
    return summaries


def find_configured_tool_args(settings: Settings, tool_id: str) -> tuple[str, ...]:
    for tool in settings.tools:
        if tool.id == tool_id:
            return tool.args
    return ()


def find_configured_tool_timeout(settings: Settings, tool_id: str) -> int | None:
    for tool in settings.tools:
        if tool.id == tool_id:
            return tool.timeout_seconds
    if tool_id == PPROF_ANALYZER_ID:
        return 60
    return None


def built_in_tool_descriptors(settings: Settings) -> list[JsonObject]:
    return [
        preprocess_descriptor(),
        pprof_descriptor(settings),
        *metadata_catalog_descriptors(),
        fetch_catalog_descriptor(settings),
        huawei_package_sync_descriptor(settings),
    ]

def configured_tool_params_schema(tool: ToolDefinition) -> JsonObject:
    base = tool.params_schema or {
        "type": "object",
        "properties": {},
        "additionalProperties": False,
    }
    schema = dict(base)
    properties = schema.get("properties")
    properties = dict(properties) if isinstance(properties, dict) else {}
    readonly_properties = configured_tool_readonly_schema_properties(tool)
    for key, value in readonly_properties.items():
        schema.setdefault(key, value)
        properties.setdefault(key, value)
    if tool_requires_input(tool):
        properties["inputFiles"] = {
            "type": "array",
            "items": {"type": "string"},
            "description": "Current Workspace paths under extracted/... or tool_inputs/...",
        }
    schema["properties"] = properties
    return schema


def configured_tool_readonly_schema_properties(tool: ToolDefinition) -> JsonObject:
    return {
        "configuredArgs": {
            "type": "array",
            "items": {"type": "string"},
            "readOnly": True,
            "value": list(tool.args),
        },
        "match": {
            "type": "object",
            "properties": {
                "filePatterns": {
                    "type": "array",
                    "items": {"type": "string"},
                    "value": list(tool.match_file_patterns),
                },
                "keywords": {
                    "type": "array",
                    "items": {"type": "string"},
                    "value": list(tool.match_keywords),
                },
            },
        },
    }


def configured_tool_params_template(tool: ToolDefinition) -> JsonObject:
    if tool_requires_input(tool):
        return {"inputFiles": []}
    return {}


def preprocess_descriptor() -> JsonObject:
    return {
        "toolId": PREPROCESS_LOG_PACKAGE_ID,
        "displayName": "Log package preprocessor",
        "description": (
            "Expand node log packages, normalize rotated logs, and materialize analyzer inputs."
        ),
        "source": "built_in",
        "tags": ["built-in", "log", "preprocess", "manual-run"],
        "enabled": True,
        "backend": "builtin",
        "readOnly": True,
        "editable": False,
        "exportable": False,
        "runnable": True,
        "minFiles": 1,
        "maxFiles": 100,
        "acceptedSuffixes": [".tar.gz", ".tgz"],
        "paramsSchema": {"type": "object", "properties": {}, "additionalProperties": False},
        "paramsTemplate": {},
        "outputViews": ["summary", "nodes", "log_groups", "tool_inputs", "warnings"],
    }


def pprof_descriptor(settings: Settings) -> JsonObject:
    runnable = bool(settings.pprof_enabled and resolve_pprof_go_command(settings))
    return {
        "toolId": PPROF_ANALYZER_ID,
        "displayName": "Golang pprof Analyzer",
        "description": "Upload a Go pprof profile and inspect top functions plus raw/tree output.",
        "source": "configured",
        "tags": ["configured", "manual-run", "pprof"],
        "enabled": settings.pprof_enabled,
        "backend": "command",
        "readOnly": False,
        "editable": True,
        "exportable": runnable,
        "runnable": runnable,
        "manualOnly": True,
        "minFiles": 1,
        "maxFiles": 1,
        "maxInputFiles": 1,
        "acceptedSuffixes": [".pprof", ".prof", ".profile", ".pb.gz"],
        "paramsSchema": pprof_params_schema(),
        "paramsTemplate": {"sampleIndex": "samples", "nodeCount": 50, "generateSvg": False},
        "outputViews": ["summary", "top_table", "tree_text", "raw_text", "svg"],
    }


def pprof_params_schema() -> JsonObject:
    fields: JsonObject = {
        "sampleIndex": {"type": "string", "default": "samples"},
        "nodeCount": {"type": "integer", "default": 50, "minimum": 1, "maximum": 200},
        "generateSvg": {"type": "boolean", "default": False},
    }
    return {
        **fields,
        "type": "object",
        "properties": dict(fields),
        "additionalProperties": False,
    }


def metadata_catalog_descriptors() -> list[JsonObject]:
    base = {
        "source": "built_in",
        "tags": ["built-in", "metadata", "read-only", "manual-run"],
        "enabled": True,
        "backend": "builtin",
        "readOnly": True,
        "editable": False,
        "exportable": False,
        "runnable": True,
        "minFiles": 0,
        "maxFiles": 0,
        "acceptedSuffixes": [],
        "outputViews": ["json"],
    }
    field_schema = {
        "type": "object",
        "properties": {
            "instanceId": {"type": "string"},
            "database": {"type": "string"},
            "measurement": {"type": "string"},
            "retentionPolicy": {"type": "string"},
            "field": metadata_field_filter_schema(),
        },
        "required": ["instanceId", "database", "measurement"],
        "additionalProperties": False,
    }
    tag_schema = {
        "type": "object",
        "properties": {
            "instanceId": {"type": "string"},
            "database": {"type": "string"},
            "measurement": {"type": "string"},
            "retentionPolicy": {"type": "string"},
        },
        "required": ["instanceId", "database", "measurement"],
        "additionalProperties": False,
    }
    return [
        {
            **base,
            "toolId": METADATA_LIST_INSTANCES_ID,
            "displayName": "Metadata instances",
            "description": "List imported metadata instance summaries.",
            "paramsSchema": {"type": "object", "properties": {}, "additionalProperties": False},
            "paramsTemplate": {},
        },
        {
            **base,
            "toolId": METADATA_GET_SNAPSHOT_ID,
            "displayName": "Metadata snapshot",
            "description": "Read one imported metadata snapshot by instance id.",
            "paramsSchema": {
                "type": "object",
                "properties": {"instanceId": {"type": "string"}},
                "required": ["instanceId"],
                "additionalProperties": False,
            },
            "paramsTemplate": {"instanceId": ""},
        },
        {
            **base,
            "toolId": METADATA_GET_FIELD_TYPES_ID,
            "displayName": "Metadata field types",
            "description": (
                "Look up field type metadata for one imported instance, "
                "database and measurement."
            ),
            "paramsSchema": field_schema,
            "paramsTemplate": {
                "instanceId": "",
                "database": "",
                "measurement": "",
                "retentionPolicy": "",
                "field": [],
            },
        },
        {
            **base,
            "toolId": METADATA_GET_TAG_FIELDS_ID,
            "displayName": "Metadata tag fields",
            "description": (
                "List Tag type fields for one imported instance, database and measurement."
            ),
            "paramsSchema": tag_schema,
            "paramsTemplate": {
                "instanceId": "",
                "database": "",
                "measurement": "",
                "retentionPolicy": "",
            },
        },
    ]


def huawei_package_sync_descriptor(settings: Settings) -> JsonObject:
    config = settings.huawei_package_sync
    runnable = bool(
        config.enabled
        and config.obs_endpoint
        and config.obs_bucket
        and config.obs_access_key
        and config.obs_secret_key
        and config.gaussdb_dsn
    )
    return {
        "toolId": HUAWEI_PACKAGE_SYNC_TOOL_ID,
        "displayName": "Huawei OBS + GaussDB Package Sync",
        "description": (
            "Upload one package to Huawei OBS, execute a GaussDB update SQL, "
            "then query OBS/GaussDB summary."
        ),
        "source": "built_in",
        "tags": ["built-in", "huawei-cloud", "obs", "gaussdb", "manual-run"],
        "enabled": config.enabled,
        "backend": "huawei_cloud_package_sync",
        "readOnly": False,
        "editable": False,
        "exportable": False,
        "runnable": runnable,
        "minFiles": 1,
        "maxFiles": 1,
        "acceptedSuffixes": ["*"],
        "paramsSchema": {
            "type": "object",
            "properties": {
                "objectKey": {"type": "string"},
                "updateSql": {"type": "string"},
                "querySql": {"type": "string"},
            },
            "required": ["updateSql", "querySql"],
            "additionalProperties": False,
        },
        "paramsTemplate": {"objectKey": "", "updateSql": "", "querySql": ""},
        "outputViews": ["summary", "obs", "gaussdb", "json"],
    }


def get_tool(settings: Settings, tool_id: str) -> ToolDefinition:
    for tool in settings.tools:
        if tool.id == tool_id:
            return tool
    raise ValueError(f"unknown tool {tool_id}")


def get_tool_descriptor(settings: Settings, tool_id: str) -> JsonObject:
    for descriptor in tool_descriptors(settings):
        if descriptor["toolId"] == tool_id:
            return descriptor
    raise ValueError(f"unknown tool {tool_id}")


def validate_manual_tool_run(
    settings: Settings,
    tool_id: str,
    upload_count: int,
    params: JsonObject | None,
    upload_filenames: list[str] | None = None,
) -> JsonObject:
    descriptor = get_tool_descriptor(settings, tool_id)
    if not descriptor.get("enabled"):
        raise ValueError(f"tool {tool_id} is disabled")
    if not descriptor.get("runnable"):
        raise ValueError(f"tool {tool_id} is not runnable")
    min_files = int(descriptor.get("minFiles", 0))
    max_files = int(descriptor.get("maxFiles", 0))
    input_file_count = configured_input_file_count(settings, tool_id, params or {})
    effective_file_count = input_file_count or upload_count
    if effective_file_count < min_files or effective_file_count > max_files:
        raise ValueError(f"tool {tool_id} expects {min_files}..{max_files} upload(s)")
    if upload_filenames is not None:
        validate_tool_upload_filenames(tool_id, descriptor, upload_filenames)
    return validate_tool_run_params(settings, tool_id, params or {})


def validate_tool_upload_filenames(
    tool_id: str,
    descriptor: JsonObject,
    filenames: list[str],
) -> None:
    if not filenames:
        return
    accepted = [
        str(item).strip()
        for item in descriptor.get("acceptedSuffixes", [])
        if str(item).strip()
    ]
    if not accepted or "*" in accepted:
        return
    invalid = [
        filename
        for filename in filenames
        if not any(upload_filename_matches(filename, pattern) for pattern in accepted)
    ]
    if invalid:
        raise ValueError(
            f"tool {tool_id} does not accept upload(s): {', '.join(invalid)} "
            f"(acceptedSuffixes: {', '.join(accepted)})"
        )


def upload_filename_matches(filename: str, pattern: str) -> bool:
    normalized = filename.replace("\\", "/").lower()
    basename = Path(normalized).name
    lowered_pattern = pattern.lower()
    if lowered_pattern == "*":
        return True
    if lowered_pattern.startswith(".") and basename.endswith(lowered_pattern):
        return True
    return fnmatchcase(normalized, lowered_pattern) or fnmatchcase(basename, lowered_pattern)


def validate_tool_run_params(
    settings: Settings,
    tool_id: str,
    params: JsonObject,
) -> JsonObject:
    if not isinstance(params, dict):
        raise ValueError("tool params must be an object")
    if tool_id in {PREPROCESS_LOG_PACKAGE_ID, METADATA_LIST_INSTANCES_ID}:
        if params:
            raise ValueError(f"tool {tool_id} does not accept params")
        return {}
    if tool_id == METADATA_GET_SNAPSHOT_ID:
        instance_id = require_string_param(tool_id, params, "instanceId")
        reject_unknown_params(tool_id, params, {"instanceId"})
        return {"instanceId": instance_id}
    if tool_id in {METADATA_GET_FIELD_TYPES_ID, METADATA_GET_TAG_FIELDS_ID}:
        allowed = {"instanceId", "database", "measurement", "retentionPolicy"}
        if tool_id == METADATA_GET_FIELD_TYPES_ID:
            allowed.add("field")
        reject_unknown_params(tool_id, params, allowed)
        result: JsonObject = {
            "instanceId": require_string_param(tool_id, params, "instanceId"),
            "database": require_string_param(tool_id, params, "database"),
            "measurement": require_string_param(tool_id, params, "measurement"),
        }
        if isinstance(params.get("retentionPolicy"), str) and params["retentionPolicy"].strip():
            result["retentionPolicy"] = params["retentionPolicy"].strip()
        if "field" in params and tool_id == METADATA_GET_FIELD_TYPES_ID:
            field = normalize_field_filter_value(params["field"])
            if field is not None:
                result["field"] = field
        return result
    if tool_id == PPROF_ANALYZER_ID:
        reject_unknown_params(tool_id, params, {"sampleIndex", "nodeCount", "generateSvg"})
        sample_index = normalize_pprof_sample_index(params.get("sampleIndex", "samples"))
        node_count = params.get("nodeCount", 50)
        if isinstance(node_count, bool) or not isinstance(node_count, int):
            raise ValueError("nodeCount must be an integer")
        generate_svg = params.get("generateSvg", False)
        if not isinstance(generate_svg, bool):
            raise ValueError("generateSvg must be a boolean")
        return {
            "sampleIndex": sample_index,
            "nodeCount": max(1, min(node_count, 200)),
            "generateSvg": generate_svg,
        }
    if tool_id == HUAWEI_PACKAGE_SYNC_TOOL_ID:
        reject_unknown_params(tool_id, params, {"objectKey", "updateSql", "querySql"})
        object_key = str(params.get("objectKey") or "").strip()
        if object_key:
            validate_obs_object_key(object_key)
        return {
            "objectKey": object_key,
            "updateSql": require_string_param(tool_id, params, "updateSql"),
            "querySql": require_string_param(tool_id, params, "querySql"),
        }
    if tool_id == "logagent.fetch":
        from .fetch import normalize_fetch_run_params

        return normalize_fetch_run_params(params)
    tool = get_tool(settings, tool_id)
    return validate_configured_tool_params(tool, params)


def normalize_pprof_sample_index(value: object) -> str:
    if not isinstance(value, str):
        raise ValueError("sampleIndex must be a string")
    sample_index = str(value).strip()
    if not sample_index or not re.fullmatch(r"[A-Za-z0-9_-]+", sample_index):
        raise ValueError("sampleIndex must contain only letters, digits, '_' or '-'")
    return sample_index


def execute_tool_run(settings: Settings, store: Store, run_id: str) -> JsonObject:
    run = store.get_run(run_id)
    if run.get("kind") != "tool_run":
        raise ValueError(f"run {run_id} is not a tool run")
    tool_id = run.get("toolId")
    if not isinstance(tool_id, str) or not tool_id:
        raise ValueError(f"tool run {run_id} is missing toolId")
    store.mark_tool_run_running(run_id)
    try:
        result = execute_tool_by_id(settings, store, run, tool_id, run.get("toolParams") or {})
        completion = tool_run_completion_result(settings, store, run, tool_id, result)
        artifact_id = completion["artifact"]["id"]
        store.complete_tool_run(run_id, artifact_id, completion["result"])
        return completion
    except Exception as error:
        store.fail_tool_run(run_id, str(error))
        raise


def tool_run_completion_result(
    settings: Settings,
    store: Store,
    run: JsonObject,
    tool_id: str,
    result: JsonObject,
) -> JsonObject:
    if tool_id not in {tool.id for tool in settings.tools}:
        return result
    results = result.get("results")
    artifacts = result.get("artifacts")
    evidence_items = result.get("evidenceItems")
    if not (
        isinstance(results, list)
        and isinstance(artifacts, list)
        and isinstance(evidence_items, list)
        and len(results) > 1
    ):
        return result
    aggregate = aggregate_configured_tool_run_result(run, tool_id, result)
    artifact = write_artifact_bytes(
        settings=settings,
        store=store,
        workspace_id=run["workspace_id"],
        filename=f"{aggregate['actionId']}_result.json",
        data=json.dumps(aggregate, ensure_ascii=True, indent=2).encode("utf-8"),
        content_type="application/json",
        schema_name="logagent.v2.tool_result.aggregate.v1",
        preview={
            "toolId": tool_id,
            "actionId": aggregate["actionId"],
            "status": aggregate["status"],
            "inputFileCount": len(aggregate["inputFiles"]),
        },
    )
    return {
        **result,
        "result": aggregate,
        "artifact": artifact,
        "aggregateArtifact": artifact,
    }


def aggregate_configured_tool_run_result(
    run: JsonObject,
    tool_id: str,
    result: JsonObject,
) -> JsonObject:
    result_items = result["results"]
    artifact_items = result["artifacts"]
    wrappers = []
    for item, artifact in zip(result_items, artifact_items, strict=False):
        action_id = item.get("actionId") if isinstance(item, dict) else None
        artifact_path = f"tool_results/{action_id}/result.json" if action_id else None
        input_file = item.get("inputFile") if isinstance(item, dict) else None
        wrappers.append(
            {
                "actionId": action_id,
                "inputFile": input_file,
                "artifactPath": artifact_path,
                "artifactId": artifact.get("id") if isinstance(artifact, dict) else None,
                "summary": item.get("summary") if isinstance(item, dict) else None,
                "result": item,
            }
        )
    action_id = f"act_tool_manual_{safe_action_segment(tool_id)}_{run['id']}"
    return {
        "schemaVersion": 1,
        "toolId": tool_id,
        "actionId": action_id,
        "status": aggregate_configured_status(result_items),
        "params": run.get("toolParams") or {},
        "inputFiles": [
            item["inputFile"]
            for item in wrappers
            if isinstance(item.get("inputFile"), str) and item["inputFile"]
        ],
        "artifactPaths": [
            item["artifactPath"]
            for item in wrappers
            if isinstance(item.get("artifactPath"), str) and item["artifactPath"]
        ],
        "results": wrappers,
        "createdAt": datetime.now(UTC).replace(microsecond=0).isoformat(),
    }


def aggregate_configured_status(results: Sequence[object]) -> str:
    failed = False
    for item in results:
        status = item.get("status") if isinstance(item, dict) else None
        if status == "TIMED_OUT":
            return "TIMED_OUT"
        if status == "FAILED":
            failed = True
    return "FAILED" if failed else "OK"


def execute_tool_by_id(
    settings: Settings,
    store: Store,
    run: JsonObject,
    tool_id: str,
    params: JsonObject,
) -> JsonObject:
    if tool_id in {tool.id for tool in settings.tools}:
        return run_configured_tool(
            settings,
            store,
            run["workspace_id"],
            run["id"],
            tool_id,
            params,
            upload_ids=run.get("toolUploadIds") or [],
        )
    if tool_id == PREPROCESS_LOG_PACKAGE_ID:
        return run_preprocess_tool(settings, store, run, params)
    if tool_id in METADATA_TOOL_IDS:
        return run_metadata_tool(settings, store, run, tool_id, params)
    if tool_id == PPROF_ANALYZER_ID:
        return run_pprof_tool(settings, store, run, params)
    if tool_id == "logagent.fetch":
        from .fetch import execute_fetch_endpoint, normalize_fetch_run_params

        run_params = normalize_fetch_run_params(params)
        return execute_fetch_endpoint(
            settings=settings,
            store=store,
            workspace_id=run["workspace_id"],
            run_id=run["id"],
            endpoint_id=run_params["endpointId"],
            run_params=run_params,
            action_id=f"act_fetch_{run['id']}",
        )
    if tool_id == HUAWEI_PACKAGE_SYNC_TOOL_ID:
        return run_huawei_package_sync_tool(settings, store, run, params)
    raise ValueError(f"unknown tool {tool_id}")


def run_configured_tool(
    settings: Settings,
    store: Store,
    workspace_id: str,
    run_id: str,
    tool_id: str,
    params: JsonObject | None = None,
    upload_ids: list[str] | None = None,
    reuse_existing: bool = False,
    action_id_override: str | None = None,
) -> JsonObject:
    tool = get_tool(settings, tool_id)
    if not tool.enabled:
        raise ValueError(f"tool {tool_id} is disabled")
    tool_params, explicit_input_files = split_configured_tool_params(tool, params or {})
    normalized_params = validate_tool_params(tool, tool_params)
    input_entries = select_tool_inputs(
        settings,
        store,
        workspace_id,
        run_id,
        tool,
        upload_ids or [],
        explicit_input_files=explicit_input_files,
    )
    if tool_requires_input(tool) and not input_entries:
        raise ValueError(f"tool {tool_id} requires an input_file but no tool input matched")
    if input_entries:
        single_action_id_override = action_id_override if len(input_entries) == 1 else None
        runs = [
            run_single_configured_tool(
                settings,
                store,
                workspace_id,
                run_id,
                tool,
                entry,
                normalized_params,
                reuse_existing=reuse_existing,
                action_id_override=single_action_id_override,
            )
            for entry in input_entries[: tool.max_input_files]
        ]
        primary = runs[0]
        if len(runs) == 1:
            return primary
        return {
            **primary,
            "results": [item["result"] for item in runs],
            "artifacts": [item["artifact"] for item in runs],
            "evidenceItems": [item["evidence"] for item in runs],
        }
    return run_single_configured_tool(
        settings,
        store,
        workspace_id,
        run_id,
        tool,
        None,
        normalized_params,
        reuse_existing=reuse_existing,
        action_id_override=action_id_override,
    )


def run_matching_configured_tools(
    settings: Settings,
    store: Store,
    workspace_id: str,
    run_id: str,
) -> list[JsonObject]:
    """Run configured input-based tools that match the current task evidence."""

    results: list[JsonObject] = []
    for tool in settings.tools:
        if not tool.enabled or not tool_requires_input(tool):
            continue
        auto_params = automatic_tool_params(tool)
        if auto_params is None:
            continue
        input_entries = select_tool_inputs(settings, store, workspace_id, run_id, tool, [])
        for entry in input_entries[: tool.max_input_files]:
            results.append(
                run_single_configured_tool(
                    settings,
                    store,
                    workspace_id,
                    run_id,
                    tool,
                    entry,
                    auto_params,
                    reuse_existing=True,
                )
            )
    return results


def automatic_tool_params(tool: ToolDefinition) -> JsonObject | None:
    if any(PARAM_PLACEHOLDER_RE.search(arg) for arg in tool.args):
        return None
    try:
        return validate_tool_params(tool, {})
    except ValueError:
        return None


def run_single_configured_tool(
    settings: Settings,
    store: Store,
    workspace_id: str,
    run_id: str,
    tool: ToolDefinition,
    input_entry: JsonObject | None,
    params: JsonObject,
    reuse_existing: bool = False,
    action_id_override: str | None = None,
) -> JsonObject:
    command = Path(tool.command)
    if not command.is_absolute():
        raise ValueError(f"tool {tool.id} command must be an absolute path")
    input_file = resolve_tool_input_path(settings, store, input_entry) if input_entry else None
    action_id = action_id_override or tool_action_id(tool, input_entry, params)
    if reuse_existing:
        existing = existing_configured_tool_result(
            settings, store, run_id, tool.id, action_id
        )
        if existing is not None:
            return existing
    tool_workspace = prepare_tool_workspace(
        settings, store, workspace_id, run_id, action_id
    )
    argv = [
        str(command),
        *format_tool_args(tool.args, input_file, action_id, params, tool_workspace),
    ]
    started = time.monotonic()
    spawn_error: str | None = None
    try:
        completed = subprocess.run(
            argv,
            check=False,
            capture_output=True,
            cwd=tool_workspace,
            timeout=tool.timeout_seconds,
        )
        timed_out = False
    except subprocess.TimeoutExpired as error:
        completed = error
        timed_out = True
    except OSError as error:
        completed = None
        timed_out = False
        spawn_error = str(error)

    duration_ms = int((time.monotonic() - started) * 1000)
    raw_stdout = captured_output_bytes(completed.stdout if completed is not None else None)
    if spawn_error is not None:
        raw_stderr = spawn_error.encode("utf-8")
    else:
        raw_stderr = captured_output_bytes(completed.stderr if completed is not None else None)
    if timed_out and not raw_stderr:
        raw_stderr = b"tool timed out"
    stdout_truncated = len(raw_stdout) > tool.max_output_bytes
    stderr_truncated = len(raw_stderr) > tool.max_output_bytes
    stdout = raw_stdout[: tool.max_output_bytes]
    stderr = raw_stderr[: tool.max_output_bytes]
    exit_code = None if timed_out or completed is None else int(completed.returncode)
    parsed_stdout = parse_json(stdout)
    status = configured_tool_status(timed_out, spawn_error, exit_code)
    error_message = configured_tool_error(status, spawn_error)
    stdout_path = f"tool_results/{action_id}/stdout.txt"
    stderr_path = f"tool_results/{action_id}/stderr.txt"
    stdout_artifact = write_artifact_bytes(
        settings=settings,
        store=store,
        workspace_id=workspace_id,
        filename=f"{action_id}_stdout.txt",
        data=stdout,
        content_type="text/plain",
        schema_name="logagent.v2.tool_stdout.v1",
        preview={
            "toolId": tool.id,
            "actionId": action_id,
            "path": stdout_path,
            "sizeBytes": len(stdout),
            "truncated": stdout_truncated,
        },
    )
    stderr_artifact = write_artifact_bytes(
        settings=settings,
        store=store,
        workspace_id=workspace_id,
        filename=f"{action_id}_stderr.txt",
        data=stderr,
        content_type="text/plain",
        schema_name="logagent.v2.tool_stderr.v1",
        preview={
            "toolId": tool.id,
            "actionId": action_id,
            "path": stderr_path,
            "sizeBytes": len(stderr),
            "truncated": stderr_truncated,
        },
    )
    result = {
        "schemaVersion": 2,
        "tool": tool.id,
        "toolId": tool.id,
        "displayName": tool.display_name,
        "actionId": action_id,
        "status": status,
        "inputFile": input_entry.get("path") if input_entry else None,
        "inputKind": input_entry.get("inputKind") if input_entry else None,
        "params": params,
        "command": argv,
        "argv": argv,
        "timedOut": timed_out,
        "exitCode": exit_code,
        "durationMs": duration_ms,
        "stdoutPath": stdout_path,
        "stderrPath": stderr_path,
        "stdoutArtifactId": stdout_artifact["id"],
        "stderrArtifactId": stderr_artifact["id"],
        "artifactIds": {
            "stdout": stdout_artifact["id"],
            "stderr": stderr_artifact["id"],
        },
        "stdoutPreview": stdout.decode("utf-8", errors="replace"),
        "stderrPreview": stderr.decode("utf-8", errors="replace"),
        "parsedStdout": parsed_stdout,
        "summary": configured_tool_summary(
            tool.id,
            parsed_stdout,
            status,
            tool.timeout_seconds,
            spawn_error,
        ),
        "findings": normalize_tool_finding_paths(
            findings_from_stdout(parsed_stdout),
            input_entry,
            input_file,
        ),
        "error": error_message,
    }
    artifact = write_artifact_bytes(
        settings=settings,
        store=store,
        workspace_id=workspace_id,
        filename=f"{action_id}_result.json",
        data=json.dumps(result, ensure_ascii=True, indent=2).encode("utf-8"),
        content_type="application/json",
        schema_name="logagent.v2.tool_result.v1",
        preview={
            "toolId": tool.id,
            "exitCode": result["exitCode"],
            "timedOut": timed_out,
            "findingCount": len(result["findings"]),
        },
    )
    evidence = store.create_evidence(
        workspace_id=workspace_id,
        run_id=run_id,
        kind="tool_result",
        final_allowed=True,
        summary=f"Tool {tool.id} completed with exitCode={result['exitCode']}.",
        artifact_id=artifact["id"],
        payload={
            "artifactId": artifact["id"],
            "toolId": tool.id,
            "actionId": action_id,
            "inputFile": result["inputFile"],
            "params": params,
            "exitCode": result["exitCode"],
            "timedOut": timed_out,
            "stdoutPath": stdout_path,
            "stderrPath": stderr_path,
            "stdoutArtifactId": stdout_artifact["id"],
            "stderrArtifactId": stderr_artifact["id"],
            "findingCount": len(result["findings"]),
            "evidenceRefPrefix": f"tool_results/{action_id}/result.json#findings/",
        },
    )
    return {"result": result, "artifact": artifact, "evidence": evidence}


def existing_configured_tool_result(
    settings: Settings,
    store: Store,
    run_id: str,
    tool_id: str,
    action_id: str,
) -> JsonObject | None:
    for evidence in reversed(store.list_evidence(run_id)):
        if evidence.get("kind") != "tool_result":
            continue
        payload = evidence.get("payload")
        if not isinstance(payload, dict):
            continue
        if payload.get("toolId") != tool_id or payload.get("actionId") != action_id:
            continue
        artifact_id = evidence.get("artifact_id")
        if not isinstance(artifact_id, str) or not artifact_id:
            continue
        artifact = store.get_artifact(artifact_id)
        result_path = resolve_artifact_path(settings, artifact["relative_path"])
        try:
            result = json.loads(result_path.read_text(encoding="utf-8"))
        except Exception:
            continue
        if isinstance(result, dict):
            return {"result": result, "artifact": artifact, "evidence": evidence}
    return None


def normalize_tool_finding_paths(
    findings: list[JsonObject],
    input_entry: JsonObject | None,
    input_file: Path | None,
) -> list[JsonObject]:
    if input_entry is None or input_file is None:
        return findings
    logical_input = input_entry.get("path")
    if not isinstance(logical_input, str) or not logical_input:
        return findings
    normalized = []
    for finding in findings:
        item = dict(finding)
        file_value = item.get("file")
        if isinstance(file_value, str) and file_value:
            item["file"] = normalize_tool_finding_path(file_value, input_file, logical_input)
        normalized.append(item)
    return normalized


def normalize_tool_finding_path(file_value: str, input_file: Path, logical_input: str) -> str:
    file_path = Path(file_value)
    if not file_path.is_absolute():
        return file_value
    try:
        resolved_file = file_path.resolve(strict=False)
        resolved_input = input_file.resolve(strict=False)
    except OSError:
        return file_value
    if resolved_file == resolved_input:
        return logical_input
    if resolved_input.is_dir() and resolved_input in resolved_file.parents:
        relative = resolved_file.relative_to(resolved_input).as_posix()
        return f"{logical_input.rstrip('/')}/{relative}"
    return file_value


def configured_tool_results_outline(results: list[JsonObject]) -> list[JsonObject]:
    return [configured_tool_result_outline(item) for item in results]


def configured_tool_result_outline(item: JsonObject) -> JsonObject:
    result = item.get("result") if isinstance(item.get("result"), dict) else {}
    evidence = item.get("evidence") if isinstance(item.get("evidence"), dict) else {}
    payload = evidence.get("payload") if isinstance(evidence.get("payload"), dict) else {}
    prefix = payload.get("evidenceRefPrefix")
    findings = result.get("findings")
    final_refs: list[str] = []
    finding_preview: list[JsonObject] = []
    if isinstance(prefix, str) and prefix.endswith("#findings/") and isinstance(findings, list):
        for index, finding in enumerate(findings[:20]):
            ref = f"{prefix}{index}"
            final_refs.append(ref)
            if isinstance(finding, dict):
                finding_preview.append({"ref": ref, **finding})
            else:
                finding_preview.append({"ref": ref, "value": finding})
    return {
        "toolId": result.get("toolId") or payload.get("toolId"),
        "actionId": result.get("actionId") or payload.get("actionId"),
        "status": result.get("status"),
        "inputFile": result.get("inputFile"),
        "inputKind": result.get("inputKind"),
        "summary": result.get("summary") or evidence.get("summary"),
        "finalEvidenceRefs": final_refs,
        "findings": finding_preview,
    }


def select_tool_inputs(
    settings: Settings,
    store: Store,
    workspace_id: str,
    run_id: str,
    tool: ToolDefinition,
    upload_ids: list[str] | None = None,
    explicit_input_files: list[str] | None = None,
) -> list[JsonObject]:
    if not tool_requires_input(tool):
        return []
    if explicit_input_files:
        return select_explicit_tool_inputs(
            settings,
            store,
            workspace_id,
            run_id,
            tool,
            explicit_input_files,
            upload_ids or [],
        )
    index = latest_tool_input_index(settings, store, run_id)
    inputs = index.get("inputs") if isinstance(index, dict) else None
    selected = []
    if isinstance(inputs, list):
        for entry in inputs:
            if not isinstance(entry, dict):
                continue
            tool_ids = entry.get("toolIds")
            if not isinstance(tool_ids, list) or tool.id not in tool_ids:
                continue
            if not isinstance(entry.get("artifactId"), str):
                continue
            selected.append(entry)
            if len(selected) >= tool.max_input_files:
                break
    if selected:
        return selected
    return select_fallback_tool_inputs(settings, store, workspace_id, run_id, tool, upload_ids or [])


def select_explicit_tool_inputs(
    settings: Settings,
    store: Store,
    workspace_id: str,
    run_id: str,
    tool: ToolDefinition,
    input_files: list[str],
    upload_ids: list[str] | None = None,
) -> list[JsonObject]:
    if len(input_files) > tool.max_input_files:
        raise ValueError(
            f"tool {tool.id} accepts at most {tool.max_input_files} input file(s)"
    )
    index = latest_tool_input_index(settings, store, run_id)
    indexed_inputs = index.get("inputs") if isinstance(index, dict) else []
    if not isinstance(indexed_inputs, list):
        indexed_inputs = []
    tool_inputs_by_path = {
        entry["path"]: entry
        for entry in indexed_inputs
        if isinstance(entry, dict) and isinstance(entry.get("path"), str)
    }
    uploads = store.list_uploads_by_ids(workspace_id, upload_ids or [])
    text_files = collect_text_files(settings, uploads)
    upload_inputs_by_path = {
        item["path"]: item for item in select_upload_artifact_inputs(uploads, tool)
    }
    selected = []
    for input_file in input_files:
        if input_file in tool_inputs_by_path:
            entry = tool_inputs_by_path[input_file]
            tool_ids = entry.get("toolIds")
            if isinstance(tool_ids, list) and tool.id not in tool_ids:
                raise ValueError(f"tool input {input_file} is not declared for {tool.id}")
            selected.append(entry)
            continue
        text_file = resolve_text_file_selector(text_files, input_file)
        if text_file is not None:
            selected.append(
                materialize_fallback_tool_input(
                    settings, store, workspace_id, tool, text_file
                )
            )
            continue
        if input_file in upload_inputs_by_path:
            selected.append(upload_inputs_by_path[input_file])
            continue
        raise ValueError(f"tool input file is not available in this workspace: {input_file}")
    return selected


def select_fallback_tool_inputs(
    settings: Settings,
    store: Store,
    workspace_id: str,
    run_id: str,
    tool: ToolDefinition,
    upload_ids: list[str] | None = None,
) -> list[JsonObject]:
    uploads = store.list_uploads_by_ids(workspace_id, upload_ids or [])
    if tool.id in STORAGE_TOOL_IDS:
        return select_upload_artifact_inputs(uploads, tool)
    text_files = collect_text_files(settings, uploads)
    files_by_path = {text_file.path: text_file for text_file in text_files}
    selected_paths: list[str] = []

    for text_file in text_files:
        if len(selected_paths) >= tool.max_input_files:
            break
        if any(
            matches_file_pattern(pattern, text_file.path, text_file.source_filename)
            for pattern in tool.match_file_patterns
        ):
            push_selected_path(selected_paths, text_file.path)

    if len(selected_paths) < tool.max_input_files and tool.match_keywords:
        grep_results = latest_initial_grep_results(settings, store, run_id)
        for match in grep_results.get("matches", []):
            if len(selected_paths) >= tool.max_input_files:
                break
            if not isinstance(match, dict):
                continue
            path = match.get("path")
            text = match.get("text")
            if not isinstance(path, str) or path not in files_by_path:
                continue
            if not isinstance(text, str) or not matches_any_keyword(text, tool.match_keywords):
                continue
            push_selected_path(selected_paths, path)

    return [
        materialize_fallback_tool_input(settings, store, workspace_id, tool, files_by_path[path])
        for path in selected_paths[: tool.max_input_files]
    ]


def select_upload_artifact_inputs(
    uploads: list[JsonObject],
    tool: ToolDefinition,
) -> list[JsonObject]:
    selected = []
    for upload in uploads:
        filename = upload["filename"]
        if tool.match_file_patterns and not any(
            matches_file_pattern(pattern, filename, filename)
            for pattern in tool.match_file_patterns
        ):
            continue
        selected.append(
            {
                "path": filename,
                "inputKind": "upload_artifact",
                "scope": "upload",
                "toolIds": [tool.id],
                "sourceFiles": [filename],
                "sourceUploadId": upload["id"],
                "sourceFilename": filename,
                "artifactId": upload["artifact_id"],
                "artifactRelativePath": upload["artifact_relative_path"],
            }
        )
        if len(selected) >= tool.max_input_files:
            break
    return selected


def latest_tool_input_index(settings: Settings, store: Store, run_id: str) -> JsonObject:
    candidates = [
        item for item in store.list_evidence(run_id) if item["kind"] == "tool_input_index"
    ]
    if not candidates:
        return {}
    artifact_id = candidates[-1].get("artifact_id")
    if not artifact_id:
        return {}
    artifact = store.get_artifact(artifact_id)
    path = resolve_artifact_path(settings, artifact["relative_path"])
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except Exception:
        return {}
    return value if isinstance(value, dict) else {}


def latest_initial_grep_results(settings: Settings, store: Store, run_id: str) -> JsonObject:
    candidates = [
        item
        for item in store.list_evidence(run_id)
        if item["kind"] == "log_search" and item["payload"].get("path") == "grep_results.json"
    ]
    if not candidates:
        return {}
    artifact_id = candidates[-1].get("artifact_id")
    if not artifact_id:
        return {}
    artifact = store.get_artifact(artifact_id)
    path = resolve_artifact_path(settings, artifact["relative_path"])
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except Exception:
        return {}
    return value if isinstance(value, dict) else {}


def materialize_fallback_tool_input(
    settings: Settings,
    store: Store,
    workspace_id: str,
    tool: ToolDefinition,
    text_file: TextFile,
) -> JsonObject:
    virtual_path = fallback_virtual_path(text_file.path)
    data = text_file.text.encode("utf-8")
    digest = sha256(virtual_path.encode("utf-8")).hexdigest()[:12]
    artifact = write_artifact_bytes(
        settings=settings,
        store=store,
        workspace_id=workspace_id,
        filename=f"{safe_action_segment(tool.id)}_{digest}.txt",
        data=data,
        content_type="text/plain",
        schema_name="logagent.v2.tool_input.text_file.v1",
        preview={
            "path": virtual_path,
            "toolId": tool.id,
            "sourcePath": text_file.path,
            "sizeBytes": len(data),
        },
    )
    return {
        "path": virtual_path,
        "inputKind": "text_file",
        "scope": "manifest_fallback",
        "toolIds": [tool.id],
        "sourceFiles": [text_file.path],
        "sourceUploadId": text_file.source_upload_id,
        "sourceFilename": text_file.source_filename,
        "lineCount": len(text_file.text.splitlines()),
        "artifactId": artifact["id"],
        "artifactRelativePath": artifact["relative_path"],
    }


def fallback_virtual_path(manifest_path: str) -> str:
    if manifest_path.startswith("extracted/") or manifest_path.startswith("tool_inputs/"):
        return manifest_path
    return f"extracted/{manifest_path}"


def matches_file_pattern(pattern: str, path: str, source_filename: str) -> bool:
    lowered_pattern = pattern.lower()
    lowered_path = path.lower()
    lowered_name = Path(path).name.lower()
    lowered_source = source_filename.lower()
    return (
        lowered_pattern == "*"
        or fnmatchcase(lowered_path, lowered_pattern)
        or fnmatchcase(lowered_name, lowered_pattern)
        or fnmatchcase(lowered_source, lowered_pattern)
    )


def matches_any_keyword(text: str, keywords: tuple[str, ...]) -> bool:
    lowered = text.lower()
    return any(keyword.lower() in lowered for keyword in keywords)


def push_selected_path(selected_paths: list[str], path: str) -> None:
    if path not in selected_paths:
        selected_paths.append(path)


def resolve_tool_input_path(
    settings: Settings, store: Store, input_entry: JsonObject
) -> Path:
    artifact = store.get_artifact(str(input_entry["artifactId"]))
    path = resolve_artifact_path(settings, artifact["relative_path"])
    if not path.is_file() and not path.is_dir():
        raise ValueError(f"tool input artifact is missing for {input_entry.get('path')}")
    return path


def format_tool_args(
    args: tuple[str, ...],
    input_file: Path | None,
    action_id: str,
    params: JsonObject,
    tool_workspace: Path,
) -> list[str]:
    result = []
    for arg in args:
        if "{input_file}" in arg and input_file is None:
            raise ValueError("tool arg requires {input_file} but no input file is selected")
        formatted = arg.replace("{input_file}", str(input_file) if input_file else "")
        formatted = formatted.replace("{workspace}", str(tool_workspace))
        formatted = formatted.replace(
            "{manifest_path}", str(tool_workspace / "manifest.json")
        )
        formatted = formatted.replace(
            "{grep_results_path}", str(tool_workspace / "grep_results.json")
        )
        formatted = formatted.replace("{action_id}", action_id)
        formatted = PARAM_PLACEHOLDER_RE.sub(
            lambda match: tool_param_to_arg(params, match.group(1)),
            formatted,
        )
        if UNSUPPORTED_PLACEHOLDER_RE.search(formatted):
            raise ValueError(f"unsupported tool argument placeholder in {arg}")
        result.append(formatted)
    return result


def prepare_tool_workspace(
    settings: Settings,
    store: Store,
    workspace_id: str,
    run_id: str,
    action_id: str,
) -> Path:
    tool_workspace = (
        settings.tmp_dir
        / "tool_workspaces"
        / safe_action_segment(workspace_id)
        / safe_action_segment(run_id)
        / safe_action_segment(action_id)
    )
    tool_workspace.mkdir(parents=True, exist_ok=True)
    materialize_run_artifact_for_tool_workspace(
        settings, store, run_id, tool_workspace, "manifest", "manifest.json"
    )
    materialize_run_artifact_for_tool_workspace(
        settings, store, run_id, tool_workspace, "log_search", "grep_results.json"
    )
    materialize_run_artifact_for_tool_workspace(
        settings, store, run_id, tool_workspace, "tool_input_index", "tool_inputs/index.json"
    )
    return tool_workspace


def materialize_run_artifact_for_tool_workspace(
    settings: Settings,
    store: Store,
    run_id: str,
    tool_workspace: Path,
    evidence_kind: str,
    payload_path: str,
) -> None:
    artifact_id = latest_evidence_artifact_id(store, run_id, evidence_kind, payload_path)
    if artifact_id is None:
        return
    artifact = store.get_artifact(artifact_id)
    source = resolve_artifact_path(settings, artifact["relative_path"])
    if not source.is_file():
        return
    target = tool_workspace / Path(*payload_path.split("/"))
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, target)


def latest_evidence_artifact_id(
    store: Store,
    run_id: str,
    evidence_kind: str,
    payload_path: str,
) -> str | None:
    for evidence in reversed(store.list_evidence(run_id)):
        if evidence.get("kind") != evidence_kind:
            continue
        if evidence.get("payload", {}).get("path") != payload_path:
            continue
        artifact_id = evidence.get("artifact_id")
        if isinstance(artifact_id, str) and artifact_id:
            return artifact_id
    return None


def configured_input_file_count(
    settings: Settings,
    tool_id: str,
    params: JsonObject,
) -> int:
    if tool_id not in {tool.id for tool in settings.tools}:
        return 0
    return len(normalize_explicit_input_files(params.get("inputFiles")))


def validate_configured_tool_params(
    tool: ToolDefinition,
    params: JsonObject,
) -> JsonObject:
    if not isinstance(params, dict):
        raise ValueError("tool params must be an object")
    input_files = normalize_explicit_input_files(params.get("inputFiles"))
    if input_files and not tool_requires_input(tool):
        raise ValueError(f"tool {tool.id} does not accept inputFiles")
    tool_params = {key: value for key, value in params.items() if key != "inputFiles"}
    normalized = validate_tool_params(tool, tool_params)
    if input_files:
        normalized["inputFiles"] = input_files
    return normalized


def split_configured_tool_params(
    tool: ToolDefinition,
    params: JsonObject | None,
) -> tuple[JsonObject, list[str]]:
    params = params or {}
    if not isinstance(params, dict):
        raise ValueError("tool params must be an object")
    input_files = normalize_explicit_input_files(params.get("inputFiles"))
    if input_files and not tool_requires_input(tool):
        raise ValueError(f"tool {tool.id} does not accept inputFiles")
    tool_params = {key: value for key, value in params.items() if key != "inputFiles"}
    return tool_params, input_files


def normalize_explicit_input_files(value: object) -> list[str]:
    if value is None:
        return []
    raw_items: list[object]
    if isinstance(value, str):
        raw_items = [value]
    elif isinstance(value, list):
        raw_items = value
    else:
        raise ValueError("inputFiles must be a string or string array")
    result = []
    for item in raw_items:
        if not isinstance(item, str):
            raise ValueError("inputFiles must contain only strings")
        normalized = normalize_workspace_input_path(item)
        if normalized not in result:
            result.append(normalized)
    return result


def normalize_workspace_input_path(value: str) -> str:
    normalized = value.strip().replace("\\", "/")
    if not normalized:
        raise ValueError("inputFiles must not contain empty paths")
    path = Path(normalized)
    if path.is_absolute():
        raise ValueError("inputFiles must be workspace-relative paths")
    parts = [part for part in normalized.split("/") if part]
    if not parts or any(part in {".", ".."} for part in parts):
        raise ValueError("inputFiles must not contain . or .. path segments")
    if len(normalized) > 1000:
        raise ValueError("inputFiles path is too long")
    return "/".join(parts)


def validate_tool_params(tool: ToolDefinition, params: JsonObject) -> JsonObject:
    if not isinstance(params, dict):
        raise ValueError("tool params must be an object")
    schema = tool.params_schema or {
        "type": "object",
        "properties": {},
        "additionalProperties": False,
    }
    properties = schema.get("properties", {})
    if not isinstance(properties, dict):
        properties = {}
    required = schema.get("required", [])
    if not isinstance(required, list):
        required = []
    result: JsonObject = {}
    for field in required:
        if field not in params:
            raise ValueError(f"tool {tool.id} param {field} is required")
    if schema.get("additionalProperties", False) is False:
        unknown = sorted(key for key in params if key not in properties)
        if unknown:
            raise ValueError(f"tool {tool.id} does not accept params: {', '.join(unknown)}")
    for key, value in params.items():
        field_schema = properties.get(key, {})
        if isinstance(field_schema, dict):
            validate_tool_param_value(tool.id, key, value, field_schema)
        result[key] = value
    return result


def validate_tool_param_value(
    tool_id: str, key: str, value: object, schema: JsonObject
) -> None:
    one_of = schema.get("oneOf")
    if isinstance(one_of, list):
        match_count = 0
        for subschema in one_of:
            if not isinstance(subschema, dict):
                continue
            try:
                validate_tool_param_value(tool_id, key, value, subschema)
            except ValueError:
                continue
            match_count += 1
        if match_count != 1:
            raise ValueError(f"tool {tool_id} param {key} must match exactly one schema")
        return
    any_of = schema.get("anyOf")
    if isinstance(any_of, list):
        for subschema in any_of:
            if not isinstance(subschema, dict):
                continue
            try:
                validate_tool_param_value(tool_id, key, value, subschema)
            except ValueError:
                continue
            return
        raise ValueError(f"tool {tool_id} param {key} must match one allowed schema")
    expected = schema.get("type")
    if expected is not None and not json_schema_type_matches(value, expected):
        raise ValueError(
            f"tool {tool_id} param {key} must be {json_schema_type_label(expected)}"
        )
    enum = schema.get("enum")
    if isinstance(enum, list) and value not in enum:
        raise ValueError(f"tool {tool_id} param {key} must be one of {enum}")
    if isinstance(value, str):
        min_length = schema.get("minLength")
        max_length = schema.get("maxLength")
        if isinstance(min_length, int) and len(value) < min_length:
            raise ValueError(f"tool {tool_id} param {key} is shorter than {min_length}")
        if isinstance(max_length, int) and len(value) > max_length:
            raise ValueError(f"tool {tool_id} param {key} is longer than {max_length}")
    if isinstance(value, (int, float)) and not isinstance(value, bool):
        minimum = schema.get("minimum")
        maximum = schema.get("maximum")
        if isinstance(minimum, (int, float)) and value < minimum:
            raise ValueError(f"tool {tool_id} param {key} must be >= {minimum}")
        if isinstance(maximum, (int, float)) and value > maximum:
            raise ValueError(f"tool {tool_id} param {key} must be <= {maximum}")
    if isinstance(value, list):
        min_items = schema.get("minItems")
        max_items = schema.get("maxItems")
        if isinstance(min_items, int) and len(value) < min_items:
            raise ValueError(f"tool {tool_id} param {key} needs at least {min_items} item(s)")
        if isinstance(max_items, int) and len(value) > max_items:
            raise ValueError(f"tool {tool_id} param {key} allows at most {max_items} item(s)")
        item_schema = schema.get("items")
        if isinstance(item_schema, dict):
            for index, item in enumerate(value):
                validate_tool_param_value(tool_id, f"{key}[{index}]", item, item_schema)
    if isinstance(value, dict):
        properties = schema.get("properties")
        if isinstance(properties, dict):
            required = schema.get("required")
            if isinstance(required, list):
                for field in required:
                    if field not in value:
                        raise ValueError(f"tool {tool_id} param {key}.{field} is required")
            if schema.get("additionalProperties", True) is False:
                unknown = sorted(field for field in value if field not in properties)
                if unknown:
                    raise ValueError(
                        f"tool {tool_id} param {key} does not accept fields: "
                        f"{', '.join(unknown)}"
                    )
            for field, item in value.items():
                field_schema = properties.get(field)
                if isinstance(field_schema, dict):
                    validate_tool_param_value(tool_id, f"{key}.{field}", item, field_schema)


def json_schema_type_matches(value: object, expected: object) -> bool:
    if isinstance(expected, list):
        return any(json_schema_type_matches(value, item) for item in expected)
    if expected == "string":
        return isinstance(value, str)
    if expected == "integer":
        return isinstance(value, int) and not isinstance(value, bool)
    if expected == "number":
        return isinstance(value, (int, float)) and not isinstance(value, bool)
    if expected == "boolean":
        return isinstance(value, bool)
    if expected == "array":
        return isinstance(value, list)
    if expected == "object":
        return isinstance(value, dict)
    if expected == "null":
        return value is None
    return True


def json_schema_type_label(expected: object) -> str:
    if isinstance(expected, list):
        return " or ".join(str(item) for item in expected)
    if expected == "array":
        return "an array"
    if expected == "object":
        return "an object"
    if expected == "integer":
        return "an integer"
    return f"a {expected}"


def tool_param_to_arg(params: JsonObject, key: str) -> str:
    if key not in params:
        raise ValueError(f"tool arg references missing param {key}")
    value = params[key]
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (int, float, str)):
        return str(value)
    return json.dumps(value, ensure_ascii=True)


def tool_requires_input(tool: ToolDefinition) -> bool:
    return any("{input_file}" in arg for arg in tool.args)


def tool_action_id(tool: ToolDefinition, input_entry: JsonObject | None, params: JsonObject) -> str:
    prefix = f"act_tool_{safe_action_segment(tool.id)}"
    params_digest = ""
    if params:
        params_digest = ":" + json.dumps(params, ensure_ascii=True, sort_keys=True)
    if input_entry is None:
        if not params:
            return prefix
        digest = sha256(params_digest.encode("utf-8")).hexdigest()[:12]
        return f"{prefix}_{digest}"
    input_path = str(input_entry.get("path", ""))
    if not params:
        return f"{prefix}_{stable_input_hash(input_path)}"
    digest = sha256((input_path + params_digest).encode("utf-8")).hexdigest()[:12]
    return f"{prefix}_{digest}"


def safe_action_segment(value: str) -> str:
    result = "".join(
        char if (char.isascii() and char.isalnum()) or char in "_-" else "_"
        for char in value
    )
    return result[:80] or "tool"


def stable_input_hash(value: str) -> str:
    hash_value = 0xCBF29CE484222325
    for byte in value.encode("utf-8"):
        hash_value ^= byte
        hash_value = (hash_value * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"{hash_value:016x}"


def run_preprocess_tool(
    settings: Settings,
    store: Store,
    run: JsonObject,
    params: JsonObject,
) -> JsonObject:
    if params:
        raise ValueError("preprocess tool does not accept params")
    started = time.monotonic()
    workspace = store.get_workspace(run["workspace_id"])
    uploads = store.list_uploads_by_ids(run["workspace_id"], run.get("toolUploadIds") or [])
    text_files = collect_text_files(settings, uploads)
    tool_input_bundle = materialize_tool_inputs(
        settings,
        store,
        run["workspace_id"],
        uploads,
        text_files,
    )
    manifest = build_manifest(
        settings,
        run["workspace_id"],
        run["id"],
        uploads,
        text_files,
        source_url=workspace.get("sourceUrl"),
        tool_inputs_path=tool_input_bundle.get("path"),
        tool_input_count=len(tool_input_bundle.get("inputs", [])),
    )
    grep_results = grep_text_files(
        text_files,
        search_keywords("", settings.grep_keywords),
        settings.max_grep_matches,
        ref_base="grep_results.json#matches/",
    )
    manifest_artifact = write_json_artifact(
        settings,
        store,
        run["workspace_id"],
        f"{run['id']}_manifest.json",
        manifest,
        schema_name="logagent.v2.manifest.v1",
    )
    grep_artifact = write_json_artifact(
        settings,
        store,
        run["workspace_id"],
        f"{run['id']}_grep_results.json",
        grep_results,
        schema_name="logagent.v2.grep_results.v1",
    )
    log_groups: dict[str, int] = {}
    for text_file in text_files:
        if text_file.log_group:
            log_groups[text_file.log_group] = log_groups.get(text_file.log_group, 0) + 1
    manifest_uploads = manifest.get("uploads", [])
    nodes = preprocess_node_summaries(manifest_uploads if isinstance(manifest_uploads, list) else [])
    node_packages = [
        upload for upload in manifest_uploads if isinstance(upload, dict) and upload.get("nodeId")
    ]
    warnings = [
        warning
        for node in nodes
        for warning in node.get("warnings", [])
        if isinstance(warning, str)
    ]
    action_id = f"act_tool_preprocess_{run['id']}"
    tool_inputs = tool_input_bundle.get("inputs", [])
    result = {
        "schemaVersion": 1,
        "toolId": PREPROCESS_LOG_PACKAGE_ID,
        "actionId": action_id,
        "status": "OK",
        "summary": (
            f"preprocessed {len(uploads)} upload(s), {len(nodes)} node(s), "
            f"{len(text_files)} extracted file(s), "
            f"{len(tool_inputs)} materialized tool input(s)"
        ),
        "manifestPath": "manifest.json",
        "grepResultsPath": "grep_results.json",
        "toolInputsPath": tool_input_bundle.get("path"),
        "uploadCount": len(uploads),
        "fileCount": len(text_files),
        "nodes": nodes,
        "nodePackages": node_packages,
        "logGroups": log_groups,
        "warnings": list(dict.fromkeys(warnings)),
        "manifestArtifactId": manifest_artifact["id"],
        "manifestArtifactPath": manifest_artifact["relative_path"],
        "grepArtifactId": grep_artifact["id"],
        "grepArtifactPath": grep_artifact["relative_path"],
        "toolInputs": tool_inputs,
        "toolInputIndex": tool_inputs,
        "durationMs": int((time.monotonic() - started) * 1000),
        "createdAt": datetime.now(UTC).isoformat(),
    }
    artifact = write_tool_result_artifact(settings, store, run["workspace_id"], action_id, result)
    evidence = store.create_evidence(
        workspace_id=run["workspace_id"],
        run_id=run["id"],
        kind="tool_result",
        final_allowed=False,
        summary=result["summary"],
        artifact_id=artifact["id"],
        payload={
            "artifactId": artifact["id"],
            "toolId": PREPROCESS_LOG_PACKAGE_ID,
            "actionId": action_id,
            "manifestArtifactId": manifest_artifact["id"],
            "grepArtifactId": grep_artifact["id"],
            "toolInputCount": len(tool_input_bundle.get("inputs", [])),
        },
    )
    return {"result": result, "artifact": artifact, "evidence": evidence}


def preprocess_node_summaries(upload_summaries: list[JsonObject]) -> list[JsonObject]:
    nodes: dict[str, JsonObject] = {}
    for upload in upload_summaries:
        if not isinstance(upload, dict):
            continue
        node_id = str(upload.get("nodeId") or "")
        if not node_id:
            continue
        entry = nodes.setdefault(
            node_id,
            {
                "nodeId": node_id,
                "packages": 0,
                "instanceIds": [],
                "timestamps": [],
                "logGroups": {},
                "ignoredFileCount": 0,
                "warnings": [],
            },
        )
        entry["packages"] += 1
        append_unique_string(entry["instanceIds"], upload.get("instanceId"))
        append_unique_string(entry["timestamps"], upload.get("packageTimestamp"))
        entry["ignoredFileCount"] += non_negative_int(upload.get("ignoredFileCount"))
        for warning in upload.get("warnings") or []:
            append_unique_string(entry["warnings"], warning)
        for group_summary in upload.get("logGroups") or []:
            if not isinstance(group_summary, dict):
                continue
            group_name = group_summary.get("name")
            if not isinstance(group_name, str) or not group_name:
                continue
            groups = entry["logGroups"]
            group = groups.setdefault(group_name, {"fileCount": 0, "compressedFileCount": 0})
            group["fileCount"] += non_negative_int(group_summary.get("fileCount"))
            group["compressedFileCount"] += non_negative_int(
                group_summary.get("compressedFileCount")
            )
    return [nodes[node_id] for node_id in sorted(nodes)]


def non_negative_int(value: object) -> int:
    if isinstance(value, bool):
        return 0
    if isinstance(value, int) and value > 0:
        return value
    return 0


def append_unique_string(values: list[str], value: object) -> None:
    if not isinstance(value, str) or not value or value in values:
        return
    values.append(value)


def run_metadata_tool(
    settings: Settings,
    store: Store,
    run: JsonObject,
    tool_id: str,
    params: JsonObject,
) -> JsonObject:
    started = time.monotonic()
    if tool_id == METADATA_LIST_INSTANCES_ID:
        value = {"instances": store.list_metadata_instances()}
        v1_result = value
    elif tool_id == METADATA_GET_SNAPSHOT_ID:
        snapshot = store.get_metadata_snapshot(params["instanceId"])
        value = {**snapshot, "snapshot": snapshot}
        v1_result = {"snapshot": snapshot}
    elif tool_id in {METADATA_GET_FIELD_TYPES_ID, METADATA_GET_TAG_FIELDS_ID}:
        value = query_field_types(
            store=store,
            instance_id=params["instanceId"],
            database=params["database"],
            measurement=params["measurement"],
            retention_policy=params.get("retentionPolicy"),
            field=params.get("field"),
            tags_only=tool_id == METADATA_GET_TAG_FIELDS_ID,
        )
        v1_result = {"result": value}
    else:
        raise ValueError(f"unsupported metadata tool {tool_id}")
    action_id = f"act_tool_metadata_{safe_action_segment(tool_id)}_{run['id']}"
    result = {
        "schemaVersion": 1,
        "toolId": tool_id,
        "actionId": action_id,
        "status": "OK",
        "summary": f"Metadata tool {tool_id} completed.",
        "params": params,
        "result": v1_result,
        "value": value,
        "durationMs": int((time.monotonic() - started) * 1000),
        "createdAt": datetime.now(UTC).isoformat(),
    }
    artifact = write_tool_result_artifact(settings, store, run["workspace_id"], action_id, result)
    evidence = store.create_evidence(
        workspace_id=run["workspace_id"],
        run_id=run["id"],
        kind="metadata_slice",
        final_allowed=False,
        summary=result["summary"],
        artifact_id=artifact["id"],
        payload={"artifactId": artifact["id"], "toolId": tool_id, "actionId": action_id},
    )
    return {"result": result, "artifact": artifact, "evidence": evidence}


def run_pprof_tool(
    settings: Settings,
    store: Store,
    run: JsonObject,
    params: JsonObject,
) -> JsonObject:
    params = validate_tool_run_params(settings, PPROF_ANALYZER_ID, params)
    go_command = resolve_pprof_go_command(settings)
    if not go_command:
        raise ValueError("pprof analyzer is not configured")
    uploads = store.list_uploads_by_ids(run["workspace_id"], run.get("toolUploadIds") or [])
    if len(uploads) != 1:
        raise ValueError("pprof analyzer requires exactly one upload")
    upload = uploads[0]
    profile_path = resolve_artifact_path(settings, upload["artifact_relative_path"])
    if not profile_path.is_file():
        raise ValueError("uploaded pprof profile is missing")
    action_id = f"act_tool_pprof_analyzer_{run['id']}"
    started = time.monotonic()
    node_count = int(params["nodeCount"])
    sample_index = str(params["sampleIndex"])
    commands = {
        "top": [
            go_command,
            "tool",
            "pprof",
            "-top",
            f"-sample_index={sample_index}",
            f"-nodecount={node_count}",
            "-symbolize=none",
            str(profile_path),
        ],
        "tree": [
            go_command,
            "tool",
            "pprof",
            "-tree",
            f"-sample_index={sample_index}",
            f"-nodecount={node_count}",
            "-symbolize=none",
            str(profile_path),
        ],
        "raw": [
            go_command,
            "tool",
            "pprof",
            "-raw",
            f"-sample_index={sample_index}",
            "-symbolize=none",
            str(profile_path),
        ],
    }
    if params.get("generateSvg"):
        commands["svg"] = [
            go_command,
            "tool",
            "pprof",
            "-svg",
            f"-sample_index={sample_index}",
            f"-nodecount={node_count}",
            "-symbolize=none",
            str(profile_path),
        ]
    outputs: dict[str, JsonObject] = {}
    warnings = []
    for name, argv in commands.items():
        outputs[name] = run_pprof_command(settings, store, run["workspace_id"], action_id, name, argv)
        if outputs[name].get("timedOut") or outputs[name].get("exitCode") not in {0, None}:
            warnings.append(f"{name} command did not complete successfully")
    top_text = outputs.get("top", {}).get("text", "")
    top_entries = parse_pprof_top(top_text)
    profile_type, total = parse_pprof_profile_summary(top_text)
    status = (
        "OK"
        if all(outputs.get(name, {}).get("exitCode") == 0 for name in ("top", "tree", "raw"))
        else "FAILED"
    )
    error = None if status == "OK" else "one or more pprof commands failed"
    stderr_artifact = write_pprof_stderr_artifact(
        settings, store, run["workspace_id"], action_id, outputs
    )
    artifact_paths = {
        "topTextPath": outputs.get("top", {}).get("path"),
        "treeTextPath": outputs.get("tree", {}).get("path"),
        "rawTextPath": outputs.get("raw", {}).get("path"),
        "svgPath": outputs.get("svg", {}).get("path")
        if outputs.get("svg", {}).get("exitCode") == 0
        else None,
        "stderrPath": f"tool_results/{action_id}/stderr.txt",
    }
    artifact_ids = {key: value.get("artifactId") for key, value in outputs.items()}
    artifact_ids["stderr"] = stderr_artifact["id"]
    result = {
        "schemaVersion": 1,
        "toolId": PPROF_ANALYZER_ID,
        "actionId": action_id,
        "status": status,
        "summary": f"pprof top produced {len(top_entries)} row(s).",
        "profile": {"uploadId": upload["id"], "filename": upload["filename"]},
        "profileType": profile_type,
        "sampleIndex": sample_index,
        "total": total,
        "top": top_entries,
        "artifacts": artifact_paths,
        "artifactIds": artifact_ids,
        "artifactPaths": artifact_paths,
        "warnings": warnings,
        "error": error,
        "durationMs": int((time.monotonic() - started) * 1000),
        "createdAt": datetime.now(UTC).isoformat(),
    }
    artifact = write_tool_result_artifact(settings, store, run["workspace_id"], action_id, result)
    evidence = store.create_evidence(
        workspace_id=run["workspace_id"],
        run_id=run["id"],
        kind="tool_result",
        final_allowed=True,
        summary=result["summary"],
        artifact_id=artifact["id"],
        payload={
            "artifactId": artifact["id"],
            "toolId": PPROF_ANALYZER_ID,
            "actionId": action_id,
            "findingCount": len(top_entries),
            "evidenceRefPrefix": f"tool_results/{action_id}/result.json#top/",
        },
    )
    return {"result": result, "artifact": artifact, "evidence": evidence}


def run_huawei_package_sync_tool(
    settings: Settings,
    store: Store,
    run: JsonObject,
    params: JsonObject,
) -> JsonObject:
    params = validate_tool_run_params(settings, HUAWEI_PACKAGE_SYNC_TOOL_ID, params)
    config = settings.huawei_package_sync
    if not config.enabled:
        raise ValueError("Huawei package sync is disabled")
    uploads = store.list_uploads_by_ids(run["workspace_id"], run.get("toolUploadIds") or [])
    if len(uploads) != 1:
        raise ValueError("Huawei package sync requires exactly one upload")
    upload = uploads[0]
    package_path = resolve_artifact_path(settings, upload["artifact_relative_path"])
    object_key = params.get("objectKey") or default_huawei_object_key(config.obs_object_prefix, upload["filename"])
    validate_obs_object_key(object_key)
    action_id = f"act_tool_huawei_package_sync_{run['id']}"
    started = time.monotonic()
    warnings: list[str] = []
    failed_step = None
    error = None
    obs_put = None
    obs_head = None
    update_result = None
    query_result = None
    try:
        obs_put = huawei_obs_request(settings, "PUT", object_key, package_path.read_bytes())
        update_result = execute_gaussdb_sql(config.gaussdb_dsn, params["updateSql"], fetch=False)
        obs_head = huawei_obs_request(settings, "HEAD", object_key, b"")
        query_result = execute_gaussdb_sql(config.gaussdb_dsn, params["querySql"], fetch=True)
    except Exception as exc:
        message = str(exc)
        if obs_put is None:
            failed_step = "obs_put"
        elif update_result is None:
            failed_step = "gaussdb_update"
        elif obs_head is None:
            failed_step = "obs_head"
        else:
            failed_step = "gaussdb_query"
        error = message[:2000]
    status = "FAILED" if error else "OK"
    if bool((query_result or {}).get("truncated")):
        warnings.append("GaussDB query rows truncated to first 200 row(s)")
    duration_ms = int((time.monotonic() - started) * 1000)
    logical_result_path = f"tool_results/{action_id}/result.json"
    gaussdb_meta = huawei_gaussdb_metadata(config.gaussdb_dsn)
    result = {
        "schemaVersion": 1,
        "toolId": HUAWEI_PACKAGE_SYNC_TOOL_ID,
        "tool": HUAWEI_PACKAGE_SYNC_TOOL_ID,
        "actionId": action_id,
        "status": status,
        "summary": (
            f"Uploaded {upload['filename']} to OBS and queried GaussDB records"
            if status == "OK"
            else f"Huawei package sync failed at {failed_step}"
        ),
        "warnings": warnings,
        "objectKey": object_key,
        "objectUrl": huawei_object_url(settings, object_key),
        "input": {
            "uploadId": upload["id"],
            "filename": upload["filename"],
            "size": upload.get("artifact_size_bytes"),
            "rawPath": upload.get("artifact_relative_path"),
        },
        "upload": {"uploadId": upload["id"], "filename": upload["filename"]},
        "obs": {
            "endpoint": config.obs_endpoint,
            "bucket": config.obs_bucket,
            "objectKey": object_key,
            "url": huawei_object_url(settings, object_key),
            "put": obs_put,
            "head": obs_head,
        },
        "obsPut": obs_put,
        "obsHead": obs_head,
        "gaussdb": {
            **gaussdb_meta,
            "updateAffectedRows": (update_result or {}).get("affectedRows"),
            "queryRowCount": (query_result or {}).get("rowCount"),
            "queryRows": (query_result or {}).get("rows"),
            "queryRowsTruncated": bool((query_result or {}).get("truncated")),
        },
        "gaussdbUpdate": update_result,
        "gaussdbQuery": query_result,
        "sql": {
            "updateSqlProvided": True,
            "updateSqlLength": len(params["updateSql"]),
            "querySqlProvided": True,
            "querySqlLength": len(params["querySql"]),
        },
        "timings": {
            "obsPutMs": (obs_put or {}).get("durationMs"),
            "gaussdbUpdateMs": (update_result or {}).get("durationMs"),
            "obsHeadMs": (obs_head or {}).get("durationMs"),
            "gaussdbQueryMs": (query_result or {}).get("durationMs"),
            "totalMs": duration_ms,
        },
        "failedStep": failed_step,
        "error": error,
        "durationMs": duration_ms,
        "credentialMetadata": {
            "obsAccessKeyEnv": "LOGAGENT_V2_HUAWEI_OBS_ACCESS_KEY",
            "obsSecretKeyEnv": "LOGAGENT_V2_HUAWEI_OBS_SECRET_KEY",
            "obsSecurityTokenEnv": "LOGAGENT_V2_HUAWEI_OBS_SECURITY_TOKEN",
            "gaussdbPasswordEnv": None,
            "gaussdbDsnEnv": "LOGAGENT_V2_HUAWEI_GAUSSDB_DSN",
        },
        "credentialEnv": {
            "obsAccessKey": "LOGAGENT_V2_HUAWEI_OBS_ACCESS_KEY",
            "obsSecretKey": "LOGAGENT_V2_HUAWEI_OBS_SECRET_KEY",
            "gaussdbDsn": "LOGAGENT_V2_HUAWEI_GAUSSDB_DSN",
        },
        "evidenceRefs": [logical_result_path],
        "createdAt": datetime.now(UTC).isoformat(),
    }
    artifact = write_tool_result_artifact(settings, store, run["workspace_id"], action_id, result)
    evidence = store.create_evidence(
        workspace_id=run["workspace_id"],
        run_id=run["id"],
        kind="tool_result",
        final_allowed=False,
        summary=result["summary"],
        artifact_id=artifact["id"],
        payload={"artifactId": artifact["id"], "toolId": HUAWEI_PACKAGE_SYNC_TOOL_ID, "actionId": action_id},
    )
    return {"result": result, "artifact": artifact, "evidence": evidence}


def huawei_gaussdb_metadata(dsn: str | None) -> JsonObject:
    if not dsn:
        return {
            "host": None,
            "port": None,
            "database": None,
            "user": None,
            "sslmode": None,
        }
    parsed = urllib.parse.urlparse(dsn)
    query = urllib.parse.parse_qs(parsed.query)
    try:
        port = parsed.port
    except ValueError:
        port = None
    return {
        "host": parsed.hostname,
        "port": port,
        "database": parsed.path.lstrip("/") or None,
        "user": urllib.parse.unquote(parsed.username) if parsed.username else None,
        "sslmode": (query.get("sslmode") or [None])[0],
    }


def write_tool_result_artifact(
    settings: Settings,
    store: Store,
    workspace_id: str,
    action_id: str,
    result: JsonObject,
) -> JsonObject:
    return write_artifact_bytes(
        settings=settings,
        store=store,
        workspace_id=workspace_id,
        filename=f"{action_id}_result.json",
        data=json.dumps(result, ensure_ascii=True, indent=2).encode("utf-8"),
        content_type="application/json",
        schema_name="logagent.v2.tool_result.v1",
        preview={
            "toolId": result.get("toolId"),
            "actionId": action_id,
            "status": result.get("status"),
        },
    )


def run_pprof_command(
    settings: Settings,
    store: Store,
    workspace_id: str,
    action_id: str,
    name: str,
    argv: list[str],
) -> JsonObject:
    started = time.monotonic()
    logical_path = pprof_logical_output_path(action_id, name)
    try:
        completed = subprocess.run(
            argv,
            check=False,
            capture_output=True,
            timeout=60,
            env={**os.environ, "PPROF_TMPDIR": str(settings.tmp_dir)},
        )
        stdout = completed.stdout[: settings.remote_max_output_bytes]
        stderr = completed.stderr[: settings.remote_max_output_bytes]
        exit_code: int | None = completed.returncode
        timed_out = False
    except subprocess.TimeoutExpired as error:
        stdout = error.stdout or b""
        stderr = error.stderr or b""
        exit_code = None
        timed_out = True
    text = stdout.decode("utf-8", errors="replace")
    stderr_text = stderr.decode("utf-8", errors="replace")
    artifact = write_artifact_bytes(
        settings=settings,
        store=store,
        workspace_id=workspace_id,
        filename=f"{action_id}_{name}.txt" if name != "svg" else f"{action_id}_{name}.svg",
        data=stdout,
        content_type="image/svg+xml" if name == "svg" else "text/plain",
        schema_name=f"logagent.v2.pprof.{name}.v1",
        preview={
            "actionId": action_id,
            "kind": name,
            "path": logical_path,
            "exitCode": exit_code,
            "timedOut": timed_out,
        },
    )
    if stderr:
        write_artifact_bytes(
            settings=settings,
            store=store,
            workspace_id=workspace_id,
            filename=f"{action_id}_{name}_stderr.txt",
            data=stderr,
            content_type="text/plain",
            schema_name="logagent.v2.pprof.stderr.v1",
            preview={"actionId": action_id, "kind": name, "sizeBytes": len(stderr)},
        )
    return {
        "artifactId": artifact["id"],
        "path": logical_path,
        "exitCode": exit_code,
        "timedOut": timed_out,
        "durationMs": int((time.monotonic() - started) * 1000),
        "text": text,
        "stderrText": stderr_text,
    }


def write_pprof_stderr_artifact(
    settings: Settings,
    store: Store,
    workspace_id: str,
    action_id: str,
    outputs: dict[str, JsonObject],
) -> JsonObject:
    chunks = []
    for name in ("top", "tree", "raw", "svg"):
        stderr_text = outputs.get(name, {}).get("stderrText")
        if isinstance(stderr_text, str) and stderr_text:
            chunks.append(f"== {name} ==\n{stderr_text.rstrip()}\n")
    data = ("\n".join(chunks)).encode("utf-8")
    return write_artifact_bytes(
        settings=settings,
        store=store,
        workspace_id=workspace_id,
        filename=f"{action_id}_stderr.txt",
        data=data,
        content_type="text/plain",
        schema_name="logagent.v2.pprof.stderr.v1",
        preview={
            "actionId": action_id,
            "kind": "stderr",
            "path": f"tool_results/{action_id}/stderr.txt",
            "sizeBytes": len(data),
        },
    )


def pprof_logical_output_path(action_id: str, name: str) -> str:
    if name == "svg":
        return f"tool_results/{action_id}/graph.svg"
    return f"tool_results/{action_id}/{name}.txt"


def parse_pprof_profile_summary(text: str) -> tuple[str | None, str | None]:
    profile_type = None
    total = None
    for line in text.splitlines():
        if line.startswith("Type:"):
            profile_type = line.split(":", 1)[1].strip() or None
        match = re.search(r"\bof\s+(.+?)\s+total\b", line)
        if match:
            total = match.group(1).strip()
    return profile_type, total


def parse_pprof_top(text: str) -> list[JsonObject]:
    rows = []
    for line in text.splitlines():
        parts = line.split()
        if len(parts) < 6 or not re.match(r"^\d", parts[0]):
            continue
        rows.append(
            {
                "rank": len(rows) + 1,
                "flat": parts[0],
                "flatPercent": parts[1] if len(parts) > 1 else None,
                "sumPercent": parts[2] if len(parts) > 2 else None,
                "cum": parts[3] if len(parts) > 3 else None,
                "cumPercent": parts[4] if len(parts) > 4 else None,
                "function": " ".join(parts[5:]),
            }
        )
    return rows[:200]


def resolve_pprof_go_command(settings: Settings) -> str | None:
    command = settings.pprof_go_command
    if not command:
        return None
    path = Path(command)
    if path.is_absolute() and path.is_file():
        return str(path)
    resolved = shutil.which(command)
    return resolved


def require_string_param(tool_id: str, params: JsonObject, key: str) -> str:
    value = params.get(key)
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"tool {tool_id} param {key} is required")
    return value.strip()


def reject_unknown_params(tool_id: str, params: JsonObject, allowed: set[str]) -> None:
    unknown = sorted(key for key in params if key not in allowed)
    if unknown:
        raise ValueError(f"tool {tool_id} does not accept params: {', '.join(unknown)}")


def validate_obs_object_key(value: str) -> None:
    if not value or value.startswith("/") or "\\" in value or "?" in value or "#" in value:
        raise ValueError("invalid OBS object key")
    parts = value.split("/")
    if any(part in {"", ".", ".."} for part in parts):
        raise ValueError("invalid OBS object key")
    if any(ord(char) < 32 for char in value):
        raise ValueError("invalid OBS object key")


def default_huawei_object_key(prefix: str, filename: str) -> str:
    clean_prefix = prefix.strip("/")
    clean_name = re.sub(r"[^A-Za-z0-9._/-]+", "_", Path(filename).name)
    return f"{clean_prefix}/{clean_name}" if clean_prefix else clean_name


def huawei_object_url(settings: Settings, object_key: str) -> str | None:
    config = settings.huawei_package_sync
    if not config.obs_endpoint or not config.obs_bucket:
        return None
    parsed = urllib.parse.urlsplit(config.obs_endpoint.rstrip("/"))
    host = parsed.hostname
    if not host:
        return None
    bucket_prefix = f"{config.obs_bucket}."
    netloc = parsed.netloc
    if host != config.obs_bucket and not host.startswith(bucket_prefix):
        port = f":{parsed.port}" if parsed.port is not None else ""
        netloc = f"{config.obs_bucket}.{host}{port}"
    encoded_key = "/".join(urllib.parse.quote(segment) for segment in object_key.split("/"))
    return urllib.parse.urlunsplit((parsed.scheme, netloc, f"/{encoded_key}", "", ""))


def huawei_obs_request(
    settings: Settings,
    method: str,
    object_key: str,
    body: bytes,
) -> JsonObject:
    config = settings.huawei_package_sync
    if not all([config.obs_endpoint, config.obs_bucket, config.obs_access_key, config.obs_secret_key]):
        raise ValueError("Huawei OBS credentials are incomplete")
    url = huawei_object_url(settings, object_key)
    assert url is not None
    date = email.utils.formatdate(usegmt=True)
    content_type = "application/octet-stream" if method == "PUT" else ""
    resource = f"/{config.obs_bucket}/{object_key}"
    string_to_sign = f"{method}\n\n{content_type}\n{date}\n{resource}"
    signature = base64.b64encode(
        hmac.new(
            config.obs_secret_key.encode("utf-8"),
            string_to_sign.encode("utf-8"),
            "sha1",
        ).digest()
    ).decode("ascii")
    headers = {
        "Date": date,
        "Authorization": f"OBS {config.obs_access_key}:{signature}",
    }
    if content_type:
        headers["Content-Type"] = content_type
    if config.obs_security_token:
        headers["x-obs-security-token"] = config.obs_security_token
    request = urllib.request.Request(url, data=body if method == "PUT" else None, method=method, headers=headers)
    started = time.monotonic()
    try:
        with urllib.request.urlopen(request, timeout=config.timeout_seconds) as response:
            return {
                "statusCode": response.status,
                "etag": response.headers.get("ETag"),
                "contentLength": parse_optional_int(response.headers.get("Content-Length")),
                "durationMs": int((time.monotonic() - started) * 1000),
            }
    except urllib.error.HTTPError as error:
        raise ValueError(f"OBS {method} returned HTTP {error.code}") from error


def parse_optional_int(value: str | None) -> int | None:
    if value is None:
        return None
    try:
        return int(value)
    except ValueError:
        return None


def execute_gaussdb_sql(dsn: str | None, sql: str, fetch: bool) -> JsonObject:
    if not dsn:
        raise ValueError("LOGAGENT_V2_HUAWEI_GAUSSDB_DSN is not configured")
    try:
        import psycopg
    except ImportError as error:
        raise ValueError("psycopg is required for Huawei GaussDB execution") from error
    started = time.monotonic()
    with psycopg.connect(dsn, connect_timeout=10) as conn:
        with conn.cursor() as cursor:
            cursor.execute(sql)
            if fetch:
                columns = [item.name for item in cursor.description or []]
                result = collect_gaussdb_query_rows(columns, iter_cursor_rows(cursor))
                result["durationMs"] = int((time.monotonic() - started) * 1000)
                return result
            affected = cursor.rowcount
        conn.commit()
    return {"affectedRows": affected, "durationMs": int((time.monotonic() - started) * 1000)}


def iter_cursor_rows(cursor: object) -> Iterable[Sequence[object]]:
    fetchmany = getattr(cursor, "fetchmany")
    while True:
        rows = fetchmany(MAX_HUAWEI_QUERY_ROWS)
        if not rows:
            break
        yield from rows


def collect_gaussdb_query_rows(
    columns: Sequence[str],
    rows: Iterable[Sequence[object]],
) -> JsonObject:
    names = unique_sql_column_names(columns)
    preview_rows: list[JsonObject] = []
    row_count = 0
    for row in rows:
        row_count += 1
        if len(preview_rows) >= MAX_HUAWEI_QUERY_ROWS:
            continue
        preview_rows.append(
            {
                names[index]: stringify_sql_value(value)
                for index, value in enumerate(row)
                if index < len(names)
            }
        )
    return {
        "rowCount": row_count,
        "truncated": row_count > MAX_HUAWEI_QUERY_ROWS,
        "rows": preview_rows,
    }


def unique_sql_column_names(columns: Sequence[str]) -> list[str]:
    counts: dict[str, int] = {}
    names: list[str] = []
    for column in columns:
        base = str(column or "column").strip() or "column"
        count = counts.get(base, 0) + 1
        counts[base] = count
        names.append(base if count == 1 else f"{base}_{count}")
    return names


def stringify_sql_value(value: object) -> object:
    if value is None or isinstance(value, (str, int, float, bool)):
        return value
    return str(value)


def parse_json(data: bytes) -> object | None:
    if not data.strip():
        return None
    try:
        value = json.loads(data.decode("utf-8"))
    except Exception:
        return None
    return value


def captured_output_bytes(value: bytes | str | None) -> bytes:
    if value is None:
        return b""
    if isinstance(value, bytes):
        return value
    return value.encode("utf-8")


def configured_tool_status(
    timed_out: bool,
    spawn_error: str | None,
    exit_code: int | None,
) -> str:
    if timed_out:
        return "TIMED_OUT"
    if spawn_error is not None:
        return "FAILED"
    return "OK" if exit_code == 0 else "FAILED"


def configured_tool_error(status: str, spawn_error: str | None) -> str | None:
    if spawn_error is not None:
        return spawn_error
    if status == "TIMED_OUT":
        return "tool timed out"
    return None


def configured_tool_summary(
    tool_id: str,
    parsed: object | None,
    status: str,
    timeout_seconds: int,
    spawn_error: str | None,
) -> str:
    parsed_summary = parsed_stdout_summary(parsed)
    if parsed_summary:
        return parsed_summary
    if status == "OK":
        return f"tool {tool_id} completed successfully"
    if status == "TIMED_OUT":
        return f"tool {tool_id} timed out after {timeout_seconds} seconds"
    if spawn_error is not None:
        return f"tool {tool_id} could not be started"
    return f"tool {tool_id} exited with non-zero status"


def parsed_stdout_summary(parsed: object | None) -> str | None:
    if isinstance(parsed, dict):
        if is_influxql_report(parsed):
            return influxql_report_summary(parsed)
        if is_influxql_compare_report(parsed):
            return influxql_compare_summary(parsed)
        if is_flux_query_report(parsed):
            return flux_query_summary(parsed)
        return string_field(parsed, ("summary", "message", "title"))
    if isinstance(parsed, str) and parsed.strip():
        return parsed.strip()[:500]
    return None


def summary_from_stdout(parsed: object | None, stdout: bytes, timed_out: bool) -> str:
    if timed_out:
        return "Tool timed out."
    parsed_summary = parsed_stdout_summary(parsed)
    if parsed_summary:
        return parsed_summary
    preview = stdout.decode("utf-8", errors="replace").strip()
    return preview[:500] if preview else "Tool produced no stdout."


def findings_from_stdout(parsed: object | None) -> list[JsonObject]:
    if not parsed:
        return []
    if isinstance(parsed, dict):
        if is_influxql_report(parsed):
            return influxql_report_findings(parsed)
        if is_influxql_compare_report(parsed):
            return influxql_compare_findings(parsed)
        if is_flux_query_report(parsed):
            return flux_query_findings(parsed)
        for key in ("findings", "issues", "diagnostics"):
            findings = parsed.get(key)
            if isinstance(findings, list):
                return parse_findings_value(findings)
        return []
    if isinstance(parsed, list):
        return parse_findings_value(parsed)
    return []


def is_influxql_report(value: JsonObject) -> bool:
    return (
        "total_records" in value
        and "total_statements" in value
        and "fingerprints" in value
    )


def is_influxql_compare_report(value: JsonObject) -> bool:
    return "batch_a" in value and "batch_b" in value and "statement_delta" in value


def is_flux_query_report(value: JsonObject) -> bool:
    return value.get("tool") == "flux_query_analyzer" or (
        isinstance(value.get("metrics"), dict)
        and isinstance(value.get("topQueries"), list)
        and ("parseErrors" in value or "summary" in value)
    )


def flux_query_summary(value: JsonObject) -> str:
    explicit = string_field(value, ("summary",))
    if explicit:
        return explicit
    metrics = value.get("metrics")
    if not isinstance(metrics, dict):
        return "flux query stats"
    total_rows = number_to_string(metrics.get("totalRows")) or "0"
    parse_success = number_to_string(metrics.get("parseSuccessCount")) or "0"
    unique_templates = number_to_string(metrics.get("uniqueTemplateCount")) or "0"
    new_templates = number_to_string(metrics.get("newTemplateCount")) or "0"
    parse_errors = number_to_string(metrics.get("parseErrorCount")) or "0"
    queries_with_duration = number_to_string(metrics.get("queriesWithDuration")) or "0"
    p95 = "unknown"
    latency = metrics.get("globalLatencyMs")
    if isinstance(latency, dict):
        p95 = number_to_string(latency.get("p95")) or p95
    return (
        "flux query stats: "
        f"rows={total_rows}, parseSuccess={parse_success}/{total_rows}, "
        f"uniqueTemplates={unique_templates}, newTemplates={new_templates}, "
        f"parseErrors={parse_errors}, durationCoverage={queries_with_duration}/{total_rows}, "
        f"p95={p95}ms"
    )


def flux_query_findings(value: JsonObject) -> list[JsonObject]:
    explicit = value.get("findings")
    if isinstance(explicit, list):
        parsed = parse_findings_value(explicit)
        if parsed:
            return parsed
    findings: list[JsonObject] = []
    findings.extend(flux_parse_error_findings(value))
    findings.extend(flux_top_query_findings(value))
    findings.extend(flux_metrics_findings(value))
    return findings


def flux_parse_error_findings(value: JsonObject) -> list[JsonObject]:
    errors = value.get("parseErrors")
    if not isinstance(errors, list):
        return []
    findings = []
    for item in errors[:5]:
        if not isinstance(item, dict):
            continue
        message = string_field(item, ("error", "message"))
        if not message:
            continue
        count = number_to_string(item.get("count")) or "0"
        findings.append(
            {
                "severity": "high",
                "message": f"Flux parse errors occurred {count} time(s); error: {message}",
            }
        )
    return findings


def flux_top_query_findings(value: JsonObject) -> list[JsonObject]:
    top_queries = value.get("topQueries")
    if not isinstance(top_queries, list):
        return []
    findings = []
    for index, item in enumerate(top_queries[:5], start=1):
        if not isinstance(item, dict):
            continue
        count = number_to_string(item.get("count")) or "0"
        ratio = flux_ratio_text(item.get("ratio"))
        latency = item.get("latencyMs")
        p95 = "unknown"
        if isinstance(latency, dict):
            p95 = number_to_string(latency.get("p95")) or p95
        fingerprint = (
            string_field(item, ("fingerprintShort", "fingerprint"))
            or "unknown"
        )
        normalized = string_field(item, ("normalizedQuery", "query")) or "unknown"
        findings.append(
            {
                "severity": flux_latency_severity(p95),
                "message": (
                    f"Top Flux template #{index}: count={count}"
                    f"{ratio}, p95={p95}ms, fingerprint={fingerprint}, "
                    f"query={normalized}"
                ),
            }
        )
    return findings


def flux_metrics_findings(value: JsonObject) -> list[JsonObject]:
    metrics = value.get("metrics")
    if not isinstance(metrics, dict):
        return []
    findings = []
    parse_error_count = int_field(metrics, "parseErrorCount") or 0
    if parse_error_count > 0 and not flux_parse_error_findings(value):
        findings.append(
            {
                "severity": "high",
                "message": f"Flux parse errors occurred {parse_error_count} time(s).",
            }
        )
    new_template_count = int_field(metrics, "newTemplateCount") or 0
    if new_template_count > 0:
        findings.append(
            {
                "severity": "medium",
                "message": (
                    f"Flux analyzer found {new_template_count} new template(s) "
                    "relative to its baseline."
                ),
            }
        )
    return findings


def flux_ratio_text(value: object) -> str:
    if isinstance(value, bool) or value is None:
        return ""
    try:
        ratio = float(value)
    except (TypeError, ValueError):
        return ""
    return f" ({ratio * 100:.1f}%)"


def flux_latency_severity(p95: str) -> str:
    try:
        value = float(p95)
    except ValueError:
        return "low"
    if value >= 1000:
        return "high"
    if value >= 200:
        return "medium"
    return "low"


def influxql_report_summary(value: JsonObject) -> str:
    total_records = int_field(value, "total_records") or 0
    records_in_window = int_field(value, "records_in_window") or 0
    total_statements = int_field(value, "total_statements") or 0
    parse_errors = int_field(value, "parse_error_count") or 0
    summary = (
        "influxql report: "
        f"records={total_records}, recordsInWindow={records_in_window}, "
        f"statements={total_statements}, parseErrors={parse_errors}"
    )
    rule_summary = influxql_rule_summary(value)
    if rule_summary:
        summary += f", specialRules={rule_summary}"
    return summary


def influxql_compare_summary(value: JsonObject) -> str:
    statement_delta = number_to_string(value.get("statement_delta")) or "0"
    qps_delta = number_to_string(value.get("qps_delta")) or "0"
    return (
        "influxql compare report: "
        f"statementDelta={statement_delta}, qpsDelta={qps_delta}, "
        f"batchA={compare_batch_summary(value.get('batch_a'))}, "
        f"batchB={compare_batch_summary(value.get('batch_b'))}"
    )


def compare_batch_summary(value: object) -> str:
    if not isinstance(value, dict):
        return "unknown"
    statements = number_to_string(value.get("total_statements")) or "0"
    parse_errors = number_to_string(value.get("parse_error_count")) or "0"
    qps = number_to_string(value.get("qps")) or "0"
    duration = number_to_string(value.get("effective_duration_seconds")) or "0"
    return (
        f"statements={statements}, parseErrors={parse_errors}, "
        f"qps={qps}, durationSeconds={duration}"
    )


def influxql_report_findings(value: JsonObject) -> list[JsonObject]:
    findings: list[JsonObject] = []
    findings.extend(influxql_special_rule_findings(value))
    findings.extend(influxql_parse_error_findings(value))
    findings.extend(influxql_realtime_findings(value))
    findings.extend(influxql_fingerprint_findings(value))
    return findings


def influxql_compare_findings(value: JsonObject) -> list[JsonObject]:
    findings: list[JsonObject] = []
    findings.extend(compare_fingerprint_findings(value, "new_fingerprints", "new fingerprint"))
    findings.extend(
        compare_fingerprint_findings(value, "removed_fingerprints", "removed fingerprint")
    )
    findings.extend(
        compare_fingerprint_findings(value, "changed_fingerprints", "changed fingerprint")
    )
    findings.extend(compare_rule_delta_findings(value))
    return findings


def influxql_rule_summary(value: JsonObject) -> str:
    rules = value.get("special_rules")
    if not isinstance(rules, list):
        return ""
    parts = []
    for item in rules[:8]:
        if not isinstance(item, dict):
            continue
        name = string_field(item, ("rule",))
        if not name:
            continue
        count = number_to_string(item.get("count")) or "0"
        parts.append(f"{name}:{count}")
    return ", ".join(parts)


def influxql_special_rule_findings(value: JsonObject) -> list[JsonObject]:
    rules = value.get("special_rules")
    if not isinstance(rules, list):
        return []
    findings = []
    for item in rules[:12]:
        if not isinstance(item, dict):
            continue
        name = string_field(item, ("rule",))
        if not name:
            continue
        count = number_to_string(item.get("count")) or "0"
        fingerprints = item.get("fingerprints")
        fingerprint_count = len(fingerprints) if isinstance(fingerprints, list) else 0
        findings.append(
            {
                "severity": influxql_rule_severity(name),
                "message": (
                    f"rule {name} matched {count} statement(s) across "
                    f"{fingerprint_count} fingerprint(s): {influxql_rule_description(name)}"
                ),
            }
        )
    return findings


def influxql_parse_error_findings(value: JsonObject) -> list[JsonObject]:
    errors = value.get("parse_errors")
    if not isinstance(errors, list):
        return []
    findings = []
    for item in errors[:5]:
        if not isinstance(item, dict):
            continue
        message = string_field(item, ("error",))
        if not message:
            continue
        count = number_to_string(item.get("count")) or "0"
        findings.append(
            {
                "severity": "high",
                "message": f"parse error occurred {count} time(s): {message}",
            }
        )
    return findings


def influxql_realtime_findings(value: JsonObject) -> list[JsonObject]:
    realtime = value.get("realtime_query")
    if not isinstance(realtime, dict):
        return []
    total = int_field(realtime, "total") or 0
    if total <= 0:
        return []
    non_realtime = int_field(realtime, "non_realtime") or 0
    unknown = int_field(realtime, "unknown") or 0
    realtime_count = int_field(realtime, "realtime") or 0
    findings = []
    if non_realtime > 0:
        findings.append(
            {
                "severity": "medium",
                "message": (
                    "realtime query classification found "
                    f"{non_realtime}/{total} non-realtime select-like statement(s)"
                ),
            }
        )
    if unknown > 0:
        reason = first_realtime_sample_reason(realtime, "sample_unknown")
        reason_text = f"; sample reason: {reason}" if reason else ""
        findings.append(
            {
                "severity": "low",
                "message": (
                    "realtime query classification is unknown for "
                    f"{unknown}/{total} select-like statement(s); "
                    f"realtime={realtime_count}{reason_text}"
                ),
            }
        )
    return findings


def influxql_fingerprint_findings(value: JsonObject) -> list[JsonObject]:
    fingerprints = value.get("fingerprints")
    if not isinstance(fingerprints, list):
        return []
    findings = []
    for item in fingerprints[:5]:
        if not isinstance(item, dict):
            continue
        count = int_field(item, "count") or 0
        rules = string_list(item.get("rules"))
        if count <= 1 and not rules:
            continue
        statement_type = string_field(item, ("statement_type",)) or ""
        normalized = string_field(item, ("normalized_query",)) or ""
        rule_text = ", ".join(rules) if rules else "none"
        findings.append(
            {
                "severity": "low",
                "message": (
                    f"fingerprint {statement_type} occurred {count} time(s), "
                    f"rules=[{rule_text}], normalized={normalized}"
                ),
            }
        )
    return findings


def compare_fingerprint_findings(value: JsonObject, key: str, label: str) -> list[JsonObject]:
    items = value.get(key)
    if not isinstance(items, list):
        return []
    findings = []
    for item in items[:8]:
        if not isinstance(item, dict):
            continue
        status = string_field(item, ("status",)) or label
        statement_type = string_field(item, ("statement_type",)) or "unknown"
        normalized = (
            string_field(item, ("normalized_query",))
            or string_field(item, ("fingerprint",))
            or "unknown"
        )
        count_a = number_to_string(item.get("count_a")) or "0"
        count_b = number_to_string(item.get("count_b")) or "0"
        count_delta = number_to_string(item.get("count_delta")) or "0"
        qps_a = number_to_string(item.get("qps_a")) or "0"
        qps_b = number_to_string(item.get("qps_b")) or "0"
        qps_delta = number_to_string(item.get("qps_delta")) or "0"
        rules = ",".join(string_list(item.get("rules"))) or "-"
        findings.append(
            {
                "severity": compare_fingerprint_severity(key, count_delta),
                "message": (
                    f"{label}: status={status}, statementType={statement_type}, "
                    f"count={count_a}->{count_b} (delta={count_delta}), "
                    f"qps={qps_a}->{qps_b} (delta={qps_delta}), "
                    f"rules={rules}, query={normalized}"
                ),
            }
        )
    return findings


def compare_rule_delta_findings(value: JsonObject) -> list[JsonObject]:
    items = value.get("rule_deltas")
    if not isinstance(items, list):
        return []
    findings = []
    for item in items[:8]:
        if not isinstance(item, dict):
            continue
        rule = string_field(item, ("rule",)) or "unknown"
        count_a = number_to_string(item.get("count_a")) or "0"
        count_b = number_to_string(item.get("count_b")) or "0"
        count_delta = number_to_string(item.get("count_delta")) or "0"
        qps_a = number_to_string(item.get("qps_a")) or "0"
        qps_b = number_to_string(item.get("qps_b")) or "0"
        qps_delta = number_to_string(item.get("qps_delta")) or "0"
        findings.append(
            {
                "severity": compare_delta_severity(count_delta),
                "message": (
                    f"rule delta: rule={rule}, count={count_a}->{count_b} "
                    f"(delta={count_delta}), qps={qps_a}->{qps_b} "
                    f"(delta={qps_delta})"
                ),
            }
        )
    return findings


def first_realtime_sample_reason(value: JsonObject, key: str) -> str | None:
    samples = value.get(key)
    if not isinstance(samples, list) or not samples:
        return None
    first = samples[0]
    if not isinstance(first, dict):
        return None
    return string_field(first, ("reason",))


def influxql_rule_severity(rule: str) -> str:
    if rule in {"write_or_destructive", "large_limit", "no_time_filter"}:
        return "high"
    if rule in {"group_by_high_cardinality_risk", "not_realtime_query"}:
        return "medium"
    return "low"


def influxql_rule_description(rule: str) -> str:
    descriptions = {
        "no_time_filter": "SELECT has no explicit time predicate",
        "has_regex": "query uses regex matching or regex measurement/source",
        "has_wildcard": "query uses wildcard selection, grouping, or metadata scope",
        "large_limit": "LIMIT or SLIMIT is greater than or equal to the configured threshold",
        "group_by_high_cardinality_risk": (
            "non-time GROUP BY dimensions exceed the configured threshold"
        ),
        "meta_query": "metadata or explain query",
        "write_or_destructive": "query writes data or performs destructive changes",
        "not_realtime_query": "select-like query is explicitly non-realtime",
    }
    return descriptions.get(rule, "unrecognized analyzer rule")


def compare_fingerprint_severity(key: str, count_delta: str) -> str:
    if key == "removed_fingerprints":
        return "low"
    if key == "new_fingerprints":
        return "high"
    return compare_delta_severity(count_delta)


def compare_delta_severity(count_delta: str) -> str:
    try:
        value = float(count_delta.strip())
    except ValueError:
        return "medium"
    if value > 0:
        return "high"
    if value < 0:
        return "low"
    return "medium"


def parse_findings_value(items: list[object]) -> list[JsonObject]:
    findings = []
    for item in items:
        finding = parse_finding_value(item)
        if finding:
            findings.append(finding)
    return findings


def parse_finding_value(value: object) -> JsonObject | None:
    if isinstance(value, str) and value.strip():
        return {"message": value.strip()}
    if not isinstance(value, dict):
        return None
    message = string_field(
        value,
        ("message", "summary", "description", "detail", "title", "cause"),
    )
    if not message:
        return None
    finding: JsonObject = {"message": message}
    severity = string_field(value, ("severity", "level", "status"))
    if severity:
        finding["severity"] = severity
    file = string_field(value, ("file", "path", "filename"))
    if file:
        finding["file"] = file
    line = number_field(value, ("line", "lineNumber", "startLine"))
    if line is not None:
        finding["line"] = line
    return finding


def string_field(value: JsonObject, keys: tuple[str, ...]) -> str | None:
    for key in keys:
        item = value.get(key)
        if isinstance(item, str) and item.strip():
            return item.strip()
        if not isinstance(item, bool) and isinstance(item, (int, float)):
            return number_to_string(item)
    return None


def number_field(value: JsonObject, keys: tuple[str, ...]) -> int | None:
    for key in keys:
        item = value.get(key)
        if isinstance(item, int):
            return item
        if isinstance(item, float):
            return int(item)
        if isinstance(item, str):
            try:
                return int(item.strip())
            except ValueError:
                continue
    return None


def int_field(value: JsonObject, key: str) -> int | None:
    return number_field(value, (key,))


def number_to_string(value: object) -> str | None:
    if isinstance(value, bool) or value is None:
        return None
    if isinstance(value, int):
        return str(value)
    if isinstance(value, float):
        return str(int(value)) if value.is_integer() else str(value)
    if isinstance(value, str) and value.strip():
        return value.strip()
    return None


def string_list(value: object) -> list[str]:
    if not isinstance(value, list):
        return []
    return [item.strip() for item in value if isinstance(item, str) and item.strip()]
