from __future__ import annotations

from fastapi import Header, HTTPException

from .config import Settings


def auth_dependency(settings: Settings):
    async def require_auth(authorization: str | None = Header(default=None)) -> None:
        if not settings.api_key:
            return
        expected = f"Bearer {settings.api_key}"
        if authorization != expected:
            raise HTTPException(status_code=401, detail="missing or invalid bearer token")

    return require_auth

