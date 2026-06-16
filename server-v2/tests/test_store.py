from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from logagent_v2.agent import AgentRuntime
from logagent_v2.store import Store


class StoreTests(unittest.TestCase):
    def test_workspace_run_job_and_stub_agent(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            store = Store(Path(tmp) / "logagent.sqlite")
            store.initialize()

            workspace = store.create_workspace("why did the query timeout?", "diagnose", "en-US")
            run = store.create_run(workspace["id"])
            jobs = store.acquire_jobs("test-worker", limit=1)

            self.assertEqual(len(jobs), 1)
            self.assertEqual(jobs[0]["kind"], "run_analysis")

            AgentRuntime(store).run_analysis(workspace["id"], run["id"])
            store.complete_job(jobs[0]["id"])

            finished = store.get_run(run["id"])
            self.assertEqual(finished["status"], "succeeded")
            self.assertEqual(finished["phase"], "finish")
            self.assertEqual(finished["finalAnswer"]["confidence"], "low")

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


if __name__ == "__main__":
    unittest.main()

