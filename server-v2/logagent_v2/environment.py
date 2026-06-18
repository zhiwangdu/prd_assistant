from __future__ import annotations

from pathlib import Path

from .artifacts import resolve_artifact_path, write_artifact_file
from .config import Settings
from .evidence import write_json_artifact
from .store import JsonObject, Store, now_iso


ENVIRONMENT_ACTION_TYPE = "collect_environment"
ENVIRONMENT_EVIDENCE_KIND = "environment_evidence"
ENVIRONMENT_REMOTE_IDEMPOTENCY_PREFIX = "environment:"
ENVIRONMENT_MAX_BATCH_TARGETS = 20


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
        remote_targets = remote_collection_targets(settings, store, raw_input)
    except (KeyError, ValueError) as error:
        return persist_environment_evidence_result(
            settings=settings,
            store=store,
            action=action,
            status="REMOTE_REJECTED",
            summary=f"remote environment collection rejected: {error}",
            result={"input": raw_input, "error": str(error)},
        )
    if remote_requested and not remote_targets:
        return persist_environment_evidence_result(
            settings=settings,
            store=store,
            action=action,
            status="REMOTE_REJECTED",
            summary=(
                "remote environment collection rejected: executorId and commandId "
                "or fileId are required"
            ),
            result={
                "input": raw_input,
                "error": "executorId and commandId or fileId are required",
            },
        )
    if remote_targets:
        if explicit_environment_targets(raw_input):
            return schedule_remote_environment_batch_collection(
                settings, store, action, remote_targets
            )
        return schedule_remote_environment_collection(
            settings, store, action, remote_targets[0]
        )

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
    parsed_key = parse_environment_remote_idempotency_key(idempotency_key)
    if parsed_key is None:
        return None
    action_id, target_index = parsed_key
    action = store.get_action(action_id)
    if action.get("kind") != "approval" or action.get("status") != "approved":
        return None
    payload = action.get("payload") or {}
    if payload.get("actionType") != ENVIRONMENT_ACTION_TYPE:
        return None
    if target_index is not None:
        return persist_batch_remote_environment_evidence(
            settings=settings,
            store=store,
            action=action,
            completed_remote_run=remote_run,
            completed_target_index=target_index,
        )

    existing = existing_environment_evidence(store, action["run_id"], action_id)
    if existing is not None:
        return existing

    target_result = remote_environment_target_result(
        settings=settings,
        store=store,
        action=action,
        remote_run=remote_run,
    )
    result = {
        "input": payload.get("input") if isinstance(payload.get("input"), dict) else {},
        **target_result["result"],
    }
    if target_result["artifactIds"]:
        result["artifactIds"] = target_result["artifactIds"]
        result["artifactPaths"] = target_result["artifactPaths"]

    evidence = persist_environment_evidence_result(
        settings=settings,
        store=store,
        action=action,
        status=target_result["status"],
        summary=target_result["summary"],
        result=result,
    )

    requeue_analysis_after_environment_collection(store, action)
    return evidence


