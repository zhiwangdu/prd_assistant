from __future__ import annotations

import os
import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any


@dataclass(frozen=True)
class ToolDefinition:
    id: str
    display_name: str
    command: str
    args: tuple[str, ...] = ()
    enabled: bool = True
    timeout_seconds: int = 30
    max_output_bytes: int = 1024 * 1024
    max_input_files: int = 1
    match_file_patterns: tuple[str, ...] = ()
    match_keywords: tuple[str, ...] = ()

    @classmethod
    def from_json(cls, value: dict) -> "ToolDefinition":
        tool_id = str(value["id"])
        match = value.get("match") if isinstance(value.get("match"), dict) else {}
        return cls(
            id=tool_id,
            display_name=str(value.get("displayName") or value.get("display_name") or tool_id),
            command=str(value["command"]),
            args=tuple(str(arg) for arg in value.get("args", [])),
            enabled=bool(value.get("enabled", True)),
            timeout_seconds=max(1, int(value.get("timeoutSeconds", 30))),
            max_output_bytes=max(1024, int(value.get("maxOutputBytes", 1024 * 1024))),
            max_input_files=max(1, int(value.get("maxInputFiles", 1))),
            match_file_patterns=tuple(
                strings_from_list(match.get("filePatterns") or match.get("file_patterns"))
            ),
            match_keywords=tuple(strings_from_list(match.get("keywords"))),
        )


@dataclass(frozen=True)
class Settings:
    data_dir: Path
    api_key: str
    host: str = "127.0.0.1"
    port: int = 50993
    max_upload_bytes: int = 512 * 1024 * 1024
    max_archive_files: int = 2000
    max_archive_bytes: int = 256 * 1024 * 1024
    max_text_file_bytes: int = 16 * 1024 * 1024
    max_grep_matches: int = 500
    max_concurrent_jobs: int = 2
    job_poll_seconds: float = 1.0
    inline_worker: bool = True
    tools: tuple[ToolDefinition, ...] = ()
    fetch_enabled: bool = False
    fetch_allowed_hosts: tuple[str, ...] = ()
    fetch_timeout_seconds: int = 20
    fetch_max_response_bytes: int = 1024 * 1024
    fetch_max_redirects: int = 5

    @property
    def sqlite_path(self) -> Path:
        return self.data_dir / "logagent.sqlite"

    @property
    def artifacts_dir(self) -> Path:
        return self.data_dir / "artifacts"

    @property
    def tmp_dir(self) -> Path:
        return self.data_dir / "tmp"

    @property
    def skills_dir(self) -> Path:
        return self.data_dir / "skills"

    def ensure_dirs(self) -> None:
        self.data_dir.mkdir(parents=True, exist_ok=True)
        self.artifacts_dir.mkdir(parents=True, exist_ok=True)
        self.tmp_dir.mkdir(parents=True, exist_ok=True)
        self.skills_dir.mkdir(parents=True, exist_ok=True)

    @classmethod
    def from_env(cls) -> "Settings":
        data_dir = Path(os.environ.get("LOGAGENT_V2_DATA_DIR", "/tmp/logagent-v2")).expanduser()
        api_key = os.environ.get("LOGAGENT_V2_API_KEY", "dev-token")
        host = os.environ.get("LOGAGENT_V2_HOST", "127.0.0.1")
        port = int(os.environ.get("LOGAGENT_V2_PORT", "50993"))
        max_upload_bytes = int(
            os.environ.get("LOGAGENT_V2_MAX_UPLOAD_BYTES", str(512 * 1024 * 1024))
        )
        max_archive_files = int(os.environ.get("LOGAGENT_V2_MAX_ARCHIVE_FILES", "2000"))
        max_archive_bytes = int(
            os.environ.get("LOGAGENT_V2_MAX_ARCHIVE_BYTES", str(256 * 1024 * 1024))
        )
        max_text_file_bytes = int(
            os.environ.get("LOGAGENT_V2_MAX_TEXT_FILE_BYTES", str(16 * 1024 * 1024))
        )
        max_grep_matches = int(os.environ.get("LOGAGENT_V2_MAX_GREP_MATCHES", "500"))
        max_concurrent_jobs = int(os.environ.get("LOGAGENT_V2_MAX_CONCURRENT_JOBS", "2"))
        inline_worker = os.environ.get("LOGAGENT_V2_INLINE_WORKER", "1") != "0"
        tools = parse_tools_env(os.environ.get("LOGAGENT_V2_TOOLS_JSON"))
        fetch_enabled = os.environ.get("LOGAGENT_V2_FETCH_ENABLED", "0") == "1"
        fetch_allowed_hosts = tuple(
            item.strip().lower()
            for item in os.environ.get("LOGAGENT_V2_FETCH_ALLOWED_HOSTS", "").split(",")
            if item.strip()
        )
        fetch_timeout_seconds = int(os.environ.get("LOGAGENT_V2_FETCH_TIMEOUT_SECONDS", "20"))
        fetch_max_response_bytes = int(
            os.environ.get("LOGAGENT_V2_FETCH_MAX_RESPONSE_BYTES", str(1024 * 1024))
        )
        fetch_max_redirects = int(os.environ.get("LOGAGENT_V2_FETCH_MAX_REDIRECTS", "5"))
        return cls(
            data_dir=data_dir,
            api_key=api_key,
            host=host,
            port=port,
            max_upload_bytes=max_upload_bytes,
            max_archive_files=max_archive_files,
            max_archive_bytes=max_archive_bytes,
            max_text_file_bytes=max_text_file_bytes,
            max_grep_matches=max_grep_matches,
            max_concurrent_jobs=max_concurrent_jobs,
            inline_worker=inline_worker,
            tools=tools,
            fetch_enabled=fetch_enabled,
            fetch_allowed_hosts=fetch_allowed_hosts,
            fetch_timeout_seconds=fetch_timeout_seconds,
            fetch_max_response_bytes=fetch_max_response_bytes,
            fetch_max_redirects=max(0, fetch_max_redirects),
        )


def parse_tools_env(raw: str | None) -> tuple[ToolDefinition, ...]:
    if not raw:
        return ()
    decoded = json.loads(raw)
    if not isinstance(decoded, list):
        raise ValueError("LOGAGENT_V2_TOOLS_JSON must be a JSON array")
    return tuple(ToolDefinition.from_json(item) for item in decoded)


def strings_from_list(value: Any) -> list[str]:
    if not isinstance(value, list):
        return []
    return [str(item) for item in value if str(item).strip()]
