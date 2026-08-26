from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def _require(pattern: str, text: str, label: str) -> str:
    match = re.search(pattern, text, re.MULTILINE)
    if not match:
        raise SystemExit(f"Release-integriteitscontrole: {label} kon niet worden gelezen")
    return match.group(1)


def main() -> int:
    version_text = (ROOT / "version.py").read_text(encoding="utf-8")
    product_version = _require(r'^__version__\s*=\s*"([^"]+)"', version_text, "version.py")
    numeric4 = f"{product_version}.0"

    win = (ROOT / "windows_version_info.txt").read_text(encoding="utf-8")
    installer = (ROOT / "installer" / "SleufBase.iss").read_text(encoding="utf-8")
    workflow = (ROOT / ".github" / "workflows" / "build-windows-exe.yml").read_text(encoding="utf-8")

    checks = {
        "Windows ProductVersion": _require(r"StringStruct\('ProductVersion', '([^']+)'\)", win, "Windows ProductVersion"),
        "Windows FileVersion": _require(r"StringStruct\('FileVersion', '([^']+)'\)", win, "Windows FileVersion"),
        "Installer MyAppVersion": _require(r'^#define MyAppVersion "([^"]+)"', installer, "installer version"),
        "Installer VersionInfoVersion": _require(r'^VersionInfoVersion=([^\r\n]+)', installer, "installer file version"),
        "Workflow release version": _require(r"SLEUFBASE_RELEASE_VERSION:\s*'([^']+)'", workflow, "workflow release version"),
    }
    expected = {
        "Windows ProductVersion": product_version,
        "Windows FileVersion": numeric4,
        "Installer MyAppVersion": product_version,
        "Installer VersionInfoVersion": numeric4,
        "Workflow release version": product_version,
    }
    mismatches = [f"{name}: {value!r} != {expected[name]!r}" for name, value in checks.items() if value != expected[name]]
    if mismatches:
        raise SystemExit("Releaseversies zijn niet consistent:\n- " + "\n- ".join(mismatches))

    print(f"Release-integriteit OK: SleufBase {product_version}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
