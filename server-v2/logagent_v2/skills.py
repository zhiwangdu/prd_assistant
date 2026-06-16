from __future__ import annotations

import hashlib
import json
import re
from pathlib import Path, PurePosixPath
from typing import Any

from .artifacts import resolve_artifact_path, write_artifact_bytes
from .config import Settings
from .ids import new_id
from .store import JsonObject, Store


SKILL_ID_RE = re.compile(r"^[A-Za-z0-9_.-]+$")


def list_skills(settings: Settings) -> list[JsonObject]:
    skills = []
    children = sorted(settings.skills_dir.iterdir()) if settings.skills_dir.exists() else []
    for child in children:
        if child.is_dir() and (child / "SKILL.md").is_file():
            skills.append(load_skill(settings, child.name, include_content=False))
    return skills


def get_skill(settings: Settings, skill_id: str, include_content: bool = True) -> JsonObject:
    validate_skill_id(skill_id)
    skill_dir = settings.skills_dir / skill_id
    if not (skill_dir / "SKILL.md").is_file():
        raise KeyError(f"unknown skill {skill_id}")
    return load_skill(settings, skill_id, include_content=include_content)


def import_skill(
    settings: Settings,
    skill_id: str,
    name: str,
    description: str,
    markdown: str,
    filename: str | None = None,
) -> JsonObject:
    validate_skill_id(skill_id)
    if filename is not None and not filename.lower().endswith((".md", ".markdown")):
        raise ValueError("skill filename must end with .md or .markdown")
    if not name.strip() or not description.strip() or not markdown.strip():
        raise ValueError("skill name, description, and markdown are required")
    skill_dir = settings.skills_dir / skill_id
    if skill_dir.exists():
        raise ValueError(f"skill {skill_id} already exists")
    skill_dir.mkdir(parents=True, exist_ok=False)
    skill_md = (
        "---\n"
        f"name: {json.dumps(name.strip(), ensure_ascii=False)}\n"
        f"description: {json.dumps(description.strip(), ensure_ascii=False)}\n"
        "---\n\n"
        f"{markdown.strip()}\n"
    )
    (skill_dir / "SKILL.md").write_text(skill_md, encoding="utf-8")
    logagent_manifest = {
        "schemaVersion": 1,
        "skillId": skill_id,
        "displayName": name.strip(),
        "products": [],
        "domainAdapters": [],
        "toolIds": [],
        "keywords": [],
        "taskKinds": ["log_analysis"],
        "includeByDefault": False,
        "priority": 0,
        "maxPromptChars": None,
        "references": [],
    }
    (skill_dir / "logagent.json").write_text(
        json.dumps(logagent_manifest, ensure_ascii=True, indent=2), encoding="utf-8"
    )
    return get_skill(settings, skill_id)


def load_skill(settings: Settings, skill_id: str, include_content: bool) -> JsonObject:
    skill_dir = settings.skills_dir / skill_id
    skill_path = skill_dir / "SKILL.md"
    raw = skill_path.read_text(encoding="utf-8")
    frontmatter, body = parse_frontmatter(raw)
    manifest = load_logagent_manifest(skill_dir)
    references = normalize_references(skill_dir, manifest.get("references", []))
    max_prompt_chars = manifest.get("maxPromptChars") or 4000
    content = body.strip()[: int(max_prompt_chars)]
    detail = {
        "skillId": skill_id,
        "name": str(frontmatter.get("name") or manifest.get("displayName") or skill_id),
        "description": str(frontmatter.get("description") or ""),
        "displayName": str(manifest.get("displayName") or frontmatter.get("name") or skill_id),
        "includeByDefault": bool(manifest.get("includeByDefault", False)),
        "priority": int(manifest.get("priority", 0)),
        "products": list_of_strings(manifest.get("products")),
        "taskKinds": list_of_strings(manifest.get("taskKinds") or ["log_analysis"]),
        "toolIds": list_of_strings(manifest.get("toolIds")),
        "keywords": list_of_strings(manifest.get("keywords")),
        "domainAdapters": list_of_strings(manifest.get("domainAdapters")),
        "references": references,
        "revision": skill_revision(raw, manifest),
        "sourcePath": skill_path.as_posix(),
    }
    if include_content:
        detail["content"] = content
    return detail


