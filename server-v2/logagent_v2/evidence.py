from __future__ import annotations

import gzip
import io
import json
import posixpath
import re
import stat
import tarfile
import zipfile
from collections import defaultdict
from dataclasses import dataclass
from hashlib import sha256
from pathlib import PurePosixPath
from typing import Iterable

from .artifacts import resolve_artifact_path, write_artifact_bytes, write_artifact_directory
from .config import DEFAULT_GREP_KEYWORDS, Settings
from .ids import new_id
from .store import JsonObject, Store


TEXT_SUFFIXES = {
    ".log",
    ".txt",
    ".out",
    ".err",
    ".trace",
    ".json",
    ".jsonl",
    ".yaml",
    ".yml",
    ".conf",
    ".cfg",
}
ARCHIVE_SUFFIXES = (".zip", ".tar", ".tar.gz", ".tgz")
BACKGROUND_EVIDENCE_KINDS = {"environment_evidence"}
SESSION_TEXT_INPUT_REF = "session_text_input.json#question"
NODE_LOG_PACKAGE_SUFFIXES = ("_logs.tar.gz", "_logs.tgz")
NODE_LOG_PACKAGE_TIMESTAMP_WIDTHS = (4, 2, 2, 2, 2, 2, 6)


@dataclass(frozen=True)
class TextFile:
    source_upload_id: str
    source_filename: str
    path: str
    size_bytes: int
    sha256: str
    text: str
    original_path: str | None = None
    log_group: str | None = None
    node_package: JsonObject | None = None


@dataclass(frozen=True)
class NodeLogPackage:
    package_id: str
    instance_id: str
    node_id: str
    timestamp: str


@dataclass(frozen=True)
class MaterializedToolInput:
    entry: JsonObject
    artifact: JsonObject


@dataclass(frozen=True)
class StorageArchiveMember:
    path: str
    data: bytes
    tool_ids: list[str]


def persist_session_text_input(
    settings: Settings,
    store: Store,
    workspace_id: str,
    run_id: str,
    question: str,
) -> JsonObject:
    document = {
        "schemaVersion": 1,
        "question": question,
    }
    artifact = write_artifact_bytes(
        settings=settings,
        store=store,
        workspace_id=workspace_id,
        filename="session_text_input.json",
        data=json.dumps(document, ensure_ascii=True, indent=2).encode("utf-8"),
        content_type="application/json",
        schema_name="logagent.v2.session_text_input.v1",
        preview={"questionPreview": question[:300]},
    )
    evidence = store.create_evidence(
        workspace_id=workspace_id,
        run_id=run_id,
        kind="user_question",
        final_allowed=True,
        summary="User question captured as initial evidence.",
        artifact_id=artifact["id"],
        payload={
            "artifactId": artifact["id"],
            "path": "session_text_input.json",
            "ref": SESSION_TEXT_INPUT_REF,
            "question": question,
        },
    )
    return {"document": document, "artifact": artifact, "evidence": evidence}


def build_initial_evidence(
    settings: Settings,
    store: Store,
    workspace_id: str,
    run_id: str,
) -> JsonObject:
    workspace = store.get_workspace(workspace_id)
    uploads = store.list_uploads(workspace_id)
    text_files = collect_text_files(settings, uploads)
    keywords = search_keywords(workspace["question"], settings.grep_keywords)
    tool_input_bundle = materialize_tool_inputs(
        settings,
        store,
        workspace_id,
        uploads,
        text_files,
    )
    manifest = build_manifest(
        settings,
        workspace_id,
        run_id,
        uploads,
        text_files,
        tool_inputs_path=tool_input_bundle.get("path"),
        tool_input_count=len(tool_input_bundle.get("inputs", [])),
    )
    grep_results = grep_text_files(
        text_files,
        keywords,
        settings.max_grep_matches,
        ref_base="grep_results.json#matches/",
    )

    manifest_artifact = write_json_artifact(
        settings,
        store,
        workspace_id,
        "manifest.json",
        manifest,
        schema_name="logagent.v2.manifest.v1",
    )
    grep_artifact = write_json_artifact(
        settings,
        store,
        workspace_id,
        "grep_results.json",
        grep_results,
        schema_name="logagent.v2.grep_results.v1",
    )

    store.create_evidence(
        workspace_id=workspace_id,
        run_id=run_id,
        kind="manifest",
        final_allowed=False,
        summary=f"Collected {len(text_files)} text files from {len(uploads)} uploads.",
        artifact_id=manifest_artifact["id"],
        payload={
            "artifactId": manifest_artifact["id"],
            "path": "manifest.json",
            "fileCount": len(text_files),
        },
    )
    store.create_evidence(
        workspace_id=workspace_id,
        run_id=run_id,
        kind="log_search",
        final_allowed=True,
        summary=f"Initial log search found {grep_results['totalMatches']} matches.",
        artifact_id=grep_artifact["id"],
        payload={
            "artifactId": grep_artifact["id"],
            "path": "grep_results.json",
            "totalMatches": grep_results["totalMatches"],
            "evidenceRefPrefix": "grep_results.json#matches/",
        },
    )
    if tool_input_bundle.get("artifact"):
        store.create_evidence(
            workspace_id=workspace_id,
            run_id=run_id,
            kind="tool_input_index",
            final_allowed=False,
            summary=f"Materialized {len(tool_input_bundle['inputs'])} tool input file(s).",
            artifact_id=tool_input_bundle["artifact"]["id"],
            payload={
                "artifactId": tool_input_bundle["artifact"]["id"],
                "path": tool_input_bundle["path"],
                "inputCount": len(tool_input_bundle["inputs"]),
            },
        )
    return {
        "manifest": manifest,
        "grepResults": grep_results,
        "manifestArtifact": manifest_artifact,
        "grepArtifact": grep_artifact,
        "toolInputIndex": tool_input_bundle if tool_input_bundle.get("artifact") else None,
        "backgroundEvidence": background_evidence_outline(store, run_id),
    }


def background_evidence_outline(store: Store, run_id: str) -> list[JsonObject]:
    items = [
        item
        for item in store.list_evidence(run_id)
        if item.get("kind") in BACKGROUND_EVIDENCE_KINDS
    ]
    return [
        {
            "evidenceId": item.get("id"),
            "kind": item.get("kind"),
            "summary": item.get("summary"),
            "artifactId": item.get("artifact_id"),
            "payload": item.get("payload", {}),
            "finalEvidenceAllowed": False,
            "createdAt": item.get("created_at"),
        }
        for item in items[-20:]
    ]


