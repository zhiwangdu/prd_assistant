import json
from contextlib import asynccontextmanager
from pathlib import Path
from typing import Annotated, Any, Literal

from fastapi import Depends, FastAPI, HTTPException, Query, Request
from fastapi.responses import FileResponse, Response
from pydantic import BaseModel, Field, ValidationError
from starlette.datastructures import UploadFile as StarletteUploadFile

from .analysis import get_run_analysis, get_run_artifacts
from .artifacts import (
    resolve_artifact_path,
    safe_filename,
    write_artifact_bytes,
    write_artifact_file,
)
from .case_memory import (
    append_case_import_message,
    case_import_preview,
    confirm_case_import,
    create_manual_case,
    create_task_case,
    preview_case_import,
    update_case,
    update_case_import_draft,
)
from .config import Settings
from .environment import (
    is_pending_environment_collection,
    persist_approved_environment_evidence,
)
from .fetch import (
    endpoint_from_curl,
    endpoint_for_storage,
    endpoint_with_credential_summary,
    execute_fetch_endpoint,
    FETCH_TOOL_ID,
    hydrate_fetch_endpoint,
    normalize_fetch_endpoint,
    normalize_fetch_run_params,
    persist_fetch_credentials,
    preview_curl_import,
    public_fetch_endpoint,
    validate_fetch_credentials_available,
)
from .exports import build_skills_zip, build_tools_zip
from .ids import new_id
from .metadata import (
    confirm_metadata_import,
    fetch_metadata_snapshot_from_url,
    get_metadata_cluster,
    import_metadata_from_url,
    import_metadata,
    list_metadata_cluster_nodes,
    metadata_import_preview,
    preview_metadata_import,
    preview_metadata_import_from_url,
    query_field_types,
    refresh_metadata_instance,
)
from .mcp import readonly_mcp_response, task_mcp_response
from .results import get_run_result
from .security import auth_dependency
from .llm import debug_log_responses, set_debug_log_responses
from .remote_execution import command_template, command_templates, file_templates
from .settings_api import (
    agent_backend_diagnostic,
    agent_backends_summary,
    domain_adapter_summaries,
    list_agent_models,
    llm_settings_summary,
    test_agent_chat,
    test_response,
    validate_settings_message,
)
from .skills import get_skill, import_skill, list_skills, preview_system_context
from .store import Store, UNSET
from .system_context import (
    activate_system_context_version,
    create_system_context_resource,
    create_system_context_version,
    get_system_context_resource,
    list_system_context_resource_summaries,
    patch_system_context_resource,
    patch_system_context_version,
    preview_system_context_resources,
    validate_context_id,
)
from .tools import get_tool_descriptor, tool_catalog, validate_manual_tool_run
from .webui_static import WebuiStaticNotFound, resolve_webui_asset
from .worker import JobRunner


class WorkspaceCreate(BaseModel):
    question: str = Field(min_length=1, max_length=20000)
    mode: Literal["diagnose", "code_investigation", "fix"] = "diagnose"
    language: Literal["zh-CN", "en-US"] = "zh-CN"
    skillIds: list[str] = Field(default_factory=list, max_length=32)


class WorkspaceUpdate(BaseModel):
    question: str | None = Field(default=None, min_length=1, max_length=20000)
    mode: Literal["diagnose", "code_investigation", "fix"] | None = None
    language: Literal["zh-CN", "en-US"] | None = None
    skillIds: list[str] | None = Field(default=None, max_length=32)


class SessionCreate(BaseModel):
    title: str | None = Field(default=None, max_length=160)
    question: str | None = Field(default=None, max_length=20000)
    sourceUrl: str | None = Field(default=None, max_length=2000)
    instanceId: str | None = Field(default=None, max_length=200)
    nodeId: str | None = Field(default=None, max_length=200)
    analysisMode: Literal["diagnose", "code_investigation", "fix"] = "diagnose"
    analysisLanguage: Literal["zh-CN", "en-US"] = "zh-CN"
    systemContextIds: list[str] = Field(default_factory=list, max_length=32)
    skillIds: list[str] = Field(default_factory=list, max_length=32)


class SessionUpdate(BaseModel):
    title: str | None = Field(default=None, max_length=160)
    question: str | None = Field(default=None, min_length=1, max_length=20000)
    sourceUrl: str | None = Field(default=None, max_length=2000)
    instanceId: str | None = Field(default=None, max_length=200)
    nodeId: str | None = Field(default=None, max_length=200)
    analysisMode: Literal["diagnose", "code_investigation", "fix"] | None = None
    analysisLanguage: Literal["zh-CN", "en-US"] | None = None
    systemContextIds: list[str] | None = Field(default=None, max_length=32)
    skillIds: list[str] | None = Field(default=None, max_length=32)
    status: Literal["draft", "ready"] | None = None


class AttachSessionUploads(BaseModel):
    uploadIds: list[str] = Field(min_length=1, max_length=100)


class TaskCreate(BaseModel):
    uploadId: str | None = Field(default=None, max_length=120)
    uploadIds: list[str] = Field(default_factory=list, max_length=100)
    sessionId: str = Field(min_length=1, max_length=120)
    sourceUrl: str | None = Field(default=None, max_length=2000)
    question: str | None = Field(default=None, max_length=20000)
    instanceId: str | None = Field(default=None, max_length=200)
    clusterId: str | None = Field(default=None, max_length=200)
    nodeId: str | None = Field(default=None, max_length=200)
    analysisMode: Literal["diagnose", "code_investigation", "fix"] | None = None
    analysisLanguage: Literal["zh-CN", "en-US"] | None = None
    systemContextIds: list[str] = Field(default_factory=list, max_length=32)
    skillIds: list[str] = Field(default_factory=list, max_length=32)


class MessageCreate(BaseModel):
    message: str = Field(min_length=1, max_length=20000)
    questionId: str | None = Field(default=None, min_length=1, max_length=200)
    resumeMode: Literal["continue", "finalize"] = "continue"
    idempotencyKey: str | None = Field(default=None, min_length=1, max_length=200)


class DecisionCreate(BaseModel):
    decision: Literal["approved", "rejected"]
    reason: str | None = Field(default=None, max_length=2000)
    input: dict[str, Any] | None = None
    idempotencyKey: str | None = Field(default=None, min_length=1, max_length=200)


class MetadataImportCreate(BaseModel):
    instanceId: str = Field(min_length=1, max_length=200)
    templateType: Literal["json", "yaml", "csv", "opengemini"] = "json"
    content: str = Field(min_length=1)
    filename: str | None = Field(default=None, max_length=300)
    remark: str | None = Field(default=None, max_length=120)


class MetadataImportFetchCreate(BaseModel):
    instanceId: str = Field(min_length=1, max_length=200)
    templateType: Literal["json", "yaml", "csv", "opengemini"] = "opengemini"
    url: str = Field(min_length=1, max_length=2000)
    remark: str | None = Field(default=None, max_length=120)


class MetadataFieldTypesQuery(BaseModel):
    instanceId: str = Field(min_length=1, max_length=200)
    database: str = Field(min_length=1, max_length=300)
    measurement: str = Field(min_length=1, max_length=300)
    retentionPolicy: str | None = Field(default=None, max_length=300)
    field: str | list[str] | None = None


class CaseCreate(BaseModel):
    title: str = Field(min_length=1, max_length=300)
    symptom: str = Field(min_length=1, max_length=10000)
    rootCause: str = Field(min_length=1, max_length=10000)
    solution: str = Field(min_length=1, max_length=10000)
    product: str | None = Field(default=None, max_length=200)
    version: str | None = Field(default=None, max_length=200)
    environment: str | None = Field(default=None, max_length=200)
    instanceId: str | None = Field(default=None, max_length=200)
    nodeId: str | None = Field(default=None, max_length=200)
    evidenceRefs: list[str] = Field(default_factory=list)
    enabled: bool = True


class CaseUpdate(BaseModel):
    title: str | None = Field(default=None, max_length=300)
    symptom: str | None = Field(default=None, max_length=10000)
    rootCause: str | None = Field(default=None, max_length=10000)
    solution: str | None = Field(default=None, max_length=10000)
    product: str | None = Field(default=None, max_length=200)
    version: str | None = Field(default=None, max_length=200)
    environment: str | None = Field(default=None, max_length=200)
    instanceId: str | None = Field(default=None, max_length=200)
    nodeId: str | None = Field(default=None, max_length=200)
    evidenceRefs: list[str] | None = None
    enabled: bool | None = None


class CaseImportPreviewCreate(BaseModel):
    content: str = Field(min_length=1, max_length=200000)
    filename: str | None = Field(default=None, max_length=300)


class CaseImportCreate(BaseModel):
    content: str | None = Field(default=None, max_length=200000)
    text: str | None = Field(default=None, max_length=200000)
    filename: str | None = Field(default=None, max_length=300)


CASE_IMPORT_MAX_CHARS = 200000
CASE_IMPORT_MAX_BYTES = CASE_IMPORT_MAX_CHARS * 4
CASE_IMPORT_TEXT_SUFFIXES = {
    ".txt",
    ".text",
    ".md",
    ".markdown",
    ".log",
    ".json",
    ".yaml",
    ".yml",
    ".csv",
}


class CaseImportMessageCreate(BaseModel):
    message: str = Field(min_length=1, max_length=20000)


class CaseImportConfirmCreate(BaseModel):
    title: str | None = Field(default=None, max_length=300)
    symptom: str | None = Field(default=None, max_length=10000)
    rootCause: str | None = Field(default=None, max_length=10000)
    solution: str | None = Field(default=None, max_length=10000)
    product: str | None = Field(default=None, max_length=200)
    version: str | None = Field(default=None, max_length=200)
    environment: str | None = Field(default=None, max_length=200)
    instanceId: str | None = Field(default=None, max_length=200)
    nodeId: str | None = Field(default=None, max_length=200)
    evidenceRefs: list[str] | None = None
    enabled: bool | None = None


class SkillImportCreate(BaseModel):
    skillId: str = Field(min_length=1, max_length=120)
    name: str = Field(min_length=1, max_length=200)
    description: str = Field(min_length=1, max_length=1000)
    markdown: str = Field(min_length=1, max_length=200000)
    filename: str | None = Field(default=None, max_length=300)


class SkillPreviewCreate(BaseModel):
    skillIds: list[str] = Field(default_factory=list, max_length=32)


class SystemContextPromptPolicy(BaseModel):
    includeByDefault: bool = True
    maxChars: int = Field(default=4000, ge=200, le=20000)
    priority: int = 0
    allowedTaskKinds: list[Literal["log_analysis", "tool_run"]] = Field(
        default_factory=list,
        max_length=20,
    )


class SystemContextResourceCreate(BaseModel):
    kind: Literal[
        "prompt_pack",
        "architecture_doc",
        "runbook",
        "glossary",
        "tool_capability",
        "knowledge_note",
        "diagnostic_skill",
    ]
    title: str = Field(min_length=1, max_length=200)
    description: str | None = Field(default=None, max_length=2000)
    scope: Literal["global", "log_analysis", "tool_run", "case_import"] = "log_analysis"
    enabled: bool = True
    tags: list[str] = Field(default_factory=list, max_length=32)
    product: str | None = Field(default=None, max_length=120)
    version: str | None = Field(default=None, max_length=120)
    environment: str | None = Field(default=None, max_length=120)
    contentType: Literal["markdown", "plain_text", "json"]
    content: str = Field(min_length=1, max_length=200000)
    summary: str | None = Field(default=None, max_length=2000)
    promptPolicy: SystemContextPromptPolicy = Field(
        default_factory=SystemContextPromptPolicy
    )


