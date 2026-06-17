from __future__ import annotations

import os
import json
from dataclasses import dataclass, field
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
    params_schema: dict[str, Any] | None = None

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
            params_schema=normalize_params_schema(
                value.get("paramsSchema") or value.get("params_schema")
            ),
        )


@dataclass(frozen=True)
class RemoteCommandTemplate:
    command_id: str
    display_name: str
    description: str
    argv: tuple[str, ...]
    enabled: bool = True
    timeout_seconds: int | None = None

    @classmethod
    def from_json(cls, value: dict) -> "RemoteCommandTemplate":
        command_id = str(value.get("commandId") or value.get("command_id") or value["id"])
        argv = tuple(str(arg) for arg in value.get("argv", []))
        if not argv:
            raise ValueError("remote command template argv must not be empty")
        return cls(
            command_id=command_id,
            display_name=str(value.get("displayName") or value.get("display_name") or command_id),
            description=str(value.get("description") or ""),
            argv=argv,
            enabled=bool(value.get("enabled", True)),
            timeout_seconds=(
                max(1, int(value["timeoutSeconds"]))
                if value.get("timeoutSeconds") is not None
                else None
            ),
        )


def default_remote_commands() -> tuple[RemoteCommandTemplate, ...]:
    return (
        RemoteCommandTemplate(
            command_id="smoke_ls_root",
            display_name="Smoke: list /root",
            description="Low-risk SSH smoke command for managed executors.",
            argv=("ls", "-la", "/root"),
        ),
    )


@dataclass(frozen=True)
class HuaweiPackageSyncSettings:
    enabled: bool = False
    obs_endpoint: str | None = None
    obs_bucket: str | None = None
    obs_object_prefix: str = ""
    obs_access_key: str | None = None
    obs_secret_key: str | None = None
    obs_security_token: str | None = None
    gaussdb_dsn: str | None = None
    timeout_seconds: int = 30


