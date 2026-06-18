from __future__ import annotations

import os
import json
import urllib.parse
import base64
import binascii
import re
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

LOGAGENT_MCP_ALLOWED_TOOL_GLOB = "mcp__logagent__*"
ANALYSIS_MODES = ("diagnose", "code_investigation", "fix")
REMOTE_FILE_PATH_RE = re.compile(r"^/[A-Za-z0-9._/@+=,-]+$")
DEFAULT_GREP_KEYWORDS = (
    "error",
    "exception",
    "timeout",
    "fail",
    "failed",
    "panic",
    "fatal",
    "refused",
    "denied",
    "verify",
)


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
        validate_tool_name(tool_id)
        enabled = bool(value.get("enabled", True))
        command = resolve_tool_json_command(tool_id, value, enabled=enabled)
        validate_tool_command_path(tool_id, command, enabled=enabled)
        match = value.get("match") if isinstance(value.get("match"), dict) else {}
        return cls(
            id=tool_id,
            display_name=str(value.get("displayName") or value.get("display_name") or tool_id),
            command=command,
            args=tuple(str(arg) for arg in value.get("args", [])),
            enabled=enabled,
            timeout_seconds=max(1, int(camel_or_snake(value, "timeoutSeconds", 30))),
            max_output_bytes=max(
                1024,
                int(camel_or_snake(value, "maxOutputBytes", 1024 * 1024)),
            ),
            max_input_files=max(1, int(camel_or_snake(value, "maxInputFiles", 1))),
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
        validate_remote_command_id(command_id)
        argv = tuple(
            arg for arg in (str(item).strip() for item in value.get("argv", [])) if arg
        )
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
        RemoteCommandTemplate(
            command_id="system_uname",
            display_name="System: kernel and OS",
            description="Read-only kernel and operating system summary.",
            argv=("uname", "-a"),
        ),
        RemoteCommandTemplate(
            command_id="uptime_load",
            display_name="System: uptime and load",
            description="Read-only uptime and load-average snapshot.",
            argv=("uptime",),
        ),
        RemoteCommandTemplate(
            command_id="disk_usage",
            display_name="System: disk usage",
            description="Read-only filesystem capacity snapshot.",
            argv=("df", "-h"),
        ),
        RemoteCommandTemplate(
            command_id="memory_usage",
            display_name="System: memory usage",
            description="Read-only memory usage snapshot.",
            argv=("free", "-m"),
        ),
        RemoteCommandTemplate(
            command_id="process_overview",
            display_name="System: process overview",
            description="Read-only process list without command-line arguments.",
            argv=("ps", "-eo", "pid,ppid,stat,comm", "--sort=comm"),
        ),
        RemoteCommandTemplate(
            command_id="network_listeners",
            display_name="System: network listeners",
            description="Read-only TCP listener snapshot.",
            argv=("ss", "-lntp"),
        ),
        RemoteCommandTemplate(
            command_id="opengemini_processes",
            display_name="openGemini: processes",
            description="Read-only openGemini process snapshot without command-line arguments.",
            argv=(
                "ps",
                "-C",
                "ts-meta,ts-sql,ts-store,opengemini,influxd",
                "-o",
                "pid,ppid,stat,comm",
                "--sort=comm",
            ),
        ),
        RemoteCommandTemplate(
            command_id="opengemini_config_dirs",
            display_name="openGemini: config directories",
            description="Read-only listing of common openGemini configuration directories.",
            argv=(
                "ls",
                "-la",
                "/etc/openGemini",
                "/etc/opengemini",
                "/opt/openGemini",
                "/opt/opengemini",
            ),
        ),
        RemoteCommandTemplate(
            command_id="opengemini_log_dirs",
            display_name="openGemini: log directories",
            description="Read-only listing of common openGemini log directories.",
            argv=(
                "ls",
                "-la",
                "/var/log/openGemini",
                "/var/log/opengemini",
                "/home/Ruby/log",
            ),
        ),
        RemoteCommandTemplate(
            command_id="opengemini_data_dirs",
            display_name="openGemini: data directories",
            description="Read-only listing of common openGemini data directories.",
            argv=(
                "ls",
                "-la",
                "/var/lib/openGemini",
                "/var/lib/opengemini",
                "/var/chroot/gemini",
            ),
        ),
        RemoteCommandTemplate(
            command_id="cassandra_processes",
            display_name="Cassandra: processes",
            description="Read-only Cassandra process snapshot without command-line arguments.",
            argv=(
                "ps",
                "-C",
                "cassandra,java",
                "-o",
                "pid,ppid,stat,comm",
                "--sort=comm",
            ),
        ),
        RemoteCommandTemplate(
            command_id="cassandra_config_dirs",
            display_name="Cassandra: config directories",
            description="Read-only listing of common Cassandra configuration directories.",
            argv=(
                "ls",
                "-la",
                "/etc/cassandra",
                "/etc/cassandra/conf",
                "/opt/cassandra/conf",
            ),
        ),
        RemoteCommandTemplate(
            command_id="cassandra_log_dirs",
            display_name="Cassandra: log directories",
            description="Read-only listing of common Cassandra log directories.",
            argv=(
                "ls",
                "-la",
                "/var/log/cassandra",
                "/opt/cassandra/logs",
            ),
        ),
        RemoteCommandTemplate(
            command_id="cassandra_data_dirs",
            display_name="Cassandra: data directories",
            description="Read-only listing of common Cassandra data directories.",
            argv=(
                "ls",
                "-la",
                "/var/lib/cassandra",
                "/data/cassandra",
            ),
        ),
        RemoteCommandTemplate(
            command_id="rocksdb_data_dirs",
            display_name="RocksDB: data directories",
            description="Read-only listing of common RocksDB data directories.",
            argv=(
                "ls",
                "-la",
                "/var/lib/rocksdb",
                "/data/rocksdb",
                "/var/lib/cassandra/data",
            ),
        ),
        RemoteCommandTemplate(
            command_id="rocksdb_wal_dirs",
            display_name="RocksDB: WAL directories",
            description="Read-only listing of common RocksDB WAL directories.",
            argv=(
                "ls",
                "-la",
                "/var/lib/rocksdb/wal",
                "/data/rocksdb/wal",
                "/var/lib/cassandra/commitlog",
            ),
        ),
        RemoteCommandTemplate(
            command_id="rocksdb_log_dirs",
            display_name="RocksDB: log directories",
            description="Read-only listing of common RocksDB log directories.",
            argv=(
                "ls",
                "-la",
                "/var/log/rocksdb",
                "/tmp/rocksdb",
            ),
        ),
    )


