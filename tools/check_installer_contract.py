from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
INSTALLER = ROOT / "installer" / "SleufBase.iss"


def fail(message: str) -> None:
    raise SystemExit(f"Installer-contract faalt: {message}")


def main() -> int:
    text = INSTALLER.read_text(encoding="utf-8")

    # Keep the installer deliberately boring: native/classic Inno Setup controls only.
    required_patterns = {
        "classic wizard": r"(?m)^WizardStyle=classic\s*$",
        "non-resizable wizard": r"(?m)^WizardResizable=no\s*$",
        "standard tasks section": r"(?m)^\[Tasks\]\s*$",
        "desktop task": r'(?m)^Name:\s*"desktopicon";\s*Description:\s*"Bureaubladsnelkoppeling maken";.*Flags:\s*unchecked\s*$',
        "desktop shortcut bound to task": r'(?m)^Name:\s*"\{autodesktop\}\\SleufBase";.*Tasks:\s*desktopicon\s*$',
    }
    for label, pattern in required_patterns.items():
        if not re.search(pattern, text):
            fail(f"vereiste ontbreekt: {label}")

    forbidden_patterns = {
        "custom [Code] section": r"(?m)^\[Code\]\s*$",
        "custom wizard initialization": r"\bInitializeWizard\b",
        "direct WizardForm manipulation": r"\bWizardForm\b",
        "custom checkbox": r"\bTNewCheckBox\b",
        "custom option page": r"\bCreateInputOptionPage\b",
        "custom wizard page": r"\bCreateCustomPage\b",
        "custom wizard style file": r"(?m)^WizardStyleFile=",
        "custom wizard background": r"(?m)^WizardBack(?:Color|ImageFile)=",
    }
    for label, pattern in forbidden_patterns.items():
        if re.search(pattern, text):
            fail(f"verboden installerlogica aangetroffen: {label}")

    # Exactly one desktop task and one task-bound desktop shortcut avoids shadow controls.
    if text.count('Name: "desktopicon";') != 1:
        fail("desktopicon-task moet exact één keer bestaan")
    if text.count('Name: "{autodesktop}\\SleufBase";') != 1:
        fail("desktopshortcut moet exact één keer bestaan")

    print("Installer-contract OK: classic native wizard, standaard desktop-task, geen custom controls")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
