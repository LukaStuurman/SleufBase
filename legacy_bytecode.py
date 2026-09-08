from __future__ import annotations

import hashlib
import hmac
import importlib.util
import json
import marshal
import sys
from pathlib import Path
from types import CodeType
from typing import Any, MutableMapping


_MANIFEST_FILENAME = "legacy_bytecode_manifest.json"
_MANIFEST_SCHEMA_VERSION = 1
_HASH_ALGORITHM = "sha256"


class LegacyBytecodeError(ImportError):
    """Raised when bundled legacy Python bytecode cannot be trusted or loaded."""


def legacy_bytecode_path(module_stem: str, package_file: str | Path) -> Path:
    cache_tag = sys.implementation.cache_tag
    if not cache_tag:
        raise LegacyBytecodeError("Python cache tag is niet beschikbaar.")
    return Path(package_file).resolve().parent / "_bytecode" / f"{module_stem}.{cache_tag}.pyc"


def legacy_bytecode_manifest_path(package_file: str | Path) -> Path:
    return Path(package_file).resolve().parent / "_bytecode" / _MANIFEST_FILENAME


def _load_manifest(package_file: str | Path) -> dict[str, Any]:
    path = legacy_bytecode_manifest_path(package_file)
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as exc:
        raise LegacyBytecodeError(f"Legacy bytecode-integriteitsmanifest ontbreekt: {path}") from exc
    except (OSError, json.JSONDecodeError, TypeError, ValueError) as exc:
        raise LegacyBytecodeError(f"Legacy bytecode-integriteitsmanifest is ongeldig: {path} ({exc})") from exc

    if not isinstance(payload, dict):
        raise LegacyBytecodeError(f"Legacy bytecode-integriteitsmanifest heeft geen object-root: {path}")
    if payload.get("schema_version") != _MANIFEST_SCHEMA_VERSION:
        raise LegacyBytecodeError(
            f"Legacy bytecode-integriteitsmanifest heeft een onbekende schema-versie: {path}"
        )
    if payload.get("hash_algorithm") != _HASH_ALGORITHM:
        raise LegacyBytecodeError(
            f"Legacy bytecode-integriteitsmanifest gebruikt niet {_HASH_ALGORITHM}: {path}"
        )
    if not isinstance(payload.get("modules"), dict):
        raise LegacyBytecodeError(f"Legacy bytecode-integriteitsmanifest mist modules: {path}")
    return payload


def _expected_integrity(module_stem: str, package_file: str | Path, path: Path) -> tuple[int, str]:
    manifest_path = legacy_bytecode_manifest_path(package_file)
    modules = _load_manifest(package_file)["modules"]
    entry = modules.get(module_stem)
    if not isinstance(entry, dict):
        raise LegacyBytecodeError(
            f"Legacy bytecode-integriteitsmanifest bevat geen entry voor {module_stem}: {manifest_path}"
        )

    filename = entry.get("filename")
    if filename != path.name:
        raise LegacyBytecodeError(
            f"Legacy bytecode-integriteitsmanifest verwacht {filename!r} voor {module_stem}, "
            f"maar runtime verwacht {path.name!r}"
        )

    size = entry.get("size")
    digest = entry.get("sha256")
    if not isinstance(size, int) or size < 16:
        raise LegacyBytecodeError(f"Ongeldige bytecodegrootte in integriteitsmanifest voor {module_stem}")
    if (
        not isinstance(digest, str)
        or len(digest) != 64
        or any(character not in "0123456789abcdef" for character in digest)
    ):
        raise LegacyBytecodeError(f"Ongeldige SHA-256 in integriteitsmanifest voor {module_stem}")
    return size, digest


def _validate_integrity(module_stem: str, package_file: str | Path, path: Path, data: bytes) -> None:
    expected_size, expected_digest = _expected_integrity(module_stem, package_file, path)
    if len(data) != expected_size:
        raise LegacyBytecodeError(
            f"Bytecode-integriteitscontrole faalde voor {module_stem}: "
            f"grootte {len(data)} != verwacht {expected_size} ({path})"
        )
    actual_digest = hashlib.sha256(data).hexdigest()
    if not hmac.compare_digest(actual_digest, expected_digest):
        raise LegacyBytecodeError(
            f"Bytecode-integriteitscontrole faalde voor {module_stem}: SHA-256 komt niet overeen ({path})"
        )


def read_validated_code(module_stem: str, package_file: str | Path) -> CodeType:
    path = legacy_bytecode_path(module_stem, package_file)
    try:
        data = path.read_bytes()
    except FileNotFoundError as exc:
        raise LegacyBytecodeError(f"Bytecode voor {module_stem} niet gevonden: {path}") from exc
    except OSError as exc:
        raise LegacyBytecodeError(f"Bytecode voor {module_stem} kon niet worden gelezen: {path} ({exc})") from exc

    _validate_integrity(module_stem, package_file, path, data)

    if len(data) < 16:
        raise LegacyBytecodeError(f"Bytecode voor {module_stem} heeft een ongeldige/truncated header: {path}")
    if data[:4] != importlib.util.MAGIC_NUMBER:
        raise LegacyBytecodeError(
            f"Bytecode voor {module_stem} hoort niet bij deze Python-runtime "
            f"({sys.version_info.major}.{sys.version_info.minor}): {path}"
        )

    try:
        code = marshal.loads(data[16:])
    except (EOFError, ValueError, TypeError) as exc:
        raise LegacyBytecodeError(f"Bytecode voor {module_stem} is beschadigd: {path}") from exc
    if not isinstance(code, CodeType):
        raise LegacyBytecodeError(f"Bytecode voor {module_stem} bevat geen uitvoerbaar Python-codeobject: {path}")
    return code


def validate_legacy_bytecode(module_stem: str, package_file: str | Path) -> Path:
    """Validate integrity and Python compatibility without executing the bytecode."""
    read_validated_code(module_stem, package_file)
    return legacy_bytecode_path(module_stem, package_file)


def load_legacy_module(
    module_stem: str,
    namespace: MutableMapping[str, Any],
    package_file: str | Path,
) -> Path:
    """Validate and execute the bundled legacy module into an existing namespace."""
    code = read_validated_code(module_stem, package_file)
    exec(code, namespace)
    return legacy_bytecode_path(module_stem, package_file)
