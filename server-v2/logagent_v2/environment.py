from __future__ import annotations

from pathlib import Path

from .artifacts import resolve_artifact_path, write_artifact_file
from .config import Settings
from .evidence import write_json_artifact
from .store import JsonObject, Store, now_iso


ENVIRONMENT_ACTION_TYPE = "collect_environment"
ENVIRONMENT_EVIDENCE_KIND = "environment_evidence"
ENVIRONMENT_REMOTE_IDEMPOTENCY_PREFIX = "environment:"


def persist_approved_environment_evidence(
    settings: Settings,
    store: Store,
    action: JsonObject,
) -> JsonObject | None:
    """Record or schedule environment evidence after approval.

    If the approved action includes a configured Remote Executor target, V2
    schedules the whitelisted remote command and records evidence when that
    remote job finishes. Otherwise it preserves the V1-compatible MOCK marker.
    """

    if action.get("kind") != "approval" or action.get("status") != "approved":
        return None
    payload = action.get("payload") or {}
    if payload.get("actionType") != ENVIRONMENT_ACTION_TYPE:
        return None

    run_id = action["run_id"]
    existing = existing_environment_evidence(store, run_id, action["id"])
    if existing is not None:
        return existing

    raw_input = environment_action_input(payload)
    remote_requested = remote_target_requested(raw_input)
    if remote_requested and not settings.remote_execution_enabled:
        return persist_environment_evidence_result(
            settings=settings,
            store=store,
            action=action,
            status="REMOTE_REJECTED",
            summary="remote environment collection rejected: remote execution is disabled",
            result={"input": raw_input, "error": "remote execution is disabled"},
        )
    try:
        remote_target = remote_collection_target(settings, store, raw_input)
    except (KeyError, ValueError) as error:
        return persist_environment_evidence_result(
            settings=settings,
            store=store,
            action=action,
            status="REMOTE_REJECTED",
            summary=f"remote environment collection rejected: {error}",
            result={"input": raw_input, "error": str(error)},
        )
    if remote_requested and remote_target is None:
        return persist_environment_evidence_result(
            settings=settings,
            store=store,
            action=action,
            status="REMOTE_REJECTED",
            summary="remote environment collection rejected: executorId and commandId are required",
            result={"input": raw_input, "error": "executorId and commandId are required"},
        )
    if remote_target is not None:
        return schedule_remote_environment_collection(settings, store, action, remote_target)

    return persist_environment_evidence_result(
        settings=settings,
        store=store,
        action=action,
        status="MOCK",
        summary="mock environment evidence captured after user approval",
        result={"input": raw_input},
    )


def persist_remote_environment_evidence(
    settings: Settings,
    store: Store,
    remote_run: JsonObject,
) -> JsonObject | None:
    idempotency_key = remote_run.get("idempotencyKey")
    if not isinstance(idempotency_key, str) or not idempotency_key.startswith(
        ENVIRONMENT_REMOTE_IDEMPOTENCY_PREFIX
    ):
        return None
    action_id = idempotency_key.removeprefix(ENVIRONMENT_REMOTE_IDEMPOTENCY_PREFIX)
    if not action_id:
        return None
    action = store.get_action(action_id)
    if action.get("kind") != "approval" or action.get("status") != "approved":
        return None
    payload = action.get("payload") or {}
    if payload.get("actionType") != ENVIRONMENT_ACTION_TYPE:
        return None

    existing = existing_environment_evidence(store, action["run_id"], action_id)
    if existing is not None:
        return existing

    remote_result = remote_run.get("result")
    command_result = remote_result.get("result") if isinstance(remote_result, dict) else None
    if not isinstance(command_result, dict):
        command_result = {}
    remote_status = str(command_result.get("status") or remote_run.get("status") or "UNKNOWN")
    status = "COLLECTED" if remote_status == "OK" else "REMOTE_FAILED"
    summary = (
        "remote environment evidence collected"
        if status == "COLLECTED"
        else f"remote environment collection finished with status {remote_status}"
    )

    run = store.get_run(action["run_id"])
    support_artifacts = materialize_remote_environment_support_artifacts(
        settings=settings,
        store=store,
        workspace_id=run["workspace_id"],
        action_id=action["id"],
        remote_result=remote_result if isinstance(remote_result, dict) else {},
        command_result=command_result,
    )
    warnings = command_result.get("warnings", [])
    if not isinstance(warnings, list):
        warnings = []
    warnings = [*warnings, *support_artifacts["warnings"]]

    result: JsonObject = {
        "input": payload.get("input") if isinstance(payload.get("input"), dict) else {},
        "remoteRunId": remote_run.get("taskId"),
        "remoteExecutorId": remote_run.get("remoteExecutorId"),
        "remoteCommandId": remote_run.get("remoteCommandId"),
        "remoteStatus": remote_status,
        "remoteResultPath": remote_result.get("resultPath")
        if isinstance(remote_result, dict)
        else None,
        "stdoutPath": command_result.get("stdoutPath"),
        "stderrPath": command_result.get("stderrPath"),
        "stdoutPreview": command_result.get("stdoutPreview"),
        "stderrPreview": command_result.get("stderrPreview"),
        "error": command_result.get("error") or remote_run.get("error"),
        "warnings": warnings,
    }
    if support_artifacts["artifactIds"]:
        result["artifactIds"] = support_artifacts["artifactIds"]
        result["artifactPaths"] = support_artifacts["artifactPaths"]

    evidence = persist_environment_evidence_result(
        settings=settings,
        store=store,
        action=action,
        status=status,
        summary=summary,
        result=result,
    )

    analysis_run = store.get_run(action["run_id"])
    if analysis_run.get("status") not in {"succeeded", "failed"}:
        store.update_run_status(action["run_id"], "queued", "queued")
        store.enqueue_run(action["run_id"])
    return evidence


