from __future__ import annotations

import re
from typing import Any

from .store import JsonObject


FORBIDDEN_ALIAS_RE = re.compile(r"(^|\b)(task|run)(\b|[_-])", re.IGNORECASE)


def fallback_run_alias(final_answer: JsonObject, question: str) -> str:
    summary = final_answer.get("summary")
    if isinstance(summary, str):
        alias = normalize_run_alias(summary)
        if alias:
            return alias
    alias = normalize_run_alias(question)
    return alias or "Analysis result"


def normalize_run_alias(value: Any) -> str | None:
    if not isinstance(value, str):
        return None
    alias = (
        value.replace("\n", " ")
        .replace("\r", " ")
        .replace("\t", " ")
        .translate(str.maketrans({"\"": "", "'": "", "`": "", ".": ""}))
    )
    alias = " ".join(alias.split()).strip("-_:|/\\ ")
    if not alias:
        return None
    lower = alias.lower()
    if "logagent" in lower or "task_" in lower or FORBIDDEN_ALIAS_RE.search(alias):
        return None
    alias = truncate_chars(alias, 40).strip()
    if len(alias) < 2:
        return None
    return alias


def truncate_chars(value: str, max_chars: int) -> str:
    if len(value) <= max_chars:
        return value
    return value[:max_chars]
