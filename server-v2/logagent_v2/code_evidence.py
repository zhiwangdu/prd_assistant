from __future__ import annotations

import hashlib
import json
import re
import subprocess
from pathlib import Path
from typing import Any

from .artifacts import resolve_artifact_path, write_artifact_bytes
from .config import CodeRepoDefinition, Settings, validate_configured_git_ref
from .store import JsonObject, Store, now_iso


MAX_CODE_KEYWORDS = 20
MAX_MATCHES_PER_KEYWORD = 10
MAX_TEXT_CHARS = 1000
CODE_EVIDENCE_REF_RE = re.compile(
    r"^(code_evidence/[A-Za-z0-9_-]+\.json)#matches/(\d+)$"
)
TOKEN_RE = re.compile(r"[A-Za-z_][A-Za-z0-9_.:-]{2,}")
STOP_WORDS = {
    "about",
    "after",
    "before",
    "check",
    "diagnose",
    "error",
    "failed",
    "failure",
    "from",
    "have",
    "into",
    "logs",
    "please",
    "query",
    "root",
    "the",
    "this",
    "timeout",
    "with",
}


def code_evidence_available(settings: Settings) -> bool:
    return bool(settings.code_repos)


def code_evidence_tool_descriptor() -> JsonObject:
    return {
        "name": "logagent.search_code",
        "description": (
            "Search configured read-only source repositories for version-bound code evidence."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "product": {"type": "string", "minLength": 1},
                "version": {"type": "string"},
                "gitRef": {"type": "string"},
                "query": {"type": "string"},
                "keywords": {
                    "type": "array",
                    "items": {"type": "string"},
                    "maxItems": MAX_CODE_KEYWORDS,
                },
                "maxMatchesPerKeyword": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": MAX_MATCHES_PER_KEYWORD,
                    "default": MAX_MATCHES_PER_KEYWORD,
                },
            },
            "required": ["product"],
            "additionalProperties": False,
        },
    }


def run_code_search(
    settings: Settings,
    store: Store,
    run: JsonObject,
    arguments: JsonObject,
) -> JsonObject:
    task_context = task_code_context(store, run)
    product = resolve_task_code_product(
        require_string(arguments, "product"),
        task_context,
    )
    repo = resolve_code_repo(settings, product)
    version = resolve_task_code_version(
        optional_string(arguments.get("version")),
        task_context,
    )
    configured_ref = resolve_code_ref(repo, version, optional_string(arguments.get("gitRef")))
    commit = git_output(repo.repo_path, "rev-parse", f"{configured_ref}^{{commit}}")
    keywords = normalize_code_keywords(arguments.get("keywords"), arguments.get("query"))
    if not keywords:
        raise ValueError("logagent.search_code requires keywords or query")
    max_matches = normalize_max_matches(arguments.get("maxMatchesPerKeyword"))
    action_id = code_action_id(repo.product, version, configured_ref, keywords, max_matches)
    existing = existing_code_evidence(settings, store, run["id"], action_id)
    if existing is not None:
        return existing

    matches: list[JsonObject] = []
    keyword_counts: dict[str, int] = {}
    for keyword in keywords:
        keyword_matches = git_grep_keyword(repo, commit, keyword, max_matches)
        keyword_counts[keyword] = len(keyword_matches)
        matches.extend(keyword_matches)
    logical_path = f"code_evidence/{action_id}.json"
    for index, match in enumerate(matches):
        match["ref"] = f"{logical_path}#matches/{index}"
    result = {
        "schemaVersion": 1,
        "kind": "code_evidence",
        "actionId": action_id,
        "product": repo.product,
        "version": version,
        "ref": configured_ref,
        "commit": commit,
        "repo": {"product": repo.product, "searchRoots": list(repo.search_roots)},
        "taskContext": task_context,
        "keywords": keywords,
        "keywordCounts": keyword_counts,
        "matchCount": len(matches),
        "truncated": any(count >= max_matches for count in keyword_counts.values()),
        "matches": matches,
        "createdAt": now_iso(),
        "finalEvidenceAllowed": True,
    }
    artifact = write_artifact_bytes(
        settings=settings,
        store=store,
        workspace_id=run["workspace_id"],
        filename=f"{action_id}_code_evidence.json",
        data=json.dumps(result, ensure_ascii=True, indent=2).encode("utf-8"),
        content_type="application/json",
        schema_name="logagent.v2.code_evidence.v1",
        preview={
            "path": logical_path,
            "product": repo.product,
            "version": version,
            "ref": configured_ref,
            "matchCount": len(matches),
        },
    )
    evidence = store.create_evidence(
        workspace_id=run["workspace_id"],
        run_id=run["id"],
        kind="code_evidence",
        final_allowed=True,
        summary=(
            f"Code evidence search found {len(matches)} match(es) in "
            f"{repo.product}@{configured_ref}."
        ),
        artifact_id=artifact["id"],
        payload={
            "artifactId": artifact["id"],
            "path": logical_path,
            "actionId": action_id,
            "product": repo.product,
            "version": version,
            "ref": configured_ref,
            "commit": commit,
            "taskContext": task_context,
            "matchCount": len(matches),
            "evidenceRefPrefix": f"{logical_path}#matches/",
        },
    )
    return code_search_response(result, evidence)


