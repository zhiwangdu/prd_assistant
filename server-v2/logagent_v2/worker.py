from __future__ import annotations

import asyncio
import socket
from contextlib import suppress

from .agent import AgentRuntime
from .config import Settings
from .remote_execution import execute_remote_command_run
from .store import JsonObject, Store


class JobRunner:
    def __init__(self, settings: Settings, store: Store):
        self.settings = settings
        self.store = store
        self.worker_id = f"{socket.gethostname()}:{id(self)}"
        self._task: asyncio.Task[None] | None = None
        self._stopping = asyncio.Event()

    async def start(self) -> None:
        if self._task is None:
            self._task = asyncio.create_task(self.run_forever())

    async def stop(self) -> None:
        self._stopping.set()
        if self._task is not None:
            self._task.cancel()
            with suppress(asyncio.CancelledError):
                await self._task

    async def run_forever(self) -> None:
        while not self._stopping.is_set():
            jobs = self.store.acquire_jobs(
                self.worker_id, limit=max(1, self.settings.max_concurrent_jobs)
            )
            if not jobs:
                await asyncio.sleep(self.settings.job_poll_seconds)
                continue
            await asyncio.gather(*(self.process_job(job) for job in jobs))

    async def process_job(self, job: JsonObject) -> None:
        try:
            if job["kind"] == "run_analysis":
                payload = job["payload"]
                AgentRuntime(self.settings, self.store).run_analysis(
                    payload["workspace_id"], payload["run_id"]
                )
            elif job["kind"] == "remote_command_run":
                payload = job["payload"]
                await asyncio.to_thread(
                    execute_remote_command_run,
                    self.settings,
                    self.store,
                    payload["run_id"],
                )
            else:
                raise ValueError(f"unknown job kind {job['kind']}")
        except Exception as error:
            payload = job.get("payload", {})
            run_id = payload.get("run_id")
            if job.get("kind") == "run_analysis" and isinstance(run_id, str):
                try:
                    self.store.update_run_status(run_id, "failed", "failed")
                    run = self.store.get_run(run_id)
                    self.store.append_event(
                        run["workspace_id"],
                        run_id,
                        "run.error",
                        {"error": str(error)[:2000]},
                    )
                except Exception:
                    pass
            elif job.get("kind") == "remote_command_run" and isinstance(run_id, str):
                try:
                    self.store.fail_remote_run(run_id, "EXECUTE_REMOTE_COMMAND", str(error))
                except Exception:
                    pass
            self.store.fail_job(job, str(error))
            return
        self.store.complete_job(job["id"])
