from __future__ import annotations

"""Recover review candidates from SleufBase's own Python 3.11 bytecode.

This is a migration aid only. Generated output is never imported by production.
Every recovered module must still pass characterization/regression tests and be
manually reviewed before it is copied into ``SleufBase._source``.
"""

import argparse
import dis
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


def _walk_code(code: CodeType, prefix: str = ""):
    qualname = getattr(code, "co_qualname", code.co_name)
    label = f"{prefix}{qualname}" if prefix else qualname
    yield label, code
    child_prefix = label + "."
    for const in code.co_consts:
        if isinstance(const, CodeType):
            yield from _walk_code(const, child_prefix)


def _disassembly(code: CodeType) -> str:
    chunks: list[str] = []
    for label, child in _walk_code(code):
        chunks.append(
            f"\n===== {label} (line {child.co_firstlineno}) =====\n"
        )
        try:
            chunks.append(dis.Bytecode(child).dis())
        except Exception as exc:  # diagnostic fallback must keep going
            chunks.append(f"<disassembly failed: {type(exc).__name__}: {exc}>\n")
    return "".join(chunks).lstrip()


def _decompile(code: CodeType) -> str:
    try:
        import depyf
    except ImportError as exc:
        raise RuntimeError(
            "depyf ontbreekt; installeer de gepinde migratie-afhankelijkheid depyf==0.20.0"
        ) from exc
    source = depyf.decompile(code)
    if not isinstance(source, str) or not source.strip():
        raise RuntimeError("depyf gaf geen broncode terug")
    return source.rstrip() + "\n"


def recover_module(module_name: str, output_dir: Path) -> dict[str, Any]:
    code = read_validated_code(module_name, REPO_ROOT / "legacy_bytecode.py")
    output_dir.mkdir(parents=True, exist_ok=True)

    disassembly_path = output_dir / f"{module_name}.dis.txt"
    disassembly_path.write_text(_disassembly(code), encoding="utf-8")

    result: dict[str, Any] = {
        "module": module_name,
        "disassembly": disassembly_path.name,
        "decompiled": False,
        "compile_ok": False,
    }
    try:
        source = _decompile(code)
        compile(source, f"<{module_name}.recovered>", "exec")
    except Exception as exc:
        error_path = output_dir / f"{module_name}.recovery-error.txt"
        error_path.write_text(
            f"{type(exc).__name__}: {exc}\n",
            encoding="utf-8",
        )
        result["error"] = f"{type(exc).__name__}: {exc}"
        return result

    source_path = output_dir / f"{module_name}.recovered.py"
    source_path.write_text(source, encoding="utf-8")
    result.update(
        {
            "decompiled": True,
            "compile_ok": True,
            "source": source_path.name,
            "source_lines": source.count("\n"),
        }
    )
    return result


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
        raise SystemExit(
            f"Recovery moet onder Python 3.11 draaien; actief is {sys.version_info.major}.{sys.version_info.minor}."
        )

    report = [recover_module(name, args.output_dir) for name in args.modules]
    report_path = args.output_dir / "recovery-report.json"
    report_path.write_text(
        json.dumps(report, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    for item in report:
        status = "source+compile OK" if item["compile_ok"] else f"diagnostic only: {item.get('error')}"
        print(f"{item['module']}: {status}")
    print(report_path)
    # A decompiler failure is not a product failure. The committed disassembly
    # remains the deterministic floor for manual reconstruction.
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
