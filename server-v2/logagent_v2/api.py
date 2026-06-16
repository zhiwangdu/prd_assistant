from __future__ import annotations

from contextlib import asynccontextmanager
from typing import Annotated, Literal

from fastapi import Depends, FastAPI, File, HTTPException, Query, UploadFile
from fastapi.responses import FileResponse
from pydantic import BaseModel, Field

from .artifacts import resolve_artifact_path, write_artifact_bytes
from .case_memory import create_manual_case, create_task_case, update_case
from .config import Settings
from .metadata import import_metadata, query_field_types
from .mcp import readonly_mcp_response, task_mcp_response
from .security import auth_dependency
from .store import Store
from .tools import tool_descriptors
from .worker import JobRunner


class WorkspaceCreate(BaseModel):
    question: str = Field(min_length=1, max_length=20000)
    mode: Literal["diagnose", "code_investigation", "fix"] = "diagnose"
    language: Literal["zh-CN", "en-US"] = "zh-CN"


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
        return store.create_workspace(payload.question, payload.mode, payload.language)

    @app.get("/api/v2/workspaces")
    async def list_workspaces(_: Auth) -> dict:
        return {"workspaces": store.list_workspaces()}

    @app.get("/api/v2/workspaces/{workspace_id}")
    async def get_workspace(_: Auth, workspace_id: str) -> dict:
        try:
            return store.get_workspace(workspace_id)
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

    @app.post("/api/v2/workspaces/{workspace_id}/runs")
    async def create_run(_: Auth, workspace_id: str) -> dict:
        try:
            return store.create_run(workspace_id)
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
        job = None
        if run["status"] == "waiting_for_user":
            store.update_run_status(run_id, "queued", "queued")
            job = store.enqueue_run(run_id)
        return {"event": event, "job": job}

    @app.post("/api/v2/actions/{action_id}/decisions")
    async def decide_action(_: Auth, action_id: str, payload: DecisionCreate) -> dict:
        try:
            action = store.decide_action(action_id, payload.decision, payload.reason)
            run = store.get_run(action["run_id"])
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        job = None
        if run["status"] == "waiting_for_approval":
            store.update_run_status(run["id"], "queued", "queued")
            job = store.enqueue_run(run["id"])
        return {"action": action, "job": job}

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

    @app.delete("/api/v2/metadata/instances/{instance_id}")
    async def delete_metadata_instance(_: Auth, instance_id: str) -> dict:
        try:
            store.delete_metadata_instance(instance_id)
            return {"deleted": True, "instanceId": instance_id}
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

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
        return readonly_mcp_response(store, request)

    @app.post("/api/v2/mcp/task/{run_id}")
    async def task_mcp(_: Auth, run_id: str, request: dict) -> dict:
        return task_mcp_response(settings, store, run_id, request)

    return app


app = create_app()