@dataclass(frozen=True)
class RemoteFileTemplate:
    file_id: str
    display_name: str
    description: str
    remote_path: str
    enabled: bool = True
    timeout_seconds: int | None = None
    max_bytes: int | None = None

    @classmethod
    def from_json(cls, value: dict) -> "RemoteFileTemplate":
        file_id = str(value.get("fileId") or value.get("file_id") or value["id"])
        validate_remote_command_id(file_id)
        remote_path = validate_remote_file_path(
            str(value.get("remotePath") or value.get("remote_path") or "")
        )
        return cls(
            file_id=file_id,
            display_name=str(value.get("displayName") or value.get("display_name") or file_id),
            description=str(value.get("description") or ""),
            remote_path=remote_path,
            enabled=bool(value.get("enabled", True)),
            timeout_seconds=(
                max(1, int(value["timeoutSeconds"]))
                if value.get("timeoutSeconds") is not None
                else None
            ),
            max_bytes=(
                max(1, int(value["maxBytes"]))
                if value.get("maxBytes") is not None
                else None
            ),
        )


@dataclass(frozen=True)
class CodeRepoDefinition:
    product: str
    repo_path: Path
    default_ref: str
    version_refs: dict[str, str] = field(default_factory=dict)
    search_roots: tuple[str, ...] = ()

    @classmethod
    def from_json(cls, product: str, value: dict) -> "CodeRepoDefinition":
        product = product.strip()
        if not product:
            raise ValueError("code repo product must not be empty")
        repo_path_value = value.get("repoPath") or value.get("repo_path")
        if not isinstance(repo_path_value, str) or not repo_path_value.strip():
            raise ValueError(f"code repo {product} requires repoPath")
        repo_path = Path(expand_tool_command(repo_path_value.strip()))
        if not repo_path.is_absolute():
            raise ValueError(f"code repo {product} repoPath must resolve to an absolute path")
        if not repo_path.is_dir():
            raise ValueError(f"code repo {product} repoPath is not a directory")
        default_ref = validate_configured_git_ref(
            product,
            str(value.get("defaultRef") or value.get("default_ref") or "HEAD"),
        )
        raw_version_refs = value.get("versionRefs") or value.get("version_refs") or {}
        if not isinstance(raw_version_refs, dict):
            raise ValueError(f"code repo {product} versionRefs must be an object")
        version_refs = {
            str(version): validate_configured_git_ref(product, str(ref))
            for version, ref in raw_version_refs.items()
        }
        return cls(
            product=product,
            repo_path=repo_path,
            default_ref=default_ref,
            version_refs=version_refs,
            search_roots=normalize_code_search_roots(
                value.get("searchRoots") or value.get("search_roots") or []
            ),
        )


def normalize_code_search_roots(value: object) -> tuple[str, ...]:
    if value is None:
        return ()
    if not isinstance(value, list):
        raise ValueError("code repo searchRoots must be an array")
    roots = []
    for item in value:
        root = str(item).strip().replace("\\", "/")
        if not root:
            continue
        if root.startswith("/") or any(part in {"", ".", ".."} for part in root.split("/")):
            raise ValueError(f"unsafe code repo search root {item!r}")
        roots.append(root)
    return tuple(dict.fromkeys(roots))


def validate_configured_git_ref(product: str, ref: str) -> str:
    ref = ref.strip()
    if (
        not ref
        or ref.startswith("-")
        or any(ord(char) < 32 or char.isspace() for char in ref)
        or ".." in ref
        or ref.endswith("/")
        or ref.endswith(".lock")
    ):
        raise ValueError(f"code repo {product} has unsafe git ref {ref!r}")
    return ref


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


@dataclass(frozen=True)
class ClaudeCodePermissionProfile:
    name: str
    permission_mode: str
    tools: str
    allowed_tools: tuple[str, ...]
    disallowed_tools: tuple[str, ...] = ()
    native_bash: bool = False
    native_edit: bool = False
    worktree_required: bool = False

    def to_json(self) -> dict[str, Any]:
        return {
            "name": self.name,
            "permissionMode": self.permission_mode,
            "tools": self.tools,
            "allowedTools": list(self.allowed_tools),
            "disallowedTools": list(self.disallowed_tools),
            "nativeBash": self.native_bash,
            "nativeEdit": self.native_edit,
            "worktreeRequired": self.worktree_required,
        }


def default_claude_code_permission_profiles() -> dict[str, ClaudeCodePermissionProfile]:
    return {
        "diagnose": ClaudeCodePermissionProfile(
            name="diagnose",
            permission_mode="dontAsk",
            tools="",
            allowed_tools=(LOGAGENT_MCP_ALLOWED_TOOL_GLOB,),
            disallowed_tools=("Bash", "Edit", "Write", "Read", "Grep"),
            native_bash=False,
            native_edit=False,
            worktree_required=False,
        ),
        "code_investigation": ClaudeCodePermissionProfile(
            name="code_investigation",
            permission_mode="dontAsk",
            tools="Read,Grep,Bash",
            allowed_tools=with_logagent_mcp_allowed_tools(("Read", "Grep", "Bash")),
            disallowed_tools=("Edit", "Write"),
            native_bash=True,
            native_edit=False,
            worktree_required=False,
        ),
        "fix": ClaudeCodePermissionProfile(
            name="fix",
            permission_mode="acceptEdits",
            tools="Read,Grep,Bash,Edit,Write",
            allowed_tools=with_logagent_mcp_allowed_tools(
                ("Read", "Grep", "Bash", "Edit", "Write")
            ),
            disallowed_tools=(),
            native_bash=True,
            native_edit=True,
            worktree_required=True,
        ),
    }


def with_logagent_mcp_allowed_tools(values: tuple[str, ...]) -> tuple[str, ...]:
    normalized = tuple(item.strip() for item in values if item.strip())
    if LOGAGENT_MCP_ALLOWED_TOOL_GLOB in normalized:
        return normalized
    return (*normalized, LOGAGENT_MCP_ALLOWED_TOOL_GLOB)


def default_webui_dir() -> Path:
    return Path(__file__).resolve().parents[2] / "webui" / "out"