def parse_frontmatter(raw: str) -> tuple[JsonObject, str]:
    if not raw.startswith("---\n"):
        raise ValueError("SKILL.md must start with YAML frontmatter")
    end = raw.find("\n---", 4)
    if end < 0:
        raise ValueError("SKILL.md frontmatter is not closed")
    frontmatter_text = raw[4:end]
    body = raw[end + 4 :]
    frontmatter = parse_frontmatter_yaml(frontmatter_text)
    if not isinstance(frontmatter, dict):
        raise ValueError("SKILL.md frontmatter must be an object")
    if not frontmatter.get("name") or not frontmatter.get("description"):
        raise ValueError("SKILL.md frontmatter requires name and description")
    return frontmatter, body


def parse_frontmatter_yaml(value: str) -> JsonObject:
    try:
        import yaml  # type: ignore[import-not-found]
    except Exception:
        return parse_simple_frontmatter(value)
    parsed = yaml.safe_load(value) or {}
    if not isinstance(parsed, dict):
        raise ValueError("SKILL.md frontmatter must be an object")
    return parsed


def parse_simple_frontmatter(value: str) -> JsonObject:
    parsed: JsonObject = {}
    for line in value.splitlines():
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        if ":" not in line:
            raise ValueError("SKILL.md frontmatter requires key: value lines")
        key, raw = line.split(":", 1)
        text = raw.strip()
        if text.startswith(('"', "'")):
            try:
                parsed[key.strip()] = json.loads(text)
                continue
            except Exception:
                pass
        parsed[key.strip()] = text.strip('"\'')
    return parsed


def load_logagent_manifest(skill_dir: Path) -> JsonObject:
    path = skill_dir / "logagent.json"
    if not path.exists():
        return {}
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError("logagent.json must be an object")
    return value


def normalize_references(skill_dir: Path, raw_references: Any) -> list[JsonObject]:
    if raw_references is None:
        return []
    if not isinstance(raw_references, list):
        raise ValueError("logagent.json references must be an array")
    result = []
    for index, item in enumerate(raw_references):
        if not isinstance(item, dict):
            raise ValueError("skill reference must be an object")
        path = str(item.get("path") or "")
        validate_reference_path(skill_dir, path)
        result.append(
            {
                "referenceId": str(item.get("referenceId") or f"ref_{index}"),
                "path": path,
                "title": str(item.get("title") or path),
                "summary": str(item.get("summary") or ""),
            }
        )
    return result


def validate_reference_path(skill_dir: Path, path: str) -> Path:
    pure = PurePosixPath(path)
    if not path or pure.is_absolute() or ".." in pure.parts:
        raise ValueError(f"unsafe skill reference path {path!r}")
    root = skill_dir.resolve()
    target = (skill_dir / path).resolve()
    if root != target and root not in target.parents:
        raise ValueError(f"skill reference path escapes skill dir: {path}")
    if target.is_symlink() or not target.is_file():
        raise ValueError(f"skill reference is not a regular file: {path}")
    return target


def skill_revision(raw_skill: str, manifest: JsonObject) -> str:
    digest = hashlib.sha256()
    digest.update(raw_skill.encode("utf-8"))
    digest.update(json.dumps(manifest, ensure_ascii=True, sort_keys=True).encode("utf-8"))
    return digest.hexdigest()[:16]


def build_system_context(
    settings: Settings,
    store: Store,
    workspace_id: str,
    run_id: str,
) -> JsonObject:
    workspace = store.get_workspace(workspace_id)
    selected_ids = list(dict.fromkeys(workspace.get("skillIds") or []))
    resources = skill_resources(
        settings,
        selected_ids,
        question=workspace.get("question", ""),
        task_mode=workspace.get("mode", ""),
    )
    return {
        "schemaVersion": 2,
        "workspaceId": workspace_id,
        "runId": run_id,
        "resources": resources,
    }


def preview_system_context(settings: Settings, skill_ids: list[str] | None = None) -> JsonObject:
    return {
        "schemaVersion": 2,
        "workspaceId": None,
        "runId": None,
        "resources": skill_resources(settings, list(dict.fromkeys(skill_ids or []))),
    }


