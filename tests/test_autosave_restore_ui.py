from __future__ import annotations

import os
from pathlib import Path
import tempfile
import unittest
from unittest import mock

from SleufBase.autosave_restore_ui import list_autosaves, load_autosave


class _PathLoaderApp:
    def __init__(self) -> None:
        self.loaded: Path | None = None
        self._sleufbase_last_project_path: Path | None = None

    def _load_project_from_path(self, path) -> None:
        self.loaded = Path(path)


class _DialogLoaderApp:
    def __init__(self) -> None:
        self.loaded: Path | None = None
        self._sleufbase_last_project_path: Path | None = None

    def load_project(self) -> None:
        from tkinter import filedialog

        selected = filedialog.askopenfilename()
        if selected:
            self.loaded = Path(selected)


class AutosaveRestoreTests(unittest.TestCase):
    def test_list_autosaves_is_newest_first_and_ignores_other_files(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            oldest = directory / "autosave_old.json"
            newest = directory / "autosave_new.json"
            other = directory / "project.json"
            temp = directory / "autosave_hidden.tmp.json"
            for path in (oldest, newest, other, temp):
                path.write_text("{}", encoding="utf-8")
            os.utime(oldest, (1000, 1000))
            os.utime(newest, (2000, 2000))

            entries = list_autosaves(directory)

        self.assertEqual([entry.path.name for entry in entries], ["autosave_new.json", "autosave_old.json"])

    def test_load_autosave_prefers_direct_project_loader(self) -> None:
        app = _PathLoaderApp()
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "autosave_test.json"
            path.write_text("{}", encoding="utf-8")
            load_autosave(app, path)

        self.assertEqual(app.loaded, path)
        self.assertEqual(app._sleufbase_last_project_path, path)

    def test_load_autosave_can_drive_normal_file_dialog_loader(self) -> None:
        app = _DialogLoaderApp()
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "autosave_dialog.json"
            path.write_text("{}", encoding="utf-8")
            with mock.patch("tkinter.filedialog.askopenfilename") as original:
                # load_autosave temporarily replaces this function and restores it afterwards.
                original.return_value = "unused"
                load_autosave(app, path)
                restored = original

        self.assertEqual(app.loaded, path)
        self.assertEqual(app._sleufbase_last_project_path, path)
        self.assertIsNotNone(restored)

    def test_missing_autosave_is_rejected(self) -> None:
        app = _PathLoaderApp()
        with tempfile.TemporaryDirectory() as tmp:
            missing = Path(tmp) / "autosave_missing.json"
            with self.assertRaises(FileNotFoundError):
                load_autosave(app, missing)


if __name__ == "__main__":
    unittest.main()