@dataclass(frozen=True)
class Settings:
    data_dir: Path
    api_key: str
    host: str = "127.0.0.1"
    port: int = 50993
    max_upload_bytes: int = 512 * 1024 * 1024
    max_chunk_bytes: int = 512 * 1024
    max_archive_files: int = 2000
    max_archive_bytes: int = 256 * 1024 * 1024
    max_text_file_bytes: int = 16 * 1024 * 1024
    max_grep_matches: int = 200
    grep_keywords: tuple[str, ...] = DEFAULT_GREP_KEYWORDS
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
    claude_code_path: Path | None = None
    claude_code_max_output_bytes: int = 1024 * 1024
    claude_code_permission_mode: str = "dontAsk"
    claude_code_tools: str = ""
    claude_code_allowed_tools: tuple[str, ...] = (LOGAGENT_MCP_ALLOWED_TOOL_GLOB,)
    claude_code_disallowed_tools: tuple[str, ...] = (
        "Bash",
        "Edit",
        "Write",
        "Read",
        "Grep",
    )
    claude_code_permission_profiles: dict[str, ClaudeCodePermissionProfile] = field(
        default_factory=default_claude_code_permission_profiles
    )
    agent_timeout_seconds: int = 60
    agent_max_rounds: int = 4
    agent_max_llm_calls: int = 4
    agent_max_actions: int = 6
    agent_max_repeated_action_fingerprints: int = 1
    agent_max_output_tokens: int = 2048
    agent_max_total_tokens: int = 200_000
    agent_max_runtime_seconds: int = 300
    agent_max_user_prompts: int = 3
    agent_max_approvals: int = 3
    remote_execution_enabled: bool = True
    remote_ssh_command: str = "/usr/bin/ssh"
    remote_scp_command: str = "/usr/bin/scp"
    remote_connect_timeout_seconds: int = 10
    remote_command_timeout_seconds: int = 30
    remote_max_output_bytes: int = 1024 * 1024
    remote_file_max_bytes: int = 16 * 1024 * 1024
    remote_host_key_policy: str = "accept-new"
    remote_commands: tuple[RemoteCommandTemplate, ...] = field(default_factory=default_remote_commands)
    remote_files: tuple[RemoteFileTemplate, ...] = ()
    code_repos: tuple[CodeRepoDefinition, ...] = ()
    code_worktree_root: Path | None = None
    code_worktree_max_per_repo: int = 5
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

    @property
    def effective_code_worktree_root(self) -> Path:
        return self.code_worktree_root or (self.data_dir / "code_worktrees")

    def ensure_dirs(self) -> None:
        self.data_dir.mkdir(parents=True, exist_ok=True)
        self.artifacts_dir.mkdir(parents=True, exist_ok=True)
        self.tmp_dir.mkdir(parents=True, exist_ok=True)
        self.skills_dir.mkdir(parents=True, exist_ok=True)
        self.effective_code_worktree_root.mkdir(parents=True, exist_ok=True)

    @classmethod
    def from_env(cls) -> "Settings":
        data_dir = Path(os.environ.get("LOGAGENT_V2_DATA_DIR", "/tmp/logagent-v2")).expanduser()
        api_key = os.environ.get("LOGAGENT_V2_API_KEY", "dev-token")
        host = os.environ.get("LOGAGENT_V2_HOST", "127.0.0.1")
        port = int(os.environ.get("LOGAGENT_V2_PORT", "50993"))
        max_upload_bytes = int(
            os.environ.get("LOGAGENT_V2_MAX_UPLOAD_BYTES", str(512 * 1024 * 1024))
        )
        max_chunk_bytes = int(os.environ.get("LOGAGENT_V2_MAX_CHUNK_BYTES", str(512 * 1024)))
        max_archive_files = int(os.environ.get("LOGAGENT_V2_MAX_ARCHIVE_FILES", "2000"))
        max_archive_bytes = int(
            os.environ.get("LOGAGENT_V2_MAX_ARCHIVE_BYTES", str(256 * 1024 * 1024))
        )
        max_text_file_bytes = int(
            os.environ.get("LOGAGENT_V2_MAX_TEXT_FILE_BYTES", str(16 * 1024 * 1024))
        )
        max_grep_matches = int(os.environ.get("LOGAGENT_V2_MAX_GREP_MATCHES", "200"))
        grep_keywords = parse_csv_env(
            os.environ.get("LOGAGENT_V2_GREP_KEYWORDS"),
            default=DEFAULT_GREP_KEYWORDS,
        )
        max_concurrent_jobs = int(os.environ.get("LOGAGENT_V2_MAX_CONCURRENT_JOBS", "2"))
        inline_worker = os.environ.get("LOGAGENT_V2_INLINE_WORKER", "1") != "0"
        tools = parse_tools_env(os.environ.get("LOGAGENT_V2_TOOLS_JSON"))
        raw_pprof_go_command = env_first(
            "LOGAGENT_V2_PPROF_GO_COMMAND",
            "LOGAGENT_TOOL_PPROF_GO",
        )
        pprof_enabled = env_bool(
            "LOGAGENT_V2_PPROF_ENABLED",
            default=bool(non_empty_string(raw_pprof_go_command)),
        )
        pprof_go_command = parse_pprof_go_command_env(
            raw_pprof_go_command,
            enabled=pprof_enabled,
        )
        huawei_package_sync = parse_huawei_package_sync_env(
            enabled=env_bool("LOGAGENT_V2_HUAWEI_PACKAGE_SYNC_ENABLED", default=False),
            obs_endpoint=os.environ.get("LOGAGENT_V2_HUAWEI_OBS_ENDPOINT"),
            obs_bucket=os.environ.get("LOGAGENT_V2_HUAWEI_OBS_BUCKET"),
            obs_object_prefix=os.environ.get("LOGAGENT_V2_HUAWEI_OBS_OBJECT_PREFIX", ""),
            obs_access_key=os.environ.get("LOGAGENT_V2_HUAWEI_OBS_ACCESS_KEY"),
            obs_secret_key=os.environ.get("LOGAGENT_V2_HUAWEI_OBS_SECRET_KEY"),
            obs_security_token=os.environ.get("LOGAGENT_V2_HUAWEI_OBS_SECURITY_TOKEN"),
            gaussdb_dsn=os.environ.get("LOGAGENT_V2_HUAWEI_GAUSSDB_DSN"),
            timeout_seconds=int(os.environ.get("LOGAGENT_V2_HUAWEI_TIMEOUT_SECONDS", "30")),
        )
        fetch_enabled = os.environ.get("LOGAGENT_V2_FETCH_ENABLED", "0") == "1"
        fetch_allowed_hosts = parse_fetch_allowed_hosts_env(
            os.environ.get("LOGAGENT_V2_FETCH_ALLOWED_HOSTS"),
            enabled=fetch_enabled,
        )
        fetch_timeout_seconds = int(os.environ.get("LOGAGENT_V2_FETCH_TIMEOUT_SECONDS", "20"))
        fetch_max_request_bytes = int(
            os.environ.get("LOGAGENT_V2_FETCH_MAX_REQUEST_BYTES", str(1024 * 1024))
        )
        fetch_max_response_bytes = int(
            os.environ.get("LOGAGENT_V2_FETCH_MAX_RESPONSE_BYTES", str(1024 * 1024))
        )
        fetch_max_redirects = int(os.environ.get("LOGAGENT_V2_FETCH_MAX_REDIRECTS", "5"))
        fetch_secret_key = parse_fetch_secret_key_env(
            os.environ.get("LOGAGENT_V2_FETCH_SECRET_KEY"),
            enabled=fetch_enabled,
        )
        agent_provider = parse_agent_provider_env(os.environ.get("LOGAGENT_V2_AGENT_PROVIDER"))
        agent_model = non_empty_string(os.environ.get("LOGAGENT_V2_AGENT_MODEL"))
        agent_base_url = non_empty_string(os.environ.get("LOGAGENT_V2_AGENT_BASE_URL"))
        agent_api_key = non_empty_string(os.environ.get("LOGAGENT_V2_AGENT_API_KEY"))
        raw_agent_binary_path = os.environ.get("LOGAGENT_V2_AGENT_BINARY_PATH")
        agent_binary_path = parse_agent_binary_path_env(
            raw_agent_binary_path,
            enabled=agent_provider == "binary",
        )
        raw_claude_code_path = env_first(
            "LOGAGENT_V2_CLAUDE_CODE_PATH",
            "LOGAGENT_CLAUDE_CODE_PATH",
        )
        claude_code_path = parse_claude_code_path_env(
            raw_claude_code_path,
            enabled=agent_provider == "claude_code",
        )
        validate_agent_provider_settings(
            agent_provider,
            agent_base_url=agent_base_url,
            agent_model=agent_model,
            agent_api_key=agent_api_key,
            agent_binary_path=agent_binary_path,
            claude_code_path=claude_code_path,
        )
        agent_binary_max_output_bytes = int(
            os.environ.get(
                "LOGAGENT_V2_AGENT_BINARY_MAX_OUTPUT_BYTES",
                str(1024 * 1024),
            )
        )
        claude_code_max_output_bytes = int(
            os.environ.get(
                "LOGAGENT_V2_CLAUDE_CODE_MAX_OUTPUT_BYTES",
                str(1024 * 1024),
            )
        )
        claude_code_permission_mode = (
            non_empty_string(os.environ.get("LOGAGENT_V2_CLAUDE_CODE_PERMISSION_MODE"))
            or "dontAsk"
        )
        claude_code_tools = os.environ.get("LOGAGENT_V2_CLAUDE_CODE_TOOLS", "").strip()
        claude_code_allowed_tools = parse_csv_env(
            os.environ.get("LOGAGENT_V2_CLAUDE_CODE_ALLOWED_TOOLS"),
            default=("mcp__logagent__*",),
        )
        claude_code_disallowed_tools = parse_csv_env(
            os.environ.get("LOGAGENT_V2_CLAUDE_CODE_DISALLOWED_TOOLS"),
            default=(),
        )
        claude_code_permission_profiles = parse_claude_code_permission_profiles_env(
            os.environ.get("LOGAGENT_V2_CLAUDE_CODE_PERMISSION_PROFILES_JSON"),
            legacy_permission_mode=claude_code_permission_mode,
            legacy_tools=claude_code_tools,
            legacy_allowed_tools=claude_code_allowed_tools,
            legacy_disallowed_tools=claude_code_disallowed_tools,
            legacy_env_present=any(
                non_empty_string(os.environ.get(name))
                for name in (
                    "LOGAGENT_V2_CLAUDE_CODE_PERMISSION_MODE",
                    "LOGAGENT_V2_CLAUDE_CODE_TOOLS",
                    "LOGAGENT_V2_CLAUDE_CODE_ALLOWED_TOOLS",
                    "LOGAGENT_V2_CLAUDE_CODE_DISALLOWED_TOOLS",
                )
            ),
        )
        diagnose_profile = claude_code_permission_profiles["diagnose"]
        claude_code_permission_mode = diagnose_profile.permission_mode
        claude_code_tools = diagnose_profile.tools
        claude_code_allowed_tools = diagnose_profile.allowed_tools
        claude_code_disallowed_tools = diagnose_profile.disallowed_tools
        agent_timeout_seconds = int(os.environ.get("LOGAGENT_V2_AGENT_TIMEOUT_SECONDS", "60"))
        agent_max_rounds = int(os.environ.get("LOGAGENT_V2_AGENT_MAX_ROUNDS", "4"))
        agent_max_llm_calls = int(os.environ.get("LOGAGENT_V2_AGENT_MAX_LLM_CALLS", "4"))
        agent_max_actions = int(os.environ.get("LOGAGENT_V2_AGENT_MAX_ACTIONS", "6"))
        agent_max_repeated_action_fingerprints = int(
            os.environ.get("LOGAGENT_V2_AGENT_MAX_REPEATED_ACTION_FINGERPRINTS", "1")
        )
        agent_max_output_tokens = int(
            os.environ.get("LOGAGENT_V2_AGENT_MAX_OUTPUT_TOKENS", "2048")
        )
        agent_max_total_tokens = int(
            os.environ.get("LOGAGENT_V2_AGENT_MAX_TOTAL_TOKENS", "200000")
        )
        agent_max_runtime_seconds = int(
            os.environ.get("LOGAGENT_V2_AGENT_MAX_RUNTIME_SECONDS", "300")
        )
        agent_max_user_prompts = int(
            os.environ.get("LOGAGENT_V2_AGENT_MAX_USER_PROMPTS", "3")
        )
        agent_max_approvals = int(
            os.environ.get("LOGAGENT_V2_AGENT_MAX_APPROVALS", "3")
        )
        remote_execution_enabled = os.environ.get("LOGAGENT_V2_REMOTE_EXECUTION_ENABLED", "1") != "0"
        remote_ssh_command = expand_tool_command(
            os.environ.get("LOGAGENT_V2_REMOTE_SSH_COMMAND", "/usr/bin/ssh")
        )
        validate_remote_ssh_command_path(
            remote_ssh_command,
            enabled=remote_execution_enabled,
        )
        remote_scp_command = expand_tool_command(
            os.environ.get("LOGAGENT_V2_REMOTE_SCP_COMMAND", "/usr/bin/scp")
        )
        validate_remote_ssh_command_path(
            remote_scp_command,
            enabled=remote_execution_enabled,
            env_name="LOGAGENT_V2_REMOTE_SCP_COMMAND",
        )
        remote_connect_timeout_seconds = int(
            os.environ.get("LOGAGENT_V2_REMOTE_CONNECT_TIMEOUT_SECONDS", "10")
        )
        remote_command_timeout_seconds = int(
            os.environ.get("LOGAGENT_V2_REMOTE_COMMAND_TIMEOUT_SECONDS", "30")
        )
        remote_max_output_bytes = int(
            os.environ.get("LOGAGENT_V2_REMOTE_MAX_OUTPUT_BYTES", str(1024 * 1024))
        )
        remote_file_max_bytes = int(
            os.environ.get("LOGAGENT_V2_REMOTE_FILE_MAX_BYTES", str(16 * 1024 * 1024))
        )
        remote_host_key_policy = os.environ.get(
            "LOGAGENT_V2_REMOTE_HOST_KEY_POLICY", "accept-new"
        ).strip().lower()
        validate_remote_host_key_policy(remote_host_key_policy)
        remote_commands = parse_remote_commands_env(
            os.environ.get("LOGAGENT_V2_REMOTE_COMMANDS_JSON")
        )
        remote_files = parse_remote_files_env(os.environ.get("LOGAGENT_V2_REMOTE_FILES_JSON"))
        code_repos = parse_code_repos_env(os.environ.get("LOGAGENT_V2_CODE_REPOS_JSON"))
        raw_code_worktree_root = os.environ.get("LOGAGENT_V2_CODE_WORKTREE_ROOT")
        code_worktree_root = (
            Path(raw_code_worktree_root).expanduser()
            if raw_code_worktree_root and raw_code_worktree_root.strip()
            else None
        )
        if code_worktree_root is not None and not code_worktree_root.is_absolute():
            raise ValueError("LOGAGENT_V2_CODE_WORKTREE_ROOT must resolve to an absolute path")
        code_worktree_max_per_repo = int(
            os.environ.get("LOGAGENT_V2_CODE_WORKTREE_MAX_PER_REPO", "5")
        )
        raw_webui_dir = os.environ.get("LOGAGENT_V2_WEBUI_DIR")
        webui_dir = Path(raw_webui_dir).expanduser() if raw_webui_dir else default_webui_dir()
        return cls(
            data_dir=data_dir,
            api_key=api_key,
            host=host,
            port=port,
            max_upload_bytes=max_upload_bytes,
            max_chunk_bytes=max(1, max_chunk_bytes),
            max_archive_files=max_archive_files,
            max_archive_bytes=max_archive_bytes,
            max_text_file_bytes=max_text_file_bytes,
            max_grep_matches=max_grep_matches,
            grep_keywords=grep_keywords,
            max_concurrent_jobs=max(1, max_concurrent_jobs),
            inline_worker=inline_worker,
            tools=tools,
            pprof_go_command=pprof_go_command,
            pprof_enabled=pprof_enabled,
            huawei_package_sync=huawei_package_sync,
            fetch_enabled=fetch_enabled,
            fetch_allowed_hosts=fetch_allowed_hosts,
            fetch_timeout_seconds=max(1, fetch_timeout_seconds),
            fetch_max_request_bytes=max(1, fetch_max_request_bytes),
            fetch_max_response_bytes=max(1, fetch_max_response_bytes),
            fetch_max_redirects=max(0, fetch_max_redirects),
            fetch_secret_key=fetch_secret_key,
            agent_provider=agent_provider,
            agent_model=agent_model,
            agent_base_url=agent_base_url,
            agent_api_key=agent_api_key,
            agent_binary_path=agent_binary_path,
            agent_binary_max_output_bytes=max(1024, agent_binary_max_output_bytes),
            claude_code_path=claude_code_path,
            claude_code_max_output_bytes=max(1024, claude_code_max_output_bytes),
            claude_code_permission_mode=claude_code_permission_mode,
            claude_code_tools=claude_code_tools,
            claude_code_allowed_tools=claude_code_allowed_tools,
            claude_code_disallowed_tools=claude_code_disallowed_tools,
            claude_code_permission_profiles=claude_code_permission_profiles,
            agent_timeout_seconds=max(1, agent_timeout_seconds),
            agent_max_rounds=max(1, agent_max_rounds),
            agent_max_llm_calls=max(1, agent_max_llm_calls),
            agent_max_actions=max(1, agent_max_actions),
            agent_max_repeated_action_fingerprints=max(
                1, agent_max_repeated_action_fingerprints
            ),
            agent_max_output_tokens=max(1, agent_max_output_tokens),
            agent_max_total_tokens=max(1, agent_max_total_tokens),
            agent_max_runtime_seconds=max(1, agent_max_runtime_seconds),
            agent_max_user_prompts=max(1, agent_max_user_prompts),
            agent_max_approvals=max(1, agent_max_approvals),
            remote_execution_enabled=remote_execution_enabled,
            remote_ssh_command=remote_ssh_command,
            remote_scp_command=remote_scp_command,
            remote_connect_timeout_seconds=max(1, remote_connect_timeout_seconds),
            remote_command_timeout_seconds=max(1, remote_command_timeout_seconds),
            remote_max_output_bytes=max(1024, remote_max_output_bytes),
            remote_file_max_bytes=max(1, remote_file_max_bytes),
            remote_host_key_policy=remote_host_key_policy,
            remote_commands=remote_commands,
            remote_files=remote_files,
            code_repos=code_repos,
            code_worktree_root=code_worktree_root,
            code_worktree_max_per_repo=max(1, code_worktree_max_per_repo),
            webui_dir=webui_dir,
        )


