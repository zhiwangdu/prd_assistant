from __future__ import annotations

from contextlib import asynccontextmanager
from typing import Annotated, Literal

from fastapi import Depends, FastAPI, File, HTTPException, Query, UploadFile
from fastapi.responses import FileResponse, Response
from pydantic import BaseModel, Field

from .artifacts import resolve_artifact_path, write_artifact_bytes
from .case_memory import create_manual_case, create_task_case, update_case
from .config import Settings
from .fetch import (
    endpoint_from_curl,
    execute_fetch_endpoint,
    fetch_catalog_descriptor,
    normalize_fetch_endpoint,
    preview_curl_import,
    public_fetch_endpoint,
)
from .exports import build_skills_zip, build_tools_zip
from .metadata import (
    confirm_metadata_import,
    import_metadata_from_url,
    import_metadata,
    metadata_import_preview,
    preview_metadata_import,
    preview_metadata_import_from_url,
    query_field_types,
)
from .mcp import readonly_mcp_response, task_mcp_response
from .security import auth_dependency
from .skills import get_skill, import_skill, list_skills, preview_system_context
from .store import Store
from .tools import tool_descriptors
from .worker import JobRunner


class WorkspaceCreate(BaseModel):
    question: str = Field(min_length=1, max_length=20000)
    mode: Literal["diagnose", "code_investigation", "fix"] = "diagnose"
    language: Literal["zh-CN", "en-US"] = "zh-CN"
    skillIds: list[str] = Field(default_factory=list, max_length=20)


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
        return {"tools": [*tool_descriptors(settings), fetch_catalog_descriptor(settings)]}

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
                public_fetch_endpoint(endpoint) for endpoint in store.list_fetch_endpoints()
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
            created = store.create_fetch_endpoint(
                name=endpoint["name"],
                method=endpoint["method"],
                url=endpoint["url"],
                headers=endpoint["headers"],
                body=endpoint.get("body"),
                enabled=endpoint["enabled"],
            )
            return public_fetch_endpoint(created)
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.post("/api/v2/fetch/endpoints")
    async def create_fetch_endpoint(_: Auth, payload: FetchEndpointCreate) -> dict:
        try:
            endpoint = normalize_fetch_endpoint(payload.model_dump())
            created = store.create_fetch_endpoint(
                name=endpoint["name"],
                method=endpoint["method"],
                url=endpoint["url"],
                headers=endpoint["headers"],
                body=endpoint.get("body"),
                enabled=endpoint["enabled"],
            )
            return public_fetch_endpoint(created)
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error

    @app.get("/api/v2/fetch/endpoints/{endpoint_id}")
    async def get_fetch_endpoint(_: Auth, endpoint_id: str) -> dict:
        try:
            return public_fetch_endpoint(store.get_fetch_endpoint(endpoint_id))
        except KeyError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error

    @app.patch("/api/v2/fetch/endpoints/{endpoint_id}")
    async def patch_fetch_endpoint(_: Auth, endpoint_id: str, payload: FetchEndpointUpdate) -> dict:
        try:
            current = store.get_fetch_endpoint(endpoint_id)
            updates = {
                key: value
                for key, value in payload.model_dump(exclude_unset=True).items()
                if key == "body" or value is not None
            }
            merged = dict(current)
            merged.update(updates)
            endpoint = normalize_fetch_endpoint(merged)
            updated = store.update_fetch_endpoint(endpoint_id, endpoint)
            return public_fetch_endpoint(updated)
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

    return app


app = create_app()
