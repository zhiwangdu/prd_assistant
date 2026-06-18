from __future__ import annotations

import io
import json
import os
import platform
import zipfile
from datetime import UTC, datetime
from hashlib import sha256
from pathlib import Path, PurePosixPath

from .config import Settings, ToolDefinition
from .skills import get_skill, list_skills
from .store import JsonObject
from .tools import PPROF_ANALYZER_ID, resolve_pprof_go_command


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


def build_tools_zip(settings: Settings) -> bytes:
    server_os = platform.system().lower() or os.name
    server_arch = platform.machine() or "unknown"
    manifest: JsonObject = {
        "schemaVersion": 1,
        "generatedAt": datetime.now(UTC).replace(microsecond=0).isoformat(),
        "serverOs": server_os,
        "serverArch": server_arch,
        "tools": [],
    }
    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, mode="w", compression=zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("README.md", tools_package_readme())
        for tool in exportable_tool_definitions(settings):
            if not tool.enabled:
                continue
            entry = tool_manifest_entry(tool, server_os, server_arch)
            try:
                package_tool(archive, tool, entry)
            except ValueError as error:
                entry["skipReason"] = str(error)
            archive.writestr(
                f"config/examples/{safe_zip_segment(tool.id)}.yaml",
                tool_config_example(tool, entry.get("binaryFilename")),
            )
            manifest["tools"].append(entry)
        archive.writestr(
            "tools-manifest.json",
            json.dumps(manifest, ensure_ascii=True, indent=2).encode("utf-8"),
        )
    return buffer.getvalue()


def exportable_tool_definitions(settings: Settings) -> list[ToolDefinition]:
    tools = [tool for tool in settings.tools if tool.enabled]
    pprof_command = resolve_pprof_go_command(settings) if settings.pprof_enabled else None
    if pprof_command:
        tools.append(
            ToolDefinition(
                id=PPROF_ANALYZER_ID,
                display_name="Golang pprof Analyzer",
                command=pprof_command,
                args=(),
                enabled=True,
                timeout_seconds=60,
                max_output_bytes=settings.remote_max_output_bytes,
                max_input_files=1,
                match_file_patterns=("*.pprof", "*.prof", "*.profile", "*.pb.gz"),
                match_keywords=(),
            )
        )
    return tools


def tool_manifest_entry(tool: ToolDefinition, server_os: str, server_arch: str) -> JsonObject:
    return {
        "toolId": tool.id,
        "displayName": tool.display_name,
        "configuredArgs": list(tool.args),
        "matchRules": {
            "filePatterns": list(tool.match_file_patterns),
            "keywords": list(tool.match_keywords),
        },
        "serverOs": server_os,
        "serverArch": server_arch,
        "binaryFilename": None,
        "sha256": None,
        "size": None,
        "packaged": False,
        "skipped": True,
        "skipReason": None,
    }


def package_tool(archive: zipfile.ZipFile, tool: ToolDefinition, entry: JsonObject) -> None:
    tool_id = safe_zip_segment(tool.id)
    command = Path(tool.command)
    if not command.is_absolute():
        raise ValueError("tool command is not an absolute path")
    resolved = command.resolve()
    if not resolved.is_file():
        raise ValueError("resolved path is not a regular file")
    if not os.access(resolved, os.X_OK):
        raise ValueError("resolved path is not executable")
    binary_filename = safe_zip_segment(resolved.name)
    data = resolved.read_bytes()
    write_zip_bytes(archive, f"bin/{tool_id}/{binary_filename}", data, mode=0o755)
    write_zip_text(
        archive,
        f"wrappers/{tool_id}.sh",
        tool_wrapper(tool_id, binary_filename),
        mode=0o755,
    )
    entry["binaryFilename"] = binary_filename
    entry["sha256"] = sha256(data).hexdigest()
    entry["size"] = len(data)
    entry["packaged"] = True
    entry["skipped"] = False
    entry["skipReason"] = None


def tool_wrapper(tool_id: str, binary_filename: str) -> str:
    return (
        "#!/usr/bin/env sh\n"
        "set -eu\n"
        'DIR="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"\n'
        f'exec "$DIR/bin/{tool_id}/{binary_filename}" "$@"\n'
    )