class SystemContextResourceUpdate(BaseModel):
    title: str | None = Field(default=None, min_length=1, max_length=200)
    description: str | None = Field(default=None, max_length=2000)
    scope: Literal["global", "log_analysis", "tool_run", "case_import"] | None = None
    enabled: bool | None = None
    tags: list[str] | None = Field(default=None, max_length=32)
    product: str | None = Field(default=None, max_length=120)
    version: str | None = Field(default=None, max_length=120)
    environment: str | None = Field(default=None, max_length=120)


class SystemContextVersionCreate(BaseModel):
    contentType: Literal["markdown", "plain_text", "json"]
    content: str = Field(min_length=1, max_length=200000)
    summary: str | None = Field(default=None, max_length=2000)
    promptPolicy: SystemContextPromptPolicy = Field(
        default_factory=SystemContextPromptPolicy
    )
    activate: bool = True


class SystemContextVersionUpdate(BaseModel):
    contentType: Literal["markdown", "plain_text", "json"] | None = None
    content: str | None = Field(default=None, min_length=1, max_length=200000)
    summary: str | None = Field(default=None, max_length=2000)
    promptPolicy: SystemContextPromptPolicy | None = None
    status: Literal["draft", "active", "archived"] | None = None


class SystemContextPreviewCreate(BaseModel):
    contextIds: list[str] = Field(default_factory=list, max_length=32)
    taskKind: Literal["log_analysis", "tool_run"] = "log_analysis"
    product: str | None = Field(default=None, max_length=120)
    version: str | None = Field(default=None, max_length=120)
    environment: str | None = Field(default=None, max_length=120)
    instanceId: str | None = Field(default=None, max_length=200)


class FetchEndpointCreate(BaseModel):
    name: str = Field(min_length=1, max_length=200)
    method: Literal["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD"] = "GET"
    url: str = Field(min_length=1, max_length=2000)
    headers: dict[str, str] = Field(default_factory=dict)
    body: str | None = Field(default=None, max_length=200000)
    enabled: bool = True
    followRedirects: bool = False
    refreshPolicy: dict[str, Any] | None = None


class FetchCurlImportPreviewCreate(BaseModel):
    curl: str = Field(min_length=1, max_length=200000)


class FetchCurlImportCreate(BaseModel):
    curl: str = Field(min_length=1, max_length=200000)
    name: str | None = Field(default=None, max_length=200)
    enabled: bool = True


class FetchEndpointUpdate(BaseModel):
    name: str | None = Field(default=None, max_length=200)
    method: Literal["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD"] | None = None
    url: str | None = Field(default=None, max_length=2000)
    headers: dict[str, str] | None = None
    body: str | None = Field(default=None, max_length=200000)
    enabled: bool | None = None
    followRedirects: bool | None = None
    refreshPolicy: dict[str, Any] | None = None


class FetchRunCreate(BaseModel):
    workspaceId: str | None = Field(default=None, max_length=120)
    variables: dict[str, str] = Field(default_factory=dict)
    headers: dict[str, str] = Field(default_factory=dict)
    body: str | None = Field(default=None, max_length=200000)


class ToolRunCreate(BaseModel):
    workspaceId: str | None = Field(default=None, max_length=120)
    uploadIds: list[str] = Field(default_factory=list, max_length=100)
    params: dict = Field(default_factory=dict)
    idempotencyKey: str | None = Field(default=None, max_length=200)


class LlmDebugUpdate(BaseModel):
    llmOutputLogging: bool


class LlmChatTestCreate(BaseModel):
    message: str = Field(min_length=1, max_length=20000)


class RemoteExecutorCreate(BaseModel):
    name: str = Field(min_length=1, max_length=120)
    host: str = Field(min_length=1, max_length=255)
    port: int = Field(default=22, ge=1, le=65535)
    user: str = Field(min_length=1, max_length=64)
    tags: list[str] = Field(default_factory=list, max_length=20)
    notes: str | None = Field(default=None, max_length=500)
    enabled: bool = True


class RemoteExecutorUpdate(BaseModel):
    name: str | None = Field(default=None, max_length=120)
    host: str | None = Field(default=None, max_length=255)
    port: int | None = Field(default=None, ge=1, le=65535)
    user: str | None = Field(default=None, max_length=64)
    tags: list[str] | None = Field(default=None, max_length=20)
    notes: str | None = Field(default=None, max_length=500)
    enabled: bool | None = None


class RemoteRunCreate(BaseModel):
    executorId: str = Field(min_length=1, max_length=120)
    commandId: str = Field(min_length=1, max_length=120)
    idempotencyKey: str | None = Field(default=None, max_length=200)


class UploadSessionInit(BaseModel):
    filename: str = Field(min_length=1, max_length=300)
    contentType: str | None = Field(default=None, max_length=200)
    sizeBytes: int | None = Field(default=None, ge=0)


TERMINAL_RUN_STATUSES = {"succeeded", "failed"}
DEFAULT_SESSION_QUESTION = "分析日志中的主要异常、可能原因和建议检查项。"


def _clean_optional(value: str | None) -> str | None:
    if value is None:
        return None
    value = value.strip()
    return value or None


def _upload_filename(value: str) -> str:
    basename = Path(value).name
    if not basename or basename in {".", ".."}:
        raise HTTPException(status_code=400, detail="invalid filename")
    filename = safe_filename(basename)
    if filename in {".", ".."}:
        raise HTTPException(status_code=400, detail="invalid filename")
    return filename


def _session_create_question(value: str | None) -> str:
    return _clean_optional(value) or DEFAULT_SESSION_QUESTION


def _session_create_title(value: str | None, question: str) -> str:
    explicit = _clean_optional(value)
    if explicit:
        return explicit
    return question[:80] or "New log analysis session"


def _session_upload_ids(values: list[str]) -> list[str]:
    upload_ids: list[str] = []
    for value in values:
        upload_id = value.strip()
        if not upload_id:
            continue
        if not upload_id.startswith("upl_"):
            raise ValueError("invalid uploadId")
        if upload_id not in upload_ids:
            upload_ids.append(upload_id)
    if not upload_ids:
        raise ValueError("missing uploadIds")
    return upload_ids


def _system_context_ids(values: list[str] | None) -> list[str]:
    context_ids: list[str] = []
    for value in values or []:
        context_id = value.strip()
        if not context_id:
            continue
        validate_context_id(context_id)
        if context_id not in context_ids:
            context_ids.append(context_id)
    if len(context_ids) > 32:
        raise ValueError("too many systemContextIds")
    return context_ids


def _skill_ids(values: list[str] | None) -> list[str]:
    skill_ids: list[str] = []
    for value in values or []:
        skill_id = value.strip()
        if not skill_id:
            continue
        valid = len(skill_id) <= 120 and all(
            ch.isascii() and (ch.isalnum() or ch in {"_", "-", "."})
            for ch in skill_id
        )
        if not valid:
            raise ValueError("invalid skillId")
        if skill_id not in skill_ids:
            skill_ids.append(skill_id)
    if len(skill_ids) > 32:
        raise ValueError("too many skillIds")
    return skill_ids


def _session_title(workspace: dict) -> str:
    explicit = _clean_optional(workspace.get("title"))
    if explicit:
        return explicit
    question = str(workspace.get("question") or "").strip()
    if not question:
        return "Untitled session"
    return question[:120]


def _session_status(workspace: dict, uploads: list[dict], runs: list[dict]) -> str:
    if workspace.get("status") == "deleted":
        return "deleted"
    if not runs:
        session_status = str(workspace.get("sessionStatus") or "draft")
        if session_status in {"draft", "ready"}:
            return session_status
        return "ready" if uploads else "draft"
    latest = runs[0]
    status = str(latest.get("status") or "draft")
    if status == "queued":
        return "ready"
    if status in {"running", "waiting_for_user", "waiting_for_approval"}:
        return status
    if status in TERMINAL_RUN_STATUSES:
        return status
    return "ready"


def _session_record(store: Store, workspace: dict) -> dict:
    uploads = store.list_uploads(workspace["id"])
    runs = store.list_runs(workspace["id"])
    active_task_id = runs[0]["id"] if runs else None
    return {
        "schemaVersion": 1,
        "sessionId": workspace["id"],
        "workspaceId": workspace["id"],
        "title": _session_title(workspace),
        "question": workspace.get("question"),
        "sourceUrl": workspace.get("sourceUrl"),
        "instanceId": workspace.get("instanceId"),
        "nodeId": workspace.get("nodeId"),
        "analysisMode": workspace.get("mode"),
        "analysisLanguage": workspace.get("language"),
        "systemContextIds": workspace.get("systemContextIds", []),
        "skillIds": workspace.get("skillIds", []),
        "uploadIds": [upload["id"] for upload in uploads],
        "taskIds": [run["id"] for run in runs],
        "activeTaskId": active_task_id,
        "status": _session_status(workspace, uploads, runs),
        "uploadCount": len(uploads),
        "taskCount": len(runs),
        "createdAt": workspace.get("created_at"),
        "updatedAt": workspace.get("updated_at"),
        "workspace": workspace,
    }


def _task_summary(workspace: dict, run: dict) -> dict:
    phase = run.get("phase")
    return {
        "taskId": run["id"],
        "runId": run["id"],
        "alias": run.get("alias"),
        "url": f"/api/v2/runs/{run['id']}",
        "taskKind": "log_analysis",
        "sessionId": workspace["id"],
        "workspaceId": workspace["id"],
        "analysisMode": workspace.get("mode"),
        "analysisLanguage": workspace.get("language"),
        "status": str(run.get("status") or "").upper(),
        "phase": str(phase).upper() if phase else None,
        "createdAt": run.get("created_at"),
        "updatedAt": run.get("updated_at"),
    }


def _task_alias_response(store: Store, run: dict) -> dict:
    workspace = store.get_workspace(run["workspace_id"])
    task = _task_summary(workspace, run)
    return {
        **task,
        "task": task,
        "run": run,
        "workspace": workspace,
    }


def _tool_run_summary(workspace: dict, run: dict) -> dict:
    phase = run.get("phase")
    mode = workspace.get("mode")
    if mode not in {"diagnose", "code_investigation", "fix"}:
        mode = "diagnose"
    return {
        "taskId": run["id"],
        "runId": run["id"],
        "alias": run.get("alias"),
        "url": f"/api/v2/tools/runs/{run['id']}",
        "taskKind": "tool_run",
        "sessionId": None,
        "workspaceId": workspace["id"],
        "analysisMode": mode,
        "analysisLanguage": workspace.get("language") or "zh-CN",
        "status": str(run.get("status") or "").upper(),
        "phase": str(phase).upper() if phase else None,
        "toolId": run.get("toolId"),
        "uploadIds": run.get("toolUploadIds", []),
        "createdAt": run.get("created_at"),
        "updatedAt": run.get("updated_at"),
    }


def _tool_run_response(store: Store, run: dict) -> dict:
    workspace = store.get_workspace(run["workspace_id"])
    task = _tool_run_summary(workspace, run)
    return {
        **run,
        **task,
        "task": task,
        "run": run,
        "workspace": workspace,
    }


def _tool_run_upload_ids(payload: ToolRunCreate) -> list[str]:
    return [upload_id.strip() for upload_id in payload.uploadIds if upload_id.strip()]


def _tool_run_workspace_id(store: Store, payload: ToolRunCreate, upload_ids: list[str]) -> str:
    workspace_id = _clean_optional(payload.workspaceId)
    if workspace_id is not None:
        store.get_workspace(workspace_id)
        return workspace_id
    if upload_ids:
        upload = store.get_upload_with_artifact(upload_ids[0])
        store.get_workspace(upload["workspace_id"])
        return upload["workspace_id"]
    workspace = store.create_workspace(
        "Run selected tool",
        "diagnose",
        "zh-CN",
        title="Manual tool run",
    )
    return workspace["id"]


