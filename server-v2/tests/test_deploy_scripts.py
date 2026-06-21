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
            "LOGAGENT_V2_STARTUP_TIMEOUT_SECONDS": "1",
        }

    def test_v2_local_help_and_timeout_validation(self) -> None:
        script = ROOT_DIR / "scripts" / "v2-local.sh"

        help_result = self.run_script(script, "--help")
        self.assertEqual(help_result.returncode, 0)
        self.assertIn("Usage: scripts/v2-local.sh", help_result.stdout)
        self.assertIn("smoke-tools", help_result.stdout)

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

            help_result = self.run_script(script, "--help", env=env)
            self.assertEqual(help_result.returncode, 0)
            self.assertIn("Usage:", help_result.stdout)
            self.assertIn("help", help_result.stdout)
            self.assertIn("smoke-tools", help_result.stdout)

            pid_file = Path(env["LOGAGENT_V2_PID_FILE"])
            pid_file.write_text(str(os.getpid()), encoding="utf-8")

            result = self.run_script(script, "status", env=env)

            self.assertEqual(result.returncode, 1)
            self.assertIn("LogAgent V2 server is not running", result.stdout)
            self.assertFalse(pid_file.exists())

    def test_logagent_v2ctl_smoke_tools_delegates_source_built_smoke(self) -> None:
        script = ROOT_DIR / "deploy" / "logagent-v2ctl.sh"

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            source = tmp_path / "source"
            scripts_dir = source / "scripts"
            scripts_dir.mkdir(parents=True)
            smoke_log = tmp_path / "smoke.args"
            smoke_script = scripts_dir / "smoke-source-built-analyzers.sh"
            smoke_script.write_text(
                "#!/usr/bin/env bash\n"
                "set -euo pipefail\n"
                "printf '%s\\n' \"$@\" > \"$LOGAGENT_TEST_SMOKE_LOG\"\n",
                encoding="utf-8",
            )
            smoke_script.chmod(0o755)

            env = self.isolated_env(tmp_path)
            env["LOGAGENT_SRC_DIR"] = source.as_posix()
            env["LOGAGENT_TEST_SMOKE_LOG"] = smoke_log.as_posix()

            result = self.run_script(
                script,
                "smoke-tools",
                "--only-tool",
                "flux_query_analyzer",
                env=env,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(
                smoke_log.read_text(encoding="utf-8").splitlines(),
                ["--only", "flux"],
            )

            unknown = self.run_script(
                script,
                "smoke-tools",
                "--only-tool",
                "unknown_analyzer",
                env=env,
            )
            self.assertEqual(unknown.returncode, 2)
            self.assertIn("Unsupported --only-tool value", unknown.stderr)

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

    def test_v2_local_status_reports_source_built_analyzers(self) -> None:
        script = ROOT_DIR / "scripts" / "v2-local.sh"

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            bin_dir = tmp_path / "bin"
            bin_dir.mkdir()
            fake_curl = bin_dir / "curl"
            fake_curl.write_text(
                """#!/usr/bin/env bash
set -euo pipefail
target="${@: -1}"
case "$target" in
  */health)
    printf '{"status":"ok"}'
    ;;
  */api/v2/tools)
    args=" $* "
    [[ "$args" == *"Authorization: Bearer secret"* ]] || {
      echo "missing Authorization header" >&2
      exit 12
    }
    cat <<'JSON'
{"sourceBuiltAnalyzers":[{"toolId":"flux_query_analyzer","status":"registered","enabled":true,"runnable":true,"commandExists":true,"commandExecutable":true},{"toolId":"influxdb_storage_analyzer","status":"unavailable","statusReason":"command_file_not_found","enabled":true,"runnable":false,"commandExists":false,"commandExecutable":false}]}
JSON
    ;;
  *)
    echo "unexpected curl target: $target" >&2
    exit 13
    ;;
esac
""",
                encoding="utf-8",
            )
            fake_curl.chmod(0o755)
            env = self.isolated_env(tmp_path)
            env["LOGAGENT_V2_API_KEY"] = "secret"
            env["PATH"] = f"{bin_dir}{os.pathsep}{os.environ.get('PATH', '')}"
            Path(env["LOGAGENT_V2_PID_FILE"]).write_text(
                str(os.getpid()),
                encoding="utf-8",
            )

            result = self.run_script(script, "status", env=env)

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("LogAgent V2 is running", result.stdout)
            self.assertIn('"status":"ok"', result.stdout)
            self.assertIn("Analyzer tools:", result.stdout)
            self.assertIn(
                "flux_query_analyzer: status=registered, enabled=true, runnable=true, "
                "commandExists=true, commandExecutable=true",
                result.stdout,
            )
            self.assertIn(
                "influxdb_storage_analyzer: status=unavailable, enabled=true, "
                "runnable=false, commandExists=false, commandExecutable=false, "
                "reason=command_file_not_found",
                result.stdout,
            )

    def test_rebuild_v2_install_validates_source_dir_before_install(self) -> None:
        script = ROOT_DIR / "deploy" / "rebuild-v2-install.sh"

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            env = self.isolated_env(tmp_path)
            env["LOGAGENT_SRC_DIR"] = ""

            help_result = self.run_script(script, "--help", env=env)
            self.assertEqual(help_result.returncode, 0)
            self.assertIn("Usage: ./rebuild-v2-install.sh", help_result.stdout)

            missing_source = self.run_script(script, "--server-only", env=env)
            self.assertEqual(missing_source.returncode, 1)
            self.assertIn("LOGAGENT_SRC_DIR is required", missing_source.stderr)

    def test_rebuild_v2_tools_only_delegates_single_tool_build(self) -> None:
        script = ROOT_DIR / "deploy" / "rebuild-v2-install.sh"

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            source = tmp_path / "source"
            (source / "server-v2").mkdir(parents=True)
            (source / "server-v2" / "pyproject.toml").write_text(
                "[project]\nname = \"fake-logagent-v2\"\nversion = \"0.0.0\"\n",
                encoding="utf-8",
            )
            scripts_dir = source / "scripts"
            scripts_dir.mkdir()
            build_log = tmp_path / "build-tools.args"
            build_tools = scripts_dir / "build-tools.sh"
            build_tools.write_text(
                "#!/usr/bin/env bash\n"
                "set -euo pipefail\n"
                "printf '%s\\n' \"$@\" > \"$LOGAGENT_TEST_BUILD_LOG\"\n",
                encoding="utf-8",
            )
            build_tools.chmod(0o755)

            env = self.isolated_env(tmp_path)
            env["LOGAGENT_SRC_DIR"] = source.as_posix()
            env["LOGAGENT_TEST_BUILD_LOG"] = build_log.as_posix()

            result = self.run_script(
                script,
                "--tools-only",
                "--only-tool",
                "flux_query_analyzer",
                "--no-restart",
                env=env,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("Building V2 analyzer tools", result.stdout)
            self.assertIn("V2 install complete; restart skipped.", result.stdout)
            self.assertFalse(Path(env["LOGAGENT_V2_VENV_DIR"]).exists())
            self.assertFalse(Path(env["LOGAGENT_V2_WEBUI_DIR"]).exists())
            self.assertEqual(
                build_log.read_text(encoding="utf-8").splitlines(),
                ["--output-dir", f"{tmp_path.as_posix()}/bin/tools", "--only", "flux"],
            )

    def test_rebuild_v2_full_install_builds_tools_by_default(self) -> None:
        script = ROOT_DIR / "deploy" / "rebuild-v2-install.sh"

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            source = tmp_path / "source"
            (source / "server-v2").mkdir(parents=True)
            (source / "server-v2" / "pyproject.toml").write_text(
                "[project]\nname = \"fake-logagent-v2\"\nversion = \"0.0.0\"\n",
                encoding="utf-8",
            )
            (source / "webui").mkdir()
            scripts_dir = source / "scripts"
            scripts_dir.mkdir()
            build_log = tmp_path / "build-tools.args"
            build_tools = scripts_dir / "build-tools.sh"
            build_tools.write_text(
                "#!/usr/bin/env bash\n"
                "set -euo pipefail\n"
                "printf '%s\\n' \"$@\" > \"$LOGAGENT_TEST_BUILD_LOG\"\n",
                encoding="utf-8",
            )
            build_tools.chmod(0o755)

            venv_bin = tmp_path / "server-v2" / ".venv" / "bin"
            venv_bin.mkdir(parents=True)
            fake_python = venv_bin / "python"
            fake_python.write_text("#!/usr/bin/env bash\nexit 0\n", encoding="utf-8")
            fake_python.chmod(0o755)

            bin_dir = tmp_path / "bin-shims"
            bin_dir.mkdir()
            fake_npm = bin_dir / "npm"
            fake_npm.write_text(
                "#!/usr/bin/env bash\n"
                "set -euo pipefail\n"
                "prefix=''\n"
                "while (($# > 0)); do\n"
                "  case \"$1\" in\n"
                "    --prefix) prefix=\"$2\"; shift 2 ;;\n"
                "    *) shift ;;\n"
                "  esac\n"
                "done\n"
                "mkdir -p \"$prefix/out\"\n"
                "printf '<html></html>\\n' > \"$prefix/out/index.html\"\n",
                encoding="utf-8",
            )
            fake_npm.chmod(0o755)

            env = self.isolated_env(tmp_path)
            env["LOGAGENT_SRC_DIR"] = source.as_posix()
            env["LOGAGENT_TEST_BUILD_LOG"] = build_log.as_posix()
            env["PATH"] = f"{bin_dir}{os.pathsep}{os.environ.get('PATH', '')}"

            result = self.run_script(script, "--no-restart", env=env)

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("Building V2 analyzer tools", result.stdout)
            self.assertEqual(
                build_log.read_text(encoding="utf-8").splitlines(),
                ["--output-dir", f"{tmp_path.as_posix()}/bin/tools"],
            )
            self.assertTrue(Path(env["LOGAGENT_V2_WEBUI_DIR"], "index.html").exists())

    def test_rebuild_v2_full_install_can_skip_default_tools(self) -> None:
        script = ROOT_DIR / "deploy" / "rebuild-v2-install.sh"

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            source = tmp_path / "source"
            (source / "server-v2").mkdir(parents=True)
            (source / "server-v2" / "pyproject.toml").write_text(
                "[project]\nname = \"fake-logagent-v2\"\nversion = \"0.0.0\"\n",
                encoding="utf-8",
            )
            (source / "webui").mkdir()
            scripts_dir = source / "scripts"
            scripts_dir.mkdir()
            build_log = tmp_path / "build-tools.args"
            build_tools = scripts_dir / "build-tools.sh"
            build_tools.write_text(
                "#!/usr/bin/env bash\n"
                "set -euo pipefail\n"
                "printf '%s\\n' \"$@\" > \"$LOGAGENT_TEST_BUILD_LOG\"\n",
                encoding="utf-8",
            )
            build_tools.chmod(0o755)

            venv_bin = tmp_path / "server-v2" / ".venv" / "bin"
            venv_bin.mkdir(parents=True)
            fake_python = venv_bin / "python"
            fake_python.write_text("#!/usr/bin/env bash\nexit 0\n", encoding="utf-8")
            fake_python.chmod(0o755)

            bin_dir = tmp_path / "bin-shims"
            bin_dir.mkdir()
            fake_npm = bin_dir / "npm"
            fake_npm.write_text(
                "#!/usr/bin/env bash\n"
                "set -euo pipefail\n"
                "prefix=''\n"
                "while (($# > 0)); do\n"
                "  case \"$1\" in\n"
                "    --prefix) prefix=\"$2\"; shift 2 ;;\n"
                "    *) shift ;;\n"
                "  esac\n"
                "done\n"
                "mkdir -p \"$prefix/out\"\n"
                "printf '<html></html>\\n' > \"$prefix/out/index.html\"\n",
                encoding="utf-8",
            )
            fake_npm.chmod(0o755)

            env = self.isolated_env(tmp_path)
            env["LOGAGENT_SRC_DIR"] = source.as_posix()
            env["LOGAGENT_TEST_BUILD_LOG"] = build_log.as_posix()
            env["PATH"] = f"{bin_dir}{os.pathsep}{os.environ.get('PATH', '')}"

            result = self.run_script(script, "--skip-tools", "--no-restart", env=env)

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertNotIn("Building V2 analyzer tools", result.stdout)
            self.assertFalse(build_log.exists())

    def test_build_tools_influxdb_uses_local_flux_replace_temporarily(self) -> None:
        script = ROOT_DIR / "scripts" / "build-tools.sh"
        influxdb_go_mod = ROOT_DIR / "third_party" / "influxdb" / "go.mod"
        influxdb_go_sum = ROOT_DIR / "third_party" / "influxdb" / "go.sum"
        flux_manifest = (
            ROOT_DIR / "third_party" / "flux" / "libflux" / "flux-core" / "Cargo.toml"
        )
        if not influxdb_go_mod.exists() or not flux_manifest.exists():
            self.skipTest("source-built analyzer submodules are not initialized")

        go_mod_before = influxdb_go_mod.read_bytes()
        go_sum_before = influxdb_go_sum.read_bytes() if influxdb_go_sum.exists() else None

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            bin_dir = tmp_path / "bin"
            bin_dir.mkdir()
            output_dir = tmp_path / "tools"
            build_env_log = tmp_path / "build-env.log"
            fake_go = bin_dir / "go"
            fake_go.write_text(
                """#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
  env)
    case "${2:-}" in
      GOROOT)
        printf '%s\\n' "$LOGAGENT_TEST_GOROOT"
        ;;
      GOVERSION)
        printf 'go1.26.4\\n'
        ;;
      *)
        exit 31
        ;;
    esac
    ;;
  mod)
    [[ "${2:-}" == "edit" ]] || exit 32
    shift 2
    replace=''
    while (($# > 0)); do
      case "$1" in
        -replace)
          replace="$2"
          shift 2
          ;;
        -replace=*)
          replace="${1#-replace=}"
          shift
          ;;
        *)
          shift
          ;;
      esac
    done
    [[ -n "$replace" ]] || exit 33
    old="${replace%%=*}"
    new="${replace#*=}"
    printf '\\nreplace %s => %s\\n' "$old" "$new" >> go.mod
    ;;
  build)
    output=''
    while (($# > 0)); do
      case "$1" in
        -o)
          output="$2"
          shift 2
          ;;
        *)
          shift
          ;;
      esac
    done
    [[ -n "$output" ]] || exit 34
    {
      printf 'PKG_CONFIG=%s\\n' "${PKG_CONFIG:-}"
      printf 'GOCACHE=%s\\n' "${GOCACHE:-}"
      printf 'GOSUMDB=%s\\n' "${GOSUMDB:-}"
      printf 'GO_MOD_BEGIN\\n'
      cat go.mod
      printf 'GO_MOD_END\\n'
    } > "$LOGAGENT_TEST_BUILD_ENV_LOG"
    mkdir -p "$(dirname "$output")"
    printf '#!/usr/bin/env bash\\n' > "$output"
    chmod 0755 "$output"
    ;;
  version)
    printf 'go version go1.26.4 darwin/arm64\\n'
    ;;
  *)
    exit 35
    ;;
esac
""",
                encoding="utf-8",
            )
            fake_go.chmod(0o755)
            fake_cargo = bin_dir / "cargo"
            fake_cargo.write_text("#!/usr/bin/env bash\nexit 0\n", encoding="utf-8")
            fake_cargo.chmod(0o755)

            env = {
                "PATH": f"{bin_dir}{os.pathsep}{os.environ.get('PATH', '')}",
                "LOGAGENT_TEST_GOROOT": (tmp_path / "goroot").as_posix(),
                "LOGAGENT_TEST_BUILD_ENV_LOG": build_env_log.as_posix(),
            }

            result = self.run_script(
                script,
                "--output-dir",
                output_dir.as_posix(),
                "--only",
                "influxdb",
                env=env,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertTrue((output_dir / "influxdb_storage_analyzer").exists())
            build_env = build_env_log.read_text(encoding="utf-8")
            self.assertIn(
                f"PKG_CONFIG={ROOT_DIR / 'third_party' / 'influxdb' / 'pkg-config.sh'}",
                build_env,
            )
            self.assertIn("GOSUMDB=off", build_env)
            self.assertIn(
                f"replace github.com/influxdata/flux => {ROOT_DIR / 'third_party' / 'flux'}",
                build_env,
            )

        self.assertEqual(influxdb_go_mod.read_bytes(), go_mod_before)
        if go_sum_before is None:
            self.assertFalse(influxdb_go_sum.exists())
        else:
            self.assertEqual(influxdb_go_sum.read_bytes(), go_sum_before)

    def test_tool_build_scripts_document_source_built_id_aliases(self) -> None:
        build_tools = ROOT_DIR / "scripts" / "build-tools.sh"
        smoke_tools = ROOT_DIR / "scripts" / "smoke-source-built-analyzers.sh"
        rebuild_v2 = ROOT_DIR / "deploy" / "rebuild-v2-install.sh"
        v2_local = ROOT_DIR / "scripts" / "v2-local.sh"

        build_help = self.run_script(build_tools, "--help")
        self.assertEqual(build_help.returncode, 0)
        self.assertIn("flux_query_analyzer", build_help.stdout)
        self.assertIn("influxdb_storage_analyzer", build_help.stdout)

        smoke_help = self.run_script(smoke_tools, "--help")
        self.assertEqual(smoke_help.returncode, 0)
        self.assertIn("opengemini_storage_analyzer", smoke_help.stdout)
        self.assertIn("influxdb_storage_analyzer", smoke_help.stdout)

        rebuild_help = self.run_script(rebuild_v2, "--help")
        self.assertEqual(rebuild_help.returncode, 0)
        self.assertIn("opengemini_storage_analyzer", rebuild_help.stdout)
        self.assertIn("--skip-tools", rebuild_help.stdout)

        local_help = self.run_script(v2_local, "--help")
        self.assertEqual(local_help.returncode, 0)
        self.assertIn("influxql_analyzer", local_help.stdout)

    def test_rebuild_v2_rejects_unknown_only_tool_before_source_validation(self) -> None:
        script = ROOT_DIR / "deploy" / "rebuild-v2-install.sh"

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            env = self.isolated_env(tmp_path)
            env["LOGAGENT_SRC_DIR"] = ""

            result = self.run_script(
                script,
                "--tools-only",
                "--only-tool",
                "unknown_analyzer",
                env=env,
            )

            self.assertEqual(result.returncode, 2)
            self.assertIn("Unsupported --only-tool value", result.stderr)


if __name__ == "__main__":
    unittest.main()
