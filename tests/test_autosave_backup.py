from __future__ import annotations

import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest import mock

from SleufBase.autosave_backup_patch import (
    AutosaveManager,
    AutosaveSettings,
    create_backup,
    load_autosave_settings,
    prune_backups,
    save_autosave_settings,
)


class _FakeApp:
    def __init__(self) -> None:
        self.current_project_path = Path("C:/work/current-project.json")
        self._sleufbase_last_project_path = self.current_project_path
        self._dirty = True
        self._title = "SleufBase - current-project.json *"
        self.after_calls: list[tuple[int, object]] = []
        self.cancelled: list[object] = []
        self._next_after_id = 0

    def title(self, value=None):
        if value is None:
            return self._title
        self._title = str(value)

    def _save_project_to_path(self, path) -> None:
        path = Path(path)
        path.write_text(
            json.dumps({"saved": True, "path": str(path)}),
            encoding="utf-8",
        )
        # Deliberately emulate a serializer that changes active-project UI state.
        self.current_project_path = path
        self._sleufbase_last_project_path = path
        self._dirty = False
        self.title(f"SleufBase - {path.name}")

    def after(self, delay_ms, callback):
        self._next_after_id += 1
        after_id = f"after-{self._next_after_id}"
        self.after_calls.append((delay_ms, callback))
        return after_id

    def after_cancel(self, after_id):
        self.cancelled.append(after_id)


class AutosaveSettingsTests(unittest.TestCase):
    def test_defaults_are_enabled_ten_minutes_and_twenty_backups(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "missing.json"
            settings = load_autosave_settings(path)
        self.assertEqual(
            settings,
            AutosaveSettings(enabled=True, interval_minutes=10, max_backups=20),
        )

    def test_corrupt_settings_fall_back_to_defaults(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "settings.json"
            path.write_text("{broken", encoding="utf-8")
            settings = load_autosave_settings(path)
        self.assertEqual(settings.interval_minutes, 10)
        self.assertEqual(settings.max_backups, 20)
        self.assertTrue(settings.enabled)

    def test_settings_are_clamped_and_written_atomically(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "settings.json"
            stored = save_autosave_settings(
                AutosaveSettings(enabled=False, interval_minutes=0, max_backups=9999),
                path,
            )
            payload = json.loads(path.read_text(encoding="utf-8"))
        self.assertFalse(stored.enabled)
        self.assertEqual(stored.interval_minutes, 1)
        self.assertEqual(stored.max_backups, 200)
        self.assertEqual(payload["interval_minutes"], 1)
        self.assertEqual(payload["max_backups"], 200)


class AutosaveBackupTests(unittest.TestCase):
    def test_backup_has_random_name_and_restores_active_project_state(self) -> None:
        app = _FakeApp()
        original_path = app.current_project_path
        original_title = app.title()
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            first = create_backup(app, directory=directory, max_backups=20)
            second = create_backup(app, directory=directory, max_backups=20)

            self.assertTrue(first.is_file())
            self.assertTrue(second.is_file())
            self.assertNotEqual(first.name, second.name)
            self.assertTrue(first.name.startswith("autosave_"))
            self.assertEqual(first.suffix, ".json")
            self.assertFalse(any(".tmp" in path.name for path in directory.iterdir()))

        self.assertEqual(app.current_project_path, original_path)
        self.assertEqual(app._sleufbase_last_project_path, original_path)
        self.assertTrue(app._dirty)
        self.assertEqual(app.title(), original_title)
        self.assertFalse(app._sleufbase_autosave_in_progress)

    def test_rotation_keeps_newest_twenty(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            files: list[Path] = []
            for index in range(25):
                path = directory / f"autosave_{index:02d}.json"
                path.write_text("{}", encoding="utf-8")
                timestamp = 1_700_000_000 + index
                os.utime(path, (timestamp, timestamp))
                files.append(path)

            removed = prune_backups(20, directory)
            remaining = sorted(path.name for path in directory.iterdir())

        self.assertEqual([path.name for path in removed], [f"autosave_{i:02d}.json" for i in range(5)])
        self.assertEqual(len(remaining), 20)
        self.assertEqual(remaining[0], "autosave_05.json")
        self.assertEqual(remaining[-1], "autosave_24.json")

    def test_failed_serializer_cleans_temp_and_restores_state(self) -> None:
        app = _FakeApp()
        original_path = app.current_project_path

        def failing_serializer(path) -> None:
            path = Path(path)
            path.write_text("partial", encoding="utf-8")
            app.current_project_path = path
            raise RuntimeError("boom")

        app._save_project_to_path = failing_serializer
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            with self.assertRaisesRegex(RuntimeError, "boom"):
                create_backup(app, directory=directory, max_backups=20)
            self.assertEqual(list(directory.iterdir()), [])

        self.assertEqual(app.current_project_path, original_path)
        self.assertFalse(app._sleufbase_autosave_in_progress)


class AutosaveSchedulerTests(unittest.TestCase):
    def test_scheduler_uses_configured_interval_and_reschedules(self) -> None:
        app = _FakeApp()
        with mock.patch(
            "SleufBase.autosave_backup_patch.load_autosave_settings",
            return_value=AutosaveSettings(True, 10, 20),
        ):
            manager = AutosaveManager(app)

        manager.reschedule()
        self.assertEqual(app.after_calls[-1][0], 10 * 60 * 1000)
        first_id = manager._after_id

        with mock.patch(
            "SleufBase.autosave_backup_patch.save_autosave_settings",
            return_value=AutosaveSettings(True, 7, 12),
        ), mock.patch("SleufBase.autosave_backup_patch.prune_backups"):
            manager.apply_settings(AutosaveSettings(True, 7, 12))

        self.assertIn(first_id, app.cancelled)
        self.assertEqual(app.after_calls[-1][0], 7 * 60 * 1000)
        self.assertEqual(manager.settings.max_backups, 12)

    def test_disabled_setting_schedules_nothing(self) -> None:
        app = _FakeApp()
        with mock.patch(
            "SleufBase.autosave_backup_patch.load_autosave_settings",
            return_value=AutosaveSettings(False, 10, 20),
        ):
            manager = AutosaveManager(app)
        manager.reschedule()
        self.assertEqual(app.after_calls, [])


if __name__ == "__main__":
    unittest.main()
