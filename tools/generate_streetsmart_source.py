from __future__ import annotations

import json
from pathlib import Path
import sys


REPO_ROOT = Path(__file__).resolve().parents[1]
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from legacy_bytecode import read_validated_code


OUTPUT = REPO_ROOT / "streetsmart_source.py"
USER_SENTINEL = "__SLEUFBASE_STREETSMART_USER_SENTINEL__"
PASSWORD_SENTINEL = "__SLEUFBASE_STREETSMART_PASSWORD_SENTINEL__"


def _legacy_namespace() -> dict[str, object]:
    namespace: dict[str, object] = {
        "__name__": "_legacy_streetsmart_generator",
        "__file__": str(REPO_ROOT / "streetsmart.py"),
    }
    code = read_validated_code("streetsmart", REPO_ROOT / "legacy_bytecode.py")
    exec(code, namespace)
    return namespace


def _login_script_parts(namespace: dict[str, object]) -> tuple[str, str, str]:
    factory = namespace["streetsmart_login_script"]
    script = factory(USER_SENTINEL, PASSWORD_SENTINEL)  # type: ignore[operator]
    user_json = json.dumps(USER_SENTINEL)
    password_json = json.dumps(PASSWORD_SENTINEL)
    if script.count(user_json) != 1 or script.count(password_json) != 1:
        raise RuntimeError("Legacy StreetSmart login script bevat onverwachte credential placeholders.")
    prefix, remainder = script.split(user_json, 1)
    middle, suffix = remainder.split(password_json, 1)
    return prefix, middle, suffix


def main() -> int:
    namespace = _legacy_namespace()
    prefix, middle, suffix = _login_script_parts(namespace)
    source = f'''from __future__ import annotations

import json
import os
from pathlib import Path
import re
import shutil
import time
from typing import Any
from urllib.parse import quote


STREETSMART_WEB_URL = "https://streetsmart.cyclomedia.com/streetsmart"
STREETSMART_RD_SRS = "EPSG:28992"

_LOGIN_SCRIPT_PREFIX = {prefix!r}
_LOGIN_SCRIPT_MIDDLE = {middle!r}
_LOGIN_SCRIPT_SUFFIX = {suffix!r}


def streetsmart_data_dir() -> Path:
    local_appdata = Path(os.environ.get("LOCALAPPDATA", Path.home() / "AppData" / "Local"))
    return local_appdata / "KlicTiffKaarten" / "streetsmart"


def _safe_streetsmart_slug(value: str) -> str:
    sanitized = re.sub(r"[^A-Za-z0-9._-]+", "_", str(value or "").strip().lower())
    return sanitized.strip("._-") or "default"


def streetsmart_browser_sessions_dir() -> Path:
    return streetsmart_data_dir() / "browser_sessions"


def streetsmart_browser_storage_dir(username: str) -> Path:
    return streetsmart_browser_sessions_dir() / _safe_streetsmart_slug(username)


def streetsmart_embedded_storage_dir() -> Path:
    return streetsmart_data_dir() / "embedded_webview2"


def clear_streetsmart_storage(username: str = "") -> list[Path]:
    normalized_username = str(username or "").strip()
    targets = [
        streetsmart_embedded_storage_dir(),
        streetsmart_browser_storage_dir(normalized_username)
        if normalized_username
        else streetsmart_browser_sessions_dir(),
    ]
    removed: list[Path] = []
    seen: set[str] = set()
    for target in targets:
        key = str(target).lower()
        if key in seen or not target.exists():
            continue
        seen.add(key)
        last_error: OSError | None = None
        for _attempt in range(20):
            try:
                shutil.rmtree(target)
                last_error = None
                break
            except FileNotFoundError:
                last_error = None
                break
            except OSError as exc:
                last_error = exc
                time.sleep(0.2)
        if last_error is not None:
            raise last_error
        removed.append(target)
    return removed


def streetsmart_state_path() -> Path:
    return streetsmart_data_dir() / "state.json"


def default_streetsmart_state() -> dict[str, Any]:
    return {{"version": 0, "selection": None}}


def load_streetsmart_state() -> dict[str, Any]:
    state_path = streetsmart_state_path()
    try:
        payload = json.loads(state_path.read_text(encoding="utf-8"))
    except (FileNotFoundError, OSError, json.JSONDecodeError, TypeError, ValueError):
        return default_streetsmart_state()
    if not isinstance(payload, dict):
        return default_streetsmart_state()
    version = payload.get("version", 0)
    try:
        normalized_version = int(version)
    except (TypeError, ValueError):
        normalized_version = 0
    selection = payload.get("selection")
    if selection is not None and not isinstance(selection, dict):
        selection = None
    return {{"version": normalized_version, "selection": selection}}


def save_streetsmart_state(selection: dict[str, Any] | None) -> Path:
    state_path = streetsmart_state_path()
    state_path.parent.mkdir(parents=True, exist_ok=True)
    payload = {{"version": time.time_ns(), "selection": selection}}
    state_path.write_text(
        json.dumps(payload, ensure_ascii=True, indent=2),
        encoding="utf-8",
    )
    return state_path


def streetsmart_selection_center(selection: dict[str, Any] | None) -> tuple[float, float] | None:
    if not isinstance(selection, dict):
        return None
    center = selection.get("center")
    if not isinstance(center, (list, tuple)) or len(center) < 2:
        return None
    try:
        x = float(center[0])
        y = float(center[1])
    except (TypeError, ValueError):
        return None
    if x != x or y != y:
        return None
    return x, y


def streetsmart_selection_url(selection: dict[str, Any] | None) -> str:
    center = streetsmart_selection_center(selection)
    if center is None:
        return STREETSMART_WEB_URL
    query = f"{{center[0]:.2f}};{{center[1]:.2f}};{{STREETSMART_RD_SRS}}"
    return f"{{STREETSMART_WEB_URL}}?q={{quote(query, safe=';:.-')}}"


def streetsmart_login_script(username: str, password: str) -> str:
    return (
        _LOGIN_SCRIPT_PREFIX
        + json.dumps(username)
        + _LOGIN_SCRIPT_MIDDLE
        + json.dumps(password)
        + _LOGIN_SCRIPT_SUFFIX
    )
'''
    OUTPUT.write_text(source, encoding="utf-8")
    print(f"Generated {{OUTPUT}} ({{len(source)}} chars)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
