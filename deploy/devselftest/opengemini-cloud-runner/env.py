import argparse
import json
from pathlib import Path


def initialize_environment(
    *,
    instance_id: str,
    endpoint: str,
    case_name: str,
    artifacts_dir: str,
) -> dict[str, str]:
    artifacts = Path(artifacts_dir)
    artifacts.mkdir(parents=True, exist_ok=True)
    config = {
        "instanceId": instance_id,
        "endpoint": endpoint,
        "caseName": case_name,
    }
    (artifacts / "test-env.json").write_text(
        json.dumps(config, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    return config


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--instance-id", required=True)
    parser.add_argument("--endpoint", required=True)
    parser.add_argument("--case-name", required=True)
    parser.add_argument("--artifacts-dir", default="/workspace/artifacts")
    args = parser.parse_args()
    initialize_environment(
        instance_id=args.instance_id,
        endpoint=args.endpoint,
        case_name=args.case_name,
        artifacts_dir=args.artifacts_dir,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
