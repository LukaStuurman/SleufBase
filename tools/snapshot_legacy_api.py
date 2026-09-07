from __future__ import annotations

"""Create deterministic API-contract snapshots for the core migration.

Legacy mode characterizes the current bytecode implementation. Source mode runs
one or more wrappers with their explicit ``SleufBase._source`` implementation.
The output is deliberately structural: names, signatures, classes and simple
constants. It is not a source-code substitute.
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
        "module": cls.__module__,
        "bases": [base.__module__ + "." + base.__qualname__ for base in cls.__bases__],
        "members": members,
    }


def _module_contract(module_name: str) -> dict[str, Any]:
    module = importlib.import_module(f"SleufBase.{module_name}")
    namespace = vars(module)
    names = sorted(name for name in namespace if not name.startswith("__"))
    allowed_class_modules = {
        module.__name__,
        f"SleufBase._source.{module_name}",
    }

    functions: dict[str, Any] = {}
    classes: dict[str, Any] = {}
    constants: dict[str, Any] = {}
    for name in names:
        value = namespace[name]
        if inspect.isfunction(value) or inspect.isbuiltin(value):
            functions[name] = {
                "module": getattr(value, "__module__", None),
                "signature": _safe_signature(value),
            }
        elif inspect.isclass(value) and value.__module__ in allowed_class_modules:
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


def _bytecode_inventory(modules: tuple[str, ...]) -> dict[str, Any]:
    inventory: dict[str, Any] = {}
    bytecode_dir = REPO_ROOT / "_bytecode"
    for module_name in modules:
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


def build_snapshot(
    modules: tuple[str, ...] = CORE_MODULES,
    *,
    implementation: str = "legacy",
) -> dict[str, Any]:
    if implementation == "legacy":
        os.environ.pop(SOURCE_ENV, None)
    elif implementation == "source":
        os.environ[SOURCE_ENV] = ",".join(modules)
    else:
        raise ValueError(f"onbekende implementatie: {implementation}")

    return {
        "format": 2,
        "implementation": implementation,
        "python": f"{sys.version_info.major}.{sys.version_info.minor}.{sys.version_info.micro}",
        "bytecode": _bytecode_inventory(modules),
        "modules": {name: _module_contract(name) for name in modules},
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--output",
        type=Path,
        default=REPO_ROOT / "migration" / "contracts" / "legacy_api.json",
    )
    parser.add_argument(
        "--implementation",
        choices=("legacy", "source"),
        default="legacy",
    )
    parser.add_argument(
        "--modules",
        nargs="*",
        choices=CORE_MODULES,
        default=list(CORE_MODULES),
    )
    args = parser.parse_args()

    modules = tuple(args.modules)
    payload = build_snapshot(modules, implementation=args.implementation)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(args.output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
