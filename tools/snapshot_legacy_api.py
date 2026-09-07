from __future__ import annotations

"""Create a deterministic contract snapshot of the current legacy bytecode core.

The snapshot is characterization data, not a source-code substitute. It records
which names/classes/signatures the rest of SleufBase can currently observe so a
reviewed Python replacement can be compared before cut-over.
"""

import argparse
import hashlib
import importlib
import inspect
import json
import os
from pathlib import Path
import sys
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[1]
PACKAGE_PARENT = REPO_ROOT.parent
if str(PACKAGE_PARENT) not in sys.path:
    sys.path.insert(0, str(PACKAGE_PARENT))

SOURCE_ENV = "SLEUFBASE_MIGRATION_SOURCE_MODULES"
CORE_MODULES = (
    "streetsmart",
    "streetsmart_browser",
    "streetsmart_panel",
    "settings",
    "app",
)

_SIMPLE_TYPES = (str, int, float, bool, type(None))


def _safe_signature(value: Any) -> str | None:
    try:
        return str(inspect.signature(value))
    except (TypeError, ValueError):
        return None


def _simple_value(value: Any) -> Any:
    if isinstance(value, _SIMPLE_TYPES):
        return value
    if isinstance(value, tuple) and all(isinstance(item, _SIMPLE_TYPES) for item in value):
        return list(value)
    if isinstance(value, list) and all(isinstance(item, _SIMPLE_TYPES) for item in value):
        return value
    if isinstance(value, dict) and all(
        isinstance(key, str) and isinstance(item, _SIMPLE_TYPES)
        for key, item in value.items()
    ):
        return dict(sorted(value.items()))
    return None


def _class_contract(cls: type[Any]) -> dict[str, Any]:
    members: dict[str, Any] = {}
    for name, value in sorted(vars(cls).items()):
        if name.startswith("__") and name not in {"__init__", "__call__"}:
            continue
        entry: dict[str, Any] = {"kind": type(value).__name__}
        target = value
        if isinstance(value, (staticmethod, classmethod)):
            target = value.__func__
        signature = _safe_signature(target) if callable(target) else None
        if signature is not None:
            entry["signature"] = signature
        simple = _simple_value(value)
        if simple is not None:
            entry["value"] = simple
        members[name] = entry
    return {
        "bases": [base.__module__ + "." + base.__qualname__ for base in cls.__bases__],
        "members": members,
    }


def _module_contract(module_name: str) -> dict[str, Any]:
    module = importlib.import_module(f"SleufBase.{module_name}")
    namespace = vars(module)
    names = sorted(name for name in namespace if not name.startswith("__"))

    functions: dict[str, Any] = {}
    classes: dict[str, Any] = {}
    constants: dict[str, Any] = {}
    for name in names:
        value = namespace[name]
        if inspect.isfunction(value) or inspect.isbuiltin(value):
            functions[name] = {"signature": _safe_signature(value)}
        elif inspect.isclass(value) and value.__module__ == module.__name__:
            classes[name] = _class_contract(value)
        elif name.isupper():
            simple = _simple_value(value)
            if simple is not None:
                constants[name] = simple

    return {
        "implementation": namespace.get("__sleufbase_core_implementation__", "unknown"),
        "names": names,
        "functions": functions,
        "classes": classes,
        "constants": constants,
    }


def _bytecode_inventory() -> dict[str, Any]:
    inventory: dict[str, Any] = {}
    bytecode_dir = REPO_ROOT / "_bytecode"
    for module_name in CORE_MODULES:
        matches = sorted(bytecode_dir.glob(f"{module_name}.cpython-*.pyc"))
        if len(matches) != 1:
            inventory[module_name] = {
                "error": f"verwacht exact 1 pyc, gevonden: {len(matches)}"
            }
            continue
        path = matches[0]
        data = path.read_bytes()
        inventory[module_name] = {
            "path": path.relative_to(REPO_ROOT).as_posix(),
            "size": len(data),
            "sha256": hashlib.sha256(data).hexdigest(),
        }
    return inventory


def build_snapshot() -> dict[str, Any]:
    # Characterization must always describe the legacy baseline, independent of
    # a developer's local migration environment.
    os.environ.pop(SOURCE_ENV, None)
    return {
        "format": 1,
        "python": f"{sys.version_info.major}.{sys.version_info.minor}.{sys.version_info.micro}",
        "bytecode": _bytecode_inventory(),
        "modules": {name: _module_contract(name) for name in CORE_MODULES},
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--output",
        type=Path,
        default=REPO_ROOT / "migration" / "contracts" / "legacy_api.json",
    )
    args = parser.parse_args()

    payload = build_snapshot()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(args.output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
