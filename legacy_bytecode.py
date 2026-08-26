from __future__ import annotations

import importlib.util
import marshal
import sys
from pathlib import Path
from types import CodeType
from typing import Any, MutableMapping


class LegacyBytecodeError(ImportError):
    """Raised when bundled legacy Python bytecode cannot be trusted or loaded."""


def legacy_bytecode_path(module_stem: str, package_file: str | Path) -> Path:
    cache_tag = sys.implementation.cache_tag
    if not cache_tag:
        raise LegacyBytecodeError("Python cache tag is niet beschikbaar.")
    return Path(package_file).resolve().parent / "_bytecode" / f"{module_stem}.{cache_tag}.pyc"


def read_validated_code(module_stem: str, package_file: str | Path) -> CodeType:
    path = legacy_bytecode_path(module_stem, package_file)
    try:
        data = path.read_bytes()
    except FileNotFoundError as exc:
        raise LegacyBytecodeError(f"Bytecode voor {module_stem} niet gevonden: {path}") from exc
    except OSError as exc:
        raise LegacyBytecodeError(f"Bytecode voor {module_stem} kon niet worden gelezen: {path} ({exc})") from exc

    if len(data) < 16:
        raise LegacyBytecodeError(f"Bytecode voor {module_stem} heeft een ongeldige/truncated header: {path}")
    if data[:4] != importlib.util.MAGIC_NUMBER:
        raise LegacyBytecodeError(
            f"Bytecode voor {module_stem} hoort niet bij deze Python-runtime ({sys.version_info.major}.{sys.version_info.minor}): {path}"
        )

    try:
        code = marshal.loads(data[16:])
    except (EOFError, ValueError, TypeError) as exc:
        raise LegacyBytecodeError(f"Bytecode voor {module_stem} is beschadigd: {path}") from exc
    if not isinstance(code, CodeType):
        raise LegacyBytecodeError(f"Bytecode voor {module_stem} bevat geen uitvoerbaar Python-codeobject: {path}")
    return code


def validate_legacy_bytecode(module_stem: str, package_file: str | Path) -> Path:
    """Validate a bundled pyc without executing it and return its resolved path."""
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