def skill_resources(
    settings: Settings,
    selected_ids: list[str],
    question: str = "",
    task_mode: str = "",
) -> list[JsonObject]:
    selected: list[tuple[JsonObject, str, int]] = []
    for skill_id in selected_ids:
        selected.append((get_skill(settings, skill_id, include_content=True), "explicit", 0))
    if not selected:
        indexed = [
            get_skill(settings, item["skillId"], include_content=True)
            for item in list_skills(settings)
        ]
        scored = []
        for skill in indexed:
            score = skill_match_score(skill, question, task_mode)
            if skill["includeByDefault"]:
                scored.append((skill, "default", score))
            elif score > 0:
                scored.append((skill, "auto", score))
        selected = sorted(
            scored,
            key=lambda item: (-item[2], -int(item[0]["priority"]), item[0]["displayName"]),
        )
    resources = []
    for skill, reason, score in selected:
        resources.append(
            {
                "kind": "diagnostic_skill",
                "skillId": skill["skillId"],
                "selectionReason": reason,
                "matchScore": score,
                "revision": skill["revision"],
                "sourcePath": skill["sourcePath"],
                "summary": skill["description"],
                "content": skill.get("content", ""),
                "references": skill["references"],
            }
        )
    return resources


def skill_match_score(skill: JsonObject, question: str, task_mode: str) -> int:
    haystack = f"{question}\n{task_mode}".lower()
    score = 0
    weighted_fields = [
        ("keywords", 6),
        ("products", 5),
        ("toolIds", 4),
        ("domainAdapters", 4),
    ]
    for field, weight in weighted_fields:
        for term in skill.get(field, []):
            score += weight if term_matches_question(str(term), haystack) else 0
    for term in (
        skill.get("skillId"),
        skill.get("name"),
        skill.get("displayName"),
        skill.get("description"),
    ):
        score += 2 if term_matches_question(str(term or ""), haystack) else 0
    return score


def term_matches_question(term: str, haystack: str) -> bool:
    normalized = term.strip().lower()
    if not normalized:
        return False
    if normalized in haystack and meaningful_term(normalized):
        return True
    return any(token in haystack for token in match_terms(normalized))


def match_terms(value: str) -> list[str]:
    return [
        token
        for token in re.findall(r"[a-z0-9_.:-]+|[\u4e00-\u9fff]{2,}", value.lower())
        if meaningful_term(token)
    ]


def meaningful_term(value: str) -> bool:
    if re.search(r"[\u4e00-\u9fff]", value):
        return len(value) >= 2
    return len(value) >= 3


def persist_system_context(
    settings: Settings,
    store: Store,
    workspace_id: str,
    run_id: str,
) -> JsonObject:
    context = build_system_context(settings, store, workspace_id, run_id)
    data = json.dumps(context, ensure_ascii=True, indent=2).encode("utf-8")
    artifact = write_artifact_bytes(
        settings=settings,
        store=store,
        workspace_id=workspace_id,
        filename="system_context.json",
        data=data,
        content_type="application/json",
        schema_name="logagent.v2.system_context.v2",
        preview={"resourceCount": len(context["resources"])},
    )
    store.create_evidence(
        workspace_id=workspace_id,
        run_id=run_id,
        kind="system_context",
        final_allowed=False,
        summary=f"System Context captured {len(context['resources'])} resources.",
        artifact_id=artifact["id"],
        payload={"artifactId": artifact["id"], "path": "system_context.json"},
    )
    return {"context": context, "artifact": artifact}