def persist_batch_remote_environment_evidence(
    settings: Settings,
    store: Store,
    action: JsonObject,
    completed_remote_run: JsonObject,
    completed_target_index: int,
) -> JsonObject | None:
    existing = existing_environment_evidence(store, action["run_id"], action["id"])
    if existing is not None:
        return existing

    remote_input = completed_remote_run.get("input")
    batch_size = (
        remote_input.get("batchSize")
        if isinstance(remote_input, dict)
        else None
    )
    if not isinstance(batch_size, int) or batch_size < 1:
        batch_size = completed_target_index + 1

    remote_runs: list[JsonObject] = []
    pending_indices: list[int] = []
    for index in range(batch_size):
        remote_run = store.find_remote_run_by_idempotency_key(
            environment_remote_idempotency_key(action["id"], index)
        )
        if remote_run is None or remote_run.get("status") in {"QUEUED", "RUNNING"}:
            pending_indices.append(index)
            continue
        remote_runs.append(remote_run)
    if pending_indices:
        return {
            "kind": ENVIRONMENT_EVIDENCE_KIND,
            "final_allowed": False,
            "summary": "remote environment batch collection still running",
            "payload": {
                "actionId": action["id"],
                "status": "QUEUED",
                "targetCount": batch_size,
                "pendingTargetIndices": pending_indices,
                "finalEvidenceAllowed": False,
            },
        }

    payload = action.get("payload") if isinstance(action.get("payload"), dict) else {}
    collected = 0
    failed = 0
    target_results: list[JsonObject] = []
    artifact_ids: JsonObject = {}
    artifact_paths: JsonObject = {}
    warnings: list[str] = []
    for remote_run in sorted(remote_runs, key=remote_run_batch_target_index):
        input_payload = remote_run.get("input") if isinstance(remote_run.get("input"), dict) else {}
        target_index_value = input_payload.get("batchTargetIndex")
        target_index = target_index_value if isinstance(target_index_value, int) else len(target_results)
        target_result = remote_environment_target_result(
            settings=settings,
            store=store,
            action=action,
            remote_run=remote_run,
            target_index=target_index,
        )
        if target_result["status"] == "COLLECTED":
            collected += 1
        else:
            failed += 1
        target_results.append(target_result["result"])
        artifact_ids.update(target_result["artifactIds"])
        artifact_paths.update(target_result["artifactPaths"])
        warnings.extend(target_result["warnings"])

    if failed == 0:
        status = "COLLECTED"
    elif collected > 0:
        status = "PARTIALLY_COLLECTED"
    else:
        status = "REMOTE_FAILED"
    summary = (
        f"remote environment batch collected {collected}/{batch_size} targets"
        if status == "COLLECTED"
        else f"remote environment batch finished with {collected}/{batch_size} successful targets"
    )
    result: JsonObject = {
        "input": payload.get("input") if isinstance(payload.get("input"), dict) else {},
        "remoteOperation": "batch",
        "targetCount": batch_size,
        "completedTargetCount": len(remote_runs),
        "successfulTargetCount": collected,
        "failedTargetCount": failed,
        "targets": target_results,
        "remoteRunIds": [item.get("taskId") for item in remote_runs],
        "remoteStatusCounts": remote_status_counts(target_results),
        "warnings": warnings,
    }
    if artifact_ids:
        result["artifactIds"] = artifact_ids
        result["artifactPaths"] = artifact_paths

    evidence = persist_environment_evidence_result(
        settings=settings,
        store=store,
        action=action,
        status=status,
        summary=summary,
        result=result,
    )
    requeue_analysis_after_environment_collection(store, action)
    return evidence


