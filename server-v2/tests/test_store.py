from __future__ import annotations

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
from logagent_v2.artifacts import resolve_artifact_path, write_artifact_bytes
from logagent_v2.case_memory import (
    create_manual_case,
    create_task_case,
    update_case as update_case_record,
)
from logagent_v2.config import Settings, ToolDefinition
from logagent_v2.exports import build_skills_zip, build_tools_zip
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
from logagent_v2.skills import import_skill, list_skills
from logagent_v2.store import Store


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
            events = store.list_timeline(run["id"])
            self.assertTrue(any(event["kind"] == "evidence.created" for event in events))
            self.assertTrue(any(event["kind"] == "run.succeeded" for event in events))

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