def _require_succeeded_log_analysis_task(run: dict, noun: str) -> None:
    if run.get("status") != "succeeded":
        raise HTTPException(
            status_code=409,
            detail={
                "message": f"{noun} is only available after success",
                "status": run.get("status"),
            },
        )
    if run.get("kind", "analysis") != "analysis":
        raise HTTPException(status_code=400, detail="task is not a log analysis task")


def _task_create_upload_ids(payload: TaskCreate) -> list[str]:
    upload_ids: list[str] = []

    def append(value: str | None) -> None:
        value = _clean_optional(value)
        if value and value not in upload_ids:
            upload_ids.append(value)

    append(payload.uploadId)
    for upload_id in payload.uploadIds:
        append(upload_id)
    return upload_ids


def _node_ids_from_snapshot(snapshot: dict) -> set[str]:
    cluster = snapshot.get("cluster") if isinstance(snapshot.get("cluster"), dict) else {}
    nodes = cluster.get("nodes") if isinstance(cluster.get("nodes"), list) else []
    node_ids: set[str] = set()
    for node in nodes:
        if not isinstance(node, dict):
            continue
        node_id = node.get("nodeId")
        if isinstance(node_id, str) and node_id:
            node_ids.add(node_id)
    return node_ids


def _resolve_task_instance_id(
    store: Store,
    *,
    instance_id: str | None,
    cluster_id: str | None,
    node_id: str | None,
) -> str | None:
    instance_id = _clean_optional(instance_id)
    cluster_id = _clean_optional(cluster_id)
    node_id = _clean_optional(node_id)
    snapshot: dict | None = None
    if instance_id:
        try:
            snapshot = store.get_metadata_snapshot(instance_id)
        except KeyError as error:
            raise ValueError(f"unknown instanceId {instance_id}") from error
        if cluster_id:
            cluster = snapshot.get("cluster") if isinstance(snapshot.get("cluster"), dict) else {}
            actual_cluster_id = cluster.get("clusterId")
            if actual_cluster_id != cluster_id:
                raise ValueError(
                    f"instanceId {instance_id} belongs to clusterId {actual_cluster_id}, "
                    f"not {cluster_id}"
                )
    elif cluster_id:
        matches: list[tuple[str, dict]] = []
        for instance in store.list_metadata_instances():
            candidate_id = instance.get("instanceId")
            if not isinstance(candidate_id, str):
                continue
            candidate_snapshot = store.get_metadata_snapshot(candidate_id)
            cluster = (
                candidate_snapshot.get("cluster")
                if isinstance(candidate_snapshot.get("cluster"), dict)
                else {}
            )
            if cluster.get("clusterId") == cluster_id:
                matches.append((candidate_id, candidate_snapshot))
        if not matches:
            raise ValueError(f"unknown clusterId {cluster_id}")
        if len(matches) > 1:
            raise ValueError(f"clusterId {cluster_id} maps to multiple instanceIds")
        instance_id, snapshot = matches[0]
    if node_id and snapshot is not None:
        node_ids = _node_ids_from_snapshot(snapshot)
        if node_id not in node_ids:
            raise ValueError(f"unknown nodeId {node_id}")
    return instance_id


def _case_import_normalize_text(value: str) -> str:
    value = value.strip()
    if not value:
        raise ValueError("case import text must not be empty")
    chars = len(value)
    if chars > CASE_IMPORT_MAX_CHARS:
        raise ValueError(
            f"case import text contains {chars} chars and exceeds "
            f"max input chars {CASE_IMPORT_MAX_CHARS}"
        )
    return value


def _case_import_optional_filename(value: str | None) -> str | None:
    if value is None:
        return None
    value = value.strip()
    if not value:
        return None
    return _case_import_filename(value)


def _case_import_filename(value: str) -> str:
    basename = Path(value).name
    if not basename or basename in {".", ".."}:
        raise ValueError("invalid filename")
    filename = safe_filename(basename)
    if filename in {".", ".."}:
        raise ValueError("invalid filename")
    return filename


def _case_import_supported_text_file(
    filename: str,
    content_type: str | None,
) -> bool:
    suffix_supported = Path(filename).suffix.lower() in CASE_IMPORT_TEXT_SUFFIXES
    if suffix_supported:
        return True
    if content_type is None:
        return False
    content_type = content_type.lower()
    return (
        content_type.startswith("text/")
        or "json" in content_type
        or "yaml" in content_type
    )


def _case_import_form_string(value: object | None) -> str | None:
    if value is None or isinstance(value, StarletteUploadFile):
        return None
    if isinstance(value, str):
        return value
    return str(value)


def _case_import_create_content(payload: CaseImportCreate) -> str:
    content = payload.content if payload.content is not None else payload.text
    if content is None:
        raise ValueError("case import text must not be empty")
    return _case_import_normalize_text(content)


async def _case_import_create_input(request: Request) -> tuple[str, str | None]:
    content_type = request.headers.get("content-type", "").lower()
    if content_type.startswith("multipart/form-data"):
        return await _case_import_create_input_from_multipart(request)
    try:
        payload_json = await request.json()
        payload = CaseImportCreate.model_validate(payload_json)
    except (json.JSONDecodeError, ValidationError) as error:
        raise ValueError(f"invalid case import JSON: {error}") from error
    return (
        _case_import_create_content(payload),
        _case_import_optional_filename(payload.filename),
    )


async def _case_import_create_input_from_multipart(
    request: Request,
) -> tuple[str, str | None]:
    try:
        form = await request.form()
    except Exception as error:
        raise ValueError(f"invalid multipart request: {error}") from error
    file_value = form.get("file")
    if isinstance(file_value, StarletteUploadFile):
        filename = _case_import_optional_filename(file_value.filename) or "case.txt"
        if not _case_import_supported_text_file(filename, file_value.content_type):
            raise ValueError(
                "unsupported case import file type; use UTF-8 "
                ".txt/.md/.log/.json/.yaml/.yml/.csv or paste text"
            )
        file_bytes = await file_value.read(CASE_IMPORT_MAX_BYTES + 1)
        if len(file_bytes) > CASE_IMPORT_MAX_BYTES:
            raise ValueError(
                f"case import file exceeds {CASE_IMPORT_MAX_BYTES} bytes"
            )
        try:
            return _case_import_normalize_text(file_bytes.decode("utf-8")), filename
        except UnicodeDecodeError as error:
            raise ValueError("case import file must be UTF-8 text") from error
    if file_value is not None:
        raise ValueError("invalid file field")
    text = _case_import_form_string(form.get("text"))
    if text is None:
        text = _case_import_form_string(form.get("content"))
    if text is None:
        raise ValueError("missing text or file field")
    filename = _case_import_optional_filename(
        _case_import_form_string(form.get("filename"))
    )
    return _case_import_normalize_text(text), filename


def normalize_optional_message_id(value: str | None) -> str | None:
    if value is None:
        return None
    normalized = value.strip()
    return normalized or None


def find_idempotent_user_message(
    store: Store,
    run_id: str,
    idempotency_key: str,
) -> dict | None:
    for event in reversed(store.list_timeline(run_id)):
        payload = event.get("payload")
        if (
            event.get("kind") == "user.message"
            and isinstance(payload, dict)
            and payload.get("idempotencyKey") == idempotency_key
        ):
            return event
    return None


def user_action_matches_question(action: dict, question_id: str) -> bool:
    payload = action.get("payload") if isinstance(action.get("payload"), dict) else {}
    return action.get("id") == question_id or payload.get("questionId") == question_id


def find_idempotent_action_decision(
    store: Store,
    run_id: str,
    action_id: str,
    idempotency_key: str,
) -> dict | None:
    for event in reversed(store.list_timeline(run_id)):
        payload = event.get("payload")
        if (
            event.get("kind", "").startswith("action.")
            and isinstance(payload, dict)
            and payload.get("actionId") == action_id
            and payload.get("idempotencyKey") == idempotency_key
        ):
            return event
    return None


