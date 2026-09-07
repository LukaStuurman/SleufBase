from __future__ import annotations

"""Recover nested function/method source fragments from the legacy code tree.

Module-level decompilers often fail on Python 3.11 MAKE_FUNCTION/class-building
instructions even when individual function code objects are straightforward.
This tool decompiles every nested code object independently and records enough
metadata to stitch reviewed modules/classes back together deterministically.
"""

import argparse
import json
from pathlib import Path
import sys
from types import CodeType
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[1]
PACKAGE_PARENT = REPO_ROOT.parent
if str(PACKAGE_PARENT) not in sys.path:
    sys.path.insert(0, str(PACKAGE_PARENT))

from SleufBase.legacy_bytecode import read_validated_code

CORE_MODULES = (
    "streetsmart",
    "streetsmart_browser",
    "streetsmart_panel",
    "settings",
    "app",
)


def _walk(code: CodeType):
    for const in code.co_consts:
        if not isinstance(const, CodeType):
            continue
        yield const
        yield from _walk(const)


def _metadata(code: CodeType) -> dict[str, Any]:
    positional = code.co_argcount
    posonly = getattr(code, "co_posonlyargcount", 0)
    kwonly = code.co_kwonlyargcount
    named_count = positional + kwonly
    return {
        "name": code.co_name,
        "qualname": getattr(code, "co_qualname", code.co_name),
        "firstlineno": code.co_firstlineno,
        "argcount": positional,
        "posonlyargcount": posonly,
        "kwonlyargcount": kwonly,
        "argument_names": list(code.co_varnames[:named_count]),
        "flags": code.co_flags,
        "freevars": list(code.co_freevars),
        "cellvars": list(code.co_cellvars),
    }


def recover_fragments(module_name: str, output_dir: Path) -> dict[str, Any]:
    try:
        import depyf
    except ImportError as exc:
        raise RuntimeError("depyf ontbreekt") from exc

    root = read_validated_code(module_name, REPO_ROOT / "legacy_bytecode.py")
    fragments: list[dict[str, Any]] = []
    for index, code in enumerate(_walk(root), start=1):
        item = _metadata(code)
        item["index"] = index
        try:
            source = depyf.decompile(code)
            if not isinstance(source, str) or not source.strip():
                raise RuntimeError("lege depyf-output")
            item["source"] = source.rstrip() + "\n"
            item["decompiled"] = True
            try:
                compile(item["source"], f"<{module_name}:{item['qualname']}>", "exec")
                item["compile_ok"] = True
            except Exception as exc:
                item["compile_ok"] = False
                item["compile_error"] = f"{type(exc).__name__}: {exc}"
        except Exception as exc:
            item["decompiled"] = False
            item["compile_ok"] = False
            item["error"] = f"{type(exc).__name__}: {exc}"
        fragments.append(item)

    output_dir.mkdir(parents=True, exist_ok=True)
    path = output_dir / f"{module_name}.fragments.json"
    path.write_text(
        json.dumps(fragments, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    successful = sum(1 for item in fragments if item["decompiled"])
    compilable = sum(1 for item in fragments if item["compile_ok"])
    return {
        "module": module_name,
        "path": path.name,
        "total": len(fragments),
        "decompiled": successful,
        "compile_ok": compilable,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=REPO_ROOT / "migration" / "recovered",
    )
    parser.add_argument(
        "--modules",
        nargs="*",
        choices=CORE_MODULES,
        default=list(CORE_MODULES),
    )
    args = parser.parse_args()

    if sys.version_info[:2] != (3, 11):
        raise SystemExit("Function-fragment recovery moet onder Python 3.11 draaien")

    report = [recover_fragments(name, args.output_dir) for name in args.modules]
    report_path = args.output_dir / "fragment-recovery-report.json"
    report_path.write_text(
        json.dumps(report, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    for item in report:
        print(
            f"{item['module']}: {item['decompiled']}/{item['total']} gedecompileerd, "
            f"{item['compile_ok']} standalone compile-ok"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