def run_log_search(
    settings: Settings,
    store: Store,
    workspace_id: str,
    run_id: str,
    keywords: list[str],
    max_matches: int | None = None,
) -> JsonObject:
    uploads = store.list_uploads(workspace_id)
    text_files = collect_text_files(settings, uploads)
    search_id = new_id("logsearch")
    artifact_path = f"log_searches/{search_id}.json"
    match_limit = max_matches if max_matches is not None else settings.max_grep_matches
    results = grep_text_files(
        text_files,
        keywords,
        match_limit,
        ref_base=f"{artifact_path}#matches/",
    )
    results["searchId"] = search_id
    results["path"] = artifact_path
    artifact = write_json_artifact(
        settings,
        store,
        workspace_id,
        f"{search_id}.json",
        results,
        schema_name="logagent.v2.log_search.v1",
    )
    evidence = store.create_evidence(
        workspace_id=workspace_id,
        run_id=run_id,
        kind="log_search",
        final_allowed=True,
        summary=f"Follow-up log search found {results['totalMatches']} matches.",
        artifact_id=artifact["id"],
        payload={
            "artifactId": artifact["id"],
            "path": artifact_path,
            "totalMatches": results["totalMatches"],
            "evidenceRefPrefix": f"{artifact_path}#matches/",
        },
    )
    return {"search": results, "artifact": artifact, "evidence": evidence}


def get_log_slice(
    settings: Settings,
    store: Store,
    workspace_id: str,
    run_id: str,
    path: str,
    line_number: int,
    before: int,
    after: int,
) -> JsonObject:
    if line_number < 1:
        raise ValueError("lineNumber must be >= 1")
    before = max(0, min(before, 50))
    after = max(0, min(after, 50))
    return get_log_line_range(
        settings=settings,
        store=store,
        workspace_id=workspace_id,
        run_id=run_id,
        path=path,
        start_line=max(1, line_number - before),
        end_line=line_number + after,
        line_number=line_number,
    )


def get_log_line_range(
    settings: Settings,
    store: Store,
    workspace_id: str,
    run_id: str,
    path: str,
    start_line: int,
    end_line: int,
    line_number: int | None = None,
) -> JsonObject:
    if start_line < 1 or end_line < 1:
        raise ValueError("startLine and endLine must be >= 1")
    if end_line < start_line:
        raise ValueError("endLine must be greater than or equal to startLine")
    if end_line - start_line > 500:
        raise ValueError("line range must contain at most 500 lines")
    uploads = store.list_uploads(workspace_id)
    text_files = collect_text_files(settings, uploads)
    selected = resolve_text_file_selector(text_files, path)
    if selected is None:
        raise ValueError(f"log path {path!r} is not available in this workspace")
    lines = selected.text.splitlines()
    start = start_line
    end = min(end_line, len(lines))
    slice_id = stable_log_slice_id(
        path=path,
        line_number=line_number,
        start_line=start,
        end_line=end,
    )
    slice_path = f"log_slices/{slice_id}.json"
    result = {
        "schemaVersion": 1,
        "sliceId": slice_id,
        "path": path,
        "sourcePath": selected.path,
        "sourceUploadId": selected.source_upload_id,
        "lineNumber": line_number or start_line,
        "startLine": start,
        "endLine": end,
        "lines": [
            {
                "lineNumber": current,
                "line": current,
                "text": lines[current - 1][:4000],
            }
            for current in range(start, end + 1)
        ],
        "ref": f"{slice_path}#lines",
    }
    if path != selected.path:
        result["requestedPath"] = path
    artifact = write_json_artifact(
        settings,
        store,
        workspace_id,
        f"{slice_id}.json",
        result,
        schema_name="logagent.v2.log_slice.v1",
    )
    evidence = store.create_evidence(
        workspace_id=workspace_id,
        run_id=run_id,
        kind="log_slice",
        final_allowed=True,
        summary=f"Log slice {path}:{start}-{end}.",
        artifact_id=artifact["id"],
        payload={
            "artifactId": artifact["id"],
            "path": slice_path,
            "sourcePath": path,
            "lineNumber": line_number,
            "ref": result["ref"],
        },
    )
    return {"slice": result, "artifact": artifact, "evidence": evidence}


def stable_log_slice_id(
    *,
    path: str,
    line_number: int | None,
    start_line: int,
    end_line: int,
) -> str:
    payload = json.dumps(
        {
            "path": path,
            "lineNumber": line_number,
            "startLine": start_line,
            "endLine": end_line,
        },
        ensure_ascii=True,
        sort_keys=True,
        separators=(",", ":"),
    )
    digest = sha256(payload.encode("utf-8")).hexdigest()[:16]
    return f"slice_{digest}"


def resolve_text_file_selector(text_files: list[TextFile], path: str) -> TextFile | None:
    for text_file in text_files:
        if text_file.path == path:
            return text_file
    aliases: dict[str, TextFile] = {}
    ambiguous: set[str] = set()
    for text_file in text_files:
        for alias in text_file_selector_aliases(text_file):
            if not alias or alias == text_file.path:
                continue
            if alias in ambiguous:
                continue
            existing = aliases.get(alias)
            if existing is not None and existing != text_file:
                aliases.pop(alias, None)
                ambiguous.add(alias)
                continue
            aliases[alias] = text_file
    if path in ambiguous:
        raise ValueError(f"log path {path!r} is ambiguous in this workspace")
    return aliases.get(path)


def text_file_selector_aliases(text_file: TextFile) -> list[str]:
    aliases = [
        text_file.source_filename,
        text_file.original_path,
        posixpath.basename(text_file.path),
    ]
    if text_file.original_path:
        aliases.append(f"extracted/{text_file.original_path}")
    return [alias for alias in aliases if alias]


def collect_text_files(settings: Settings, uploads: list[JsonObject]) -> list[TextFile]:
    text_files: list[TextFile] = []
    total_archive_bytes = 0
    used_extracted_dirs: list[str] = []
    for upload in uploads:
        filename = upload["filename"]
        artifact_path = resolve_artifact_path(settings, upload["artifact_relative_path"])
        raw = artifact_path.read_bytes()
        path_prefix = None
        if parse_node_log_package(filename) is None:
            path_prefix = generic_extracted_dir(filename, used_extracted_dirs)
        if is_archive(filename):
            extracted, extracted_bytes = read_archive_text_files(
                settings,
                upload,
                raw,
                path_prefix,
            )
            total_archive_bytes += extracted_bytes
            if total_archive_bytes > settings.max_archive_bytes:
                raise ValueError("archive extraction exceeds LOGAGENT_V2_MAX_ARCHIVE_BYTES")
            text_files.extend(extracted)
        elif is_text_path(filename):
            assert path_prefix is not None
            text_files.append(
                text_file_from_bytes(
                    settings,
                    upload,
                    direct_upload_logical_path(filename, path_prefix),
                    raw,
                    original_path=filename,
                )
            )
    return text_files


def is_archive(path: str) -> bool:
    lowered = path.lower()
    return lowered.endswith(ARCHIVE_SUFFIXES)


def is_text_path(path: str) -> bool:
    lowered = path.lower()
    return any(lowered.endswith(suffix) for suffix in TEXT_SUFFIXES)


def read_archive_text_files(
    settings: Settings,
    upload: JsonObject,
    raw: bytes,
    path_prefix: str | None,
) -> tuple[list[TextFile], int]:
    filename = upload["filename"].lower()
    if filename.endswith(".zip"):
        if path_prefix is None:
            raise ValueError("zip uploads require a logical path prefix")
        return read_zip_text_files(settings, upload, raw, path_prefix)
    if filename.endswith((".tar", ".tar.gz", ".tgz")):
        return read_tar_text_files(settings, upload, raw, path_prefix)
    return [], 0


