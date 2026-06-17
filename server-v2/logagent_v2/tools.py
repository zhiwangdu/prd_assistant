from __future__ import annotations

import base64
import email.utils
import hmac
import json
import os
import re
import shutil
import subprocess
import time
import urllib.error
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


def tool_descriptors(settings: Settings) -> list[JsonObject]:
    descriptors = [
        {
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
            "exportable": tool.enabled,
            "runnable": tool.enabled,
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
        for tool in settings.tools
    ]
    descriptors.extend(built_in_tool_descriptors(settings))
    return descriptors


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
        return {
            "sampleIndex": sample_index,
            "nodeCount": max(1, min(node_count, 200)),
            "generateSvg": bool(params.get("generateSvg", False)),
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
        artifact_id = result["artifact"]["id"]
        store.complete_tool_run(run_id, artifact_id, result["result"])
        return result
    except Exception as error:
        store.fail_tool_run(run_id, str(error))
        raise


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
        runs = [
            run_single_configured_tool(
                settings, store, workspace_id, run_id, tool, entry, normalized_params
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
        settings, store, workspace_id, run_id, tool, None, normalized_params
    )


def run_single_configured_tool(
    settings: Settings,
    store: Store,
    workspace_id: str,
    run_id: str,
    tool: ToolDefinition,
    input_entry: JsonObject | None,
    params: JsonObject,
) -> JsonObject:
    command = Path(tool.command)
    if not command.is_absolute():
        raise ValueError(f"tool {tool.id} command must be an absolute path")
    input_file = resolve_tool_input_path(settings, store, input_entry) if input_entry else None
    action_id = tool_action_id(tool, input_entry, params)
    argv = [str(command), *format_tool_args(tool.args, input_file, action_id, params)]
    try:
        completed = subprocess.run(
            argv,
            check=False,
            capture_output=True,
            timeout=tool.timeout_seconds,
        )
        timed_out = False
    except subprocess.TimeoutExpired as error:
        completed = error
        timed_out = True

    stdout = (completed.stdout or b"")[: tool.max_output_bytes]
    stderr = (completed.stderr or b"")[: tool.max_output_bytes]
    parsed_stdout = parse_json(stdout)
    result = {
        "schemaVersion": 1,
        "toolId": tool.id,
        "displayName": tool.display_name,
        "actionId": action_id,
        "inputFile": input_entry.get("path") if input_entry else None,
        "inputKind": input_entry.get("inputKind") if input_entry else None,
        "params": params,
        "argv": argv,
        "timedOut": timed_out,
        "exitCode": None if timed_out else int(completed.returncode),
        "stdoutPreview": stdout.decode("utf-8", errors="replace"),
        "stderrPreview": stderr.decode("utf-8", errors="replace"),
        "parsedStdout": parsed_stdout,
        "summary": summary_from_stdout(parsed_stdout, stdout, timed_out),
        "findings": findings_from_stdout(parsed_stdout),
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
            "findingCount": len(result["findings"]),
            "evidenceRefPrefix": f"tool_results/{action_id}/result.json#findings/",
        },
    )
    return {"result": result, "artifact": artifact, "evidence": evidence}


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
    text_files_by_path: dict[str, TextFile] = {}
    for text_file in text_files:
        text_files_by_path[text_file.path] = text_file
        text_files_by_path[fallback_virtual_path(text_file.path)] = text_file
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
        if input_file in text_files_by_path:
            selected.append(
                materialize_fallback_tool_input(
                    settings, store, workspace_id, tool, text_files_by_path[input_file]
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
) -> list[str]:
    result = []
    for arg in args:
        if "{input_file}" in arg and input_file is None:
            raise ValueError("tool arg requires {input_file} but no input file is selected")
        formatted = arg.replace("{input_file}", str(input_file) if input_file else "")
        formatted = formatted.replace("{action_id}", action_id)
        formatted = PARAM_PLACEHOLDER_RE.sub(
            lambda match: tool_param_to_arg(params, match.group(1)),
            formatted,
        )
        result.append(formatted)
    return result


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
    expected = schema.get("type")
    if expected == "string" and not isinstance(value, str):
        raise ValueError(f"tool {tool_id} param {key} must be a string")
    if expected == "integer" and (isinstance(value, bool) or not isinstance(value, int)):
        raise ValueError(f"tool {tool_id} param {key} must be an integer")
    if expected == "number" and (
        isinstance(value, bool) or not isinstance(value, (int, float))
    ):
        raise ValueError(f"tool {tool_id} param {key} must be a number")
    if expected == "boolean" and not isinstance(value, bool):
        raise ValueError(f"tool {tool_id} param {key} must be a boolean")
    if expected == "array" and not isinstance(value, list):
        raise ValueError(f"tool {tool_id} param {key} must be an array")
    enum = schema.get("enum")
    if isinstance(enum, list) and value not in enum:
        raise ValueError(f"tool {tool_id} param {key} must be one of {enum}")


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
    params_digest = ""
    if params:
        params_digest = ":" + json.dumps(params, ensure_ascii=True, sort_keys=True)
    if input_entry is None:
        if not params:
            return tool.id
        digest = sha256(params_digest.encode("utf-8")).hexdigest()[:12]
        return f"{safe_action_segment(tool.id)}_{digest}"
    digest = sha256(
        (str(input_entry.get("path", "")) + params_digest).encode("utf-8")
    ).hexdigest()[:12]
    return f"{safe_action_segment(tool.id)}_{digest}"


def safe_action_segment(value: str) -> str:
    result = "".join(char if char.isalnum() or char in "._-" else "_" for char in value)
    return result[:80] or "tool"


def run_preprocess_tool(
    settings: Settings,
    store: Store,
    run: JsonObject,
    params: JsonObject,
) -> JsonObject:
    if params:
        raise ValueError("preprocess tool does not accept params")
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
        run["workspace_id"],
        run["id"],
        uploads,
        text_files,
        tool_inputs_path=tool_input_bundle.get("path"),
        tool_input_count=len(tool_input_bundle.get("inputs", [])),
    )
    grep_results = grep_text_files(
        text_files,
        search_keywords(""),
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
    node_packages: dict[str, JsonObject] = {}
    for text_file in text_files:
        if text_file.log_group:
            log_groups[text_file.log_group] = log_groups.get(text_file.log_group, 0) + 1
        if text_file.node_package:
            node_packages[
                f"{text_file.node_package.get('nodeId')}:{text_file.node_package.get('timestamp')}"
            ] = text_file.node_package
    nodes = preprocess_node_summaries(text_files)
    action_id = f"act_tool_preprocess_{run['id']}"
    result = {
        "schemaVersion": 1,
        "toolId": PREPROCESS_LOG_PACKAGE_ID,
        "actionId": action_id,
        "status": "OK",
        "summary": (
            f"preprocessed {len(uploads)} upload(s), {len(nodes)} node(s), "
            f"{len(text_files)} extracted file(s), "
            f"{len(tool_input_bundle.get('inputs', []))} materialized tool input(s)"
        ),
        "uploadCount": len(uploads),
        "fileCount": len(text_files),
        "nodes": nodes,
        "nodePackages": list(node_packages.values()),
        "logGroups": log_groups,
        "warnings": [],
        "manifestArtifactId": manifest_artifact["id"],
        "grepArtifactId": grep_artifact["id"],
        "toolInputIndex": tool_input_bundle.get("inputs", []),
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


def preprocess_node_summaries(text_files: list[TextFile]) -> list[JsonObject]:
    nodes: dict[str, JsonObject] = {}
    package_keys: dict[str, set[str]] = {}
    for text_file in text_files:
        package = text_file.node_package
        if not package:
            continue
        node_id = str(package.get("nodeId") or "")
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
        seen = package_keys.setdefault(node_id, set())
        package_key = f"{text_file.source_upload_id}:{package.get('timestamp', '')}"
        if package_key not in seen:
            seen.add(package_key)
            entry["packages"] = len(seen)
        append_unique_string(entry["instanceIds"], package.get("instanceId"))
        append_unique_string(entry["timestamps"], package.get("timestamp"))
        if text_file.log_group:
            groups = entry["logGroups"]
            group = groups.setdefault(
                text_file.log_group,
                {"fileCount": 0, "compressedFileCount": 0},
            )
            group["fileCount"] += 1
            source_path = text_file.original_path or text_file.path
            if source_path.lower().endswith(".gz"):
                group["compressedFileCount"] += 1
    return [nodes[node_id] for node_id in sorted(nodes)]


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
    if tool_id == METADATA_LIST_INSTANCES_ID:
        value = {"instances": store.list_metadata_instances()}
    elif tool_id == METADATA_GET_SNAPSHOT_ID:
        snapshot = store.get_metadata_snapshot(params["instanceId"])
        value = {**snapshot, "snapshot": snapshot}
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
    else:
        raise ValueError(f"unsupported metadata tool {tool_id}")
    action_id = f"act_tool_{safe_action_segment(tool_id)}_{run['id']}"
    result = {
        "schemaVersion": 1,
        "toolId": tool_id,
        "actionId": action_id,
        "status": "OK",
        "summary": f"Metadata tool {tool_id} completed.",
        "value": value,
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
    action_id = f"act_tool_pprof_{run['id']}"
    node_count = int(params["nodeCount"])
    sample_index = str(params["sampleIndex"])
    commands = {
        "top": [
            go_command,
            "tool",
            "pprof",
            f"-sample_index={sample_index}",
            "-top",
            f"-nodecount={node_count}",
            str(profile_path),
        ],
        "tree": [go_command, "tool", "pprof", f"-sample_index={sample_index}", "-tree", str(profile_path)],
        "raw": [go_command, "tool", "pprof", f"-sample_index={sample_index}", "-raw", str(profile_path)],
    }
    if params.get("generateSvg"):
        commands["svg"] = [
            go_command,
            "tool",
            "pprof",
            f"-sample_index={sample_index}",
            "-svg",
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
        "artifacts": artifact_ids,
        "artifactIds": artifact_ids,
        "artifactPaths": artifact_paths,
        "warnings": warnings,
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
    result = {
        "schemaVersion": 1,
        "toolId": HUAWEI_PACKAGE_SYNC_TOOL_ID,
        "actionId": action_id,
        "status": status,
        "summary": (
            "Huawei package sync completed."
            if status == "OK"
            else f"Huawei package sync failed at {failed_step}: {error}"
        ),
        "objectKey": object_key,
        "objectUrl": huawei_object_url(settings, object_key),
        "upload": {"uploadId": upload["id"], "filename": upload["filename"]},
        "obsPut": obs_put,
        "obsHead": obs_head,
        "gaussdbUpdate": update_result,
        "gaussdbQuery": query_result,
        "failedStep": failed_step,
        "error": error,
        "durationMs": int((time.monotonic() - started) * 1000),
        "credentialEnv": {
            "obsAccessKey": "LOGAGENT_V2_HUAWEI_OBS_ACCESS_KEY",
            "obsSecretKey": "LOGAGENT_V2_HUAWEI_OBS_SECRET_KEY",
            "gaussdbDsn": "LOGAGENT_V2_HUAWEI_GAUSSDB_DSN",
        },
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
    endpoint = config.obs_endpoint.rstrip("/")
    return f"{endpoint}/{config.obs_bucket}/{object_key}"


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
                "contentLength": response.headers.get("Content-Length"),
                "durationMs": int((time.monotonic() - started) * 1000),
            }
    except urllib.error.HTTPError as error:
        raise ValueError(f"OBS {method} returned HTTP {error.code}") from error


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
                rows = cursor.fetchmany(200)
                return {
                    "rowCount": len(rows),
                    "truncated": len(rows) == 200,
                    "rows": [
                        {columns[index]: stringify_sql_value(value) for index, value in enumerate(row)}
                        for row in rows
                    ],
                    "durationMs": int((time.monotonic() - started) * 1000),
                }
            affected = cursor.rowcount
        conn.commit()
    return {"affectedRows": affected, "durationMs": int((time.monotonic() - started) * 1000)}


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


def summary_from_stdout(parsed: object | None, stdout: bytes, timed_out: bool) -> str:
    if timed_out:
        return "Tool timed out."
    if isinstance(parsed, dict):
        if is_influxql_report(parsed):
            return influxql_report_summary(parsed)
        if is_influxql_compare_report(parsed):
            return influxql_compare_summary(parsed)
        summary = string_field(parsed, ("summary", "message", "title"))
        if summary:
            return summary
    if isinstance(parsed, str) and parsed.strip():
        return parsed.strip()[:500]
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
        and isinstance(value.get("fingerprints"), list)
    )


def is_influxql_compare_report(value: JsonObject) -> bool:
    return "batch_a" in value and "batch_b" in value and "statement_delta" in value


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
