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

from .artifacts import resolve_artifact_path, write_artifact_bytes
from .config import Settings
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
BASE_KEYWORDS = ["error", "fail", "fatal", "panic", "exception", "timeout", "warn", "slow"]
BACKGROUND_EVIDENCE_KINDS = {"environment_evidence"}
SESSION_TEXT_INPUT_REF = "session_text_input.json#question"
NODE_LOG_PACKAGE_RE = re.compile(
    r"^(?P<package_id>[^_]+)_(?P<instance_id>[^_]+)_(?P<node_id>[^_]+)_"
    r"(?P<timestamp>[^_]+)_logs\.(?:tar\.gz|tgz)$",
    re.IGNORECASE,
)


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
    keywords = search_keywords(workspace["question"])
    tool_input_bundle = materialize_tool_inputs(
        settings,
        store,
        workspace_id,
        uploads,
        text_files,
    )
    manifest = build_manifest(
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
) -> JsonObject:
    uploads = store.list_uploads(workspace_id)
    text_files = collect_text_files(settings, uploads)
    search_id = new_id("logsearch")
    artifact_path = f"log_searches/{search_id}.json"
    results = grep_text_files(
        text_files,
        keywords,
        settings.max_grep_matches,
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
    uploads = store.list_uploads(workspace_id)
    text_files = collect_text_files(settings, uploads)
    selected = next((text_file for text_file in text_files if text_file.path == path), None)
    if selected is None:
        raise ValueError(f"log path {path!r} is not available in this workspace")
    lines = selected.text.splitlines()
    start = max(1, line_number - before)
    end = min(len(lines), line_number + after)
    slice_id = new_id("logslice")
    slice_path = f"log_slices/{slice_id}.json"
    result = {
        "schemaVersion": 1,
        "sliceId": slice_id,
        "path": path,
        "sourceUploadId": selected.source_upload_id,
        "lineNumber": line_number,
        "startLine": start,
        "endLine": end,
        "lines": [
            {
                "lineNumber": current,
                "text": lines[current - 1][:4000],
            }
            for current in range(start, end + 1)
        ],
        "ref": f"{slice_path}#lines",
    }
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


def collect_text_files(settings: Settings, uploads: list[JsonObject]) -> list[TextFile]:
    text_files: list[TextFile] = []
    total_archive_bytes = 0
    for upload in uploads:
        filename = upload["filename"]
        artifact_path = resolve_artifact_path(settings, upload["artifact_relative_path"])
        raw = artifact_path.read_bytes()
        if is_archive(filename):
            extracted, extracted_bytes = read_archive_text_files(settings, upload, raw)
            total_archive_bytes += extracted_bytes
            if total_archive_bytes > settings.max_archive_bytes:
                raise ValueError("archive extraction exceeds LOGAGENT_V2_MAX_ARCHIVE_BYTES")
            text_files.extend(extracted)
        elif is_text_path(filename):
            text_files.append(text_file_from_bytes(settings, upload, filename, raw))
    return text_files


def is_archive(path: str) -> bool:
    lowered = path.lower()
    return lowered.endswith(ARCHIVE_SUFFIXES)


def is_text_path(path: str) -> bool:
    lowered = path.lower()
    return any(lowered.endswith(suffix) for suffix in TEXT_SUFFIXES)


def read_archive_text_files(
    settings: Settings, upload: JsonObject, raw: bytes
) -> tuple[list[TextFile], int]:
    filename = upload["filename"].lower()
    if filename.endswith(".zip"):
        return read_zip_text_files(settings, upload, raw)
    if filename.endswith((".tar", ".tar.gz", ".tgz")):
        return read_tar_text_files(settings, upload, raw)
    return [], 0


def read_zip_text_files(
    settings: Settings, upload: JsonObject, raw: bytes
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
            if info.file_size > settings.max_text_file_bytes:
                continue
            data = archive.read(info, pwd=None)
            total_bytes += len(data)
            result.append(text_file_from_bytes(settings, upload, path, data))
    return result, total_bytes


def read_tar_text_files(
    settings: Settings, upload: JsonObject, raw: bytes
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
    match = NODE_LOG_PACKAGE_RE.match(filename)
    if match is None:
        return None
    return NodeLogPackage(
        package_id=match.group("package_id"),
        instance_id=match.group("instance_id"),
        node_id=match.group("node_id"),
        timestamp=match.group("timestamp"),
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
                    "lineNumber": line_number,
                    "nodeId": node_id,
                    "instanceId": instance_id or None,
                    "packageTimestamp": timestamp,
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
    results: list[MaterializedToolInput] = []
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
            results.append(
                materialize_archive_storage_input(
                    settings, store, workspace_id, upload, path, data, tool_ids
                )
            )
    return results


def materialize_tar_storage_inputs(
    settings: Settings,
    store: Store,
    workspace_id: str,
    upload: JsonObject,
    raw: bytes,
    active_tool_ids: set[str],
) -> list[MaterializedToolInput]:
    results: list[MaterializedToolInput] = []
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
            results.append(
                materialize_archive_storage_input(
                    settings, store, workspace_id, upload, path, data, tool_ids
                )
            )
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
    ):
        tool_ids.append("opengemini_storage_analyzer")
    if name.endswith(".tsm") or name.endswith(".tsi") or "_series" in parts or "_series" in lowered:
        tool_ids.append("influxdb_storage_analyzer")
    return tool_ids


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


def storage_virtual_path(prefix: str, source_path: str) -> str:
    digest = sha256(f"{prefix}:{source_path}".encode("utf-8")).hexdigest()[:16]
    suffix = safe_segment(PurePosixPath(source_path).name)
    return f"tool_inputs/storage/{digest}/{suffix}"


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


def extract_influxql_query(line: str) -> str | None:
    stripped = line.strip()
    if not stripped:
        return None
    try:
        value = json.loads(stripped)
    except Exception:
        value = None
    if isinstance(value, dict):
        for key in ("query", "sql", "statement"):
            query = value.get(key)
            if isinstance(query, str) and looks_like_influxql(query):
                return query.strip()
    if looks_like_influxql(stripped):
        return stripped
    return None


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
        )
    )


def looks_like_flux(value: str) -> bool:
    lowered = value.strip().lower()
    return "|>" in lowered and re.search(r"\bfrom\s*\(", lowered) is not None


def safe_segment(value: str) -> str:
    return re.sub(r"[^A-Za-z0-9._-]+", "_", value).strip("._")[:80] or "unknown"


def search_keywords(question: str) -> list[str]:
    tokens = [
        token.lower()
        for token in re.findall(r"[A-Za-z0-9_./:-]{3,}", question)
        if not token.isdigit()
    ]
    keywords = list(dict.fromkeys(BASE_KEYWORDS + tokens))
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
    for text_file in text_files:
        for line_number, line in enumerate(text_file.text.splitlines(), start=1):
            lowered_line = line.lower()
            for keyword, lowered_keyword in lowered_keywords:
                if lowered_keyword not in lowered_line:
                    continue
                index = len(matches)
                keyword_counts[keyword] += 1
                matches.append(
                    {
                        "index": index,
                        "ref": f"{ref_base}{index}",
                        "path": text_file.path,
                        "sourceUploadId": text_file.source_upload_id,
                        "lineNumber": line_number,
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
        "totalMatches": len(matches),
        "truncated": len(matches) >= max_matches,
        "matches": matches,
    }


def build_manifest(
    workspace_id: str,
    run_id: str,
    uploads: list[JsonObject],
    text_files: list[TextFile],
    tool_inputs_path: str | None = None,
    tool_input_count: int = 0,
) -> JsonObject:
    files = []
    for text_file in text_files:
        item = {
            "path": text_file.path,
            "sourceUploadId": text_file.source_upload_id,
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
        files.append(item)
    manifest = {
        "schemaVersion": 1,
        "workspaceId": workspace_id,
        "runId": run_id,
        "uploadCount": len(uploads),
        "fileCount": len(text_files),
        "uploads": [
            {
                "uploadId": upload["id"],
                "filename": upload["filename"],
                "artifactId": upload["artifact_id"],
                "sizeBytes": upload["artifact_size_bytes"],
                "sha256": upload["artifact_sha256"],
            }
            for upload in uploads
        ],
        "files": files,
    }
    if tool_inputs_path:
        manifest["toolInputsPath"] = tool_inputs_path
        manifest["toolInputCount"] = tool_input_count
    return manifest


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