def task_code_context(store: Store, run: JsonObject) -> JsonObject:
    workspace_id = run.get("workspace_id")
    if not isinstance(workspace_id, str) or not workspace_id:
        return {}
    try:
        workspace = store.get_workspace(workspace_id)
    except KeyError:
        return {}
    instance_id = workspace.get("instanceId")
    if not isinstance(instance_id, str) or not instance_id.strip():
        return {}
    instance_id = instance_id.strip()
    metadata = store.get_metadata_instance(instance_id, missing_ok=True)
    if not isinstance(metadata, dict):
        return {"instanceId": instance_id}
    snapshot = metadata.get("snapshot")
    instance = snapshot.get("instance") if isinstance(snapshot, dict) else {}
    if not isinstance(instance, dict):
        instance = {}
    context: JsonObject = {"instanceId": instance_id}
    product = optional_string(instance.get("product"))
    version = optional_string(instance.get("version"))
    if product:
        context["product"] = product
    if version:
        context["version"] = version
    return context


def resolve_task_code_product(requested_product: str, context: JsonObject) -> str:
    product = requested_product.strip()
    context_product = optional_string(context.get("product"))
    if context_product and product.lower() != context_product.lower():
        raise ValueError(
            "logagent.search_code product must match task metadata instance product "
            f"{context_product}"
        )
    return product


def resolve_task_code_version(
    requested_version: str | None,
    context: JsonObject,
) -> str | None:
    context_version = optional_string(context.get("version"))
    if context_version:
        if requested_version and requested_version != context_version:
            raise ValueError(
                "logagent.search_code version must match task metadata instance version "
                f"{context_version}"
            )
        return context_version
    return requested_version


def resolve_code_repo(settings: Settings, product: str) -> CodeRepoDefinition:
    normalized = product.strip().lower()
    for repo in settings.code_repos:
        if repo.product.lower() == normalized:
            return repo
    raise ValueError(f"unknown code repo product {product}")


def resolve_code_ref(
    repo: CodeRepoDefinition,
    version: str | None,
    explicit_ref: str | None,
) -> str:
    allowed_refs = {repo.default_ref, *repo.version_refs.values()}
    if explicit_ref:
        explicit_ref = validate_configured_git_ref(repo.product, explicit_ref)
        if explicit_ref not in allowed_refs:
            raise ValueError("gitRef must match a configured defaultRef or versionRefs value")
        if version:
            expected_ref = repo.version_refs.get(version)
            if expected_ref is None:
                raise ValueError(f"unknown version {version} for code repo {repo.product}")
            if explicit_ref != expected_ref:
                raise ValueError("gitRef must match the configured ref for version")
        return explicit_ref
    if version and version in repo.version_refs:
        return repo.version_refs[version]
    if version:
        raise ValueError(f"unknown version {version} for code repo {repo.product}")
    return repo.default_ref


def normalize_code_keywords(raw_keywords: Any, raw_query: Any) -> list[str]:
    keywords: list[str] = []
    if isinstance(raw_keywords, list):
        for item in raw_keywords:
            if isinstance(item, str):
                add_keyword(keywords, item)
    elif raw_keywords is not None:
        raise ValueError("keywords must be an array of strings")
    if isinstance(raw_query, str):
        for token in TOKEN_RE.findall(raw_query):
            add_keyword(keywords, token)
    elif raw_query is not None:
        raise ValueError("query must be a string")
    return keywords[:MAX_CODE_KEYWORDS]


def add_keyword(keywords: list[str], value: str) -> None:
    keyword = value.strip()
    if len(keyword) < 3:
        return
    if keyword.lower() in STOP_WORDS:
        return
    if keyword not in keywords:
        keywords.append(keyword)