def create_app(settings: Settings | None = None) -> FastAPI:
    settings = settings or Settings.from_env()
    settings.ensure_dirs()
    store = Store(settings.sqlite_path)
    store.initialize()
    runner = JobRunner(settings, store)

    @asynccontextmanager
    async def lifespan(app: FastAPI):
        app.state.settings = settings
        app.state.store = store
        app.state.job_recovery = store.recover_interrupted_jobs()
        if settings.inline_worker:
            await runner.start()
        yield
        await runner.stop()

    app = FastAPI(title="LogAgent V2", version="0.1.0", lifespan=lifespan)
    require_auth = auth_dependency(settings)
    Auth = Annotated[None, Depends(require_auth)]

    @app.get("/health")
    async def health() -> dict:
        return {"ok": True, "service": "logagent-v2"}

    @app.post("/api/v2/workspaces")
    async def create_workspace(_: Auth, payload: WorkspaceCreate) -> dict:
        try:
            return store.create_workspace(
                payload.question,
                payload.mode,
                payload.language,
                skill_ids=_skill_ids(payload.skillIds),
            )
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.get("/api/v2/workspaces")
    async def list_workspaces(_: Auth) -> dict:
        return {"workspaces": store.list_workspaces()}

    @app.get("/api/v2/workspaces/{workspace_id}")
    async def get_workspace(_: Auth, workspace_id: str) -> dict:
        try:
            return store.get_workspace(workspace_id)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.patch("/api/v2/workspaces/{workspace_id}")
    async def update_workspace(_: Auth, workspace_id: str, payload: WorkspaceUpdate) -> dict:
        try:
            return store.update_workspace(
                workspace_id,
                question=payload.question.strip() if payload.question is not None else None,
                mode=payload.mode,
                language=payload.language,
                skill_ids=(
                    _skill_ids(payload.skillIds)
                    if payload.skillIds is not None
                    else None
                ),
            )
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.delete("/api/v2/workspaces/{workspace_id}")
    async def delete_workspace(_: Auth, workspace_id: str) -> dict:
        try:
            workspace = store.delete_workspace(workspace_id)
            return {"deleted": True, "workspaceId": workspace_id, "workspace": workspace}
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.post("/api/v2/sessions", status_code=201)
    async def create_session(_: Auth, payload: SessionCreate) -> dict:
        try:
            question = _session_create_question(payload.question)
            workspace = store.create_workspace(
                question,
                payload.analysisMode,
                payload.analysisLanguage,
                skill_ids=_skill_ids(payload.skillIds),
                title=_session_create_title(payload.title, question),
                source_url=_clean_optional(payload.sourceUrl),
                instance_id=_clean_optional(payload.instanceId),
                node_id=_clean_optional(payload.nodeId),
                system_context_ids=_system_context_ids(payload.systemContextIds),
            )
            return _session_record(store, workspace)
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.get("/api/v2/sessions")
    async def list_sessions(_: Auth) -> dict:
        return {
            "sessions": [
                _session_record(store, workspace) for workspace in store.list_workspaces()
            ]
        }

    @app.get("/api/v2/sessions/{session_id}")
    async def get_session(_: Auth, session_id: str) -> dict:
        try:
            return _session_record(store, store.get_workspace(session_id))
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.patch("/api/v2/sessions/{session_id}")
    async def update_session(_: Auth, session_id: str, payload: SessionUpdate) -> dict:
        try:
            fields_set = payload.model_fields_set
            question = payload.question.strip() if payload.question is not None else None
            if question is not None and not question:
                raise ValueError("question cannot be empty")
            workspace = store.update_workspace(
                session_id,
                title=_clean_optional(payload.title) if payload.title is not None else None,
                question=question,
                source_url=(
                    _clean_optional(payload.sourceUrl)
                    if "sourceUrl" in fields_set
                    else UNSET
                ),
                instance_id=(
                    _clean_optional(payload.instanceId)
                    if "instanceId" in fields_set
                    else UNSET
                ),
                node_id=(
                    _clean_optional(payload.nodeId) if "nodeId" in fields_set else UNSET
                ),
                mode=payload.analysisMode,
                language=payload.analysisLanguage,
                system_context_ids=(
                    _system_context_ids(payload.systemContextIds)
                    if payload.systemContextIds is not None
                    else None
                ),
                skill_ids=(
                    _skill_ids(payload.skillIds)
                    if payload.skillIds is not None
                    else None
                ),
                session_status=payload.status,
            )
            return _session_record(store, workspace)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.delete("/api/v2/sessions/{session_id}")
    async def delete_session(_: Auth, session_id: str) -> dict:
        try:
            runs = store.list_runs(session_id)
            unfinished = [
                run for run in runs if run["status"] not in TERMINAL_RUN_STATUSES
            ]
            if unfinished:
                raise HTTPException(
                    status_code=409,
                    detail="session has unfinished tasks",
                )
            workspace = store.delete_workspace(session_id)
            return {
                "deleted": True,
                "sessionId": session_id,
                "workspaceId": session_id,
                "session": _session_record(store, workspace),
            }
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/v2/workspaces/{workspace_id}/uploads")
    async def list_workspace_uploads(_: Auth, workspace_id: str) -> dict:
        try:
            store.get_workspace(workspace_id)
            return {"uploads": store.list_uploads(workspace_id)}
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/v2/workspaces/{workspace_id}/upload-sessions")
    async def list_workspace_upload_sessions(_: Auth, workspace_id: str) -> dict:
        try:
            return {"sessions": store.list_upload_sessions(workspace_id)}
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/v2/workspaces/{workspace_id}/runs")
    async def list_workspace_runs(_: Auth, workspace_id: str) -> dict:
        try:
            return {"runs": store.list_runs(workspace_id)}
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/v2/sessions/{session_id}/uploads")
    async def list_session_uploads(_: Auth, session_id: str) -> dict:
        try:
            store.get_workspace(session_id)
            return {"uploads": store.list_uploads(session_id)}
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/v2/sessions/{session_id}/upload-sessions")
    async def list_session_upload_sessions(_: Auth, session_id: str) -> dict:
        try:
            return {"sessions": store.list_upload_sessions(session_id)}
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/v2/sessions/{session_id}/tasks")
    async def list_session_tasks(_: Auth, session_id: str) -> dict:
        try:
            workspace = store.get_workspace(session_id)
            runs = store.list_runs(session_id)
            return {
                "sessionId": session_id,
                "tasks": [_task_summary(workspace, run) for run in runs],
                "runs": runs,
            }
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/v2/sessions/{session_id}/timeline")
    async def get_session_timeline(_: Auth, session_id: str) -> dict:
        try:
            return {"events": store.list_workspace_timeline(session_id)}
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    async def receive_single_upload_request(workspace_id: str, request: Request) -> dict:
        try:
            store.get_workspace(workspace_id)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

        content_type = request.headers.get("content-type", "").lower()
        if not content_type.startswith("multipart/form-data"):
            raise HTTPException(status_code=415, detail="unsupported upload content type")
        try:
            form = await request.form()
        except Exception as error:
            raise HTTPException(
                status_code=400, detail=f"invalid multipart request: {error}"
            ) from error

        files = [
            value
            for key, value in form.multi_items()
            if key == "file" and isinstance(value, StarletteUploadFile)
        ]
        if len(files) != 1:
            raise HTTPException(status_code=400, detail="expected exactly one file field")

        filename_override: str | None = None
        for key, value in form.multi_items():
            if key == "filename":
                if isinstance(value, StarletteUploadFile):
                    raise HTTPException(status_code=400, detail="invalid filename field")
                filename_override = str(value)

        file = files[0]
        filename = _upload_filename(
            filename_override if filename_override is not None else file.filename or "upload.bin"
        )
        data = await file.read(settings.max_upload_bytes + 1)
        if len(data) > settings.max_upload_bytes:
            raise HTTPException(status_code=413, detail="upload exceeds max_upload_bytes")
        artifact = write_artifact_bytes(
            settings=settings,
            store=store,
            workspace_id=workspace_id,
            filename=filename,
            data=data,
            content_type=file.content_type or "application/octet-stream",
            schema_name=None,
            preview={"filename": filename, "sizeBytes": len(data)},
        )
        upload = store.create_upload(workspace_id, filename, artifact["id"])
        return {"upload": upload, "artifact": artifact}

    @app.post("/api/v2/workspaces/{workspace_id}/uploads")
    async def upload_file(_: Auth, workspace_id: str, request: Request) -> dict:
        return await receive_single_upload_request(workspace_id, request)

    @app.post("/api/v2/sessions/{session_id}/uploads")
    async def upload_or_attach_session_uploads(
        _: Auth, session_id: str, request: Request
    ) -> dict:
        content_type = request.headers.get("content-type", "").lower()
        if content_type.startswith("application/json"):
            try:
                store.get_workspace(session_id)
            except KeyError as error:
                raise HTTPException(status_code=404, detail=str(error)) from error
            try:
                payload = AttachSessionUploads.model_validate(await request.json())
                workspace = store.attach_uploads(
                    session_id,
                    _session_upload_ids(payload.uploadIds),
                )
                return _session_record(store, workspace)
            except ValidationError as error:
                raise HTTPException(status_code=400, detail=str(error)) from error
            except KeyError as error:
                raise HTTPException(status_code=400, detail=str(error)) from error
            except ValueError as error:
                raise HTTPException(status_code=400, detail=str(error)) from error
        if content_type.startswith("multipart/form-data"):
            return await receive_single_upload_request(session_id, request)
        raise HTTPException(status_code=415, detail="unsupported upload content type")

    @app.delete("/api/v2/sessions/{session_id}/uploads/{upload_id}")
    async def detach_session_upload(_: Auth, session_id: str, upload_id: str) -> dict:
        try:
            store.get_workspace(session_id)
            workspace = store.detach_upload(session_id, upload_id)
            return _session_record(store, workspace)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=409, detail=str(error)) from error

    @app.post("/api/v2/workspaces/{workspace_id}/uploads/batch")
    async def upload_files(
        _: Auth,
        workspace_id: str,
        request: Request,
    ) -> dict:
        try:
            store.get_workspace(workspace_id)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        content_type = request.headers.get("content-type", "").lower()
        if not content_type.startswith("multipart/form-data"):
            raise HTTPException(status_code=415, detail="unsupported upload content type")
        try:
            form = await request.form()
        except Exception as error:
            raise HTTPException(
                status_code=400, detail=f"invalid multipart request: {error}"
            ) from error
        files = [
            value
            for key, value in form.multi_items()
            if key in {"file", "files"} and isinstance(value, StarletteUploadFile)
        ]
        if not files:
            raise HTTPException(status_code=400, detail="missing file fields")
        file_names = [
            (file, _upload_filename(file.filename or "upload.bin")) for file in files
        ]
        results = []
        for file, filename in file_names:
            data = await file.read(settings.max_upload_bytes + 1)
            if len(data) > settings.max_upload_bytes:
                raise HTTPException(
                    status_code=413,
                    detail=f"upload {file.filename or 'upload.bin'} exceeds max_upload_bytes",
                )
            artifact = write_artifact_bytes(
                settings=settings,
                store=store,
                workspace_id=workspace_id,
                filename=filename,
                data=data,
                content_type=file.content_type or "application/octet-stream",
                schema_name=None,
                preview={"filename": filename, "sizeBytes": len(data)},
            )
            upload = store.create_upload(workspace_id, filename, artifact["id"])
            results.append({"upload": upload, "artifact": artifact})
        return {"uploads": results}

    @app.post("/api/v2/sessions/{session_id}/uploads/batch")
    async def upload_session_files(
        _: Auth,
        session_id: str,
        request: Request,
    ) -> dict:
        return await upload_files(None, session_id, request)

    @app.post("/api/v2/workspaces/{workspace_id}/uploads/init")
    async def init_upload_session(
        _: Auth,
        workspace_id: str,
        payload: UploadSessionInit,
    ) -> dict:
        try:
            store.get_workspace(workspace_id)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        if payload.sizeBytes is not None and payload.sizeBytes > settings.max_upload_bytes:
            raise HTTPException(status_code=413, detail="upload exceeds max_upload_bytes")
        session_id = new_id("ups")
        filename = _upload_filename(payload.filename)
        temp_relative_path = f"tmp/upload_sessions/{session_id}/{filename}"
        session = store.create_upload_session(
            session_id=session_id,
            workspace_id=workspace_id,
            filename=filename,
            content_type=payload.contentType or "application/octet-stream",
            expected_size_bytes=payload.sizeBytes,
            temp_relative_path=temp_relative_path,
        )
        return {"session": session}

    @app.post("/api/v2/sessions/{session_id}/uploads/init")
    async def init_session_upload_session(
        _: Auth,
        session_id: str,
        payload: UploadSessionInit,
    ) -> dict:
        return await init_upload_session(None, session_id, payload)

    @app.get("/api/v2/uploads/{session_id}")
    async def get_upload_session(_: Auth, session_id: str) -> dict:
        try:
            return store.get_upload_session(session_id)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.post("/api/v2/uploads/{session_id}/chunks")
    async def upload_chunk(
        _: Auth,
        session_id: str,
        request: Request,
        offset: int = Query(ge=0),
    ) -> dict:
        try:
            session = store.get_upload_session(session_id)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        if session["status"] != "active":
            raise HTTPException(status_code=409, detail="upload session is not active")
        data = bytearray()
        async for chunk in request.stream():
            if not chunk:
                continue
            data.extend(chunk)
            if len(data) > settings.max_chunk_bytes:
                raise HTTPException(
                    status_code=413,
                    detail=(
                        f"chunk size {len(data)} exceeds max_chunk_bytes "
                        f"{settings.max_chunk_bytes}"
                    ),
                )
        if offset != session["received_bytes"]:
            raise HTTPException(
                status_code=409,
                detail=f"chunk offset {offset} does not match received_bytes "
                f"{session['received_bytes']}",
            )
        chunk_bytes = bytes(data)
        next_offset = offset + len(chunk_bytes)
        expected_size = session.get("expected_size_bytes")
        if expected_size is not None and next_offset > expected_size:
            raise HTTPException(status_code=400, detail="chunk exceeds expected upload size")
        if next_offset > settings.max_upload_bytes:
            raise HTTPException(status_code=413, detail="upload exceeds max_upload_bytes")
        path = resolve_artifact_path(settings, session["temp_relative_path"])
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("r+b" if path.exists() else "wb") as target:
            target.seek(offset)
            target.write(chunk_bytes)
        session = store.update_upload_session_progress(session_id, next_offset)
        return {"session": session}

    @app.post("/api/v2/uploads/{session_id}/complete")
    async def complete_upload_session(_: Auth, session_id: str) -> dict:
        try:
            session = store.get_upload_session(session_id)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        if session["status"] == "completed":
            return {
                "session": session,
                "upload": store.get_upload(session["upload_id"]),
                "artifact": store.get_artifact(session["artifact_id"]),
            }
        if session["status"] != "active":
            raise HTTPException(status_code=409, detail="upload session is not active")
        expected_size = session.get("expected_size_bytes")
        if expected_size is not None and session["received_bytes"] != expected_size:
            raise HTTPException(status_code=409, detail="upload session is incomplete")
        path = resolve_artifact_path(settings, session["temp_relative_path"])
        if not path.exists():
            raise HTTPException(status_code=409, detail="upload session has no chunk data")
        actual_size = path.stat().st_size
        if actual_size != session["received_bytes"]:
            raise HTTPException(status_code=409, detail="upload session size mismatch")
        artifact = write_artifact_file(
            settings=settings,
            store=store,
            workspace_id=session["workspace_id"],
            filename=session["filename"],
            source_path=path,
            content_type=session["content_type"],
            schema_name=None,
            preview={"filename": session["filename"], "sizeBytes": actual_size},
        )
        upload = store.create_upload(session["workspace_id"], session["filename"], artifact["id"])
        completed = store.complete_upload_session(session_id, upload["id"], artifact["id"])
        path.unlink(missing_ok=True)
        return {"session": completed, "upload": upload, "artifact": artifact}

    @app.post("/api/v2/workspaces/{workspace_id}/runs")
    async def create_run(_: Auth, workspace_id: str) -> dict:
        try:
            return store.create_run(workspace_id)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.post("/api/v2/sessions/{session_id}/tasks", status_code=202)
    async def create_session_task(_: Auth, session_id: str) -> dict:
        try:
            workspace = store.get_workspace(session_id)
            run = store.create_run(session_id)
            task = _task_summary(workspace, run)
            return {
                **task,
                "sessionId": session_id,
                "workspaceId": session_id,
                "taskId": run["id"],
                "runId": run["id"],
                "task": task,
                "run": run,
            }
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.post("/api/v2/tasks", status_code=202)
    async def create_task_alias(_: Auth, payload: TaskCreate) -> dict:
        session_id = _clean_optional(payload.sessionId)
        if session_id is None:
            raise HTTPException(status_code=400, detail="sessionId is required")
        try:
            workspace = store.get_workspace(session_id)
            fields_set = payload.model_fields_set
            upload_ids = _task_create_upload_ids(payload)
            if upload_ids:
                store.list_uploads_by_ids(session_id, upload_ids)
                workspace = store.attach_uploads(session_id, upload_ids)
            question = _clean_optional(payload.question) if payload.question is not None else None
            current_instance_id = workspace.get("instanceId")
            metadata_fields_set = bool(
                {"instanceId", "clusterId", "nodeId"} & fields_set
            )
            resolved_instance_id = current_instance_id
            if metadata_fields_set:
                requested_instance_id = (
                    _clean_optional(payload.instanceId)
                    if "instanceId" in fields_set
                    else current_instance_id
                )
                requested_node_id = (
                    _clean_optional(payload.nodeId)
                    if "nodeId" in fields_set
                    else workspace.get("nodeId")
                )
                resolved_instance_id = _resolve_task_instance_id(
                    store,
                    instance_id=requested_instance_id,
                    cluster_id=payload.clusterId,
                    node_id=requested_node_id,
                )
            should_update_workspace = (
                question is not None
                or "sourceUrl" in fields_set
                or "instanceId" in fields_set
                or "clusterId" in fields_set
                or "nodeId" in fields_set
                or payload.analysisMode is not None
                or payload.analysisLanguage is not None
                or "systemContextIds" in fields_set
                or "skillIds" in fields_set
            )
            if should_update_workspace:
                workspace = store.update_workspace(
                    session_id,
                    question=question,
                    source_url=(
                        _clean_optional(payload.sourceUrl)
                        if "sourceUrl" in fields_set
                        else UNSET
                    ),
                    instance_id=(
                        resolved_instance_id
                        if ("instanceId" in fields_set or "clusterId" in fields_set)
                        else UNSET
                    ),
                    node_id=(
                        _clean_optional(payload.nodeId) if "nodeId" in fields_set else UNSET
                    ),
                    mode=payload.analysisMode,
                    language=payload.analysisLanguage,
                    system_context_ids=(
                        _system_context_ids(payload.systemContextIds)
                        if "systemContextIds" in fields_set
                        else None
                    ),
                    skill_ids=(
                        _skill_ids(payload.skillIds) if "skillIds" in fields_set else None
                    ),
                )
            run = store.create_run(session_id)
            task = _task_summary(workspace, run)
            return {
                **task,
                "sessionId": session_id,
                "workspaceId": session_id,
                "taskId": run["id"],
                "runId": run["id"],
                "uploadIds": workspace.get("uploadIds", []),
                "task": task,
                "run": run,
            }
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.get("/api/v2/runs")
    async def list_runs(_: Auth, workspaceId: str | None = None) -> dict:
        try:
            return {"runs": store.list_runs(workspaceId)}
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/v2/tasks")
    async def list_tasks_alias(_: Auth, workspaceId: str | None = None) -> dict:
        try:
            runs = store.list_runs(workspaceId)
            return {
                "tasks": [_task_alias_response(store, run) for run in runs],
                "runs": runs,
            }
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/v2/runs/{run_id}")
    async def get_run(_: Auth, run_id: str) -> dict:
        try:
            return store.get_run(run_id)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/v2/tasks/{task_id}")
    async def get_task_alias(_: Auth, task_id: str) -> dict:
        try:
            return _task_alias_response(store, store.get_run(task_id))
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/v2/runs/{run_id}/timeline")
    async def get_timeline(_: Auth, run_id: str) -> dict:
        try:
            return {"events": store.list_timeline(run_id)}
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/v2/tasks/{task_id}/timeline")
    async def get_task_timeline_alias(_: Auth, task_id: str) -> dict:
        return await get_timeline(_, task_id)

    @app.get("/api/v2/runs/{run_id}/evidence")
    async def list_evidence(_: Auth, run_id: str) -> dict:
        try:
            return {"evidence": store.list_evidence(run_id)}
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/v2/tasks/{task_id}/evidence")
    async def list_task_evidence_alias(_: Auth, task_id: str) -> dict:
        return await list_evidence(_, task_id)

    @app.get("/api/v2/runs/{run_id}/artifacts")
    async def list_run_artifacts(_: Auth, run_id: str) -> dict:
        try:
            return get_run_artifacts(settings, store, run_id)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/v2/tasks/{task_id}/artifacts")
    async def list_task_artifacts_alias(_: Auth, task_id: str) -> dict:
        try:
            run = store.get_run(task_id)
            _require_succeeded_log_analysis_task(run, "task artifacts")
            return get_run_artifacts(settings, store, task_id)
        except HTTPException:
            raise
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/v2/runs/{run_id}/analysis")
    async def get_analysis(_: Auth, run_id: str) -> dict:
        try:
            return get_run_analysis(settings, store, run_id)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/v2/tasks/{task_id}/analysis")
    async def get_task_analysis_alias(_: Auth, task_id: str) -> dict:
        return await get_analysis(_, task_id)

    @app.get("/api/v2/runs/{run_id}/result")
    async def get_result(_: Auth, run_id: str) -> dict:
        try:
            run = store.get_run(run_id)
            if not run.get("finalAnswer"):
                raise HTTPException(
                    status_code=409,
                    detail={
                        "message": "run result is only available after success",
                        "status": run.get("status"),
                    },
                )
            return get_run_result(settings, store, run_id)
        except HTTPException:
            raise
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/v2/tasks/{task_id}/result")
    async def get_task_result_alias(_: Auth, task_id: str) -> dict:
        try:
            run = store.get_run(task_id)
            _require_succeeded_log_analysis_task(run, "task result")
            return get_run_result(settings, store, task_id)
        except HTTPException:
            raise
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    async def submit_run_message(run_id: str, payload: MessageCreate) -> dict:
        try:
            run = store.get_run(run_id)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        idempotency_key = normalize_optional_message_id(payload.idempotencyKey)
        if idempotency_key is not None:
            existing = find_idempotent_user_message(store, run_id, idempotency_key)
            if existing is not None:
                return {
                    "event": existing,
                    "answeredActions": [],
                    "job": None,
                    "duplicate": True,
                }
        if run["status"] != "waiting_for_user":
            raise HTTPException(
                status_code=409,
                detail={
                    "message": "run is not waiting for user input",
                    "status": run["status"],
                },
            )
        pending_user_actions = [
            action
            for action in store.list_actions(run_id)
            if action.get("kind") == "user_input" and action.get("status") == "pending"
        ]
        if payload.questionId is not None and not any(
            user_action_matches_question(action, payload.questionId)
            for action in pending_user_actions
        ):
            raise HTTPException(
                status_code=400,
                detail=f"unknown pending questionId {payload.questionId}",
            )
        event = store.append_event(
            run["workspace_id"],
            run_id,
            "user.message",
            {
                "message": payload.message,
                "questionId": payload.questionId,
                "resumeMode": payload.resumeMode,
                "idempotencyKey": idempotency_key,
            },
        )
        answered_actions = store.answer_user_input_actions(
            run_id,
            payload.message,
            payload.resumeMode,
            question_id=payload.questionId,
        )
        job = None
        store.update_run_status(run_id, "queued", "queued")
        job = store.enqueue_run(run_id)
        return {
            "event": event,
            "answeredActions": answered_actions,
            "job": job,
            "duplicate": False,
        }

    @app.post("/api/v2/runs/{run_id}/messages")
    async def post_message(_: Auth, run_id: str, payload: MessageCreate) -> dict:
        return await submit_run_message(run_id, payload)

    @app.post("/api/v2/tasks/{task_id}/messages")
    async def post_task_message_alias(_: Auth, task_id: str, payload: MessageCreate) -> dict:
        return await submit_run_message(task_id, payload)

    async def submit_action_decision(
        action_id: str,
        payload: DecisionCreate,
        *,
        task_id: str | None = None,
    ) -> dict:
        try:
            current_action = store.get_action(action_id)
            if task_id is not None and current_action.get("run_id") != task_id:
                raise HTTPException(
                    status_code=404,
                    detail=f"unknown actionId {action_id} for taskId {task_id}",
                )
            run = store.get_run(current_action["run_id"])
            idempotency_key = normalize_optional_message_id(payload.idempotencyKey)
            if idempotency_key is not None:
                existing = find_idempotent_action_decision(
                    store,
                    run["id"],
                    action_id,
                    idempotency_key,
                )
                if existing is not None:
                    return {
                        "action": store.get_action(action_id),
                        "environmentEvidence": None,
                        "job": None,
                        "event": existing,
                        "duplicate": True,
                    }
            if run["status"] != "waiting_for_approval":
                raise HTTPException(
                    status_code=409,
                    detail={
                        "message": "run is not waiting for approval",
                        "status": run["status"],
                    },
                )
            if (
                current_action.get("kind") != "approval"
                or current_action.get("status") != "pending"
            ):
                raise HTTPException(
                    status_code=400,
                    detail=f"unknown pending actionId {action_id}",
                )
            action = store.decide_action(
                action_id,
                payload.decision,
                payload.reason,
                idempotency_key=idempotency_key,
                input_override=payload.input if payload.decision == "approved" else None,
            )
            environment_evidence = persist_approved_environment_evidence(
                settings, store, action
            )
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        job = None
        if run["status"] == "waiting_for_approval":
            if is_pending_environment_collection(environment_evidence):
                store.update_run_status(run["id"], "waiting_for_approval", "collect_environment")
            else:
                store.update_run_status(run["id"], "queued", "queued")
                job = store.enqueue_run(run["id"])
        return {
            "action": action,
            "environmentEvidence": environment_evidence,
            "job": job,
            "duplicate": False,
        }

    @app.post("/api/v2/actions/{action_id}/decisions")
    async def decide_action(_: Auth, action_id: str, payload: DecisionCreate) -> dict:
        return await submit_action_decision(action_id, payload)

    @app.post("/api/v2/tasks/{task_id}/actions/{action_id}/decision")
    async def decide_task_action_alias(
        _: Auth, task_id: str, action_id: str, payload: DecisionCreate
    ) -> dict:
        return await submit_action_decision(action_id, payload, task_id=task_id)

    @app.get("/api/v2/evidence/{evidence_id}")
    async def get_evidence(_: Auth, evidence_id: str) -> dict:
        try:
            return store.get_evidence(evidence_id)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/v2/artifacts/{artifact_id}")
    async def get_artifact(_: Auth, artifact_id: str):
        try:
            artifact = store.get_artifact(artifact_id)
            path = resolve_artifact_path(settings, artifact["relative_path"])
        except (KeyError, ValueError) as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        if not path.exists():
            raise HTTPException(status_code=404, detail="artifact file is missing")
        return FileResponse(path, media_type=artifact["content_type"])

    @app.get("/api/tools")
    @app.get("/api/v2/tools")
    async def list_tools(_: Auth) -> dict:
        return tool_catalog(settings)

    @app.post("/api/tools/{tool_id}/runs", status_code=202)
    @app.post("/api/v2/tools/{tool_id}/runs", status_code=202)
    async def create_tool_run(_: Auth, tool_id: str, payload: ToolRunCreate) -> dict:
        try:
            upload_ids = _tool_run_upload_ids(payload)
            workspace_id = _tool_run_workspace_id(store, payload, upload_ids)
            uploads = (
                store.list_uploads_by_ids(workspace_id, upload_ids)
                if upload_ids
                else []
            )
            params = validate_manual_tool_run(
                settings,
                tool_id,
                len(upload_ids),
                payload.params,
                upload_filenames=[upload["filename"] for upload in uploads],
            )
            run = store.create_tool_run(
                workspace_id=workspace_id,
                tool_id=tool_id,
                params=params,
                upload_ids=upload_ids,
            )
            return _tool_run_response(store, run)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.get("/api/tools/runs")
    @app.get("/api/v2/tools/runs")
    async def list_tool_runs(
        _: Auth,
        toolId: str | None = None,
        workspaceId: str | None = None,
        limit: int = Query(default=50, ge=1, le=200),
    ) -> dict:
        try:
            runs = store.list_tool_runs(
                tool_id=toolId,
                workspace_id=workspaceId,
                limit=limit,
            )
            return {
                "runs": [_tool_run_response(store, run) for run in runs],
                "rawRuns": runs,
            }
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/tools/runs/{run_id}")
    @app.get("/api/v2/tools/runs/{run_id}")
    async def get_tool_run(_: Auth, run_id: str) -> dict:
        try:
            run = store.get_run(run_id)
            if run.get("kind") != "tool_run":
                raise ValueError(f"run {run_id} is not a tool run")
            return _tool_run_response(store, run)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.get("/api/tools/runs/{run_id}/artifacts")
    @app.get("/api/tools/runs/{run_id}/result")
    @app.get("/api/v2/tools/runs/{run_id}/result")
    async def get_tool_run_result(_: Auth, run_id: str) -> dict:
        try:
            run = store.get_run(run_id)
            if run.get("kind") != "tool_run":
                raise ValueError(f"run {run_id} is not a tool run")
            artifact_id = run.get("toolResultArtifactId")
            if not artifact_id:
                raise HTTPException(
                    status_code=409,
                    detail={
                        "message": "tool run result is only available after success",
                        "status": run.get("status"),
                    },
                )
            artifact = store.get_artifact(artifact_id)
            path = resolve_artifact_path(settings, artifact["relative_path"])
            result = json_load_file(path)
            return {
                "runId": run["id"],
                "taskId": run["id"],
                "toolId": run.get("toolId"),
                "resultPath": artifact["relative_path"],
                "run": run,
                "artifact": artifact,
                "result": result,
            }
        except HTTPException:
            raise
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/v2/tools/runs/{run_id}/artifacts")
    async def get_tool_run_artifacts(_: Auth, run_id: str) -> dict:
        try:
            run = store.get_run(run_id)
            if run.get("kind") != "tool_run":
                raise ValueError(f"run {run_id} is not a tool run")
            return store.list_run_artifacts(run_id)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.get("/api/tools/{tool_id}")
    @app.get("/api/v2/tools/{tool_id}")
    async def get_tool(_: Auth, tool_id: str) -> dict:
        try:
            return get_tool_descriptor(settings, tool_id)
        except ValueError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/v2/debug/llm")
    async def get_llm_debug(_: Auth) -> dict:
        return {"llmOutputLogging": debug_log_responses()}

    @app.put("/api/v2/debug/llm")
    async def update_llm_debug(_: Auth, payload: LlmDebugUpdate) -> dict:
        return {"llmOutputLogging": set_debug_log_responses(payload.llmOutputLogging)}

    @app.get("/api/v2/settings/llm")
    async def get_llm_settings(_: Auth) -> dict:
        return {"llm": llm_settings_summary(settings)}

    @app.get("/api/v2/settings/llm/models")
    async def test_llm_models(_: Auth) -> dict:
        return test_response(lambda: list_agent_models(settings))

    @app.post("/api/v2/settings/llm/chat")
    async def test_llm_chat(_: Auth, payload: LlmChatTestCreate) -> dict:
        try:
            message = validate_settings_message(payload.message)
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error
        return test_response(lambda: test_agent_chat(settings, message))

    @app.get("/api/v2/settings/agent-backends")
    async def get_agent_backends(_: Auth) -> dict:
        return {"agentBackends": agent_backends_summary(settings)}

    @app.post("/api/v2/settings/agent-backends/{backend_id}/test")
    async def test_agent_backend(_: Auth, backend_id: str) -> dict:
        return test_response(lambda: agent_backend_diagnostic(settings, backend_id))

    @app.get("/api/v2/settings/domain-adapters")
    async def get_domain_adapters(_: Auth) -> dict:
        return {"domainAdapters": domain_adapter_summaries()}

    @app.get("/api/executors")
    @app.get("/api/v2/executors")
    async def list_executors(_: Auth) -> dict:
        return {"executors": store.list_remote_executors()}

    @app.post("/api/executors", status_code=201)
    @app.post("/api/v2/executors", status_code=201)
    async def create_executor(_: Auth, payload: RemoteExecutorCreate) -> dict:
        try:
            return store.create_remote_executor(normalize_executor_payload(payload.model_dump()))
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.get("/api/executors/{executor_id}")
    @app.get("/api/v2/executors/{executor_id}")
    async def get_executor(_: Auth, executor_id: str) -> dict:
        try:
            return store.get_remote_executor(executor_id)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.patch("/api/executors/{executor_id}")
    @app.patch("/api/v2/executors/{executor_id}")
    async def patch_executor(_: Auth, executor_id: str, payload: RemoteExecutorUpdate) -> dict:
        try:
            updates = normalize_executor_payload(
                payload.model_dump(exclude_unset=True), partial=True
            )
            return store.update_remote_executor(executor_id, updates)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.delete("/api/executors/{executor_id}")
    @app.delete("/api/v2/executors/{executor_id}")
    async def delete_executor(_: Auth, executor_id: str) -> dict:
        try:
            return store.disable_remote_executor(executor_id)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/executor-command-templates")
    @app.get("/api/v2/executor-command-templates")
    async def list_executor_command_templates(_: Auth) -> dict:
        return {
            "enabled": settings.remote_execution_enabled,
            "commands": command_templates(settings),
        }

    @app.get("/api/executor-file-templates")
    @app.get("/api/v2/executor-file-templates")
    async def list_executor_file_templates(_: Auth) -> dict:
        return {
            "enabled": settings.remote_execution_enabled,
            "files": file_templates(settings),
        }

    @app.get("/api/executor-runs")
    @app.get("/api/v2/executor-runs")
    async def list_executor_runs(
        _: Auth,
        executorId: str | None = None,
        limit: int = Query(default=50, ge=1, le=200),
    ) -> dict:
        return {"runs": compact_remote_runs(store.list_remote_runs(executorId, limit))}

    @app.post("/api/executor-runs", status_code=202)
    @app.post("/api/v2/executor-runs", status_code=202)
    async def create_executor_run(_: Auth, payload: RemoteRunCreate) -> dict:
        if not settings.remote_execution_enabled:
            raise HTTPException(status_code=400, detail="remote execution is disabled")
        try:
            executor = store.get_remote_executor(payload.executorId)
        except KeyError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error
        if not executor["enabled"]:
            raise HTTPException(
                status_code=400,
                detail=f"executor {payload.executorId} is disabled",
            )
        template = command_template(settings, payload.commandId)
        if template is None:
            raise HTTPException(status_code=400, detail=f"unknown commandId {payload.commandId}")
        if not template.enabled:
            raise HTTPException(
                status_code=400,
                detail=f"remote command {payload.commandId} is disabled",
            )
        return remote_run_summary(
            store.create_remote_run(
                executor_id=executor["executorId"],
                command_id=template.command_id,
                alias=f"{template.display_name} on {executor['name']}",
                idempotency_key=payload.idempotencyKey,
            )
        )

    @app.get("/api/executor-runs/{run_id}")
    @app.get("/api/v2/executor-runs/{run_id}")
    async def get_executor_run(_: Auth, run_id: str) -> dict:
        try:
            return remote_run_detail(store.get_remote_run(run_id))
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/executor-runs/{run_id}/result")
    @app.get("/api/v2/executor-runs/{run_id}/result")
    async def get_executor_run_result(_: Auth, run_id: str) -> dict:
        try:
            run = store.get_remote_run(run_id)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        if not run.get("result"):
            raise HTTPException(
                status_code=409,
                detail={
                    "message": "remote command result is only available after success",
                    "status": run.get("status"),
                },
            )
        return run["result"]

    @app.get("/api/executor-runs/{run_id}/files/{file_name}")
    @app.get("/api/v2/executor-runs/{run_id}/files/{file_name}")
    async def get_executor_run_file(_: Auth, run_id: str, file_name: str):
        try:
            run = store.get_remote_run(run_id)
            path, media_type = remote_run_file(settings, run, file_name)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        if not path.exists():
            raise HTTPException(status_code=404, detail="remote run file is missing")
        return FileResponse(path, media_type=media_type, filename=path.name)

    @app.get("/api/v2/exports/skills.zip")
    async def export_skills(_: Auth) -> Response:
        try:
            data = build_skills_zip(settings)
        except ValueError as error:
            raise HTTPException(status_code=500, detail=str(error)) from error
        return Response(
            content=data,
            media_type="application/zip",
            headers={"Content-Disposition": 'attachment; filename="skills.zip"'},
        )

    @app.get("/api/v2/exports/tools.zip")
    async def export_tools(_: Auth) -> Response:
        try:
            data = build_tools_zip(settings)
        except ValueError as error:
            raise HTTPException(status_code=500, detail=str(error)) from error
        return Response(
            content=data,
            media_type="application/zip",
            headers={"Content-Disposition": 'attachment; filename="tools.zip"'},
        )

    @app.get("/api/v2/skills")
    async def list_diagnostic_skills(_: Auth) -> dict:
        try:
            return {"skills": list_skills(settings)}
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.get("/api/v2/skills/{skill_id}")
    async def get_diagnostic_skill(_: Auth, skill_id: str) -> dict:
        try:
            return get_skill(settings, skill_id)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.post("/api/v2/skills/imports")
    async def create_skill_import(_: Auth, payload: SkillImportCreate) -> dict:
        try:
            return import_skill(
                settings=settings,
                skill_id=payload.skillId,
                name=payload.name,
                description=payload.description,
                markdown=payload.markdown,
                filename=payload.filename,
            )
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.post("/api/v2/skills/preview")
    async def preview_skills(_: Auth, payload: SkillPreviewCreate) -> dict:
        try:
            return preview_system_context(settings, payload.skillIds)
        except (KeyError, ValueError) as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.get("/api/v2/system-context/resources")
    async def list_system_context_resources(_: Auth) -> dict:
        return {"resources": list_system_context_resource_summaries(store)}

    @app.post("/api/v2/system-context/resources", status_code=201)
    async def create_system_context_resource_api(
        _: Auth, payload: SystemContextResourceCreate
    ) -> dict:
        try:
            return create_system_context_resource(store, payload.model_dump())
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.get("/api/v2/system-context/resources/{context_id}")
    async def get_system_context_resource_api(_: Auth, context_id: str) -> dict:
        try:
            return get_system_context_resource(store, context_id)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.patch("/api/v2/system-context/resources/{context_id}")
    async def patch_system_context_resource_api(
        _: Auth,
        context_id: str,
        payload: SystemContextResourceUpdate,
    ) -> dict:
        try:
            return patch_system_context_resource(
                store,
                context_id,
                payload.model_dump(exclude_unset=True),
            )
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.post("/api/v2/system-context/resources/{context_id}/versions", status_code=201)
    async def create_system_context_version_api(
        _: Auth,
        context_id: str,
        payload: SystemContextVersionCreate,
    ) -> dict:
        try:
            return create_system_context_version(store, context_id, payload.model_dump())
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.patch("/api/v2/system-context/resources/{context_id}/versions/{version_id}")
    async def patch_system_context_version_api(
        _: Auth,
        context_id: str,
        version_id: str,
        payload: SystemContextVersionUpdate,
    ) -> dict:
        try:
            return patch_system_context_version(
                store,
                context_id,
                version_id,
                payload.model_dump(exclude_unset=True),
            )
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.post(
        "/api/v2/system-context/resources/{context_id}/versions/{version_id}/activate"
    )
    async def activate_system_context_version_api(
        _: Auth,
        context_id: str,
        version_id: str,
    ) -> dict:
        try:
            return activate_system_context_version(store, context_id, version_id)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.post("/api/v2/system-context/preview")
    async def preview_system_context_resources_api(
        _: Auth, payload: SystemContextPreviewCreate
    ) -> dict:
        try:
            return preview_system_context_resources(
                store,
                context_ids=payload.contextIds,
                task_kind=payload.taskKind,
                product=payload.product,
                version=payload.version,
                environment=payload.environment,
                instance_id=payload.instanceId,
            )
        except (KeyError, ValueError) as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.get("/api/fetch/endpoints")
    @app.get("/api/v2/fetch/endpoints")
    async def list_fetch_endpoints(_: Auth) -> dict:
        return {
            "enabled": settings.fetch_enabled,
            "allowedHosts": list(settings.fetch_allowed_hosts),
            "endpoints": [
                public_fetch_endpoint(endpoint_with_credential_summary(store, endpoint))
                for endpoint in store.list_fetch_endpoints()
            ],
        }

    @app.get("/api/fetch/runs")
    @app.get("/api/v2/fetch/runs")
    async def list_fetch_runs(
        _: Auth,
        fetchId: str | None = None,
        endpointId: str | None = None,
        fetch_id: str | None = Query(default=None),
        workspaceId: str | None = None,
        limit: int = Query(default=50, ge=1, le=200),
    ) -> dict:
        try:
            endpoint_filter = normalize_fetch_run_filter(fetchId or endpointId or fetch_id)
            runs = store.list_tool_runs(
                tool_id=FETCH_TOOL_ID,
                workspace_id=workspaceId,
                limit=limit,
            )
            if endpoint_filter:
                runs = [
                    run
                    for run in runs
                    if fetch_run_endpoint_id(run) == endpoint_filter
                ]
            return {"enabled": settings.fetch_enabled, "runs": runs}
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.post("/api/fetch/imports/preview")
    @app.post("/api/v2/fetch/imports/preview")
    async def preview_fetch_import(_: Auth, payload: FetchCurlImportPreviewCreate) -> dict:
        try:
            return preview_curl_import(payload.curl)
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.post("/api/v2/fetch/imports")
    async def create_fetch_import(_: Auth, payload: FetchCurlImportCreate) -> dict:
        try:
            endpoint = endpoint_from_curl(
                payload.curl,
                name=payload.name,
                enabled=payload.enabled,
            )
            validate_fetch_credentials_available(settings, endpoint)
            stored = endpoint_for_storage(endpoint)
            created = store.create_fetch_endpoint(
                name=stored["name"],
                method=stored["method"],
                url=stored["url"],
                headers=stored["headers"],
                body=stored.get("body"),
                enabled=stored["enabled"],
                follow_redirects=stored.get("followRedirects", False),
                schema_version=stored.get("schemaVersion", 2),
                refresh_policy=stored.get("refreshPolicy"),
            )
            persist_fetch_credentials(settings, store, created["id"], endpoint)
            return public_fetch_endpoint(
                endpoint_with_credential_summary(store, store.get_fetch_endpoint(created["id"]))
            )
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.post("/api/fetch/endpoints")
    @app.post("/api/v2/fetch/endpoints")
    async def create_fetch_endpoint(_: Auth, payload: FetchEndpointCreate) -> dict:
        try:
            endpoint = normalize_fetch_endpoint(payload.model_dump())
            validate_fetch_credentials_available(settings, endpoint)
            stored = endpoint_for_storage(endpoint)
            created = store.create_fetch_endpoint(
                name=stored["name"],
                method=stored["method"],
                url=stored["url"],
                headers=stored["headers"],
                body=stored.get("body"),
                enabled=stored["enabled"],
                follow_redirects=stored.get("followRedirects", False),
                schema_version=stored.get("schemaVersion", 2),
                refresh_policy=stored.get("refreshPolicy"),
            )
            persist_fetch_credentials(settings, store, created["id"], endpoint)
            return public_fetch_endpoint(
                endpoint_with_credential_summary(store, store.get_fetch_endpoint(created["id"]))
            )
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.get("/api/fetch/endpoints/{endpoint_id}")
    @app.get("/api/v2/fetch/endpoints/{endpoint_id}")
    async def get_fetch_endpoint(_: Auth, endpoint_id: str) -> dict:
        try:
            return public_fetch_endpoint(
                endpoint_with_credential_summary(store, store.get_fetch_endpoint(endpoint_id))
            )
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.patch("/api/fetch/endpoints/{endpoint_id}")
    @app.patch("/api/v2/fetch/endpoints/{endpoint_id}")
    async def patch_fetch_endpoint(_: Auth, endpoint_id: str, payload: FetchEndpointUpdate) -> dict:
        try:
            current = hydrate_fetch_endpoint(settings, store, store.get_fetch_endpoint(endpoint_id))
            updates = {
                key: value
                for key, value in payload.model_dump(exclude_unset=True).items()
                if key == "body" or value is not None
            }
            merged = dict(current)
            merged.update(updates)
            endpoint = normalize_fetch_endpoint(merged)
            validate_fetch_credentials_available(settings, endpoint)
            stored = endpoint_for_storage(endpoint)
            updated = store.update_fetch_endpoint(endpoint_id, stored)
            persist_fetch_credentials(settings, store, endpoint_id, endpoint)
            return public_fetch_endpoint(endpoint_with_credential_summary(store, updated))
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.delete("/api/fetch/endpoints/{endpoint_id}")
    @app.delete("/api/v2/fetch/endpoints/{endpoint_id}")
    async def delete_fetch_endpoint(_: Auth, endpoint_id: str) -> dict:
        try:
            store.delete_fetch_endpoint(endpoint_id)
            return {"deleted": True, "endpointId": endpoint_id}
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.post("/api/fetch/endpoints/{endpoint_id}/runs", status_code=202)
    @app.post("/api/v2/fetch/endpoints/{endpoint_id}/runs", status_code=202)
    async def create_fetch_endpoint_run(
        _: Auth,
        endpoint_id: str,
        payload: FetchRunCreate | None = None,
    ) -> dict:
        try:
            endpoint = store.get_fetch_endpoint(endpoint_id)
            if not endpoint.get("enabled", True):
                raise ValueError(f"fetch endpoint {endpoint_id} is disabled")
            values = fetch_run_payload_values(payload)
            workspace_id = values.pop("workspaceId", None)
            if workspace_id:
                store.get_workspace(workspace_id)
            else:
                workspace = store.create_workspace(
                    f"Run fetch endpoint {endpoint.get('name') or endpoint_id}",
                    "diagnose",
                    "en-US",
                )
                workspace_id = workspace["id"]
            params = normalize_fetch_run_params({"endpointId": endpoint_id, **values})
            validated = validate_manual_tool_run(
                settings,
                FETCH_TOOL_ID,
                0,
                params,
            )
            fetch_run = store.create_tool_run(
                workspace_id=workspace_id,
                tool_id=FETCH_TOOL_ID,
                params=validated,
                upload_ids=[],
            )
            store.set_fetch_endpoint_last_run(endpoint_id, fetch_run["id"])
            return fetch_run
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.post("/api/v2/runs/{run_id}/fetch/{endpoint_id}")
    async def run_fetch_endpoint(
        _: Auth, run_id: str, endpoint_id: str, payload: FetchRunCreate | None = None
    ) -> dict:
        try:
            run = store.get_run(run_id)
            params = normalize_fetch_run_params(
                {
                    "endpointId": endpoint_id,
                    **fetch_run_payload_values(payload, include_workspace=False),
                }
            )
            return execute_fetch_endpoint(
                settings=settings,
                store=store,
                workspace_id=run["workspace_id"],
                run_id=run_id,
                endpoint_id=endpoint_id,
                run_params=params,
            )
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.get("/api/v2/metadata/instances")
    async def list_metadata_instances(_: Auth) -> dict:
        return {"instances": store.list_metadata_instances()}

    @app.get("/api/v2/metadata/instances/{instance_id}")
    async def get_metadata_instance(_: Auth, instance_id: str) -> dict:
        try:
            return store.get_metadata_instance(instance_id)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/v2/metadata/instances/{instance_id}/snapshot")
    async def get_metadata_snapshot(_: Auth, instance_id: str) -> dict:
        try:
            return store.get_metadata_snapshot(instance_id)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.post("/api/v2/metadata/instances/{instance_id}/refresh")
    async def refresh_metadata_instance_api(_: Auth, instance_id: str) -> dict:
        try:
            return refresh_metadata_instance(store, instance_id)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.delete("/api/v2/metadata/instances/{instance_id}")
    async def delete_metadata_instance(_: Auth, instance_id: str) -> dict:
        try:
            store.delete_metadata_instance(instance_id)
            return {"deleted": True, "instanceId": instance_id}
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/v2/metadata/clusters/{cluster_id}")
    async def get_metadata_cluster_api(_: Auth, cluster_id: str) -> dict:
        try:
            return {"cluster": get_metadata_cluster(store, cluster_id)}
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/v2/metadata/clusters/{cluster_id}/nodes")
    async def list_metadata_cluster_nodes_api(_: Auth, cluster_id: str) -> dict:
        try:
            return {
                "clusterId": cluster_id,
                "nodes": list_metadata_cluster_nodes(store, cluster_id),
            }
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/v2/metadata/imports")
    async def list_metadata_imports(
        _: Auth,
        limit: int = Query(default=50, ge=1, le=200),
    ) -> dict:
        return {
            "imports": [
                metadata_import_preview(item) for item in store.list_metadata_imports(limit)
            ]
        }

    @app.get("/api/v2/metadata/imports/{import_id}")
    async def get_metadata_import(_: Auth, import_id: str) -> dict:
        try:
            draft = store.get_metadata_import(import_id)
            return {"import": metadata_import_preview(draft), "snapshot": draft["snapshot"]}
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/v2/metadata/imports/{import_id}/preview")
    async def get_metadata_import_preview(_: Auth, import_id: str) -> dict:
        try:
            return metadata_import_preview(store.get_metadata_import(import_id))
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.post("/api/v2/metadata/imports/preview")
    async def create_metadata_import_preview(_: Auth, payload: MetadataImportCreate) -> dict:
        try:
            return preview_metadata_import(
                store=store,
                instance_id=payload.instanceId,
                template_type=payload.templateType,
                content=payload.content,
                remark=payload.remark,
            )
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.post("/api/v2/metadata/imports/fetch/preview")
    async def create_metadata_fetch_preview(
        _: Auth, payload: MetadataImportFetchCreate
    ) -> dict:
        try:
            return preview_metadata_import_from_url(
                settings=settings,
                store=store,
                instance_id=payload.instanceId,
                template_type=payload.templateType,
                url=payload.url,
                remark=payload.remark,
            )
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.post("/api/v2/metadata/imports/{import_id}/confirm")
    async def confirm_metadata_import_draft(_: Auth, import_id: str) -> dict:
        try:
            return confirm_metadata_import(store, import_id)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.post("/api/v2/metadata/imports/fetch")
    async def create_metadata_fetch_import(
        _: Auth, payload: MetadataImportFetchCreate
    ) -> dict:
        try:
            return import_metadata_from_url(
                settings=settings,
                store=store,
                instance_id=payload.instanceId,
                template_type=payload.templateType,
                url=payload.url,
                remark=payload.remark,
            )
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.post("/api/v2/metadata/snapshots/fetch")
    async def fetch_metadata_snapshot_api(
        _: Auth, payload: MetadataImportFetchCreate
    ) -> dict:
        try:
            return fetch_metadata_snapshot_from_url(
                settings=settings,
                instance_id=payload.instanceId,
                template_type=payload.templateType,
                url=payload.url,
                remark=payload.remark,
            )
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.post("/api/v2/metadata/imports")
    async def create_metadata_import(_: Auth, payload: MetadataImportCreate) -> dict:
        try:
            return import_metadata(
                store=store,
                instance_id=payload.instanceId,
                template_type=payload.templateType,
                content=payload.content,
                remark=payload.remark,
            )
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.post("/api/v2/metadata/field-types")
    async def get_metadata_field_types(_: Auth, payload: MetadataFieldTypesQuery) -> dict:
        try:
            return query_field_types(
                store=store,
                instance_id=payload.instanceId,
                database=payload.database,
                measurement=payload.measurement,
                retention_policy=payload.retentionPolicy,
                field=payload.field,
            )
        except (KeyError, ValueError) as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.post("/api/v2/metadata/tag-fields")
    async def get_metadata_tag_fields(_: Auth, payload: MetadataFieldTypesQuery) -> dict:
        try:
            return query_field_types(
                store=store,
                instance_id=payload.instanceId,
                database=payload.database,
                measurement=payload.measurement,
                retention_policy=payload.retentionPolicy,
                tags_only=True,
            )
        except (KeyError, ValueError) as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.post("/api/v2/cases")
    async def create_case(_: Auth, payload: CaseCreate) -> dict:
        try:
            return create_manual_case(store, payload.model_dump())
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.post("/api/v2/runs/{run_id}/case")
    async def create_run_case(_: Auth, run_id: str, payload: CaseUpdate | None = None) -> dict:
        try:
            overrides = payload.model_dump(exclude_none=True) if payload else {}
            return create_task_case(store, run_id, overrides)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.post("/api/v2/tasks/{task_id}/case")
    async def create_task_case_alias(
        _: Auth, task_id: str, payload: CaseUpdate | None = None
    ) -> dict:
        return await create_run_case(_, task_id, payload)

    @app.get("/api/v2/cases")
    async def search_cases(
        _: Auth,
        query: str | None = None,
        limit: int = Query(default=5, ge=1, le=50),
        includeDisabled: bool = False,
    ) -> dict:
        return {
            "cases": store.search_cases(
                query=query,
                limit=limit,
                include_disabled=includeDisabled,
            )
        }

    @app.get("/api/v2/cases/imports")
    async def list_case_imports(
        _: Auth,
        limit: int = Query(default=50, ge=1, le=200),
    ) -> dict:
        return {
            "imports": [
                case_import_preview(item) for item in store.list_case_imports(limit=limit)
            ]
        }

    @app.get("/api/v2/cases/imports/{import_id}")
    async def get_case_import(_: Auth, import_id: str) -> dict:
        try:
            return {"import": case_import_preview(store.get_case_import(import_id))}
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.post("/api/v2/cases/imports", status_code=201)
    async def create_case_import_api(_: Auth, request: Request) -> dict:
        try:
            content, filename = await _case_import_create_input(request)
            result = preview_case_import(
                store=store,
                content=content,
                filename=filename,
            )
            return {**result, "draft": result["import"]}
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.post("/api/v2/cases/imports/preview")
    async def preview_case_import_api(_: Auth, payload: CaseImportPreviewCreate) -> dict:
        return preview_case_import(
            store=store,
            content=payload.content,
            filename=payload.filename,
        )

    @app.post("/api/v2/cases/imports/{import_id}/messages")
    async def append_case_import_message_api(
        _: Auth,
        import_id: str,
        payload: CaseImportMessageCreate,
    ) -> dict:
        try:
            return append_case_import_message(store, import_id, payload.message)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.patch("/api/v2/cases/imports/{import_id}")
    async def patch_case_import(
        _: Auth,
        import_id: str,
        payload: CaseUpdate,
    ) -> dict:
        try:
            return update_case_import_draft(
                store,
                import_id,
                payload.model_dump(exclude_unset=True),
            )
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.post("/api/v2/cases/imports/{import_id}/confirm")
    async def confirm_case_import_api(
        _: Auth,
        import_id: str,
        payload: CaseImportConfirmCreate | None = None,
    ) -> dict:
        try:
            overrides = payload.model_dump(exclude_none=True) if payload else {}
            return confirm_case_import(store, import_id, overrides)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.get("/api/v2/cases/{case_id}")
    async def get_case(_: Auth, case_id: str) -> dict:
        try:
            return store.get_case(case_id)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.patch("/api/v2/cases/{case_id}")
    async def patch_case(_: Auth, case_id: str, payload: CaseUpdate) -> dict:
        try:
            return update_case(store, case_id, payload.model_dump(exclude_unset=True))
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.post("/api/v2/mcp/readonly")
    async def readonly_mcp(_: Auth, request: Any) -> Any:
        return readonly_mcp_response(settings, store, request)

    @app.post("/api/v2/mcp/task/{run_id}")
    async def task_mcp(_: Auth, run_id: str, request: Any) -> Any:
        return task_mcp_response(settings, store, run_id, request)

    @app.get("/", include_in_schema=False)
    async def webui_root() -> FileResponse:
        return serve_webui_asset(settings, "")

    @app.get("/{asset_path:path}", include_in_schema=False)
    async def webui_asset(asset_path: str) -> FileResponse:
        return serve_webui_asset(settings, asset_path)

    return app