def claude_code_profile_for_mode(
    settings: Settings, analysis_mode: object
) -> ClaudeCodePermissionProfile:
    try:
        mode = normalize_analysis_mode(analysis_mode)
    except ValueError:
        mode = "diagnose"
    return settings.claude_code_permission_profiles.get(
        mode,
        default_claude_code_permission_profiles()[mode],
    )


def parse_tools_env(raw: str | None) -> tuple[ToolDefinition, ...]:
    configured: list[ToolDefinition] = []
    if raw:
        decoded = json.loads(raw)
        configured.extend(tool_definitions_from_json(decoded))
    configured.extend(default_source_built_tools_from_env())
    seen = set()
    deduped = []
    for tool in configured:
        if tool.id in seen:
            continue
        seen.add(tool.id)
        deduped.append(tool)
    return tuple(deduped)


def parse_code_repos_env(raw: str | None) -> tuple[CodeRepoDefinition, ...]:
    value = non_empty_string(raw)
    if not value:
        return ()
    decoded = json.loads(value)
    if isinstance(decoded, dict):
        repos = [
            CodeRepoDefinition.from_json(str(product), descriptor)
            for product, descriptor in decoded.items()
            if isinstance(descriptor, dict)
        ]
        if len(repos) != len(decoded):
            raise ValueError("LOGAGENT_V2_CODE_REPOS_JSON object values must be objects")
        return tuple(repos)
    if isinstance(decoded, list):
        repos = []
        for descriptor in decoded:
            if not isinstance(descriptor, dict):
                raise ValueError("LOGAGENT_V2_CODE_REPOS_JSON entries must be objects")
            product = descriptor.get("product")
            if not isinstance(product, str) or not product.strip():
                raise ValueError("LOGAGENT_V2_CODE_REPOS_JSON entries require product")
            repos.append(CodeRepoDefinition.from_json(product.strip(), descriptor))
        return tuple(repos)
    raise ValueError("LOGAGENT_V2_CODE_REPOS_JSON must be a JSON object or array")