def tool_config_example(tool: ToolDefinition, binary_filename: object) -> str:
    if tool.id == PPROF_ANALYZER_ID:
        return pprof_config_example(binary_filename)
    packaged_path = (
        f"/absolute/path/to/extracted/tools/bin/{safe_zip_segment(tool.id)}/{binary_filename}"
        if isinstance(binary_filename, str)
        else "<absolute path to executable>"
    )
    return (
        "# Example LOGAGENT_V2_TOOLS_JSON entry.\n"
        "- id: " + json.dumps(tool.id) + "\n"
        "  displayName: " + json.dumps(tool.display_name) + "\n"
        "  command: " + json.dumps(packaged_path) + "\n"
        "  args: " + json.dumps(list(tool.args), ensure_ascii=True) + "\n"
        f"  enabled: {str(tool.enabled).lower()}\n"
        f"  timeoutSeconds: {tool.timeout_seconds}\n"
        f"  maxOutputBytes: {tool.max_output_bytes}\n"
        f"  maxInputFiles: {tool.max_input_files}\n"
        "  match:\n"
        "    filePatterns: " + json.dumps(list(tool.match_file_patterns)) + "\n"
        "    keywords: " + json.dumps(list(tool.match_keywords)) + "\n"
    )


def pprof_config_example(binary_filename: object) -> str:
    packaged_path = (
        f"/absolute/path/to/extracted/tools/bin/{safe_zip_segment(PPROF_ANALYZER_ID)}/"
        f"{binary_filename}"
        if isinstance(binary_filename, str)
        else "<absolute path to go executable>"
    )
    return (
        "# pprof_analyzer is the V1-style pprof adapter.\n"
        "# Configure it with dedicated environment variables, not as a generic\n"
        "# LOGAGENT_V2_TOOLS_JSON subprocess entry. The Server invokes this command\n"
        "# as: $LOGAGENT_V2_PPROF_GO_COMMAND tool pprof ...\n"
        "pprof_analyzer:\n"
        "  env:\n"
        '    LOGAGENT_V2_PPROF_ENABLED: "1"\n'
        "    LOGAGENT_V2_PPROF_GO_COMMAND: "
        + json.dumps(packaged_path, ensure_ascii=True)
        + "\n"
    )


def tools_package_readme() -> str:
    return (
        "# LogAgent V2 Tools Package\n\n"
        "This archive contains executable snapshots for enabled configured "
        "subprocess tools and the enabled pprof_analyzer Go command.\n\n"
        "- Binaries are under `bin/<tool_id>/`.\n"
        "- Shell wrappers are under `wrappers/` for packaged executable tools.\n"
        "- `tools-manifest.json` records configured args, match rules, sha256, "
        "size, and skipped tools.\n"
        "- `config/examples/` contains LOGAGENT_V2_TOOLS_JSON snippets for "
        "generic subprocess tools; pprof_analyzer uses the dedicated "
        "LOGAGENT_V2_PPROF_GO_COMMAND environment variable.\n"
        "- Built-in tools without standalone executables, such as Fetch, "
        "Metadata, preprocess, and Huawei package sync, are omitted.\n"
        "- No API keys, environment variable values, server config files, "
        "uploads, or workspaces are included.\n"
    )


def write_zip_text(
    archive: zipfile.ZipFile,
    path: str,
    content: str,
    mode: int = 0o644,
) -> None:
    write_zip_bytes(archive, path, content.encode("utf-8"), mode)


def write_zip_bytes(
    archive: zipfile.ZipFile,
    path: str,
    data: bytes,
    mode: int = 0o644,
) -> None:
    info = zipfile.ZipInfo(safe_zip_path(path))
    info.create_system = 3
    info.compress_type = zipfile.ZIP_DEFLATED
    info.external_attr = (mode & 0o777) << 16
    archive.writestr(info, data)


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


def safe_zip_segment(value: str) -> str:
    if not value or "/" in value or "\\" in value or value in {".", ".."}:
        raise ValueError(f"unsafe zip path segment {value!r}")
    validate_relative_path(value)
    return value
