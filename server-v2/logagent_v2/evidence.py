from __future__ import annotations

import gzip
import io
import json
import posixpath
import re
import stat
import tarfile
import zipfile
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
    manifest = build_manifest(workspace_id, run_id, uploads, text_files)
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
    return {
        "manifest": manifest,
        "grepResults": grep_results,
        "manifestArtifact": manifest_artifact,
        "grepArtifact": grep_artifact,
    }


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
    workspace_id: str, run_id: str, uploads: list[JsonObject], text_files: list[TextFile]
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
    return {
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