def serve_webui_asset(settings: Settings, asset_path: str) -> FileResponse:
    try:
        path = resolve_webui_asset(settings.webui_dir, asset_path)
    except WebuiStaticNotFound as exc:
        raise HTTPException(status_code=404, detail=str(exc)) from exc
    return FileResponse(path)


def normalize_executor_payload(payload: dict, partial: bool = False) -> dict:
    result = dict(payload)
    if not partial or "name" in result:
        result["name"] = normalize_non_empty(result.get("name"), "name", 120)
    if not partial or "host" in result:
        result["host"] = normalize_non_empty(result.get("host"), "host", 255)
    if not partial or "user" in result:
        result["user"] = normalize_non_empty(result.get("user"), "user", 64)
    if "tags" in result and result["tags"] is not None:
        result["tags"] = normalize_tags(result["tags"])
    if "notes" in result and isinstance(result["notes"], str):
        result["notes"] = result["notes"].strip() or None
    return result


def normalize_non_empty(value: object, field: str, max_chars: int) -> str:
    text = str(value or "").strip()
    if not text:
        raise ValueError(f"{field} must not be empty")
    if len(text) > max_chars:
        raise ValueError(f"{field} exceeds maximum length of {max_chars}")
    return text


def normalize_tags(tags: list[str]) -> list[str]:
    normalized = []
    for tag in tags:
        value = str(tag).strip()
        if not value:
            continue
        if len(value) > 40:
            raise ValueError("executor tag exceeds maximum length of 40")
        if value not in normalized:
            normalized.append(value)
    if len(normalized) > 20:
        raise ValueError("executor tags exceed maximum length of 20")
    return normalized