def remote_environment_target_result(
    settings: Settings,
    store: Store,
    action: JsonObject,
    remote_run: JsonObject,
    target_index: int | None = None,
) -> JsonObject:
    remote_result = remote_run.get("result")
    command_result = remote_result.get("result") if isinstance(remote_result, dict) else None
    if not isinstance(command_result, dict):
        command_result = {}
    remote_operation = str(command_result.get("operation") or remote_run.get("operation") or "command")
    remote_status = str(command_result.get("status") or remote_run.get("status") or "UNKNOWN")
    status = "COLLECTED" if remote_status == "OK" else "REMOTE_FAILED"
    summary = (
        "remote environment evidence collected"
        if status == "COLLECTED"
        else f"remote environment collection finished with status {remote_status}"
    )

    support_artifacts = materialize_remote_environment_support_artifacts(
        settings=settings,
        store=store,
        workspace_id=store.get_run(action["run_id"])["workspace_id"],
        action_id=action["id"],
        remote_result=remote_result if isinstance(remote_result, dict) else {},
        command_result=command_result,
        target_index=target_index,
    )
    warnings = command_result.get("warnings", [])
    if not isinstance(warnings, list):
        warnings = []
    warnings = [*warnings, *support_artifacts["warnings"]]

    result: JsonObject = {
        "remoteOperation": remote_operation,
        "remoteRunId": remote_run.get("taskId"),
        "remoteExecutorId": remote_run.get("remoteExecutorId"),
        "remoteCommandId": (
            remote_run.get("remoteCommandId") if remote_operation == "command" else None
        ),
        "remoteFileId": command_result.get("fileId"),
        "remoteStatus": remote_status,
        "remoteResultPath": remote_result.get("resultPath")
        if isinstance(remote_result, dict)
        else None,
        "stdoutPath": command_result.get("stdoutPath"),
        "stderrPath": command_result.get("stderrPath"),
        "collectedFilePath": command_result.get("collectedFilePath"),
        "fileSizeBytes": command_result.get("fileSizeBytes"),
        "sha256": command_result.get("sha256"),
        "stdoutPreview": command_result.get("stdoutPreview"),
        "stderrPreview": command_result.get("stderrPreview"),
        "error": command_result.get("error") or remote_run.get("error"),
        "warnings": warnings,
    }
    input_payload = remote_run.get("input")
    if isinstance(input_payload, dict):
        if isinstance(input_payload.get("batchTargetIndex"), int):
            result["targetIndex"] = input_payload["batchTargetIndex"]
        if isinstance(input_payload.get("batchSize"), int):
            result["batchSize"] = input_payload["batchSize"]
    return {
        "status": status,
        "summary": summary,
        "result": result,
        "artifactIds": support_artifacts["artifactIds"],
        "artifactPaths": support_artifacts["artifactPaths"],
        "warnings": warnings,
    }


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
    file_id = raw_input.get("fileId") or raw_input.get("remoteFileId")
    if not isinstance(executor_id, str) or not executor_id.strip():
        return None
    has_command = isinstance(command_id, str) and bool(command_id.strip())
    has_file = isinstance(file_id, str) and bool(file_id.strip())
    if has_command and has_file:
        raise ValueError("collect_environment input must choose either commandId or fileId")
    if not has_command and not has_file:
        return None
    executor = store.get_remote_executor(executor_id.strip())
    if not executor["enabled"]:
        raise ValueError(f"executor {executor_id} is disabled")
    if has_command:
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
            "operation": "command",
            "executorId": executor["executorId"],
            "executorName": executor["name"],
            "commandId": command.command_id,
            "commandDisplayName": command.display_name,
        }
    file_template = next(
        (
            item
            for item in settings.remote_files
            if item.file_id == str(file_id).strip()
        ),
        None,
    )
    if file_template is None:
        raise ValueError(f"unknown fileId {file_id}")
    if not file_template.enabled:
        raise ValueError(f"remote file {file_id} is disabled")
    return {
        "operation": "file_collection",
        "executorId": executor["executorId"],
        "executorName": executor["name"],
        "fileId": file_template.file_id,
        "fileDisplayName": file_template.display_name,
    }


def remote_collection_targets(
    settings: Settings,
    store: Store,
    raw_input: JsonObject,
) -> list[JsonObject]:
    if explicit_environment_targets(raw_input):
        raw_targets = raw_input.get("targets")
        if raw_targets is None:
            raw_targets = raw_input.get("remoteTargets")
        if not isinstance(raw_targets, list):
            raise ValueError("collect_environment targets must be an array")
        if not raw_targets:
            raise ValueError("collect_environment targets must not be empty")
        if len(raw_targets) > ENVIRONMENT_MAX_BATCH_TARGETS:
            raise ValueError(
                f"collect_environment targets exceed maximum {ENVIRONMENT_MAX_BATCH_TARGETS}"
            )
        targets: list[JsonObject] = []
        for index, item in enumerate(raw_targets):
            if not isinstance(item, dict):
                raise ValueError(f"collect_environment target {index} must be an object")
            target_input = dict(item)
            for key in ("executorId", "remoteExecutorId"):
                if key not in target_input and key in raw_input:
                    target_input[key] = raw_input[key]
            target = remote_collection_target(settings, store, target_input)
            if target is None:
                raise ValueError(
                    f"collect_environment target {index} requires executorId and commandId or fileId"
                )
            target["targetIndex"] = index
            target["input"] = target_input
            targets.append(target)
        return targets

    target = remote_collection_target(settings, store, raw_input)
    return [target] if target is not None else []


