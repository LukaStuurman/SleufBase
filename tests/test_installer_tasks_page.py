from pathlib import Path
import re
import unittest


class InstallerTasksPageTests(unittest.TestCase):
    def test_desktop_shortcut_lives_on_ready_page(self) -> None:
        installer = Path("installer/SleufBase.iss").read_text(encoding="utf-8")

        # Er mag geen aparte Tasks/custom input page meer bestaan. Die extra
        # wizardstap liep bij gebruikers interactief vast in v0.3.32/v0.3.33.
        self.assertIsNone(re.search(r"(?m)^\[Tasks\]\s*$", installer))
        self.assertNotIn("CreateInputOptionPage(", installer)
        self.assertNotIn("TInputOptionWizardPage", installer)

        # De checkbox staat op de bestaande Ready-pagina, zodat de ingebouwde
        # Inno Setup Next/Install-flow wordt gebruikt.
        self.assertIn("DisableReadyPage=no", installer)
        self.assertIn("AlwaysShowDirOnReadyPage=yes", installer)
        self.assertIn("TNewCheckBox", installer)
        self.assertIn("WizardForm.ReadyMemo.Parent", installer)
        self.assertIn("Bureaubladsnelkoppeling maken", installer)
        self.assertIn("DesktopShortcutCheckBox.Checked := False;", installer)

        self.assertIn("function ShouldCreateDesktopShortcut(): Boolean;", installer)
        self.assertIn("Check: ShouldCreateDesktopShortcut", installer)


if __name__ == "__main__":
    unittest.main()
