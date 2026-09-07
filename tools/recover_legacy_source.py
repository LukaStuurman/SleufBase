from __future__ import annotations

"""Recover review candidates from SleufBase's own Python 3.11 bytecode.

This is a migration aid only. Generated output is never imported by production.
Every recovered module must still pass characterization/regression tests and be
manually reviewed before it is copied into ``SleufBase._source``.

Two independent recovery engines are supported:
- depyf, pinned in the migration workflow;
- an optional pinned pycdc executable, used as a second opinion for Python 3.11.

A failure in either decompiler never destroys the deterministic recursive
``dis`` output, which remains the minimum review evidence.
"""

import argparse
import dis
import json
from pathlib import Path
import subprocess
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
        chunks.append(f"\n===== {label} (line {child.co_firstlineno}) =====\n")
        try:
            chunks.append(dis.Bytecode(child).dis())
        except Exception as exc:  # diagnostic fallback must keep going
            chunks.append(f"<disassembly failed: {type(exc).__name__}: {exc}>\n")
    return "".join(chunks).lstrip()


def _legacy_pyc_path(module_name: str) -> Path:
    matches = sorted((REPO_ROOT / "_bytecode").glob(f"{module_name}.cpython-*.pyc"))
    if len(matches) != 1:
        raise RuntimeError(
            f"verwacht exact 1 bytecodebestand voor {module_name}, gevonden {len(matches)}"
        )
    return matches[0]


def _decompile_depyf(code: CodeType) -> str:
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


def _decompile_pycdc(executable: Path, module_name: str) -> str:
    process = subprocess.run(
        [str(executable), str(_legacy_pyc_path(module_name))],
        cwd=REPO_ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        encoding="utf-8",
        errors="replace",
        check=False,
        timeout=120,
    )
    source = process.stdout
    if process.returncode != 0:
        detail = process.stderr.strip() or source.strip() or f"exitcode {process.returncode}"
        raise RuntimeError(f"pycdc faalde ({process.returncode}): {detail[:2000]}")
    if not source.strip():
        raise RuntimeError("pycdc gaf geen broncode terug")
    return source.rstrip() + "\n"


def _attempt_source(
    *,
    engine: str,
    source_factory,
    module_name: str,
    output_dir: Path,
) -> dict[str, Any]:
    result: dict[str, Any] = {"engine": engine, "decompiled": False, "compile_ok": False}
    try:
        source = source_factory()
    except Exception as exc:
        result["error"] = f"{type(exc).__name__}: {exc}"
        return result

    source_path = output_dir / f"{module_name}.{engine}.py"
    source_path.write_text(source, encoding="utf-8")
    result["decompiled"] = True
    result["source"] = source_path.name
    result["source_lines"] = source.count("\n")
    try:
        compile(source, f"<{module_name}.{engine}>", "exec")
    except Exception as exc:
        result["error"] = f"{type(exc).__name__}: {exc}"
        return result
    result["compile_ok"] = True
    return result


def recover_module(
    module_name: str,
    output_dir: Path,
    pycdc: Path | None,
) -> dict[str, Any]:
    code = read_validated_code(module_name, REPO_ROOT / "legacy_bytecode.py")
    output_dir.mkdir(parents=True, exist_ok=True)

    disassembly_path = output_dir / f"{module_name}.dis.txt"
    disassembly_path.write_text(_disassembly(code), encoding="utf-8")

    attempts = [
        _attempt_source(
            engine="depyf",
            source_factory=lambda: _decompile_depyf(code),
            module_name=module_name,
            output_dir=output_dir,
        )
    ]
    if pycdc is not None:
        attempts.append(
            _attempt_source(
                engine="pycdc",
                source_factory=lambda: _decompile_pycdc(pycdc, module_name),
                module_name=module_name,
                output_dir=output_dir,
            )
        )

    return {
        "module": module_name,
        "disassembly": disassembly_path.name,
        "attempts": attempts,
        "has_compilable_candidate": any(item["compile_ok"] for item in attempts),
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
    parser.add_argument(
        "--pycdc",
        type=Path,
        default=None,
        help="Optional pinned pycdc executable for an independent Python 3.11 recovery attempt.",
    )
    args = parser.parse_args()

    if sys.version_info[:2] != (3, 11):
        raise SystemExit(
            f"Recovery moet onder Python 3.11 draaien; actief is {sys.version_info.major}.{sys.version_info.minor}."
        )
    if args.pycdc is not None and not args.pycdc.is_file():
        raise SystemExit(f"pycdc executable ontbreekt: {args.pycdc}")

    report = [recover_module(name, args.output_dir, args.pycdc) for name in args.modules]
    report_path = args.output_dir / "recovery-report.json"
    report_path.write_text(
        json.dumps(report, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    for item in report:
        statuses = []
        for attempt in item["attempts"]:
            if attempt["compile_ok"]:
                status = "compile OK"
            elif attempt["decompiled"]:
                status = f"source, compile failed: {attempt.get('error')}"
            else:
                status = f"failed: {attempt.get('error')}"
            statuses.append(f"{attempt['engine']}={status}")
        print(f"{item['module']}: " + "; ".join(statuses))
    print(report_path)
    # Decompiler failure is migration evidence, not a product failure. The
    # deterministic disassembly remains available for manual reconstruction.
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
