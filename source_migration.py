from __future__ import annotations

import os
from pathlib import Path
from typing import Any, Callable, Mapping, MutableMapping

from .legacy_bytecode import load_legacy_module


MIGRATABLE_MODULES = (
    "settings",
    "streetsmart",
    "streetsmart_browser",
    "streetsmart_panel",
    "app",
)


class SourceMigrationError(ImportError):
    """Raised when an explicit source/legacy migration selection cannot be honoured."""


def migration_environment_variable(module_stem: str) -> str:
    """Return the environment variable used to select one module implementation."""

    normalized = str(module_stem or "").strip().lower()
    if normalized not in MIGRATABLE_MODULES:
        raise SourceMigrationError(f"Onbekende migratiemodule: {module_stem!r}")
    return f"SLEUFBASE_MIGRATION_{normalized.upper()}"


def selected_implementation(
    module_stem: str,
    environ: Mapping[str, str] | None = None,
) -> str:
    """Return ``legacy`` or ``source`` for a migratable module.

    Legacy remains the hard default until a module has passed its characterization
    tests. There is intentionally no automatic fallback from source to legacy:
    source-mode failures must remain visible during the migration.
    """

    variable = migration_environment_variable(module_stem)
    environment = os.environ if environ is None else environ
    raw_value = str(environment.get(variable, "legacy") or "legacy").strip().casefold()
    if raw_value in {"legacy", "bytecode"}:
        return "legacy"
    if raw_value in {"source", "python"}:
        return "source"
    raise SourceMigrationError(
        f"Ongeldige waarde voor {variable}: {raw_value!r}. Gebruik 'legacy' of 'source'."
    )


def source_enabled(module_stem: str, environ: Mapping[str, str] | None = None) -> bool:
    return selected_implementation(module_stem, environ=environ) == "source"


def load_migrating_module(
    module_stem: str,
    namespace: MutableMapping[str, Any],
    package_file: str | Path,
    *,
    source_loader: Callable[[MutableMapping[str, Any]], None] | None = None,
) -> Path | None:
    """Load the selected implementation into ``namespace``.

    During the migration every wrapper can keep calling this helper. Legacy is
    used by default. Once normal Python source exists, the wrapper supplies a
    ``source_loader`` and CI can run the same characterization tests in both
    modes. Selecting source before such a loader exists fails explicitly.
    """

    implementation = selected_implementation(module_stem)
    if implementation == "legacy":
        return load_legacy_module(module_stem, namespace, package_file)

    if source_loader is None:
        variable = migration_environment_variable(module_stem)
        raise SourceMigrationError(
            f"Broncode-implementatie voor {module_stem!r} is nog niet beschikbaar. "
            f"Zet {variable}=legacy of voeg eerst een source_loader toe."
        )

    source_loader(namespace)
    return None


def migration_state(environ: Mapping[str, str] | None = None) -> dict[str, str]:
    """Return the selected implementation for every migration target."""

    return {
        module_stem: selected_implementation(module_stem, environ=environ)
        for module_stem in MIGRATABLE_MODULES
    }