def normalize_fetch_run_filter(value: str | None) -> str | None:
    normalized = str(value or "").strip()
    if not normalized:
        return None
    if len(normalized) > 200:
        raise ValueError("fetch run filter is too long")
    return normalized


def fetch_run_payload_values(
    payload: FetchRunCreate | None,
    include_workspace: bool = True,
) -> dict:
    values = payload.model_dump(exclude_none=True) if payload else {}
    if not include_workspace:
        values.pop("workspaceId", None)
    return values


def fetch_run_endpoint_id(run: dict) -> str | None:
    params = run.get("toolParams")
    if not isinstance(params, dict):
        return None
    endpoint_id = params.get("endpointId") or params.get("fetchId")
    return endpoint_id if isinstance(endpoint_id, str) else None


def compact_remote_runs(runs: list[dict]) -> list[dict]:
    return [remote_run_summary(run) for run in runs]


def remote_run_summary(run: dict) -> dict:
    return {
        "taskId": run["taskId"],
        "runId": run["taskId"],
        "alias": run.get("alias"),
        "url": f"/api/v2/executor-runs/{run['taskId']}",
        "taskKind": run["taskKind"],
        "sessionId": None,
        "analysisMode": "diagnose",
        "analysisLanguage": "zh-CN",
        "operation": run.get("operation", "command"),
        "status": run["status"],
        "phase": run.get("phase"),
        "createdAt": run["createdAt"],
    }