def default_webui_dir() -> Path:
    return Path(__file__).resolve().parents[2] / "webui" / "out"


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
    pprof_go_command: str | None = None
    pprof_enabled: bool = False
    huawei_package_sync: HuaweiPackageSyncSettings = field(
        default_factory=HuaweiPackageSyncSettings
    )
    fetch_enabled: bool = False
    fetch_allowed_hosts: tuple[str, ...] = ()
    fetch_timeout_seconds: int = 20
    fetch_max_request_bytes: int = 1024 * 1024
    fetch_max_response_bytes: int = 1024 * 1024
    fetch_max_redirects: int = 5
    fetch_secret_key: str | None = None
    agent_provider: str = "stub"
    agent_model: str | None = None
    agent_base_url: str | None = None
    agent_api_key: str | None = None
    agent_binary_path: Path | None = None
    agent_binary_max_output_bytes: int = 1024 * 1024
    agent_timeout_seconds: int = 60
    agent_max_rounds: int = 3
    agent_max_output_tokens: int = 2048
    remote_execution_enabled: bool = True
    remote_ssh_command: str = "ssh"
    remote_connect_timeout_seconds: int = 10
    remote_command_timeout_seconds: int = 30
    remote_max_output_bytes: int = 1024 * 1024
    remote_host_key_policy: str = "accept-new"
    remote_commands: tuple[RemoteCommandTemplate, ...] = field(default_factory=default_remote_commands)
    webui_dir: Path = field(default_factory=default_webui_dir)

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
        pprof_go_command = (
            os.environ.get("LOGAGENT_V2_PPROF_GO_COMMAND")
            or os.environ.get("LOGAGENT_TOOL_PPROF_GO")
        )
        pprof_enabled = env_bool(
            "LOGAGENT_V2_PPROF_ENABLED",
            default=bool(pprof_go_command),
        )
        huawei_package_sync = HuaweiPackageSyncSettings(
            enabled=env_bool("LOGAGENT_V2_HUAWEI_PACKAGE_SYNC_ENABLED", default=False),
            obs_endpoint=os.environ.get("LOGAGENT_V2_HUAWEI_OBS_ENDPOINT"),
            obs_bucket=os.environ.get("LOGAGENT_V2_HUAWEI_OBS_BUCKET"),
            obs_object_prefix=os.environ.get("LOGAGENT_V2_HUAWEI_OBS_OBJECT_PREFIX", ""),
            obs_access_key=os.environ.get("LOGAGENT_V2_HUAWEI_OBS_ACCESS_KEY"),
            obs_secret_key=os.environ.get("LOGAGENT_V2_HUAWEI_OBS_SECRET_KEY"),
            obs_security_token=os.environ.get("LOGAGENT_V2_HUAWEI_OBS_SECURITY_TOKEN"),
            gaussdb_dsn=os.environ.get("LOGAGENT_V2_HUAWEI_GAUSSDB_DSN"),
            timeout_seconds=max(
                1, int(os.environ.get("LOGAGENT_V2_HUAWEI_TIMEOUT_SECONDS", "30"))
            ),
        )
        fetch_enabled = os.environ.get("LOGAGENT_V2_FETCH_ENABLED", "0") == "1"
        fetch_allowed_hosts = tuple(
            item.strip().lower()
            for item in os.environ.get("LOGAGENT_V2_FETCH_ALLOWED_HOSTS", "").split(",")
            if item.strip()
        )
        fetch_timeout_seconds = int(os.environ.get("LOGAGENT_V2_FETCH_TIMEOUT_SECONDS", "20"))
        fetch_max_request_bytes = int(
            os.environ.get("LOGAGENT_V2_FETCH_MAX_REQUEST_BYTES", str(1024 * 1024))
        )
        fetch_max_response_bytes = int(
            os.environ.get("LOGAGENT_V2_FETCH_MAX_RESPONSE_BYTES", str(1024 * 1024))
        )
        fetch_max_redirects = int(os.environ.get("LOGAGENT_V2_FETCH_MAX_REDIRECTS", "5"))
        fetch_secret_key = os.environ.get("LOGAGENT_V2_FETCH_SECRET_KEY")
        agent_provider = os.environ.get("LOGAGENT_V2_AGENT_PROVIDER", "stub")
        agent_model = os.environ.get("LOGAGENT_V2_AGENT_MODEL")
        agent_base_url = os.environ.get("LOGAGENT_V2_AGENT_BASE_URL")
        agent_api_key = os.environ.get("LOGAGENT_V2_AGENT_API_KEY")
        raw_agent_binary_path = os.environ.get("LOGAGENT_V2_AGENT_BINARY_PATH")
        agent_binary_path = (
            Path(raw_agent_binary_path).expanduser() if raw_agent_binary_path else None
        )
        agent_binary_max_output_bytes = int(
            os.environ.get(
                "LOGAGENT_V2_AGENT_BINARY_MAX_OUTPUT_BYTES",
                str(1024 * 1024),
            )
        )
        agent_timeout_seconds = int(os.environ.get("LOGAGENT_V2_AGENT_TIMEOUT_SECONDS", "60"))
        agent_max_rounds = int(os.environ.get("LOGAGENT_V2_AGENT_MAX_ROUNDS", "3"))
        agent_max_output_tokens = int(
            os.environ.get("LOGAGENT_V2_AGENT_MAX_OUTPUT_TOKENS", "2048")
        )
        remote_execution_enabled = os.environ.get("LOGAGENT_V2_REMOTE_EXECUTION_ENABLED", "1") != "0"
        remote_ssh_command = os.environ.get("LOGAGENT_V2_REMOTE_SSH_COMMAND", "ssh")
        remote_connect_timeout_seconds = int(
            os.environ.get("LOGAGENT_V2_REMOTE_CONNECT_TIMEOUT_SECONDS", "10")
        )
        remote_command_timeout_seconds = int(
            os.environ.get("LOGAGENT_V2_REMOTE_COMMAND_TIMEOUT_SECONDS", "30")
        )
        remote_max_output_bytes = int(
            os.environ.get("LOGAGENT_V2_REMOTE_MAX_OUTPUT_BYTES", str(1024 * 1024))
        )
        remote_host_key_policy = os.environ.get(
            "LOGAGENT_V2_REMOTE_HOST_KEY_POLICY", "accept-new"
        )
        remote_commands = parse_remote_commands_env(
            os.environ.get("LOGAGENT_V2_REMOTE_COMMANDS_JSON")
        )
        raw_webui_dir = os.environ.get("LOGAGENT_V2_WEBUI_DIR")
        webui_dir = Path(raw_webui_dir).expanduser() if raw_webui_dir else default_webui_dir()
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
            pprof_go_command=pprof_go_command,
            pprof_enabled=pprof_enabled,
            huawei_package_sync=huawei_package_sync,
            fetch_enabled=fetch_enabled,
            fetch_allowed_hosts=fetch_allowed_hosts,
            fetch_timeout_seconds=fetch_timeout_seconds,
            fetch_max_request_bytes=max(1, fetch_max_request_bytes),
            fetch_max_response_bytes=fetch_max_response_bytes,
            fetch_max_redirects=max(0, fetch_max_redirects),
            fetch_secret_key=fetch_secret_key,
            agent_provider=agent_provider,
            agent_model=agent_model,
            agent_base_url=agent_base_url,
            agent_api_key=agent_api_key,
            agent_binary_path=agent_binary_path,
            agent_binary_max_output_bytes=max(1024, agent_binary_max_output_bytes),
            agent_timeout_seconds=max(1, agent_timeout_seconds),
            agent_max_rounds=max(1, agent_max_rounds),
            agent_max_output_tokens=max(1, agent_max_output_tokens),
            remote_execution_enabled=remote_execution_enabled,
            remote_ssh_command=remote_ssh_command,
            remote_connect_timeout_seconds=max(1, remote_connect_timeout_seconds),
            remote_command_timeout_seconds=max(1, remote_command_timeout_seconds),
            remote_max_output_bytes=max(1024, remote_max_output_bytes),
            remote_host_key_policy=remote_host_key_policy,
            remote_commands=remote_commands,
            webui_dir=webui_dir,
        )