def environment_action_input(payload: JsonObject) -> JsonObject:
    raw_input = payload.get("input")
    return raw_input if isinstance(raw_input, dict) else {}


def explicit_environment_targets(raw_input: JsonObject) -> bool:
    return "targets" in raw_input or "remoteTargets" in raw_input


def remote_target_requested(raw_input: JsonObject) -> bool:
    return any(
        key in raw_input
        for key in (
            "targets",
            "remoteTargets",
            "executorId",
            "remoteExecutorId",
            "commandId",
            "remoteCommandId",
            "fileId",
            "remoteFileId",
        )
    )


def materialize_remote_environment_support_artifacts(
    settings: Settings,
    store: Store,
    workspace_id: str,
    action_id: str,
    remote_result: JsonObject,
    command_result: JsonObject,
    target_index: int | None = None,
) -> JsonObject:
    artifact_ids: JsonObject = {}
    artifact_paths: JsonObject = {}
    warnings: list[str] = []
    logical_prefix = f"environment_evidence/{action_id}"
    role_prefix = ""
    filename_prefix = action_id
    if target_index is not None:
        logical_prefix = f"{logical_prefix}/targets/{target_index}"
        role_prefix = f"target{target_index}_"
        filename_prefix = f"{action_id}_target{target_index}"
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
        (
            "collected_file",
            "collectedFilePath",
            command_result.get("collectedFilePath"),
            "collected_file.bin",
            "application/octet-stream",
            None,
        ),
    ]
    for role, path_field, source_relative, filename, content_type, schema_name in specs:
        if not isinstance(source_relative, str) or not source_relative:
            continue
        logical_path = f"{logical_prefix}/{filename}"
        artifact_role = f"{role_prefix}{role}"
        artifact_path_field = path_field if target_index is None else f"{artifact_role}Path"
        source_path = resolve_remote_artifact_source(settings, source_relative, warnings, role)
        if source_path is None:
            continue
        artifact = write_artifact_file(
            settings=settings,
            store=store,
            workspace_id=workspace_id,
            filename=f"{filename_prefix}_{filename}",
            source_path=source_path,
            content_type=content_type,
            schema_name=schema_name,
            preview={
                "actionId": action_id,
                "role": artifact_role,
                "path": logical_path,
                "sourcePath": source_relative,
                "targetIndex": target_index,
            },
        )
        artifact_ids[artifact_role] = artifact["id"]
        artifact_paths[artifact_path_field] = logical_path
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
    remote_run = create_environment_remote_run(store, action, target)
    return pending_environment_collection_payload(action, target, remote_run)


def schedule_remote_environment_batch_collection(
    settings: Settings,
    store: Store,
    action: JsonObject,
    targets: list[JsonObject],
) -> JsonObject:
    run_id = action["run_id"]
    existing = existing_environment_evidence(store, run_id, action["id"])
    if existing is not None:
        return existing
    remote_runs = [
        create_environment_remote_run(
            store,
            action,
            target,
            target_index=index,
            batch_size=len(targets),
        )
        for index, target in enumerate(targets)
    ]
    return {
        "kind": ENVIRONMENT_EVIDENCE_KIND,
        "final_allowed": False,
        "summary": "remote environment batch collection queued",
        "payload": {
            "actionId": action["id"],
            "status": "QUEUED",
            "remoteOperation": "batch",
            "targetCount": len(targets),
            "remoteRunIds": [item["taskId"] for item in remote_runs],
            "targets": [
                {
                    "targetIndex": index,
                    "remoteRunId": remote_runs[index]["taskId"],
                    "remoteExecutorId": target["executorId"],
                    "remoteCommandId": target.get("commandId"),
                    "remoteFileId": target.get("fileId"),
                    "remoteOperation": target.get("operation") or "command",
                }
                for index, target in enumerate(targets)
            ],
            "finalEvidenceAllowed": False,
        },
    }


