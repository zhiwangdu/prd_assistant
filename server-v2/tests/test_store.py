from __future__ import annotations

import base64
import gzip
import io
import json
import os
import sys
import tarfile
import tempfile
import asyncio
import threading
import unittest
import zipfile
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

from logagent_v2.agent import AgentRuntime
from logagent_v2.alias import fallback_run_alias, normalize_run_alias
from logagent_v2.analysis import get_run_analysis, get_run_artifacts
from logagent_v2.artifacts import (
    resolve_artifact_path,
    safe_filename,
    write_artifact_bytes,
    write_artifact_file,
)
from logagent_v2.case_memory import (
    append_case_import_message,
    confirm_case_import,
    create_manual_case,
    create_task_case,
    preview_case_import,
    update_case as update_case_record,
    update_case_import_draft,
)
from logagent_v2.config import (
    HuaweiPackageSyncSettings,
    RemoteCommandTemplate,
    Settings,
    ToolDefinition,
    parse_remote_commands_env,
    parse_tools_env,
)
from logagent_v2.environment import persist_approved_environment_evidence
from logagent_v2.evidence import build_initial_evidence
from logagent_v2.exports import build_skills_zip, build_tools_zip
from logagent_v2.fetch import (
    endpoint_for_storage,
    endpoint_from_curl,
    endpoint_with_credential_summary,
    execute_fetch_endpoint,
    hydrate_fetch_endpoint,
    persist_fetch_credentials,
    preview_curl_import,
    public_fetch_endpoint,
    validate_fetch_credentials_available,
    validate_url_allowed,
)
from logagent_v2.final_answer import (
    FinalAnswerValidationError,
    normalize_and_validate_final_answer,
)
from logagent_v2.mcp import readonly_mcp_response, task_mcp_response
from logagent_v2.metadata import (
    confirm_metadata_import,
    import_metadata_from_url,
    import_metadata,
    metadata_tool_descriptors,
    preview_metadata_import,
    preview_metadata_import_from_url,
    query_field_types,
    refresh_metadata_instance,
)
from logagent_v2.results import get_run_result
from logagent_v2.llm import debug_log_responses, set_debug_log_responses
from logagent_v2.remote_execution import command_templates, strict_host_key_checking_value
from logagent_v2.settings_api import (
    agent_backend_diagnostic,
    agent_backends_summary,
    domain_adapter_summaries,
    list_agent_models,
    llm_settings_summary,
    test_agent_chat,
    test_response,
)
from logagent_v2.skills import import_skill, list_skills
from logagent_v2.store import Store
from logagent_v2.system_context import (
    activate_system_context_version,
    create_system_context_resource,
    create_system_context_version,
    list_system_context_resource_summaries,
    preview_system_context_resources,
)
import logagent_v2.tools as tools_module
from logagent_v2.tools import (
    execute_tool_run,
    findings_from_stdout,
    parse_json,
    summary_from_stdout,
    tool_descriptors,
    validate_manual_tool_run,
    validate_tool_run_params,
)
from logagent_v2.webui_static import WebuiStaticNotFound, resolve_webui_asset
from logagent_v2.worker import JobRunner


