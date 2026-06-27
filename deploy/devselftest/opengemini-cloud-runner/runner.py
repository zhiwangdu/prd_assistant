import json
import os
import sys
import traceback
from pathlib import Path

from cases import opengemini_rw_smoke
from env import initialize_environment


CASES = {
    "opengemini_rw_smoke": opengemini_rw_smoke.run,
}


def _required_env(name: str) -> str:
    value = os.environ.get(name, "").strip()
    if not value:
        raise RuntimeError(f"missing required env {name}")
    return value


def _endpoint() -> str:
    endpoint = os.environ.get("DEVSELFTEST_PARAM_ENDPOINT", "").strip()
    if endpoint:
        return endpoint
    host = os.environ.get("DEVSELFTEST_HOST", "").strip()
    port = os.environ.get("DEVSELFTEST_PORT", "").strip()
    if host and port:
        return f"http://{host}:{port}"
    raise RuntimeError("missing DEVSELFTEST_PARAM_ENDPOINT or DEVSELFTEST_HOST/PORT")


def _write_result(path: Path, result: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def _artifacts_dir() -> Path:
    explicit = os.environ.get("SELFTEST_ARTIFACTS_DIR", "").strip()
    if explicit:
        return Path(explicit)
    mounted = Path("/workspace/artifacts")
    if mounted.exists():
        return mounted
    return Path(os.environ.get("DEVSELFTEST_ARTIFACTS_DIR", "/workspace/artifacts"))


def main() -> int:
    artifacts_dir = _artifacts_dir()
    result_path = artifacts_dir / "test-result.json"
    try:
        case_name = _required_env("DEVSELFTEST_PARAM_CASE_NAME")
        instance_id = _required_env("DEVSELFTEST_PARAM_INSTANCE_ID")
        endpoint = _endpoint()
        if case_name not in CASES:
            raise RuntimeError(f"unknown case {case_name}; available: {', '.join(sorted(CASES))}")

        config = initialize_environment(
            instance_id=instance_id,
            endpoint=endpoint,
            case_name=case_name,
            artifacts_dir=str(artifacts_dir),
        )
        details = CASES[case_name](config)
        result = {
            "status": "OK",
            "caseName": case_name,
            "instanceId": instance_id,
            "endpoint": endpoint,
            "details": details,
        }
        _write_result(result_path, result)
        print(json.dumps(result, ensure_ascii=False))
        return 0
    except Exception as exc:
        result = {
            "status": "FAILED",
            "error": str(exc),
            "traceback": traceback.format_exc(),
        }
        _write_result(result_path, result)
        print(json.dumps(result, ensure_ascii=False), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