def read_zip_text_files(
    settings: Settings,
    upload: JsonObject,
    raw: bytes,
    path_prefix: str,
) -> tuple[list[TextFile], int]:
    result: list[TextFile] = []
    total_bytes = 0
    with zipfile.ZipFile(io.BytesIO(raw)) as archive:
        for index, info in enumerate(archive.infolist()):
            if index >= settings.max_archive_files:
                raise ValueError("archive file count exceeds LOGAGENT_V2_MAX_ARCHIVE_FILES")
            if info.is_dir():
                continue
            mode = (info.external_attr >> 16) & 0o170000
            if mode and stat.S_ISLNK(mode):
                continue
            path = safe_archive_path(info.filename)
            if not is_text_path(path):
                continue
            logical_path = f"{path_prefix}/{path}"
            if info.file_size > settings.max_text_file_bytes:
                continue
            data = archive.read(info, pwd=None)
            total_bytes += len(data)
            result.append(
                text_file_from_bytes(
                    settings,
                    upload,
                    logical_path,
                    data,
                    original_path=path,
                )
            )
    return result, total_bytes


def read_tar_text_files(
    settings: Settings,
    upload: JsonObject,
    raw: bytes,
    path_prefix: str | None,
) -> tuple[list[TextFile], int]:
    result: list[TextFile] = []
    total_bytes = 0
    node_package = parse_node_log_package(upload["filename"])
    with tarfile.open(fileobj=io.BytesIO(raw), mode="r:*") as archive:
        for index, member in enumerate(archive):
            if index >= settings.max_archive_files:
                raise ValueError("archive file count exceeds LOGAGENT_V2_MAX_ARCHIVE_FILES")
            if not member.isfile():
                continue
            path = safe_archive_path(member.name)
            logical_path = path
            log_group: str | None = None
            if node_package is not None:
                classified = classify_node_log_member(path, node_package)
                if classified is None:
                    continue
                logical_path, log_group = classified
            elif not is_text_path(path):
                continue
            else:
                if path_prefix is None:
                    raise ValueError("tar uploads require a logical path prefix")
                logical_path = f"{path_prefix}/{path}"
            if member.size > settings.max_text_file_bytes:
                continue
            extracted = archive.extractfile(member)
            if extracted is None:
                continue
            data = extracted.read(settings.max_text_file_bytes + 1)
            if len(data) > settings.max_text_file_bytes:
                continue
            decoded = decode_log_bytes(data, settings.max_text_file_bytes)
            if decoded is None:
                continue
            total_bytes += len(decoded)
            result.append(
                text_file_from_bytes(
                    settings,
                    upload,
                    logical_path,
                    decoded,
                    original_path=path,
                    log_group=log_group,
                    node_package=node_package,
                )
            )
    if node_package is not None and not result:
        raise ValueError("node log package contains no supported log directories")
    return result, total_bytes


def safe_archive_path(path: str) -> str:
    normalized = posixpath.normpath(path.replace("\\", "/"))
    pure = PurePosixPath(normalized)
    if normalized in {"", "."} or pure.is_absolute() or ".." in pure.parts:
        raise ValueError(f"unsafe archive path {path!r}")
    return normalized


def text_file_from_bytes(
    settings: Settings,
    upload: JsonObject,
    path: str,
    data: bytes,
    original_path: str | None = None,
    log_group: str | None = None,
    node_package: NodeLogPackage | None = None,
) -> TextFile:
    if len(data) > settings.max_text_file_bytes:
        raise ValueError(f"text file {path} exceeds LOGAGENT_V2_MAX_TEXT_FILE_BYTES")
    return TextFile(
        source_upload_id=upload["id"],
        source_filename=upload["filename"],
        path=path,
        size_bytes=len(data),
        sha256=sha256(data).hexdigest(),
        text=data.decode("utf-8", errors="replace"),
        original_path=original_path,
        log_group=log_group,
        node_package=node_package_payload(node_package),
    )


def parse_node_log_package(filename: str) -> NodeLogPackage | None:
    stem = node_log_package_stem(filename)
    if stem is None:
        return None
    parts = stem.split("_")
    if len(parts) == 10:
        package_id, instance_id, node_id, *timestamp_parts = parts
        if not all(is_safe_log_package_id(item) for item in (package_id, instance_id, node_id)):
            return None
        if not v1_timestamp_parts_are_valid(timestamp_parts):
            return None
        return NodeLogPackage(
            package_id=package_id,
            instance_id=instance_id,
            node_id=node_id,
            timestamp="_".join(timestamp_parts),
        )
    if len(parts) == 4:
        package_id, instance_id, node_id, timestamp = parts
        if not all(is_safe_log_package_id(item) for item in (package_id, instance_id, node_id)):
            return None
        if not timestamp or not all(char.isascii() and char.isalnum() for char in timestamp):
            return None
        return NodeLogPackage(
            package_id=package_id,
            instance_id=instance_id,
            node_id=node_id,
            timestamp=timestamp,
        )
    return None


def node_log_package_stem(filename: str) -> str | None:
    lowered = filename.lower()
    for suffix in NODE_LOG_PACKAGE_SUFFIXES:
        if lowered.endswith(suffix):
            return filename[: -len(suffix)]
    return None


def is_safe_log_package_id(value: str) -> bool:
    return bool(value) and len(value) <= 128 and all(
        char.isascii() and char.isalnum() for char in value
    )


def v1_timestamp_parts_are_valid(parts: list[str]) -> bool:
    if len(parts) != len(NODE_LOG_PACKAGE_TIMESTAMP_WIDTHS):
        return False
    return all(
        len(value) == width and value.isascii() and value.isdigit()
        for value, width in zip(parts, NODE_LOG_PACKAGE_TIMESTAMP_WIDTHS, strict=True)
    )


def classify_node_log_member(
    path: str, node_package: NodeLogPackage
) -> tuple[str, str] | None:
    parts = PurePosixPath(path).parts
    markers = (
        (("var", "chroot", "gemini", "log", "tsdb"), "tsdb"),
        (("var", "chroot", "gemini", "log", "stream"), "stream"),
        (("home", "Ruby", "log"), "agent"),
    )
    for marker, group in markers:
        start = find_subsequence(parts, marker)
        if start is None:
            continue
        tail = parts[start + len(marker) :]
        if not tail:
            return None
        relative_tail = "/".join(tail)
        return (
            f"extracted/{node_package.node_id}/{node_package.timestamp}/{group}/{relative_tail}",
            group,
        )
    return None


def find_subsequence(parts: tuple[str, ...], marker: tuple[str, ...]) -> int | None:
    if len(parts) < len(marker):
        return None
    for index in range(0, len(parts) - len(marker) + 1):
        if parts[index : index + len(marker)] == marker:
            return index
    return None


def decode_log_bytes(data: bytes, max_bytes: int) -> bytes | None:
    if not data.startswith(b"\x1f\x8b"):
        return data
    try:
        with gzip.GzipFile(fileobj=io.BytesIO(data)) as archive:
            decoded = archive.read(max_bytes + 1)
    except OSError:
        return data
    if len(decoded) > max_bytes:
        return None
    return decoded


