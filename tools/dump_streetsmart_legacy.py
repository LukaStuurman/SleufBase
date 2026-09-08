from __future__ import annotations

import dis
import inspect
import json
from pathlib import Path
from types import CodeType, FunctionType

from legacy_bytecode import read_validated_code


REPO_ROOT = Path(__file__).resolve().parents[1]


def _safe_constant(value):
    if isinstance(value, (str, int, float, bool)) or value is None:
        return value
    if isinstance(value, tuple):
        return [_safe_constant(item) for item in value]
    return repr(value)


def main() -> int:
    namespace = {"__name__": "_legacy_streetsmart_probe", "__file__": str(REPO_ROOT / "streetsmart.py")}
    code = read_validated_code("streetsmart", REPO_ROOT / "legacy_bytecode.py")
    exec(code, namespace)

    print("=== MODULE VALUES ===")
    values = {}
    for name, value in sorted(namespace.items()):
        if name.startswith("__") or isinstance(value, FunctionType):
            continue
        if inspect.ismodule(value) or inspect.isclass(value):
            continue
        values[name] = _safe_constant(value)
    print(json.dumps(values, ensure_ascii=False, indent=2, sort_keys=True))

    print("=== FUNCTIONS ===")
    for name, value in sorted(namespace.items()):
        if not isinstance(value, FunctionType) or value.__module__ != "_legacy_streetsmart_probe":
            continue
        print(f"\n--- {name}{inspect.signature(value)} ---")
        print("defaults:", repr(value.__defaults__), "kwdefaults:", repr(value.__kwdefaults__))
        print("constants:", repr(tuple(_safe_constant(item) for item in value.__code__.co_consts)))
        dis.dis(value)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
