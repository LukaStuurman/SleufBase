from __future__ import annotations

import base64
import json
import os
import time
from pathlib import Path
from typing import Any


TOKEN_MAX_AGE_SECONDS = 60 * 60


def _token_path() -> Path:
    base = os.environ.get("LOCALAPPDATA") or os.environ.get("APPDATA")
    root = Path(base) if base else (Path.home() / ".sleufbase")
    return root / "SleufBase" / "auth" / "streetsmart_bearer.json"


def _jwt_expiry(token: str) -> float | None:
    """Read the JWT exp claim for expiry handling only; no trust decision is made."""

    parts = str(token or "").split(".")
    if len(parts) < 2:
        return None
    try:
        payload = parts[1] + ("=" * (-len(parts[1]) % 4))
        data = json.loads(base64.urlsafe_b64decode(payload.encode("ascii")).decode("utf-8"))
        exp = data.get("exp") if isinstance(data, dict) else None
        return float(exp) if exp is not None else None
    except (ValueError, TypeError, json.JSONDecodeError, UnicodeDecodeError):
        return None


def save_streetsmart_bearer_token(token: str, permissions: Any = None) -> bool:
    token = str(token or "").strip()
    if token.lower().startswith("bearer "):
        token = token[7:].strip()
    if not token:
        return False

    now = time.time()
    payload = {
        "token": token,
        "captured_at": now,
        "expires_at": _jwt_expiry(token),
        "permissions": permissions if isinstance(permissions, (list, tuple, dict, str, int, float, bool)) else None,
    }
    path = _token_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    temp_path = path.with_suffix(".tmp")
    temp_path.write_text(json.dumps(payload, ensure_ascii=False), encoding="utf-8")
    temp_path.replace(path)
    try:
        os.chmod(path, 0o600)
    except OSError:
        pass
    return True


def load_streetsmart_bearer_token(max_age_seconds: int = TOKEN_MAX_AGE_SECONDS) -> str | None:
    path = _token_path()
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (FileNotFoundError, OSError, json.JSONDecodeError, TypeError, ValueError):
        return None
    if not isinstance(payload, dict):
        return None

    token = str(payload.get("token") or "").strip()
    if not token:
        return None

    now = time.time()
    try:
        captured_at = float(payload.get("captured_at") or 0.0)
    except (TypeError, ValueError):
        captured_at = 0.0
    if captured_at <= 0 or (now - captured_at) > max(60, int(max_age_seconds)):
        clear_streetsmart_bearer_token()
        return None

    try:
        expires_at_raw = payload.get("expires_at")
        expires_at = float(expires_at_raw) if expires_at_raw is not None else None
    except (TypeError, ValueError):
        expires_at = None
    if expires_at is not None and now >= (expires_at - 30):
        clear_streetsmart_bearer_token()
        return None
    return token


def clear_streetsmart_bearer_token() -> None:
    try:
        _token_path().unlink(missing_ok=True)
    except OSError:
        pass


def bearer_authorization_header(token: str) -> str:
    normalized = str(token or "").strip()
    if normalized.lower().startswith("bearer "):
        return normalized
    return f"Bearer {normalized}"