def node_package_payload(node_package: NodeLogPackage | None) -> JsonObject | None:
    if node_package is None:
        return None
    return {
        "packageId": node_package.package_id,
        "instanceId": node_package.instance_id,
        "nodeId": node_package.node_id,
        "timestamp": node_package.timestamp,
    }


def materialize_tool_inputs(
    settings: Settings,
    store: Store,
    workspace_id: str,
    uploads: list[JsonObject],
    text_files: list[TextFile],
) -> JsonObject:
    inputs = [
        *materialize_node_package_log_text_inputs(settings, store, workspace_id, text_files),
        *materialize_influxql_inputs(settings, store, workspace_id, text_files),
        *materialize_flux_inputs(settings, store, workspace_id, text_files),
        *materialize_storage_inputs(settings, store, workspace_id, uploads),
    ]
    if not inputs:
        return {"path": None, "inputs": []}
    index = {
        "schemaVersion": 1,
        "generatedBy": "logagent_v2_tool_input_materializer",
        "inputs": [item.entry for item in inputs],
    }
    artifact = write_json_artifact(
        settings,
        store,
        workspace_id,
        "tool_inputs_index.json",
        index,
        schema_name="logagent.v2.tool_input_index.v1",
    )
    return {
        "path": "tool_inputs/index.json",
        "inputs": index["inputs"],
        "artifact": artifact,
    }


def materialize_node_package_log_text_inputs(
    settings: Settings,
    store: Store,
    workspace_id: str,
    text_files: list[TextFile],
) -> list[MaterializedToolInput]:
    grouped_records: dict[tuple[str, str, str, str], list[JsonObject]] = defaultdict(list)
    grouped_sources: dict[tuple[str, str, str, str], set[str]] = defaultdict(set)
    for text_file in text_files:
        package = text_file.node_package
        log_group = text_file.log_group
        if not package or not log_group:
            continue
        node_id = str(package.get("nodeId") or "unknown")
        timestamp = str(package.get("timestamp") or "unknown")
        instance_id = str(package.get("instanceId") or "")
        key = (node_id, timestamp, instance_id, log_group)
        for line_number, line in enumerate(text_file.text.splitlines(), start=1):
            grouped_records[key].append(
                {
                    "schemaVersion": 1,
                    "nodeId": node_id,
                    "instanceId": instance_id,
                    "packageTimestamp": timestamp,
                    "logGroup": log_group,
                    "sourcePath": text_file.path,
                    "originalPath": text_file.original_path or text_file.path,
                    "line": line_number,
                    "message": line,
                }
            )
            grouped_sources[key].add(text_file.path)

    results: list[MaterializedToolInput] = []
    for key, records in grouped_records.items():
        node_id, timestamp, instance_id, log_group = key
        if not records:
            continue
        clean_node = safe_segment(node_id)
        clean_timestamp = safe_segment(timestamp)
        clean_group = safe_segment(log_group)
        virtual_path = f"tool_inputs/log_text/{clean_node}/{clean_timestamp}/{clean_group}.jsonl"
        data = "\n".join(json.dumps(record, ensure_ascii=True) for record in records) + "\n"
        artifact = write_artifact_bytes(
            settings=settings,
            store=store,
            workspace_id=workspace_id,
            filename=f"log_text_{clean_node}_{clean_timestamp}_{clean_group}.jsonl",
            data=data.encode("utf-8"),
            content_type="application/x-ndjson",
            schema_name="logagent.v2.tool_input.log_text_jsonl.v1",
            preview={
                "path": virtual_path,
                "recordCount": len(records),
                "logGroup": log_group,
            },
        )
        entry = {
            "path": virtual_path,
            "inputKind": "log_text_jsonl",
            "scope": "log_group",
            "nodeId": node_id,
            "instanceId": instance_id or None,
            "packageTimestamp": timestamp,
            "logGroup": log_group,
            "sourceFiles": sorted(grouped_sources[key]),
            "recordCount": len(records),
            "artifactId": artifact["id"],
            "artifactRelativePath": artifact["relative_path"],
        }
        results.append(MaterializedToolInput(entry=entry, artifact=artifact))
    return results


def materialize_influxql_inputs(
    settings: Settings,
    store: Store,
    workspace_id: str,
    text_files: list[TextFile],
) -> list[MaterializedToolInput]:
    return [
        *materialize_node_package_influxql_inputs(settings, store, workspace_id, text_files),
        *materialize_file_query_inputs(
            settings=settings,
            store=store,
            workspace_id=workspace_id,
            text_files=[
                text_file
                for text_file in text_files
                if not (text_file.node_package and text_file.log_group == "tsdb")
            ],
            tool_id="influxql_analyzer",
            input_kind="influxql_jsonl",
            virtual_root="tool_inputs/influxql_analyzer/workspace",
            filename_prefix="influxql_workspace",
            schema_name="logagent.v2.tool_input.influxql_jsonl.v1",
            extractor=extract_influxql_query,
        ),
    ]


def materialize_node_package_influxql_inputs(
    settings: Settings,
    store: Store,
    workspace_id: str,
    text_files: list[TextFile],
) -> list[MaterializedToolInput]:
    grouped_records: dict[tuple[str, str, str], list[JsonObject]] = defaultdict(list)
    grouped_sources: dict[tuple[str, str, str], set[str]] = defaultdict(set)
    metadata: dict[tuple[str, str, str], JsonObject] = {}
    for text_file in text_files:
        package = text_file.node_package
        if not package or text_file.log_group != "tsdb":
            continue
        node_id = str(package.get("nodeId") or "unknown")
        timestamp = str(package.get("timestamp") or "unknown")
        instance_id = str(package.get("instanceId") or "")
        key = (node_id, timestamp, instance_id)
        metadata[key] = package
        for line_number, line in enumerate(text_file.text.splitlines(), start=1):
            query = extract_influxql_query(line)
            if not query:
                continue
            grouped_records[key].append(
                {
                    "query": query,
                    "sourcePath": text_file.path,
                    "line": line_number,
                    "lineNumber": line_number,
                    "nodeId": node_id,
                    "instanceId": instance_id or None,
                    "packageTimestamp": timestamp,
                    "logGroup": "tsdb",
                }
            )
            grouped_sources[key].add(text_file.path)

    results: list[MaterializedToolInput] = []
    for key, records in grouped_records.items():
        node_id, timestamp, instance_id = key
        if not records:
            continue
        clean_node = safe_segment(node_id)
        clean_timestamp = safe_segment(timestamp)
        virtual_path = f"tool_inputs/influxql_analyzer/{clean_node}/{clean_timestamp}.jsonl"
        data = ("\n".join(json.dumps(record, ensure_ascii=True) for record in records) + "\n")
        artifact = write_artifact_bytes(
            settings=settings,
            store=store,
            workspace_id=workspace_id,
            filename=f"influxql_{clean_node}_{clean_timestamp}.jsonl",
            data=data.encode("utf-8"),
            content_type="application/x-ndjson",
            schema_name="logagent.v2.tool_input.influxql_jsonl.v1",
            preview={
                "path": virtual_path,
                "toolIds": ["influxql_analyzer"],
                "recordCount": len(records),
            },
        )
        package = metadata[key]
        entry = {
            "path": virtual_path,
            "inputKind": "influxql_jsonl",
            "scope": "package",
            "toolIds": ["influxql_analyzer"],
            "nodeId": node_id,
            "instanceId": instance_id or None,
            "packageTimestamp": package.get("timestamp"),
            "logGroup": "tsdb",
            "sourceFiles": sorted(grouped_sources[key]),
            "recordCount": len(records),
            "artifactId": artifact["id"],
            "artifactRelativePath": artifact["relative_path"],
        }
        results.append(MaterializedToolInput(entry=entry, artifact=artifact))
    return results