def read_task_skill_reference(
    settings: Settings,
    store: Store,
    run_id: str,
    skill_id: str,
    reference_id: str | None,
    path: str | None,
) -> JsonObject:
    context = read_system_context_artifact(settings, store, run_id)
    resource = find_skill_resource(context, skill_id)
    if resource is None:
        raise ValueError(f"skill {skill_id} is not selected in this run")
    current = get_skill(settings, skill_id, include_content=False)
    if current["revision"] != resource["revision"]:
        raise ValueError(f"skill {skill_id} revision changed since run snapshot")
    reference = find_reference(resource["references"], reference_id, path)
    content = read_reference_content(settings, skill_id, reference["path"])
    ref_id = new_id("skillref")
    artifact_path = f"skill_references/{ref_id}.json"
    value = {
        "schemaVersion": 1,
        "skillId": skill_id,
        "reference": reference,
        "content": content,
        "backgroundRef": f"{artifact_path}#content",
        "finalEvidenceAllowed": False,
    }
    artifact = write_artifact_bytes(
        settings=settings,
        store=store,
        workspace_id=store.get_run(run_id)["workspace_id"],
        filename=f"{ref_id}.json",
        data=json.dumps(value, ensure_ascii=True, indent=2).encode("utf-8"),
        content_type="application/json",
        schema_name="logagent.v2.skill_reference.v1",
        preview={"skillId": skill_id, "path": reference["path"]},
    )
    store.create_evidence(
        workspace_id=store.get_run(run_id)["workspace_id"],
        run_id=run_id,
        kind="skill_reference",
        final_allowed=False,
        summary=f"Skill reference {skill_id}:{reference['path']}.",
        artifact_id=artifact["id"],
        payload={"artifactId": artifact["id"], "backgroundRef": value["backgroundRef"]},
    )
    return value


def read_readonly_skill_reference(
    settings: Settings,
    skill_id: str,
    reference_id: str | None,
    path: str | None,
) -> JsonObject:
    skill = get_skill(settings, skill_id, include_content=False)
    reference = find_reference(skill["references"], reference_id, path)
    return {
        "schemaVersion": 1,
        "skillId": skill_id,
        "reference": reference,
        "content": read_reference_content(settings, skill_id, reference["path"]),
        "finalEvidenceAllowed": False,
    }


def read_system_context_artifact(settings: Settings, store: Store, run_id: str) -> JsonObject:
    candidates = [
        item
        for item in store.list_evidence(run_id)
        if item["kind"] == "system_context" and item.get("artifact_id")
    ]
    if not candidates:
        raise ValueError(f"run {run_id} has no system_context")
    artifact = store.get_artifact(candidates[-1]["artifact_id"])
    path = resolve_artifact_path(settings, artifact["relative_path"])
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError("system_context artifact is invalid")
    return value


def find_skill_resource(context: JsonObject, skill_id: str) -> JsonObject | None:
    for resource in context.get("resources", []):
        if resource.get("kind") == "diagnostic_skill" and resource.get("skillId") == skill_id:
            return resource
    return None


def find_reference(
    references: list[JsonObject], reference_id: str | None, path: str | None
) -> JsonObject:
    for reference in references:
        if reference_id and reference.get("referenceId") == reference_id:
            return reference
        if path and reference.get("path") == path:
            return reference
    raise ValueError("skill reference is not declared")


def read_reference_content(settings: Settings, skill_id: str, path: str) -> str:
    skill_dir = settings.skills_dir / skill_id
    target = validate_reference_path(skill_dir, path)
    return target.read_text(encoding="utf-8")[:20000]


def validate_skill_id(skill_id: str) -> None:
    if not SKILL_ID_RE.fullmatch(skill_id) or not any(ch.isalnum() for ch in skill_id):
        raise ValueError(f"invalid skillId {skill_id!r}")


def list_of_strings(value: Any) -> list[str]:
    if not isinstance(value, list):
        return []
    return [item for item in value if isinstance(item, str)]


def skill_tool_descriptors() -> list[JsonObject]:
    return [
        {
            "name": "logagent.list_skills",
            "description": "List V2 diagnostic skills.",
            "inputSchema": {"type": "object", "additionalProperties": False},
        },
        {
            "name": "logagent.get_skill",
            "description": "Read one V2 diagnostic skill by skillId.",
            "inputSchema": {
                "type": "object",
                "properties": {"skillId": {"type": "string", "minLength": 1}},
                "required": ["skillId"],
                "additionalProperties": False,
            },
        },
        {
            "name": "logagent.get_skill_reference",
            "description": "Read a declared diagnostic skill reference.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "skillId": {"type": "string", "minLength": 1},
                    "referenceId": {"type": "string"},
                    "path": {"type": "string"},
                },
                "required": ["skillId"],
                "additionalProperties": False,
            },
        },
        {
            "name": "logagent.preview_system_context",
            "description": "Preview selected V2 diagnostic skills without writing a run.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "skillIds": {"type": "array", "items": {"type": "string"}},
                },
                "additionalProperties": False,
            },
        },
    ]