def tool_definitions_from_json(decoded: Any) -> list[ToolDefinition]:
    if isinstance(decoded, list):
        values = decoded
    elif isinstance(decoded, dict):
        values = []
        for tool_id, descriptor in decoded.items():
            if not isinstance(descriptor, dict):
                raise ValueError("LOGAGENT_V2_TOOLS_JSON object values must be objects")
            values.append({"id": tool_id, **descriptor})
    else:
        raise ValueError("LOGAGENT_V2_TOOLS_JSON must be a JSON array or object")
    if not all(isinstance(item, dict) for item in values):
        raise ValueError("LOGAGENT_V2_TOOLS_JSON entries must be objects")
    return [ToolDefinition.from_json(item) for item in values]


def camel_or_snake(value: dict, camel_key: str, default: Any) -> Any:
    if camel_key in value:
        return value[camel_key]
    snake_key = camel_to_snake(camel_key)
    return value.get(snake_key, default)


def camel_to_snake(value: str) -> str:
    result = []
    for char in value:
        if char.isupper():
            result.append("_")
            result.append(char.lower())
        else:
            result.append(char)
    return "".join(result).lstrip("_")


def resolve_tool_json_command(tool_id: str, value: dict, *, enabled: bool) -> str:
    command_value = value.get("command")
    if command_value is None:
        command_value = value.get("path")
    path_env = value.get("pathEnv") or value.get("path_env")
    if command_value is None and path_env:
        if not enabled:
            return ""
        env_name = str(path_env)
        command_value = os.environ.get(env_name)
        if not command_value:
            raise ValueError(f"tool {tool_id} path_env {env_name} is not set")
    if command_value is None:
        if enabled:
            raise ValueError(f"tool {tool_id} command/path/path_env is required")
        return ""
    return expand_tool_command(str(command_value))


