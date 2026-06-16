from __future__ import annotations

import tempfile
import unittest
import zipfile
import json
import sys
from pathlib import Path

from logagent_v2.agent import AgentRuntime
from logagent_v2.artifacts import write_artifact_bytes
from logagent_v2.config import Settings, ToolDefinition
from logagent_v2.mcp import task_mcp_response
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


if __name__ == "__main__":
    unittest.main()
