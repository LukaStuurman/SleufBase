from __future__ import annotations

"""Compare one reviewed source core module with the recorded legacy contract."""

import argparse
import difflib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[1]
CORE_MODULES = (
    "streetsmart",
    "streetsmart_browser",
    "streetsmart_panel",
    "settings",
    "app",
)


def _normalize(value: Any, module_name: str) -> Any:
    source_module = f"SleufBase._source.{module_name}"
    public_module = f"SleufBase.{module_name}"
    if isinstance(value, str):
        return value.replace(source_module, public_module)
    if isinstance(value, list):
        return [_normalize(item, module_name) for item in value]
    if isinstance(value, dict):
        return {
            key: _normalize(item, module_name)
            for key, item in value.items()
            if key != "implementation"
        }
    return value


def _source_snapshot(module_name: str) -> dict[str, Any]:
    with tempfile.TemporaryDirectory(prefix="sleufbase-contract-") as directory:
        output = Path(directory) / "source.json"
        process = subprocess.run(
            [
                sys.executable,
                str(REPO_ROOT / "tools" / "snapshot_legacy_api.py"),
                "--implementation",
                "source",
                "--modules",
                module_name,
                "--output",
                str(output),
            ],
            cwd=REPO_ROOT,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
            timeout=120,
        )
        if process.returncode != 0:
            detail = process.stderr.strip() or process.stdout.strip()
            raise RuntimeError(f"source contract snapshot faalde: {detail}")
        return json.loads(output.read_text(encoding="utf-8"))


def compare_contract(module_name: str, baseline_path: Path) -> tuple[bool, str]:
    baseline = json.loads(baseline_path.read_text(encoding="utf-8"))
    if module_name not in baseline.get("modules", {}):
        raise RuntimeError(f"legacy contract bevat module {module_name!r} niet")

    source = _source_snapshot(module_name)
    expected = _normalize(baseline["modules"][module_name], module_name)
    actual = _normalize(source["modules"][module_name], module_name)
    if expected == actual:
        return True, ""

    expected_text = json.dumps(expected, ensure_ascii=False, indent=2, sort_keys=True).splitlines()
    actual_text = json.dumps(actual, ensure_ascii=False, indent=2, sort_keys=True).splitlines()
    diff = "\n".join(
        difflib.unified_diff(
            expected_text,
            actual_text,
            fromfile=f"legacy/{module_name}",
            tofile=f"source/{module_name}",
            lineterm="",
        )
    )
    return False, diff


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("module", choices=CORE_MODULES)
    parser.add_argument(
        "--baseline",
        type=Path,
        default=REPO_ROOT / "migration" / "contracts" / "legacy_api.json",
    )
    parser.add_argument("--diff-output", type=Path, default=None)
    args = parser.parse_args()

    ok, diff = compare_contract(args.module, args.baseline)
    if ok:
        print(f"Contract gelijk: {args.module}")
        return 0

    if args.diff_output is not None:
        args.diff_output.parent.mkdir(parents=True, exist_ok=True)
        args.diff_output.write_text(diff + "\n", encoding="utf-8")
        print(f"Contractverschil geschreven naar {args.diff_output}")
    else:
        print(diff)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
