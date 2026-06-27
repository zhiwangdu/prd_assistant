import time
from typing import Any

import requests


def _check_query_response(payload: dict[str, Any]) -> None:
    for result in payload.get("results", []):
        if "error" in result:
            raise RuntimeError(result["error"])


def run(config: dict[str, str]) -> dict[str, Any]:
    endpoint = config["endpoint"].rstrip("/")
    case_name = config["caseName"]
    db_name = f"toolhub_selftest_{int(time.time() * 1000)}"
    measurement = "rw_smoke"
    timestamp = int(time.time_ns())
    line = f"{measurement},case={case_name} value=1i {timestamp}"

    query = requests.post(
        f"{endpoint}/query",
        params={"q": f"CREATE DATABASE {db_name}"},
        timeout=10,
    )
    query.raise_for_status()
    _check_query_response(query.json())

    write = requests.post(
        f"{endpoint}/write",
        params={"db": db_name},
        data=line,
        timeout=10,
    )
    write.raise_for_status()

    last_payload: dict[str, Any] | None = None
    for _ in range(10):
        select = requests.get(
            f"{endpoint}/query",
            params={"db": db_name, "q": f"SELECT value FROM {measurement}"},
            timeout=10,
        )
        select.raise_for_status()
        payload = select.json()
        _check_query_response(payload)
        last_payload = payload
        for result in payload.get("results", []):
            for series in result.get("series", []):
                if series.get("values"):
                    return {
                        "database": db_name,
                        "measurement": measurement,
                        "writtenLine": line,
                        "queryValues": series["values"],
                    }
        time.sleep(0.5)

    raise RuntimeError(f"query did not return the written point: {last_payload}")
