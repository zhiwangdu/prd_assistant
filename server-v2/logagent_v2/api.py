from __future__ import annotations

import json
from contextlib import asynccontextmanager
from pathlib import Path
from typing import Annotated, Literal

from fastapi import Depends, FastAPI, File, HTTPException, Query, Request, UploadFile
from fastapi.responses import FileResponse, Response
from pydantic import BaseModel, Field

from .analysis import get_run_analysis
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
)
from .config import Settings
from .environment import persist_approved_environment_evidence
from .fetch import (
    endpoint_from_curl,
    endpoint_for_storage,
    endpoint_with_credential_summary,
    execute_fetch_endpoint,
    hydrate_fetch_endpoint,
    normalize_fetch_endpoint,
    persist_fetch_credentials,
    preview_curl_import,
    public_fetch_endpoint,
    validate_fetch_credentials_available,
)
from .exports import build_skills_zip, build_tools_zip
from .ids import new_id
from .metadata import (
    confirm_metadata_import,
    import_metadata_from_url,
    import_metadata,
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
from .remote_execution import command_template, command_templates
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
from .store import Store
from .tools import get_tool_descriptor, tool_descriptors, validate_manual_tool_run
from .webui_static import WebuiStaticNotFound, resolve_webui_asset
from .worker import JobRunner


class WorkspaceCreate(BaseModel):
    question: str = Field(min_length=1, max_length=20000)
    mode: Literal["diagnose", "code_investigation", "fix"] = "diagnose"
    language: Literal["zh-CN", "en-US"] = "zh-CN"
    skillIds: list[str] = Field(default_factory=list, max_length=20)


class WorkspaceUpdate(BaseModel):
    question: str | None = Field(default=None, min_length=1, max_length=20000)
    mode: Literal["diagnose", "code_investigation", "fix"] | None = None
    language: Literal["zh-CN", "en-US"] | None = None
    skillIds: list[str] | None = Field(default=None, max_length=20)


class MessageCreate(BaseModel):
    message: str = Field(min_length=1, max_length=20000)
    resumeMode: Literal["continue", "finalize"] = "continue"


class DecisionCreate(BaseModel):
    decision: Literal["approved", "rejected"]
    reason: str | None = Field(default=None, max_length=2000)


class MetadataImportCreate(BaseModel):
    instanceId: str = Field(min_length=1, max_length=200)
    templateType: Literal["json", "yaml", "opengemini"] = "json"
    content: str = Field(min_length=1)
    filename: str | None = Field(default=None, max_length=300)
    remark: str | None = Field(default=None, max_length=120)


class MetadataImportFetchCreate(BaseModel):
    instanceId: str = Field(min_length=1, max_length=200)
    templateType: Literal["json", "yaml", "opengemini"] = "opengemini"
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
    skillIds: list[str] = Field(default_factory=list, max_length=20)


class FetchEndpointCreate(BaseModel):
    name: str = Field(min_length=1, max_length=200)
    method: Literal["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD"] = "GET"
    url: str = Field(min_length=1, max_length=2000)
    headers: dict[str, str] = Field(default_factory=dict)
    body: str | None = Field(default=None, max_length=200000)
    enabled: bool = True


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


class ToolRunCreate(BaseModel):
    workspaceId: str = Field(min_length=1, max_length=120)
    uploadIds: list[str] = Field(default_factory=list, max_length=100)
    params: dict = Field(default_factory=dict)


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
        return store.create_workspace(
            payload.question,
            payload.mode,
            payload.language,
            skill_ids=payload.skillIds,
        )

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
                skill_ids=payload.skillIds,
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

    @app.post("/api/v2/workspaces/{workspace_id}/uploads")
    async def upload_file(_: Auth, workspace_id: str, file: UploadFile = File(...)) -> dict:
        try:
            store.get_workspace(workspace_id)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        data = await file.read(settings.max_upload_bytes + 1)
        if len(data) > settings.max_upload_bytes:
            raise HTTPException(status_code=413, detail="upload exceeds max_upload_bytes")
        artifact = write_artifact_bytes(
            settings=settings,
            store=store,
            workspace_id=workspace_id,
            filename=file.filename or "upload.bin",
            data=data,
            content_type=file.content_type or "application/octet-stream",
            schema_name=None,
            preview={"filename": file.filename or "upload.bin", "sizeBytes": len(data)},
        )
        upload = store.create_upload(workspace_id, file.filename or "upload.bin", artifact["id"])
        return {"upload": upload, "artifact": artifact}

    @app.post("/api/v2/workspaces/{workspace_id}/uploads/batch")
    async def upload_files(
        _: Auth,
        workspace_id: str,
        files: list[UploadFile] = File(...),
    ) -> dict:
        try:
            store.get_workspace(workspace_id)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        results = []
        for file in files:
            data = await file.read(settings.max_upload_bytes + 1)
            if len(data) > settings.max_upload_bytes:
                raise HTTPException(
                    status_code=413,
                    detail=f"upload {file.filename or 'upload.bin'} exceeds max_upload_bytes",
                )
            filename = file.filename or "upload.bin"
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
        temp_relative_path = (
            f"tmp/upload_sessions/{session_id}/{safe_filename(payload.filename)}"
        )
        session = store.create_upload_session(
            session_id=session_id,
            workspace_id=workspace_id,
            filename=payload.filename,
            content_type=payload.contentType or "application/octet-stream",
            expected_size_bytes=payload.sizeBytes,
            temp_relative_path=temp_relative_path,
        )
        return {"session": session}

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
        data = await request.body()
        if offset != session["received_bytes"]:
            raise HTTPException(
                status_code=409,
                detail=f"chunk offset {offset} does not match received_bytes "
                f"{session['received_bytes']}",
            )
        next_offset = offset + len(data)
        expected_size = session.get("expected_size_bytes")
        if expected_size is not None and next_offset > expected_size:
            raise HTTPException(status_code=400, detail="chunk exceeds expected upload size")
        if next_offset > settings.max_upload_bytes:
            raise HTTPException(status_code=413, detail="upload exceeds max_upload_bytes")
        path = resolve_artifact_path(settings, session["temp_relative_path"])
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("r+b" if path.exists() else "wb") as target:
            target.seek(offset)
            target.write(data)
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

    @app.get("/api/v2/runs")
    async def list_runs(_: Auth, workspaceId: str | None = None) -> dict:
        try:
            return {"runs": store.list_runs(workspaceId)}
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/v2/runs/{run_id}")
    async def get_run(_: Auth, run_id: str) -> dict:
        try:
            return store.get_run(run_id)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/v2/runs/{run_id}/timeline")
    async def get_timeline(_: Auth, run_id: str) -> dict:
        try:
            return {"events": store.list_timeline(run_id)}
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/v2/runs/{run_id}/evidence")
    async def list_evidence(_: Auth, run_id: str) -> dict:
        try:
            return {"evidence": store.list_evidence(run_id)}
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/v2/runs/{run_id}/artifacts")
    async def list_run_artifacts(_: Auth, run_id: str) -> dict:
        try:
            return store.list_run_artifacts(run_id)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/v2/runs/{run_id}/analysis")
    async def get_analysis(_: Auth, run_id: str) -> dict:
        try:
            return get_run_analysis(settings, store, run_id)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/v2/runs/{run_id}/result")
    async def get_result(_: Auth, run_id: str) -> dict:
        try:
            return get_run_result(settings, store, run_id)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.post("/api/v2/runs/{run_id}/messages")
    async def post_message(_: Auth, run_id: str, payload: MessageCreate) -> dict:
        try:
            run = store.get_run(run_id)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        event = store.append_event(
            run["workspace_id"],
            run_id,
            "user.message",
            {"message": payload.message, "resumeMode": payload.resumeMode},
        )
        answered_actions = store.answer_user_input_actions(
            run_id, payload.message, payload.resumeMode
        )
        job = None
        if run["status"] == "waiting_for_user":
            store.update_run_status(run_id, "queued", "queued")
            job = store.enqueue_run(run_id)
        return {"event": event, "answeredActions": answered_actions, "job": job}

    @app.post("/api/v2/actions/{action_id}/decisions")
    async def decide_action(_: Auth, action_id: str, payload: DecisionCreate) -> dict:
        try:
            action = store.decide_action(action_id, payload.decision, payload.reason)
            environment_evidence = persist_approved_environment_evidence(
                settings, store, action
            )
            run = store.get_run(action["run_id"])
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        job = None
        if run["status"] == "waiting_for_approval":
            store.update_run_status(run["id"], "queued", "queued")
            job = store.enqueue_run(run["id"])
        return {"action": action, "environmentEvidence": environment_evidence, "job": job}

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

    @app.get("/api/v2/tools")
    async def list_tools(_: Auth) -> dict:
        return {"tools": tool_descriptors(settings)}

    @app.post("/api/v2/tools/{tool_id}/runs", status_code=202)
    async def create_tool_run(_: Auth, tool_id: str, payload: ToolRunCreate) -> dict:
        try:
            store.get_workspace(payload.workspaceId)
            params = validate_manual_tool_run(
                settings,
                tool_id,
                len(payload.uploadIds),
                payload.params,
            )
            return store.create_tool_run(
                workspace_id=payload.workspaceId,
                tool_id=tool_id,
                params=params,
                upload_ids=payload.uploadIds,
            )
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.get("/api/v2/tools/runs")
    async def list_tool_runs(
        _: Auth,
        toolId: str | None = None,
        workspaceId: str | None = None,
        limit: int = Query(default=50, ge=1, le=200),
    ) -> dict:
        try:
            return {
                "runs": store.list_tool_runs(
                    tool_id=toolId,
                    workspace_id=workspaceId,
                    limit=limit,
                )
            }
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/v2/tools/runs/{run_id}")
    async def get_tool_run(_: Auth, run_id: str) -> dict:
        try:
            run = store.get_run(run_id)
            if run.get("kind") != "tool_run":
                raise ValueError(f"run {run_id} is not a tool run")
            return run
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.get("/api/v2/tools/runs/{run_id}/result")
    async def get_tool_run_result(_: Auth, run_id: str) -> dict:
        try:
            run = store.get_run(run_id)
            if run.get("kind") != "tool_run":
                raise ValueError(f"run {run_id} is not a tool run")
            artifact_id = run.get("toolResultArtifactId")
            if not artifact_id:
                raise ValueError("tool run result is not available")
            artifact = store.get_artifact(artifact_id)
            path = resolve_artifact_path(settings, artifact["relative_path"])
            return {
                "run": run,
                "artifact": artifact,
                "result": json_load_file(path),
            }
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

    @app.get("/api/v2/executors")
    async def list_executors(_: Auth) -> dict:
        return {"executors": store.list_remote_executors()}

    @app.post("/api/v2/executors", status_code=201)
    async def create_executor(_: Auth, payload: RemoteExecutorCreate) -> dict:
        try:
            return store.create_remote_executor(normalize_executor_payload(payload.model_dump()))
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.get("/api/v2/executors/{executor_id}")
    async def get_executor(_: Auth, executor_id: str) -> dict:
        try:
            return store.get_remote_executor(executor_id)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

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

    @app.delete("/api/v2/executors/{executor_id}")
    async def delete_executor(_: Auth, executor_id: str) -> dict:
        try:
            return store.disable_remote_executor(executor_id)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/v2/executor-command-templates")
    async def list_executor_command_templates(_: Auth) -> dict:
        return {
            "enabled": settings.remote_execution_enabled,
            "commands": command_templates(settings),
        }

    @app.get("/api/v2/executor-runs")
    async def list_executor_runs(
        _: Auth,
        executorId: str | None = None,
        limit: int = Query(default=50, ge=1, le=200),
    ) -> dict:
        return {"runs": compact_remote_runs(store.list_remote_runs(executorId, limit))}

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

    @app.get("/api/v2/executor-runs/{run_id}")
    async def get_executor_run(_: Auth, run_id: str) -> dict:
        try:
            return remote_run_detail(store.get_remote_run(run_id))
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.get("/api/v2/executor-runs/{run_id}/result")
    async def get_executor_run_result(_: Auth, run_id: str) -> dict:
        try:
            run = store.get_remote_run(run_id)
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        if not run.get("result"):
            raise HTTPException(status_code=404, detail="remote run result is not available")
        return run["result"]

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
            )
            persist_fetch_credentials(settings, store, created["id"], endpoint)
            return public_fetch_endpoint(
                endpoint_with_credential_summary(store, store.get_fetch_endpoint(created["id"]))
            )
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

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
            )
            persist_fetch_credentials(settings, store, created["id"], endpoint)
            return public_fetch_endpoint(
                endpoint_with_credential_summary(store, store.get_fetch_endpoint(created["id"]))
            )
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.get("/api/v2/fetch/endpoints/{endpoint_id}")
    async def get_fetch_endpoint(_: Auth, endpoint_id: str) -> dict:
        try:
            return public_fetch_endpoint(
                endpoint_with_credential_summary(store, store.get_fetch_endpoint(endpoint_id))
            )
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

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

    @app.delete("/api/v2/fetch/endpoints/{endpoint_id}")
    async def delete_fetch_endpoint(_: Auth, endpoint_id: str) -> dict:
        try:
            store.delete_fetch_endpoint(endpoint_id)
            return {"deleted": True, "endpointId": endpoint_id}
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.post("/api/v2/runs/{run_id}/fetch/{endpoint_id}")
    async def run_fetch_endpoint(_: Auth, run_id: str, endpoint_id: str) -> dict:
        try:
            run = store.get_run(run_id)
            return execute_fetch_endpoint(
                settings=settings,
                store=store,
                workspace_id=run["workspace_id"],
                run_id=run_id,
                endpoint_id=endpoint_id,
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
    async def readonly_mcp(_: Auth, request: dict) -> dict:
        return readonly_mcp_response(settings, store, request)

    @app.post("/api/v2/mcp/task/{run_id}")
    async def task_mcp(_: Auth, run_id: str, request: dict) -> dict:
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


def compact_remote_runs(runs: list[dict]) -> list[dict]:
    return [remote_run_summary(run) for run in runs]


def remote_run_summary(run: dict) -> dict:
    return {
        "taskId": run["taskId"],
        "alias": run.get("alias"),
        "taskKind": run["taskKind"],
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
            "remoteCommandId": run.get("remoteCommandId"),
            "error": run.get("error"),
            "updatedAt": run.get("updatedAt"),
        }
    )
    return result


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
