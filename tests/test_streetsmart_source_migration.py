from __future__ import annotations

import importlib
import inspect
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock


REPO_ROOT = Path(__file__).resolve().parents[1]
PACKAGE_PARENT = REPO_ROOT.parent
if str(PACKAGE_PARENT) not in sys.path:
    sys.path.insert(0, str(PACKAGE_PARENT))

from SleufBase import source_migration, streetsmart_source
from SleufBase.legacy_bytecode import read_validated_code


FUNCTION_NAMES = (
    "_safe_streetsmart_slug",
    "clear_streetsmart_storage",
    "default_streetsmart_state",
    "load_streetsmart_state",
    "save_streetsmart_state",
    "streetsmart_browser_sessions_dir",
    "streetsmart_browser_storage_dir",
    "streetsmart_data_dir",
    "streetsmart_embedded_storage_dir",
    "streetsmart_login_script",
    "streetsmart_selection_center",
    "streetsmart_selection_url",
    "streetsmart_state_path",
)


def _legacy_namespace() -> dict[str, object]:
    namespace: dict[str, object] = {
        "__name__": "SleufBase._legacy_streetsmart_characterization",
        "__file__": str(REPO_ROOT / "streetsmart.py"),
    }
    code = read_validated_code("streetsmart", REPO_ROOT / "legacy_bytecode.py")
    exec(code, namespace)
    return namespace


class StreetSmartSourceParityTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.legacy = _legacy_namespace()

    def test_constants_match_legacy(self) -> None:
        self.assertEqual(
            streetsmart_source.STREETSMART_WEB_URL,
            self.legacy["STREETSMART_WEB_URL"],
        )
        self.assertEqual(
            streetsmart_source.STREETSMART_RD_SRS,
            self.legacy["STREETSMART_RD_SRS"],
        )

    def test_function_signatures_match_legacy(self) -> None:
        for name in FUNCTION_NAMES:
            with self.subTest(name=name):
                self.assertEqual(
                    inspect.signature(getattr(streetsmart_source, name)),
                    inspect.signature(self.legacy[name]),
                )

    def test_slug_and_path_helpers_match_legacy(self) -> None:
        samples = ("", "  ", "User Name", "A.B-C_1", "..!!!..", "Üser@example.com")
        legacy_slug = self.legacy["_safe_streetsmart_slug"]
        for value in samples:
            with self.subTest(value=value):
                self.assertEqual(
                    streetsmart_source._safe_streetsmart_slug(value),
                    legacy_slug(value),
                )

        with tempfile.TemporaryDirectory() as temp_dir:
            with mock.patch.dict(os.environ, {"LOCALAPPDATA": temp_dir}, clear=False):
                self.assertEqual(
                    streetsmart_source.streetsmart_data_dir(),
                    self.legacy["streetsmart_data_dir"](),
                )
                for username in ("User Name", "A.B-C_1"):
                    self.assertEqual(
                        streetsmart_source.streetsmart_browser_storage_dir(username),
                        self.legacy["streetsmart_browser_storage_dir"](username),
                    )
                self.assertEqual(
                    streetsmart_source.streetsmart_embedded_storage_dir(),
                    self.legacy["streetsmart_embedded_storage_dir"](),
                )
                self.assertEqual(
                    streetsmart_source.streetsmart_state_path(),
                    self.legacy["streetsmart_state_path"](),
                )

    def test_selection_helpers_match_legacy(self) -> None:
        cases = (
            None,
            {},
            {"center": None},
            {"center": []},
            {"center": [1]},
            {"center": [1, 2]},
            {"center": ["123.456", "456.789"]},
            {"center": [float("nan"), 2]},
            {"center": [float("inf"), -float("inf")]},
            {"center": [object(), 2]},
        )
        legacy_center = self.legacy["streetsmart_selection_center"]
        legacy_url = self.legacy["streetsmart_selection_url"]
        for selection in cases:
            with self.subTest(selection=selection):
                self.assertEqual(
                    streetsmart_source.streetsmart_selection_center(selection),
                    legacy_center(selection),
                )
                self.assertEqual(
                    streetsmart_source.streetsmart_selection_url(selection),
                    legacy_url(selection),
                )

    def test_login_script_matches_legacy_byte_for_byte(self) -> None:
        legacy_script = self.legacy["streetsmart_login_script"]
        credentials = (
            ("user@example.com", "simple"),
            ('u"ser\\name', 'p"ass\\word'),
            ("gebruiker-é", "wachtwoord-€"),
            ("", ""),
        )
        for username, password in credentials:
            with self.subTest(username=username):
                self.assertEqual(
                    streetsmart_source.streetsmart_login_script(username, password),
                    legacy_script(username, password),
                )

    def test_load_state_matches_legacy_for_valid_and_invalid_payloads(self) -> None:
        payloads = (
            None,
            "not-json",
            json.dumps([]),
            json.dumps({}),
            json.dumps({"version": "12", "selection": {"center": [1, 2]}}),
            json.dumps({"version": "invalid", "selection": []}),
            json.dumps({"version": None, "selection": None}),
        )
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            for index, payload in enumerate(payloads):
                state_path = root / f"state-{index}.json"
                if payload is not None:
                    state_path.write_text(payload, encoding="utf-8")

                old_legacy_path = self.legacy["streetsmart_state_path"]
                self.legacy["streetsmart_state_path"] = lambda p=state_path: p
                try:
                    legacy_result = self.legacy["load_streetsmart_state"]()
                finally:
                    self.legacy["streetsmart_state_path"] = old_legacy_path

                with mock.patch.object(
                    streetsmart_source,
                    "streetsmart_state_path",
                    return_value=state_path,
                ):
                    source_result = streetsmart_source.load_streetsmart_state()

                with self.subTest(payload=payload):
                    self.assertEqual(source_result, legacy_result)

    def test_save_state_matches_legacy_serialization(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            legacy_path = root / "legacy.json"
            source_path = root / "source.json"
            selection = {"center": [123.4, 567.8], "label": "é"}

            old_legacy_path = self.legacy["streetsmart_state_path"]
            self.legacy["streetsmart_state_path"] = lambda: legacy_path
            try:
                with mock.patch.object(streetsmart_source.time, "time_ns", return_value=123456789):
                    self.legacy["save_streetsmart_state"](selection)
            finally:
                self.legacy["streetsmart_state_path"] = old_legacy_path

            with mock.patch.object(
                streetsmart_source,
                "streetsmart_state_path",
                return_value=source_path,
            ):
                with mock.patch.object(streetsmart_source.time, "time_ns", return_value=123456789):
                    streetsmart_source.save_streetsmart_state(selection)

            self.assertEqual(source_path.read_bytes(), legacy_path.read_bytes())

    def test_clear_storage_matches_legacy(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            legacy_root = root / "legacy"
            source_root = root / "source"
            for base in (legacy_root, source_root):
                (base / "embedded_webview2" / "nested").mkdir(parents=True)
                (base / "browser_sessions" / "user_name" / "nested").mkdir(parents=True)
                (base / "embedded_webview2" / "nested" / "x.txt").write_text("x", encoding="utf-8")
                (base / "browser_sessions" / "user_name" / "nested" / "x.txt").write_text("x", encoding="utf-8")

            old_legacy_data_dir = self.legacy["streetsmart_data_dir"]
            self.legacy["streetsmart_data_dir"] = lambda: legacy_root
            try:
                legacy_removed = self.legacy["clear_streetsmart_storage"]("User Name")
            finally:
                self.legacy["streetsmart_data_dir"] = old_legacy_data_dir

            with mock.patch.object(
                streetsmart_source,
                "streetsmart_data_dir",
                return_value=source_root,
            ):
                source_removed = streetsmart_source.clear_streetsmart_storage("User Name")

            self.assertEqual(
                [path.relative_to(source_root) for path in source_removed],
                [path.relative_to(legacy_root) for path in legacy_removed],
            )
            self.assertFalse((legacy_root / "embedded_webview2").exists())
            self.assertFalse((source_root / "embedded_webview2").exists())
            self.assertFalse((legacy_root / "browser_sessions" / "user_name").exists())
            self.assertFalse((source_root / "browser_sessions" / "user_name").exists())


class StreetSmartWrapperSourceModeTests(unittest.TestCase):
    def test_source_is_default_for_streetsmart_only(self) -> None:
        self.assertEqual(
            source_migration.selected_implementation("streetsmart", environ={}),
            "source",
        )
        self.assertEqual(
            source_migration.selected_implementation("settings", environ={}),
            "legacy",
        )

    def test_wrapper_source_mode_does_not_call_legacy_loader(self) -> None:
        variable = source_migration.migration_environment_variable("streetsmart")
        previous_module = sys.modules.pop("SleufBase.streetsmart", None)
        try:
            with mock.patch.dict(os.environ, {variable: "source"}, clear=False):
                with mock.patch.object(
                    source_migration,
                    "load_legacy_module",
                    side_effect=AssertionError("legacy loader mag niet worden aangeroepen"),
                ):
                    module = importlib.import_module("SleufBase.streetsmart")
            self.assertEqual(module.STREETSMART_WEB_URL, streetsmart_source.STREETSMART_WEB_URL)
            self.assertIs(module.save_streetsmart_state, streetsmart_source.save_streetsmart_state)
        finally:
            sys.modules.pop("SleufBase.streetsmart", None)
            if previous_module is not None:
                sys.modules["SleufBase.streetsmart"] = previous_module

    def test_legacy_mode_remains_available_as_rollback(self) -> None:
        variable = source_migration.migration_environment_variable("streetsmart")
        namespace: dict[str, object] = {}
        with mock.patch.dict(os.environ, {variable: "legacy"}, clear=False):
            path = source_migration.load_migrating_module(
                "streetsmart",
                namespace,
                REPO_ROOT / "streetsmart.py",
                source_loader=lambda target: target.update(source_should_not_run=True),
            )
        self.assertIsNotNone(path)
        self.assertIn("STREETSMART_WEB_URL", namespace)
        self.assertNotIn("source_should_not_run", namespace)


if __name__ == "__main__":
    unittest.main()