def default_source_built_tools_from_env() -> list[ToolDefinition]:
    definitions = [
        (
            "flux_query_analyzer",
            "Flux Query Analyzer",
            source_built_tool_command(
                ("LOGAGENT_V2_TOOL_FLUX_QUERY_ANALYZER", "LOGAGENT_TOOL_FLUX_QUERY_ANALYZER"),
                "flux_query_analyzer",
            ),
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
            source_built_tool_command(
                (
                    "LOGAGENT_V2_TOOL_INFLUXQL_ANALYZER",
                    "LOGAGENT_TOOL_INFLUXQL_ANALYZER",
                ),
                "influxql-analyzer",
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
            source_built_tool_command(
                (
                    "LOGAGENT_V2_TOOL_OPENGEMINI_STORAGE_ANALYZER",
                    "LOGAGENT_TOOL_OPENGEMINI_STORAGE_ANALYZER",
                ),
                "opengemini-storage-analyzer",
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
            source_built_tool_command(
                (
                    "LOGAGENT_V2_TOOL_INFLUXDB_STORAGE_ANALYZER",
                    "LOGAGENT_TOOL_INFLUXDB_STORAGE_ANALYZER",
                ),
                "influxdb_storage_analyzer",
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
        command = expand_tool_command(command)
        validate_tool_command_path(tool_id, command, enabled=True)
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


def source_built_tool_command(env_names: tuple[str, ...], filename: str) -> str | None:
    configured = env_first(*env_names)
    if configured:
        return configured
    for tools_dir in source_built_tool_dirs():
        candidate = tools_dir / filename
        if candidate.is_file():
            return str(candidate.resolve())
    return None


def source_built_tool_dirs() -> list[Path]:
    raw_dirs = []
    if os.environ.get("LOGAGENT_V2_TOOLS_DIR"):
        raw_dirs.append(os.environ["LOGAGENT_V2_TOOLS_DIR"])
    for env_name in ("LOGAGENT_V2_APP_DIR", "LOGAGENT_APP_DIR"):
        app_dir = non_empty_string(os.environ.get(env_name))
        if app_dir:
            raw_dirs.append(str(Path(app_dir) / "bin" / "tools"))
    dirs: list[Path] = []
    seen: set[str] = set()
    for raw_dir in raw_dirs:
        path = Path(os.path.expandvars(os.path.expanduser(raw_dir)))
        if not path.is_absolute():
            path = Path.cwd() / path
        key = str(path)
        if key not in seen:
            seen.add(key)
            dirs.append(path)
    return dirs


def expand_tool_command(command: str) -> str:
    return str(Path(os.path.expanduser(os.path.expandvars(command))))


def validate_tool_command_path(tool_id: str, command: str, *, enabled: bool) -> None:
    if not enabled:
        return
    if not command.strip() or not Path(command).is_absolute():
        raise ValueError(f"tool {tool_id} command must resolve to an absolute path")


def validate_tool_name(name: str) -> None:
    valid = bool(name) and all(
        char.isascii() and (char.isalnum() or char in {"_", "-"})
        for char in name
    )
    if not valid:
        raise ValueError(f"invalid tool name {name}")


def validate_remote_ssh_command_path(
    command: str,
    *,
    enabled: bool,
    env_name: str = "LOGAGENT_V2_REMOTE_SSH_COMMAND",
) -> None:
    if not enabled:
        return
    if not command.strip() or not Path(command).is_absolute():
        raise ValueError(
            f"{env_name} must resolve to an absolute path when remote execution is enabled"
        )


def validate_remote_host_key_policy(policy: str) -> None:
    if policy not in {"accept-new", "strict", "no"}:
        raise ValueError(
            "LOGAGENT_V2_REMOTE_HOST_KEY_POLICY must be one of accept-new, strict, or no"
        )


def validate_remote_command_id(command_id: str) -> None:
    valid = bool(command_id) and all(
        char.isascii() and (char.isalnum() or char in {"_", "-"})
        for char in command_id
    )
    if not valid:
        raise ValueError(f"invalid remote command id {command_id}")


def validate_remote_file_path(value: str) -> str:
    remote_path = value.strip()
    if not remote_path:
        raise ValueError("remote file template remotePath must not be empty")
    normalized = remote_path.replace("\\", "/")
    parts = [part for part in normalized.split("/") if part]
    if (
        normalized != remote_path
        or not REMOTE_FILE_PATH_RE.match(remote_path)
        or "//" in remote_path
        or any(part in {".", ".."} for part in parts)
    ):
        raise ValueError(f"unsafe remote file path {value!r}")
    return remote_path


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


def parse_huawei_package_sync_env(
    *,
    enabled: bool,
    obs_endpoint: str | None,
    obs_bucket: str | None,
    obs_object_prefix: str | None,
    obs_access_key: str | None,
    obs_secret_key: str | None,
    obs_security_token: str | None,
    gaussdb_dsn: str | None,
    timeout_seconds: int,
) -> HuaweiPackageSyncSettings:
    normalized_endpoint = (obs_endpoint or "").strip().rstrip("/")
    normalized_bucket = (obs_bucket or "").strip()
    normalized_prefix = normalize_huawei_object_prefix(obs_object_prefix or "")
    normalized_access_key = non_empty_string(obs_access_key)
    normalized_secret_key = non_empty_string(obs_secret_key)
    normalized_security_token = non_empty_string(obs_security_token)
    normalized_dsn = non_empty_string(gaussdb_dsn)

    if enabled:
        validate_huawei_obs_endpoint(normalized_endpoint)
        if not normalized_bucket:
            raise ValueError("LOGAGENT_V2_HUAWEI_OBS_BUCKET is required when enabled")
        if not is_valid_huawei_bucket_name(normalized_bucket):
            raise ValueError("LOGAGENT_V2_HUAWEI_OBS_BUCKET contains unsupported characters")
        if not normalized_access_key:
            raise ValueError("LOGAGENT_V2_HUAWEI_OBS_ACCESS_KEY is required when enabled")
        if not normalized_secret_key:
            raise ValueError("LOGAGENT_V2_HUAWEI_OBS_SECRET_KEY is required when enabled")
        if not normalized_dsn:
            raise ValueError("LOGAGENT_V2_HUAWEI_GAUSSDB_DSN is required when enabled")

    return HuaweiPackageSyncSettings(
        enabled=enabled,
        obs_endpoint=normalized_endpoint or None,
        obs_bucket=normalized_bucket or None,
        obs_object_prefix=normalized_prefix,
        obs_access_key=normalized_access_key,
        obs_secret_key=normalized_secret_key,
        obs_security_token=normalized_security_token,
        gaussdb_dsn=normalized_dsn,
        timeout_seconds=max(1, timeout_seconds),
    )


def non_empty_string(value: str | None) -> str | None:
    if value is None:
        return None
    normalized = value.strip()
    return normalized or None


def parse_agent_provider_env(raw: str | None) -> str:
    provider = (raw or "stub").strip().lower()
    if provider not in {"stub", "openai_compatible", "binary", "claude_code"}:
        raise ValueError(
            "LOGAGENT_V2_AGENT_PROVIDER must be one of stub, openai_compatible, "
            "binary, or claude_code"
        )
    return provider


def parse_agent_binary_path_env(raw: str | None, *, enabled: bool) -> Path | None:
    value = non_empty_string(raw)
    if not value:
        if enabled:
            raise ValueError("LOGAGENT_V2_AGENT_BINARY_PATH is required for binary provider")
        return None
    path = Path(os.path.expandvars(os.path.expanduser(value)))
    if enabled and not path.is_absolute():
        raise ValueError("LOGAGENT_V2_AGENT_BINARY_PATH must resolve to an absolute path")
    return path


def parse_claude_code_path_env(raw: str | None, *, enabled: bool) -> Path | None:
    value = non_empty_string(raw)
    if not value:
        if enabled:
            raise ValueError(
                "LOGAGENT_V2_CLAUDE_CODE_PATH or LOGAGENT_CLAUDE_CODE_PATH "
                "is required for claude_code provider"
            )
        return None
    path = Path(os.path.expandvars(os.path.expanduser(value)))
    if enabled and not path.is_absolute():
        raise ValueError("LOGAGENT_V2_CLAUDE_CODE_PATH must resolve to an absolute path")
    return path


def parse_csv_env(raw: str | None, *, default: tuple[str, ...]) -> tuple[str, ...]:
    value = non_empty_string(raw)
    if not value:
        return default
    return tuple(item.strip() for item in value.split(",") if item.strip())


def parse_claude_code_permission_profiles_env(
    raw: str | None,
    *,
    legacy_permission_mode: str,
    legacy_tools: str,
    legacy_allowed_tools: tuple[str, ...],
    legacy_disallowed_tools: tuple[str, ...],
    legacy_env_present: bool,
) -> dict[str, ClaudeCodePermissionProfile]:
    profiles = default_claude_code_permission_profiles()
    value = non_empty_string(raw)
    if value:
        decoded = json.loads(value)
        if not isinstance(decoded, dict):
            raise ValueError(
                "LOGAGENT_V2_CLAUDE_CODE_PERMISSION_PROFILES_JSON must be an object"
            )
        for raw_mode, raw_profile in decoded.items():
            mode = normalize_analysis_mode(raw_mode)
            if not isinstance(raw_profile, dict):
                raise ValueError(f"Claude Code permission profile {mode} must be an object")
            profiles[mode] = parse_claude_code_permission_profile(
                mode,
                raw_profile,
                profiles[mode],
            )
    if legacy_env_present:
        base = profiles["diagnose"]
        profiles["diagnose"] = ClaudeCodePermissionProfile(
            name=base.name,
            permission_mode=legacy_permission_mode,
            tools=legacy_tools,
            allowed_tools=with_logagent_mcp_allowed_tools(legacy_allowed_tools),
            disallowed_tools=legacy_disallowed_tools,
            native_bash=base.native_bash,
            native_edit=base.native_edit,
            worktree_required=base.worktree_required,
        )
    return profiles


def parse_claude_code_permission_profile(
    mode: str,
    raw: dict[str, Any],
    base: ClaudeCodePermissionProfile,
) -> ClaudeCodePermissionProfile:
    permission_mode = (
        optional_profile_string(
            raw.get("permissionMode") if "permissionMode" in raw else raw.get("permission_mode"),
            f"Claude Code permission profile {mode}.permissionMode",
        )
        or base.permission_mode
    )
    tools = raw.get("tools")
    if tools is None:
        tools = base.tools
    if not isinstance(tools, str):
        raise ValueError(f"Claude Code permission profile {mode}.tools must be a string")
    allowed_raw = raw.get("allowedTools")
    if allowed_raw is None:
        allowed_raw = raw.get("allowed_tools")
    disallowed_raw = raw.get("disallowedTools")
    if disallowed_raw is None:
        disallowed_raw = raw.get("disallowed_tools")
    return ClaudeCodePermissionProfile(
        name=optional_profile_string(
            raw.get("name"),
            f"Claude Code permission profile {mode}.name",
        )
        or base.name
        or mode,
        permission_mode=permission_mode,
        tools=tools.strip(),
        allowed_tools=with_logagent_mcp_allowed_tools(
            tuple_from_string_list(allowed_raw, default=base.allowed_tools)
        ),
        disallowed_tools=tuple_from_string_list(
            disallowed_raw,
            default=base.disallowed_tools,
        ),
        native_bash=profile_bool(
            raw,
            "nativeBash",
            "native_bash",
            base.native_bash,
            mode,
        ),
        native_edit=profile_bool(
            raw,
            "nativeEdit",
            "native_edit",
            base.native_edit,
            mode,
        ),
        worktree_required=profile_bool(
            raw,
            "worktreeRequired",
            "worktree_required",
            base.worktree_required,
            mode,
        ),
    )


def normalize_analysis_mode(value: object) -> str:
    if not isinstance(value, str) or value.strip() not in ANALYSIS_MODES:
        raise ValueError(
            "analysisMode must be one of diagnose, code_investigation, or fix"
        )
    return value.strip()


def optional_profile_string(value: object, field_name: str) -> str | None:
    if value is None:
        return None
    if not isinstance(value, str):
        raise ValueError(f"{field_name} must be a string")
    return non_empty_string(value)


def profile_bool(
    raw: dict[str, Any],
    camel_name: str,
    snake_name: str,
    default: bool,
    mode: str,
) -> bool:
    value = raw.get(camel_name, raw.get(snake_name, default))
    if not isinstance(value, bool):
        raise ValueError(f"Claude Code permission profile {mode}.{camel_name} must be boolean")
    return value


def tuple_from_string_list(
    value: object,
    *,
    default: tuple[str, ...],
) -> tuple[str, ...]:
    if value is None:
        return default
    if not isinstance(value, list) or not all(isinstance(item, str) for item in value):
        raise ValueError("Claude Code permission profile tool lists must be string arrays")
    return tuple(item.strip() for item in value if item.strip())


def parse_pprof_go_command_env(raw: str | None, *, enabled: bool) -> str | None:
    command = non_empty_string(raw)
    if not command:
        if enabled:
            raise ValueError("LOGAGENT_V2_PPROF_GO_COMMAND is required when pprof is enabled")
        return None
    command = expand_tool_command(command)
    if enabled and not Path(command).is_absolute():
        raise ValueError("LOGAGENT_V2_PPROF_GO_COMMAND must resolve to an absolute path")
    return command


def validate_agent_provider_settings(
    provider: str,
    *,
    agent_base_url: str | None,
    agent_model: str | None,
    agent_api_key: str | None,
    agent_binary_path: Path | None,
    claude_code_path: Path | None,
) -> None:
    if provider == "openai_compatible":
        if not agent_base_url:
            raise ValueError(
                "LOGAGENT_V2_AGENT_BASE_URL is required for openai_compatible provider"
            )
        if not agent_model:
            raise ValueError(
                "LOGAGENT_V2_AGENT_MODEL is required for openai_compatible provider"
            )
        if not agent_api_key:
            raise ValueError(
                "LOGAGENT_V2_AGENT_API_KEY is required for openai_compatible provider"
            )
    if provider == "binary" and agent_binary_path is None:
        raise ValueError("LOGAGENT_V2_AGENT_BINARY_PATH is required for binary provider")
    if provider == "claude_code" and claude_code_path is None:
        raise ValueError(
            "LOGAGENT_V2_CLAUDE_CODE_PATH or LOGAGENT_CLAUDE_CODE_PATH "
            "is required for claude_code provider"
        )


def validate_huawei_obs_endpoint(endpoint: str) -> None:
    if not endpoint:
        raise ValueError("LOGAGENT_V2_HUAWEI_OBS_ENDPOINT is required when enabled")
    parsed = urllib.parse.urlsplit(endpoint)
    if parsed.scheme not in {"http", "https"}:
        raise ValueError("LOGAGENT_V2_HUAWEI_OBS_ENDPOINT must use http or https")
    if not parsed.hostname:
        raise ValueError("LOGAGENT_V2_HUAWEI_OBS_ENDPOINT must include host")
    if parsed.path not in {"", "/"}:
        raise ValueError("LOGAGENT_V2_HUAWEI_OBS_ENDPOINT must not include a path")
    if parsed.username or parsed.password or parsed.query or parsed.fragment:
        raise ValueError(
            "LOGAGENT_V2_HUAWEI_OBS_ENDPOINT must not include credentials, query, or fragment"
        )


def normalize_huawei_object_prefix(raw: str) -> str:
    trimmed = raw.strip().strip("/")
    if not trimmed:
        return ""
    validate_huawei_object_key(trimmed)
    return trimmed


def validate_huawei_object_key(value: str) -> None:
    trimmed = value.strip()
    if not trimmed:
        raise ValueError("Huawei OBS object key must not be empty")
    if len(trimmed) > 1024:
        raise ValueError("Huawei OBS object key must be at most 1024 bytes")
    if (
        trimmed.startswith("/")
        or "\\" in trimmed
        or "?" in trimmed
        or "#" in trimmed
    ):
        raise ValueError("Huawei OBS object key must be relative")
    if any(part in {"", ".", ".."} for part in trimmed.split("/")):
        raise ValueError("Huawei OBS object key must not contain unsafe path segments")
    if any(ord(char) < 32 for char in trimmed):
        raise ValueError("Huawei OBS object key must not contain control characters")


def is_valid_huawei_bucket_name(value: str) -> bool:
    return bool(value) and len(value) <= 255 and all(
        char.isascii() and (char.isalnum() or char in {".", "-"})
        for char in value
    )


def parse_fetch_allowed_hosts_env(raw: str | None, *, enabled: bool) -> tuple[str, ...]:
    entries = [
        parse_fetch_allowed_host(item)
        for item in (raw or "").split(",")
        if item.strip()
    ]
    if enabled and not entries:
        raise ValueError("LOGAGENT_V2_FETCH_ALLOWED_HOSTS must not be empty when Fetch is enabled")
    return tuple(entries)


def parse_fetch_allowed_host(raw: str) -> str:
    value = raw.strip()
    if not value:
        raise ValueError("fetch allowed host entries must not be empty")
    if "://" in value:
        parsed = urllib.parse.urlsplit(value)
        if parsed.scheme not in {"http", "https"}:
            raise ValueError("fetch allowed host scheme must be http or https")
        host = parsed.hostname
        if not host:
            raise ValueError("fetch allowed host URL must include host")
        host = host.lower()
        if host == "*":
            raise ValueError("fetch allowed host must be an explicit host")
        try:
            port = parsed.port
        except ValueError as error:
            raise ValueError(f"invalid fetch allowed host port in {raw}") from error
        if port is None:
            port = 443 if parsed.scheme == "https" else 80
        return f"{parsed.scheme}://{format_fetch_host(host)}:{port}"

    host, port = split_fetch_host_port(value)
    host = host.strip().lower()
    if not host or host == "*":
        raise ValueError("fetch allowed host must be an explicit host")
    if port is None:
        return host
    return f"{host}:{port}"


def parse_fetch_secret_key_env(raw: str | None, *, enabled: bool) -> str | None:
    secret_key = raw.strip() if raw else ""
    if not secret_key:
        if enabled:
            raise ValueError("LOGAGENT_V2_FETCH_SECRET_KEY is required when Fetch is enabled")
        return None
    if enabled:
        validate_fetch_secret_key(secret_key)
    return secret_key


def validate_fetch_secret_key(secret_key: str) -> None:
    try:
        decoded = base64.b64decode(
            secret_key.encode("ascii"),
            altchars=b"-_",
            validate=True,
        )
    except (binascii.Error, UnicodeEncodeError, ValueError) as error:
        raise ValueError("LOGAGENT_V2_FETCH_SECRET_KEY must be a valid base64 key") from error
    if len(decoded) != 32:
        raise ValueError("LOGAGENT_V2_FETCH_SECRET_KEY must decode to 32 bytes")


def split_fetch_host_port(value: str) -> tuple[str, int | None]:
    if ":" not in value:
        return value, None
    host, port_text = value.rsplit(":", 1)
    if ":" in host:
        return value, None
    try:
        port = int(port_text)
    except ValueError as error:
        raise ValueError(f"invalid fetch allowed host port in {value}") from error
    if port < 0 or port > 65535:
        raise ValueError(f"invalid fetch allowed host port in {value}")
    return host, port


def format_fetch_host(host: str) -> str:
    if ":" in host and not (host.startswith("[") and host.endswith("]")):
        return f"[{host}]"
    return host


def parse_remote_commands_env(raw: str | None) -> tuple[RemoteCommandTemplate, ...]:
    if not raw:
        return default_remote_commands()
    decoded = json.loads(raw)
    if not isinstance(decoded, list):
        raise ValueError("LOGAGENT_V2_REMOTE_COMMANDS_JSON must be a JSON array")
    return tuple(RemoteCommandTemplate.from_json(item) for item in decoded)


def parse_remote_files_env(raw: str | None) -> tuple[RemoteFileTemplate, ...]:
    if not raw:
        return ()
    decoded = json.loads(raw)
    if not isinstance(decoded, list):
        raise ValueError("LOGAGENT_V2_REMOTE_FILES_JSON must be a JSON array")
    if not all(isinstance(item, dict) for item in decoded):
        raise ValueError("LOGAGENT_V2_REMOTE_FILES_JSON entries must be objects")
    return tuple(RemoteFileTemplate.from_json(item) for item in decoded)


def strings_from_list(value: Any) -> list[str]:
    if not isinstance(value, list):
        return []
    return [str(item).lower() for item in value if str(item).strip()]


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
