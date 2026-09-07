from __future__ import annotations

"""Temporary, explicit source/bytecode switch used during the core migration.

Production stays on the legacy bytecode implementation until a module has a
reviewed source implementation in ``SleufBase._source``.  There is deliberately
no automatic fallback from source to bytecode: source-mode failures must be
visible in CI instead of being silently hidden by the legacy implementation.
"""

import importlib
import os
from types import ModuleType
from typing import Any, MutableMapping

from .legacy_bytecode import load_legacy_module

SOURCE_MODULES_ENV = "SLEUFBASE_MIGRATION_SOURCE_MODULES"
MIGRATABLE_MODULES = frozenset(
    {
        "streetsmart",
        "streetsmart_browser",
        "streetsmart_panel",
        "settings",
        "app",
    }
)

_MODULE_METADATA_NAMES = frozenset(
    {
        "__name__",
        "__package__",
        "__loader__",
        "__spec__",
        "__file__",
        "__cached__",
        "__builtins__",
    }
)


class SourceMigrationError(ImportError):
    """Raised when an explicitly requested source implementation is unavailable."""


def requested_source_modules(value: str | None = None) -> frozenset[str]:
    """Return the explicitly enabled source modules.

    The environment variable accepts a comma-separated list or ``all``. Unknown
    module names are rejected so a typo can never accidentally change migration
    behaviour.
    """

    raw = os.environ.get(SOURCE_MODULES_ENV, "") if value is None else value
    tokens = {token.strip().casefold() for token in str(raw or "").split(",") if token.strip()}
    if not tokens:
        return frozenset()
    if tokens & {"all", "*"}:
        if len(tokens) != 1:
            raise SourceMigrationError(
                f"{SOURCE_MODULES_ENV}: 'all'/'*' kan niet met losse modulenamen worden gecombineerd."
            )
        return MIGRATABLE_MODULES

    unknown = tokens - MIGRATABLE_MODULES
    if unknown:
        names = ", ".join(sorted(unknown))
        raise SourceMigrationError(f"{SOURCE_MODULES_ENV}: onbekende module(s): {names}")
    return frozenset(tokens)


def source_enabled(module_stem: str) -> bool:
    normalized = str(module_stem or "").strip().casefold()
    if normalized not in MIGRATABLE_MODULES:
        raise SourceMigrationError(f"Niet-migreerbare kernmodule: {module_stem!r}")
    return normalized in requested_source_modules()


def source_module_name(module_stem: str) -> str:
    normalized = str(module_stem or "").strip().casefold()
    if normalized not in MIGRATABLE_MODULES:
        raise SourceMigrationError(f"Niet-migreerbare kernmodule: {module_stem!r}")
    package_root = __package__ or "SleufBase"
    return f"{package_root}._source.{normalized}"


def _copy_source_namespace(module: ModuleType, namespace: MutableMapping[str, Any]) -> None:
    # Legacy bytecode is executed directly into the wrapper namespace and thus
    # exposes private helpers too. Preserve that contract during migration while
    # keeping the wrapper's own import metadata intact.
    for name, value in vars(module).items():
        if name in _MODULE_METADATA_NAMES:
            continue
        namespace[name] = value


def load_source_or_legacy(
    module_stem: str,
    namespace: MutableMapping[str, Any],
    package_file: str,
) -> str:
    """Load one core module using the explicitly selected implementation.

    Source mode is strict: if ``SleufBase._source.<module>`` does not exist or
    fails while importing, the exception is propagated. The loader never falls
    back to bytecode after a source-mode failure.
    """

    normalized = str(module_stem or "").strip().casefold()
    if source_enabled(normalized):
        target = source_module_name(normalized)
        try:
            module = importlib.import_module(target)
        except ModuleNotFoundError as exc:
            if exc.name == target:
                raise SourceMigrationError(
                    f"Bronimplementatie voor {normalized} is expliciet aangezet maar ontbreekt: {target}"
                ) from exc
            raise
        _copy_source_namespace(module, namespace)
        namespace["__sleufbase_core_implementation__"] = "source"
        return "source"

    load_legacy_module(normalized, namespace, package_file)
    namespace["__sleufbase_core_implementation__"] = "legacy-bytecode"
    return "legacy-bytecode"