def parse_tools_env(raw: str | None) -> tuple[ToolDefinition, ...]:
    configured: list[ToolDefinition] = []
    if raw:
        decoded = json.loads(raw)
        if not isinstance(decoded, list):
            raise ValueError("LOGAGENT_V2_TOOLS_JSON must be a JSON array")
        configured.extend(ToolDefinition.from_json(item) for item in decoded)
    configured.extend(default_source_built_tools_from_env())
    seen = set()
    deduped = []
    for tool in configured:
        if tool.id in seen:
            continue
        seen.add(tool.id)
        deduped.append(tool)
    return tuple(deduped)


def default_source_built_tools_from_env() -> list[ToolDefinition]:
    definitions = [
        (
            "flux_query_analyzer",
            "Flux Query Analyzer",
            env_first("LOGAGENT_V2_TOOL_FLUX_QUERY_ANALYZER", "LOGAGENT_TOOL_FLUX_QUERY_ANALYZER"),
            30,
            3,
            (
                "--input",
                "{input_file}",
                "--format",
                "json",
                "--top-k",
                "20",
                "--max-input-lines",
                "100000",
                "--max-error-findings",
                "20",
            ),
            ("*.jsonl", "*.ndjson"),
            ("flux", '"query"', "duration_ms"),
        ),
        (
            "influxql_analyzer",
            "InfluxQL Analyzer",
            env_first(
                "LOGAGENT_V2_TOOL_INFLUXQL_ANALYZER",
                "LOGAGENT_TOOL_INFLUXQL_ANALYZER",
            ),
            30,
            3,
            ("-input", "{input_file}", "-output", "json", "-detail-limit", "5"),
            ("*.jsonl",),
            ("influxql", '"query"', "select", "show series", "show measurements"),
        ),
        (
            "opengemini_storage_analyzer",
            "openGemini Storage Analyzer",
            env_first(
                "LOGAGENT_V2_TOOL_OPENGEMINI_STORAGE_ANALYZER",
                "LOGAGENT_TOOL_OPENGEMINI_STORAGE_ANALYZER",
            ),
            30,
            10,
            ("--input", "{input_file}", "--format", "json"),
            (
                "*.tssp",
                "*.tssp.init",
                "metadata.json",
                "metaindex.bin",
                "index.bin",
                "items.bin",
                "lens.bin",
                "*_mergeset.bf",
                "*_mergeset.bf.last",
                "*_mergeset.bf.init",
            ),
            (
                "tssp",
                "mergeset",
                "metadata.json",
                "invalid file",
                "open tssp",
            ),
        ),
        (
            "influxdb_storage_analyzer",
            "InfluxDB Storage Analyzer",
            env_first(
                "LOGAGENT_V2_TOOL_INFLUXDB_STORAGE_ANALYZER",
                "LOGAGENT_TOOL_INFLUXDB_STORAGE_ANALYZER",
            ),
            60,
            5,
            ("-input", "{input_file}", "-kind", "auto", "-max-samples", "10"),
            ("*.tsm", "*.tsi"),
            ("_series", "tsm", "tsi", "series file"),
        ),
    ]
    tools = []
    for (
        tool_id,
        display_name,
        command,
        timeout_seconds,
        max_input_files,
        args,
        patterns,
        keywords,
    ) in definitions:
        if not command:
            continue
        tools.append(
            ToolDefinition(
                id=tool_id,
                display_name=display_name,
                command=command,
                args=args,
                enabled=True,
                timeout_seconds=timeout_seconds,
                max_output_bytes=1024 * 1024,
                max_input_files=max_input_files,
                match_file_patterns=patterns,
                match_keywords=keywords,
            )
        )
    return tools


def env_first(*names: str) -> str | None:
    for name in names:
        value = os.environ.get(name)
        if value:
            return value
    return None


def env_bool(name: str, default: bool) -> bool:
    raw = os.environ.get(name)
    if raw is None:
        return default
    return raw.strip().lower() in {"1", "true", "yes", "on"}


def parse_remote_commands_env(raw: str | None) -> tuple[RemoteCommandTemplate, ...]:
    if not raw:
        return default_remote_commands()
    decoded = json.loads(raw)
    if not isinstance(decoded, list):
        raise ValueError("LOGAGENT_V2_REMOTE_COMMANDS_JSON must be a JSON array")
    return tuple(RemoteCommandTemplate.from_json(item) for item in decoded)


def strings_from_list(value: Any) -> list[str]:
    if not isinstance(value, list):
        return []
    return [str(item) for item in value if str(item).strip()]


def normalize_params_schema(value: Any) -> dict[str, Any] | None:
    if value is None:
        return None
    if not isinstance(value, dict):
        raise ValueError("tool paramsSchema must be an object")
    schema = dict(value)
    if schema.get("type", "object") != "object":
        raise ValueError("tool paramsSchema must use type=object")
    schema["type"] = "object"
    properties = schema.get("properties", {})
    if properties is not None and not isinstance(properties, dict):
        raise ValueError("tool paramsSchema properties must be an object")
    required = schema.get("required", [])
    if required is not None and not isinstance(required, list):
        raise ValueError("tool paramsSchema required must be an array")
    return schema
