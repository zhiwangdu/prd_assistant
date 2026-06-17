from __future__ import annotations

import os
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT_DIR = Path(__file__).resolve().parents[2]


class DeployScriptTests(unittest.TestCase):
    def run_script(
        self,
        script: Path,
        *args: str,
        env: dict[str, str] | None = None,
    ) -> subprocess.CompletedProcess[str]:
        merged_env = os.environ.copy()
        if env:
            merged_env.update(env)
        return subprocess.run(
            ["bash", script.as_posix(), *args],
            cwd=ROOT_DIR,
            env=merged_env,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )

    def isolated_env(self, tmp_path: Path) -> dict[str, str]:
        return {
            "HOME": (tmp_path / "home").as_posix(),
            "LOGAGENT_ENV_FILE": (tmp_path / "missing.env").as_posix(),
            "LOGAGENT_V2_APP_DIR": tmp_path.as_posix(),
            "LOGAGENT_V2_DATA_DIR": (tmp_path / "data-v2").as_posix(),
            "LOGAGENT_V2_WEBUI_DIR": (tmp_path / "webui" / "out").as_posix(),
            "LOGAGENT_V2_PID_FILE": (tmp_path / "logagent-v2.pid").as_posix(),
            "LOGAGENT_V2_LOG_FILE": (tmp_path / "logagent-v2.log").as_posix(),
            "LOGAGENT_V2_VENV_DIR": (tmp_path / "server-v2" / ".venv").as_posix(),
        }

    def test_v2_local_help_and_timeout_validation(self) -> None:
        script = ROOT_DIR / "scripts" / "v2-local.sh"

        help_result = self.run_script(script, "--help")
        self.assertEqual(help_result.returncode, 0)
        self.assertIn("Usage: scripts/v2-local.sh", help_result.stdout)

        invalid_timeout = self.run_script(
            script,
            "status",
            env={"LOGAGENT_V2_STARTUP_TIMEOUT_SECONDS": "0"},
        )
        self.assertEqual(invalid_timeout.returncode, 2)
        self.assertIn(
            "LOGAGENT_V2_STARTUP_TIMEOUT_SECONDS must be a positive integer",
            invalid_timeout.stderr,
        )

    def test_logagent_v2ctl_status_is_scoped_to_pid_file(self) -> None:
        script = ROOT_DIR / "deploy" / "logagent-v2ctl.sh"

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            env = self.isolated_env(tmp_path)
            pid_file = Path(env["LOGAGENT_V2_PID_FILE"])
            pid_file.write_text(str(os.getpid()), encoding="utf-8")

            result = self.run_script(script, "status", env=env)

            self.assertEqual(result.returncode, 1)
            self.assertIn("LogAgent V2 server is not running", result.stdout)
            self.assertFalse(pid_file.exists())

    def test_logagent_v2ctl_start_requires_installed_runtime(self) -> None:
        script = ROOT_DIR / "deploy" / "logagent-v2ctl.sh"

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            env = self.isolated_env(tmp_path)

            result = self.run_script(script, "start", env=env)

            self.assertEqual(result.returncode, 1)
            self.assertIn("V2 Python not executable", result.stderr)
            self.assertIn("Run deploy/rebuild-v2-install.sh first.", result.stderr)
            self.assertFalse(Path(env["LOGAGENT_V2_PID_FILE"]).exists())


if __name__ == "__main__":
    unittest.main()
