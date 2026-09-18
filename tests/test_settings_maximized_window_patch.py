from __future__ import annotations

import tkinter as tk
import unittest

from SleufBase import settings_maximized_window_patch as patch


class _FakeDialog:
    def __init__(self, *, state_works: bool = True, attributes_works: bool = True) -> None:
        self._settings_geometry_done = False
        self.state_works = state_works
        self.attributes_works = attributes_works
        self.state_calls: list[str] = []
        self.attribute_calls: list[tuple[str, bool]] = []
        self.geometry_calls: list[str] = []
        self.resizable_calls: list[tuple[bool, bool]] = []
        self.minsize_calls: list[tuple[int, int]] = []

    def update_idletasks(self) -> None:
        pass

    def winfo_screenwidth(self) -> int:
        return 1920

    def winfo_screenheight(self) -> int:
        return 1080

    def resizable(self, width: bool, height: bool) -> None:
        self.resizable_calls.append((width, height))

    def minsize(self, width: int, height: int) -> None:
        self.minsize_calls.append((width, height))

    def state(self, value: str) -> None:
        self.state_calls.append(value)
        if not self.state_works:
            raise tk.TclError("zoomed state unavailable")

    def attributes(self, name: str, value: bool) -> None:
        self.attribute_calls.append((name, value))
        if not self.attributes_works:
            raise tk.TclError("zoomed attribute unavailable")

    def geometry(self, value: str) -> None:
        self.geometry_calls.append(value)


class SettingsMaximizedWindowPatchTests(unittest.TestCase):
    def test_settings_window_uses_native_zoomed_state(self) -> None:
        dialog = _FakeDialog()

        patch._configure_maximized_dialog_geometry(dialog)

        self.assertEqual(dialog.state_calls, ["zoomed"])
        self.assertEqual(dialog.attribute_calls, [])
        self.assertEqual(dialog.geometry_calls, [])
        self.assertEqual(dialog.resizable_calls, [(True, True)])
        self.assertEqual(dialog.minsize_calls, [(860, 620)])
        self.assertTrue(dialog._settings_geometry_done)

    def test_full_screen_geometry_is_used_when_maximize_is_unavailable(self) -> None:
        dialog = _FakeDialog(state_works=False, attributes_works=False)

        patch._configure_maximized_dialog_geometry(dialog)

        self.assertEqual(dialog.state_calls, ["zoomed"])
        self.assertEqual(dialog.attribute_calls, [("-zoomed", True)])
        self.assertEqual(dialog.geometry_calls, ["1920x1080+0+0"])
        self.assertTrue(dialog._settings_geometry_done)

    def test_geometry_is_only_configured_once(self) -> None:
        dialog = _FakeDialog()
        dialog._settings_geometry_done = True

        patch._configure_maximized_dialog_geometry(dialog)

        self.assertEqual(dialog.state_calls, [])
        self.assertEqual(dialog.geometry_calls, [])


if __name__ == "__main__":
    unittest.main()