def materialize_storage_inputs(
    settings: Settings,
    store: Store,
    workspace_id: str,
    uploads: list[JsonObject],
) -> list[MaterializedToolInput]:
    active_tool_ids = enabled_storage_tool_ids(settings)
    if not active_tool_ids:
        return []
    results: list[MaterializedToolInput] = []
    for upload in uploads:
        filename = upload["filename"]
        artifact_path = resolve_artifact_path(settings, upload["artifact_relative_path"])
        if is_archive(filename):
            raw = artifact_path.read_bytes()
            results.extend(
                materialize_archive_storage_inputs(
                    settings,
                    store,
                    workspace_id,
                    upload,
                    raw,
                    active_tool_ids,
                )
            )
            continue
        tool_ids = matching_storage_tool_ids(filename, active_tool_ids)
        if not tool_ids:
            continue
        results.append(storage_input_from_upload(upload, tool_ids))
    return results


def materialize_archive_storage_inputs(
    settings: Settings,
    store: Store,
    workspace_id: str,
    upload: JsonObject,
    raw: bytes,
    active_tool_ids: set[str],
) -> list[MaterializedToolInput]:
    filename = upload["filename"].lower()
    if filename.endswith(".zip"):
        return materialize_zip_storage_inputs(
            settings,
            store,
            workspace_id,
            upload,
            raw,
            active_tool_ids,
        )
    if filename.endswith((".tar", ".tar.gz", ".tgz")):
        return materialize_tar_storage_inputs(
            settings,
            store,
            workspace_id,
            upload,
            raw,
            active_tool_ids,
        )
    return []


def materialize_zip_storage_inputs(
    settings: Settings,
    store: Store,
    workspace_id: str,
    upload: JsonObject,
    raw: bytes,
    active_tool_ids: set[str],
) -> list[MaterializedToolInput]:
    members: list[StorageArchiveMember] = []
    total_bytes = 0
    with zipfile.ZipFile(io.BytesIO(raw)) as archive:
        for index, info in enumerate(archive.infolist()):
            if index >= settings.max_archive_files:
                raise ValueError("archive file count exceeds LOGAGENT_V2_MAX_ARCHIVE_FILES")
            if info.is_dir():
                continue
            mode = (info.external_attr >> 16) & 0o170000
            if mode and stat.S_ISLNK(mode):
                continue
            path = safe_archive_path(info.filename)
            tool_ids = matching_storage_tool_ids(path, active_tool_ids)
            if not tool_ids:
                continue
            if total_bytes + info.file_size > settings.max_archive_bytes:
                raise ValueError("storage tool input extraction exceeds LOGAGENT_V2_MAX_ARCHIVE_BYTES")
            data = archive.read(info, pwd=None)
            total_bytes += len(data)
            members.append(StorageArchiveMember(path=path, data=data, tool_ids=tool_ids))
    return materialize_storage_members(settings, store, workspace_id, upload, members)


def materialize_tar_storage_inputs(
    settings: Settings,
    store: Store,
    workspace_id: str,
    upload: JsonObject,
    raw: bytes,
    active_tool_ids: set[str],
) -> list[MaterializedToolInput]:
    members: list[StorageArchiveMember] = []
    total_bytes = 0
    with tarfile.open(fileobj=io.BytesIO(raw), mode="r:*") as archive:
        for index, member in enumerate(archive):
            if index >= settings.max_archive_files:
                raise ValueError("archive file count exceeds LOGAGENT_V2_MAX_ARCHIVE_FILES")
            if not member.isfile():
                continue
            path = safe_archive_path(member.name)
            tool_ids = matching_storage_tool_ids(path, active_tool_ids)
            if not tool_ids:
                continue
            if total_bytes + member.size > settings.max_archive_bytes:
                raise ValueError("storage tool input extraction exceeds LOGAGENT_V2_MAX_ARCHIVE_BYTES")
            extracted = archive.extractfile(member)
            if extracted is None:
                continue
            data = extracted.read(member.size + 1)
            if len(data) > member.size:
                raise ValueError(f"storage archive member changed size while reading: {path}")
            total_bytes += len(data)
            members.append(StorageArchiveMember(path=path, data=data, tool_ids=tool_ids))
    return materialize_storage_members(settings, store, workspace_id, upload, members)


def materialize_storage_members(
    settings: Settings,
    store: Store,
    workspace_id: str,
    upload: JsonObject,
    members: list[StorageArchiveMember],
) -> list[MaterializedToolInput]:
    directory_groups: dict[tuple[str, str], list[StorageArchiveMember]] = defaultdict(list)
    for member in members:
        for tool_id, root in storage_directory_roots_for_path(member.path, set(member.tool_ids)):
            directory_groups[(tool_id, root)].append(member)

    results = materialize_archive_storage_directories(
        settings,
        store,
        workspace_id,
        upload,
        directory_groups,
    )
    covered = {
        (tool_id, member.path)
        for (tool_id, _root), grouped_members in directory_groups.items()
        for member in grouped_members
    }
    for member in members:
        remaining_tool_ids = [
            tool_id for tool_id in member.tool_ids if (tool_id, member.path) not in covered
        ]
        if not remaining_tool_ids:
            continue
        results.append(
            materialize_archive_storage_input(
                settings,
                store,
                workspace_id,
                upload,
                member.path,
                member.data,
                remaining_tool_ids,
            )
        )
    return results


def materialize_archive_storage_directories(
    settings: Settings,
    store: Store,
    workspace_id: str,
    upload: JsonObject,
    directory_groups: dict[tuple[str, str], list[StorageArchiveMember]],
) -> list[MaterializedToolInput]:
    results: list[MaterializedToolInput] = []
    for (tool_id, root), members in sorted(directory_groups.items()):
        files = [
            (archive_relative_path(root, member.path), member.data)
            for member in sorted(members, key=lambda item: item.path)
        ]
        files = [(path, data) for path, data in files if path]
        if not files:
            continue
        digest = sha256(f"{upload['id']}:{tool_id}:{root}".encode("utf-8")).hexdigest()[:16]
        dirname = f"storage_dir_{safe_segment(tool_id)}_{digest}"
        virtual_path = storage_directory_virtual_path(tool_id, digest, root)
        artifact = write_artifact_directory(
            settings=settings,
            store=store,
            workspace_id=workspace_id,
            dirname=dirname,
            files=files,
            schema_name="logagent.v2.tool_input.storage_directory.v1",
            preview={
                "path": virtual_path,
                "toolIds": [tool_id],
                "sourceUploadId": upload["id"],
                "sourceRoot": root,
                "fileCount": len(files),
                "sizeBytes": sum(len(data) for _, data in files),
            },
        )
        entry = {
            "path": virtual_path,
            "inputKind": storage_directory_input_kind(tool_id),
            "scope": "archive_directory",
            "toolIds": [tool_id],
            "sourceFiles": [member.path for member in members],
            "sourceUploadId": upload["id"],
            "sourceFilename": upload["filename"],
            "sourceArchiveRoot": root,
            "fileCount": len(files),
            "sizeBytes": artifact["size_bytes"],
            "artifactId": artifact["id"],
            "artifactRelativePath": artifact["relative_path"],
        }
        results.append(MaterializedToolInput(entry=entry, artifact=artifact))
    return results