def remote_run_detail(run: dict) -> dict:
    result = dict(remote_run_summary(run))
    result.update(
        {
            "attempts": run.get("attempts", 0),
            "remoteExecutorId": run.get("remoteExecutorId"),
            "remoteCommandId": (
                run.get("remoteCommandId") if run.get("operation") != "file_collection" else None
            ),
            "remoteFileId": (
                run.get("remoteCommandId") if run.get("operation") == "file_collection" else None
            ),
            "input": run.get("input"),
            "error": run.get("error"),
            "updatedAt": run.get("updatedAt"),
        }
    )
    return result


def remote_run_file(settings: Settings, run: dict, file_name: str) -> tuple[Path, str]:
    result = run.get("result")
    if not isinstance(result, dict):
        raise ValueError("remote run result is not available")
    payload = result.get("result")
    if not isinstance(payload, dict):
        raise ValueError("remote run result payload is not available")
    if file_name == "result":
        relative = result.get("resultPath")
        media_type = "application/json"
    elif file_name == "stdout":
        relative = payload.get("stdoutPath")
        media_type = "text/plain"
    elif file_name == "stderr":
        relative = payload.get("stderrPath")
        media_type = "text/plain"
    elif file_name in {"collected", "file"}:
        relative = payload.get("collectedFilePath")
        media_type = "application/octet-stream"
    else:
        raise ValueError("remote run file must be one of result, stdout, stderr, collected")
    if not isinstance(relative, str) or not relative:
        raise ValueError(f"remote run {file_name} file is not available")
    data_dir = settings.data_dir.resolve()
    path = (settings.data_dir / relative).resolve()
    if data_dir != path and data_dir not in path.parents:
        raise ValueError("remote run file path escapes data_dir")
    return path, media_type


def json_load_file(path: Path) -> dict:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except OSError as error:
        raise ValueError(f"artifact file is missing: {error}") from error
    except json.JSONDecodeError as error:
        raise ValueError(f"artifact is not JSON: {error}") from error
    if not isinstance(value, dict):
        raise ValueError("artifact JSON is not an object")
    return value


app = create_app()