def is_pending_environment_collection(value: JsonObject | None) -> bool:
    if not isinstance(value, dict):
        return False
    payload = value.get("payload")
    return isinstance(payload, dict) and payload.get("status") == "QUEUED"


def remote_collection_target(
    settings: Settings,
    store: Store,
    raw_input: JsonObject,
) -> JsonObject | None:
    executor_id = raw_input.get("executorId") or raw_input.get("remoteExecutorId")
    command_id = raw_input.get("commandId") or raw_input.get("remoteCommandId")
    if not isinstance(executor_id, str) or not executor_id.strip():
        return None
    if not isinstance(command_id, str) or not command_id.strip():
        return None
    executor = store.get_remote_executor(executor_id.strip())
    if not executor["enabled"]:
        raise ValueError(f"executor {executor_id} is disabled")
    command = next(
        (
            item
            for item in settings.remote_commands
            if item.command_id == command_id.strip()
        ),
        None,
    )
    if command is None:
        raise ValueError(f"unknown commandId {command_id}")
    if not command.enabled:
        raise ValueError(f"remote command {command_id} is disabled")
    return {
        "executorId": executor["executorId"],
        "executorName": executor["name"],
        "commandId": command.command_id,
        "commandDisplayName": command.display_name,
    }


def environment_action_input(payload: JsonObject) -> JsonObject:
    raw_input = payload.get("input")
    return raw_input if isinstance(raw_input, dict) else {}


def remote_target_requested(raw_input: JsonObject) -> bool:
    return any(
        key in raw_input
        for key in ("executorId", "remoteExecutorId", "commandId", "remoteCommandId")
    )


def materialize_remote_environment_support_artifacts(
    settings: Settings,
    store: Store,
    workspace_id: str,
    action_id: str,
    remote_result: JsonObject,
    command_result: JsonObject,
) -> JsonObject:
    artifact_ids: JsonObject = {}
    artifact_paths: JsonObject = {}
    warnings: list[str] = []
    specs = [
        (
            "result",
            "resultPath",
            remote_result.get("resultPath"),
            "remote_result.json",
            "application/json",
            "logagent.v2.remote_command_result.v1",
        ),
        (
            "stdout",
            "stdoutPath",
            command_result.get("stdoutPath"),
            "stdout.txt",
            "text/plain; charset=utf-8",
            None,
        ),
        (
            "stderr",
            "stderrPath",
            command_result.get("stderrPath"),
            "stderr.txt",
            "text/plain; charset=utf-8",
            None,
        ),
    ]
    for role, path_field, source_relative, filename, content_type, schema_name in specs:
        if not isinstance(source_relative, str) or not source_relative:
            continue
        logical_path = f"environment_evidence/{action_id}/{filename}"
        source_path = resolve_remote_artifact_source(settings, source_relative, warnings, role)
        if source_path is None:
            continue
        artifact = write_artifact_file(
            settings=settings,
            store=store,
            workspace_id=workspace_id,
            filename=f"{action_id}_{filename}",
            source_path=source_path,
            content_type=content_type,
            schema_name=schema_name,
            preview={
                "actionId": action_id,
                "role": role,
                "path": logical_path,
                "sourcePath": source_relative,
            },
        )
        artifact_ids[role] = artifact["id"]
        artifact_paths[path_field] = logical_path
    return {"artifactIds": artifact_ids, "artifactPaths": artifact_paths, "warnings": warnings}


