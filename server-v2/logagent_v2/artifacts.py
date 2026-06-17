from __future__ import annotations

import hashlib
import re
from pathlib import Path
from pathlib import PurePosixPath

from .config import Settings
from .ids import new_id
from .store import JsonObject, Store


SAFE_FILENAME_RE = re.compile(r"[^A-Za-z0-9._-]+")


def safe_filename(filename: str) -> str:
    name = Path(filename).name or "upload.bin"
    name = SAFE_FILENAME_RE.sub("_", name)
    return name[:180] or "upload.bin"


def write_artifact_bytes(
    settings: Settings,
    store: Store,
    workspace_id: str,
    filename: str,
    data: bytes,
    content_type: str,
    schema_name: str | None = None,
    preview: JsonObject | None = None,
) -> JsonObject:
    artifact_id = new_id("artfile")
    clean_name = safe_filename(filename)
    relative_path = Path("artifacts") / workspace_id / artifact_id / clean_name
    absolute_path = settings.data_dir / relative_path
    absolute_path.parent.mkdir(parents=True, exist_ok=True)
    absolute_path.write_bytes(data)
    digest = hashlib.sha256(data).hexdigest()
    return store.create_artifact(
        workspace_id=workspace_id,
        relative_path=relative_path.as_posix(),
        sha256=digest,
        size_bytes=len(data),
        content_type=content_type,
        schema_name=schema_name,
        preview=preview or {"filename": clean_name},
    )


def write_artifact_file(
    settings: Settings,
    store: Store,
    workspace_id: str,
    filename: str,
    source_path: Path,
    content_type: str,
    schema_name: str | None = None,
    preview: JsonObject | None = None,
) -> JsonObject:
    artifact_id = new_id("artfile")
    clean_name = safe_filename(filename)
    relative_path = Path("artifacts") / workspace_id / artifact_id / clean_name
    absolute_path = settings.data_dir / relative_path
    absolute_path.parent.mkdir(parents=True, exist_ok=True)
    digest = hashlib.sha256()
    size_bytes = 0
    with source_path.open("rb") as source:
        with absolute_path.open("wb") as target:
            while True:
                chunk = source.read(1024 * 1024)
                if not chunk:
                    break
                digest.update(chunk)
                size_bytes += len(chunk)
                target.write(chunk)
    return store.create_artifact(
        workspace_id=workspace_id,
        relative_path=relative_path.as_posix(),
        sha256=digest.hexdigest(),
        size_bytes=size_bytes,
        content_type=content_type,
        schema_name=schema_name,
        preview=preview or {"filename": clean_name, "sizeBytes": size_bytes},
    )


def write_artifact_directory(
    settings: Settings,
    store: Store,
    workspace_id: str,
    dirname: str,
    files: list[tuple[str, bytes]],
    content_type: str = "inode/directory",
    schema_name: str | None = None,
    preview: JsonObject | None = None,
) -> JsonObject:
    artifact_id = new_id("artfile")
    clean_name = safe_filename(dirname)
    relative_path = Path("artifacts") / workspace_id / artifact_id / clean_name
    absolute_path = settings.data_dir / relative_path
    absolute_path.mkdir(parents=True, exist_ok=True)
    digest = hashlib.sha256()
    size_bytes = 0
    for relative_file, data in sorted(files, key=lambda item: item[0]):
        safe_relative = safe_relative_artifact_path(relative_file)
        safe_relative_text = safe_relative.as_posix()
        digest.update(safe_relative_text.encode("utf-8"))
        digest.update(b"\0")
        digest.update(data)
        size_bytes += len(data)
        target = absolute_path / safe_relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
    return store.create_artifact(
        workspace_id=workspace_id,
        relative_path=relative_path.as_posix(),
        sha256=digest.hexdigest(),
        size_bytes=size_bytes,
        content_type=content_type,
        schema_name=schema_name,
        preview=preview or {"dirname": clean_name, "fileCount": len(files)},
    )


def safe_relative_artifact_path(path: str) -> Path:
    normalized = path.strip().replace("\\", "/")
    pure = PurePosixPath(normalized)
    if (
        not normalized
        or normalized == "."
        or pure.is_absolute()
        or any(part in {"", ".", ".."} for part in pure.parts)
    ):
        raise ValueError(f"unsafe artifact directory member path {path!r}")
    return Path(*pure.parts)


def resolve_artifact_path(settings: Settings, relative_path: str) -> Path:
    root = settings.data_dir.resolve()
    target = (settings.data_dir / relative_path).resolve()
    if root != target and root not in target.parents:
        raise ValueError("artifact path escapes data_dir")
    return target
