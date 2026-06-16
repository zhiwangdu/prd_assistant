from __future__ import annotations

import os
from dataclasses import dataclass
from pathlib import Path


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

    @property
    def sqlite_path(self) -> Path:
        return self.data_dir / "logagent.sqlite"

    @property
    def artifacts_dir(self) -> Path:
        return self.data_dir / "artifacts"

    @property
    def tmp_dir(self) -> Path:
        return self.data_dir / "tmp"

    def ensure_dirs(self) -> None:
        self.data_dir.mkdir(parents=True, exist_ok=True)
        self.artifacts_dir.mkdir(parents=True, exist_ok=True)
        self.tmp_dir.mkdir(parents=True, exist_ok=True)

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
        )