def storage_input_from_upload(
    upload: JsonObject,
    tool_ids: list[str],
) -> MaterializedToolInput:
    virtual_path = storage_virtual_path(upload["id"], upload["filename"])
    entry = {
        "path": virtual_path,
        "inputKind": storage_input_kind(tool_ids),
        "scope": "upload",
        "toolIds": tool_ids,
        "sourceFiles": [upload["filename"]],
        "sourceUploadId": upload["id"],
        "sourceFilename": upload["filename"],
        "artifactId": upload["artifact_id"],
        "artifactRelativePath": upload["artifact_relative_path"],
    }
    return MaterializedToolInput(entry=entry, artifact={})


def materialize_archive_storage_input(
    settings: Settings,
    store: Store,
    workspace_id: str,
    upload: JsonObject,
    member_path: str,
    data: bytes,
    tool_ids: list[str],
) -> MaterializedToolInput:
    digest = sha256(f"{upload['id']}:{member_path}".encode("utf-8")).hexdigest()[:16]
    virtual_path = storage_virtual_path(digest, member_path)
    artifact = write_artifact_bytes(
        settings=settings,
        store=store,
        workspace_id=workspace_id,
        filename=f"storage_{digest}_{PurePosixPath(member_path).name}",
        data=data,
        content_type="application/octet-stream",
        schema_name="logagent.v2.tool_input.storage_file.v1",
        preview={
            "path": virtual_path,
            "toolIds": tool_ids,
            "sourceUploadId": upload["id"],
            "sourcePath": member_path,
            "sizeBytes": len(data),
        },
    )
    entry = {
        "path": virtual_path,
        "inputKind": storage_input_kind(tool_ids),
        "scope": "archive",
        "toolIds": tool_ids,
        "sourceFiles": [member_path],
        "sourceUploadId": upload["id"],
        "sourceFilename": upload["filename"],
        "sourceArchivePath": member_path,
        "sizeBytes": len(data),
        "artifactId": artifact["id"],
        "artifactRelativePath": artifact["relative_path"],
    }
    return MaterializedToolInput(entry=entry, artifact=artifact)


def storage_tool_ids_for_path(path: str) -> list[str]:
    lowered = path.lower()
    name = PurePosixPath(lowered).name
    parts = PurePosixPath(lowered).parts
    tool_ids = []
    if (
        name.endswith(".tssp")
        or name.endswith(".tssp.init")
        or "tsi" in name
        or "mergeset" in lowered
        or any("tsi" in part or "mergeset" in part for part in parts[:-1])
    ):
        tool_ids.append("opengemini_storage_analyzer")
    if name.endswith(".tsm") or name.endswith(".tsi") or "_series" in parts or "_series" in lowered:
        tool_ids.append("influxdb_storage_analyzer")
    return tool_ids


def storage_directory_roots_for_path(
    path: str,
    active_tool_ids: set[str],
) -> list[tuple[str, str]]:
    lowered_parts = PurePosixPath(path.lower()).parts
    original_parts = PurePosixPath(path).parts
    roots: list[tuple[str, str]] = []
    if "influxdb_storage_analyzer" in active_tool_ids:
        for index, part in enumerate(lowered_parts[:-1]):
            if part == "_series":
                roots.append(("influxdb_storage_analyzer", "/".join(original_parts[: index + 1])))
                break
    if "opengemini_storage_analyzer" in active_tool_ids:
        for index, part in enumerate(lowered_parts[:-1]):
            if "mergeset" in part or "tsi" in part:
                roots.append(("opengemini_storage_analyzer", "/".join(original_parts[: index + 1])))
                break
    return roots


def archive_relative_path(root: str, path: str) -> str:
    if path == root:
        return ""
    prefix = f"{root}/"
    return path[len(prefix) :] if path.startswith(prefix) else PurePosixPath(path).name


def matching_storage_tool_ids(path: str, active_tool_ids: set[str]) -> list[str]:
    return [tool_id for tool_id in storage_tool_ids_for_path(path) if tool_id in active_tool_ids]


def enabled_storage_tool_ids(settings: Settings) -> set[str]:
    return {
        tool.id
        for tool in settings.tools
        if tool.enabled
        and tool.id in {"opengemini_storage_analyzer", "influxdb_storage_analyzer"}
    }


def storage_input_kind(tool_ids: list[str]) -> str:
    if tool_ids == ["opengemini_storage_analyzer"]:
        return "opengemini_storage_file"
    if tool_ids == ["influxdb_storage_analyzer"]:
        return "influxdb_storage_file"
    return "storage_file"


def storage_directory_input_kind(tool_id: str) -> str:
    if tool_id == "opengemini_storage_analyzer":
        return "opengemini_storage_directory"
    if tool_id == "influxdb_storage_analyzer":
        return "influxdb_storage_directory"
    return "storage_directory"


def storage_virtual_path(prefix: str, source_path: str) -> str:
    digest = sha256(f"{prefix}:{source_path}".encode("utf-8")).hexdigest()[:16]
    suffix = safe_segment(PurePosixPath(source_path).name)
    return f"tool_inputs/storage/{digest}/{suffix}"


def storage_directory_virtual_path(tool_id: str, digest: str, source_root: str) -> str:
    suffix = safe_segment(PurePosixPath(source_root).name)
    return f"tool_inputs/storage_dirs/{safe_segment(tool_id)}/{digest}/{suffix}"


def materialize_flux_inputs(
    settings: Settings,
    store: Store,
    workspace_id: str,
    text_files: list[TextFile],
) -> list[MaterializedToolInput]:
    return materialize_file_query_inputs(
        settings=settings,
        store=store,
        workspace_id=workspace_id,
        text_files=text_files,
        tool_id="flux_query_analyzer",
        input_kind="flux_query_jsonl",
        virtual_root="tool_inputs/flux_query_analyzer/workspace",
        filename_prefix="flux_workspace",
        schema_name="logagent.v2.tool_input.flux_query_jsonl.v1",
        extractor=extract_flux_query,
    )


