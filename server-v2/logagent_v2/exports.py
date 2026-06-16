from __future__ import annotations

import io
import json
import os
import zipfile
from datetime import UTC, datetime
from pathlib import Path, PurePosixPath

from .config import Settings
from .skills import get_skill, list_skills
from .store import JsonObject


def build_skills_zip(settings: Settings) -> bytes:
    manifest: JsonObject = {
        "schemaVersion": 1,
        "generatedAt": datetime.now(UTC).replace(microsecond=0).isoformat(),
        "skills": [],
    }
    buffer = io.BytesIO()
    used_paths: set[str] = set()
    with zipfile.ZipFile(buffer, mode="w", compression=zipfile.ZIP_DEFLATED) as archive:
        for skill in list_skills(settings):
            skill_id = skill["skillId"]
            skill_dir = settings.skills_dir / skill_id
            files = []
            for path in iter_regular_skill_files(skill_dir):
                relative_path = path.relative_to(skill_dir).as_posix()
                zip_path = safe_zip_path(f"{skill_id}/{relative_path}")
                if zip_path in used_paths:
                    raise ValueError(f"duplicate skill export path {zip_path}")
                used_paths.add(zip_path)
                archive.writestr(zip_path, path.read_bytes())
                files.append(
                    {
                        "path": relative_path,
                        "zipPath": zip_path,
                        "size": path.stat().st_size,
                    }
                )
            detail = get_skill(settings, skill_id, include_content=False)
            manifest["skills"].append(
                {
                    "skillId": skill_id,
                    "displayName": detail["displayName"],
                    "revision": detail["revision"],
                    "sourceRoot": skill_dir.as_posix(),
                    "sourcePath": (skill_dir / "SKILL.md").as_posix(),
                    "files": files,
                }
            )
        archive.writestr(
            "manifest.json",
            json.dumps(manifest, ensure_ascii=True, indent=2).encode("utf-8"),
        )
    return buffer.getvalue()


def iter_regular_skill_files(skill_dir: Path) -> list[Path]:
    files: list[Path] = []
    for root, dirs, names in os.walk(skill_dir, followlinks=False):
        root_path = Path(root)
        dirs[:] = [
            name
            for name in dirs
            if not (root_path / name).is_symlink()
        ]
        for name in names:
            path = root_path / name
            if path.is_symlink() or not path.is_file():
                continue
            validate_relative_path(path.relative_to(skill_dir).as_posix())
            files.append(path)
    return sorted(files, key=lambda path: path.relative_to(skill_dir).as_posix())


def safe_zip_path(path: str) -> str:
    validate_relative_path(path)
    return path


def validate_relative_path(path: str) -> None:
    pure = PurePosixPath(path)
    if not path or pure.is_absolute() or ".." in pure.parts:
        raise ValueError(f"unsafe zip path {path!r}")
