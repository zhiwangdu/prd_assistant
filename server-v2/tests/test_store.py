from __future__ import annotations

import base64
import gzip
import io
import json
import sys
import tarfile
import tempfile
import threading
import unittest
import zipfile
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

from logagent_v2.agent import AgentRuntime
from logagent_v2.artifacts import (
    resolve_artifact_path,
    safe_filename,
    write_artifact_bytes,
    write_artifact_file,
)
from logagent_v2.case_memory import (
    confirm_case_import,
    create_manual_case,
    create_task_case,
    preview_case_import,
    update_case as update_case_record,
)
from logagent_v2.config import Settings, ToolDefinition
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
    preview_metadata_import,
    preview_metadata_import_from_url,
    query_field_types,
)
from logagent_v2.results import get_run_result
from logagent_v2.skills import import_skill, list_skills
from logagent_v2.store import Store
from logagent_v2.tools import (
    findings_from_stdout,
    parse_json,
    summary_from_stdout,
    tool_descriptors,
)


class StoreTests(unittest.TestCase):
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
                    "params": {"uri": f"logagent-v2://run/{run['id']}/analysis_package"},
                },
            )
            package = json.loads(package_response["result"]["contents"][0]["text"])
            self.assertEqual(package["workspace"]["question"], "why did the query timeout?")
            self.assertEqual(package["manifest"]["fileCount"], 1)
            self.assertEqual(package["allowedEvidenceRefs"], ["grep_results.json#matches/0"])
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
            self.assertEqual(request_doc["allowedEvidenceRefs"], ["grep_results.json#matches/0"])
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
            events = store.list_timeline(run["id"])
            self.assertTrue(any(event["kind"] == "evidence.created" for event in events))
            self.assertTrue(any(event["kind"] == "run.succeeded" for event in events))

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
            names = {item["name"] for item in listed["result"]["resources"]}
            self.assertIn("manifest", names)
            self.assertIn("grep_results", names)

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
            settings = Settings(data_dir=Path(tmp), api_key="test", tools=(tool,))
            settings.ensure_dirs()
            store = Store(settings.sqlite_path)
            store.initialize()
            workspace = store.create_workspace("run tool", "diagnose", "en-US")
            run = store.create_run(workspace["id"])

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
            evidence = store.list_evidence(run["id"])
            self.assertTrue(any(item["kind"] == "tool_result" for item in evidence))

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
            evidence = [
                item
                for item in store.list_evidence(run["id"])
                if item["kind"] == "tool_result"
            ]
            self.assertEqual(len(evidence), 2)
            self.assertNotEqual(
                evidence[0]["payload"]["actionId"], evidence[1]["payload"]["actionId"]
            )

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
                self.assertEqual(
                    listed["endpoints"][0]["headers"]["Authorization"], "__REDACTED__"
                )
                self.assertIn("api_key=__REDACTED__", listed["endpoints"][0]["url"])
                self.assertEqual(
                    listed["endpoints"][0]["bodyPreview"],
                    "password=__REDACTED__&keep=value",
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
                ok_endpoint = store.create_fetch_endpoint(
                    name="redirect ok",
                    method="GET",
                    url=f"http://127.0.0.1:{server.server_port}/redirect-ok",
                    headers={"Authorization": "Bearer secret"},
                    body=None,
                    enabled=True,
                )
                blocked_endpoint = store.create_fetch_endpoint(
                    name="redirect blocked",
                    method="GET",
                    url=f"http://127.0.0.1:{server.server_port}/redirect-blocked",
                    headers={"Authorization": "Bearer secret"},
                    body=None,
                    enabled=True,
                )
                workspace = store.create_workspace("fetch redirect", "diagnose", "en-US")
                run = store.create_run(workspace["id"])

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
                            "question": "Which version?",
                            "reason": "version affects diagnostics",
                        },
                    },
                },
            )
            prompt_payload = json.loads(prompt_response["result"]["content"][0]["text"])
            prompt_action = store.get_action(prompt_payload["action"]["id"])
            self.assertEqual(prompt_action["kind"], "user_input")
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
            self.assertEqual(store.get_run(run["id"])["status"], "waiting_for_approval")

            decided = store.decide_action(approval["id"], "approved", "ok")
            self.assertEqual(decided["status"], "approved")
            self.assertEqual(decided["result"]["decision"], "approved")

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
            evidence = store.list_evidence(run["id"])
            metadata_slices = [item for item in evidence if item["kind"] == "metadata_slice"]
            self.assertEqual(len(metadata_slices), 1)
            self.assertFalse(metadata_slices[0]["final_allowed"])

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

            completed = confirm_case_import(
                store,
                preview["import"]["importId"],
                {
                    "title": "Manual title",
                    "rootCause": "Missing index caused slow query.",
                    "solution": "Create the missing index.",
                },
            )
            self.assertEqual(completed["case"]["title"], "Manual title")
            self.assertEqual(completed["import"]["validationErrors"], [])

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
            self.assertTrue(ref_body["backgroundRef"].startswith("skill_references/"))
            evidence = store.list_evidence(run["id"])
            skill_refs = [item for item in evidence if item["kind"] == "skill_reference"]
            self.assertEqual(len(skill_refs), 1)
            self.assertFalse(skill_refs[0]["final_allowed"])

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


if __name__ == "__main__":
    unittest.main()
