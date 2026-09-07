from __future__ import annotations

import os
import sys
from pathlib import Path
from types import ModuleType
import unittest
from unittest import mock

REPO_ROOT = Path(__file__).resolve().parents[1]
PACKAGE_PARENT = REPO_ROOT.parent
if str(PACKAGE_PARENT) not in sys.path:
    sys.path.insert(0, str(PACKAGE_PARENT))

from SleufBase import source_migration


class SourceMigrationSelectionTests(unittest.TestCase):
    def test_source_mode_is_disabled_by_default(self) -> None:
        with mock.patch.dict(os.environ, {}, clear=True):
            self.assertEqual(source_migration.requested_source_modules(), frozenset())
            self.assertFalse(source_migration.source_enabled("streetsmart"))

    def test_all_selects_every_migratable_module(self) -> None:
        self.assertEqual(
            source_migration.requested_source_modules("all"),
            source_migration.MIGRATABLE_MODULES,
        )
        self.assertEqual(
            source_migration.requested_source_modules("*"),
            source_migration.MIGRATABLE_MODULES,
        )

    def test_unknown_module_is_rejected(self) -> None:
        with self.assertRaises(source_migration.SourceMigrationError):
            source_migration.requested_source_modules("streetsmart,typo")

    def test_all_cannot_be_combined_with_individual_names(self) -> None:
        with self.assertRaises(source_migration.SourceMigrationError):
            source_migration.requested_source_modules("all,settings")


class SourceMigrationLoaderTests(unittest.TestCase):
    def test_legacy_is_the_default_and_uses_validated_loader(self) -> None:
        namespace: dict[str, object] = {}
        with mock.patch.dict(os.environ, {}, clear=True), mock.patch.object(
            source_migration, "load_legacy_module"
        ) as legacy_loader:
            selected = source_migration.load_source_or_legacy(
                "streetsmart", namespace, __file__
            )

        self.assertEqual(selected, "legacy-bytecode")
        self.assertEqual(namespace["__sleufbase_core_implementation__"], "legacy-bytecode")
        legacy_loader.assert_called_once_with("streetsmart", namespace, __file__)

    def test_source_mode_copies_source_namespace_without_legacy_fallback(self) -> None:
        fake_name = "SleufBase._source.streetsmart"
        fake_module = ModuleType(fake_name)
        fake_module.answer = 42
        fake_module._private_contract_value = "preserved"
        namespace: dict[str, object] = {"__name__": "SleufBase.streetsmart"}

        with mock.patch.dict(
            os.environ,
            {source_migration.SOURCE_MODULES_ENV: "streetsmart"},
            clear=True,
        ), mock.patch.dict(sys.modules, {fake_name: fake_module}), mock.patch.object(
            source_migration, "load_legacy_module"
        ) as legacy_loader:
            selected = source_migration.load_source_or_legacy(
                "streetsmart", namespace, __file__
            )

        self.assertEqual(selected, "source")
        self.assertEqual(namespace["answer"], 42)
        self.assertEqual(namespace["_private_contract_value"], "preserved")
        self.assertEqual(namespace["__name__"], "SleufBase.streetsmart")
        self.assertEqual(namespace["__sleufbase_core_implementation__"], "source")
        legacy_loader.assert_not_called()

    def test_missing_requested_source_never_falls_back_to_bytecode(self) -> None:
        target = source_migration.source_module_name("streetsmart")
        real_import = source_migration.importlib.import_module

        def missing_source(name: str):
            if name == target:
                error = ModuleNotFoundError(f"No module named {name!r}")
                error.name = name
                raise error
            return real_import(name)

        with mock.patch.dict(
            os.environ,
            {source_migration.SOURCE_MODULES_ENV: "streetsmart"},
            clear=True,
        ), mock.patch.object(
            source_migration.importlib, "import_module", side_effect=missing_source
        ), mock.patch.object(source_migration, "load_legacy_module") as legacy_loader:
            with self.assertRaises(source_migration.SourceMigrationError):
                source_migration.load_source_or_legacy("streetsmart", {}, __file__)

        legacy_loader.assert_not_called()


if __name__ == "__main__":
    unittest.main()
