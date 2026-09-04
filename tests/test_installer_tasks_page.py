from pathlib import Path
import re
import unittest


class InstallerTasksPageTests(unittest.TestCase):
    def test_desktop_shortcut_uses_standard_inno_tasks_page(self) -> None:
        installer = Path("installer/SleufBase.iss").read_text(encoding="utf-8")

        # v0.3.35 deliberately restored the last-known-good standard Inno Setup
        # wizard structure from v0.3.31. Keep the desktop shortcut as a normal
        # built-in Tasks checkbox rather than introducing custom wizard controls.
        self.assertIsNotNone(re.search(r"(?m)^\[Tasks\]\s*$", installer))
        self.assertRegex(
            installer,
            r'Name:\s*"desktopicon";\s*Description:\s*"Bureaubladsnelkoppeling maken"',
        )
        self.assertRegex(
            installer,
            r'Name:\s*"\{autodesktop\}\\SleufBase";[^\n]*Tasks:\s*desktopicon',
        )

        # Any custom [Code] overlay/input page can intercept or obscure the native
        # wizard controls, which was the failure mode of the intermediate fixes.
        self.assertIsNone(re.search(r"(?m)^\[Code\]\s*$", installer))
        for forbidden in (
            "CreateInputOptionPage(",
            "TInputOptionWizardPage",
            "TNewCheckBox",
            "InitializeWizard",
            "WizardForm",
        ):
            self.assertNotIn(forbidden, installer)


if __name__ == "__main__":
    unittest.main()