def materialize_file_query_inputs(
    settings: Settings,
    store: Store,
    workspace_id: str,
    text_files: list[TextFile],
    tool_id: str,
    input_kind: str,
    virtual_root: str,
    filename_prefix: str,
    schema_name: str,
    extractor,
) -> list[MaterializedToolInput]:
    results: list[MaterializedToolInput] = []
    for text_file in text_files:
        records = []
        for line_number, line in enumerate(text_file.text.splitlines(), start=1):
            query = extractor(line)
            if not query:
                continue
            package = text_file.node_package or {}
            records.append(
                {
                    "query": query,
                    "sourcePath": text_file.path,
                    "lineNumber": line_number,
                    "nodeId": package.get("nodeId"),
                    "instanceId": package.get("instanceId"),
                    "packageTimestamp": package.get("timestamp"),
                }
            )
        if not records:
            continue
        input_hash = sha256(f"{tool_id}:{text_file.path}".encode("utf-8")).hexdigest()[:16]
        virtual_path = f"{virtual_root}/{input_hash}.jsonl"
        data = "\n".join(json.dumps(record, ensure_ascii=True) for record in records) + "\n"
        artifact = write_artifact_bytes(
            settings=settings,
            store=store,
            workspace_id=workspace_id,
            filename=f"{filename_prefix}_{input_hash}.jsonl",
            data=data.encode("utf-8"),
            content_type="application/x-ndjson",
            schema_name=schema_name,
            preview={
                "path": virtual_path,
                "toolIds": [tool_id],
                "recordCount": len(records),
            },
        )
        entry = {
            "path": virtual_path,
            "inputKind": input_kind,
            "scope": "file",
            "toolIds": [tool_id],
            "sourceFiles": [text_file.path],
            "recordCount": len(records),
            "artifactId": artifact["id"],
            "artifactRelativePath": artifact["relative_path"],
        }
        results.append(MaterializedToolInput(entry=entry, artifact=artifact))
    return results


INFLUXQL_QUERY_KEYS = ("query", "sql", "stmt", "statement")


def extract_influxql_query(line: str) -> str | None:
    stripped = line.strip()
    if not stripped:
        return None
    try:
        value = json.loads(stripped)
    except Exception:
        value = None
    if isinstance(value, dict):
        for key in INFLUXQL_QUERY_KEYS:
            query = value.get(key)
            if isinstance(query, str):
                query = clean_query(query)
                if looks_like_influxql(query):
                    return query
    for key in INFLUXQL_QUERY_KEYS:
        query = extract_key_value(stripped, key)
        if query:
            query = clean_query(query)
            if looks_like_influxql(query):
                return query
    query = clean_query(stripped)
    if looks_like_influxql(query):
        return query
    return None


def extract_key_value(line: str, key: str) -> str | None:
    lower = line.lower()
    needle = f"{key}="
    start = lower.find(needle)
    if start < 0:
        return None
    rest = line[start + len(needle) :].lstrip()
    if not rest:
        return None
    first = rest[0]
    if first in {"'", '"'}:
        value = []
        escaped = False
        for char in rest[1:]:
            if escaped:
                value.append(char)
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == first:
                break
            else:
                value.append(char)
        return "".join(value)
    return rest.split(maxsplit=1)[0].strip(",")


def clean_query(value: str) -> str:
    return value.strip().strip('"').strip("'")


def extract_flux_query(line: str) -> str | None:
    stripped = line.strip()
    if not stripped:
        return None
    try:
        value = json.loads(stripped)
    except Exception:
        value = None
    if isinstance(value, dict):
        for key in ("flux", "fluxQuery", "query", "script", "statement"):
            query = value.get(key)
            if isinstance(query, str) and looks_like_flux(query):
                return query.strip()
    if looks_like_flux(stripped):
        return stripped
    return None


def looks_like_influxql(value: str) -> bool:
    lowered = value.strip().lower()
    return lowered.startswith(
        (
            "select ",
            "show ",
            "explain ",
            "delete ",
            "drop ",
            "create ",
            "alter ",
            "grant ",
            "revoke ",
        )
    )


def looks_like_flux(value: str) -> bool:
    lowered = value.strip().lower()
    return "|>" in lowered and re.search(r"\bfrom\s*\(", lowered) is not None


def safe_segment(value: str) -> str:
    return re.sub(r"[^A-Za-z0-9._-]+", "_", value).strip("._")[:80] or "unknown"


def search_keywords(
    question: str = "",
    configured_keywords: Iterable[str] | None = None,
) -> list[str]:
    del question
    keywords = [
        keyword.strip()
        for keyword in (configured_keywords or DEFAULT_GREP_KEYWORDS)
        if keyword.strip()
    ]
    keywords = list(dict.fromkeys(keywords))
    return keywords[:32]


def grep_text_files(
    text_files: Iterable[TextFile],
    keywords: list[str],
    max_matches: int,
    ref_base: str,
) -> JsonObject:
    lowered_keywords = [(keyword, keyword.lower()) for keyword in keywords]
    matches: list[JsonObject] = []
    keyword_counts = {keyword: 0 for keyword in keywords}
    matched_keywords: set[str] = set()
    for text_file in text_files:
        for line_number, line in enumerate(text_file.text.splitlines(), start=1):
            lowered_line = line.lower()
            for keyword, lowered_keyword in lowered_keywords:
                if lowered_keyword not in lowered_line:
                    continue
                index = len(matches)
                keyword_counts[keyword] += 1
                matched_keywords.add(lowered_keyword)
                ref = f"{ref_base}{index}"
                matches.append(
                    {
                        "index": index,
                        "ref": ref,
                        "evidenceRef": ref,
                        "path": text_file.path,
                        "file": text_file.path,
                        "sourceUploadId": text_file.source_upload_id,
                        "lineNumber": line_number,
                        "line": line_number,
                        "keyword": keyword,
                        "text": line[:2000],
                    }
                )
                break
            if len(matches) >= max_matches:
                break
        if len(matches) >= max_matches:
            break
    return {
        "schemaVersion": 1,
        "keywords": keywords,
        "keywordCounts": keyword_counts,
        "unmatchedKeywords": [
            keyword for keyword, lowered_keyword in lowered_keywords if lowered_keyword not in matched_keywords
        ],
        "totalMatches": len(matches),
        "truncated": len(matches) >= max_matches,
        "matches": matches,
    }


def build_manifest(
    settings: Settings,
    workspace_id: str,
    run_id: str,
    uploads: list[JsonObject],
    text_files: list[TextFile],
    tool_inputs_path: str | None = None,
    tool_input_count: int = 0,
) -> JsonObject:
    upload_summaries, file_metadata = build_upload_manifest_summaries(
        settings,
        uploads,
        text_files,
    )
    files = []
    for text_file in text_files:
        item = {
            "path": text_file.path,
            "size": text_file.size_bytes,
            "sourceUploadId": text_file.source_upload_id,
            "uploadId": text_file.source_upload_id,
            "sourceFilename": text_file.source_filename,
            "sizeBytes": text_file.size_bytes,
            "sha256": text_file.sha256,
        }
        if text_file.original_path:
            item["originalPath"] = text_file.original_path
        if text_file.log_group:
            item["logGroup"] = text_file.log_group
        if text_file.node_package:
            item["nodePackage"] = text_file.node_package
            item["instanceId"] = text_file.node_package.get("instanceId")
            item["nodeId"] = text_file.node_package.get("nodeId")
            item["packageTimestamp"] = text_file.node_package.get("timestamp")
        if metadata := file_metadata.get((text_file.source_upload_id, text_file.path)):
            item.update(metadata)
        files.append(item)
    first_upload = uploads[0] if uploads else None
    manifest = {
        "schemaVersion": 1,
        "workspaceId": workspace_id,
        "runId": run_id,
        "taskId": run_id,
        "uploadId": first_upload["id"] if first_upload else "",
        "uploadIds": [upload["id"] for upload in uploads],
        "source": "upload",
        "filename": first_upload["filename"] if first_upload else "session_text_input",
        "uploadCount": len(uploads),
        "fileCount": len(text_files),
        "uploads": upload_summaries,
        "files": files,
    }
    if tool_inputs_path:
        manifest["toolInputsPath"] = tool_inputs_path
        manifest["toolInputCount"] = tool_input_count
    return manifest


