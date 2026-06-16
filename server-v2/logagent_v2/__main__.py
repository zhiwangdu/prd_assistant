from __future__ import annotations

import argparse

from .api import create_app
from .config import Settings
from .store import Store


def main() -> None:
    parser = argparse.ArgumentParser(prog="logagent-v2")
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("init-db")
    subparsers.add_parser("server")
    args = parser.parse_args()

    settings = Settings.from_env()
    settings.ensure_dirs()

    if args.command == "init-db":
        Store(settings.sqlite_path).initialize()
        print(f"initialized {settings.sqlite_path}")
        return

    if args.command == "server":
        import uvicorn

        uvicorn.run(
            create_app(settings),
            host=settings.host,
            port=settings.port,
            log_level="info",
        )


if __name__ == "__main__":
    main()

