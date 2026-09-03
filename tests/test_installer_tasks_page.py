from pathlib import Path
import re
import unittest


class InstallerTasksPageTests(unittest.TestCase):
    def test_desktop_shortcut_uses_explicit_custom_page(self) -> None:
        installer = Path("installer/SleufBase.iss").read_text(encoding="utf-8")

        self.assertIsNone(re.search(r"(?m)^\[Tasks\]\s*$", installer))
        self.assertIn("CreateInputOptionPage(", installer)
        self.assertIn("Bureaubladsnelkoppeling maken", installer)
        self.assertIn("function ShouldCreateDesktopShortcut(): Boolean;", installer)
        self.assertIn("Check: ShouldCreateDesktopShortcut", installer)

        # De standaard Select Tasks-pagina veroorzaakte in v0.3.32 een lege
        # wizardpagina. De checkbox moet daarom door onze eigen wizardpagina
        # worden gemaakt en rechtstreeks aan het desktopicoon gekoppeld zijn.
        self.assertIn("ExtraTasksPage.Values[0] := False;", installer)


if __name__ == "__main__":
    unittest.main()
