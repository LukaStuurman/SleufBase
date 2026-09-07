from __future__ import annotations

import os
from pathlib import Path
import subprocess
import sys
import unittest
from unittest import mock


REPO_ROOT = Path(__file__).resolve().parents[1]
PACKAGE_PARENT = REPO_ROOT.parent
if str(PACKAGE_PARENT) not in sys.path:
    sys.path.insert(0, str(PACKAGE_PARENT))

from SleufBase import source_migration


class SourceMigrationSelectionTests(unittest.TestCase):
    def test_every_target_defaults_to_legacy(self) -> None:
        state = source_migration.migration_state(environ={})
        self.assertEqual(set(state), set(source_migration.MIGRATABLE_MODULES))
        self.assertTrue(all(value == "legacy" for value in state.values()))

    def test_source_and_legacy_aliases_are_explicit(self) -> None:
        variable = source_migration.migration_environment_variable("streetsmart")
        self.assertEqual(
            source_migration.selected_implementation("streetsmart", {variable: "python"}),
            "source",
        )
        self.assertEqual(
            source_migration.selected_implementation("streetsmart", {variable: "bytecode"}),
            "legacy",
        )

    def test_invalid_selection_is_rejected(self) -> None:
        variable = source_migration.migration_environment_variable("settings")
        with self.assertRaisesRegex(source_migration.SourceMigrationError, variable):
            source_migration.selected_implementation("settings", {variable: "automatic"})

    def test_unknown_module_is_rejected(self) -> None:
        with self.assertRaises(source_migration.SourceMigrationError):
            source_migration.selected_implementation("unknown", {})

    def test_source_without_implementation_fails_closed(self) -> None:
        variable = source_migration.migration_environment_variable("streetsmart")
        with mock.patch.dict(os.environ, {variable: "source"}, clear=False):
            with self.assertRaisesRegex(source_migration.SourceMigrationError, "nog niet beschikbaar"):
                source_migration.load_migrating_module(
                    "streetsmart",
                    {},
                    REPO_ROOT / "streetsmart.py",
                )

    def test_source_loader_is_used_without_legacy_fallback(self) -> None:
        variable = source_migration.migration_environment_variable("streetsmart")
        namespace: dict[str, object] = {}

        def source_loader(target: dict[str, object]) -> None:
            target["migration_probe"] = 42

        with mock.patch.dict(os.environ, {variable: "source"}, clear=False):
            with mock.patch.object(source_migration, "load_legacy_module") as legacy_loader:
                result = source_migration.load_migrating_module(
                    "streetsmart",
                    namespace,
                    REPO_ROOT / "streetsmart.py",
                    source_loader=source_loader,
                )

        self.assertIsNone(result)
        self.assertEqual(namespace["migration_probe"], 42)
        legacy_loader.assert_not_called()


class LegacyCharacterizationContractTests(unittest.TestCase):
    def test_frozen_bytecode_contract_is_valid(self) -> None:
        result = subprocess.run(
            [sys.executable, str(REPO_ROOT / "tools" / "snapshot_legacy_api.py"), "--check"],
            cwd=REPO_ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertIn("Legacy bytecode-contract OK: 5 modules", result.stdout)

    def test_small_legacy_wrappers_use_migration_switch(self) -> None:
        for filename in (
            "settings.py",
            "streetsmart.py",
            "streetsmart_browser.py",
            "streetsmart_panel.py",
        ):
            text = (REPO_ROOT / filename).read_text(encoding="utf-8")
            self.assertIn("load_migrating_module", text, filename)
            self.assertNotIn("load_legacy_module", text, filename)


if __name__ == "__main__":
    unittest.main()