def build_upload_manifest_summaries(
    settings: Settings,
    uploads: list[JsonObject],
    text_files: list[TextFile],
) -> tuple[list[JsonObject], dict[tuple[str, str], JsonObject]]:
    package_files_by_upload: dict[str, list[TextFile]] = defaultdict(list)
    for text_file in text_files:
        if text_file.node_package:
            package_files_by_upload[text_file.source_upload_id].append(text_file)

    used_extracted_dirs: list[str] = []
    file_metadata: dict[tuple[str, str], JsonObject] = {}
    summaries: list[JsonObject] = []
    for upload in uploads:
        node_package = parse_node_log_package(upload["filename"])
        package_summary: JsonObject = {}
        if node_package is not None:
            extracted_dir = f"extracted/{node_package.node_id}/{node_package.timestamp}"
            package_summary, package_file_metadata = summarize_node_package_upload(
                settings,
                upload,
                node_package,
                package_files_by_upload.get(upload["id"], []),
            )
            file_metadata.update(package_file_metadata)
        else:
            extracted_dir = generic_extracted_dir(upload["filename"], used_extracted_dirs)
        summary = {
            "uploadId": upload["id"],
            "filename": upload["filename"],
            "artifactId": upload["artifact_id"],
            "size": upload["artifact_size_bytes"],
            "sizeBytes": upload["artifact_size_bytes"],
            "sha256": upload["artifact_sha256"],
            "rawPath": upload["artifact_relative_path"],
            "extractedDir": extracted_dir,
        }
        summary.update(package_summary)
        summaries.append(summary)
    return summaries, file_metadata


def summarize_node_package_upload(
    settings: Settings,
    upload: JsonObject,
    node_package: NodeLogPackage,
    text_files: list[TextFile],
) -> tuple[JsonObject, dict[tuple[str, str], JsonObject]]:
    log_groups: dict[str, JsonObject] = {}
    ignored_count = 0
    ignored_samples: list[str] = []
    warnings: list[str] = []
    file_metadata: dict[tuple[str, str], JsonObject] = {}
    artifact_path = resolve_artifact_path(settings, upload["artifact_relative_path"])
    try:
        with tarfile.open(fileobj=io.BytesIO(artifact_path.read_bytes()), mode="r:*") as archive:
            for index, member in enumerate(archive):
                if index >= settings.max_archive_files:
                    warnings.append("archive file count exceeds LOGAGENT_V2_MAX_ARCHIVE_FILES")
                    break
                if member.isdir():
                    continue
                if not member.isfile():
                    continue
                original_path = member.name.replace("\\", "/")
                path = safe_archive_path(member.name)
                classified = classify_node_log_member(path, node_package)
                if classified is None:
                    ignored_count += 1
                    if len(ignored_samples) < 20:
                        ignored_samples.append(original_path)
                    continue
                logical_path, log_group = classified
                group = log_groups.setdefault(
                    log_group,
                    {"name": log_group, "fileCount": 0, "compressedFileCount": 0},
                )
                group["fileCount"] += 1
                compressed = node_package_member_has_gzip_magic(archive, member)
                if compressed:
                    group["compressedFileCount"] += 1
                metadata: JsonObject = {"compressed": compressed}
                if compressed:
                    metadata["compression"] = "gzip"
                file_metadata[(upload["id"], logical_path)] = metadata
    except (OSError, tarfile.TarError, ValueError) as exc:
        warnings.append(f"failed to summarize node log package: {exc}")

    if not log_groups:
        log_groups = derive_log_group_summaries_from_text_files(text_files)
    summary = {
        "packageId": node_package.package_id,
        "instanceId": node_package.instance_id,
        "nodeId": node_package.node_id,
        "packageTimestamp": node_package.timestamp,
        "nodeDir": f"extracted/{node_package.node_id}/{node_package.timestamp}",
        "logGroups": [log_groups[name] for name in sorted(log_groups)],
    }
    if ignored_count:
        summary["ignoredFileCount"] = ignored_count
    if ignored_samples:
        summary["ignoredPathSamples"] = ignored_samples
    if warnings:
        summary["warnings"] = warnings[:50]
    return summary, file_metadata


def derive_log_group_summaries_from_text_files(text_files: list[TextFile]) -> dict[str, JsonObject]:
    groups: dict[str, JsonObject] = {}
    for text_file in text_files:
        if not text_file.log_group:
            continue
        group = groups.setdefault(
            text_file.log_group,
            {"name": text_file.log_group, "fileCount": 0, "compressedFileCount": 0},
        )
        group["fileCount"] += 1
    return groups


def node_package_member_has_gzip_magic(archive: tarfile.TarFile, member: tarfile.TarInfo) -> bool:
    extracted = archive.extractfile(member)
    if extracted is None:
        return False
    return extracted.read(2) == b"\x1f\x8b"


def generic_extracted_dir(filename: str, used: list[str]) -> str:
    base = upload_dir_name(filename)
    candidate = base
    index = 2
    while candidate in used:
        candidate = f"{base}_{index}"
        index += 1
    used.append(candidate)
    return f"extracted/{candidate}"


def direct_upload_logical_path(filename: str, path_prefix: str) -> str:
    name = posixpath.basename(filename.replace("\\", "/")) or "upload.bin"
    if name in {".", ".."}:
        name = "upload.bin"
    return f"{path_prefix}/{name}"


def upload_dir_name(filename: str) -> str:
    lower = filename.lower()
    suffixes = (".tar.gz", ".tgz", ".zip", ".tar", ".log", ".txt")
    without_suffix = filename
    for suffix in suffixes:
        if lower.endswith(suffix):
            without_suffix = filename[: -len(suffix)]
            break
    safe = "".join(
        char if char.isascii() and (char.isalnum() or char in "-_.") else "_"
        for char in without_suffix
    ).strip(".")
    return safe or "upload"


def write_json_artifact(
    settings: Settings,
    store: Store,
    workspace_id: str,
    filename: str,
    value: JsonObject,
    schema_name: str,
) -> JsonObject:
    encoded = json.dumps(value, ensure_ascii=True, indent=2).encode("utf-8")
    return write_artifact_bytes(
        settings=settings,
        store=store,
        workspace_id=workspace_id,
        filename=filename,
        data=encoded,
        content_type="application/json",
        schema_name=schema_name,
        preview={
            "filename": filename,
            "schemaName": schema_name,
            "sizeBytes": len(encoded),
        },
    )