def create_environment_remote_run(
    store: Store,
    action: JsonObject,
    target: JsonObject,
    target_index: int | None = None,
    batch_size: int | None = None,
) -> JsonObject:
    operation = str(target.get("operation") or "command")
    command_id = str(target.get("commandId") or target.get("fileId") or "")
    input_payload: JsonObject = {
        "actionId": action["id"],
        "operation": operation,
        "commandId": target.get("commandId"),
        "fileId": target.get("fileId"),
    }
    if target_index is not None:
        input_payload["batchTargetIndex"] = target_index
    if batch_size is not None:
        input_payload["batchSize"] = batch_size
    return store.create_remote_run(
        executor_id=target["executorId"],
        command_id=command_id,
        alias=f"Collect environment via {target.get('commandDisplayName') or target.get('fileDisplayName')}",
        idempotency_key=environment_remote_idempotency_key(action["id"], target_index),
        operation=operation,
        input_payload=input_payload,
    )


def pending_environment_collection_payload(
    action: JsonObject,
    target: JsonObject,
    remote_run: JsonObject,
) -> JsonObject:
    operation = str(target.get("operation") or "command")
    return {
        "kind": ENVIRONMENT_EVIDENCE_KIND,
        "final_allowed": False,
        "summary": "remote environment collection queued",
        "payload": {
            "actionId": action["id"],
            "status": "QUEUED",
            "remoteOperation": operation,
            "remoteRunId": remote_run["taskId"],
            "remoteExecutorId": target["executorId"],
            "remoteCommandId": target.get("commandId"),
            "remoteFileId": target.get("fileId"),
            "finalEvidenceAllowed": False,
        },
    }


def environment_remote_idempotency_key(action_id: str, target_index: int | None = None) -> str:
    suffix = action_id if target_index is None else f"{action_id}:{target_index}"
    return f"{ENVIRONMENT_REMOTE_IDEMPOTENCY_PREFIX}{suffix}"


def parse_environment_remote_idempotency_key(value: object) -> tuple[str, int | None] | None:
    if not isinstance(value, str) or not value.startswith(ENVIRONMENT_REMOTE_IDEMPOTENCY_PREFIX):
        return None
    suffix = value.removeprefix(ENVIRONMENT_REMOTE_IDEMPOTENCY_PREFIX)
    if not suffix:
        return None
    if ":" not in suffix:
        return suffix, None
    action_id, index_text = suffix.rsplit(":", 1)
    if not action_id or not index_text.isdigit():
        return suffix, None
    return action_id, int(index_text)


def remote_run_batch_target_index(remote_run: JsonObject) -> int:
    input_payload = remote_run.get("input")
    if not isinstance(input_payload, dict):
        return 0
    target_index = input_payload.get("batchTargetIndex")
    return target_index if isinstance(target_index, int) else 0


def remote_status_counts(targets: list[JsonObject]) -> JsonObject:
    counts: JsonObject = {}
    for target in targets:
        status = target.get("remoteStatus") or "UNKNOWN"
        if not isinstance(status, str):
            status = str(status)
        counts[status] = int(counts.get(status, 0)) + 1
    return counts


def requeue_analysis_after_environment_collection(store: Store, action: JsonObject) -> None:
    analysis_run = store.get_run(action["run_id"])
    if analysis_run.get("status") not in {"succeeded", "failed"}:
        store.update_run_status(action["run_id"], "queued", "queued")
        store.enqueue_run(action["run_id"])


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
        "remoteOperation": result.get("remoteOperation"),
        "remoteExecutorId": result.get("remoteExecutorId"),
        "remoteCommandId": result.get("remoteCommandId"),
        "remoteFileId": result.get("remoteFileId"),
        "remoteStatus": result.get("remoteStatus"),
        "targetCount": result.get("targetCount"),
        "remoteRunIds": result.get("remoteRunIds"),
        "targets": result.get("targets"),
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
