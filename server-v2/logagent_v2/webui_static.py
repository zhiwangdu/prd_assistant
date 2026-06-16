from __future__ import annotations

from pathlib import Path


class WebuiStaticNotFound(ValueError):
    pass


def resolve_webui_asset(webui_dir: Path, asset_path: str) -> Path:
    normalized_path = asset_path.strip("/")
    if normalized_path == "api" or normalized_path.startswith("api/"):
        raise WebuiStaticNotFound("API route not found")

    root = webui_dir.resolve()
    index_path = root / "index.html"
    if not index_path.is_file():
        raise WebuiStaticNotFound(f"WebUI build not found under {root}")

    if normalized_path:
        candidate = (root / normalized_path).resolve()
        if not path_is_under(candidate, root):
            raise WebuiStaticNotFound("static asset not found")
        if candidate.is_file():
            return candidate
        if "." in Path(normalized_path).name:
            raise WebuiStaticNotFound("static asset not found")

    return index_path


def path_is_under(path: Path, root: Path) -> bool:
    try:
        path.relative_to(root)
    except ValueError:
        return False
    return True
