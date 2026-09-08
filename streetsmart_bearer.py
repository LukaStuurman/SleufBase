from __future__ import annotations

import base64
import ctypes
import json
import os
import time
from pathlib import Path
from typing import Any


TOKEN_MAX_AGE_SECONDS = 60 * 60
_TOKEN_SCHEMA_VERSION = 2
_PROTECTION_DPAPI = "dpapi-current-user"
_PROTECTION_PLAINTEXT = "plaintext-nonwindows"
_CRYPTPROTECT_UI_FORBIDDEN = 0x1


class _DataBlob(ctypes.Structure):
    _fields_ = [
        ("cbData", ctypes.c_uint32),
        ("pbData", ctypes.POINTER(ctypes.c_ubyte)),
    ]


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


def _blob_from_bytes(data: bytes) -> tuple[_DataBlob, ctypes.Array[Any]]:
    buffer = ctypes.create_string_buffer(data)
    blob = _DataBlob(
        len(data),
        ctypes.cast(buffer, ctypes.POINTER(ctypes.c_ubyte)),
    )
    return blob, buffer


def _configure_dpapi_functions():
    crypt32 = ctypes.windll.crypt32
    kernel32 = ctypes.windll.kernel32

    crypt32.CryptProtectData.argtypes = [
        ctypes.POINTER(_DataBlob),
        ctypes.c_wchar_p,
        ctypes.POINTER(_DataBlob),
        ctypes.c_void_p,
        ctypes.c_void_p,
        ctypes.c_uint32,
        ctypes.POINTER(_DataBlob),
    ]
    crypt32.CryptProtectData.restype = ctypes.c_int
    crypt32.CryptUnprotectData.argtypes = [
        ctypes.POINTER(_DataBlob),
        ctypes.c_void_p,
        ctypes.POINTER(_DataBlob),
        ctypes.c_void_p,
        ctypes.c_void_p,
        ctypes.c_uint32,
        ctypes.POINTER(_DataBlob),
    ]
    crypt32.CryptUnprotectData.restype = ctypes.c_int
    kernel32.LocalFree.argtypes = [ctypes.c_void_p]
    kernel32.LocalFree.restype = ctypes.c_void_p
    return crypt32, kernel32


def _dpapi_protect_text(value: str) -> str:
    if os.name != "nt":
        raise OSError("Windows DPAPI is alleen beschikbaar op Windows.")
    source, source_buffer = _blob_from_bytes(value.encode("utf-8"))
    output = _DataBlob()
    crypt32, kernel32 = _configure_dpapi_functions()
    _ = source_buffer
    if not crypt32.CryptProtectData(
        ctypes.byref(source),
        "SleufBase StreetSmart bearer",
        None,
        None,
        None,
        _CRYPTPROTECT_UI_FORBIDDEN,
        ctypes.byref(output),
    ):
        raise OSError("Windows DPAPI kon het StreetSmart-token niet versleutelen.")
    try:
        encrypted = ctypes.string_at(output.pbData, output.cbData)
    finally:
        if output.pbData:
            kernel32.LocalFree(output.pbData)
    return base64.b64encode(encrypted).decode("ascii")


def _dpapi_unprotect_text(value: str) -> str:
    if os.name != "nt":
        raise OSError("Windows DPAPI is alleen beschikbaar op Windows.")
    try:
        encrypted = base64.b64decode(value.encode("ascii"), validate=True)
    except (ValueError, UnicodeEncodeError) as exc:
        raise OSError("Ongeldige DPAPI-tokenopslag.") from exc
    source, source_buffer = _blob_from_bytes(encrypted)
    output = _DataBlob()
    crypt32, kernel32 = _configure_dpapi_functions()
    _ = source_buffer
    if not crypt32.CryptUnprotectData(
        ctypes.byref(source),
        None,
        None,
        None,
        None,
        _CRYPTPROTECT_UI_FORBIDDEN,
        ctypes.byref(output),
    ):
        raise OSError("Windows DPAPI kon het StreetSmart-token niet ontsleutelen.")
    try:
        decrypted = ctypes.string_at(output.pbData, output.cbData)
    finally:
        if output.pbData:
            kernel32.LocalFree(output.pbData)
    try:
        return decrypted.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise OSError("Ontsleuteld StreetSmart-token is geen geldige UTF-8 tekst.") from exc


def _normalized_permissions(permissions: Any) -> Any:
    if isinstance(permissions, (list, tuple, dict, str, int, float, bool)):
        return permissions
    return None


def _write_token_payload(
    token: str,
    *,
    captured_at: float,
    expires_at: float | None,
    permissions: Any,
) -> bool:
    payload: dict[str, Any] = {
        "schema_version": _TOKEN_SCHEMA_VERSION,
        "captured_at": captured_at,
        "expires_at": expires_at,
        "permissions": _normalized_permissions(permissions),
    }
    if os.name == "nt":
        try:
            payload["token_protected"] = _dpapi_protect_text(token)
        except OSError:
            return False
        payload["protection"] = _PROTECTION_DPAPI
    else:
        # SleufBase production builds target Windows. Keeping a plaintext format on
        # non-Windows hosts preserves developer/test usability without pretending
        # to provide platform encryption that is not available there.
        payload["token"] = token
        payload["protection"] = _PROTECTION_PLAINTEXT

    path = _token_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    temp_path = path.with_name(f".{path.name}.{os.getpid()}.{time.time_ns()}.tmp")
    try:
        temp_path.write_text(json.dumps(payload, ensure_ascii=False), encoding="utf-8")
        os.replace(temp_path, path)
    finally:
        try:
            temp_path.unlink(missing_ok=True)
        except OSError:
            pass
    try:
        os.chmod(path, 0o600)
    except OSError:
        pass
    return True


def save_streetsmart_bearer_token(token: str, permissions: Any = None) -> bool:
    token = str(token or "").strip()
    if token.lower().startswith("bearer "):
        token = token[7:].strip()
    if not token:
        return False

    now = time.time()
    return _write_token_payload(
        token,
        captured_at=now,
        expires_at=_jwt_expiry(token),
        permissions=permissions,
    )


def _token_from_payload(payload: dict[str, Any]) -> str | None:
    protection = str(payload.get("protection") or "").strip()
    protected = str(payload.get("token_protected") or "").strip()
    if protection == _PROTECTION_DPAPI or protected:
        if not protected or os.name != "nt":
            return None
        try:
            return _dpapi_unprotect_text(protected).strip() or None
        except OSError:
            return None

    token = str(payload.get("token") or "").strip()
    return token or None


def load_streetsmart_bearer_token(max_age_seconds: int = TOKEN_MAX_AGE_SECONDS) -> str | None:
    path = _token_path()
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (FileNotFoundError, OSError, json.JSONDecodeError, TypeError, ValueError):
        return None
    if not isinstance(payload, dict):
        return None

    token = _token_from_payload(payload)
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

    # Migrate an existing plaintext Windows token in-place only after its original
    # age/expiry checks have passed. Preserve timestamps so migration never extends
    # the token lifetime.
    if os.name == "nt" and payload.get("protection") != _PROTECTION_DPAPI:
        _write_token_payload(
            token,
            captured_at=captured_at,
            expires_at=expires_at,
            permissions=payload.get("permissions"),
        )
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