class StoreTests(unittest.TestCase):
    def test_static_webui_resolves_index_assets_and_spa_fallback(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            webui_dir = tmp_path / "webui-out"
            assets_dir = webui_dir / "assets"
            assets_dir.mkdir(parents=True)
            (webui_dir / "index.html").write_text(
                "<html><body><div id=\"root\">LogAgent V2 UI</div></body></html>",
                encoding="utf-8",
            )
            (assets_dir / "app.js").write_text(
                "console.log('logagent-v2');",
                encoding="utf-8",
            )

            root = resolve_webui_asset(webui_dir, "")
            asset = resolve_webui_asset(webui_dir, "assets/app.js")
            fallback = resolve_webui_asset(webui_dir, "workspaces/demo")

            self.assertEqual(root, (webui_dir / "index.html").resolve())
            self.assertEqual(asset, (assets_dir / "app.js").resolve())
            self.assertEqual(fallback, (webui_dir / "index.html").resolve())
            with self.assertRaises(WebuiStaticNotFound):
                resolve_webui_asset(webui_dir, "assets/missing.js")
            with self.assertRaises(WebuiStaticNotFound):
                resolve_webui_asset(webui_dir, "api/v2/not-a-route")
            with self.assertRaises(WebuiStaticNotFound):
                resolve_webui_asset(webui_dir, "../secret")

    def test_fallback_run_alias_normalizes_summary_or_question(self) -> None:
        self.assertEqual(
            normalize_run_alias("Compaction timeout analysis."),
            "Compaction timeout analysis",
        )
        self.assertIsNone(normalize_run_alias("task_1781102775938_1"))
        self.assertEqual(
            fallback_run_alias(
                {"summary": "LogAgent task"},
                "why did the write path timeout?",
            ),
            "why did the write path timeout?",
        )
        self.assertEqual(
            fallback_run_alias({"summary": "x"}, "run_123"),
            "Analysis result",
        )

    def test_workspace_update_and_soft_delete(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            store = Store(Path(tmp) / "logagent.sqlite")
            store.initialize()
            workspace = store.create_workspace("old question", "diagnose", "zh-CN")
            run = store.create_run(workspace["id"])

            updated = store.update_workspace(
                workspace["id"],
                question="new question",
                mode="fix",
                language="en-US",
                skill_ids=["skill-a"],
            )

            self.assertEqual(updated["question"], "new question")
            self.assertEqual(updated["mode"], "fix")
            self.assertEqual(updated["language"], "en-US")
            self.assertEqual(updated["skillIds"], ["skill-a"])
            events = store.list_timeline(run["id"])
            self.assertTrue(any(event["kind"] == "workspace.updated" for event in events))

            deleted = store.delete_workspace(workspace["id"])

            self.assertEqual(deleted["status"], "deleted")
            self.assertEqual(store.list_workspaces(), [])
            self.assertEqual(len(store.list_workspaces(include_deleted=True)), 1)
            with self.assertRaises(ValueError):
                store.create_run(workspace["id"])

    def test_session_alias_routes_map_to_workspace_runs(self) -> None:
        from fastapi.testclient import TestClient
        from logagent_v2.api import create_app

        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(data_dir=Path(tmp), api_key="test", inline_worker=False)
            settings.ensure_dirs()
            headers = {"Authorization": "Bearer test"}

            with TestClient(create_app(settings)) as client:
                created = client.post(
                    "/api/v2/sessions",
                    headers=headers,
                    json={
                        "title": "Timeout analysis",
                        "question": "why timeout?",
                        "sourceUrl": "webui-smoke",
                        "instanceId": "inst-a",
                        "nodeId": "node-a",
                        "analysisLanguage": "en-US",
                        "systemContextIds": ["ctx-a"],
                        "skillIds": ["skill-a"],
                    },
                )
                self.assertEqual(created.status_code, 201)
                session = created.json()
                session_id = session["sessionId"]
                self.assertTrue(session_id.startswith("ws_"))
                self.assertEqual(session["workspaceId"], session_id)
                self.assertEqual(session["title"], "Timeout analysis")
                self.assertEqual(session["question"], "why timeout?")
                self.assertEqual(session["sourceUrl"], "webui-smoke")
                self.assertEqual(session["instanceId"], "inst-a")
                self.assertEqual(session["nodeId"], "node-a")
                self.assertEqual(session["analysisLanguage"], "en-US")
                self.assertEqual(session["systemContextIds"], ["ctx-a"])
                self.assertEqual(session["skillIds"], ["skill-a"])
                self.assertEqual(session["status"], "draft")

                patched = client.patch(
                    f"/api/v2/sessions/{session_id}",
                    headers=headers,
                    json={
                        "title": "Updated title",
                        "question": "updated question",
                        "sourceUrl": None,
                        "instanceId": "inst-b",
                        "nodeId": None,
                        "analysisLanguage": "zh-CN",
                        "systemContextIds": ["ctx-b"],
                        "status": "ready",
                    },
                )
                self.assertEqual(patched.status_code, 200)
                patched_body = patched.json()
                self.assertEqual(patched_body["title"], "Updated title")
                self.assertEqual(patched_body["question"], "updated question")
                self.assertIsNone(patched_body["sourceUrl"])
                self.assertEqual(patched_body["instanceId"], "inst-b")
                self.assertIsNone(patched_body["nodeId"])
                self.assertEqual(patched_body["analysisLanguage"], "zh-CN")
                self.assertEqual(patched_body["systemContextIds"], ["ctx-b"])
                self.assertEqual(patched_body["status"], "ready")

                fetched = client.get(
                    f"/api/v2/sessions/{session_id}",
                    headers=headers,
                )
                self.assertEqual(fetched.status_code, 200)
                self.assertEqual(fetched.json()["title"], "Updated title")
                self.assertEqual(fetched.json()["instanceId"], "inst-b")
                self.assertIsNone(fetched.json()["sourceUrl"])

                uploaded = client.post(
                    f"/api/v2/sessions/{session_id}/uploads",
                    headers=headers,
                    files={"file": ("db.log", b"query timeout\n", "text/plain")},
                )
                self.assertEqual(uploaded.status_code, 200)
                upload_id = uploaded.json()["upload"]["id"]

                detached = client.delete(
                    f"/api/v2/sessions/{session_id}/uploads/{upload_id}",
                    headers=headers,
                )
                self.assertEqual(detached.status_code, 200)
                self.assertEqual(detached.json()["uploadIds"], [])
                self.assertEqual(detached.json()["status"], "draft")

                empty_uploads = client.get(
                    f"/api/v2/sessions/{session_id}/uploads",
                    headers=headers,
                )
                self.assertEqual(empty_uploads.status_code, 200)
                self.assertEqual(empty_uploads.json()["uploads"], [])

                attached = client.post(
                    f"/api/v2/sessions/{session_id}/uploads",
                    headers=headers,
                    json={"uploadIds": [upload_id]},
                )
                self.assertEqual(attached.status_code, 200)
                self.assertEqual(attached.json()["uploadIds"], [upload_id])
                self.assertEqual(attached.json()["status"], "ready")

                uploads = client.get(
                    f"/api/v2/sessions/{session_id}/uploads",
                    headers=headers,
                )
                self.assertEqual(uploads.status_code, 200)
                self.assertEqual([item["id"] for item in uploads.json()["uploads"]], [upload_id])
                after_upload = client.get(
                    f"/api/v2/sessions/{session_id}",
                    headers=headers,
                )
                self.assertEqual(after_upload.status_code, 200)
                self.assertEqual(after_upload.json()["status"], "ready")

                task = client.post(
                    f"/api/v2/sessions/{session_id}/tasks",
                    headers=headers,
                )
                self.assertEqual(task.status_code, 202)
                task_body = task.json()
                self.assertTrue(task_body["taskId"].startswith("run_"))
                self.assertEqual(task_body["runId"], task_body["taskId"])
                self.assertEqual(task_body["sessionId"], session_id)
                self.assertEqual(task_body["task"]["taskId"], task_body["taskId"])
                self.assertEqual(task_body["taskKind"], "log_analysis")
                self.assertEqual(task_body["status"], "QUEUED")
                after_task = client.get(
                    f"/api/v2/sessions/{session_id}",
                    headers=headers,
                )
                self.assertEqual(after_task.status_code, 200)
                self.assertEqual(after_task.json()["status"], "ready")
                self.assertEqual(after_task.json()["activeTaskId"], task_body["taskId"])
                blocked_detach = client.delete(
                    f"/api/v2/sessions/{session_id}/uploads/{upload_id}",
                    headers=headers,
                )
                self.assertEqual(blocked_detach.status_code, 409)

                tasks = client.get(
                    f"/api/v2/sessions/{session_id}/tasks",
                    headers=headers,
                )
                self.assertEqual(tasks.status_code, 200)
                self.assertEqual(
                    [item["taskId"] for item in tasks.json()["tasks"]],
                    [task_body["taskId"]],
                )
                self.assertEqual(tasks.json()["tasks"][0]["status"], "QUEUED")
                self.assertEqual(tasks.json()["runs"][0]["id"], task_body["taskId"])

                timeline = client.get(
                    f"/api/v2/sessions/{session_id}/timeline",
                    headers=headers,
                )
                self.assertEqual(timeline.status_code, 200)
                event_kinds = [event["kind"] for event in timeline.json()["events"]]
                self.assertIn("workspace.created", event_kinds)
                self.assertIn("workspace.updated", event_kinds)
                self.assertIn("upload.created", event_kinds)
                self.assertIn("upload.detached", event_kinds)
                self.assertIn("upload.attached", event_kinds)
                self.assertIn("run.queued", event_kinds)

                blocked_delete = client.delete(
                    f"/api/v2/sessions/{session_id}",
                    headers=headers,
                )
                self.assertEqual(blocked_delete.status_code, 409)

                empty = client.post(
                    "/api/v2/sessions",
                    headers=headers,
                    json={"title": "empty draft"},
                )
                self.assertEqual(empty.status_code, 201)
                empty_id = empty.json()["sessionId"]
                deleted = client.delete(
                    f"/api/v2/sessions/{empty_id}",
                    headers=headers,
                )
                self.assertEqual(deleted.status_code, 200)
                self.assertTrue(deleted.json()["deleted"])

                listed = client.get("/api/v2/sessions", headers=headers)
                self.assertEqual(listed.status_code, 200)
                self.assertEqual(
                    [item["sessionId"] for item in listed.json()["sessions"]],
                    [session_id],
                )

    def test_workspace_run_job_and_stub_agent(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(data_dir=Path(tmp), api_key="test")
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()

            workspace = store.create_workspace("why did the query timeout?", "diagnose", "en-US")
            artifact = write_artifact_bytes(
                settings,
                store,
                workspace["id"],
                "db.log",
                b"2026-06-17 query timeout on shard 1\nnormal line\n",
                "text/plain",
            )
            store.create_upload(workspace["id"], "db.log", artifact["id"])
            run = store.create_run(workspace["id"])
            jobs = store.acquire_jobs("test-worker", limit=1)

            self.assertEqual(len(jobs), 1)
            self.assertEqual(jobs[0]["kind"], "run_analysis")
            self.assertEqual(store.list_runs(workspace["id"])[0]["id"], run["id"])
            self.assertEqual(store.list_runs()[0]["id"], run["id"])

            AgentRuntime(settings, store).run_analysis(workspace["id"], run["id"])
            store.complete_job(jobs[0]["id"])

            finished = store.get_run(run["id"])
            self.assertEqual(finished["status"], "succeeded")
            self.assertEqual(finished["phase"], "finish")
            self.assertEqual(finished["finalAnswer"]["confidence"], "low")
            self.assertEqual(finished["finalAnswer"]["evidenceRefs"], ["grep_results.json#matches/0"])
            self.assertTrue(finished["alias"])
            self.assertNotIn("task_", finished["alias"].lower())
            self.assertEqual(store.list_runs(workspace["id"])[0]["alias"], finished["alias"])

            evidence = store.list_evidence(run["id"])
            self.assertTrue(any(item["kind"] == "manifest" for item in evidence))
            self.assertTrue(any(item["kind"] == "log_search" for item in evidence))
            self.assertTrue(any(item["kind"] == "result" for item in evidence))
            self.assertTrue(any(item["kind"] == "result_markdown" for item in evidence))
            package_evidence = next(
                item for item in evidence if item["kind"] == "analysis_package"
            )
            self.assertFalse(package_evidence["final_allowed"])
            package_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 2,
                    "method": "resources/read",
                    "params": {"uri": f"logagent://task/{run['id']}/analysis_package"},
                },
            )
            self.assertEqual(
                package_response["result"]["contents"][0]["uri"],
                f"logagent://task/{run['id']}/analysis_package",
            )
            package = json.loads(package_response["result"]["contents"][0]["text"])
            self.assertEqual(package["workspace"]["question"], "why did the query timeout?")
            self.assertEqual(package["manifest"]["fileCount"], 1)
            self.assertEqual(
                package["allowedEvidenceRefs"],
                ["session_text_input.json#question", "grep_results.json#matches/0"],
            )
            self.assertIn("analysis_package", {item["name"] for item in package["resources"]})
            self.assertIn("analysis_state", {item["name"] for item in package["resources"]})
            self.assertFalse(package["systemContext"]["resources"])
            resource_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {"jsonrpc": "2.0", "id": 3, "method": "resources/list"},
            )
            resource_names = {
                item["name"] for item in resource_response["result"]["resources"]
            }
            self.assertIn("analysis_state", resource_names)
            self.assertIn("agent_request", resource_names)
            self.assertIn("agent_response", resource_names)
            self.assertIn("claude_mcp_config", resource_names)
            self.assertIn("claude_session", resource_names)
            self.assertIn("result", resource_names)
            self.assertIn("result_markdown", resource_names)
            state_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 4,
                    "method": "resources/read",
                    "params": {"uri": f"logagent-v2://run/{run['id']}/analysis_state"},
                },
            )
            state = json.loads(state_response["result"]["contents"][0]["text"])
            self.assertEqual(state["status"], "succeeded")
            self.assertEqual(state["rounds"][0]["status"], "completed")
            request_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 5,
                    "method": "resources/read",
                    "params": {"uri": f"logagent-v2://run/{run['id']}/agent_request"},
                },
            )
            request_doc = json.loads(request_response["result"]["contents"][0]["text"])
            self.assertEqual(request_doc["provider"], "stub")
            self.assertEqual(request_doc["transport"]["type"], "local_stub")
            self.assertEqual(
                request_doc["allowedEvidenceRefs"],
                ["session_text_input.json#question", "grep_results.json#matches/0"],
            )
            response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 6,
                    "method": "resources/read",
                    "params": {"uri": f"logagent-v2://run/{run['id']}/agent_response"},
                },
            )
            response_doc = json.loads(response["result"]["contents"][0]["text"])
            self.assertEqual(response_doc["provider"], "stub")
            self.assertEqual(response_doc["status"], "completed")
            self.assertEqual(response_doc["validation"]["status"], "passed")
            self.assertEqual(response_doc["validatedFinalAnswer"]["confidence"], "low")
            claude_config_doc = {
                "schemaVersion": 1,
                "mcpServers": {"logagent": {"command": "logagent-v2"}},
            }
            claude_session_doc = {
                "schemaVersion": 1,
                "runtimeStatus": "succeeded",
                "claudeSessionId": "sess-test",
                "mcpConfigPath": "claude_mcp_config.json",
            }
            claude_config_artifact = write_artifact_bytes(
                settings,
                store,
                workspace["id"],
                "claude_mcp_config.json",
                json.dumps(claude_config_doc).encode("utf-8"),
                "application/json",
                schema_name="logagent.v2.claude_mcp_config.v1",
            )
            store.create_evidence(
                workspace["id"],
                run["id"],
                "claude_mcp_config",
                False,
                "Claude MCP config artifact captured.",
                {"path": "claude_mcp_config.json"},
                artifact_id=claude_config_artifact["id"],
            )
            claude_session_artifact = write_artifact_bytes(
                settings,
                store,
                workspace["id"],
                "claude_session.json",
                json.dumps(claude_session_doc).encode("utf-8"),
                "application/json",
                schema_name="logagent.v2.claude_session.v1",
            )
            store.create_evidence(
                workspace["id"],
                run["id"],
                "claude_session",
                False,
                "Claude session artifact captured.",
                {"path": "claude_session.json"},
                artifact_id=claude_session_artifact["id"],
            )
            claude_resource = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 61,
                    "method": "resources/read",
                    "params": {"uri": f"logagent-v2://run/{run['id']}/claude_session"},
                },
            )
            self.assertEqual(
                json.loads(claude_resource["result"]["contents"][0]["text"])[
                    "claudeSessionId"
                ],
                "sess-test",
            )
            artifacts = get_run_artifacts(settings, store, run["id"])
            self.assertEqual(artifacts["taskId"], run["id"])
            self.assertEqual(artifacts["manifestPath"], "manifest.json")
            self.assertEqual(artifacts["manifest"]["fileCount"], 1)
            self.assertEqual(artifacts["grepResultsPath"], "grep_results.json")
            self.assertEqual(artifacts["grepResults"]["totalMatches"], 1)
            self.assertEqual(artifacts["textInputPath"], "session_text_input.json")
            self.assertEqual(
                artifacts["textInput"]["question"],
                "why did the query timeout?",
            )
            self.assertEqual(artifacts["metadataContextPath"], "metadata_context.json")
            self.assertEqual(artifacts["systemContextPath"], "system_context.json")
            self.assertEqual(artifacts["analysisPackagePath"], "analysis_package.json")
            self.assertEqual(artifacts["agentResponsePath"], "agent_response.json")
            self.assertEqual(artifacts["claudeMcpConfigPath"], "claude_mcp_config.json")
            self.assertEqual(
                artifacts["claudeMcpConfig"]["mcpServers"]["logagent"]["command"],
                "logagent-v2",
            )
            self.assertEqual(artifacts["claudeSessionPath"], "claude_session.json")
            self.assertEqual(artifacts["claudeSession"]["claudeSessionId"], "sess-test")
            self.assertEqual(artifacts["analysisStatePath"], "analysis_state.json")
            self.assertEqual(artifacts["mcpCallsPath"], "mcp_calls.jsonl")
            self.assertIn(
                "resources/read",
                {call["name"] for call in artifacts["mcpCalls"]},
            )
            self.assertEqual(artifacts["toolResults"], [])
            self.assertGreaterEqual(artifacts["artifactIndex"]["artifactCount"], 7)
            result_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 7,
                    "method": "resources/read",
                    "params": {"uri": f"logagent-v2://run/{run['id']}/result"},
                },
            )
            result_doc = json.loads(result_response["result"]["contents"][0]["text"])
            self.assertEqual(result_doc["finalAnswer"]["confidence"], "low")
            markdown_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 8,
                    "method": "resources/read",
                    "params": {"uri": f"logagent-v2://run/{run['id']}/result_markdown"},
                },
            )
            markdown_content = markdown_response["result"]["contents"][0]
            self.assertEqual(markdown_content["mimeType"], "text/markdown")
            self.assertIn("# LogAgent Result", markdown_content["text"])
            self.assertIn("grep_results.json#matches/0", markdown_content["text"])
            run_result = get_run_result(settings, store, run["id"])
            self.assertEqual(run_result["finalAnswer"]["confidence"], "low")
            self.assertEqual(run_result["artifacts"]["json"]["content_type"], "application/json")
            self.assertEqual(run_result["artifacts"]["markdown"]["content_type"], "text/markdown")
            run_artifacts = store.list_run_artifacts(run["id"])
            self.assertEqual(run_artifacts["run"]["id"], run["id"])
            self.assertEqual(len(run_artifacts["uploads"]), 1)
            self.assertEqual(run_artifacts["uploads"][0]["filename"], "db.log")
            artifact_kinds = {
                item["evidence_kind"] for item in run_artifacts["evidenceArtifacts"]
            }
            self.assertIn("manifest", artifact_kinds)
            self.assertIn("log_search", artifact_kinds)
            self.assertIn("analysis_package", artifact_kinds)
            self.assertIn("result", artifact_kinds)
            self.assertIn("user_question", artifact_kinds)
            self.assertTrue(
                all(item["artifact_id"] for item in run_artifacts["evidenceArtifacts"])
            )
            question_artifact = next(
                item
                for item in run_artifacts["evidenceArtifacts"]
                if item["evidence_kind"] == "user_question"
            )
            self.assertEqual(question_artifact["evidence_payload"]["path"], "session_text_input.json")
            question_path = resolve_artifact_path(settings, question_artifact["relative_path"])
            self.assertEqual(
                json.loads(question_path.read_text(encoding="utf-8"))["question"],
                "why did the query timeout?",
            )
            validated = normalize_and_validate_final_answer(
                settings,
                store,
                run["id"],
                {
                    "summary": "Question-only evidence is citeable.",
                    "symptoms": [],
                    "likelyRootCauses": [
                        {
                            "cause": "The user described the target failure mode.",
                            "evidenceRefs": ["session_text_input.json#question"],
                        }
                    ],
                    "nextChecks": [],
                    "fixSuggestions": [],
                    "missingInformation": [],
                    "confidence": "low",
                    "evidenceRefs": ["session_text_input.json#question"],
                },
            )
            self.assertEqual(validated["evidenceRefs"], ["session_text_input.json#question"])
            analysis = get_run_analysis(settings, store, run["id"])
            self.assertEqual(analysis["run"]["id"], run["id"])
            self.assertEqual(analysis["workspace"]["id"], workspace["id"])
            self.assertTrue(analysis["timeline"])
            self.assertTrue(analysis["evidence"])
            self.assertEqual(analysis["resources"]["analysis_state"]["status"], "succeeded")
            self.assertEqual(analysis["resources"]["agent_response"]["status"], "completed")
            self.assertEqual(
                analysis["resources"]["analysis_package"]["allowedEvidenceRefs"],
                ["session_text_input.json#question", "grep_results.json#matches/0"],
            )
            self.assertEqual(analysis["result"]["finalAnswer"]["confidence"], "low")
            events = store.list_timeline(run["id"])
            self.assertTrue(any(event["kind"] == "evidence.created" for event in events))
            succeeded_events = [event for event in events if event["kind"] == "run.succeeded"]
            self.assertTrue(succeeded_events)
            self.assertEqual(succeeded_events[-1]["payload"]["alias"], finished["alias"])

    def test_mcp_tool_lists_cover_v1_builtin_names(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(data_dir=Path(tmp), api_key="test")
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace("tool coverage", "diagnose", "en-US")
            run = store.create_run(workspace["id"])

            task_list = task_mcp_response(
                settings,
                store,
                run["id"],
                {"jsonrpc": "2.0", "id": 41, "method": "tools/list"},
            )
            task_tool_names = {item["name"] for item in task_list["result"]["tools"]}
            self.assertTrue(
                {
                    "logagent.search_logs",
                    "logagent.get_log_slice",
                    "logagent.run_domain_tool",
                    "logagent.request_user_input",
                    "logagent.request_approval",
                    "logagent.get_metadata_topology",
                    "logagent.query_metadata",
                    "logagent.list_metadata_instances",
                    "logagent.get_metadata_snapshot",
                    "logagent.get_metadata_field_types",
                    "logagent.get_metadata_tag_fields",
                    "logagent.recall_cases",
                    "logagent.list_skills",
                    "logagent.get_skill",
                    "logagent.get_skill_reference",
                    "logagent.preview_system_context",
                    "logagent.list_fetch_endpoints",
                    "logagent.fetch",
                }.issubset(task_tool_names)
            )

            readonly_list = readonly_mcp_response(
                settings,
                store,
                {"jsonrpc": "2.0", "id": 42, "method": "tools/list"},
            )
            readonly_tool_names = {
                item["name"] for item in readonly_list["result"]["tools"]
            }
            self.assertTrue(
                {
                    "logagent.list_tools",
                    "logagent.list_domain_adapters",
                    "logagent.list_metadata_instances",
                    "logagent.get_metadata_snapshot",
                    "logagent.get_metadata_field_types",
                    "logagent.get_metadata_tag_fields",
                    "logagent.search_cases",
                    "logagent.get_case",
                    "logagent.list_skills",
                    "logagent.get_skill",
                    "logagent.get_skill_reference",
                    "logagent.preview_system_context",
                }.issubset(readonly_tool_names)
            )

            catalog_tool_ids = {item["toolId"] for item in tool_descriptors(settings)}
            self.assertTrue(
                {
                    "logagent.preprocess_log_package",
                    "pprof_analyzer",
                    "logagent.list_metadata_instances",
                    "logagent.get_metadata_snapshot",
                    "logagent.get_metadata_field_types",
                    "logagent.get_metadata_tag_fields",
                    "logagent.fetch",
                    "logagent.huawei_cloud_package_sync",
                }.issubset(catalog_tool_ids)
            )

    def test_run_analysis_includes_pending_actions(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(data_dir=Path(tmp), api_key="test")
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace("need input", "diagnose", "en-US")
            run = store.create_run(workspace["id"])
            action = store.create_action(
                run["id"],
                "user_input",
                {"question": "Which node is affected?", "required": True},
            )
            store.update_run_status(run["id"], "waiting_for_user", "waiting_for_user")

            analysis = get_run_analysis(settings, store, run["id"])

            self.assertEqual(analysis["pendingActions"][0]["id"], action["id"])
            self.assertEqual(
                analysis["pendingActions"][0]["payload"]["question"],
                "Which node is affected?",
            )

    def test_user_message_answers_pending_user_input_actions(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            store = Store(Path(tmp) / "logagent.sqlite")
            store.initialize()
            workspace = store.create_workspace("need input", "diagnose", "en-US")
            run = store.create_run(workspace["id"])
            action = store.create_action(
                run["id"],
                "user_input",
                {"question": "Which node is affected?", "required": True},
            )

            answered = store.answer_user_input_actions(
                run["id"], "node-1 is affected", "continue"
            )

            self.assertEqual([item["id"] for item in answered], [action["id"]])
            updated = store.get_action(action["id"])
            self.assertEqual(updated["status"], "answered")
            self.assertEqual(updated["result"]["message"], "node-1 is affected")
            self.assertEqual(updated["result"]["resumeMode"], "continue")
            analysis_actions = store.list_actions(run["id"])
            self.assertEqual(
                [item for item in analysis_actions if item["status"] == "pending"],
                [],
            )
            events = store.list_timeline(run["id"])
            self.assertTrue(
                any(event["kind"] == "action.user_input.answered" for event in events)
            )

    def test_agent_resume_context_includes_user_messages_and_actions(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(data_dir=Path(tmp), api_key="test")
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace("finish with current evidence", "diagnose", "en-US")
            run = store.create_run(workspace["id"])
            action = store.create_action(
                run["id"],
                "user_input",
                {"question": "Upload logs or finish?", "required": False},
            )
            store.update_run_status(run["id"], "waiting_for_user", "waiting_for_user")
            store.append_event(
                workspace["id"],
                run["id"],
                "user.message",
                {"message": "No more logs, finish now.", "resumeMode": "finalize"},
            )
            store.answer_user_input_actions(
                run["id"], "No more logs, finish now.", "finalize"
            )

            final_answer = AgentRuntime(settings, store).run_analysis(
                workspace["id"], run["id"]
            )

            self.assertEqual(final_answer["missingInformation"], [])
            self.assertEqual(final_answer["userMessage"], "No more logs, finish now.")
            request_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 41,
                    "method": "resources/read",
                    "params": {"uri": f"logagent-v2://run/{run['id']}/agent_request"},
                },
            )
            request_doc = json.loads(request_response["result"]["contents"][0]["text"])
            prompt = json.loads(request_doc["payload"]["prompt"])
            interaction = prompt["interactionContext"]
            self.assertEqual(
                interaction["userMessages"][-1]["message"], "No more logs, finish now."
            )
            self.assertEqual(interaction["resumeDirective"], "finalize_with_current_evidence")
            self.assertEqual(interaction["actionResults"][-1]["id"], action["id"])
            self.assertEqual(interaction["actionResults"][-1]["status"], "answered")
            self.assertEqual(interaction["pendingActions"], [])

    def test_run_message_api_validates_waiting_question_and_idempotency(self) -> None:
        from fastapi.testclient import TestClient
        from logagent_v2.api import create_app

        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(
                data_dir=Path(tmp),
                api_key="test",
                inline_worker=False,
            )
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace("need input", "diagnose", "en-US")
            run = store.create_run(workspace["id"])
            action = store.create_action(
                run["id"],
                "user_input",
                {
                    "questionId": "q-version",
                    "question": "Which version?",
                    "required": True,
                },
            )
            queued_workspace = store.create_workspace("not waiting", "diagnose", "en-US")
            queued_run = store.create_run(queued_workspace["id"])
            headers = {"Authorization": "Bearer test"}

            with TestClient(create_app(settings)) as client:
                not_waiting = client.post(
                    f"/api/v2/runs/{queued_run['id']}/messages",
                    headers=headers,
                    json={"message": "hello"},
                )
                self.assertEqual(not_waiting.status_code, 409)

                store.update_run_status(run["id"], "waiting_for_user", "waiting_for_user")
                unknown = client.post(
                    f"/api/v2/runs/{run['id']}/messages",
                    headers=headers,
                    json={"questionId": "q-missing", "message": "version 1.2.3"},
                )
                self.assertEqual(unknown.status_code, 400)

                first = client.post(
                    f"/api/v2/runs/{run['id']}/messages",
                    headers=headers,
                    json={
                        "questionId": "q-version",
                        "message": "version 1.2.3",
                        "resumeMode": "finalize",
                        "idempotencyKey": "msg-version-1",
                    },
                )
                self.assertEqual(first.status_code, 200)
                first_body = first.json()
                self.assertFalse(first_body["duplicate"])
                self.assertEqual(first_body["answeredActions"][0]["id"], action["id"])
                self.assertEqual(first_body["job"]["runId"], run["id"])
                self.assertEqual(store.get_action(action["id"])["status"], "answered")
                self.assertEqual(store.get_run(run["id"])["status"], "queued")

                duplicate = client.post(
                    f"/api/v2/runs/{run['id']}/messages",
                    headers=headers,
                    json={
                        "questionId": "q-version",
                        "message": "version 1.2.3",
                        "resumeMode": "finalize",
                        "idempotencyKey": "msg-version-1",
                    },
                )
                self.assertEqual(duplicate.status_code, 200)
                duplicate_body = duplicate.json()
                self.assertTrue(duplicate_body["duplicate"])
                self.assertEqual(
                    duplicate_body["event"]["id"],
                    first_body["event"]["id"],
                )
                self.assertEqual(duplicate_body["answeredActions"], [])
                self.assertIsNone(duplicate_body["job"])
                message_events = [
                    event for event in store.list_timeline(run["id"])
                    if event["kind"] == "user.message"
                ]
                self.assertEqual(len(message_events), 1)
                self.assertEqual(message_events[0]["payload"]["questionId"], "q-version")

    def test_action_decision_api_validates_waiting_status_and_idempotency(self) -> None:
        from fastapi.testclient import TestClient
        from logagent_v2.api import create_app

        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(
                data_dir=Path(tmp),
                api_key="test",
                inline_worker=False,
            )
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace("need approval", "diagnose", "en-US")
            run = store.create_run(workspace["id"])
            action = store.create_action(
                run["id"],
                "approval",
                {
                    "actionType": "manual_approval",
                    "reason": "Need operator confirmation",
                    "input": {},
                },
            )
            headers = {"Authorization": "Bearer test"}

            with TestClient(create_app(settings)) as client:
                not_waiting = client.post(
                    f"/api/v2/actions/{action['id']}/decisions",
                    headers=headers,
                    json={"decision": "approved", "reason": "ok"},
                )
                self.assertEqual(not_waiting.status_code, 409)

                store.update_run_status(
                    run["id"], "waiting_for_approval", "waiting_for_approval"
                )
                first = client.post(
                    f"/api/v2/actions/{action['id']}/decisions",
                    headers=headers,
                    json={
                        "decision": "approved",
                        "reason": "ok",
                        "idempotencyKey": "approval-1",
                    },
                )
                self.assertEqual(first.status_code, 200)
                first_body = first.json()
                self.assertFalse(first_body["duplicate"])
                self.assertEqual(first_body["action"]["status"], "approved")
                self.assertEqual(
                    first_body["action"]["result"]["idempotencyKey"],
                    "approval-1",
                )
                self.assertEqual(first_body["job"]["runId"], run["id"])
                self.assertEqual(store.get_run(run["id"])["status"], "queued")

                duplicate = client.post(
                    f"/api/v2/actions/{action['id']}/decisions",
                    headers=headers,
                    json={
                        "decision": "approved",
                        "reason": "ok",
                        "idempotencyKey": "approval-1",
                    },
                )
                self.assertEqual(duplicate.status_code, 200)
                duplicate_body = duplicate.json()
                self.assertTrue(duplicate_body["duplicate"])
                self.assertEqual(
                    duplicate_body["event"]["id"],
                    next(
                        event["id"]
                        for event in store.list_timeline(run["id"])
                        if event["kind"] == "action.approved"
                    ),
                )
                self.assertIsNone(duplicate_body["job"])
                decision_events = [
                    event for event in store.list_timeline(run["id"])
                    if event["kind"] == "action.approved"
                ]
                self.assertEqual(len(decision_events), 1)
                self.assertEqual(
                    decision_events[0]["payload"]["idempotencyKey"],
                    "approval-1",
                )

    def test_batch_and_chunked_upload_storage_is_persisted(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(data_dir=Path(tmp), api_key="test")
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace("upload logs", "diagnose", "en-US")

            for filename, data in (
                ("a.log", b"alpha error\n"),
                ("b.log", b"beta warning\n"),
            ):
                artifact = write_artifact_bytes(
                    settings=settings,
                    store=store,
                    workspace_id=workspace["id"],
                    filename=filename,
                    data=data,
                    content_type="text/plain",
                    preview={"filename": filename, "sizeBytes": len(data)},
                )
                store.create_upload(workspace["id"], filename, artifact["id"])

            session_id = "ups_test"
            temp_relative_path = (
                f"tmp/upload_sessions/{session_id}/{safe_filename('chunked.log')}"
            )
            session = store.create_upload_session(
                session_id=session_id,
                workspace_id=workspace["id"],
                filename="chunked.log",
                content_type="text/plain",
                expected_size_bytes=12,
                temp_relative_path=temp_relative_path,
            )
            self.assertEqual(session["received_bytes"], 0)
            temp_path = resolve_artifact_path(settings, temp_relative_path)
            temp_path.parent.mkdir(parents=True, exist_ok=True)
            temp_path.write_bytes(b"chunk ")
            session = store.update_upload_session_progress(session_id, 6)
            self.assertEqual(session["received_bytes"], 6)
            with temp_path.open("ab") as target:
                target.write(b"upload")
            session = store.update_upload_session_progress(session_id, 12)
            self.assertEqual(session["received_bytes"], 12)

            artifact = write_artifact_file(
                settings=settings,
                store=store,
                workspace_id=workspace["id"],
                filename=session["filename"],
                source_path=temp_path,
                content_type=session["content_type"],
                preview={"filename": session["filename"], "sizeBytes": 12},
            )
            upload = store.create_upload(workspace["id"], session["filename"], artifact["id"])
            completed = store.complete_upload_session(session_id, upload["id"], artifact["id"])
            temp_path.unlink(missing_ok=True)

            self.assertEqual(completed["status"], "completed")
            self.assertEqual(completed["upload_id"], upload["id"])
            self.assertEqual(artifact["size_bytes"], 12)
            artifact_path = resolve_artifact_path(settings, artifact["relative_path"])
            self.assertEqual(artifact_path.read_bytes(), b"chunk upload")
            uploads = store.list_uploads(workspace["id"])
            self.assertEqual(len(uploads), 3)
            sessions = store.list_upload_sessions(workspace["id"])
            self.assertEqual([item["id"] for item in sessions], [session_id])
            self.assertEqual(store.list_upload_sessions()[0]["id"], session_id)

    def test_agent_runtime_uses_openai_compatible_provider(self) -> None:
        captured: dict[str, object] = {}

        class AgentProviderHandler(BaseHTTPRequestHandler):
            def do_POST(self) -> None:
                length = int(self.headers.get("Content-Length", "0"))
                captured["authorization"] = self.headers.get("Authorization")
                payload = json.loads(self.rfile.read(length).decode("utf-8"))
                captured["payload"] = payload
                answer = {
                    "summary": "model summary",
                    "symptoms": ["timeout line"],
                    "likelyRootCauses": [
                        {
                            "cause": "model cause",
                            "evidenceRefs": ["grep_results.json#matches/0"],
                        }
                    ],
                    "nextChecks": [],
                    "fixSuggestions": [],
                    "missingInformation": [],
                    "confidence": "medium",
                    "evidenceRefs": ["grep_results.json#matches/0"],
                }
                body = json.dumps(
                    {"choices": [{"message": {"content": json.dumps(answer)}}]}
                ).encode("utf-8")
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, format: str, *args: object) -> None:
                return

        server = HTTPServer(("127.0.0.1", 0), AgentProviderHandler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            with tempfile.TemporaryDirectory() as tmp:
                settings = Settings(
                    data_dir=Path(tmp),
                    api_key="test",
                    agent_provider="openai_compatible",
                    agent_base_url=f"http://127.0.0.1:{server.server_port}/v1",
                    agent_model="mock-model",
                    agent_api_key="secret",
                )
                settings.ensure_dirs()
                store = Store(settings.sqlite_path)
                store.initialize()
                workspace = store.create_workspace("why timeout?", "diagnose", "en-US")
                artifact = write_artifact_bytes(
                    settings,
                    store,
                    workspace["id"],
                    "db.log",
                    b"query timeout on shard 1\n",
                    "text/plain",
                )
                store.create_upload(workspace["id"], "db.log", artifact["id"])
                run = store.create_run(workspace["id"])
                final_answer = AgentRuntime(settings, store).run_analysis(
                    workspace["id"], run["id"]
                )
                self.assertEqual(final_answer["summary"], "model summary")
                self.assertEqual(final_answer["confidence"], "medium")
                self.assertEqual(captured["authorization"], "Bearer secret")
                request_payload = captured["payload"]
                assert isinstance(request_payload, dict)
                self.assertEqual(request_payload["model"], "mock-model")
                self.assertIn("grep_results.json#matches/0", request_payload["messages"][1]["content"])
                agent_request = task_mcp_response(
                    settings,
                    store,
                    run["id"],
                    {
                        "jsonrpc": "2.0",
                        "id": 21,
                        "method": "resources/read",
                        "params": {"uri": f"logagent-v2://run/{run['id']}/agent_request"},
                    },
                )
                request_doc = json.loads(agent_request["result"]["contents"][0]["text"])
                self.assertEqual(request_doc["provider"], "openai_compatible")
                self.assertEqual(request_doc["model"], "mock-model")
                self.assertNotIn("Authorization", json.dumps(request_doc))
                agent_response = task_mcp_response(
                    settings,
                    store,
                    run["id"],
                    {
                        "jsonrpc": "2.0",
                        "id": 22,
                        "method": "resources/read",
                        "params": {"uri": f"logagent-v2://run/{run['id']}/agent_response"},
                    },
                )
                response_doc = json.loads(agent_response["result"]["contents"][0]["text"])
                self.assertEqual(response_doc["provider"], "openai_compatible")
                self.assertEqual(response_doc["status"], "completed")
                self.assertEqual(response_doc["validation"]["status"], "passed")
                self.assertEqual(response_doc["response"]["httpStatus"], 200)
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=2)

    def test_agent_runtime_uses_binary_provider(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            prompt_capture = root / "binary_prompt.txt"
            binary = root / "fake-agent-provider"
            answer = {
                "summary": "binary summary",
                "symptoms": ["timeout line"],
                "likelyRootCauses": [
                    {
                        "cause": "binary cause",
                        "evidenceRefs": ["grep_results.json#matches/0"],
                    }
                ],
                "nextChecks": [],
                "fixSuggestions": [],
                "missingInformation": [],
                "confidence": "medium",
                "evidenceRefs": ["grep_results.json#matches/0"],
            }
            binary.write_text(
                "#!/usr/bin/env python3\n"
                "import json, pathlib, sys\n"
                "if len(sys.argv) != 3 or sys.argv[1] != 'run':\n"
                "    raise SystemExit(2)\n"
                f"pathlib.Path({json.dumps(prompt_capture.as_posix())}).write_text(sys.argv[2])\n"
                f"print(json.dumps({json.dumps(answer)}))\n",
                encoding="utf-8",
            )
            binary.chmod(0o755)
            settings = Settings(
                data_dir=root / "data",
                api_key="test",
                agent_provider="binary",
                agent_model="mock-binary",
                agent_binary_path=binary,
            )
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace("why timeout?", "diagnose", "en-US")
            artifact = write_artifact_bytes(
                settings,
                store,
                workspace["id"],
                "db.log",
                b"query timeout on shard 1\n",
                "text/plain",
            )
            store.create_upload(workspace["id"], "db.log", artifact["id"])
            run = store.create_run(workspace["id"])

            final_answer = AgentRuntime(settings, store).run_analysis(
                workspace["id"], run["id"]
            )

            self.assertEqual(final_answer["summary"], "binary summary")
            self.assertIn("grep_results.json#matches/0", prompt_capture.read_text())
            agent_request = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 31,
                    "method": "resources/read",
                    "params": {"uri": f"logagent-v2://run/{run['id']}/agent_request"},
                },
            )
            request_doc = json.loads(agent_request["result"]["contents"][0]["text"])
            self.assertEqual(request_doc["provider"], "binary")
            self.assertEqual(request_doc["transport"]["type"], "local_binary")
            self.assertTrue(request_doc["transport"]["binaryPathConfigured"])
            self.assertNotIn(binary.as_posix(), json.dumps(request_doc))
            agent_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 32,
                    "method": "resources/read",
                    "params": {"uri": f"logagent-v2://run/{run['id']}/agent_response"},
                },
            )
            response_doc = json.loads(agent_response["result"]["contents"][0]["text"])
            self.assertEqual(response_doc["provider"], "binary")
            self.assertEqual(response_doc["status"], "completed")
            self.assertEqual(response_doc["response"]["exitCode"], 0)
            self.assertNotIn(binary.as_posix(), json.dumps(response_doc))
            self.assertEqual(response_doc["validation"]["status"], "passed")

    def test_agent_provider_validation_failure_keeps_audit_artifacts(self) -> None:
        class InvalidRefProviderHandler(BaseHTTPRequestHandler):
            def do_POST(self) -> None:
                length = int(self.headers.get("Content-Length", "0"))
                self.rfile.read(length)
                answer = {
                    "summary": "model summary",
                    "symptoms": [],
                    "likelyRootCauses": [
                        {
                            "cause": "bad ref",
                            "evidenceRefs": ["grep_results.json#matches/999"],
                        }
                    ],
                    "nextChecks": [],
                    "fixSuggestions": [],
                    "missingInformation": [],
                    "confidence": "medium",
                    "evidenceRefs": ["grep_results.json#matches/999"],
                }
                body = json.dumps(
                    {"choices": [{"message": {"content": json.dumps(answer)}}]}
                ).encode("utf-8")
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, format: str, *args: object) -> None:
                return

        server = HTTPServer(("127.0.0.1", 0), InvalidRefProviderHandler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            with tempfile.TemporaryDirectory() as tmp:
                settings = Settings(
                    data_dir=Path(tmp),
                    api_key="test",
                    agent_provider="openai_compatible",
                    agent_base_url=f"http://127.0.0.1:{server.server_port}/v1",
                    agent_model="mock-model",
                )
                settings.ensure_dirs()
                store = Store(settings.sqlite_path)
                store.initialize()
                workspace = store.create_workspace("why timeout?", "diagnose", "en-US")
                artifact = write_artifact_bytes(
                    settings,
                    store,
                    workspace["id"],
                    "db.log",
                    b"query timeout on shard 1\n",
                    "text/plain",
                )
                store.create_upload(workspace["id"], "db.log", artifact["id"])
                run = store.create_run(workspace["id"])
                with self.assertRaises(FinalAnswerValidationError):
                    AgentRuntime(settings, store).run_analysis(workspace["id"], run["id"])

                evidence_kinds = {item["kind"] for item in store.list_evidence(run["id"])}
                self.assertIn("agent_request", evidence_kinds)
                self.assertIn("agent_response", evidence_kinds)
                self.assertIn("analysis_state", evidence_kinds)
                agent_response = task_mcp_response(
                    settings,
                    store,
                    run["id"],
                    {
                        "jsonrpc": "2.0",
                        "id": 31,
                        "method": "resources/read",
                        "params": {"uri": f"logagent-v2://run/{run['id']}/agent_response"},
                    },
                )
                response_doc = json.loads(agent_response["result"]["contents"][0]["text"])
                self.assertEqual(response_doc["status"], "completed")
                self.assertEqual(response_doc["validation"]["status"], "failed")
                self.assertEqual(response_doc["validation"]["type"], "FinalAnswerValidationError")
                self.assertEqual(
                    response_doc["finalAnswer"]["evidenceRefs"],
                    ["grep_results.json#matches/999"],
                )
                state_response = task_mcp_response(
                    settings,
                    store,
                    run["id"],
                    {
                        "jsonrpc": "2.0",
                        "id": 32,
                        "method": "resources/read",
                        "params": {"uri": f"logagent-v2://run/{run['id']}/analysis_state"},
                    },
                )
                state = json.loads(state_response["result"]["contents"][0]["text"])
                self.assertEqual(state["status"], "failed")
                self.assertEqual(state["finalAnswerStatus"], "invalid")
                self.assertEqual(state["rounds"][0]["validation"]["status"], "failed")
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=2)

    def test_agent_provider_can_request_readonly_log_search_before_final_answer(self) -> None:
        captured_prompts: list[dict] = []

        class ToolLoopProviderHandler(BaseHTTPRequestHandler):
            def do_POST(self) -> None:
                length = int(self.headers.get("Content-Length", "0"))
                payload = json.loads(self.rfile.read(length).decode("utf-8"))
                prompt = json.loads(payload["messages"][1]["content"])
                captured_prompts.append(prompt)
                if len(captured_prompts) == 1:
                    answer = {
                        "type": "tool_calls",
                        "toolCalls": [
                            {
                                "name": "logagent.search_logs",
                                "arguments": {"keywords": ["panic"]},
                            }
                        ],
                    }
                else:
                    observation = prompt["toolObservations"][0]
                    ref = observation["result"]["search"]["matches"][0]["ref"]
                    answer = {
                        "summary": "model used follow-up search",
                        "symptoms": ["panic line"],
                        "likelyRootCauses": [
                            {
                                "cause": "panic was found by a follow-up search",
                                "evidenceRefs": [ref],
                            }
                        ],
                        "nextChecks": [],
                        "fixSuggestions": [],
                        "missingInformation": [],
                        "confidence": "high",
                        "evidenceRefs": [ref],
                    }
                body = json.dumps(
                    {"choices": [{"message": {"content": json.dumps(answer)}}]}
                ).encode("utf-8")
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, format: str, *args: object) -> None:
                return

        server = HTTPServer(("127.0.0.1", 0), ToolLoopProviderHandler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            with tempfile.TemporaryDirectory() as tmp:
                settings = Settings(
                    data_dir=Path(tmp),
                    api_key="test",
                    agent_provider="openai_compatible",
                    agent_base_url=f"http://127.0.0.1:{server.server_port}/v1",
                    agent_model="mock-model",
                    agent_max_rounds=3,
                )
                settings.ensure_dirs()
                store = Store(settings.sqlite_path)
                store.initialize()
                workspace = store.create_workspace("find panic root cause", "diagnose", "en-US")
                artifact = write_artifact_bytes(
                    settings,
                    store,
                    workspace["id"],
                    "db.log",
                    b"normal startup\npanic: shard crashed\n",
                    "text/plain",
                )
                store.create_upload(workspace["id"], "db.log", artifact["id"])
                run = store.create_run(workspace["id"])

                final_answer = AgentRuntime(settings, store).run_analysis(
                    workspace["id"], run["id"]
                )

                self.assertEqual(final_answer["summary"], "model used follow-up search")
                self.assertEqual(final_answer["confidence"], "high")
                self.assertTrue(final_answer["evidenceRefs"][0].startswith("log_searches/"))
                self.assertEqual(len(captured_prompts), 2)
                self.assertEqual(captured_prompts[0]["toolObservations"], [])
                available_tool_names = {
                    tool["name"] for tool in captured_prompts[0]["availableTools"]
                }
                self.assertIn("logagent.recall_cases", available_tool_names)
                self.assertIn("logagent.get_metadata_topology", available_tool_names)
                self.assertIn("logagent.query_metadata", available_tool_names)
                self.assertIn("logagent.search_cases", available_tool_names)
                self.assertIn("logagent.list_metadata_instances", available_tool_names)
                self.assertIn("logagent.list_skills", available_tool_names)
                self.assertIn("logagent.list_fetch_endpoints", available_tool_names)
                self.assertNotIn("logagent.fetch", available_tool_names)
                self.assertNotIn("logagent.request_user_input", available_tool_names)
                self.assertNotIn("logagent.request_approval", available_tool_names)
                self.assertEqual(
                    captured_prompts[1]["toolObservations"][0]["name"],
                    "logagent.search_logs",
                )
                follow_up_ref = captured_prompts[1]["toolObservations"][0]["result"][
                    "search"
                ]["matches"][0]["ref"]
                self.assertIn(follow_up_ref, captured_prompts[1]["allowedEvidenceRefs"])
                state_response = task_mcp_response(
                    settings,
                    store,
                    run["id"],
                    {
                        "jsonrpc": "2.0",
                        "id": 41,
                        "method": "resources/read",
                        "params": {"uri": f"logagent-v2://run/{run['id']}/analysis_state"},
                    },
                )
                state = json.loads(state_response["result"]["contents"][0]["text"])
                self.assertEqual(state["status"], "succeeded")
                self.assertEqual(len(state["rounds"]), 2)
                self.assertEqual(state["rounds"][0]["status"], "tool_calls_executed")
                self.assertEqual(state["rounds"][1]["status"], "completed")
                first_response_evidence = [
                    item for item in store.list_evidence(run["id"])
                    if item["kind"] == "agent_response"
                ][0]
                first_response_path = resolve_artifact_path(
                    settings,
                    store.get_artifact(first_response_evidence["artifact_id"])["relative_path"],
                )
                first_response = json.loads(first_response_path.read_text(encoding="utf-8"))
                self.assertEqual(first_response["toolCalls"][0]["name"], "logagent.search_logs")
                self.assertEqual(
                    first_response["validation"]["status"], "tool_calls_executed"
                )
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=2)

    def test_agent_provider_can_use_case_search_tool_observation(self) -> None:
        captured_prompts: list[dict] = []

        class CaseToolProviderHandler(BaseHTTPRequestHandler):
            def do_POST(self) -> None:
                length = int(self.headers.get("Content-Length", "0"))
                payload = json.loads(self.rfile.read(length).decode("utf-8"))
                prompt = json.loads(payload["messages"][1]["content"])
                captured_prompts.append(prompt)
                if len(captured_prompts) == 1:
                    answer = {
                        "type": "tool_calls",
                        "toolCalls": [
                            {
                                "name": "logagent.search_cases",
                                "arguments": {"query": "compaction panic", "limit": 3},
                            }
                        ],
                    }
                else:
                    case_title = prompt["toolObservations"][0]["result"]["cases"][0]["title"]
                    answer = {
                        "summary": f"similar case found: {case_title}",
                        "symptoms": [],
                        "likelyRootCauses": [],
                        "nextChecks": ["Compare the uploaded log against the recalled case."],
                        "fixSuggestions": [],
                        "missingInformation": [],
                        "confidence": "medium",
                        "evidenceRefs": [],
                    }
                body = json.dumps(
                    {"choices": [{"message": {"content": json.dumps(answer)}}]}
                ).encode("utf-8")
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, format: str, *args: object) -> None:
                return

        server = HTTPServer(("127.0.0.1", 0), CaseToolProviderHandler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            with tempfile.TemporaryDirectory() as tmp:
                settings = Settings(
                    data_dir=Path(tmp),
                    api_key="test",
                    agent_provider="openai_compatible",
                    agent_base_url=f"http://127.0.0.1:{server.server_port}/v1",
                    agent_model="mock-model",
                )
                settings.ensure_dirs()
                store = Store(settings.sqlite_path)
                store.initialize()
                create_manual_case(
                    store,
                    {
                        "title": "Compaction panic case",
                        "symptom": "panic during compaction",
                        "rootCause": "corrupt segment",
                        "solution": "rebuild the affected shard",
                    },
                )
                workspace = store.create_workspace("compaction panic", "diagnose", "en-US")
                artifact = write_artifact_bytes(
                    settings,
                    store,
                    workspace["id"],
                    "db.log",
                    b"panic during compaction\n",
                    "text/plain",
                )
                store.create_upload(workspace["id"], "db.log", artifact["id"])
                run = store.create_run(workspace["id"])

                final_answer = AgentRuntime(settings, store).run_analysis(
                    workspace["id"], run["id"]
                )

                self.assertIn("Compaction panic case", final_answer["summary"])
                self.assertEqual(len(captured_prompts), 2)
                observation = captured_prompts[1]["toolObservations"][0]
                self.assertEqual(observation["name"], "logagent.search_cases")
                self.assertEqual(
                    observation["result"]["cases"][0]["title"], "Compaction panic case"
                )
                evidence_kinds = {item["kind"] for item in store.list_evidence(run["id"])}
                self.assertIn("case_context", evidence_kinds)
                state_response = task_mcp_response(
                    settings,
                    store,
                    run["id"],
                    {
                        "jsonrpc": "2.0",
                        "id": 51,
                        "method": "resources/read",
                        "params": {"uri": f"logagent-v2://run/{run['id']}/analysis_state"},
                    },
                )
                state = json.loads(state_response["result"]["contents"][0]["text"])
                self.assertEqual(len(state["rounds"]), 2)
                self.assertEqual(state["rounds"][0]["status"], "tool_calls_executed")
                self.assertEqual(state["rounds"][1]["status"], "completed")
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=2)

    def test_job_lock_prevents_duplicate_acquire(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            store = Store(Path(tmp) / "logagent.sqlite")
            store.initialize()
            workspace = store.create_workspace("question", "diagnose", "zh-CN")
            store.create_run(workspace["id"])

            first = store.acquire_jobs("worker-a", limit=1)
            second = store.acquire_jobs("worker-b", limit=1)

            self.assertEqual(len(first), 1)
            self.assertEqual(second, [])

    def test_interrupted_run_analysis_job_is_requeued_on_startup(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            store = Store(Path(tmp) / "logagent.sqlite")
            store.initialize()
            workspace = store.create_workspace("question", "diagnose", "zh-CN")
            run = store.create_run(workspace["id"])
            first = store.acquire_jobs("worker-a", limit=1)
            self.assertEqual(first[0]["kind"], "run_analysis")
            store.update_run_status(run["id"], "running", "agent_round")

            recovery = store.recover_interrupted_jobs()

            self.assertEqual(recovery["runAnalysisRequeued"], 1)
            recovered_run = store.get_run(run["id"])
            self.assertEqual(recovered_run["status"], "queued")
            self.assertEqual(recovered_run["phase"], "queued")
            events = store.list_timeline(run["id"])
            self.assertTrue(any(event["kind"] == "run.recovered" for event in events))
            reacquired = store.acquire_jobs("worker-b", limit=1)
            self.assertEqual(len(reacquired), 1)
            self.assertEqual(reacquired[0]["id"], first[0]["id"])

    def test_interrupted_remote_command_job_is_requeued_on_startup(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            store = Store(Path(tmp) / "logagent.sqlite")
            store.initialize()
            executor = store.create_remote_executor(
                {
                    "name": "remote",
                    "host": "127.0.0.1",
                    "port": 22,
                    "user": "root",
                    "tags": [],
                    "enabled": True,
                    "notes": None,
                }
            )
            run = store.create_remote_run(
                executor["executorId"],
                "smoke_ls_root",
                "Smoke on remote",
            )
            first = store.acquire_jobs("worker-a", limit=1)
            self.assertEqual(first[0]["kind"], "remote_command_run")
            store.mark_remote_run_running(run["taskId"], "EXECUTE_REMOTE_COMMAND")

            recovery = store.recover_interrupted_jobs()

            self.assertEqual(recovery["remoteRunsRequeued"], 1)
            recovered_run = store.get_remote_run(run["taskId"])
            self.assertEqual(recovered_run["status"], "QUEUED")
            self.assertEqual(recovered_run["phase"], "QUEUED")
            reacquired = store.acquire_jobs("worker-b", limit=1)
            self.assertEqual(len(reacquired), 1)
            self.assertEqual(reacquired[0]["id"], first[0]["id"])

    def test_zip_upload_is_indexed_and_unsafe_paths_are_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(data_dir=Path(tmp), api_key="test")
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace("panic in compaction", "diagnose", "zh-CN")

            zip_path = Path(tmp) / "logs.zip"
            with zipfile.ZipFile(zip_path, "w") as archive:
                archive.writestr("node/tsdb.log", "panic: compaction failed\n")
            artifact = write_artifact_bytes(
                settings,
                store,
                workspace["id"],
                "logs.zip",
                zip_path.read_bytes(),
                "application/zip",
            )
            store.create_upload(workspace["id"], "logs.zip", artifact["id"])
            run = store.create_run(workspace["id"])

            AgentRuntime(settings, store).run_analysis(workspace["id"], run["id"])
            evidence = store.list_evidence(run["id"])
            search = next(item for item in evidence if item["kind"] == "log_search")
            self.assertEqual(search["payload"]["totalMatches"], 1)

            bad_workspace = store.create_workspace("bad", "diagnose", "zh-CN")
            bad_zip_path = Path(tmp) / "bad.zip"
            with zipfile.ZipFile(bad_zip_path, "w") as archive:
                archive.writestr("../evil.log", "error outside\n")
            bad_artifact = write_artifact_bytes(
                settings,
                store,
                bad_workspace["id"],
                "bad.zip",
                bad_zip_path.read_bytes(),
                "application/zip",
            )
            store.create_upload(bad_workspace["id"], "bad.zip", bad_artifact["id"])
            bad_run = store.create_run(bad_workspace["id"])
            with self.assertRaises(ValueError):
                AgentRuntime(settings, store).run_analysis(bad_workspace["id"], bad_run["id"])

    def test_node_log_package_is_classified_and_gzip_rotation_is_read(self) -> None:
        def add_file(archive: tarfile.TarFile, name: str, data: bytes) -> None:
            info = tarfile.TarInfo(name)
            info.size = len(data)
            archive.addfile(info, io.BytesIO(data))

        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(data_dir=Path(tmp), api_key="test")
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace("stream timeout warning", "diagnose", "en-US")

            tar_path = Path(tmp) / "Pkg_Inst_NodeA_20260617120000_logs.tar.gz"
            with tarfile.open(tar_path, "w:gz") as archive:
                add_file(
                    archive,
                    "wrapper/var/chroot/gemini/log/tsdb/query.log",
                    b"tsdb query timeout\n",
                )
                add_file(
                    archive,
                    "wrapper/var/chroot/gemini/log/stream/stream.rotate.1",
                    gzip.compress(b"stream warning from rotated gzip\n"),
                )
                add_file(
                    archive,
                    "wrapper/home/Ruby/log/agent-current",
                    b"agent error line\n",
                )
            artifact = write_artifact_bytes(
                settings,
                store,
                workspace["id"],
                tar_path.name,
                tar_path.read_bytes(),
                "application/gzip",
            )
            store.create_upload(workspace["id"], tar_path.name, artifact["id"])
            run = store.create_run(workspace["id"])

            AgentRuntime(settings, store).run_analysis(workspace["id"], run["id"])
            manifest_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 11,
                    "method": "resources/read",
                    "params": {"uri": f"logagent-v2://run/{run['id']}/manifest"},
                },
            )
            manifest = json.loads(manifest_response["result"]["contents"][0]["text"])
            paths = {item["path"] for item in manifest["files"]}
            self.assertEqual(manifest["fileCount"], 3)
            self.assertIn("extracted/NodeA/20260617120000/tsdb/query.log", paths)
            self.assertIn("extracted/NodeA/20260617120000/stream/stream.rotate.1", paths)
            self.assertIn("extracted/NodeA/20260617120000/agent/agent-current", paths)
            groups = {item["path"]: item["logGroup"] for item in manifest["files"]}
            self.assertEqual(
                groups["extracted/NodeA/20260617120000/stream/stream.rotate.1"], "stream"
            )

            grep_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 12,
                    "method": "resources/read",
                    "params": {"uri": f"logagent-v2://run/{run['id']}/grep_results"},
                },
            )
            grep_results = json.loads(grep_response["result"]["contents"][0]["text"])
            self.assertTrue(
                any("rotated gzip" in match["text"] for match in grep_results["matches"])
            )

            bad_workspace = store.create_workspace("bad node package", "diagnose", "en-US")
            bad_tar_path = Path(tmp) / "Pkg_Inst_NodeA_20260617130000_logs.tar.gz"
            with tarfile.open(bad_tar_path, "w:gz") as archive:
                add_file(archive, "wrapper/other.log", b"error outside supported dirs\n")
            bad_artifact = write_artifact_bytes(
                settings,
                store,
                bad_workspace["id"],
                bad_tar_path.name,
                bad_tar_path.read_bytes(),
                "application/gzip",
            )
            store.create_upload(bad_workspace["id"], bad_tar_path.name, bad_artifact["id"])
            bad_run = store.create_run(bad_workspace["id"])
            with self.assertRaisesRegex(ValueError, "no supported log directories"):
                AgentRuntime(settings, store).run_analysis(bad_workspace["id"], bad_run["id"])

    def test_task_mcp_reads_resources_and_runs_follow_up_search(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(data_dir=Path(tmp), api_key="test")
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace("slow query", "diagnose", "en-US")
            artifact = write_artifact_bytes(
                settings,
                store,
                workspace["id"],
                "query.log",
                b"slow query on cpu\ncache miss warning\n",
                "text/plain",
            )
            store.create_upload(workspace["id"], "query.log", artifact["id"])
            run = store.create_run(workspace["id"])
            AgentRuntime(settings, store).run_analysis(workspace["id"], run["id"])

            listed = task_mcp_response(
                settings,
                store,
                run["id"],
                {"jsonrpc": "2.0", "id": 1, "method": "resources/list"},
            )
            batch = task_mcp_response(
                settings,
                store,
                run["id"],
                [
                    {"jsonrpc": "2.0", "id": 101, "method": "initialize"},
                    {"jsonrpc": "2.0", "id": 102, "method": "resources/list"},
                ],
            )
            self.assertIsInstance(batch, list)
            self.assertEqual([item["id"] for item in batch], [101, 102])
            self.assertEqual(batch[0]["result"]["serverInfo"]["name"], "logagent-v2-task")
            ping = task_mcp_response(
                settings,
                store,
                run["id"],
                {"jsonrpc": "2.0", "id": 103, "method": "ping"},
            )
            self.assertEqual(ping["result"], {})
            prompts = task_mcp_response(
                settings,
                store,
                run["id"],
                {"jsonrpc": "2.0", "id": 104, "method": "prompts/list"},
            )
            self.assertEqual(prompts["result"]["prompts"], [])
            names = {item["name"] for item in listed["result"]["resources"]}
            resource_uris = {item["uri"] for item in listed["result"]["resources"]}
            self.assertIn("manifest", names)
            self.assertIn("grep_results", names)
            self.assertIn("artifact_index", names)
            self.assertIn("case_context", names)
            self.assertIn("tool_results", names)
            self.assertIn("mcp_calls", names)
            self.assertIn(f"logagent://task/{run['id']}/analysis_package", resource_uris)
            self.assertIn(f"logagent-v2://run/{run['id']}/analysis_package", resource_uris)

            manifest = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 2,
                    "method": "resources/read",
                    "params": {"uri": f"logagent-v2://run/{run['id']}/manifest"},
                },
            )
            manifest_body = json.loads(manifest["result"]["contents"][0]["text"])
            self.assertEqual(manifest_body["fileCount"], 1)

            search = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 3,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.search_logs",
                        "arguments": {"keywords": ["cache"]},
                    },
                },
            )
            payload = json.loads(search["result"]["content"][0]["text"])
            self.assertEqual(payload["search"]["totalMatches"], 1)
            self.assertTrue(payload["search"]["matches"][0]["ref"].startswith("log_searches/"))
            self.assertIn("#matches/0", payload["search"]["matches"][0]["ref"])
            self.assertEqual(payload["artifactPath"], payload["search"]["path"])
            self.assertEqual(payload["totalMatches"], 1)
            self.assertEqual(payload["keywordCounts"]["cache"], 1)
            self.assertEqual(payload["unmatchedKeywords"], [])
            self.assertEqual(payload["matches"][0]["evidenceRef"], payload["search"]["matches"][0]["ref"])
            self.assertEqual(payload["matches"][0]["file"], "query.log")
            self.assertEqual(payload["matches"][0]["line"], payload["search"]["matches"][0]["lineNumber"])
            self.assertEqual(payload["evidenceRefs"], [payload["search"]["matches"][0]["ref"]])
            self.assertIn("matches[].text", payload["note"])

            slice_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 4,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.get_log_slice",
                        "arguments": {
                            "path": "query.log",
                            "lineNumber": 2,
                            "before": 1,
                            "after": 0,
                        },
                    },
                },
            )
            slice_payload = json.loads(slice_response["result"]["content"][0]["text"])
            self.assertEqual(slice_payload["slice"]["startLine"], 1)
            self.assertEqual(slice_payload["slice"]["endLine"], 2)
            self.assertTrue(slice_payload["slice"]["ref"].startswith("log_slices/"))
            self.assertEqual(
                slice_payload["artifactPath"],
                slice_payload["slice"]["ref"].split("#", 1)[0],
            )
            self.assertEqual(slice_payload["evidenceRefs"], [slice_payload["slice"]["ref"]])
            self.assertEqual(slice_payload["lines"][0]["line"], 1)
            self.assertEqual(
                slice_payload["lines"][0]["lineNumber"],
                slice_payload["slice"]["lines"][0]["lineNumber"],
            )

            index_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 45,
                    "method": "resources/read",
                    "params": {"uri": f"logagent-v2://run/{run['id']}/artifact_index"},
                },
            )
            index_body = json.loads(index_response["result"]["contents"][0]["text"])
            index_paths = {item["path"] for item in index_body["artifacts"]}
            self.assertIn("session_text_input.json", index_paths)
            self.assertIn("manifest.json", index_paths)
            self.assertIn("grep_results.json", index_paths)
            self.assertIn("mcp_calls.jsonl", index_paths)
            self.assertTrue(any(path.startswith("log_searches/") for path in index_paths))

            calls_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 5,
                    "method": "resources/read",
                    "params": {"uri": f"logagent-v2://run/{run['id']}/mcp_calls"},
                },
            )
            calls_body = json.loads(calls_response["result"]["contents"][0]["text"])
            self.assertEqual(calls_body["callCount"], 4)
            self.assertEqual(
                [call["name"] for call in calls_body["calls"]],
                [
                    "resources/read",
                    "logagent.search_logs",
                    "logagent.get_log_slice",
                    "resources/read",
                ],
            )
            self.assertEqual(calls_body["calls"][0]["result"]["resource"], "manifest")
            self.assertEqual(calls_body["calls"][3]["result"]["resource"], "artifact_index")
            self.assertIn(payload["search"]["matches"][0]["ref"], calls_body["calls"][1]["evidenceRefs"])
            self.assertIn(slice_payload["slice"]["ref"], calls_body["calls"][2]["evidenceRefs"])

            analysis = get_run_analysis(settings, store, run["id"])
            self.assertEqual(analysis["resources"]["artifact_index"]["artifactCount"], len(index_paths))
            self.assertEqual(analysis["resources"]["case_context"]["caseCount"], 0)
            self.assertEqual(analysis["resources"]["tool_results"]["toolResultCount"], 0)
            self.assertEqual(analysis["resources"]["mcp_calls"]["callCount"], 5)
            self.assertEqual(
                analysis["resources"]["mcp_calls"]["calls"][-1]["result"]["resource"],
                "mcp_calls",
            )

    def test_task_mcp_search_logs_honors_max_matches(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(data_dir=Path(tmp), api_key="test")
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace("panic search", "diagnose", "en-US")
            artifact = write_artifact_bytes(
                settings,
                store,
                workspace["id"],
                "panic.log",
                b"panic one\npanic two\npanic three\n",
                "text/plain",
            )
            store.create_upload(workspace["id"], "panic.log", artifact["id"])
            run = store.create_run(workspace["id"])

            search = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 31,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.search_logs",
                        "arguments": {"keywords": ["panic"], "maxMatches": 1},
                    },
                },
            )
            payload = json.loads(search["result"]["content"][0]["text"])
            self.assertEqual(payload["search"]["totalMatches"], 1)
            self.assertTrue(payload["search"]["truncated"])
            self.assertEqual(len(payload["search"]["matches"]), 1)
            self.assertEqual(payload["totalMatches"], 1)
            self.assertTrue(payload["search"]["matches"][0]["ref"] in payload["evidenceRefs"])

    def test_task_mcp_get_log_slice_accepts_start_end_lines(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(data_dir=Path(tmp), api_key="test")
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace("line range", "diagnose", "en-US")
            artifact = write_artifact_bytes(
                settings,
                store,
                workspace["id"],
                "range.log",
                b"one\ntwo\nthree\nfour\n",
                "text/plain",
            )
            store.create_upload(workspace["id"], "range.log", artifact["id"])
            run = store.create_run(workspace["id"])

            response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 32,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.get_log_slice",
                        "arguments": {
                            "path": "range.log",
                            "startLine": 2,
                            "endLine": 3,
                        },
                    },
                },
            )
            payload = json.loads(response["result"]["content"][0]["text"])
            self.assertEqual(payload["slice"]["startLine"], 2)
            self.assertEqual(payload["slice"]["endLine"], 3)
            self.assertEqual(
                [item["text"] for item in payload["slice"]["lines"]],
                ["two", "three"],
            )
            self.assertTrue(payload["slice"]["ref"].startswith("log_slices/"))
            self.assertEqual(payload["artifactPath"], payload["slice"]["ref"].split("#", 1)[0])
            self.assertEqual(payload["evidenceRefs"], [payload["slice"]["ref"]])
            self.assertEqual([item["line"] for item in payload["lines"]], [2, 3])

            mixed = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 33,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.get_log_slice",
                        "arguments": {
                            "path": "range.log",
                            "lineNumber": 2,
                            "startLine": 2,
                            "endLine": 3,
                        },
                    },
                },
            )
            self.assertIn("cannot mix", mixed["error"]["message"])

            eof = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 34,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.get_log_slice",
                        "arguments": {
                            "path": "range.log",
                            "startLine": 10,
                            "endLine": 12,
                        },
                    },
                },
            )
            eof_payload = json.loads(eof["result"]["content"][0]["text"])
            self.assertEqual(eof_payload["slice"]["startLine"], 10)
            self.assertEqual(eof_payload["slice"]["endLine"], 4)
            self.assertEqual(eof_payload["slice"]["lines"], [])

    def test_task_mcp_runs_configured_tool_by_id(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tool = ToolDefinition(
                id="mock_tool",
                display_name="Mock Tool",
                command=sys.executable,
                args=(
                    "-c",
                    "import json; print(json.dumps({'summary':'mock ok','findings':[{'message':'hit'}]}))",
                ),
                timeout_seconds=5,
            )
            settings = Settings(
                data_dir=Path(tmp),
                api_key="test",
                tools=(tool,),
                pprof_enabled=True,
                pprof_go_command=sys.executable,
            )
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace("run tool", "diagnose", "en-US")
            run = store.create_run(workspace["id"])

            tools_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {"jsonrpc": "2.0", "id": 4, "method": "tools/list"},
            )
            run_tool_descriptor = next(
                item
                for item in tools_response["result"]["tools"]
                if item["name"] == "logagent.run_domain_tool"
            )
            self.assertEqual(
                run_tool_descriptor["inputSchema"]["anyOf"],
                [{"required": ["toolId"]}, {"required": ["tool", "inputFile"]}],
            )
            self.assertEqual(
                run_tool_descriptor["inputSchema"]["properties"]["tool"]["enum"],
                ["mock_tool"],
            )
            self.assertEqual(
                run_tool_descriptor["inputSchema"]["properties"]["toolId"]["enum"],
                ["mock_tool"],
            )

            response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 5,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.run_domain_tool",
                        "arguments": {"toolId": "mock_tool"},
                    },
                },
            )
            payload = json.loads(response["result"]["content"][0]["text"])
            self.assertEqual(payload["result"]["summary"], "mock ok")
            self.assertEqual(payload["result"]["findings"][0]["message"], "hit")
            self.assertEqual(payload["artifactPath"], "tool_results/mock_tool/result.json")
            self.assertEqual(payload["artifactPaths"], [payload["artifactPath"]])
            self.assertEqual(payload["summary"], "mock ok")
            self.assertEqual(payload["evidenceRefs"], [payload["artifactPath"]])
            self.assertEqual(
                payload["finalEvidenceRefs"],
                ["tool_results/mock_tool/result.json#findings/0"],
            )
            evidence = store.list_evidence(run["id"])
            self.assertTrue(any(item["kind"] == "tool_result" for item in evidence))
            results_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 6,
                    "method": "resources/read",
                    "params": {"uri": f"logagent-v2://run/{run['id']}/tool_results"},
                },
            )
            results_body = json.loads(results_response["result"]["contents"][0]["text"])
            self.assertEqual(results_body["toolResultCount"], 1)
            self.assertEqual(results_body["toolResults"][0]["summary"], "mock ok")
            self.assertEqual(results_body["toolResults"][0]["toolId"], "mock_tool")
            self.assertEqual(
                results_body["toolResults"][0]["path"],
                f"tool_results/{results_body['toolResults'][0]['actionId']}/result.json",
            )

    def test_env_source_built_tool_defaults_match_v1_examples(self) -> None:
        env_values = {
            "LOGAGENT_V2_TOOL_FLUX_QUERY_ANALYZER": "/opt/logagent/tools/flux_query_analyzer",
            "LOGAGENT_V2_TOOL_INFLUXQL_ANALYZER": "/opt/logagent/tools/influxql-analyzer",
            "LOGAGENT_V2_TOOL_OPENGEMINI_STORAGE_ANALYZER": (
                "/opt/logagent/tools/opengemini-storage-analyzer"
            ),
            "LOGAGENT_V2_TOOL_INFLUXDB_STORAGE_ANALYZER": (
                "/opt/logagent/tools/influxdb_storage_analyzer"
            ),
            "LOGAGENT_TOOL_FLUX_QUERY_ANALYZER": None,
            "LOGAGENT_TOOL_INFLUXQL_ANALYZER": None,
            "LOGAGENT_TOOL_OPENGEMINI_STORAGE_ANALYZER": None,
            "LOGAGENT_TOOL_INFLUXDB_STORAGE_ANALYZER": None,
        }
        previous = {key: os.environ.get(key) for key in env_values}
        try:
            for key, value in env_values.items():
                if value is None:
                    os.environ.pop(key, None)
                else:
                    os.environ[key] = value

            tools = {tool.id: tool for tool in parse_tools_env(None)}
        finally:
            for key, value in previous.items():
                if value is None:
                    os.environ.pop(key, None)
                else:
                    os.environ[key] = value

        self.assertEqual(tools["flux_query_analyzer"].timeout_seconds, 30)
        self.assertEqual(tools["flux_query_analyzer"].max_input_files, 3)
        self.assertEqual(
            tools["flux_query_analyzer"].match_file_patterns,
            ("*.jsonl", "*.ndjson"),
        )
        self.assertEqual(tools["influxql_analyzer"].timeout_seconds, 30)
        self.assertEqual(tools["influxql_analyzer"].max_input_files, 3)
        self.assertEqual(tools["influxql_analyzer"].match_file_patterns, ("*.jsonl",))
        self.assertEqual(tools["opengemini_storage_analyzer"].max_input_files, 10)
        self.assertEqual(
            tools["opengemini_storage_analyzer"].match_file_patterns,
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
        )
        self.assertEqual(tools["influxdb_storage_analyzer"].timeout_seconds, 60)
        self.assertEqual(tools["influxdb_storage_analyzer"].max_input_files, 5)
        self.assertEqual(
            tools["influxdb_storage_analyzer"].match_file_patterns,
            ("*.tsm", "*.tsi"),
        )
        self.assertIn("series file", tools["influxdb_storage_analyzer"].match_keywords)

    def test_parse_tools_env_expands_command_and_allows_disabled_relative(self) -> None:
        previous = os.environ.get("LOGAGENT_TEST_TOOL_DIR")
        try:
            with tempfile.TemporaryDirectory() as tmpdir:
                os.environ["LOGAGENT_TEST_TOOL_DIR"] = tmpdir
                raw = json.dumps(
                    [
                        {
                            "id": "expanded_tool",
                            "command": "${LOGAGENT_TEST_TOOL_DIR}/mock-tool",
                            "enabled": True,
                        },
                        {
                            "id": "disabled_relative_tool",
                            "command": "relative-disabled-tool",
                            "enabled": False,
                        },
                    ]
                )

                tools = {tool.id: tool for tool in parse_tools_env(raw)}
                self.assertEqual(
                    tools["expanded_tool"].command,
                    str(Path(tmpdir) / "mock-tool"),
                )
                self.assertEqual(
                    tools["disabled_relative_tool"].command,
                    "relative-disabled-tool",
                )
        finally:
            if previous is None:
                os.environ.pop("LOGAGENT_TEST_TOOL_DIR", None)
            else:
                os.environ["LOGAGENT_TEST_TOOL_DIR"] = previous

    def test_parse_tools_env_accepts_v1_map_path_env_and_snake_case(self) -> None:
        env_values = {
            "LOGAGENT_TEST_TOOL_DIR": None,
            "LOGAGENT_TEST_PATH_ENV": None,
        }
        previous = {key: os.environ.get(key) for key in env_values}
        try:
            with tempfile.TemporaryDirectory() as tmpdir:
                os.environ["LOGAGENT_TEST_TOOL_DIR"] = tmpdir
                os.environ["LOGAGENT_TEST_PATH_ENV"] = "${LOGAGENT_TEST_TOOL_DIR}/v1-tool"
                raw = json.dumps(
                    {
                        "v1_tool": {
                            "path_env": "LOGAGENT_TEST_PATH_ENV",
                            "timeout_seconds": 9,
                            "max_output_bytes": 4096,
                            "max_input_files": 4,
                            "args": ["--input", "{input_file}"],
                            "match": {
                                "file_patterns": ["*.LOG"],
                                "keywords": ["ERROR"],
                            },
                        },
                        "disabled_v1_tool": {
                            "path": "relative-disabled-tool",
                            "enabled": False,
                        },
                        "disabled_env_tool": {
                            "path_env": "LOGAGENT_TEST_PATH_ENV",
                            "enabled": False,
                        },
                    }
                )

                tools = {tool.id: tool for tool in parse_tools_env(raw)}
                self.assertEqual(
                    tools["v1_tool"].command,
                    str(Path(tmpdir) / "v1-tool"),
                )
                self.assertEqual(tools["v1_tool"].timeout_seconds, 9)
                self.assertEqual(tools["v1_tool"].max_output_bytes, 4096)
                self.assertEqual(tools["v1_tool"].max_input_files, 4)
                self.assertEqual(tools["v1_tool"].args, ("--input", "{input_file}"))
                self.assertEqual(tools["v1_tool"].match_file_patterns, ("*.log",))
                self.assertEqual(tools["v1_tool"].match_keywords, ("error",))
                self.assertEqual(
                    tools["disabled_v1_tool"].command,
                    "relative-disabled-tool",
                )
                self.assertEqual(tools["disabled_env_tool"].command, "")
        finally:
            for key, value in previous.items():
                if value is None:
                    os.environ.pop(key, None)
                else:
                    os.environ[key] = value

    def test_parse_tools_env_rejects_missing_enabled_path_env(self) -> None:
        previous = os.environ.get("LOGAGENT_TEST_MISSING_PATH_ENV")
        try:
            os.environ.pop("LOGAGENT_TEST_MISSING_PATH_ENV", None)
            raw = json.dumps(
                {
                    "missing_path_env_tool": {
                        "path_env": "LOGAGENT_TEST_MISSING_PATH_ENV",
                        "enabled": True,
                    }
                }
            )
            with self.assertRaisesRegex(ValueError, "path_env .* is not set"):
                parse_tools_env(raw)
        finally:
            if previous is None:
                os.environ.pop("LOGAGENT_TEST_MISSING_PATH_ENV", None)
            else:
                os.environ["LOGAGENT_TEST_MISSING_PATH_ENV"] = previous

    def test_parse_tools_env_rejects_enabled_relative_command(self) -> None:
        raw = json.dumps(
            [
                {
                    "id": "relative_tool",
                    "command": "relative-tool",
                    "enabled": True,
                }
            ]
        )
        with self.assertRaisesRegex(ValueError, "absolute path"):
            parse_tools_env(raw)

    def test_parse_tools_env_rejects_invalid_tool_id(self) -> None:
        for tool_id in ["", "bad id", "bad/id", "bad.id"]:
            with self.subTest(tool_id=tool_id):
                raw = json.dumps(
                    [
                        {
                            "id": tool_id,
                            "command": "/tmp/mock-tool",
                            "enabled": False,
                        }
                    ]
                )
                with self.assertRaisesRegex(ValueError, "invalid tool name"):
                    parse_tools_env(raw)

    def test_parse_tools_env_normalizes_match_values(self) -> None:
        raw = json.dumps(
            [
                {
                    "id": "match_tool",
                    "command": "/tmp/mock-tool",
                    "enabled": False,
                    "match": {
                        "filePatterns": ["*.LOG", "*Timeout*"],
                        "keywords": ["ERROR", "Slow Query"],
                    },
                }
            ]
        )
        tools = {tool.id: tool for tool in parse_tools_env(raw)}
        self.assertEqual(tools["match_tool"].match_file_patterns, ("*.log", "*timeout*"))
        self.assertEqual(tools["match_tool"].match_keywords, ("error", "slow query"))

    def test_settings_rejects_enabled_fetch_without_allowlist(self) -> None:
        env_values = {
            "LOGAGENT_V2_FETCH_ENABLED": "1",
            "LOGAGENT_V2_FETCH_ALLOWED_HOSTS": "",
        }
        previous = {key: os.environ.get(key) for key in env_values}
        try:
            os.environ.update(env_values)
            with self.assertRaisesRegex(ValueError, "FETCH_ALLOWED_HOSTS.*not be empty"):
                Settings.from_env()
        finally:
            for key, value in previous.items():
                if value is None:
                    os.environ.pop(key, None)
                else:
                    os.environ[key] = value

    def test_settings_normalizes_scheme_specific_fetch_allowed_hosts(self) -> None:
        key = base64.urlsafe_b64encode(b"2" * 32).decode("ascii")
        env_values = {
            "LOGAGENT_V2_FETCH_ENABLED": "1",
            "LOGAGENT_V2_FETCH_ALLOWED_HOSTS": "HTTP://127.0.0.1:50992,https://example.com",
            "LOGAGENT_V2_FETCH_SECRET_KEY": key,
        }
        previous = {key: os.environ.get(key) for key in env_values}
        try:
            os.environ.update(env_values)
            settings = Settings.from_env()
        finally:
            for key, value in previous.items():
                if value is None:
                    os.environ.pop(key, None)
                else:
                    os.environ[key] = value

        self.assertEqual(
            settings.fetch_allowed_hosts,
            ("http://127.0.0.1:50992", "https://example.com:443"),
        )
        validate_url_allowed(settings, "http://127.0.0.1:50992/getdata")
        validate_url_allowed(settings, "https://example.com/getdata")
        with self.assertRaisesRegex(ValueError, "not in allowlist"):
            validate_url_allowed(settings, "https://127.0.0.1:50992/getdata")
        with self.assertRaisesRegex(ValueError, "not in allowlist"):
            validate_url_allowed(settings, "http://example.com/getdata")

    def test_settings_validates_fetch_secret_key_when_enabled(self) -> None:
        env_names = {
            "LOGAGENT_V2_FETCH_ENABLED",
            "LOGAGENT_V2_FETCH_ALLOWED_HOSTS",
            "LOGAGENT_V2_FETCH_SECRET_KEY",
        }
        previous = {key: os.environ.get(key) for key in env_names}
        try:
            os.environ["LOGAGENT_V2_FETCH_ENABLED"] = "1"
            os.environ["LOGAGENT_V2_FETCH_ALLOWED_HOSTS"] = "127.0.0.1"
            os.environ.pop("LOGAGENT_V2_FETCH_SECRET_KEY", None)
            with self.assertRaisesRegex(ValueError, "FETCH_SECRET_KEY.*required"):
                Settings.from_env()

            os.environ["LOGAGENT_V2_FETCH_SECRET_KEY"] = "not-base64!"
            with self.assertRaisesRegex(ValueError, "FETCH_SECRET_KEY.*base64"):
                Settings.from_env()

            os.environ["LOGAGENT_V2_FETCH_SECRET_KEY"] = base64.urlsafe_b64encode(
                b"short"
            ).decode("ascii")
            with self.assertRaisesRegex(ValueError, "FETCH_SECRET_KEY.*32 bytes"):
                Settings.from_env()

            valid_key = base64.urlsafe_b64encode(b"3" * 32).decode("ascii")
            os.environ["LOGAGENT_V2_FETCH_SECRET_KEY"] = f" {valid_key} "
            settings = Settings.from_env()
        finally:
            for key, value in previous.items():
                if value is None:
                    os.environ.pop(key, None)
                else:
                    os.environ[key] = value

        self.assertEqual(settings.fetch_secret_key, valid_key)

    def test_settings_validates_huawei_package_sync_when_enabled(self) -> None:
        env_names = {
            "LOGAGENT_V2_HUAWEI_PACKAGE_SYNC_ENABLED",
            "LOGAGENT_V2_HUAWEI_OBS_ENDPOINT",
            "LOGAGENT_V2_HUAWEI_OBS_BUCKET",
            "LOGAGENT_V2_HUAWEI_OBS_OBJECT_PREFIX",
            "LOGAGENT_V2_HUAWEI_OBS_ACCESS_KEY",
            "LOGAGENT_V2_HUAWEI_OBS_SECRET_KEY",
            "LOGAGENT_V2_HUAWEI_OBS_SECURITY_TOKEN",
            "LOGAGENT_V2_HUAWEI_GAUSSDB_DSN",
            "LOGAGENT_V2_HUAWEI_TIMEOUT_SECONDS",
        }
        previous = {key: os.environ.get(key) for key in env_names}
        base = {
            "LOGAGENT_V2_HUAWEI_PACKAGE_SYNC_ENABLED": "1",
            "LOGAGENT_V2_HUAWEI_OBS_ENDPOINT": "https://obs.example.com/",
            "LOGAGENT_V2_HUAWEI_OBS_BUCKET": "valid-bucket.1",
            "LOGAGENT_V2_HUAWEI_OBS_OBJECT_PREFIX": "/packages/demo/",
            "LOGAGENT_V2_HUAWEI_OBS_ACCESS_KEY": " access ",
            "LOGAGENT_V2_HUAWEI_OBS_SECRET_KEY": " secret ",
            "LOGAGENT_V2_HUAWEI_OBS_SECURITY_TOKEN": " token ",
            "LOGAGENT_V2_HUAWEI_GAUSSDB_DSN": " postgresql://example/db ",
            "LOGAGENT_V2_HUAWEI_TIMEOUT_SECONDS": "0",
        }
        try:
            os.environ.update(base)
            os.environ["LOGAGENT_V2_HUAWEI_OBS_ENDPOINT"] = ""
            with self.assertRaisesRegex(ValueError, "HUAWEI_OBS_ENDPOINT.*required"):
                Settings.from_env()

            os.environ["LOGAGENT_V2_HUAWEI_OBS_ENDPOINT"] = "https://obs.example.com/path"
            with self.assertRaisesRegex(ValueError, "HUAWEI_OBS_ENDPOINT.*path"):
                Settings.from_env()

            os.environ["LOGAGENT_V2_HUAWEI_OBS_ENDPOINT"] = "https://obs.example.com/"
            os.environ["LOGAGENT_V2_HUAWEI_OBS_BUCKET"] = "bad_bucket"
            with self.assertRaisesRegex(ValueError, "HUAWEI_OBS_BUCKET.*unsupported"):
                Settings.from_env()

            os.environ.update(base)
            settings = Settings.from_env()
        finally:
            for key, value in previous.items():
                if value is None:
                    os.environ.pop(key, None)
                else:
                    os.environ[key] = value

        huawei = settings.huawei_package_sync
        self.assertTrue(huawei.enabled)
        self.assertEqual(huawei.obs_endpoint, "https://obs.example.com")
        self.assertEqual(huawei.obs_bucket, "valid-bucket.1")
        self.assertEqual(huawei.obs_object_prefix, "packages/demo")
        self.assertEqual(huawei.obs_access_key, "access")
        self.assertEqual(huawei.obs_secret_key, "secret")
        self.assertEqual(huawei.obs_security_token, "token")
        self.assertEqual(huawei.gaussdb_dsn, "postgresql://example/db")
        self.assertEqual(huawei.timeout_seconds, 1)
        descriptors = {item["toolId"]: item for item in tool_descriptors(settings)}
        self.assertTrue(descriptors["logagent.huawei_cloud_package_sync"]["runnable"])

    def test_settings_validates_agent_provider_env(self) -> None:
        env_names = {
            "LOGAGENT_V2_AGENT_PROVIDER",
            "LOGAGENT_V2_AGENT_BASE_URL",
            "LOGAGENT_V2_AGENT_MODEL",
            "LOGAGENT_V2_AGENT_API_KEY",
            "LOGAGENT_V2_AGENT_BINARY_PATH",
        }
        previous = {key: os.environ.get(key) for key in env_names}
        try:
            os.environ["LOGAGENT_V2_AGENT_PROVIDER"] = "unknown"
            with self.assertRaisesRegex(ValueError, "AGENT_PROVIDER.*openai_compatible"):
                Settings.from_env()

            os.environ["LOGAGENT_V2_AGENT_PROVIDER"] = "openai_compatible"
            os.environ.pop("LOGAGENT_V2_AGENT_BASE_URL", None)
            os.environ["LOGAGENT_V2_AGENT_MODEL"] = "model"
            os.environ["LOGAGENT_V2_AGENT_API_KEY"] = "key"
            with self.assertRaisesRegex(ValueError, "AGENT_BASE_URL.*required"):
                Settings.from_env()

            os.environ["LOGAGENT_V2_AGENT_BASE_URL"] = " https://api.example.com/v1 "
            os.environ.pop("LOGAGENT_V2_AGENT_API_KEY", None)
            with self.assertRaisesRegex(ValueError, "AGENT_API_KEY.*required"):
                Settings.from_env()

            os.environ["LOGAGENT_V2_AGENT_API_KEY"] = " secret "
            settings = Settings.from_env()
            self.assertEqual(settings.agent_provider, "openai_compatible")
            self.assertEqual(settings.agent_base_url, "https://api.example.com/v1")
            self.assertEqual(settings.agent_model, "model")
            self.assertEqual(settings.agent_api_key, "secret")

            os.environ["LOGAGENT_V2_AGENT_PROVIDER"] = "binary"
            os.environ.pop("LOGAGENT_V2_AGENT_BINARY_PATH", None)
            with self.assertRaisesRegex(ValueError, "AGENT_BINARY_PATH.*required"):
                Settings.from_env()

            os.environ["LOGAGENT_V2_AGENT_BINARY_PATH"] = "relative-agent"
            with self.assertRaisesRegex(ValueError, "AGENT_BINARY_PATH.*absolute path"):
                Settings.from_env()

            os.environ["LOGAGENT_V2_AGENT_BINARY_PATH"] = "/opt/logagent/bin/agent"
            settings = Settings.from_env()
        finally:
            for key, value in previous.items():
                if value is None:
                    os.environ.pop(key, None)
                else:
                    os.environ[key] = value

        self.assertEqual(settings.agent_provider, "binary")
        self.assertEqual(settings.agent_binary_path, Path("/opt/logagent/bin/agent"))

    def test_settings_validates_pprof_go_command_when_enabled(self) -> None:
        env_names = {
            "LOGAGENT_V2_PPROF_ENABLED",
            "LOGAGENT_V2_PPROF_GO_COMMAND",
            "LOGAGENT_TOOL_PPROF_GO",
            "LOGAGENT_TEST_PPROF_DIR",
        }
        previous = {key: os.environ.get(key) for key in env_names}
        try:
            os.environ.pop("LOGAGENT_V2_PPROF_ENABLED", None)
            os.environ.pop("LOGAGENT_V2_PPROF_GO_COMMAND", None)
            os.environ.pop("LOGAGENT_TOOL_PPROF_GO", None)
            settings = Settings.from_env()
            self.assertFalse(settings.pprof_enabled)
            self.assertIsNone(settings.pprof_go_command)

            os.environ["LOGAGENT_V2_PPROF_ENABLED"] = "1"
            with self.assertRaisesRegex(ValueError, "PPROF_GO_COMMAND.*required"):
                Settings.from_env()

            os.environ["LOGAGENT_V2_PPROF_GO_COMMAND"] = "go"
            with self.assertRaisesRegex(ValueError, "PPROF_GO_COMMAND.*absolute path"):
                Settings.from_env()

            os.environ["LOGAGENT_V2_PPROF_ENABLED"] = "0"
            settings = Settings.from_env()
            self.assertFalse(settings.pprof_enabled)
            self.assertEqual(settings.pprof_go_command, "go")

            with tempfile.TemporaryDirectory() as tmpdir:
                os.environ["LOGAGENT_TEST_PPROF_DIR"] = tmpdir
                os.environ["LOGAGENT_V2_PPROF_ENABLED"] = "1"
                os.environ["LOGAGENT_V2_PPROF_GO_COMMAND"] = (
                    "${LOGAGENT_TEST_PPROF_DIR}/go"
                )
                settings = Settings.from_env()
        finally:
            for key, value in previous.items():
                if value is None:
                    os.environ.pop(key, None)
                else:
                    os.environ[key] = value

        self.assertTrue(settings.pprof_enabled)
        self.assertEqual(settings.pprof_go_command, str(Path(tmpdir) / "go"))

    def test_settings_clamps_job_and_fetch_numeric_limits(self) -> None:
        env_values = {
            "LOGAGENT_V2_MAX_CONCURRENT_JOBS": "0",
            "LOGAGENT_V2_FETCH_TIMEOUT_SECONDS": "0",
            "LOGAGENT_V2_FETCH_MAX_REQUEST_BYTES": "0",
            "LOGAGENT_V2_FETCH_MAX_RESPONSE_BYTES": "0",
            "LOGAGENT_V2_FETCH_MAX_REDIRECTS": "-1",
        }
        previous = {key: os.environ.get(key) for key in env_values}
        try:
            os.environ.update(env_values)
            settings = Settings.from_env()
        finally:
            for key, value in previous.items():
                if value is None:
                    os.environ.pop(key, None)
                else:
                    os.environ[key] = value

        self.assertEqual(settings.max_concurrent_jobs, 1)
        self.assertEqual(settings.fetch_timeout_seconds, 1)
        self.assertEqual(settings.fetch_max_request_bytes, 1)
        self.assertEqual(settings.fetch_max_response_bytes, 1)
        self.assertEqual(settings.fetch_max_redirects, 0)

    def test_source_built_tool_env_rejects_relative_command(self) -> None:
        env_values = {
            "LOGAGENT_V2_TOOL_INFLUXQL_ANALYZER": "relative-influxql-analyzer",
            "LOGAGENT_TOOL_INFLUXQL_ANALYZER": None,
            "LOGAGENT_V2_TOOL_FLUX_QUERY_ANALYZER": None,
            "LOGAGENT_TOOL_FLUX_QUERY_ANALYZER": None,
            "LOGAGENT_V2_TOOL_OPENGEMINI_STORAGE_ANALYZER": None,
            "LOGAGENT_TOOL_OPENGEMINI_STORAGE_ANALYZER": None,
            "LOGAGENT_V2_TOOL_INFLUXDB_STORAGE_ANALYZER": None,
            "LOGAGENT_TOOL_INFLUXDB_STORAGE_ANALYZER": None,
        }
        previous = {key: os.environ.get(key) for key in env_values}
        try:
            for key, value in env_values.items():
                if value is None:
                    os.environ.pop(key, None)
                else:
                    os.environ[key] = value

            with self.assertRaisesRegex(ValueError, "influxql_analyzer.*absolute path"):
                parse_tools_env(None)
        finally:
            for key, value in previous.items():
                if value is None:
                    os.environ.pop(key, None)
                else:
                    os.environ[key] = value

    def test_settings_rejects_relative_remote_ssh_command_when_enabled(self) -> None:
        env_values = {
            "LOGAGENT_V2_REMOTE_EXECUTION_ENABLED": "1",
            "LOGAGENT_V2_REMOTE_SSH_COMMAND": "ssh",
        }
        previous = {key: os.environ.get(key) for key in env_values}
        try:
            os.environ.update(env_values)
            with self.assertRaisesRegex(ValueError, "REMOTE_SSH_COMMAND.*absolute path"):
                Settings.from_env()
        finally:
            for key, value in previous.items():
                if value is None:
                    os.environ.pop(key, None)
                else:
                    os.environ[key] = value

    def test_settings_allows_relative_remote_ssh_command_when_disabled(self) -> None:
        env_values = {
            "LOGAGENT_V2_REMOTE_EXECUTION_ENABLED": "0",
            "LOGAGENT_V2_REMOTE_SSH_COMMAND": "ssh",
        }
        previous = {key: os.environ.get(key) for key in env_values}
        try:
            os.environ.update(env_values)
            settings = Settings.from_env()
        finally:
            for key, value in previous.items():
                if value is None:
                    os.environ.pop(key, None)
                else:
                    os.environ[key] = value

        self.assertFalse(settings.remote_execution_enabled)
        self.assertEqual(settings.remote_ssh_command, "ssh")

    def test_settings_validates_remote_host_key_policy(self) -> None:
        previous = os.environ.get("LOGAGENT_V2_REMOTE_HOST_KEY_POLICY")
        try:
            os.environ["LOGAGENT_V2_REMOTE_HOST_KEY_POLICY"] = "STRICT"
            self.assertEqual(Settings.from_env().remote_host_key_policy, "strict")

            os.environ["LOGAGENT_V2_REMOTE_HOST_KEY_POLICY"] = "no"
            self.assertEqual(Settings.from_env().remote_host_key_policy, "no")

            os.environ["LOGAGENT_V2_REMOTE_HOST_KEY_POLICY"] = "off"
            with self.assertRaisesRegex(ValueError, "HOST_KEY_POLICY.*accept-new, strict, or no"):
                Settings.from_env()
        finally:
            if previous is None:
                os.environ.pop("LOGAGENT_V2_REMOTE_HOST_KEY_POLICY", None)
            else:
                os.environ["LOGAGENT_V2_REMOTE_HOST_KEY_POLICY"] = previous

    def test_parse_remote_commands_env_validates_command_id(self) -> None:
        templates = parse_remote_commands_env(
            json.dumps([{"id": "smoke-1_ok", "argv": ["true"]}])
        )
        self.assertEqual(templates[0].command_id, "smoke-1_ok")

        for command_id in ["", "bad id", "bad/id", "bad.id"]:
            with self.subTest(command_id=command_id):
                raw = json.dumps([{"id": command_id, "argv": ["true"]}])
                with self.assertRaisesRegex(ValueError, "invalid remote command id"):
                    parse_remote_commands_env(raw)

    def test_parse_remote_commands_env_normalizes_argv(self) -> None:
        templates = parse_remote_commands_env(
            json.dumps(
                [
                    {
                        "id": "smoke_ls_root",
                        "argv": ["  ls ", "", " -la", "  /root  "],
                    }
                ]
            )
        )
        self.assertEqual(templates[0].argv, ("ls", "-la", "/root"))

        with self.assertRaisesRegex(ValueError, "argv must not be empty"):
            parse_remote_commands_env(json.dumps([{"id": "empty", "argv": [" ", ""]}]))

    def test_strict_host_key_checking_rejects_unknown_policy(self) -> None:
        self.assertEqual(strict_host_key_checking_value("strict"), "yes")
        self.assertEqual(strict_host_key_checking_value("no"), "no")
        self.assertEqual(strict_host_key_checking_value("accept-new"), "accept-new")
        with self.assertRaisesRegex(ValueError, "accept-new, strict, or no"):
            strict_host_key_checking_value("off")

    def test_tool_registry_includes_configured_and_builtin_tools(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tool = ToolDefinition(
                id="mock_tool",
                display_name="Mock Tool",
                command=sys.executable,
                args=("-c", "print('ok')"),
                timeout_seconds=5,
                match_file_patterns=("*.log", "*timeout*"),
                match_keywords=("timeout",),
            )
            disabled_tool = ToolDefinition(
                id="disabled_tool",
                display_name="Disabled Tool",
                command=sys.executable,
                args=("-c", "print('disabled')"),
                enabled=False,
                match_file_patterns=("*.trace",),
            )
            settings = Settings(
                data_dir=Path(tmp),
                api_key="test",
                tools=(tool, disabled_tool),
            )

            descriptors = {item["toolId"]: item for item in tool_descriptors(settings)}

            self.assertEqual(descriptors["mock_tool"]["source"], "configured")
            self.assertEqual(descriptors["mock_tool"]["backend"], "command")
            self.assertEqual(descriptors["mock_tool"]["readOnly"], False)
            self.assertEqual(descriptors["mock_tool"]["editable"], True)
            self.assertEqual(descriptors["mock_tool"]["exportable"], True)
            self.assertEqual(descriptors["mock_tool"]["minFiles"], 1)
            self.assertEqual(descriptors["mock_tool"]["acceptedSuffixes"], ["*.log", "*timeout*"])
            mock_schema = descriptors["mock_tool"]["paramsSchema"]
            self.assertEqual(
                mock_schema["configuredArgs"]["value"],
                ["-c", "print('ok')"],
            )
            self.assertEqual(
                mock_schema["match"]["properties"]["filePatterns"]["value"],
                ["*.log", "*timeout*"],
            )
            self.assertEqual(
                mock_schema["match"]["properties"]["keywords"]["value"],
                ["timeout"],
            )
            self.assertEqual(
                mock_schema["properties"]["configuredArgs"],
                mock_schema["configuredArgs"],
            )
            self.assertEqual(mock_schema["properties"]["match"], mock_schema["match"])
            self.assertIn("manual-run", descriptors["mock_tool"]["tags"])
            self.assertIn("tool-runner", descriptors["mock_tool"]["tags"])
            self.assertIn("external", descriptors["mock_tool"]["tags"])
            self.assertEqual(descriptors["disabled_tool"]["source"], "configured")
            self.assertEqual(descriptors["disabled_tool"]["backend"], "command")
            self.assertEqual(descriptors["disabled_tool"]["readOnly"], False)
            self.assertEqual(descriptors["disabled_tool"]["editable"], True)
            self.assertEqual(descriptors["disabled_tool"]["exportable"], False)
            self.assertEqual(descriptors["disabled_tool"]["runnable"], False)
            with self.assertRaises(ValueError):
                validate_manual_tool_run(settings, "mock_tool", upload_count=0, params={})
            self.assertEqual(
                validate_manual_tool_run(settings, "mock_tool", upload_count=1, params={}),
                {},
            )
            with self.assertRaisesRegex(ValueError, "does not accept upload"):
                validate_manual_tool_run(
                    settings,
                    "mock_tool",
                    upload_count=1,
                    params={},
                    upload_filenames=["query.txt"],
                )
            self.assertEqual(
                validate_manual_tool_run(
                    settings,
                    "mock_tool",
                    upload_count=1,
                    params={},
                    upload_filenames=["query.log"],
                ),
                {},
            )
            self.assertIn("logagent.preprocess_log_package", descriptors)
            self.assertIn("logagent.list_metadata_instances", descriptors)
            self.assertIn("logagent.get_metadata_snapshot", descriptors)
            self.assertIn("logagent.fetch", descriptors)
            self.assertIn("pprof_analyzer", descriptors)
            self.assertIn("logagent.huawei_cloud_package_sync", descriptors)
            self.assertEqual(
                descriptors["logagent.list_metadata_instances"]["displayName"],
                "Metadata instances",
            )
            self.assertEqual(descriptors["logagent.list_metadata_instances"]["backend"], "builtin")
            self.assertIn("read-only", descriptors["logagent.list_metadata_instances"]["tags"])
            self.assertIn("manual-run", descriptors["logagent.list_metadata_instances"]["tags"])
            self.assertEqual(
                descriptors["logagent.get_metadata_field_types"]["paramsTemplate"],
                {
                    "instanceId": "",
                    "database": "",
                    "measurement": "",
                    "retentionPolicy": "",
                    "field": [],
                },
            )
            self.assertEqual(
                descriptors["logagent.get_metadata_tag_fields"]["paramsTemplate"],
                {
                    "instanceId": "",
                    "database": "",
                    "measurement": "",
                    "retentionPolicy": "",
                },
            )
            self.assertIn(
                "normalize rotated logs",
                descriptors["logagent.preprocess_log_package"]["description"],
            )
            self.assertEqual(
                descriptors["logagent.preprocess_log_package"]["outputViews"],
                ["summary", "nodes", "log_groups", "tool_inputs", "warnings"],
            )
            self.assertFalse(descriptors["logagent.fetch"]["readOnly"])
            self.assertIn("manual-run", descriptors["logagent.fetch"]["tags"])
            self.assertEqual(
                descriptors["logagent.fetch"]["paramsTemplate"],
                {"fetchId": "", "variables": {}, "headers": {}, "body": None},
            )
            self.assertEqual(
                descriptors["logagent.fetch"]["outputViews"],
                ["summary", "request", "response", "body_artifact"],
            )
            with self.assertRaisesRegex(ValueError, "endpointId or fetchId"):
                validate_tool_run_params(settings, "logagent.fetch", {})
            self.assertEqual(descriptors["pprof_analyzer"]["source"], "configured")
            self.assertEqual(descriptors["pprof_analyzer"]["backend"], "command")
            self.assertEqual(descriptors["pprof_analyzer"]["readOnly"], False)
            self.assertEqual(descriptors["pprof_analyzer"]["editable"], True)
            self.assertEqual(descriptors["pprof_analyzer"]["exportable"], False)
            self.assertEqual(descriptors["pprof_analyzer"]["manualOnly"], True)
            self.assertEqual(
                descriptors["pprof_analyzer"]["paramsTemplate"]["nodeCount"],
                50,
            )
            pprof_schema = descriptors["pprof_analyzer"]["paramsSchema"]
            self.assertEqual(pprof_schema["sampleIndex"]["default"], "samples")
            self.assertEqual(pprof_schema["nodeCount"]["maximum"], 200)
            self.assertEqual(
                pprof_schema["properties"]["generateSvg"],
                pprof_schema["generateSvg"],
            )
            self.assertEqual(
                validate_tool_run_params(
                    settings,
                    "pprof_analyzer",
                    {"sampleIndex": " alloc_space ", "nodeCount": 999},
                )["nodeCount"],
                200,
            )
            with self.assertRaisesRegex(ValueError, "sampleIndex"):
                validate_tool_run_params(settings, "pprof_analyzer", {"sampleIndex": ""})
            with self.assertRaisesRegex(ValueError, "sampleIndex"):
                validate_tool_run_params(settings, "pprof_analyzer", {"sampleIndex": None})
            with self.assertRaisesRegex(ValueError, "sampleIndex"):
                validate_tool_run_params(
                    settings,
                    "pprof_analyzer",
                    {"sampleIndex": "bad/value"},
                )
            with self.assertRaisesRegex(ValueError, "generateSvg"):
                validate_tool_run_params(
                    settings,
                    "pprof_analyzer",
                    {"generateSvg": "false"},
                )
            self.assertEqual(
                descriptors["logagent.huawei_cloud_package_sync"]["acceptedSuffixes"],
                ["*"],
            )
            self.assertEqual(
                descriptors["logagent.huawei_cloud_package_sync"]["displayName"],
                "Huawei OBS + GaussDB Package Sync",
            )
            self.assertIn(
                "huawei-cloud",
                descriptors["logagent.huawei_cloud_package_sync"]["tags"],
            )
            self.assertEqual(
                descriptors["logagent.huawei_cloud_package_sync"]["outputViews"],
                ["summary", "obs", "gaussdb", "json"],
            )
            field_schema = descriptors["logagent.get_metadata_field_types"][
                "paramsSchema"
            ]["properties"]["field"]
            self.assertEqual(field_schema["oneOf"][0]["type"], "string")
            self.assertEqual(field_schema["oneOf"][1]["type"], "array")
            self.assertEqual(field_schema["oneOf"][1]["minItems"], 1)
            mcp_metadata = {
                item["name"]: item for item in metadata_tool_descriptors()
            }
            mcp_field_schema = mcp_metadata["logagent.get_metadata_field_types"][
                "inputSchema"
            ]["properties"]["field"]
            self.assertEqual(mcp_field_schema, field_schema)
            self.assertEqual(
                validate_tool_run_params(
                    settings,
                    "logagent.get_metadata_field_types",
                    {
                        "instanceId": " inst1 ",
                        "database": " db0 ",
                        "measurement": " cpu ",
                        "field": " ",
                    },
                ),
                {"instanceId": "inst1", "database": "db0", "measurement": "cpu"},
            )
            self.assertEqual(
                validate_tool_run_params(
                    settings,
                    "logagent.get_metadata_field_types",
                    {
                        "instanceId": "inst1",
                        "database": "db0",
                        "measurement": "cpu",
                        "field": [" host ", "value"],
                    },
                )["field"],
                ["host", "value"],
            )
            with self.assertRaisesRegex(ValueError, "field entries must be non-empty strings"):
                validate_tool_run_params(
                    settings,
                    "logagent.get_metadata_field_types",
                    {
                        "instanceId": "inst1",
                        "database": "db0",
                        "measurement": "cpu",
                        "field": ["host", " "],
                    },
                )
            with self.assertRaisesRegex(ValueError, "does not accept upload"):
                validate_manual_tool_run(
                    settings,
                    "logagent.preprocess_log_package",
                    upload_count=1,
                    params={},
                    upload_filenames=["logs.zip"],
                )
            self.assertEqual(
                validate_manual_tool_run(
                    settings,
                    "logagent.preprocess_log_package",
                    upload_count=1,
                    params={},
                    upload_filenames=["Pkg_Inst_Node_20260617120000_logs.tar.gz"],
                ),
                {},
            )

    def test_tool_run_route_rejects_upload_suffix_mismatch(self) -> None:
        from fastapi.testclient import TestClient
        from logagent_v2.api import create_app

        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(data_dir=Path(tmp), api_key="test", inline_worker=False)
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace("manual preprocess", "tool_run", "en-US")
            artifact = write_artifact_bytes(
                settings,
                store,
                workspace["id"],
                "logs.zip",
                b"not a tar.gz",
                "application/zip",
            )
            upload = store.create_upload(workspace["id"], "logs.zip", artifact["id"])

            with TestClient(create_app(settings)) as client:
                response = client.post(
                    "/api/v2/tools/logagent.preprocess_log_package/runs",
                    headers={"Authorization": "Bearer test"},
                    json={
                        "workspaceId": workspace["id"],
                        "uploadIds": [upload["id"]],
                        "params": {},
                    },
                )

            self.assertEqual(response.status_code, 400)
            self.assertIn("does not accept upload", response.json()["detail"])

    def test_preprocess_tool_run_materializes_node_package_inputs(self) -> None:
        def add_file(archive: tarfile.TarFile, name: str, data: bytes) -> None:
            info = tarfile.TarInfo(name)
            info.size = len(data)
            archive.addfile(info, io.BytesIO(data))

        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(data_dir=Path(tmp), api_key="test")
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace("manual preprocess", "tool_run", "en-US")
            tar_path = Path(tmp) / "Pkg_Inst_NodeA_20260617120000_logs.tar.gz"
            with tarfile.open(tar_path, "w:gz") as archive:
                add_file(
                    archive,
                    "wrapper/var/chroot/gemini/log/tsdb/query.log",
                    b"select * from cpu\n",
                )
                add_file(
                    archive,
                    "wrapper/var/chroot/gemini/log/stream/stream.log",
                    b"stream timeout warning\n",
                )
            artifact = write_artifact_bytes(
                settings,
                store,
                workspace["id"],
                tar_path.name,
                tar_path.read_bytes(),
                "application/gzip",
            )
            upload = store.create_upload(workspace["id"], tar_path.name, artifact["id"])
            params = validate_manual_tool_run(
                settings,
                "logagent.preprocess_log_package",
                upload_count=1,
                params={},
                upload_filenames=[tar_path.name],
            )
            tool_run = store.create_tool_run(
                workspace_id=workspace["id"],
                tool_id="logagent.preprocess_log_package",
                params=params,
                upload_ids=[upload["id"]],
            )

            executed = execute_tool_run(settings, store, tool_run["id"])
            result = executed["result"]

            self.assertEqual(result["toolId"], "logagent.preprocess_log_package")
            self.assertEqual(result["uploadCount"], 1)
            self.assertEqual(result["fileCount"], 2)
            self.assertEqual(result["logGroups"], {"tsdb": 1, "stream": 1})
            self.assertEqual(result["nodePackages"][0]["nodeId"], "NodeA")
            self.assertEqual(result["nodes"][0]["nodeId"], "NodeA")
            self.assertEqual(result["nodes"][0]["packages"], 1)
            self.assertEqual(result["nodes"][0]["instanceIds"], ["Inst"])
            self.assertEqual(result["nodes"][0]["timestamps"], ["20260617120000"])
            self.assertEqual(
                result["nodes"][0]["logGroups"],
                {
                    "stream": {"fileCount": 1, "compressedFileCount": 0},
                    "tsdb": {"fileCount": 1, "compressedFileCount": 0},
                },
            )
            self.assertEqual(result["warnings"], [])
            self.assertTrue(
                any(
                    item["path"] == "tool_inputs/influxql_analyzer/NodeA/20260617120000.jsonl"
                    and item["toolIds"] == ["influxql_analyzer"]
                    for item in result["toolInputIndex"]
                )
            )
            finished = store.get_run(tool_run["id"])
            self.assertEqual(finished["status"], "succeeded")
            evidence = store.list_evidence(tool_run["id"])
            preprocess_evidence = next(
                item
                for item in evidence
                if item["payload"].get("toolId") == "logagent.preprocess_log_package"
            )
            self.assertFalse(preprocess_evidence["final_allowed"])
            self.assertEqual(preprocess_evidence["payload"]["toolInputCount"], 1)

    def test_pprof_tool_result_includes_v1_artifact_paths(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            fake_go = tmp_path / "fake-go"
            fake_go.write_text(
                """#!/usr/bin/env bash
set -euo pipefail
args=" $* "
if [[ "$args" == *" -top "* ]]; then
  [[ "$args" == *" -nodecount=20 "* ]] || { echo "missing top nodecount: $*" >&2; exit 3; }
  [[ "$args" == *" -symbolize=none "* ]] || { echo "missing top symbolize: $*" >&2; exit 3; }
  cat <<'OUT'
Type: samples
Showing nodes accounting for 10ms, 10% of 100ms total
10ms 10% 10% 20ms 20% github.com/acme/foo
OUT
elif [[ "$args" == *" -tree "* ]]; then
  [[ "$args" == *" -nodecount=20 "* ]] || { echo "missing tree nodecount: $*" >&2; exit 3; }
  [[ "$args" == *" -symbolize=none "* ]] || { echo "missing tree symbolize: $*" >&2; exit 3; }
  echo "tree output"
elif [[ "$args" == *" -raw "* ]]; then
  [[ "$args" != *" -nodecount=20 "* ]] || { echo "unexpected raw nodecount: $*" >&2; exit 3; }
  [[ "$args" == *" -symbolize=none "* ]] || { echo "missing raw symbolize: $*" >&2; exit 3; }
  echo "raw output"
elif [[ "$args" == *" -svg "* ]]; then
  [[ "$args" == *" -nodecount=20 "* ]] || { echo "missing svg nodecount: $*" >&2; exit 3; }
  [[ "$args" == *" -symbolize=none "* ]] || { echo "missing svg symbolize: $*" >&2; exit 3; }
  echo "<svg></svg>"
else
  echo "unexpected pprof args: $*" >&2
  exit 2
fi
""",
                encoding="utf-8",
            )
            fake_go.chmod(0o755)
            settings = Settings(
                data_dir=tmp_path / "data",
                api_key="test",
                pprof_enabled=True,
                pprof_go_command=str(fake_go),
            )
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            descriptors = {item["toolId"]: item for item in tool_descriptors(settings)}
            self.assertEqual(descriptors["pprof_analyzer"]["source"], "configured")
            self.assertEqual(descriptors["pprof_analyzer"]["exportable"], True)
            self.assertEqual(descriptors["pprof_analyzer"]["runnable"], True)
            self.assertEqual(descriptors["pprof_analyzer"]["paramsTemplate"]["nodeCount"], 50)
            self.assertEqual(
                descriptors["pprof_analyzer"]["paramsSchema"]["nodeCount"]["default"],
                50,
            )
            workspace = store.create_workspace("pprof run", "tool_run", "en-US")
            profile_artifact = write_artifact_bytes(
                settings,
                store,
                workspace["id"],
                "sample.pb.gz",
                b"profile",
                "application/octet-stream",
            )
            upload = store.create_upload(
                workspace["id"], "sample.pb.gz", profile_artifact["id"]
            )
            tool_run = store.create_tool_run(
                workspace_id=workspace["id"],
                tool_id="pprof_analyzer",
                params={"sampleIndex": "samples", "nodeCount": 20, "generateSvg": True},
                upload_ids=[upload["id"]],
            )

            executed = execute_tool_run(settings, store, tool_run["id"])
            result = executed["result"]
            action_id = result["actionId"]

            self.assertEqual(result["profileType"], "samples")
            self.assertEqual(result["total"], "100ms")
            self.assertEqual(result["top"][0]["function"], "github.com/acme/foo")
            self.assertEqual(
                result["artifactPaths"]["topTextPath"],
                f"tool_results/{action_id}/top.txt",
            )
            self.assertEqual(
                result["artifactPaths"]["treeTextPath"],
                f"tool_results/{action_id}/tree.txt",
            )
            self.assertEqual(
                result["artifactPaths"]["rawTextPath"],
                f"tool_results/{action_id}/raw.txt",
            )
            self.assertEqual(
                result["artifactPaths"]["svgPath"],
                f"tool_results/{action_id}/graph.svg",
            )
            self.assertEqual(
                result["artifactPaths"]["stderrPath"],
                f"tool_results/{action_id}/stderr.txt",
            )
            self.assertEqual(result["artifacts"], result["artifactIds"])
            self.assertIn("top", result["artifactIds"])
            self.assertIn("stderr", result["artifactIds"])

    def test_huawei_package_sync_result_matches_v1_shape(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            settings = Settings(
                data_dir=tmp_path / "data",
                api_key="test",
                huawei_package_sync=HuaweiPackageSyncSettings(
                    enabled=True,
                    obs_endpoint="https://obs.example.com",
                    obs_bucket="bucket-a",
                    obs_object_prefix="packages/demo",
                    obs_access_key="access",
                    obs_secret_key="secret",
                    gaussdb_dsn=(
                        "postgresql://dbuser:secret@gauss.example.com:5432/pkgdb"
                        "?sslmode=require"
                    ),
                    timeout_seconds=5,
                ),
            )
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace("huawei package sync", "tool_run", "en-US")
            package_artifact = write_artifact_bytes(
                settings,
                store,
                workspace["id"],
                "package.tar.gz",
                b"package-bytes",
                "application/gzip",
            )
            upload = store.create_upload(
                workspace["id"],
                "package.tar.gz",
                package_artifact["id"],
            )
            tool_run = store.create_tool_run(
                workspace_id=workspace["id"],
                tool_id="logagent.huawei_cloud_package_sync",
                params={
                    "objectKey": "",
                    "updateSql": " update packages set active = true ",
                    "querySql": " select id from packages ",
                },
                upload_ids=[upload["id"]],
            )

            calls = []

            def fake_obs_request(settings_arg, method, object_key, body):
                calls.append(("obs", method, object_key, len(body)))
                if method == "PUT":
                    return {
                        "statusCode": 200,
                        "etag": '"put-etag"',
                        "contentLength": len(body),
                        "durationMs": 7,
                    }
                return {
                    "statusCode": 200,
                    "etag": '"head-etag"',
                    "contentLength": 12,
                    "durationMs": 3,
                }

            def fake_execute_gaussdb_sql(dsn, sql, fetch):
                calls.append(("sql", sql, fetch))
                if fetch:
                    return {
                        "rowCount": 1,
                        "truncated": True,
                        "rows": [{"id": "pkg-1"}],
                        "durationMs": 11,
                    }
                return {"affectedRows": 2, "durationMs": 5}

            original_obs_request = tools_module.huawei_obs_request
            original_execute_sql = tools_module.execute_gaussdb_sql
            try:
                tools_module.huawei_obs_request = fake_obs_request
                tools_module.execute_gaussdb_sql = fake_execute_gaussdb_sql
                executed = execute_tool_run(settings, store, tool_run["id"])
            finally:
                tools_module.huawei_obs_request = original_obs_request
                tools_module.execute_gaussdb_sql = original_execute_sql

            result = executed["result"]
            action_id = result["actionId"]

            self.assertEqual(
                calls,
                [
                    ("obs", "PUT", "packages/demo/package.tar.gz", len(b"package-bytes")),
                    ("sql", "update packages set active = true", False),
                    ("obs", "HEAD", "packages/demo/package.tar.gz", 0),
                    ("sql", "select id from packages", True),
                ],
            )
            self.assertEqual(result["tool"], "logagent.huawei_cloud_package_sync")
            self.assertEqual(result["status"], "OK")
            self.assertEqual(
                result["summary"],
                "Uploaded package.tar.gz to OBS and queried GaussDB records",
            )
            self.assertEqual(result["objectKey"], "packages/demo/package.tar.gz")
            self.assertEqual(result["objectUrl"], "https://obs.example.com/bucket-a/packages/demo/package.tar.gz")
            self.assertEqual(result["input"]["uploadId"], upload["id"])
            self.assertEqual(result["input"]["filename"], "package.tar.gz")
            self.assertEqual(result["input"]["size"], len(b"package-bytes"))
            self.assertTrue(result["input"]["rawPath"].endswith("/package.tar.gz"))
            self.assertEqual(result["obs"]["endpoint"], "https://obs.example.com")
            self.assertEqual(result["obs"]["bucket"], "bucket-a")
            self.assertEqual(result["obs"]["put"]["etag"], '"put-etag"')
            self.assertEqual(result["obs"]["head"]["etag"], '"head-etag"')
            self.assertEqual(result["gaussdb"]["host"], "gauss.example.com")
            self.assertEqual(result["gaussdb"]["port"], 5432)
            self.assertEqual(result["gaussdb"]["database"], "pkgdb")
            self.assertEqual(result["gaussdb"]["user"], "dbuser")
            self.assertEqual(result["gaussdb"]["sslmode"], "require")
            self.assertEqual(result["gaussdb"]["updateAffectedRows"], 2)
            self.assertEqual(result["gaussdb"]["queryRowCount"], 1)
            self.assertEqual(result["gaussdb"]["queryRows"], [{"id": "pkg-1"}])
            self.assertTrue(result["gaussdb"]["queryRowsTruncated"])
            self.assertEqual(result["sql"]["updateSqlLength"], len("update packages set active = true"))
            self.assertEqual(result["sql"]["querySqlLength"], len("select id from packages"))
            self.assertEqual(result["timings"]["obsPutMs"], 7)
            self.assertEqual(result["timings"]["gaussdbUpdateMs"], 5)
            self.assertEqual(result["timings"]["obsHeadMs"], 3)
            self.assertEqual(result["timings"]["gaussdbQueryMs"], 11)
            self.assertIn("GaussDB query rows truncated", result["warnings"][0])
            self.assertEqual(
                result["evidenceRefs"],
                [f"tool_results/{action_id}/result.json"],
            )
            self.assertNotIn("secret", json.dumps(result))
            evidence = store.list_evidence(tool_run["id"])
            huawei_evidence = next(
                item
                for item in evidence
                if item["payload"].get("toolId") == "logagent.huawei_cloud_package_sync"
            )
            self.assertFalse(huawei_evidence["final_allowed"])

    def test_readonly_mcp_tools_catalog_matches_v1_shape(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tool = ToolDefinition(
                id="mock_tool",
                display_name="Mock Tool",
                command=sys.executable,
                args=("-c", "print('ok')"),
                timeout_seconds=7,
                max_input_files=3,
                match_file_patterns=("*.log",),
                match_keywords=("timeout",),
            )
            settings = Settings(data_dir=Path(tmp), api_key="test", tools=(tool,))
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()

            resource = readonly_mcp_response(
                settings,
                store,
                {
                    "jsonrpc": "2.0",
                    "id": 14,
                    "method": "resources/read",
                    "params": {"uri": "logagent://tools/catalog"},
                },
            )
            catalog = json.loads(resource["result"]["contents"][0]["text"])
            self.assertEqual(catalog["schemaVersion"], 1)
            self.assertIn("tools", catalog)
            self.assertIn("configuredTools", catalog)
            self.assertEqual(resource["result"]["contents"][0]["uri"], "logagent://tools/catalog")
            configured = {
                item["toolId"]: item for item in catalog["configuredTools"]
            }
            self.assertEqual(configured["mock_tool"]["configuredArgs"], ["-c", "print('ok')"])
            self.assertEqual(configured["mock_tool"]["timeoutSeconds"], 7)
            self.assertEqual(configured["mock_tool"]["maxInputFiles"], 3)
            self.assertEqual(configured["mock_tool"]["match"]["filePatterns"], ["*.log"])
            self.assertEqual(configured["mock_tool"]["match"]["keywords"], ["timeout"])
            self.assertEqual(configured["pprof_analyzer"]["enabled"], False)
            self.assertEqual(configured["pprof_analyzer"]["timeoutSeconds"], 60)
            self.assertEqual(configured["pprof_analyzer"]["configuredArgs"], [])
            tools = {item["toolId"]: item for item in catalog["tools"]}
            self.assertEqual(tools["mock_tool"]["source"], "configured")
            self.assertEqual(
                tools["mock_tool"]["paramsSchema"]["configuredArgs"]["value"],
                ["-c", "print('ok')"],
            )
            self.assertEqual(
                tools["mock_tool"]["paramsSchema"]["match"]["properties"]["keywords"]["value"],
                ["timeout"],
            )
            self.assertEqual(tools["pprof_analyzer"]["source"], "configured")
            self.assertEqual(tools["pprof_analyzer"]["manualOnly"], True)
            self.assertEqual(tools["logagent.fetch"]["source"], "built_in")

            tool_call = readonly_mcp_response(
                settings,
                store,
                {
                    "jsonrpc": "2.0",
                    "id": 15,
                    "method": "tools/call",
                    "params": {"name": "logagent.list_tools", "arguments": {}},
                },
            )
            called_catalog = json.loads(tool_call["result"]["content"][0]["text"])
            self.assertEqual(called_catalog["configuredTools"], catalog["configuredTools"])

    def test_manual_tool_run_executes_metadata_builtin(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(data_dir=Path(tmp), api_key="test")
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace("manual metadata", "diagnose", "en-US")
            import_metadata(
                store,
                instance_id="inst-manual",
                template_type="json",
                content=json.dumps(
                    {
                        "cluster": {
                            "clusterId": "cluster-manual",
                            "nodes": [{"nodeId": "n1", "host": "127.0.0.1"}],
                        }
                    }
                ),
                remark="manual tool run",
            )

            tool_run = store.create_tool_run(
                workspace_id=workspace["id"],
                tool_id="logagent.list_metadata_instances",
                params={},
            )

            self.assertEqual(tool_run["kind"], "tool_run")
            self.assertEqual(store.list_runs(workspace["id"]), [])
            self.assertEqual(store.list_tool_runs(tool_id="logagent.list_metadata_instances")[0]["id"], tool_run["id"])
            jobs = store.acquire_jobs("test-worker", limit=1)
            self.assertEqual(jobs[0]["kind"], "tool_run")

            executed = execute_tool_run(settings, store, tool_run["id"])
            store.complete_job(jobs[0]["id"])

            finished = store.get_run(tool_run["id"])
            self.assertEqual(finished["status"], "succeeded")
            self.assertEqual(finished["toolResultArtifactId"], executed["artifact"]["id"])
            result_path = resolve_artifact_path(settings, executed["artifact"]["relative_path"])
            result = json.loads(result_path.read_text(encoding="utf-8"))
            self.assertEqual(result["value"]["instances"][0]["instanceId"], "inst-manual")
            evidence = store.list_evidence(tool_run["id"])
            self.assertTrue(any(item["kind"] == "metadata_slice" for item in evidence))

            snapshot_run = store.create_tool_run(
                workspace_id=workspace["id"],
                tool_id="logagent.get_metadata_snapshot",
                params={"instanceId": "inst-manual"},
            )
            executed_snapshot = execute_tool_run(settings, store, snapshot_run["id"])
            snapshot_result_path = resolve_artifact_path(
                settings, executed_snapshot["artifact"]["relative_path"]
            )
            snapshot_result = json.loads(snapshot_result_path.read_text(encoding="utf-8"))
            self.assertEqual(
                snapshot_result["value"]["snapshot"]["instance"]["instanceId"],
                "inst-manual",
            )
            self.assertEqual(snapshot_result["value"]["instance"]["instanceId"], "inst-manual")

    def test_task_mcp_runs_configured_tool_with_params_schema(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            script = (
                "import json,sys;"
                "print(json.dumps({'summary':sys.argv[1]+' '+sys.argv[2],"
                "'findings':[{'message':sys.argv[3]}]}))"
            )
            tool = ToolDefinition(
                id="param_tool",
                display_name="Param Tool",
                command=sys.executable,
                args=(
                    "-c",
                    script,
                    "{params.mode}",
                    "{params.limit}",
                    "{params.enabled}",
                ),
                timeout_seconds=5,
                params_schema={
                    "type": "object",
                    "properties": {
                        "mode": {"type": "string", "enum": ["fast", "full"]},
                        "limit": {"type": "integer"},
                        "enabled": {"type": "boolean"},
                    },
                    "required": ["mode", "limit"],
                    "additionalProperties": False,
                },
            )
            settings = Settings(data_dir=Path(tmp), api_key="test", tools=(tool,))
            settings.ensure_dirs()
            descriptor = tool_descriptors(settings)[0]
            self.assertEqual(descriptor["paramsSchema"]["required"], ["mode", "limit"])
            self.assertEqual(
                descriptor["paramsSchema"]["configuredArgs"]["value"],
                [
                    "-c",
                    script,
                    "{params.mode}",
                    "{params.limit}",
                    "{params.enabled}",
                ],
            )

            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace("run param tool", "diagnose", "en-US")
            run = store.create_run(workspace["id"])

            response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 34,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.run_domain_tool",
                        "arguments": {
                            "toolId": "param_tool",
                            "params": {"mode": "fast", "limit": 3, "enabled": True},
                        },
                    },
                },
            )
            payload = json.loads(response["result"]["content"][0]["text"])
            self.assertEqual(payload["result"]["summary"], "fast 3")
            self.assertEqual(payload["result"]["findings"][0]["message"], "true")
            self.assertEqual(payload["result"]["params"]["limit"], 3)
            self.assertIn("params", payload["evidence"]["payload"])

            bad_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 35,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.run_domain_tool",
                        "arguments": {
                            "toolId": "param_tool",
                            "params": {"mode": "fast", "limit": 3, "extra": "no"},
                        },
                    },
                },
            )
            self.assertIn("does not accept params", bad_response["error"]["message"])

    def test_configured_tool_runs_in_materialized_workspace(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            script = (
                "import json,pathlib,sys;"
                "manifest=pathlib.Path(sys.argv[1]);"
                "grep=pathlib.Path(sys.argv[2]);"
                "workspace=pathlib.Path(sys.argv[3]);"
                "cwd=pathlib.Path.cwd();"
                "manifest_data=json.loads(manifest.read_text());"
                "grep_data=json.loads(grep.read_text());"
                "print(json.dumps({'summary':"
                "f\"cwd={cwd.resolve()==workspace.resolve()} "
                "manifest={manifest.exists()} grep={grep.exists()}\","
                "'findings':[{'message':json.dumps({"
                "'files':manifest_data['fileCount'],"
                "'matches':grep_data['totalMatches'],"
                "'workspace':str(workspace)"
                "})}]}))"
            )
            tool = ToolDefinition(
                id="workspace_tool",
                display_name="Workspace Tool",
                command=sys.executable,
                args=(
                    "-c",
                    script,
                    "{manifest_path}",
                    "{grep_results_path}",
                    "{workspace}",
                ),
                timeout_seconds=5,
            )
            settings = Settings(data_dir=Path(tmp), api_key="test", tools=(tool,))
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace("panic timeout", "diagnose", "en-US")
            artifact = write_artifact_bytes(
                settings,
                store,
                workspace["id"],
                "db.log",
                b"panic timeout\n",
                "text/plain",
            )
            store.create_upload(workspace["id"], "db.log", artifact["id"])
            run = store.create_run(workspace["id"])
            build_initial_evidence(settings, store, workspace["id"], run["id"])

            response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 36,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.run_domain_tool",
                        "arguments": {"toolId": "workspace_tool"},
                    },
                },
            )

            payload = json.loads(response["result"]["content"][0]["text"])
            result = payload["result"]
            self.assertEqual(result["summary"], "cwd=True manifest=True grep=True")
            finding = json.loads(result["findings"][0]["message"])
            self.assertEqual(finding["files"], 1)
            self.assertGreaterEqual(finding["matches"], 1)
            self.assertEqual(Path(result["argv"][3]).name, "manifest.json")
            self.assertEqual(Path(result["argv"][4]).name, "grep_results.json")
            self.assertEqual(Path(result["argv"][5]), Path(finding["workspace"]))
            self.assertEqual(Path(result["argv"][3]).parent, Path(result["argv"][5]))

    def test_configured_tool_rejects_unknown_placeholder(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tool = ToolDefinition(
                id="bad_placeholder_tool",
                display_name="Bad Placeholder Tool",
                command=sys.executable,
                args=("-c", "print('unexpected')", "{unknown}"),
                timeout_seconds=5,
            )
            settings = Settings(data_dir=Path(tmp), api_key="test", tools=(tool,))
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace("bad placeholder", "diagnose", "en-US")
            run = store.create_run(workspace["id"])

            response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 37,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.run_domain_tool",
                        "arguments": {"toolId": "bad_placeholder_tool"},
                    },
                },
            )

            self.assertIn(
                "unsupported tool argument placeholder",
                response["error"]["message"],
            )

    def test_configured_tool_result_uses_v1_record_shape(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tool = ToolDefinition(
                id="failing_tool",
                display_name="Failing Tool",
                command=sys.executable,
                args=("-c", "import sys; sys.exit(2)"),
                timeout_seconds=5,
            )
            settings = Settings(data_dir=Path(tmp), api_key="test", tools=(tool,))
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace("failing tool", "diagnose", "en-US")
            run = store.create_run(workspace["id"])

            response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 38,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.run_domain_tool",
                        "arguments": {"toolId": "failing_tool"},
                    },
                },
            )

            payload = json.loads(response["result"]["content"][0]["text"])
            result = payload["result"]
            self.assertEqual(result["schemaVersion"], 2)
            self.assertEqual(result["tool"], "failing_tool")
            self.assertEqual(result["toolId"], "failing_tool")
            self.assertEqual(result["status"], "FAILED")
            self.assertEqual(result["exitCode"], 2)
            self.assertIsInstance(result["durationMs"], int)
            self.assertEqual(result["command"], result["argv"])
            self.assertEqual(result["stdoutPath"], "tool_results/failing_tool/stdout.txt")
            self.assertEqual(result["stderrPath"], "tool_results/failing_tool/stderr.txt")
            self.assertEqual(
                result["summary"],
                "tool failing_tool exited with non-zero status",
            )
            self.assertIsNone(result["error"])

    def test_configured_tool_spawn_error_returns_failed_record(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tool = ToolDefinition(
                id="missing_binary_tool",
                display_name="Missing Binary Tool",
                command=str(Path(tmp) / "missing-tool"),
                args=(),
                timeout_seconds=5,
            )
            settings = Settings(data_dir=Path(tmp), api_key="test", tools=(tool,))
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace("missing tool", "diagnose", "en-US")
            run = store.create_run(workspace["id"])

            response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 39,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.run_domain_tool",
                        "arguments": {"toolId": "missing_binary_tool"},
                    },
                },
            )

            payload = json.loads(response["result"]["content"][0]["text"])
            result = payload["result"]
            self.assertEqual(result["schemaVersion"], 2)
            self.assertEqual(result["status"], "FAILED")
            self.assertIsNone(result["exitCode"])
            self.assertEqual(
                result["summary"],
                "tool missing_binary_tool could not be started",
            )
            self.assertIn("missing-tool", result["error"])
            self.assertIn("missing-tool", result["stderrPreview"])

    def test_configured_tool_timeout_returns_timed_out_record(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tool = ToolDefinition(
                id="slow_tool",
                display_name="Slow Tool",
                command=sys.executable,
                args=("-c", "import time; time.sleep(2)"),
                timeout_seconds=1,
            )
            settings = Settings(data_dir=Path(tmp), api_key="test", tools=(tool,))
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace("slow tool", "diagnose", "en-US")
            run = store.create_run(workspace["id"])

            response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 40,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.run_domain_tool",
                        "arguments": {"toolId": "slow_tool"},
                    },
                },
            )

            payload = json.loads(response["result"]["content"][0]["text"])
            result = payload["result"]
            self.assertEqual(result["schemaVersion"], 2)
            self.assertEqual(result["status"], "TIMED_OUT")
            self.assertTrue(result["timedOut"])
            self.assertIsNone(result["exitCode"])
            self.assertEqual(result["summary"], "tool slow_tool timed out after 1 seconds")
            self.assertEqual(result["error"], "tool timed out")
            self.assertIn("tool timed out", result["stderrPreview"])

    def test_materialized_tool_inputs_feed_configured_tool(self) -> None:
        def add_file(archive: tarfile.TarFile, name: str, data: bytes) -> None:
            info = tarfile.TarInfo(name)
            info.size = len(data)
            archive.addfile(info, io.BytesIO(data))

        with tempfile.TemporaryDirectory() as tmp:
            script = (
                "import json,pathlib,sys;"
                "lines=pathlib.Path(sys.argv[1]).read_text().splitlines();"
                "print(json.dumps({'summary':'rows='+str(len(lines)),"
                "'findings':[{'message':lines[0]}]}))"
            )
            tool = ToolDefinition(
                id="influxql_analyzer",
                display_name="InfluxQL Analyzer",
                command=sys.executable,
                args=("-c", script, "{input_file}"),
                timeout_seconds=5,
            )
            settings = Settings(data_dir=Path(tmp), api_key="test", tools=(tool,))
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace("slow query", "diagnose", "en-US")

            tar_path = Path(tmp) / "Pkg_Inst_NodeB_20260617130000_logs.tar.gz"
            with tarfile.open(tar_path, "w:gz") as archive:
                add_file(
                    archive,
                    "wrapper/var/chroot/gemini/log/tsdb/query.log",
                    b'{"query":"select * from cpu"}\nplain text\n',
                )
            artifact = write_artifact_bytes(
                settings,
                store,
                workspace["id"],
                tar_path.name,
                tar_path.read_bytes(),
                "application/gzip",
            )
            store.create_upload(workspace["id"], tar_path.name, artifact["id"])
            run = store.create_run(workspace["id"])

            AgentRuntime(settings, store).run_analysis(workspace["id"], run["id"])
            manifest_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 30,
                    "method": "resources/read",
                    "params": {"uri": f"logagent-v2://run/{run['id']}/manifest"},
                },
            )
            manifest = json.loads(manifest_response["result"]["contents"][0]["text"])
            self.assertEqual(manifest["toolInputsPath"], "tool_inputs/index.json")
            self.assertEqual(manifest["toolInputCount"], 1)

            index_evidence = next(
                item
                for item in store.list_evidence(run["id"])
                if item["kind"] == "tool_input_index"
            )
            index_artifact = store.get_artifact(index_evidence["artifact_id"])
            index_path = resolve_artifact_path(settings, index_artifact["relative_path"])
            index = json.loads(index_path.read_text(encoding="utf-8"))
            entry = index["inputs"][0]
            self.assertEqual(entry["toolIds"], ["influxql_analyzer"])
            self.assertEqual(entry["recordCount"], 1)
            self.assertTrue(entry["path"].startswith("tool_inputs/influxql_analyzer/NodeB/"))

            tool_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 31,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.run_domain_tool",
                        "arguments": {"toolId": "influxql_analyzer"},
                    },
                },
            )
            payload = json.loads(tool_response["result"]["content"][0]["text"])
            self.assertEqual(payload["result"]["summary"], "rows=1")
            self.assertEqual(payload["result"]["inputFile"], entry["path"])
            self.assertIn("select * from cpu", payload["result"]["findings"][0]["message"])

            ref = payload["evidence"]["payload"]["evidenceRefPrefix"] + "0"
            answer = {
                "summary": "Tool-backed answer.",
                "symptoms": [],
                "likelyRootCauses": [{"cause": "slow query", "evidenceRefs": [ref]}],
                "nextChecks": [],
                "fixSuggestions": [],
                "missingInformation": [],
                "confidence": "medium",
                "evidenceRefs": [ref],
            }
            validated = normalize_and_validate_final_answer(
                settings, store, run["id"], answer
            )
            self.assertEqual(validated["evidenceRefs"], [ref])

    def test_generic_influxql_and_flux_tool_inputs_feed_tools(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            script = (
                "import json,pathlib,sys;"
                "lines=pathlib.Path(sys.argv[1]).read_text().splitlines();"
                "print(json.dumps({'summary':sys.argv[2]+' rows='+str(len(lines)),"
                "'findings':[{'message':json.loads(lines[0])['query']}]}))"
            )
            influx_tool = ToolDefinition(
                id="influxql_analyzer",
                display_name="InfluxQL Analyzer",
                command=sys.executable,
                args=("-c", script, "{input_file}", "influx"),
                timeout_seconds=5,
            )
            flux_tool = ToolDefinition(
                id="flux_query_analyzer",
                display_name="Flux Query Analyzer",
                command=sys.executable,
                args=("-c", script, "{input_file}", "flux"),
                timeout_seconds=5,
            )
            settings = Settings(
                data_dir=Path(tmp),
                api_key="test",
                tools=(influx_tool, flux_tool),
            )
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace("analyze query templates", "diagnose", "en-US")
            content = (
                "{\"query\":\"select * from cpu where host = 'a'\"}\n"
                '{"flux":"from(bucket: \\"metrics\\") |> range(start: -1h)"}\n'
            ).encode("utf-8")
            artifact = write_artifact_bytes(
                settings,
                store,
                workspace["id"],
                "queries.log",
                content,
                "text/plain",
            )
            store.create_upload(workspace["id"], "queries.log", artifact["id"])
            run = store.create_run(workspace["id"])
            AgentRuntime(settings, store).run_analysis(workspace["id"], run["id"])

            index_evidence = next(
                item
                for item in store.list_evidence(run["id"])
                if item["kind"] == "tool_input_index"
            )
            index_path = resolve_artifact_path(
                settings,
                store.get_artifact(index_evidence["artifact_id"])["relative_path"],
            )
            index = json.loads(index_path.read_text(encoding="utf-8"))
            entries = {entry["toolIds"][0]: entry for entry in index["inputs"]}
            self.assertEqual(entries["influxql_analyzer"]["inputKind"], "influxql_jsonl")
            self.assertEqual(entries["flux_query_analyzer"]["inputKind"], "flux_query_jsonl")
            self.assertEqual(entries["flux_query_analyzer"]["scope"], "file")

            influx_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 32,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.run_domain_tool",
                        "arguments": {"toolId": "influxql_analyzer"},
                    },
                },
            )
            influx_payload = json.loads(influx_response["result"]["content"][0]["text"])
            self.assertEqual(influx_payload["result"]["summary"], "influx rows=1")
            self.assertIn("select * from cpu", influx_payload["result"]["findings"][0]["message"])

            flux_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 33,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.run_domain_tool",
                        "arguments": {"toolId": "flux_query_analyzer"},
                    },
                },
            )
            flux_payload = json.loads(flux_response["result"]["content"][0]["text"])
            self.assertEqual(flux_payload["result"]["summary"], "flux rows=1")
            self.assertIn("from(bucket:", flux_payload["result"]["findings"][0]["message"])

    def test_archive_storage_tool_inputs_feed_storage_analyzer(self) -> None:
        def add_file(archive: tarfile.TarFile, name: str, data: bytes) -> None:
            info = tarfile.TarInfo(name)
            info.size = len(data)
            archive.addfile(info, io.BytesIO(data))

        with tempfile.TemporaryDirectory() as tmp:
            script = (
                "import json,pathlib,sys;"
                "p=pathlib.Path(sys.argv[1]);"
                "print(json.dumps({'summary':'bytes='+str(p.stat().st_size),"
                "'findings':[{'message':p.name}]}))"
            )
            tool = ToolDefinition(
                id="opengemini_storage_analyzer",
                display_name="openGemini Storage Analyzer",
                command=sys.executable,
                args=("-c", script, "{input_file}"),
                timeout_seconds=5,
                max_input_files=3,
            )
            settings = Settings(data_dir=Path(tmp), api_key="test", tools=(tool,))
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace("storage file issue", "diagnose", "en-US")

            tar_path = Path(tmp) / "storage.tar.gz"
            with tarfile.open(tar_path, "w:gz") as archive:
                add_file(archive, "data/shard/0001.tssp", b"TSSP\x00payload")
                add_file(archive, "logs/app.log", b"INFO boot\n")
            artifact = write_artifact_bytes(
                settings,
                store,
                workspace["id"],
                tar_path.name,
                tar_path.read_bytes(),
                "application/gzip",
            )
            store.create_upload(workspace["id"], tar_path.name, artifact["id"])
            run = store.create_run(workspace["id"])
            AgentRuntime(settings, store).run_analysis(workspace["id"], run["id"])

            index_evidence = next(
                item
                for item in store.list_evidence(run["id"])
                if item["kind"] == "tool_input_index"
            )
            index_path = resolve_artifact_path(
                settings,
                store.get_artifact(index_evidence["artifact_id"])["relative_path"],
            )
            index = json.loads(index_path.read_text(encoding="utf-8"))
            storage_entry = next(
                item
                for item in index["inputs"]
                if item["toolIds"] == ["opengemini_storage_analyzer"]
            )
            self.assertEqual(storage_entry["inputKind"], "opengemini_storage_file")
            self.assertEqual(storage_entry["scope"], "archive")
            self.assertEqual(storage_entry["sourceArchivePath"], "data/shard/0001.tssp")
            self.assertTrue(storage_entry["path"].startswith("tool_inputs/storage/"))

            response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 37,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.run_domain_tool",
                        "arguments": {"toolId": "opengemini_storage_analyzer"},
                    },
                },
            )
            payload = json.loads(response["result"]["content"][0]["text"])
            self.assertEqual(payload["result"]["summary"], "bytes=12")
            self.assertEqual(payload["result"]["inputFile"], storage_entry["path"])
            self.assertIn("storage_", payload["result"]["findings"][0]["message"])

    def test_archive_series_directory_tool_input_feeds_storage_analyzer(self) -> None:
        def add_file(archive: tarfile.TarFile, name: str, data: bytes) -> None:
            info = tarfile.TarInfo(name)
            info.size = len(data)
            archive.addfile(info, io.BytesIO(data))

        with tempfile.TemporaryDirectory() as tmp:
            script = (
                "import json,pathlib,sys;"
                "p=pathlib.Path(sys.argv[1]);"
                "files=sorted(x.relative_to(p).as_posix() for x in p.rglob('*') if x.is_file());"
                "print(json.dumps({'summary':'files='+str(len(files)),"
                "'findings':[{'message':files[0]}]}))"
            )
            tool = ToolDefinition(
                id="influxdb_storage_analyzer",
                display_name="InfluxDB Storage Analyzer",
                command=sys.executable,
                args=("-c", script, "{input_file}"),
                timeout_seconds=5,
                max_input_files=3,
            )
            settings = Settings(data_dir=Path(tmp), api_key="test", tools=(tool,))
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace("series index issue", "diagnose", "en-US")

            tar_path = Path(tmp) / "series.tar.gz"
            with tarfile.open(tar_path, "w:gz") as archive:
                add_file(archive, "engine/db/rp/_series/00/0000", b"series-a")
                add_file(archive, "engine/db/rp/_series/01/0001", b"series-b")
            artifact = write_artifact_bytes(
                settings,
                store,
                workspace["id"],
                tar_path.name,
                tar_path.read_bytes(),
                "application/gzip",
            )
            store.create_upload(workspace["id"], tar_path.name, artifact["id"])
            run = store.create_run(workspace["id"])
            AgentRuntime(settings, store).run_analysis(workspace["id"], run["id"])

            index_evidence = next(
                item
                for item in store.list_evidence(run["id"])
                if item["kind"] == "tool_input_index"
            )
            index_path = resolve_artifact_path(
                settings,
                store.get_artifact(index_evidence["artifact_id"])["relative_path"],
            )
            index = json.loads(index_path.read_text(encoding="utf-8"))
            directory_entry = next(
                item
                for item in index["inputs"]
                if item["toolIds"] == ["influxdb_storage_analyzer"]
            )
            self.assertEqual(directory_entry["inputKind"], "influxdb_storage_directory")
            self.assertEqual(directory_entry["scope"], "archive_directory")
            self.assertEqual(directory_entry["sourceArchiveRoot"], "engine/db/rp/_series")
            self.assertEqual(directory_entry["fileCount"], 2)

            directory_artifact = store.get_artifact(directory_entry["artifactId"])
            directory_path = resolve_artifact_path(settings, directory_artifact["relative_path"])
            self.assertTrue(directory_path.is_dir())
            self.assertTrue((directory_path / "00" / "0000").is_file())

            response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 38,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.run_domain_tool",
                        "arguments": {"toolId": "influxdb_storage_analyzer"},
                    },
                },
            )
            payload = json.loads(response["result"]["content"][0]["text"])
            self.assertEqual(payload["result"]["summary"], "files=2")
            self.assertEqual(payload["result"]["inputFile"], directory_entry["path"])
            self.assertEqual(payload["result"]["inputKind"], "influxdb_storage_directory")
            self.assertEqual(payload["result"]["findings"][0]["message"], "00/0000")

    def test_archive_tsi_directory_tool_input_feeds_opengemini_storage_analyzer(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            script = (
                "import json,pathlib,sys;"
                "p=pathlib.Path(sys.argv[1]);"
                "files=sorted(x.relative_to(p).as_posix() for x in p.rglob('*') if x.is_file());"
                "print(json.dumps({'summary':'files='+str(len(files)),"
                "'findings':[{'message':files[-1]}]}))"
            )
            tool = ToolDefinition(
                id="opengemini_storage_analyzer",
                display_name="openGemini Storage Analyzer",
                command=sys.executable,
                args=("-c", script, "{input_file}"),
                timeout_seconds=5,
                max_input_files=3,
            )
            settings = Settings(data_dir=Path(tmp), api_key="test", tools=(tool,))
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace("tsi index issue", "diagnose", "en-US")

            zip_path = Path(tmp) / "tsi.zip"
            with zipfile.ZipFile(zip_path, "w") as archive:
                archive.writestr("index/tsi/part/metaindex.bin", b"meta")
                archive.writestr("index/tsi/part/items.bin", b"items")
            artifact = write_artifact_bytes(
                settings,
                store,
                workspace["id"],
                zip_path.name,
                zip_path.read_bytes(),
                "application/zip",
            )
            store.create_upload(workspace["id"], zip_path.name, artifact["id"])
            run = store.create_run(workspace["id"])
            AgentRuntime(settings, store).run_analysis(workspace["id"], run["id"])

            index_evidence = next(
                item
                for item in store.list_evidence(run["id"])
                if item["kind"] == "tool_input_index"
            )
            index_path = resolve_artifact_path(
                settings,
                store.get_artifact(index_evidence["artifact_id"])["relative_path"],
            )
            index = json.loads(index_path.read_text(encoding="utf-8"))
            directory_entry = next(
                item
                for item in index["inputs"]
                if item["toolIds"] == ["opengemini_storage_analyzer"]
            )
            self.assertEqual(directory_entry["inputKind"], "opengemini_storage_directory")
            self.assertEqual(directory_entry["scope"], "archive_directory")
            self.assertEqual(directory_entry["sourceArchiveRoot"], "index/tsi")
            self.assertEqual(directory_entry["fileCount"], 2)

            response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 39,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.run_domain_tool",
                        "arguments": {"toolId": "opengemini_storage_analyzer"},
                    },
                },
            )
            payload = json.loads(response["result"]["content"][0]["text"])
            self.assertEqual(payload["result"]["summary"], "files=2")
            self.assertEqual(payload["result"]["inputFile"], directory_entry["path"])
            self.assertEqual(payload["result"]["inputKind"], "opengemini_storage_directory")
            self.assertEqual(payload["result"]["findings"][0]["message"], "part/metaindex.bin")

    def test_tool_runner_falls_back_to_manifest_and_grep_inputs(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            script = (
                "import json,pathlib,sys;"
                "p=pathlib.Path(sys.argv[1]);"
                "first=p.read_text().splitlines()[0];"
                "print(json.dumps({'summary':first,'findings':[{'message':p.name}]}))"
            )
            tool = ToolDefinition(
                id="fallback_tool",
                display_name="Fallback Tool",
                command=sys.executable,
                args=("-c", script, "{input_file}"),
                timeout_seconds=5,
                max_input_files=2,
                match_file_patterns=("*.log",),
                match_keywords=("select",),
            )
            settings = Settings(data_dir=Path(tmp), api_key="test", tools=(tool,))
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace("select query issue", "diagnose", "en-US")

            zip_path = Path(tmp) / "queries.zip"
            with zipfile.ZipFile(zip_path, "w") as archive:
                archive.writestr("queries/one.log", "plain log line\n")
                archive.writestr("queries/two.txt", "select * from cpu\n")
                archive.writestr("queries/three.txt", "select * from mem\n")
            artifact = write_artifact_bytes(
                settings,
                store,
                workspace["id"],
                zip_path.name,
                zip_path.read_bytes(),
                "application/zip",
            )
            store.create_upload(workspace["id"], zip_path.name, artifact["id"])
            run = store.create_run(workspace["id"])
            AgentRuntime(settings, store).run_analysis(workspace["id"], run["id"])

            tool_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 32,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.run_domain_tool",
                        "arguments": {"toolId": "fallback_tool"},
                    },
                },
            )
            payload = json.loads(tool_response["result"]["content"][0]["text"])
            inputs = [item["inputFile"] for item in payload["results"]]
            self.assertEqual(
                inputs,
                ["extracted/queries/one.log", "extracted/queries/two.txt"],
            )
            summaries = [item["summary"] for item in payload["results"]]
            self.assertEqual(summaries, ["plain log line", "select * from cpu"])
            self.assertEqual(len(payload["evidenceItems"]), 2)
            self.assertEqual(
                payload["artifactPaths"],
                [
                    f"tool_results/{payload['results'][0]['actionId']}/result.json",
                    f"tool_results/{payload['results'][1]['actionId']}/result.json",
                ],
            )
            self.assertEqual(payload["artifactPath"], payload["artifactPaths"][0])
            self.assertEqual(payload["evidenceRefs"], payload["artifactPaths"])
            evidence = [
                item
                for item in store.list_evidence(run["id"])
                if item["kind"] == "tool_result"
            ]
            self.assertEqual(len(evidence), 2)
            self.assertNotEqual(
                evidence[0]["payload"]["actionId"], evidence[1]["payload"]["actionId"]
            )

    def test_task_mcp_run_domain_tool_accepts_legacy_tool_input_file(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            script = (
                "import json,pathlib,sys;"
                "p=pathlib.Path(sys.argv[1]);"
                "print(json.dumps({'summary':p.read_text().splitlines()[0],"
                "'findings':[{'message':p.name}]}))"
            )
            tool = ToolDefinition(
                id="explicit_tool",
                display_name="Explicit Tool",
                command=sys.executable,
                args=("-c", script, "{input_file}"),
                timeout_seconds=5,
            )
            settings = Settings(data_dir=Path(tmp), api_key="test", tools=(tool,))
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace("explicit input", "diagnose", "en-US")
            artifact = write_artifact_bytes(
                settings,
                store,
                workspace["id"],
                "query.log",
                b"explicit line\n",
                "text/plain",
            )
            store.create_upload(workspace["id"], "query.log", artifact["id"])
            run = store.create_run(workspace["id"])
            AgentRuntime(settings, store).run_analysis(workspace["id"], run["id"])

            response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 36,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.run_domain_tool",
                        "arguments": {"tool": "explicit_tool", "inputFile": "query.log"},
                    },
                },
            )
            payload = json.loads(response["result"]["content"][0]["text"])
            self.assertEqual(payload["result"]["summary"], "explicit line")
            self.assertEqual(payload["result"]["inputFile"], "extracted/query.log")
            self.assertIn("explicit_tool_", payload["result"]["findings"][0]["message"])

    def test_manual_configured_tool_run_uses_explicit_input_files(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            script = (
                "import json,pathlib,sys;"
                "p=pathlib.Path(sys.argv[1]);"
                "print(json.dumps({'summary':p.read_text().splitlines()[0],"
                "'findings':[{'message':p.name}]}))"
            )
            tool = ToolDefinition(
                id="manual_explicit_tool",
                display_name="Manual Explicit Tool",
                command=sys.executable,
                args=("-c", script, "{input_file}"),
                timeout_seconds=5,
            )
            settings = Settings(data_dir=Path(tmp), api_key="test", tools=(tool,))
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace("manual explicit input", "diagnose", "en-US")
            artifact = write_artifact_bytes(
                settings,
                store,
                workspace["id"],
                "manual.log",
                b"manual line\n",
                "text/plain",
            )
            store.create_upload(workspace["id"], "manual.log", artifact["id"])
            descriptor = tool_descriptors(settings)[0]
            self.assertEqual(descriptor["paramsTemplate"], {"inputFiles": []})
            self.assertIn("inputFiles", descriptor["paramsSchema"]["properties"])

            params = validate_manual_tool_run(
                settings,
                "manual_explicit_tool",
                upload_count=0,
                params={"inputFiles": ["manual.log"]},
            )
            with self.assertRaises(ValueError):
                validate_manual_tool_run(
                    settings,
                    "manual_explicit_tool",
                    upload_count=0,
                    params={"inputFiles": ["../manual.log"]},
                )
            tool_run = store.create_tool_run(
                workspace_id=workspace["id"],
                tool_id="manual_explicit_tool",
                params=params,
            )
            executed = execute_tool_run(settings, store, tool_run["id"])

            self.assertEqual(executed["result"]["summary"], "manual line")
            self.assertEqual(executed["result"]["inputFile"], "extracted/manual.log")
            finished = store.get_run(tool_run["id"])
            self.assertEqual(finished["status"], "succeeded")

    def test_tool_stdout_parses_influxql_report(self) -> None:
        parsed = parse_json(
            b"""{
  "total_records": 2,
  "records_in_window": 2,
  "total_statements": 2,
  "parse_error_count": 1,
  "fingerprints": [
    {
      "statement_type": "SELECT",
      "normalized_query": "SELECT * FROM cpu LIMIT 1",
      "count": 1,
      "rules": ["large_limit", "no_time_filter"]
    }
  ],
  "special_rules": [
    {"rule": "large_limit", "count": 1, "fingerprints": ["fp1"]},
    {"rule": "no_time_filter", "count": 1, "fingerprints": ["fp1"]}
  ],
  "parse_errors": [
    {"error": "found BAD, expected SELECT", "count": 1, "sample_queries": ["BAD"]}
  ],
  "realtime_query": {
    "total": 1,
    "realtime": 0,
    "non_realtime": 0,
    "unknown": 1,
    "sample_unknown": [{"reason": "query has no where time predicate"}]
  }
}"""
        )

        summary = summary_from_stdout(parsed, b"", False)
        findings = findings_from_stdout(parsed)
        self.assertIn("records=2", summary)
        self.assertIn("specialRules=large_limit:1, no_time_filter:1", summary)
        self.assertTrue(
            any(
                finding.get("severity") == "high"
                and "rule large_limit" in finding["message"]
                for finding in findings
            )
        )
        self.assertTrue(
            any("parse error occurred 1 time" in finding["message"] for finding in findings)
        )
        self.assertTrue(
            any(
                "realtime query classification is unknown" in finding["message"]
                for finding in findings
            )
        )
        self.assertTrue(
            any("fingerprint SELECT occurred 1 time" in finding["message"] for finding in findings)
        )

    def test_tool_stdout_detects_influxql_report_with_null_fingerprints(self) -> None:
        parsed = parse_json(
            b"""{
  "total_records": 2,
  "records_in_window": 1,
  "total_statements": 2,
  "parse_error_count": 0,
  "fingerprints": null
}"""
        )

        self.assertEqual(
            summary_from_stdout(parsed, b"", False),
            (
                "influxql report: records=2, recordsInWindow=1, "
                "statements=2, parseErrors=0"
            ),
        )
        self.assertEqual(findings_from_stdout(parsed), [])

    def test_tool_stdout_parses_influxql_compare_report(self) -> None:
        parsed = parse_json(
            b"""{
  "batch_a": {"total_statements": 10},
  "batch_b": {"total_statements": 14, "qps": 2.5, "effective_duration_seconds": 5},
  "statement_delta": 4,
  "qps_delta": 0.5,
  "new_fingerprints": [
    {
      "fingerprint": "fp-new",
      "statement_type": "SELECT",
      "normalized_query": "SELECT * FROM cpu",
      "status": "new",
      "count_a": 0,
      "count_b": 4,
      "count_delta": 4,
      "qps_a": 0,
      "qps_b": 0.5,
      "qps_delta": 0.5,
      "rules": ["no_time_filter"]
    }
  ],
  "removed_fingerprints": [],
  "changed_fingerprints": [],
  "rule_deltas": [
    {
      "rule": "large_limit",
      "count_a": 1,
      "count_b": 3,
      "count_delta": 2,
      "qps_a": 0.1,
      "qps_b": 0.3,
      "qps_delta": 0.2
    }
  ]
}"""
        )

        summary = summary_from_stdout(parsed, b"", False)
        findings = findings_from_stdout(parsed)
        self.assertIn("statementDelta=4", summary)
        self.assertIn("batchB=statements=14", summary)
        self.assertTrue(
            any(
                finding.get("severity") == "high"
                and "count=0->4" in finding["message"]
                and "rules=no_time_filter" in finding["message"]
                for finding in findings
            )
        )
        self.assertTrue(
            any(
                "rule=large_limit" in finding["message"]
                and "qps=0.1->0.3" in finding["message"]
                for finding in findings
            )
        )

    def test_final_answer_evidence_refs_are_validated(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tool = ToolDefinition(
                id="mock_tool",
                display_name="Mock Tool",
                command=sys.executable,
                args=(
                    "-c",
                    "import json; print(json.dumps({'summary':'mock ok','findings':[{'message':'hit'}]}))",
                ),
                timeout_seconds=5,
            )
            settings = Settings(data_dir=Path(tmp), api_key="test", tools=(tool,))
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace("why did timeout happen?", "diagnose", "en-US")
            artifact = write_artifact_bytes(
                settings,
                store,
                workspace["id"],
                "query.log",
                b"query timeout on shard 1\nnormal line\ncache warning\n",
                "text/plain",
            )
            store.create_upload(workspace["id"], "query.log", artifact["id"])
            run = store.create_run(workspace["id"])
            AgentRuntime(settings, store).run_analysis(workspace["id"], run["id"])

            search_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 8,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.search_logs",
                        "arguments": {"keywords": ["cache"]},
                    },
                },
            )
            search_payload = json.loads(search_response["result"]["content"][0]["text"])
            follow_up_ref = search_payload["search"]["matches"][0]["ref"]

            slice_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 9,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.get_log_slice",
                        "arguments": {"path": "query.log", "lineNumber": 3},
                    },
                },
            )
            slice_payload = json.loads(slice_response["result"]["content"][0]["text"])
            slice_ref = slice_payload["slice"]["ref"]

            tool_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 10,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.run_domain_tool",
                        "arguments": {"toolId": "mock_tool"},
                    },
                },
            )
            self.assertIn("result", tool_response)
            tool_ref = "tool_results/mock_tool/result.json#findings/0"

            refs = [
                "grep_results.json#matches/0",
                follow_up_ref,
                slice_ref,
                tool_ref,
            ]
            answer = {
                "summary": "Validated answer.",
                "symptoms": "timeout symptom",
                "likelyRootCauses": [{"cause": "Evidence-backed cause", "evidenceRefs": refs}],
                "nextChecks": [],
                "fixSuggestions": [],
                "missingInformation": [],
                "confidence": "medium",
                "evidenceRefs": refs,
            }
            validated = normalize_and_validate_final_answer(
                settings, store, run["id"], answer
            )
            self.assertEqual(validated["symptoms"], ["timeout symptom"])

            bad_index = dict(answer, evidenceRefs=["grep_results.json#matches/999"])
            with self.assertRaises(FinalAnswerValidationError):
                normalize_and_validate_final_answer(settings, store, run["id"], bad_index)

            background_ref = dict(answer, evidenceRefs=["manifest.json#files/0"])
            with self.assertRaises(FinalAnswerValidationError):
                normalize_and_validate_final_answer(settings, store, run["id"], background_ref)

    def test_fetch_endpoint_runs_through_task_mcp_and_final_refs(self) -> None:
        class FetchHandler(BaseHTTPRequestHandler):
            def do_GET(self) -> None:
                body = json.dumps({"ok": True, "path": self.path}).encode("utf-8")
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("X-Api-Key", "response-secret")
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, format: str, *args: object) -> None:
                return

        server = HTTPServer(("127.0.0.1", 0), FetchHandler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            with tempfile.TemporaryDirectory() as tmp:
                settings = Settings(
                    data_dir=Path(tmp),
                    api_key="test",
                    fetch_enabled=True,
                    fetch_allowed_hosts=("127.0.0.1",),
                )
                settings.ensure_dirs()
                store = Store(settings.sqlite_path)
                store.initialize()
                endpoint = store.create_fetch_endpoint(
                    name="local metadata",
                    method="GET",
                    url=f"http://127.0.0.1:{server.server_port}/metadata?api_key=secret",
                    headers={"Authorization": "Bearer secret", "X-Trace": "visible"},
                    body="password=secret&keep=value",
                    enabled=True,
                )
                workspace = store.create_workspace("fetch metadata", "diagnose", "en-US")
                run = store.create_run(workspace["id"])

                list_response = task_mcp_response(
                    settings,
                    store,
                    run["id"],
                    {
                        "jsonrpc": "2.0",
                        "id": 20,
                        "method": "tools/call",
                        "params": {"name": "logagent.list_fetch_endpoints", "arguments": {}},
                    },
                )
                listed = json.loads(list_response["result"]["content"][0]["text"])
                self.assertEqual(listed["schemaVersion"], 1)
                self.assertTrue(listed["enabled"])
                self.assertFalse(listed["finalEvidenceAllowed"])
                self.assertEqual(listed["endpoints"][0]["fetchId"], endpoint["id"])
                self.assertEqual(listed["endpoints"][0]["method"], "GET")
                self.assertEqual(listed["endpoints"][0]["description"], "")
                self.assertEqual(listed["endpoints"][0]["tags"], [])
                self.assertIn(
                    "api_key=__REDACTED__", listed["endpoints"][0]["urlTemplate"]
                )
                self.assertIsNone(listed["endpoints"][0]["credentialVersion"])
                self.assertEqual(
                    listed["endpoints"][0]["headers"]["Authorization"], "__REDACTED__"
                )
                self.assertIn("api_key=__REDACTED__", listed["endpoints"][0]["url"])
                self.assertEqual(
                    listed["endpoints"][0]["bodyPreview"],
                    "password=__REDACTED__&keep=value",
                )
                disabled_list_response = task_mcp_response(
                    Settings(data_dir=Path(tmp), api_key="test", fetch_enabled=False),
                    store,
                    run["id"],
                    {
                        "jsonrpc": "2.0",
                        "id": 200,
                        "method": "tools/call",
                        "params": {"name": "logagent.list_fetch_endpoints", "arguments": {}},
                    },
                )
                self.assertIn(
                    "fetch is disabled by configuration",
                    disabled_list_response["error"]["message"],
                )

                fetch_response = task_mcp_response(
                    settings,
                    store,
                    run["id"],
                    {
                        "jsonrpc": "2.0",
                        "id": 21,
                        "method": "tools/call",
                        "params": {
                            "name": "logagent.fetch",
                            "arguments": {"endpointId": endpoint["id"]},
                        },
                    },
                )
                payload = json.loads(fetch_response["result"]["content"][0]["text"])
                self.assertEqual(payload["result"]["response"]["statusCode"], 200)
                self.assertEqual(
                    payload["result"]["response"]["headers"]["X-Api-Key"], "__REDACTED__"
                )
                self.assertIn("api_key=__REDACTED__", payload["result"]["request"]["url"])
                self.assertEqual(
                    payload["result"]["request"]["bodyPreview"],
                    "password=__REDACTED__&keep=value",
                )
                self.assertEqual(payload["evidence"]["kind"], "fetch_result")
                self.assertTrue(payload["evidence"]["final_allowed"])

                ref = payload["result"]["evidenceRef"]
                answer = {
                    "summary": "Fetch-backed answer.",
                    "symptoms": [],
                    "likelyRootCauses": [
                        {"cause": "metadata confirms state", "evidenceRefs": [ref]}
                    ],
                    "nextChecks": [],
                    "fixSuggestions": [],
                    "missingInformation": [],
                    "confidence": "medium",
                    "evidenceRefs": [ref],
                }
                validated = normalize_and_validate_final_answer(
                    settings, store, run["id"], answer
                )
                self.assertEqual(validated["evidenceRefs"], [ref])

                readonly_fetch = readonly_mcp_response(
                    settings,
                    store,
                    {
                        "jsonrpc": "2.0",
                        "id": 22,
                        "method": "tools/call",
                        "params": {
                            "name": "logagent.fetch",
                            "arguments": {"endpointId": endpoint["id"]},
                        },
                    },
                )
                self.assertIn("error", readonly_fetch)

                blocked_settings = Settings(
                    data_dir=Path(tmp),
                    api_key="test",
                    fetch_enabled=True,
                    fetch_allowed_hosts=("example.com",),
                )
                blocked = task_mcp_response(
                    blocked_settings,
                    store,
                    run["id"],
                    {
                        "jsonrpc": "2.0",
                        "id": 23,
                        "method": "tools/call",
                        "params": {
                            "name": "logagent.fetch",
                            "arguments": {"endpointId": endpoint["id"]},
                        },
                    },
                )
                self.assertIn("not in allowlist", blocked["error"]["message"])
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=2)

    def test_fetch_runs_route_lists_fetch_tool_runs(self) -> None:
        from fastapi.testclient import TestClient
        from logagent_v2.api import create_app

        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(
                data_dir=Path(tmp),
                api_key="test",
                fetch_enabled=True,
                inline_worker=False,
            )
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace("fetch runs", "diagnose", "en-US")
            first = store.create_tool_run(
                workspace["id"],
                "logagent.fetch",
                {"endpointId": "fetch_a"},
            )
            second = store.create_tool_run(
                workspace["id"],
                "logagent.fetch",
                {"fetchId": "fetch_b"},
            )
            store.create_tool_run(workspace["id"], "mock_tool", {})
            headers = {"Authorization": "Bearer test"}

            with TestClient(create_app(settings)) as client:
                listed = client.get("/api/v2/fetch/runs", headers=headers)
                self.assertEqual(listed.status_code, 200)
                listed_body = listed.json()
                self.assertTrue(listed_body["enabled"])
                self.assertEqual(
                    [item["id"] for item in listed_body["runs"]],
                    [second["id"], first["id"]],
                )

                endpoint_filtered = client.get(
                    "/api/v2/fetch/runs?endpointId=fetch_a",
                    headers=headers,
                )
                self.assertEqual(endpoint_filtered.status_code, 200)
                self.assertEqual(
                    [item["id"] for item in endpoint_filtered.json()["runs"]],
                    [first["id"]],
                )

                fetch_id_filtered = client.get(
                    "/api/v2/fetch/runs?fetch_id=fetch_b",
                    headers=headers,
                )
                self.assertEqual(fetch_id_filtered.status_code, 200)
                self.assertEqual(
                    [item["id"] for item in fetch_id_filtered.json()["runs"]],
                    [second["id"]],
                )

    def test_fetch_endpoint_run_route_creates_tool_run(self) -> None:
        from fastapi.testclient import TestClient
        from logagent_v2.api import create_app

        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(
                data_dir=Path(tmp),
                api_key="test",
                fetch_enabled=True,
                inline_worker=False,
            )
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            endpoint = store.create_fetch_endpoint(
                name="runtime metadata",
                method="GET",
                url="http://127.0.0.1/metadata/{instance}",
                headers={},
                body=None,
                enabled=True,
            )
            workspace = store.create_workspace("existing workspace", "diagnose", "en-US")
            headers = {"Authorization": "Bearer test"}

            with TestClient(create_app(settings)) as client:
                created = client.post(
                    f"/api/v2/fetch/endpoints/{endpoint['id']}/runs",
                    headers=headers,
                    json={
                        "workspaceId": workspace["id"],
                        "variables": {"instance": "inst-a"},
                    },
                )
                self.assertEqual(created.status_code, 202)
                created_body = created.json()
                self.assertEqual(created_body["kind"], "tool_run")
                self.assertEqual(created_body["toolId"], "logagent.fetch")
                self.assertEqual(created_body["workspace_id"], workspace["id"])
                self.assertEqual(created_body["toolParams"]["endpointId"], endpoint["id"])
                self.assertEqual(
                    created_body["toolParams"]["variables"],
                    {"instance": "inst-a"},
                )

                auto_workspace_run = client.post(
                    f"/api/v2/fetch/endpoints/{endpoint['id']}/runs",
                    headers=headers,
                    json={},
                )
                self.assertEqual(auto_workspace_run.status_code, 202)
                auto_body = auto_workspace_run.json()
                self.assertNotEqual(auto_body["workspace_id"], workspace["id"])
                self.assertTrue(
                    store.get_workspace(auto_body["workspace_id"])["question"].startswith(
                        "Run fetch endpoint"
                    )
                )

    def test_fetch_runtime_params_apply_overrides_and_body_artifact(self) -> None:
        captured: dict[str, str] = {}

        class RuntimeFetchHandler(BaseHTTPRequestHandler):
            def do_POST(self) -> None:
                length = int(self.headers.get("Content-Length", "0"))
                captured["path"] = self.path
                captured["x_base"] = self.headers.get("X-Base", "")
                captured["x_run"] = self.headers.get("X-Run", "")
                captured["authorization"] = self.headers.get("Authorization", "")
                captured["body"] = self.rfile.read(length).decode("utf-8")
                body = json.dumps(
                    {
                        "ok": True,
                        "path": captured["path"],
                        "body": captured["body"],
                    }
                ).encode("utf-8")
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, format: str, *args: object) -> None:
                return

        server = HTTPServer(("127.0.0.1", 0), RuntimeFetchHandler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            with tempfile.TemporaryDirectory() as tmp:
                settings = Settings(
                    data_dir=Path(tmp),
                    api_key="test",
                    fetch_enabled=True,
                    fetch_allowed_hosts=("127.0.0.1",),
                )
                settings.ensure_dirs()
                store = Store(settings.sqlite_path)
                store.initialize()
                endpoint = store.create_fetch_endpoint(
                    name="runtime metadata",
                    method="POST",
                    url=(
                        f"http://127.0.0.1:{server.server_port}"
                        "/metadata/{instance}?token={token}&keep=1"
                    ),
                    headers={"X-Base": "base"},
                    body="default=body",
                    enabled=True,
                )
                workspace = store.create_workspace("fetch runtime", "diagnose", "en-US")
                run = store.create_run(workspace["id"])

                fetch_response = task_mcp_response(
                    settings,
                    store,
                    run["id"],
                    {
                        "jsonrpc": "2.0",
                        "id": 24,
                        "method": "tools/call",
                        "params": {
                            "name": "logagent.fetch",
                            "arguments": {
                                "fetchId": endpoint["id"],
                                "variables": {
                                    "instance": "i001",
                                    "token": "runtime-secret",
                                },
                                "headers": {
                                    "X-Run": "trace-1",
                                    "Authorization": "Bearer runtime-secret",
                                },
                                "body": "password=runtime-secret&keep=override",
                            },
                        },
                    },
                )
                payload = json.loads(fetch_response["result"]["content"][0]["text"])

                self.assertEqual(captured["path"], "/metadata/i001?token=runtime-secret&keep=1")
                self.assertEqual(captured["x_base"], "base")
                self.assertEqual(captured["x_run"], "trace-1")
                self.assertEqual(captured["authorization"], "Bearer runtime-secret")
                self.assertEqual(captured["body"], "password=runtime-secret&keep=override")

                result = payload["result"]
                self.assertEqual(result["schemaVersion"], 2)
                self.assertTrue(result["httpOk"])
                self.assertEqual(result["statusCode"], 200)
                self.assertEqual(result["fetchId"], endpoint["id"])
                self.assertIn("token=__REDACTED__", result["request"]["url"])
                self.assertEqual(result["request"]["variables"]["token"], "__REDACTED__")
                self.assertEqual(result["request"]["variables"]["instance"], "i001")
                self.assertEqual(result["request"]["headers"]["Authorization"], "__REDACTED__")
                self.assertEqual(result["request"]["headers"]["X-Run"], "trace-1")
                self.assertEqual(
                    result["request"]["bodyPreview"],
                    "password=__REDACTED__&keep=override",
                )
                self.assertEqual(
                    result["bodyArtifactPath"],
                    f"tool_results/{result['actionId']}/response_body.bin",
                )
                self.assertEqual(result["response"]["bodyArtifactId"], result["bodyArtifactId"])
                body_artifact = store.get_artifact(result["bodyArtifactId"])
                body_path = resolve_artifact_path(settings, body_artifact["relative_path"])
                self.assertIn(
                    '"body": "password=runtime-secret&keep=override"',
                    body_path.read_text(encoding="utf-8"),
                )
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=2)

    def test_fetch_rejects_request_body_above_configured_limit(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(
                data_dir=Path(tmp),
                api_key="test",
                fetch_enabled=True,
                fetch_allowed_hosts=("127.0.0.1",),
                fetch_max_request_bytes=8,
            )
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            endpoint = store.create_fetch_endpoint(
                name="body limit",
                method="POST",
                url="http://127.0.0.1:9/metadata",
                headers={},
                body="default",
                enabled=True,
            )
            workspace = store.create_workspace("fetch body limit", "diagnose", "en-US")
            run = store.create_run(workspace["id"])

            fetch_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 25,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.fetch",
                        "arguments": {
                            "endpointId": endpoint["id"],
                            "body": "123456789",
                        },
                    },
                },
            )

            self.assertIn("LOGAGENT_V2_FETCH_MAX_REQUEST_BYTES", fetch_response["error"]["message"])
            self.assertEqual(store.list_evidence(run["id"]), [])

    def test_fetch_sensitive_credentials_are_encrypted_and_hydrated(self) -> None:
        captured: dict[str, str] = {}

        class CredentialHandler(BaseHTTPRequestHandler):
            def do_POST(self) -> None:
                length = int(self.headers.get("Content-Length", "0"))
                captured["path"] = self.path
                captured["authorization"] = self.headers.get("Authorization", "")
                captured["body"] = self.rfile.read(length).decode("utf-8")
                body = json.dumps({"ok": True}).encode("utf-8")
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, format: str, *args: object) -> None:
                return

        server = HTTPServer(("127.0.0.1", 0), CredentialHandler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            with tempfile.TemporaryDirectory() as tmp:
                key = base64.urlsafe_b64encode(b"1" * 32).decode("ascii")
                settings = Settings(
                    data_dir=Path(tmp),
                    api_key="test",
                    fetch_enabled=True,
                    fetch_allowed_hosts=("127.0.0.1",),
                    fetch_secret_key=key,
                )
                settings.ensure_dirs()
                store = Store(settings.sqlite_path)
                store.initialize()
                endpoint = {
                    "name": "secret endpoint",
                    "method": "POST",
                    "url": f"http://127.0.0.1:{server.server_port}/data?api_key=query-secret",
                    "headers": {
                        "Authorization": "Bearer header-secret",
                        "X-Trace": "visible",
                    },
                    "body": '{"password":"body-secret","keep":"value"}',
                    "enabled": True,
                }
                with self.assertRaises(ValueError):
                    validate_fetch_credentials_available(
                        Settings(data_dir=Path(tmp), api_key="test"),
                        endpoint,
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
                stored_endpoint = store.get_fetch_endpoint(created["id"])
                self.assertIn("api_key=__REDACTED__", stored_endpoint["url"])
                self.assertEqual(stored_endpoint["headers"]["Authorization"], "__REDACTED__")
                self.assertIn('"password": "__REDACTED__"', stored_endpoint["body"])
                credential = store.get_fetch_credential_set(created["id"])
                self.assertIsNotNone(credential)
                assert credential is not None
                self.assertNotIn("header-secret", credential["encrypted"])
                public = public_fetch_endpoint(
                    endpoint_with_credential_summary(store, stored_endpoint)
                )
                self.assertTrue(public["hasCredentials"])
                self.assertEqual(
                    public["credentialSet"]["redacted"]["detectedSensitiveFields"][0][
                        "location"
                    ],
                    "query",
                )

                hydrated = hydrate_fetch_endpoint(settings, store, stored_endpoint)
                self.assertIn("api_key=query-secret", hydrated["url"])
                self.assertEqual(hydrated["headers"]["Authorization"], "Bearer header-secret")
                self.assertIn("body-secret", hydrated["body"])

                workspace = store.create_workspace("fetch secret", "diagnose", "en-US")
                run = store.create_run(workspace["id"])
                result = execute_fetch_endpoint(settings, store, workspace["id"], run["id"], created["id"])
                self.assertEqual(result["result"]["status"], "OK")
                self.assertIn("api_key=query-secret", captured["path"])
                self.assertEqual(captured["authorization"], "Bearer header-secret")
                self.assertIn("body-secret", captured["body"])
                self.assertIn("__REDACTED__", result["result"]["request"]["url"])
                self.assertIn("__REDACTED__", result["result"]["request"]["bodyPreview"])
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=2)

    def test_fetch_redirects_are_revalidated_against_allowlist(self) -> None:
        class RedirectHandler(BaseHTTPRequestHandler):
            def do_GET(self) -> None:
                if self.path.startswith("/redirect-ok"):
                    location = f"http://127.0.0.1:{self.server.server_port}/target?api_key=secret"
                    self.send_response(302)
                    self.send_header("Location", location)
                    self.end_headers()
                    return
                if self.path.startswith("/redirect-blocked"):
                    self.send_response(302)
                    self.send_header("Location", "http://example.com/target")
                    self.end_headers()
                    return
                body = json.dumps({"ok": True, "path": self.path}).encode("utf-8")
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, format: str, *args: object) -> None:
                return

        server = HTTPServer(("127.0.0.1", 0), RedirectHandler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            with tempfile.TemporaryDirectory() as tmp:
                settings = Settings(
                    data_dir=Path(tmp),
                    api_key="test",
                    fetch_enabled=True,
                    fetch_allowed_hosts=("127.0.0.1",),
                    fetch_max_redirects=2,
                )
                settings.ensure_dirs()
                store = Store(settings.sqlite_path)
                store.initialize()
                no_follow_endpoint = store.create_fetch_endpoint(
                    name="redirect no follow",
                    method="GET",
                    url=f"http://127.0.0.1:{server.server_port}/redirect-ok",
                    headers={"Authorization": "Bearer secret"},
                    body=None,
                    enabled=True,
                )
                ok_endpoint = store.create_fetch_endpoint(
                    name="redirect ok",
                    method="GET",
                    url=f"http://127.0.0.1:{server.server_port}/redirect-ok",
                    headers={"Authorization": "Bearer secret"},
                    body=None,
                    enabled=True,
                    follow_redirects=True,
                )
                blocked_endpoint = store.create_fetch_endpoint(
                    name="redirect blocked",
                    method="GET",
                    url=f"http://127.0.0.1:{server.server_port}/redirect-blocked",
                    headers={"Authorization": "Bearer secret"},
                    body=None,
                    enabled=True,
                    follow_redirects=True,
                )
                workspace = store.create_workspace("fetch redirect", "diagnose", "en-US")
                run = store.create_run(workspace["id"])

                no_follow_response = task_mcp_response(
                    settings,
                    store,
                    run["id"],
                    {
                        "jsonrpc": "2.0",
                        "id": 240,
                        "method": "tools/call",
                        "params": {
                            "name": "logagent.fetch",
                            "arguments": {"endpointId": no_follow_endpoint["id"]},
                        },
                    },
                )
                no_follow_payload = json.loads(no_follow_response["result"]["content"][0]["text"])
                no_follow_result = no_follow_payload["result"]
                self.assertEqual(no_follow_result["status"], "OK")
                self.assertEqual(no_follow_result["response"]["statusCode"], 302)
                self.assertFalse(no_follow_result["httpOk"])
                self.assertEqual(no_follow_result["response"]["redirectCount"], 0)

                ok_response = task_mcp_response(
                    settings,
                    store,
                    run["id"],
                    {
                        "jsonrpc": "2.0",
                        "id": 24,
                        "method": "tools/call",
                        "params": {
                            "name": "logagent.fetch",
                            "arguments": {"endpointId": ok_endpoint["id"]},
                        },
                    },
                )
                ok_payload = json.loads(ok_response["result"]["content"][0]["text"])
                ok_result = ok_payload["result"]
                self.assertEqual(ok_result["status"], "OK")
                self.assertEqual(ok_result["response"]["statusCode"], 200)
                self.assertEqual(ok_result["response"]["redirectCount"], 1)
                self.assertIn("api_key=__REDACTED__", ok_result["response"]["finalUrl"])
                self.assertEqual(ok_result["response"]["redirects"][0]["statusCode"], 302)

                blocked_response = task_mcp_response(
                    settings,
                    store,
                    run["id"],
                    {
                        "jsonrpc": "2.0",
                        "id": 25,
                        "method": "tools/call",
                        "params": {
                            "name": "logagent.fetch",
                            "arguments": {"endpointId": blocked_endpoint["id"]},
                        },
                    },
                )
                blocked_payload = json.loads(blocked_response["result"]["content"][0]["text"])
                self.assertEqual(blocked_payload["result"]["status"], "FAILED")
                self.assertIn("not in allowlist", blocked_payload["result"]["error"])
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=2)

    def test_fetch_curl_import_preview_redacts_sensitive_values(self) -> None:
        curl = r"""curl 'https://api.example.com/v1/items?limit=10&api_key=secret-query' \
  -H 'Authorization: Bearer secret-token' \
  -H 'Content-Type: application/json' \
  --data-raw '{"password":"secret-body","keep":"value"}' \
  --compressed \
  --location"""

        preview = preview_curl_import(curl)
        endpoint = preview["endpoint"]
        self.assertEqual(endpoint["method"], "POST")
        self.assertIn("api_key=__REDACTED__", endpoint["url"])
        self.assertTrue(endpoint["followRedirects"])
        self.assertEqual(endpoint["headers"]["Authorization"], "__REDACTED__")
        self.assertIn('"password": "__REDACTED__"', endpoint["bodyPreview"])
        self.assertIn({"location": "query", "name": "api_key"}, preview["detectedSensitiveFields"])
        self.assertIn(
            {"location": "header", "name": "Authorization"},
            preview["detectedSensitiveFields"],
        )
        self.assertIn(
            {"location": "body", "name": "password"},
            preview["detectedSensitiveFields"],
        )

        endpoint = endpoint_from_curl(curl, name="Imported API", enabled=False)
        self.assertEqual(endpoint["name"], "Imported API")
        self.assertEqual(endpoint["method"], "POST")
        self.assertEqual(endpoint["headers"]["Authorization"], "Bearer secret-token")
        self.assertIn("api_key=secret-query", endpoint["url"])
        self.assertFalse(endpoint["enabled"])

        head = endpoint_from_curl("curl -I https://api.example.com/health")
        self.assertEqual(head["method"], "HEAD")
        self.assertFalse(head["followRedirects"])

        prompted = endpoint_from_curl("$ curl -I https://api.example.com/health")
        self.assertEqual(prompted["method"], "HEAD")
        self.assertEqual(prompted["url"], "https://api.example.com/health")
        self.assertFalse(prompted["followRedirects"])

        with self.assertRaisesRegex(ValueError, "unsupported curl flag --form"):
            preview_curl_import("curl https://api.example.com --form file=@/tmp/a")

        with self.assertRaisesRegex(ValueError, "controlled"):
            endpoint_from_curl("curl https://api.example.com -H 'Host: evil.example.com'")

    def test_metadata_preview_confirm_import_workflow(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(data_dir=Path(tmp), api_key="test")
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            content = json.dumps(
                {
                    "cluster": {
                        "clusterId": "cluster-a",
                        "nodes": [{"nodeId": "n1", "host": "127.0.0.1"}],
                        "databases": [{"name": "db0"}],
                    }
                }
            )

            preview = preview_metadata_import(
                store,
                instance_id="inst-preview",
                template_type="json",
                content=content,
                remark="preview only",
            )
            self.assertEqual(preview["import"]["status"], "previewed")
            self.assertEqual(preview["import"]["nodeCount"], 1)
            self.assertEqual(store.list_metadata_instances(), [])

            drafts = store.list_metadata_imports()
            self.assertEqual(drafts[0]["importId"], preview["import"]["importId"])
            self.assertEqual(
                store.get_metadata_import(preview["import"]["importId"])["status"],
                "previewed",
            )

            confirmed = confirm_metadata_import(store, preview["import"]["importId"])
            self.assertEqual(confirmed["import"]["status"], "confirmed")
            instances = store.list_metadata_instances()
            self.assertEqual(instances[0]["instanceId"], "inst-preview")
            self.assertEqual(instances[0]["nodeCount"], 1)

    def test_metadata_cluster_routes_derive_from_snapshots(self) -> None:
        from fastapi.testclient import TestClient
        from logagent_v2.api import create_app

        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(data_dir=Path(tmp), api_key="test", inline_worker=False)
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            import_metadata(
                store,
                instance_id="inst-cluster",
                template_type="json",
                content=json.dumps(
                    {
                        "cluster": {
                            "clusterId": "cluster-route",
                            "nodes": [{"nodeId": "n1", "host": "127.0.0.1"}],
                            "databases": [{"name": "db0"}],
                        }
                    }
                ),
                remark="cluster route",
            )
            headers = {"Authorization": "Bearer test"}

            with TestClient(create_app(settings)) as client:
                cluster_response = client.get(
                    "/api/v2/metadata/clusters/cluster-route",
                    headers=headers,
                )
                self.assertEqual(cluster_response.status_code, 200)
                self.assertEqual(
                    cluster_response.json()["cluster"]["clusterId"],
                    "cluster-route",
                )
                nodes_response = client.get(
                    "/api/v2/metadata/clusters/cluster-route/nodes",
                    headers=headers,
                )
                self.assertEqual(nodes_response.status_code, 200)
                self.assertEqual(nodes_response.json()["nodes"][0]["nodeId"], "n1")
                missing = client.get(
                    "/api/v2/metadata/clusters/missing",
                    headers=headers,
                )
                self.assertEqual(missing.status_code, 404)

    def test_metadata_refresh_rebuilds_snapshot_from_stored_raw(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(data_dir=Path(tmp), api_key="test")
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            imported = import_metadata(
                store,
                instance_id="inst-refresh",
                template_type="opengemini",
                content=json.dumps(
                    {
                        "ClusterID": 99,
                        "Version": "1.2.3",
                        "DataNodes": [{"ID": 1, "Host": "10.0.0.1"}],
                        "Databases": {"db0": {"RetentionPolicies": {}}},
                    }
                ),
                remark="refresh me",
            )
            stale_snapshot = dict(imported["snapshot"])
            stale_snapshot["cluster"] = dict(stale_snapshot["cluster"])
            stale_snapshot["cluster"]["nodes"] = []
            store.upsert_metadata_instance(
                instance_id="inst-refresh",
                remark="refresh me",
                template_type="opengemini",
                snapshot=stale_snapshot,
                raw=store.get_metadata_instance("inst-refresh")["raw"],
            )
            self.assertEqual(store.list_metadata_instances()[0]["nodeCount"], 0)

            refreshed = refresh_metadata_instance(store, "inst-refresh")

            self.assertEqual(refreshed["snapshot"]["instance"]["version"], "1.2.3")
            self.assertEqual(len(refreshed["snapshot"]["cluster"]["nodes"]), 1)
            self.assertEqual(store.list_metadata_instances()[0]["nodeCount"], 1)

    def test_metadata_url_fetch_preview_confirm_uses_allowlist(self) -> None:
        class MetadataHandler(BaseHTTPRequestHandler):
            def do_GET(self) -> None:
                body = json.dumps(
                    {
                        "ClusterID": 99,
                        "DataNodes": [{"ID": 1, "Host": "10.0.0.1"}],
                        "Databases": {"db0": {"RetentionPolicies": {}}},
                    }
                ).encode("utf-8")
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, format: str, *args: object) -> None:
                return

        server = HTTPServer(("127.0.0.1", 0), MetadataHandler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            with tempfile.TemporaryDirectory() as tmp:
                settings = Settings(
                    data_dir=Path(tmp),
                    api_key="test",
                    fetch_enabled=True,
                    fetch_allowed_hosts=("127.0.0.1",),
                )
                settings.ensure_dirs()
                store = Store(settings.sqlite_path)
                store.initialize()
                url = f"http://127.0.0.1:{server.server_port}/getdata?token=secret"

                preview = preview_metadata_import_from_url(
                    settings,
                    store,
                    instance_id="inst-url",
                    template_type="opengemini",
                    url=url,
                    remark="url import",
                )
                self.assertEqual(preview["fetch"]["statusCode"], 200)
                self.assertIn("token=__REDACTED__", preview["import"]["sourceUrl"])
                self.assertEqual(store.list_metadata_instances(), [])

                confirmed = confirm_metadata_import(store, preview["import"]["importId"])
                self.assertEqual(confirmed["instance"]["instanceId"], "inst-url")
                self.assertEqual(store.list_metadata_instances()[0]["databaseCount"], 1)

                direct = import_metadata_from_url(
                    settings,
                    store,
                    instance_id="inst-url-direct",
                    template_type="opengemini",
                    url=url,
                )
                self.assertEqual(direct["instance"]["instanceId"], "inst-url-direct")
                instance_count = len(store.list_metadata_instances())

                from fastapi.testclient import TestClient
                from logagent_v2.api import create_app

                with TestClient(create_app(settings)) as client:
                    fetched_snapshot = client.post(
                        "/api/v2/metadata/snapshots/fetch",
                        headers={"Authorization": "Bearer test"},
                        json={
                            "instanceId": "inst-url-snapshot",
                            "templateType": "opengemini",
                            "url": url,
                            "remark": "snapshot only",
                        },
                    )
                self.assertEqual(fetched_snapshot.status_code, 200)
                fetched_body = fetched_snapshot.json()
                self.assertEqual(
                    fetched_body["instance"]["instanceId"],
                    "inst-url-snapshot",
                )
                self.assertEqual(fetched_body["fetch"]["statusCode"], 200)
                self.assertIn("token=__REDACTED__", fetched_body["fetch"]["url"])
                self.assertEqual(
                    len(store.list_metadata_instances()),
                    instance_count,
                )

                blocked_settings = Settings(
                    data_dir=Path(tmp),
                    api_key="test",
                    fetch_enabled=True,
                    fetch_allowed_hosts=("example.com",),
                )
                with self.assertRaisesRegex(ValueError, "not in allowlist"):
                    preview_metadata_import_from_url(
                        blocked_settings,
                        store,
                        instance_id="blocked",
                        template_type="opengemini",
                        url=url,
                    )
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=2)

    def test_task_mcp_waiting_actions_are_persisted_and_resumable(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(data_dir=Path(tmp), api_key="test")
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace("need more data", "diagnose", "en-US")
            run = store.create_run(workspace["id"])

            prompt_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 6,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.request_user_input",
                        "arguments": {
                            "questionId": "q-version",
                            "question": "Which version?",
                            "reason": "version affects diagnostics",
                        },
                    },
                },
            )
            prompt_payload = json.loads(prompt_response["result"]["content"][0]["text"])
            prompt_action = store.get_action(prompt_payload["action"]["id"])
            self.assertEqual(prompt_action["kind"], "user_input")
            self.assertEqual(prompt_action["payload"]["questionId"], "q-version")
            self.assertEqual(prompt_payload["runtimeStatus"], "waiting_for_user")
            self.assertEqual(prompt_payload["artifactPath"], "mcp_waiting_request.json")
            self.assertEqual(
                prompt_payload["evidenceRefs"], ["mcp_waiting_request.json#request"]
            )
            self.assertEqual(store.get_run(run["id"])["status"], "waiting_for_user")

            store.update_run_status(run["id"], "queued", "queued")
            queued = store.enqueue_run(run["id"])
            self.assertEqual(queued["runId"], run["id"])

            approval_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 7,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.request_approval",
                        "arguments": {
                            "actionType": "remote_collect",
                            "reason": "Need remote logs",
                            "input": {"node": "n1"},
                        },
                    },
                },
            )
            approval_payload = json.loads(approval_response["result"]["content"][0]["text"])
            approval = store.get_action(approval_payload["action"]["id"])
            self.assertEqual(approval["kind"], "approval")
            self.assertEqual(approval_payload["runtimeStatus"], "waiting_for_approval")
            self.assertEqual(approval_payload["artifactPath"], "mcp_waiting_request.json")
            self.assertEqual(
                approval_payload["evidenceRefs"], ["mcp_waiting_request.json#request"]
            )
            self.assertEqual(store.get_run(run["id"])["status"], "waiting_for_approval")

            decided = store.decide_action(approval["id"], "approved", "ok")
            self.assertEqual(decided["status"], "approved")
            self.assertEqual(decided["result"]["decision"], "approved")

            store.update_run_status(run["id"], "queued", "queued")
            legacy_approval_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 8,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.request_approval",
                        "arguments": {"reason": "Need operator confirmation"},
                    },
                },
            )
            legacy_payload = json.loads(
                legacy_approval_response["result"]["content"][0]["text"]
            )
            legacy_action = store.get_action(legacy_payload["action"]["id"])
            self.assertEqual(
                legacy_action["payload"]["actionType"],
                "manual_approval",
            )
            waiting_evidence = [
                item
                for item in store.list_evidence(run["id"])
                if item["kind"] == "mcp_waiting_request"
            ]
            self.assertGreaterEqual(len(waiting_evidence), 3)
            last_artifact = store.get_artifact(waiting_evidence[-1]["artifact_id"])
            last_path = resolve_artifact_path(settings, last_artifact["relative_path"])
            last_value = json.loads(last_path.read_text(encoding="utf-8"))
            self.assertEqual(last_value["runtimeStatus"], "waiting_for_approval")
            self.assertEqual(last_value["request"]["reason"], "Need operator confirmation")

    def test_approved_collect_environment_records_background_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(data_dir=Path(tmp), api_key="test")
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace(
                "need approved environment evidence", "diagnose", "en-US"
            )
            run = store.create_run(workspace["id"])
            action = store.create_action(
                run["id"],
                "approval",
                {
                    "actionType": "collect_environment",
                    "reason": "Need node status",
                    "input": {"scope": "node", "commands": ["uptime"], "nodeId": "n1"},
                },
            )

            decided = store.decide_action(action["id"], "approved", "ok")
            evidence = persist_approved_environment_evidence(settings, store, decided)

            self.assertIsNotNone(evidence)
            assert evidence is not None
            self.assertEqual(evidence["kind"], "environment_evidence")
            self.assertFalse(evidence["final_allowed"])
            self.assertEqual(evidence["payload"]["actionId"], action["id"])
            artifact = store.get_artifact(evidence["artifact_id"])
            artifact_path = resolve_artifact_path(settings, artifact["relative_path"])
            artifact_json = json.loads(artifact_path.read_text(encoding="utf-8"))
            self.assertEqual(artifact_json["status"], "MOCK")
            self.assertEqual(artifact_json["input"]["nodeId"], "n1")
            self.assertFalse(artifact_json["finalEvidenceAllowed"])

            analysis = get_run_analysis(settings, store, run["id"])
            self.assertEqual(
                analysis["resources"]["environment_evidence"]["actionId"],
                action["id"],
            )
            resource_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 77,
                    "method": "resources/read",
                    "params": {
                        "uri": f"logagent-v2://run/{run['id']}/environment_evidence"
                    },
                },
            )
            resource_json = json.loads(resource_response["result"]["contents"][0]["text"])
            self.assertEqual(resource_json["actionId"], action["id"])

            final_answer = AgentRuntime(settings, store).run_analysis(
                workspace["id"], run["id"]
            )
            self.assertEqual(final_answer["confidence"], "low")
            request_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 78,
                    "method": "resources/read",
                    "params": {"uri": f"logagent-v2://run/{run['id']}/agent_request"},
                },
            )
            request_doc = json.loads(request_response["result"]["contents"][0]["text"])
            prompt = json.loads(request_doc["payload"]["prompt"])
            self.assertEqual(
                prompt["backgroundEvidence"][0]["kind"], "environment_evidence"
            )
            self.assertEqual(
                prompt["backgroundEvidence"][0]["payload"]["actionId"], action["id"]
            )
            package_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 79,
                    "method": "resources/read",
                    "params": {"uri": f"logagent-v2://run/{run['id']}/analysis_package"},
                },
            )
            package = json.loads(package_response["result"]["contents"][0]["text"])
            self.assertEqual(
                package["backgroundEvidence"][0]["payload"]["actionId"], action["id"]
            )
            self.assertIn(
                "environment_evidence",
                package["finalEvidencePolicy"]["backgroundOnlyKinds"],
            )

    def test_approved_collect_environment_can_use_remote_executor(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            fake_ssh = root / "fake-ssh"
            fake_ssh.write_text(
                "#!/usr/bin/env python3\n"
                "import json, sys\n"
                "print(json.dumps({'argv': sys.argv[1:]}))\n",
                encoding="utf-8",
            )
            fake_ssh.chmod(0o755)
            settings = Settings(
                data_dir=root / "data",
                api_key="test",
                remote_ssh_command=fake_ssh.as_posix(),
                remote_commands=(
                    RemoteCommandTemplate(
                        command_id="env_snapshot",
                        display_name="Environment snapshot",
                        description="collect bounded environment status",
                        argv=("uname", "-a"),
                        timeout_seconds=5,
                    ),
                ),
            )
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace(
                "need real environment evidence", "diagnose", "en-US"
            )
            run = store.create_run(workspace["id"])
            initial_jobs = store.acquire_jobs("test-worker", limit=1)
            self.assertEqual(initial_jobs[0]["kind"], "run_analysis")
            store.complete_job(initial_jobs[0]["id"])
            store.update_run_status(run["id"], "waiting_for_approval", "waiting_for_approval")
            executor = store.create_remote_executor(
                {
                    "name": "fake executor",
                    "host": "127.0.0.1",
                    "port": 2222,
                    "user": "root",
                    "enabled": True,
                }
            )
            action = store.create_action(
                run["id"],
                "approval",
                {
                    "actionType": "collect_environment",
                    "reason": "Need remote node status",
                    "input": {
                        "executorId": executor["executorId"],
                        "commandId": "env_snapshot",
                        "scope": "node_status",
                    },
                },
            )
            decided = store.decide_action(action["id"], "approved", "ok")

            pending = persist_approved_environment_evidence(settings, store, decided)

            self.assertIsNotNone(pending)
            assert pending is not None
            self.assertEqual(pending["payload"]["status"], "QUEUED")
            self.assertEqual(pending["payload"]["remoteCommandId"], "env_snapshot")
            self.assertEqual(
                [
                    item
                    for item in store.list_evidence(run["id"])
                    if item["kind"] == "environment_evidence"
                ],
                [],
            )
            remote_jobs = store.acquire_jobs("remote-worker", limit=1)
            self.assertEqual(remote_jobs[0]["kind"], "remote_command_run")

            asyncio.run(JobRunner(settings, store).process_job(remote_jobs[0]))

            evidence_items = [
                item
                for item in store.list_evidence(run["id"])
                if item["kind"] == "environment_evidence"
            ]
            self.assertEqual(len(evidence_items), 1)
            evidence = evidence_items[0]
            self.assertEqual(evidence["payload"]["status"], "COLLECTED")
            self.assertEqual(evidence["payload"]["remoteCommandId"], "env_snapshot")
            artifact = store.get_artifact(evidence["artifact_id"])
            artifact_path = resolve_artifact_path(settings, artifact["relative_path"])
            artifact_json = json.loads(artifact_path.read_text(encoding="utf-8"))
            self.assertEqual(artifact_json["status"], "COLLECTED")
            self.assertEqual(artifact_json["remoteStatus"], "OK")
            self.assertIn("uname", artifact_json["stdoutPreview"])
            self.assertEqual(store.get_run(run["id"])["status"], "queued")
            resume_jobs = store.acquire_jobs("analysis-worker", limit=1)
            self.assertEqual(resume_jobs[0]["kind"], "run_analysis")

    def test_collect_environment_invalid_remote_target_records_rejection(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(data_dir=Path(tmp), api_key="test")
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace("bad env target", "diagnose", "en-US")
            run = store.create_run(workspace["id"])
            action = store.create_action(
                run["id"],
                "approval",
                {
                    "actionType": "collect_environment",
                    "reason": "Need remote node status",
                    "input": {"commandId": "missing_executor"},
                },
            )
            decided = store.decide_action(action["id"], "approved", "ok")

            evidence = persist_approved_environment_evidence(settings, store, decided)

            self.assertIsNotNone(evidence)
            assert evidence is not None
            self.assertEqual(evidence["payload"]["status"], "REMOTE_REJECTED")
            artifact = store.get_artifact(evidence["artifact_id"])
            artifact_path = resolve_artifact_path(settings, artifact["relative_path"])
            artifact_json = json.loads(artifact_path.read_text(encoding="utf-8"))
            self.assertEqual(artifact_json["status"], "REMOTE_REJECTED")
            self.assertIn("executorId", artifact_json["error"])

    def test_metadata_import_query_and_mcp_background_slice(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(data_dir=Path(tmp), api_key="test")
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            raw = {
                "ClusterID": 42,
                "DataNodes": [{"ID": 1, "Host": "10.0.0.1", "Status": "alive"}],
                "Databases": {
                    "db0": {
                        "DefaultRetentionPolicy": "autogen",
                        "RetentionPolicies": {
                            "autogen": {
                                "Measurements": {
                                    "cpu": {
                                        "Schema": {
                                            "host": 6,
                                            "value": {"Typ": 3, "EndTime": 123},
                                        }
                                    }
                                }
                            }
                        },
                    }
                },
            }
            imported = import_metadata(
                store,
                instance_id="inst1",
                template_type="opengemini",
                content=json.dumps(raw),
                remark="test cluster",
            )
            self.assertEqual(imported["snapshot"]["instance"]["tags"]["sourceClusterId"], "42")
            instances = store.list_metadata_instances()
            self.assertEqual(instances[0]["instanceId"], "inst1")
            self.assertEqual(instances[0]["nodeCount"], 1)
            self.assertEqual(instances[0]["databaseCount"], 1)

            readonly_resources = readonly_mcp_response(
                settings,
                store,
                {"jsonrpc": "2.0", "id": 129, "method": "resources/list"},
            )
            readonly_resource_uris = {
                item["uri"] for item in readonly_resources["result"]["resources"]
            }
            self.assertIn(
                "logagent://metadata/instances/inst1/snapshot",
                readonly_resource_uris,
            )
            self.assertIn(
                "logagent-v2://metadata/instances/inst1/snapshot",
                readonly_resource_uris,
            )

            fields = query_field_types(
                store,
                instance_id="inst1",
                database="db0",
                measurement="cpu",
                field=["host", "value", "missing"],
            )
            by_name = {item["name"]: item for item in fields["fields"]}
            self.assertEqual(by_name["host"]["typeLabel"], "Tag")
            self.assertEqual(by_name["value"]["typeLabel"], "Float")
            self.assertEqual(fields["missingFields"], ["missing"])
            self.assertTrue(fields["defaultRetentionPolicyUsed"])
            unfiltered_fields = query_field_types(
                store,
                instance_id="inst1",
                database="db0",
                measurement="cpu",
                field=" ",
            )
            self.assertEqual(
                [item["name"] for item in unfiltered_fields["fields"]],
                ["host", "value"],
            )
            with self.assertRaisesRegex(ValueError, "field entries must be non-empty strings"):
                query_field_types(
                    store,
                    instance_id="inst1",
                    database="db0",
                    measurement="cpu",
                    field=["host", ""],
                )

            readonly_instances = readonly_mcp_response(
                settings,
                store,
                {
                    "jsonrpc": "2.0",
                    "id": 13,
                    "method": "resources/read",
                    "params": {"uri": "logagent-v2://metadata/instances"},
                },
            )
            readonly_body = json.loads(readonly_instances["result"]["contents"][0]["text"])
            self.assertEqual(readonly_body["instances"][0]["instanceId"], "inst1")

            readonly_instances_alias = readonly_mcp_response(
                settings,
                store,
                {
                    "jsonrpc": "2.0",
                    "id": 131,
                    "method": "resources/read",
                    "params": {"uri": "logagent://metadata/instances"},
                },
            )
            readonly_alias_body = json.loads(
                readonly_instances_alias["result"]["contents"][0]["text"]
            )
            self.assertEqual(readonly_alias_body["instances"][0]["instanceId"], "inst1")

            readonly_snapshot_alias = readonly_mcp_response(
                settings,
                store,
                {
                    "jsonrpc": "2.0",
                    "id": 132,
                    "method": "resources/read",
                    "params": {"uri": "logagent://metadata/instances/inst1/snapshot"},
                },
            )
            snapshot_alias_body = json.loads(
                readonly_snapshot_alias["result"]["contents"][0]["text"]
            )
            self.assertEqual(snapshot_alias_body["instance"]["instanceId"], "inst1")

            readonly_snapshot_tool = readonly_mcp_response(
                settings,
                store,
                {
                    "jsonrpc": "2.0",
                    "id": 133,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.get_metadata_snapshot",
                        "arguments": {"instanceId": "inst1"},
                    },
                },
            )
            readonly_snapshot_body = json.loads(
                readonly_snapshot_tool["result"]["content"][0]["text"]
            )
            self.assertEqual(readonly_snapshot_body["snapshot"]["instance"]["instanceId"], "inst1")
            self.assertEqual(readonly_snapshot_body["instance"]["instanceId"], "inst1")

            readonly_tags = readonly_mcp_response(
                settings,
                store,
                {
                    "jsonrpc": "2.0",
                    "id": 14,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.get_metadata_tag_fields",
                        "arguments": {
                            "instanceId": "inst1",
                            "database": "db0",
                            "measurement": "cpu",
                        },
                    },
                },
            )
            tag_body = json.loads(readonly_tags["result"]["content"][0]["text"])
            self.assertEqual([item["name"] for item in tag_body["fields"]], ["host"])
            self.assertEqual(
                [item["name"] for item in tag_body["result"]["fields"]],
                ["host"],
            )
            readonly_tags_with_field = readonly_mcp_response(
                settings,
                store,
                {
                    "jsonrpc": "2.0",
                    "id": 141,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.get_metadata_tag_fields",
                        "arguments": {
                            "instanceId": "inst1",
                            "database": "db0",
                            "measurement": "cpu",
                            "field": "host",
                        },
                    },
                },
            )
            self.assertIn(
                "metadata tag field params do not support field",
                readonly_tags_with_field["error"]["message"],
            )

            workspace = store.create_workspace("metadata context", "diagnose", "en-US")
            run = store.create_run(workspace["id"])
            task_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 15,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.get_metadata_field_types",
                        "arguments": {
                            "instanceId": "inst1",
                            "database": "db0",
                            "measurement": "cpu",
                        },
                    },
                },
            )
            task_body = json.loads(task_response["result"]["content"][0]["text"])
            self.assertFalse(task_body["finalEvidenceAllowed"])
            self.assertTrue(task_body["artifactPath"].startswith("metadata_slices/field_types_"))
            self.assertEqual(task_body["backgroundRef"], f"{task_body['artifactPath']}#fields")
            self.assertEqual(task_body["evidenceRefs"], [task_body["backgroundRef"]])
            self.assertEqual(task_body["result"]["artifactPath"], task_body["artifactPath"])
            self.assertEqual(task_body["result"]["backgroundRef"], task_body["backgroundRef"])
            self.assertTrue(task_body["result"]["defaultRetentionPolicyUsed"])
            evidence = store.list_evidence(run["id"])
            metadata_slices = [item for item in evidence if item["kind"] == "metadata_slice"]
            self.assertEqual(len(metadata_slices), 1)
            self.assertFalse(metadata_slices[0]["final_allowed"])
            self.assertEqual(metadata_slices[0]["payload"]["path"], task_body["artifactPath"])

            task_tags_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 16,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.get_metadata_tag_fields",
                        "arguments": {
                            "instanceId": "inst1",
                            "database": "db0",
                            "measurement": "cpu",
                        },
                    },
                },
            )
            task_tags_body = json.loads(task_tags_response["result"]["content"][0]["text"])
            self.assertTrue(
                task_tags_body["artifactPath"].startswith("metadata_slices/tag_fields_")
            )
            self.assertEqual(
                [item["name"] for item in task_tags_body["result"]["fields"]],
                ["host"],
            )
            self.assertEqual(task_tags_body["evidenceRefs"], [task_tags_body["backgroundRef"]])
            metadata_slices = [
                item for item in store.list_evidence(run["id"]) if item["kind"] == "metadata_slice"
            ]
            self.assertEqual(len(metadata_slices), 2)

    def test_task_mcp_metadata_v1_aliases_query_bounded_slices(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(data_dir=Path(tmp), api_key="test")
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            raw = {
                "ClusterID": 42,
                "DataNodes": [{"ID": 1, "Host": "10.0.0.1", "Status": "alive"}],
                "PtView": [
                    {
                        "Database": "db0",
                        "PtId": 7,
                        "OwnerNodeID": 2,
                        "Status": 1,
                        "Version": 3,
                    }
                ],
                "Databases": {
                    "db0": {
                        "DefaultRetentionPolicy": "autogen",
                        "RetentionPolicies": {
                            "autogen": {
                                "Measurements": {
                                    "cpu": {
                                        "Schema": {
                                            "host": 6,
                                            "usage": {"Typ": 3},
                                        }
                                    }
                                },
                                "ShardGroups": [
                                    {
                                        "ID": 10,
                                        "ShardIds": [100, 101],
                                        "Owners": [7],
                                    }
                                ],
                                "IndexGroups": [
                                    {
                                        "ID": 20,
                                        "Indexes": [
                                            {"ID": 200, "Owners": [7], "Tier": 1}
                                        ],
                                    }
                                ],
                            }
                        },
                    }
                },
            }
            import_metadata(
                store,
                instance_id="inst-meta",
                template_type="opengemini",
                content=json.dumps(raw),
                remark="metadata alias cluster",
            )
            workspace = store.create_workspace(
                "inst-meta db0 shard owner investigation",
                "diagnose",
                "en-US",
            )
            run = store.create_run(workspace["id"])
            AgentRuntime(settings, store).run_analysis(workspace["id"], run["id"])

            listed = task_mcp_response(
                settings,
                store,
                run["id"],
                {"jsonrpc": "2.0", "id": 30, "method": "tools/list"},
            )
            tool_names = {tool["name"] for tool in listed["result"]["tools"]}
            self.assertIn("logagent.get_metadata_topology", tool_names)
            self.assertIn("logagent.query_metadata", tool_names)

            topology_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 31,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.get_metadata_topology",
                        "arguments": {},
                    },
                },
            )
            topology = json.loads(topology_response["result"]["content"][0]["text"])
            self.assertEqual(topology["kind"], "metadata_context_outline")
            self.assertEqual(topology["selected"]["instanceId"], "inst-meta")
            self.assertEqual(topology["counts"]["shards"], 2)
            self.assertEqual(
                topology["sections"]["shards"]["query"]["tool"],
                "logagent.query_metadata",
            )
            self.assertFalse(topology["finalEvidenceAllowed"])

            query_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 32,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.query_metadata",
                        "arguments": {
                            "section": "shards",
                            "database": "db0",
                            "ownerNodeId": 2,
                            "limit": 1,
                        },
                    },
                },
            )
            query_body = json.loads(query_response["result"]["content"][0]["text"])
            self.assertEqual(query_body["section"], "shards")
            self.assertEqual(query_body["total"], 2)
            self.assertEqual(query_body["items"][0]["id"], 100)
            self.assertEqual(query_body["nextCursor"], "1")
            self.assertTrue(query_body["truncated"])
            self.assertFalse(query_body["finalEvidenceAllowed"])
            self.assertTrue(query_body["backgroundRef"].startswith("metadata_slices/slice_"))
            metadata_slices = [
                item for item in store.list_evidence(run["id"])
                if item["kind"] == "metadata_slice"
                and item["payload"].get("tool") == "logagent.query_metadata"
            ]
            self.assertEqual(len(metadata_slices), 1)
            self.assertEqual(
                metadata_slices[0]["payload"]["backgroundRef"],
                query_body["backgroundRef"],
            )

            index_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 33,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.query_metadata",
                        "arguments": {
                            "section": "indexes",
                            "database": "db0",
                            "ptId": 7,
                        },
                    },
                },
            )
            index_body = json.loads(index_response["result"]["content"][0]["text"])
            self.assertEqual(index_body["items"][0]["id"], 200)

            invalid_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 34,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.query_metadata",
                        "arguments": {"section": "nodes", "database": "db0"},
                    },
                },
            )
            self.assertIn("filter database is not supported", invalid_response["error"]["message"])

    def test_metadata_context_auto_selection_run_resource(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(data_dir=Path(tmp), api_key="test")
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            prod_raw = {
                "ClusterID": 7,
                "DataNodes": [{"ID": 1, "Host": "10.0.0.1", "Status": "alive"}],
                "Databases": {
                    "metrics": {
                        "DefaultRetentionPolicy": "autogen",
                        "RetentionPolicies": {
                            "autogen": {
                                "Measurements": {
                                    "cpu": {
                                        "Schema": {
                                            "host": 6,
                                            "usage": {"Typ": 3},
                                        }
                                    }
                                }
                            }
                        },
                    }
                },
            }
            backup_raw = {
                "ClusterID": 8,
                "DataNodes": [{"ID": 2, "Host": "10.0.0.2", "Status": "alive"}],
                "Databases": {
                    "backupdb": {
                        "RetentionPolicies": {
                            "autogen": {"Measurements": {"retention": {"Schema": {"path": 6}}}}
                        }
                    }
                },
            }
            import_metadata(
                store,
                instance_id="prod-og",
                template_type="opengemini",
                content=json.dumps(prod_raw),
                remark="production query cluster",
            )
            import_metadata(
                store,
                instance_id="backup-og",
                template_type="opengemini",
                content=json.dumps(backup_raw),
                remark="backup cluster",
            )

            workspace = store.create_workspace(
                "prod-og cpu usage timeout in metrics",
                "diagnose",
                "en-US",
            )
            run = store.create_run(workspace["id"])
            AgentRuntime(settings, store).run_analysis(workspace["id"], run["id"])

            listed = task_mcp_response(
                settings,
                store,
                run["id"],
                {"jsonrpc": "2.0", "id": 16, "method": "resources/list"},
            )
            resource_names = {
                item["name"] for item in listed["result"]["resources"]
            }
            self.assertIn("metadata_context", resource_names)

            context_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 17,
                    "method": "resources/read",
                    "params": {"uri": f"logagent-v2://run/{run['id']}/metadata_context"},
                },
            )
            context = json.loads(context_response["result"]["contents"][0]["text"])
            self.assertEqual(context["schemaVersion"], 1)
            self.assertEqual(context["selection"]["totalInstances"], 2)
            self.assertEqual(context["resources"][0]["instanceId"], "prod-og")
            self.assertEqual(context["resources"][0]["selectionReason"], "auto")
            self.assertGreater(context["resources"][0]["matchScore"], 0)
            self.assertNotIn(
                "backup-og", {item["instanceId"] for item in context["resources"]}
            )
            database = context["resources"][0]["cluster"]["databases"][0]
            self.assertEqual(database["name"], "metrics")
            measurement = database["retentionPolicies"][0]["measurements"][0]
            self.assertEqual(measurement["name"], "cpu")

            evidence = store.list_evidence(run["id"])
            metadata_contexts = [
                item for item in evidence if item["kind"] == "metadata_context"
            ]
            self.assertEqual(len(metadata_contexts), 1)
            self.assertFalse(metadata_contexts[0]["final_allowed"])

            bound_workspace = store.create_workspace(
                "prod-og cpu usage timeout in metrics",
                "diagnose",
                "en-US",
                instance_id="backup-og",
                node_id="backup-og:data-2",
            )
            bound_run = store.create_run(bound_workspace["id"])
            AgentRuntime(settings, store).run_analysis(bound_workspace["id"], bound_run["id"])
            bound_context_response = task_mcp_response(
                settings,
                store,
                bound_run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 171,
                    "method": "resources/read",
                    "params": {"uri": f"logagent-v2://run/{bound_run['id']}/metadata_context"},
                },
            )
            bound_context = json.loads(
                bound_context_response["result"]["contents"][0]["text"]
            )
            self.assertEqual(bound_context["selection"]["mode"], "session_binding")
            self.assertEqual(bound_context["selection"]["boundInstanceId"], "backup-og")
            self.assertEqual(bound_context["selection"]["boundNodeId"], "backup-og:data-2")
            self.assertEqual(bound_context["resources"][0]["instanceId"], "backup-og")
            self.assertEqual(
                bound_context["resources"][0]["selectionReason"],
                "session_binding",
            )
            package_response = task_mcp_response(
                settings,
                store,
                bound_run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 172,
                    "method": "resources/read",
                    "params": {"uri": f"logagent-v2://run/{bound_run['id']}/analysis_package"},
                },
            )
            package = json.loads(package_response["result"]["contents"][0]["text"])
            self.assertEqual(package["workspace"]["instanceId"], "backup-og")
            self.assertEqual(package["workspace"]["nodeId"], "backup-og:data-2")

    def test_case_memory_manual_task_and_mcp_background_context(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(data_dir=Path(tmp), api_key="test")
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()

            manual = create_manual_case(
                store,
                {
                    "title": "Timeout during compaction",
                    "symptom": "query timeout and slow compaction",
                    "rootCause": "compaction backlog",
                    "solution": "reduce concurrent compactions",
                    "product": "opengemini",
                    "evidenceRefs": ["grep_results.json#matches/0"],
                },
            )
            search = store.search_cases("timeout compaction", limit=5)
            self.assertEqual(search[0]["caseId"], manual["caseId"])

            disabled = update_case_record(store, manual["caseId"], {"enabled": False})
            self.assertFalse(disabled["enabled"])
            self.assertEqual(store.search_cases("timeout", limit=5), [])
            self.assertEqual(
                store.search_cases("timeout", limit=5, include_disabled=True)[0]["caseId"],
                manual["caseId"],
            )

            workspace = store.create_workspace("why did the query timeout?", "diagnose", "en-US")
            artifact = write_artifact_bytes(
                settings,
                store,
                workspace["id"],
                "db.log",
                b"query timeout on shard 1\n",
                "text/plain",
            )
            store.create_upload(workspace["id"], "db.log", artifact["id"])
            run = store.create_run(workspace["id"])
            AgentRuntime(settings, store).run_analysis(workspace["id"], run["id"])
            task_case = create_task_case(
                store,
                run["id"],
                {"solution": "inspect shard load and compaction queue"},
            )
            self.assertEqual(task_case["sourceType"], "task")
            self.assertEqual(create_task_case(store, run["id"], {})["caseId"], task_case["caseId"])

            readonly_cases = readonly_mcp_response(
                settings,
                store,
                {
                    "jsonrpc": "2.0",
                    "id": 16,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.search_cases",
                        "arguments": {"query": "shard timeout", "limit": 5},
                    },
                },
            )
            readonly_body = json.loads(readonly_cases["result"]["content"][0]["text"])
            self.assertEqual(readonly_body["cases"][0]["caseId"], task_case["caseId"])

            task_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 17,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.search_cases",
                        "arguments": {"query": "timeout", "limit": 5},
                    },
                },
            )
            task_body = json.loads(task_response["result"]["content"][0]["text"])
            self.assertFalse(task_body["finalEvidenceAllowed"])
            evidence = store.list_evidence(run["id"])
            case_context = [item for item in evidence if item["kind"] == "case_context"]
            self.assertEqual(len(case_context), 1)
            self.assertFalse(case_context[0]["final_allowed"])
            context_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 171,
                    "method": "resources/read",
                    "params": {"uri": f"logagent-v2://run/{run['id']}/case_context"},
                },
            )
            context_body = json.loads(context_response["result"]["contents"][0]["text"])
            self.assertEqual(context_body["caseCount"], 1)
            self.assertEqual(context_body["cases"][0]["caseId"], task_case["caseId"])
            self.assertFalse(context_body["finalEvidenceAllowed"])
            case_answer = {
                "summary": "Historical Case evidence is citeable.",
                "symptoms": [],
                "likelyRootCauses": [
                    {
                        "cause": "The recalled Case matches the symptom.",
                        "evidenceRefs": [f"历史案例 {task_case['caseId']}"],
                    }
                ],
                "nextChecks": [],
                "fixSuggestions": [],
                "missingInformation": [],
                "confidence": "medium",
                "evidenceRefs": ["case_context.json#cases/0"],
            }
            case_validated = normalize_and_validate_final_answer(
                settings,
                store,
                run["id"],
                case_answer,
            )
            self.assertEqual(case_validated["evidenceRefs"], ["case_context.json#cases/0"])
            self.assertEqual(
                case_validated["likelyRootCauses"][0]["evidenceRefs"],
                ["case_context.json#cases/0"],
            )
            bad_case_answer = dict(case_answer, evidenceRefs=["case_context.json#cases/2"])
            with self.assertRaises(FinalAnswerValidationError):
                normalize_and_validate_final_answer(settings, store, run["id"], bad_case_answer)

            recall_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 18,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.recall_cases",
                        "arguments": {"query": "timeout", "limit": 5},
                    },
                },
            )
            recall_body = json.loads(recall_response["result"]["content"][0]["text"])
            self.assertEqual(recall_body["caseCount"], 1)
            self.assertEqual(recall_body["cases"][0]["caseId"], task_case["caseId"])
            self.assertTrue(recall_body["backgroundRef"].startswith("case_recall/recall_"))
            self.assertEqual(
                recall_body["evidenceRefs"],
                [f"{recall_body['artifactPath']}#cases/0"],
            )
            self.assertFalse(recall_body["finalEvidenceAllowed"])
            recall_context = [
                item
                for item in store.list_evidence(run["id"])
                if item["kind"] == "case_context"
                and item["payload"].get("tool") == "logagent.recall_cases"
            ]
            self.assertEqual(recall_context[0]["payload"]["path"], recall_body["artifactPath"])
            self.assertEqual(
                recall_context[0]["payload"]["backgroundRef"],
                recall_body["backgroundRef"],
            )

    def test_case_import_preview_confirm_and_search(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            store = Store(Path(tmp) / "logagent.sqlite")
            store.initialize()
            content = """
Title: Query timeout during compaction
Product: opengemini
Version: 1.3.0
Environment: staging
Instance ID: inst-a
Symptom:
Queries timed out while compaction backlog grew.
Root Cause:
Compaction workers fell behind after retention policy changes.
Solution:
Reduce concurrent writes and increase compaction throughput.
Evidence Refs:
grep_results.json#matches/0
"""
            preview = preview_case_import(store, content, filename="case.txt")
            case_import = preview["import"]
            self.assertEqual(case_import["status"], "previewed")
            self.assertEqual(case_import["filename"], "case.txt")
            self.assertEqual(case_import["validationErrors"], [])
            self.assertEqual(case_import["draft"]["product"], "opengemini")
            self.assertEqual(case_import["draft"]["instanceId"], "inst-a")

            listed = store.list_case_imports()
            self.assertEqual(listed[0]["importId"], case_import["importId"])
            confirmed = confirm_case_import(store, case_import["importId"])
            self.assertEqual(confirmed["import"]["status"], "confirmed")
            self.assertEqual(confirmed["case"]["sourceType"], "manual")
            self.assertEqual(
                confirmed["case"]["evidenceRefs"], ["grep_results.json#matches/0"]
            )
            self.assertEqual(
                store.search_cases("compaction timeout", limit=5)[0]["caseId"],
                confirmed["case"]["caseId"],
            )
            repeated = confirm_case_import(store, case_import["importId"])
            self.assertEqual(repeated["case"]["caseId"], confirmed["case"]["caseId"])

    def test_case_import_confirm_rejects_incomplete_draft(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            store = Store(Path(tmp) / "logagent.sqlite")
            store.initialize()
            preview = preview_case_import(store, "Only a symptom line")
            self.assertIn("rootCause is required", preview["import"]["validationErrors"])
            with self.assertRaises(ValueError):
                confirm_case_import(store, preview["import"]["importId"])

            patched = update_case_import_draft(
                store,
                preview["import"]["importId"],
                {
                    "rootCause": "Missing index caused slow query.",
                    "solution": "Create the missing index.",
                },
            )
            self.assertEqual(patched["import"]["validationErrors"], [])
            confirmed_from_patch = confirm_case_import(
                store,
                preview["import"]["importId"],
            )
            self.assertEqual(
                confirmed_from_patch["case"]["rootCause"],
                "Missing index caused slow query.",
            )
            with self.assertRaises(ValueError):
                update_case_import_draft(
                    store,
                    preview["import"]["importId"],
                    {"solution": "Change after confirm"},
                )

            message_preview = preview_case_import(store, "Only another symptom line")
            updated = append_case_import_message(
                store,
                message_preview["import"]["importId"],
                "Root Cause: Missing index caused slow query.\n"
                "Solution: Create the missing index.",
            )
            self.assertEqual(updated["import"]["messages"][0]["role"], "user")
            self.assertEqual(updated["import"]["validationErrors"], [])

            confirmed_from_message = confirm_case_import(
                store,
                message_preview["import"]["importId"],
            )
            self.assertEqual(
                confirmed_from_message["case"]["rootCause"],
                "Missing index caused slow query.",
            )

            second_preview = preview_case_import(store, "Only another symptom")
            completed = confirm_case_import(
                store,
                second_preview["import"]["importId"],
                {
                    "title": "Manual title",
                    "rootCause": "Missing index caused slow query.",
                    "solution": "Create the missing index.",
                },
            )
            self.assertEqual(completed["case"]["title"], "Manual title")
            self.assertEqual(completed["import"]["validationErrors"], [])

    def test_case_import_patch_route_updates_draft(self) -> None:
        from fastapi.testclient import TestClient
        from logagent_v2.api import create_app

        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(data_dir=Path(tmp), api_key="test", inline_worker=False)
            settings.ensure_dirs()
            headers = {"Authorization": "Bearer test"}

            with TestClient(create_app(settings)) as client:
                preview_response = client.post(
                    "/api/v2/cases/imports/preview",
                    headers=headers,
                    json={"content": "Title: Imported case\nSymptom: Query timeout"},
                )
                self.assertEqual(preview_response.status_code, 200)
                import_id = preview_response.json()["import"]["importId"]

                patch_response = client.patch(
                    f"/api/v2/cases/imports/{import_id}",
                    headers=headers,
                    json={
                        "rootCause": "Compaction backlog blocked reads.",
                        "solution": "Increase compaction workers.",
                        "evidenceRefs": ["grep_results.json#matches/0"],
                    },
                )
                self.assertEqual(patch_response.status_code, 200)
                patched_import = patch_response.json()["import"]
                self.assertEqual(patched_import["validationErrors"], [])
                self.assertEqual(
                    patched_import["draft"]["evidenceRefs"],
                    ["grep_results.json#matches/0"],
                )

                confirm_response = client.post(
                    f"/api/v2/cases/imports/{import_id}/confirm",
                    headers=headers,
                    json={},
                )
                self.assertEqual(confirm_response.status_code, 200)
                self.assertEqual(confirm_response.json()["case"]["sourceType"], "manual")

                rejected_patch = client.patch(
                    f"/api/v2/cases/imports/{import_id}",
                    headers=headers,
                    json={"solution": "Late edit"},
                )
                self.assertEqual(rejected_patch.status_code, 400)

    def test_case_search_uses_fts_and_updates_index(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            store = Store(Path(tmp) / "logagent.sqlite")
            store.initialize()
            shard_case = create_manual_case(
                store,
                {
                    "title": "Shard ownership changed",
                    "symptom": "query timeout after shard owner movement",
                    "rootCause": "pt owner moved during rebalance",
                    "solution": "verify pt view and rebalance state",
                },
            )
            create_manual_case(
                store,
                {
                    "title": "Backup retention issue",
                    "symptom": "backup lag",
                    "rootCause": "retention policy backlog",
                    "solution": "adjust retention workers",
                },
            )

            hits = store.search_cases("shard owner", limit=5)
            self.assertEqual(hits[0]["caseId"], shard_case["caseId"])
            self.assertEqual(hits[0]["searchBackend"], "hybrid")
            self.assertIn("vectorScore", hits[0])

            update_case_record(
                store,
                shard_case["caseId"],
                {
                    "title": "Backup worker issue",
                    "symptom": "backup lag",
                    "rootCause": "retention backlog",
                    "solution": "adjust retention workers",
                },
            )
            self.assertEqual(store.search_cases("shard owner", limit=5), [])

    def test_case_search_uses_local_vector_recall(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            store = Store(Path(tmp) / "logagent.sqlite")
            store.initialize()
            compaction_case = create_manual_case(
                store,
                {
                    "title": "Timeout during compaction",
                    "symptom": "query timeout and compaction backlog",
                    "rootCause": "compaction workers are saturated",
                    "solution": "reduce compaction pressure",
                },
            )
            hits = store.search_cases("timed out compactions", limit=5)
            self.assertEqual(hits[0]["caseId"], compaction_case["caseId"])
            self.assertEqual(hits[0]["searchBackend"], "vector")
            self.assertGreater(hits[0]["vectorScore"], 0)

    def test_skill_system_context_and_reference_mcp(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(data_dir=Path(tmp), api_key="test")
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            skill_dir = settings.skills_dir / "opengemini-diagnosis"
            (skill_dir / "references").mkdir(parents=True)
            (skill_dir / "SKILL.md").write_text(
                "---\n"
                "name: openGemini Diagnosis\n"
                "description: Diagnose openGemini logs.\n"
                "---\n\n"
                "Always ground conclusions in current task evidence.\n",
                encoding="utf-8",
            )
            (skill_dir / "references" / "topology.md").write_text(
                "PT ownership and shard topology reference.\n",
                encoding="utf-8",
            )
            (skill_dir / "logagent.json").write_text(
                json.dumps(
                    {
                        "schemaVersion": 1,
                        "displayName": "openGemini Diagnosis",
                        "includeByDefault": False,
                        "priority": 80,
                        "references": [
                            {
                                "referenceId": "topology",
                                "path": "references/topology.md",
                                "title": "Topology",
                                "summary": "Topology reference",
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )
            self.assertEqual(list_skills(settings)[0]["skillId"], "opengemini-diagnosis")

            readonly_skill = readonly_mcp_response(
                settings,
                store,
                {
                    "jsonrpc": "2.0",
                    "id": 17,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.get_skill",
                        "arguments": {"skillId": "opengemini-diagnosis"},
                    },
                },
            )
            readonly_skill_body = json.loads(readonly_skill["result"]["content"][0]["text"])
            self.assertEqual(readonly_skill_body["skill"]["skillId"], "opengemini-diagnosis")
            self.assertEqual(readonly_skill_body["skillId"], "opengemini-diagnosis")

            workspace = store.create_workspace(
                "explain shard ownership",
                "diagnose",
                "en-US",
                skill_ids=["opengemini-diagnosis"],
            )
            run = store.create_run(workspace["id"])
            AgentRuntime(settings, store).run_analysis(workspace["id"], run["id"])

            context_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 18,
                    "method": "resources/read",
                    "params": {"uri": f"logagent-v2://run/{run['id']}/system_context"},
                },
            )
            context = json.loads(context_response["result"]["contents"][0]["text"])
            self.assertEqual(context["resources"][0]["skillId"], "opengemini-diagnosis")
            self.assertEqual(context["resources"][0]["references"][0]["referenceId"], "topology")

            task_ref = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 19,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.get_skill_reference",
                        "arguments": {
                            "skillId": "opengemini-diagnosis",
                            "referenceId": "topology",
                        },
                    },
                },
            )
            ref_body = json.loads(task_ref["result"]["content"][0]["text"])
            self.assertIn("shard topology", ref_body["content"])
            self.assertTrue(ref_body["artifactPath"].startswith("skill_references/skill_ref_"))
            self.assertTrue(ref_body["backgroundRef"].startswith("skill_references/"))
            self.assertEqual(ref_body["backgroundRef"], f"{ref_body['artifactPath']}#content")
            self.assertEqual(ref_body["canonicalRef"], ref_body["backgroundRef"])
            self.assertEqual(ref_body["evidenceRefs"], [ref_body["backgroundRef"]])
            self.assertFalse(ref_body["finalEvidenceAllowed"])
            self.assertEqual(
                ref_body["skillRevision"], context["resources"][0]["revision"]
            )
            self.assertEqual(ref_body["referenceId"], "topology")
            self.assertEqual(ref_body["path"], "references/topology.md")
            self.assertEqual(ref_body["title"], "Topology")
            self.assertEqual(ref_body["summary"], "Topology reference")
            self.assertFalse(ref_body["truncated"])
            evidence = store.list_evidence(run["id"])
            skill_refs = [item for item in evidence if item["kind"] == "skill_reference"]
            self.assertEqual(len(skill_refs), 1)
            self.assertFalse(skill_refs[0]["final_allowed"])
            self.assertEqual(skill_refs[0]["payload"]["path"], ref_body["artifactPath"])
            self.assertEqual(
                skill_refs[0]["payload"]["backgroundRef"], ref_body["backgroundRef"]
            )

            readonly_ref = readonly_mcp_response(
                settings,
                store,
                {
                    "jsonrpc": "2.0",
                    "id": 20,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.get_skill_reference",
                        "arguments": {
                            "skillId": "opengemini-diagnosis",
                            "path": "references/topology.md",
                        },
                    },
                },
            )
            readonly_body = json.loads(readonly_ref["result"]["content"][0]["text"])
            self.assertFalse(readonly_body["finalEvidenceAllowed"])
            self.assertIn("PT ownership", readonly_body["content"])

            readonly_resources = readonly_mcp_response(
                settings,
                store,
                {"jsonrpc": "2.0", "id": 21, "method": "resources/list"},
            )
            readonly_resource_uris = {
                item["uri"] for item in readonly_resources["result"]["resources"]
            }
            self.assertIn("logagent://skills/opengemini-diagnosis", readonly_resource_uris)
            self.assertIn(
                "logagent-v2://skills/opengemini-diagnosis",
                readonly_resource_uris,
            )

    def test_system_context_auto_matches_skills_from_question(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(data_dir=Path(tmp), api_key="test")
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            matched = import_skill(
                settings=settings,
                skill_id="opengemini-topology",
                name="openGemini Topology",
                description="Diagnose shard ownership and PT movement.",
                markdown="Use topology evidence for shard and PT ownership questions.",
            )
            matched_dir = settings.skills_dir / matched["skillId"]
            manifest = json.loads((matched_dir / "logagent.json").read_text(encoding="utf-8"))
            manifest.update(
                {
                    "products": ["openGemini"],
                    "keywords": ["shard ownership", "pt movement"],
                    "priority": 50,
                }
            )
            (matched_dir / "logagent.json").write_text(
                json.dumps(manifest, ensure_ascii=True, indent=2),
                encoding="utf-8",
            )
            import_skill(
                settings=settings,
                skill_id="unrelated",
                name="Unrelated",
                description="Unrelated database backup guidance.",
                markdown="Only backup guidance.",
            )

            workspace = store.create_workspace(
                "openGemini shard ownership changed after rebalance",
                "diagnose",
                "en-US",
            )
            run = store.create_run(workspace["id"])
            AgentRuntime(settings, store).run_analysis(workspace["id"], run["id"])
            context_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 26,
                    "method": "resources/read",
                    "params": {"uri": f"logagent-v2://run/{run['id']}/system_context"},
                },
            )
            context = json.loads(context_response["result"]["contents"][0]["text"])
            resources = {item["skillId"]: item for item in context["resources"]}
            self.assertIn("opengemini-topology", resources)
            self.assertEqual(resources["opengemini-topology"]["selectionReason"], "auto")
            self.assertGreater(resources["opengemini-topology"]["matchScore"], 0)
            self.assertNotIn("unrelated", resources)

    def test_legacy_system_context_resources_versions_and_preview(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            store = Store(Path(tmp) / "logagent.sqlite")
            store.initialize()
            resource = create_system_context_resource(
                store,
                {
                    "kind": "runbook",
                    "title": "Compaction runbook",
                    "description": "Triage compaction timeout cases.",
                    "scope": "log_analysis",
                    "enabled": True,
                    "tags": ["compaction", "timeout", "compaction"],
                    "product": "openGemini",
                    "contentType": "markdown",
                    "content": "Check compaction backlog before changing query limits.",
                    "summary": "Compaction triage",
                    "promptPolicy": {
                        "includeByDefault": False,
                        "priority": 20,
                        "maxChars": 2000,
                    },
                },
            )
            context_id = resource["contextId"]
            first_version_id = resource["activeVersionId"]

            summaries = list_system_context_resource_summaries(store)
            self.assertEqual(summaries[0]["contextId"], context_id)
            self.assertEqual(summaries[0]["tags"], ["compaction", "timeout"])
            self.assertEqual(
                preview_system_context_resources(store)["resources"],
                [],
            )

            explicit_preview = preview_system_context_resources(
                store,
                context_ids=[context_id],
                product="openGemini",
            )
            self.assertEqual(explicit_preview["resources"][0]["contextId"], context_id)
            self.assertIn("Check compaction backlog", explicit_preview["prompt"])

            with_draft = create_system_context_version(
                store,
                context_id,
                {
                    "contentType": "plain_text",
                    "content": "Use compaction queue and shard ownership evidence.",
                    "summary": "Updated compaction triage",
                    "activate": False,
                },
            )
            second_version_id = with_draft["versions"][-1]["versionId"]
            self.assertEqual(with_draft["activeVersionId"], first_version_id)

            activated = activate_system_context_version(store, context_id, second_version_id)
            versions = {item["versionId"]: item for item in activated["versions"]}
            self.assertEqual(activated["activeVersionId"], second_version_id)
            self.assertEqual(versions[first_version_id]["status"], "archived")
            self.assertEqual(versions[second_version_id]["status"], "active")

            metadata_snapshot = {
                "instance": {
                    "product": "openGemini",
                    "version": "1.4",
                    "environment": "prod",
                },
                "cluster": {
                    "nodes": [{"nodeId": "n1"}],
                    "databases": [{"name": "db0"}],
                },
            }
            store.upsert_metadata_instance(
                "prod-og",
                "production",
                "json",
                metadata_snapshot,
                metadata_snapshot,
            )
            all_summaries = {
                item["contextId"]: item for item in list_system_context_resource_summaries(store)
            }
            self.assertIn("meta_prod-og", all_summaries)
            metadata_preview = preview_system_context_resources(
                store,
                context_ids=["meta_prod-og"],
            )
            self.assertEqual(metadata_preview["resources"][0]["kind"], "metadata_instance")
            self.assertIn("Metadata adapter", metadata_preview["prompt"])

    def test_readonly_mcp_preview_system_context_accepts_metadata_filters(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(data_dir=Path(tmp), api_key="test")
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            resource = create_system_context_resource(
                store,
                {
                    "kind": "runbook",
                    "title": "openGemini compaction",
                    "description": "Compaction guidance.",
                    "scope": "log_analysis",
                    "enabled": True,
                    "product": "openGemini",
                    "contentType": "markdown",
                    "content": "Check compaction backlog.",
                    "summary": "Compaction",
                },
            )
            import_metadata(
                store,
                "prod-og",
                "json",
                json.dumps(
                    {
                        "instance": {
                            "product": "openGemini",
                            "version": "1.4",
                            "environment": "prod",
                        },
                        "cluster": {
                            "nodes": [{"nodeId": "n1"}],
                            "databases": [{"name": "db0"}],
                        },
                    }
                ),
                remark="production",
            )

            tools = readonly_mcp_response(
                settings,
                store,
                {"jsonrpc": "2.0", "id": 21, "method": "tools/list"},
            )
            preview_tool = next(
                item
                for item in tools["result"]["tools"]
                if item["name"] == "logagent.preview_system_context"
            )
            self.assertIn("instanceId", preview_tool["inputSchema"]["properties"])
            response = readonly_mcp_response(
                settings,
                store,
                {
                    "jsonrpc": "2.0",
                    "id": 22,
                    "method": "tools/call",
                    "params": {
                        "name": "logagent.preview_system_context",
                        "arguments": {
                            "product": "openGemini",
                            "instanceId": "prod-og",
                        },
                    },
                },
            )
            payload = json.loads(response["result"]["content"][0]["text"])
            context_ids = {item["contextId"] for item in payload["systemResources"]}
            self.assertIn(resource["contextId"], context_ids)
            self.assertIn("meta_prod-og", context_ids)
            self.assertIn("Check compaction backlog", payload["prompt"])
            self.assertIn("Metadata adapter", payload["prompt"])

    def test_session_system_context_ids_materialize_into_run_context(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(data_dir=Path(tmp), api_key="test")
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            resource = create_system_context_resource(
                store,
                {
                    "kind": "runbook",
                    "title": "Compaction runbook",
                    "scope": "log_analysis",
                    "contentType": "markdown",
                    "content": "Check compaction backlog before changing query limits.",
                    "summary": "Compaction triage",
                    "promptPolicy": {"includeByDefault": False},
                },
            )
            workspace = store.create_workspace(
                "compaction timeout",
                "diagnose",
                "en-US",
                system_context_ids=[resource["contextId"]],
            )
            run = store.create_run(workspace["id"])
            AgentRuntime(settings, store).run_analysis(workspace["id"], run["id"])
            context_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 23,
                    "method": "resources/read",
                    "params": {"uri": f"logagent-v2://run/{run['id']}/system_context"},
                },
            )
            context = json.loads(context_response["result"]["contents"][0]["text"])
            system_context_ids = {item["contextId"] for item in context["systemResources"]}
            self.assertIn(resource["contextId"], system_context_ids)
            self.assertIn("Check compaction backlog", context["prompt"])

            package_response = task_mcp_response(
                settings,
                store,
                run["id"],
                {
                    "jsonrpc": "2.0",
                    "id": 24,
                    "method": "resources/read",
                    "params": {"uri": f"logagent-v2://run/{run['id']}/analysis_package"},
                },
            )
            package = json.loads(package_response["result"]["contents"][0]["text"])
            self.assertEqual(package["systemContext"]["systemResourceCount"], 1)
            self.assertEqual(
                package["systemContext"]["systemResources"][0]["contextId"],
                resource["contextId"],
            )

    def test_legacy_system_context_resource_api_smoke(self) -> None:
        from fastapi.testclient import TestClient
        from logagent_v2.api import create_app

        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(data_dir=Path(tmp), api_key="test", inline_worker=False)
            headers = {"Authorization": "Bearer test"}
            with TestClient(create_app(settings)) as client:
                created = client.post(
                    "/api/v2/system-context/resources",
                    headers=headers,
                    json={
                        "kind": "knowledge_note",
                        "title": "API note",
                        "scope": "global",
                        "contentType": "markdown",
                        "content": "Use API-created context in previews.",
                        "promptPolicy": {"includeByDefault": False, "priority": 10},
                    },
                )
                self.assertEqual(created.status_code, 201)
                context_id = created.json()["contextId"]

                listed = client.get("/api/v2/system-context/resources", headers=headers)
                self.assertEqual(listed.status_code, 200)
                self.assertEqual(listed.json()["resources"][0]["contextId"], context_id)

                new_version = client.post(
                    f"/api/v2/system-context/resources/{context_id}/versions",
                    headers=headers,
                    json={
                        "contentType": "plain_text",
                        "content": "Second version content.",
                        "activate": False,
                    },
                )
                self.assertEqual(new_version.status_code, 201)
                version_id = new_version.json()["versions"][-1]["versionId"]

                activated = client.post(
                    f"/api/v2/system-context/resources/{context_id}/versions/"
                    f"{version_id}/activate",
                    headers=headers,
                )
                self.assertEqual(activated.status_code, 200)
                self.assertEqual(activated.json()["activeVersionId"], version_id)

                preview = client.post(
                    "/api/v2/system-context/preview",
                    headers=headers,
                    json={"contextIds": [context_id], "taskKind": "log_analysis"},
                )
                self.assertEqual(preview.status_code, 200)
                self.assertIn("Second version content.", preview.json()["prompt"])

    def test_skills_zip_exports_regular_files_and_skips_symlinks(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(data_dir=Path(tmp), api_key="test")
            settings.ensure_dirs()
            skill = import_skill(
                settings=settings,
                skill_id="export-skill",
                name="Export Skill",
                description="Export test skill.",
                markdown="Use exported references.",
            )
            skill_dir = settings.skills_dir / skill["skillId"]
            (skill_dir / "references").mkdir()
            (skill_dir / "references" / "note.md").write_text(
                "reference note\n", encoding="utf-8"
            )
            outside = Path(tmp) / "outside-secret.md"
            outside.write_text("secret\n", encoding="utf-8")
            try:
                (skill_dir / "references" / "linked.md").symlink_to(outside)
            except OSError:
                pass

            archive_bytes = build_skills_zip(settings)
            with zipfile.ZipFile(io.BytesIO(archive_bytes)) as archive:
                names = set(archive.namelist())
                self.assertIn("export-skill/SKILL.md", names)
                self.assertIn("export-skill/logagent.json", names)
                self.assertIn("export-skill/references/note.md", names)
                self.assertNotIn("export-skill/references/linked.md", names)
                manifest = json.loads(archive.read("manifest.json").decode("utf-8"))

            self.assertEqual(manifest["schemaVersion"], 1)
            self.assertEqual(manifest["skills"][0]["skillId"], "export-skill")
            files = manifest["skills"][0]["files"]
            self.assertTrue(any(item["path"] == "SKILL.md" for item in files))
            self.assertTrue(all(item["path"] != "references/linked.md" for item in files))

    def test_tools_zip_packages_executable_and_marks_missing_skipped(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            executable = root / "fake-tool"
            executable.write_text("#!/usr/bin/env sh\necho ok\n", encoding="utf-8")
            executable.chmod(0o755)
            settings = Settings(
                data_dir=root / "data",
                api_key="test",
                tools=(
                    ToolDefinition(
                        id="fake_tool",
                        display_name="Fake Tool",
                        command=executable.as_posix(),
                        args=("--input", "{input_file}"),
                        max_input_files=2,
                        match_file_patterns=("*.log",),
                        match_keywords=("panic",),
                    ),
                    ToolDefinition(
                        id="missing_tool",
                        display_name="Missing Tool",
                        command=(root / "missing-tool").as_posix(),
                    ),
                    ToolDefinition(
                        id="disabled_tool",
                        display_name="Disabled Tool",
                        command=executable.as_posix(),
                        enabled=False,
                    ),
                ),
            )

            archive_bytes = build_tools_zip(settings)
            with zipfile.ZipFile(io.BytesIO(archive_bytes)) as archive:
                names = set(archive.namelist())
                self.assertIn("README.md", names)
                self.assertIn("bin/fake_tool/fake-tool", names)
                self.assertIn("wrappers/fake_tool.sh", names)
                binary_mode = (archive.getinfo("bin/fake_tool/fake-tool").external_attr >> 16)
                wrapper_mode = (archive.getinfo("wrappers/fake_tool.sh").external_attr >> 16)
                self.assertEqual(binary_mode & 0o777, 0o755)
                self.assertEqual(wrapper_mode & 0o777, 0o755)
                self.assertIn("config/examples/fake_tool.yaml", names)
                self.assertIn("config/examples/missing_tool.yaml", names)
                self.assertNotIn("bin/missing_tool/missing-tool", names)
                self.assertNotIn("config/examples/disabled_tool.yaml", names)
                manifest = json.loads(archive.read("tools-manifest.json").decode("utf-8"))

            tools = {item["toolId"]: item for item in manifest["tools"]}
            self.assertTrue(tools["fake_tool"]["packaged"])
            self.assertFalse(tools["fake_tool"]["skipped"])
            self.assertEqual(tools["fake_tool"]["configuredArgs"], ["--input", "{input_file}"])
            self.assertFalse(tools["missing_tool"]["packaged"])
            self.assertTrue(tools["missing_tool"]["skipped"])
            self.assertIn("regular file", tools["missing_tool"]["skipReason"])
            self.assertNotIn("disabled_tool", tools)

    def test_tools_zip_exports_enabled_pprof_go_command(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            fake_go = root / "fake-go"
            fake_go.write_text("#!/usr/bin/env sh\necho go\n", encoding="utf-8")
            fake_go.chmod(0o755)
            settings = Settings(
                data_dir=root / "data",
                api_key="test",
                pprof_enabled=True,
                pprof_go_command=fake_go.as_posix(),
            )

            archive_bytes = build_tools_zip(settings)
            with zipfile.ZipFile(io.BytesIO(archive_bytes)) as archive:
                names = set(archive.namelist())
                self.assertIn("bin/pprof_analyzer/fake-go", names)
                self.assertIn("wrappers/pprof_analyzer.sh", names)
                self.assertIn("config/examples/pprof_analyzer.yaml", names)
                manifest = json.loads(archive.read("tools-manifest.json").decode("utf-8"))

            tools = {item["toolId"]: item for item in manifest["tools"]}
            self.assertTrue(tools["pprof_analyzer"]["packaged"])
            self.assertFalse(tools["pprof_analyzer"]["skipped"])
            self.assertEqual(tools["pprof_analyzer"]["configuredArgs"], [])
            self.assertEqual(
                tools["pprof_analyzer"]["matchRules"]["filePatterns"],
                ["*.pprof", "*.prof", "*.profile", "*.pb.gz"],
            )

    def test_v2_settings_summaries_and_debug_toggle(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(data_dir=Path(tmp), api_key="test")

            summary = llm_settings_summary(settings)
            self.assertEqual(summary["provider"], "stub")
            self.assertEqual(summary["configuredModel"], "stub")
            self.assertEqual(summary["maxOutputTokens"], 2048)
            self.assertFalse(summary["baseUrlConfigured"])

            models = list_agent_models(settings)
            self.assertEqual(models["models"], ["stub"])
            chat = test_agent_chat(settings, "hello")
            self.assertEqual(chat["provider"], "stub")
            self.assertIn("hello", chat["response"])

            backends = agent_backends_summary(settings)
            self.assertEqual(backends["defaultBackend"], "logagent_v2_agent")
            self.assertEqual(backends["backends"][0]["backendType"], "langgraph_oriented_agent")
            diagnostic = agent_backend_diagnostic(settings, "logagent_v2_agent")
            self.assertEqual(diagnostic["status"], "configured")
            self.assertTrue(diagnostic["details"])

            self.assertFalse(debug_log_responses())
            self.assertTrue(set_debug_log_responses(True))
            self.assertTrue(debug_log_responses())
            self.assertFalse(set_debug_log_responses(False))

    def test_v2_settings_report_openai_configuration_errors(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(
                data_dir=Path(tmp),
                api_key="test",
                agent_provider="openai_compatible",
                agent_base_url="http://127.0.0.1:1/v1",
                agent_model=None,
            )

            response = test_response(lambda: agent_backend_diagnostic(settings, "logagent_v2_agent"))
            self.assertFalse(response["ok"])
            self.assertIn("LOGAGENT_V2_AGENT_MODEL", response["error"])

    def test_v2_settings_binary_provider_diagnostics_and_chat(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            binary = root / "settings-binary-provider"
            answer = {
                "summary": "binary settings ok",
                "symptoms": [],
                "likelyRootCauses": [],
                "nextChecks": [],
                "fixSuggestions": [],
                "missingInformation": [],
                "confidence": "low",
                "evidenceRefs": [],
            }
            binary.write_text(
                "#!/usr/bin/env python3\n"
                "import json, sys\n"
                "if sys.argv[1] != 'run':\n"
                "    raise SystemExit(2)\n"
                f"print(json.dumps({json.dumps(answer)}))\n",
                encoding="utf-8",
            )
            binary.chmod(0o755)
            settings = Settings(
                data_dir=root / "data",
                api_key="test",
                agent_provider="binary",
                agent_model=None,
                agent_binary_path=binary,
            )

            summary = llm_settings_summary(settings)
            self.assertEqual(summary["provider"], "binary")
            self.assertEqual(summary["configuredModel"], "binary-reserved")
            self.assertTrue(summary["binaryPathConfigured"])
            self.assertEqual(list_agent_models(settings)["models"], ["binary-reserved"])
            diagnostic = agent_backend_diagnostic(settings, "logagent_v2_agent")
            self.assertEqual(diagnostic["status"], "configured")
            self.assertTrue(agent_backends_summary(settings)["backends"][0]["commandConfigured"])
            chat = test_agent_chat(settings, "hello")
            self.assertEqual(chat["provider"], "binary")
            self.assertEqual(chat["response"], "binary settings ok")

    def test_v2_domain_adapters_are_exposed_in_readonly_mcp(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            settings = Settings(data_dir=Path(tmp), api_key="test")
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()

            adapters = domain_adapter_summaries()
            self.assertEqual([item["id"] for item in adapters], [
                "opengemini_influxdb",
                "cassandra",
                "rocksdb",
            ])
            self.assertEqual(adapters[0]["status"], "active")
            self.assertEqual(adapters[1]["status"], "skeleton")

            resources = readonly_mcp_response(
                settings,
                store,
                {"jsonrpc": "2.0", "id": 1, "method": "resources/list"},
            )
            batch = readonly_mcp_response(
                settings,
                store,
                [
                    {"jsonrpc": "2.0", "id": 101, "method": "initialize"},
                    {"jsonrpc": "2.0", "id": 102, "method": "resources/list"},
                ],
            )
            self.assertIsInstance(batch, list)
            self.assertEqual([item["id"] for item in batch], [101, 102])
            self.assertEqual(batch[0]["result"]["serverInfo"]["name"], "logagent-v2-readonly")
            ping = readonly_mcp_response(
                settings,
                store,
                {"jsonrpc": "2.0", "id": 103, "method": "ping"},
            )
            self.assertEqual(ping["result"], {})
            prompts = readonly_mcp_response(
                settings,
                store,
                {"jsonrpc": "2.0", "id": 104, "method": "prompts/list"},
            )
            self.assertEqual(prompts["result"]["prompts"], [])
            resource_uris = {
                item["uri"] for item in resources["result"]["resources"]
            }
            self.assertIn("logagent://domain-adapters", resource_uris)
            self.assertIn("logagent-v2://domain-adapters", resource_uris)

            alias_resource = readonly_mcp_response(
                settings,
                store,
                {
                    "jsonrpc": "2.0",
                    "id": 11,
                    "method": "resources/read",
                    "params": {"uri": "logagent://domain-adapters"},
                },
            )
            alias_payload = json.loads(alias_resource["result"]["contents"][0]["text"])
            self.assertEqual(alias_payload["domainAdapters"][0]["id"], "opengemini_influxdb")

            tool_call = readonly_mcp_response(
                settings,
                store,
                {
                    "jsonrpc": "2.0",
                    "id": 2,
                    "method": "tools/call",
                    "params": {"name": "logagent.list_domain_adapters", "arguments": {}},
                },
            )
            payload = json.loads(tool_call["result"]["content"][0]["text"])
            self.assertEqual(payload["domainAdapters"][0]["id"], "opengemini_influxdb")

    def test_v2_remote_executor_run_uses_queued_fake_ssh(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            fake_ssh = root / "fake-ssh"
            fake_ssh.write_text(
                "#!/usr/bin/env python3\n"
                "import json, sys\n"
                "print(json.dumps(sys.argv[1:]))\n",
                encoding="utf-8",
            )
            fake_ssh.chmod(0o755)
            settings = Settings(
                data_dir=root / "data",
                api_key="test",
                remote_ssh_command=fake_ssh.as_posix(),
                remote_commands=(
                    RemoteCommandTemplate(
                        command_id="smoke_ls_root",
                        display_name="Smoke",
                        description="test",
                        argv=("ls", "-la", "/root"),
                        timeout_seconds=5,
                    ),
                ),
            )
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()

            executor = store.create_remote_executor(
                {
                    "name": "local fake",
                    "host": "127.0.0.1",
                    "port": 2222,
                    "user": "root",
                    "tags": ["test"],
                    "enabled": True,
                    "notes": "fake ssh",
                }
            )
            self.assertEqual(executor["executorId"][:9], "executor_")
            template_descriptor = command_templates(settings)[0]
            self.assertEqual(template_descriptor["commandId"], "smoke_ls_root")
            self.assertEqual(template_descriptor["enabled"], True)
            self.assertEqual(template_descriptor["timeoutSeconds"], 5)

            run = store.create_remote_run(
                executor["executorId"],
                "smoke_ls_root",
                "Smoke on local fake",
                idempotency_key="remote-idem-1",
            )
            same = store.create_remote_run(
                executor["executorId"],
                "smoke_ls_root",
                "Smoke on local fake",
                idempotency_key="remote-idem-1",
            )
            self.assertEqual(same["taskId"], run["taskId"])
            jobs = store.acquire_jobs("test-worker", limit=1)
            self.assertEqual(jobs[0]["kind"], "remote_command_run")

            asyncio.run(JobRunner(settings, store).process_job(jobs[0]))

            finished = store.get_remote_run(run["taskId"])
            self.assertEqual(finished["status"], "SUCCEEDED")
            self.assertEqual(finished["phase"], "FINISHED")
            self.assertEqual(finished["result"]["result"]["status"], "OK")
            stdout_preview = finished["result"]["result"]["stdoutPreview"]
            self.assertIn("root@127.0.0.1", stdout_preview)
            self.assertIn("ls", stdout_preview)
            result_path = settings.data_dir / finished["result"]["resultPath"]
            self.assertTrue(result_path.exists())

    def test_remote_command_template_descriptors_include_global_enabled_and_timeout(self) -> None:
        settings = Settings(
            data_dir=Path("/tmp/logagent-v2-test-unused"),
            api_key="test",
            remote_execution_enabled=False,
            remote_command_timeout_seconds=44,
            remote_commands=(
                RemoteCommandTemplate(
                    command_id="smoke",
                    display_name="Smoke",
                    description="disabled globally",
                    argv=("true",),
                ),
            ),
        )

        descriptor = command_templates(settings)[0]
        self.assertEqual(descriptor["enabled"], False)
        self.assertEqual(descriptor["timeoutSeconds"], 44)


if __name__ == "__main__":
    unittest.main()
