from __future__ import annotations

import json
import subprocess
import time
from pathlib import Path
from typing import Any

from .config import RemoteCommandTemplate, Settings
from .store import JsonObject, Store


def command_templates(settings: Settings) -> list[JsonObject]:
    return [public_command_template(template) for template in settings.remote_commands]


def command_template(settings: Settings, command_id: str) -> RemoteCommandTemplate | None:
    for template in settings.remote_commands:
        if template.command_id == command_id:
            return template
    return None


def public_command_template(template: RemoteCommandTemplate) -> JsonObject:
    return {
        "commandId": template.command_id,
        "displayName": template.display_name,
        "description": template.description,
        "enabled": template.enabled,
        "argv": list(template.argv),
        "timeoutSeconds": template.timeout_seconds,
    }


def execute_remote_command_run(settings: Settings, store: Store, run_id: str) -> JsonObject:
    if not settings.remote_execution_enabled:
        raise ValueError("remote execution is disabled")
    run = store.get_remote_run(run_id)
    executor = store.get_remote_executor(run["remoteExecutorId"])
    if not executor["enabled"]:
        raise ValueError(f"executor {executor['executorId']} is disabled")
    template = command_template(settings, run["remoteCommandId"])
    if template is None:
        raise ValueError(f"unknown commandId {run['remoteCommandId']}")
    if not template.enabled:
        raise ValueError(f"remote command {template.command_id} is disabled")
    if not template.argv:
        raise ValueError(f"remote command {template.command_id} has empty argv")

    store.mark_remote_run_running(run_id, "EXECUTE_REMOTE_COMMAND")
    result_dir = settings.data_dir / "remote_runs" / run_id / "remote_command"
    result_dir.mkdir(parents=True, exist_ok=True)
    stdout_path = result_dir / "stdout.txt"
    stderr_path = result_dir / "stderr.txt"
    result_path = result_dir / "result.json"

    timeout_seconds = template.timeout_seconds or settings.remote_command_timeout_seconds
    command_argv = build_ssh_argv(settings, executor, template)
    started_at = now_seconds()
    started = time.monotonic()
    try:
        completed = subprocess.run(
            command_argv,
            capture_output=True,
            timeout=timeout_seconds,
            check=False,
        )
        status = "OK" if completed.returncode == 0 else "FAILED"
        exit_code: int | None = completed.returncode
        stdout = completed.stdout
        stderr = completed.stderr
        error = None
    except subprocess.TimeoutExpired:
        status = "TIMED_OUT"
        exit_code = None
        stdout = b""
        stderr = b""
        error = f"remote command timed out after {timeout_seconds}s"
    except OSError as exc:
        status = "FAILED"
        exit_code = None
        stdout = b""
        stderr = b""
        error = f"failed to start ssh: {exc}"
    completed_at = now_seconds()

    stdout, stdout_truncated = cap_output(stdout, settings.remote_max_output_bytes)
    stderr, stderr_truncated = cap_output(stderr, settings.remote_max_output_bytes)
    warnings = []
    if stdout_truncated:
        warnings.append(f"stdout truncated to {settings.remote_max_output_bytes} bytes")
    if stderr_truncated:
        warnings.append(f"stderr truncated to {settings.remote_max_output_bytes} bytes")
    stdout_path.write_bytes(stdout)
    stderr_path.write_bytes(stderr)

    result = {
        "schemaVersion": 1,
        "executorId": executor["executorId"],
        "executorName": executor["name"],
        "host": executor["host"],
        "port": executor["port"],
        "user": executor["user"],
        "commandId": template.command_id,
        "commandDisplayName": template.display_name,
        "status": status,
        "exitCode": exit_code,
        "durationMs": int((time.monotonic() - started) * 1000),
        "commandArgv": list(template.argv),
        "sshArgvPreview": redact_ssh_argv(command_argv),
        "stdoutPath": relative_path(settings, stdout_path),
        "stderrPath": relative_path(settings, stderr_path),
        "stdoutPreview": preview(stdout),
        "stderrPreview": preview(stderr),
        "warnings": warnings,
        "error": error,
        "startedAt": started_at,
        "completedAt": completed_at,
    }
    result_path.write_text(json.dumps(result, ensure_ascii=True, indent=2), encoding="utf-8")
    response = {
        "taskId": run_id,
        "executorId": executor["executorId"],
        "commandId": template.command_id,
        "resultPath": relative_path(settings, result_path),
        "result": result,
    }
    store.complete_remote_run(run_id, response)
    return response


def build_ssh_argv(
    settings: Settings,
    executor: JsonObject,
    template: RemoteCommandTemplate,
) -> list[str]:
    return [
        settings.remote_ssh_command,
        "-o",
        "BatchMode=yes",
        "-o",
        f"ConnectTimeout={settings.remote_connect_timeout_seconds}",
        "-o",
        f"StrictHostKeyChecking={strict_host_key_checking_value(settings.remote_host_key_policy)}",
        "-p",
        str(executor["port"]),
        f"{executor['user']}@{executor['host']}",
        *template.argv,
    ]


def strict_host_key_checking_value(policy: str) -> str:
    normalized = policy.strip().lower()
    if normalized == "strict":
        return "yes"
    if normalized in {"off", "no", "disabled"}:
        return "no"
    return "accept-new"


def cap_output(value: bytes, max_bytes: int) -> tuple[bytes, bool]:
    if len(value) <= max_bytes:
        return value, False
    return value[:max_bytes], True


def preview(value: bytes) -> str:
    return value[:8192].decode("utf-8", errors="replace")


def relative_path(settings: Settings, path: Path) -> str:
    return path.resolve().relative_to(settings.data_dir.resolve()).as_posix()


def redact_ssh_argv(argv: list[str]) -> list[str]:
    # SSH argv does not include secret material; keep this as a separate function
    # so future identity-file support has a single redaction point.
    return list(argv)


def now_seconds() -> str:
    from .store import now_iso

    return now_iso()