def normalize_max_matches(value: Any) -> int:
    if value is None:
        return MAX_MATCHES_PER_KEYWORD
    if isinstance(value, bool) or not isinstance(value, int):
        raise ValueError("maxMatchesPerKeyword must be an integer")
    return max(1, min(value, MAX_MATCHES_PER_KEYWORD))


def git_grep_keyword(
    repo: CodeRepoDefinition,
    commit: str,
    keyword: str,
    max_matches: int,
) -> list[JsonObject]:
    args = [
        "grep",
        "-n",
        "-I",
        "--no-color",
        "-F",
        "-e",
        keyword,
        commit,
    ]
    if repo.search_roots:
        args.extend(["--", *repo.search_roots])
    completed = git_run(repo.repo_path, *args)
    if completed.returncode == 1:
        return []
    if completed.returncode != 0:
        raise ValueError((completed.stderr or completed.stdout).strip() or "git grep failed")
    matches: list[JsonObject] = []
    for line in completed.stdout.splitlines():
        parsed = parse_git_grep_line(line, commit)
        if parsed is None:
            continue
        file_path, line_number, text = parsed
        matches.append(
            {
                "keyword": keyword,
                "file": file_path,
                "line": line_number,
                "lineNumber": line_number,
                "text": text[:MAX_TEXT_CHARS],
                "snippet": text[:MAX_TEXT_CHARS],
                "reason": f"Matched configured code keyword {keyword!r}.",
            }
        )
        if len(matches) >= max_matches:
            break
    return matches


def parse_git_grep_line(line: str, commit: str) -> tuple[str, int, str] | None:
    if line.startswith(f"{commit}:"):
        line = line[len(commit) + 1 :]
    first, sep, rest = line.partition(":")
    if not sep:
        return None
    line_text, sep, text = rest.partition(":")
    if not sep:
        return None
    try:
        line_number = int(line_text)
    except ValueError:
        return None
    return first, line_number, text


def git_output(repo_path: Path, *args: str) -> str:
    completed = git_run(repo_path, *args)
    if completed.returncode != 0:
        raise ValueError((completed.stderr or completed.stdout).strip() or "git failed")
    return completed.stdout.strip()


def git_run(repo_path: Path, *args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", "-C", repo_path.as_posix(), *args],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )


def code_action_id(
    product: str,
    version: str | None,
    ref: str,
    keywords: list[str],
    max_matches: int,
) -> str:
    digest = hashlib.sha256(
        json.dumps(
            {
                "product": product,
                "version": version,
                "ref": ref,
                "keywords": keywords,
                "maxMatchesPerKeyword": max_matches,
            },
            sort_keys=True,
            ensure_ascii=True,
        ).encode("utf-8")
    ).hexdigest()[:16]
    return f"code_{digest}"


def existing_code_evidence(
    settings: Settings,
    store: Store,
    run_id: str,
    action_id: str,
) -> JsonObject | None:
    for evidence in reversed(store.list_evidence(run_id)):
        if evidence.get("kind") != "code_evidence":
            continue
        payload = evidence.get("payload") if isinstance(evidence.get("payload"), dict) else {}
        if payload.get("actionId") != action_id or not evidence.get("artifact_id"):
            continue
        artifact = store.get_artifact(evidence["artifact_id"])
        path = resolve_artifact_path(settings, artifact["relative_path"])
        try:
            result = json.loads(path.read_text(encoding="utf-8"))
        except Exception:
            continue
        if isinstance(result, dict):
            return code_search_response(result, evidence)
    return None


def code_search_response(result: JsonObject, evidence: JsonObject) -> JsonObject:
    matches = result.get("matches") if isinstance(result.get("matches"), list) else []
    evidence_refs = [
        match["ref"]
        for match in matches
        if isinstance(match, dict) and isinstance(match.get("ref"), str)
    ]
    payload = evidence.get("payload") if isinstance(evidence.get("payload"), dict) else {}
    return {
        "schemaVersion": 1,
        "codeEvidence": result,
        "artifactPath": payload.get("path"),
        "product": result.get("product"),
        "version": result.get("version"),
        "ref": result.get("ref"),
        "commit": result.get("commit"),
        "matches": matches,
        "matchCount": result.get("matchCount", len(matches)),
        "evidenceRefs": evidence_refs,
        "finalEvidenceRefs": evidence_refs,
        "finalEvidenceAllowed": True,
    }


def require_string(arguments: JsonObject, name: str) -> str:
    value = arguments.get(name)
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"{name} is required")
    return value.strip()


def optional_string(value: Any) -> str | None:
    if value is None:
        return None
    if not isinstance(value, str):
        raise ValueError("optional string argument must be a string")
    value = value.strip()
    return value or None