def resolve_remote_artifact_source(
    settings: Settings,
    source_relative: str,
    warnings: list[str],
    role: str,
) -> Path | None:
    try:
        source_path = resolve_artifact_path(settings, source_relative)
    except ValueError as error:
        warnings.append(f"remote {role} artifact path rejected: {error}")
        return None
    if not source_path.is_file():
        warnings.append(f"remote {role} artifact file is missing: {source_relative}")
        return None
    return source_path


def schedule_remote_environment_collection(
    settings: Settings,
    store: Store,
    action: JsonObject,
    target: JsonObject,
) -> JsonObject:
    run_id = action["run_id"]
    existing = existing_environment_evidence(store, run_id, action["id"])
    if existing is not None:
        return existing
    idempotency_key = f"{ENVIRONMENT_REMOTE_IDEMPOTENCY_PREFIX}{action['id']}"
    remote_run = store.create_remote_run(
        executor_id=target["executorId"],
        command_id=target["commandId"],
        alias=f"Collect environment via {target['commandDisplayName']}",
        idempotency_key=idempotency_key,
    )
    return {
        "kind": ENVIRONMENT_EVIDENCE_KIND,
        "final_allowed": False,
        "summary": "remote environment collection queued",
        "payload": {
            "actionId": action["id"],
            "status": "QUEUED",
            "remoteRunId": remote_run["taskId"],
            "remoteExecutorId": target["executorId"],
            "remoteCommandId": target["commandId"],
            "finalEvidenceAllowed": False,
        },
    }


def existing_environment_evidence(
    store: Store,
    run_id: str,
    action_id: str,
) -> JsonObject | None:
    for evidence in store.list_evidence(run_id):
        if (
            evidence.get("kind") == ENVIRONMENT_EVIDENCE_KIND
            and evidence.get("payload", {}).get("actionId") == action_id
        ):
            return evidence
    return None


def persist_environment_evidence_result(
    settings: Settings,
    store: Store,
    action: JsonObject,
    status: str,
    summary: str,
    result: JsonObject,
) -> JsonObject:
    run = store.get_run(action["run_id"])
    payload = action.get("payload") if isinstance(action.get("payload"), dict) else {}
    artifact_path = f"environment_evidence/{action['id']}/result.json"
    result = {
        "schemaVersion": 1,
        "actionId": action["id"],
        "status": status,
        "summary": summary,
        "input": payload.get("input") if isinstance(payload.get("input"), dict) else {},
        **result,
        "createdAt": now_iso(),
        "finalEvidenceAllowed": False,
    }
    artifact = write_json_artifact(
        settings=settings,
        store=store,
        workspace_id=run["workspace_id"],
        filename=f"{action['id']}_environment_result.json",
        value=result,
        schema_name="logagent.v2.environment_evidence.v1",
    )
    evidence_payload = {
        "artifactId": artifact["id"],
        "path": artifact_path,
        "actionId": action["id"],
        "status": status,
        "remoteRunId": result.get("remoteRunId"),
        "remoteExecutorId": result.get("remoteExecutorId"),
        "remoteCommandId": result.get("remoteCommandId"),
        "remoteStatus": result.get("remoteStatus"),
        "finalEvidenceAllowed": False,
    }
    if isinstance(result.get("artifactIds"), dict):
        evidence_payload["artifactIds"] = result["artifactIds"]
    if isinstance(result.get("artifactPaths"), dict):
        evidence_payload["artifactPaths"] = result["artifactPaths"]
    return store.create_evidence(
        workspace_id=run["workspace_id"],
        run_id=run["id"],
        kind=ENVIRONMENT_EVIDENCE_KIND,
        final_allowed=False,
        summary=summary,
        artifact_id=artifact["id"],
        payload=evidence_payload,
    )
