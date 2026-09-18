from __future__ import annotations

import tkinter as tk

from . import settings_ui


PATCH_VERSION = 1
_INSTALLED = False
_ORIGINAL_CONFIGURE_DIALOG_GEOMETRY = settings_ui._configure_dialog_geometry


def _configure_maximized_dialog_geometry(dialog: tk.Misc) -> None:
    """Open the settings dialog maximized, with a full-screen geometry fallback."""

    if bool(getattr(dialog, "_settings_geometry_done", False)):
        return

    try:
        dialog.update_idletasks()
        screen_width = max(int(dialog.winfo_screenwidth()), 1)
        screen_height = max(int(dialog.winfo_screenheight()), 1)
        dialog.resizable(True, True)
        dialog.minsize(min(860, screen_width), min(620, screen_height))
    except (AttributeError, TypeError, ValueError, tk.TclError):
        screen_width = 0
        screen_height = 0

    maximized = False
    try:
        # Native Windows/Tk behaviour: fills the usable desktop while keeping
        # normal window controls and the taskbar available.
        dialog.state("zoomed")
        maximized = True
    except (AttributeError, tk.TclError):
        pass

    if not maximized:
        try:
            # Supported by some Tk window managers where state("zoomed") is not.
            dialog.attributes("-zoomed", True)
            maximized = True
        except (AttributeError, tk.TclError):
            pass

    if not maximized and screen_width > 0 and screen_height > 0:
        try:
            # Last-resort fallback for platforms/window managers without a
            # maximize state. This still makes the settings window fill the screen.
            dialog.geometry(f"{screen_width}x{screen_height}+0+0")
        except (AttributeError, tk.TclError):
            pass

    try:
        dialog._settings_geometry_done = True
    except Exception:
        pass


def install_settings_maximized_window_patch() -> None:
    global _INSTALLED
    if _INSTALLED:
        return

    settings_ui._configure_dialog_geometry = _configure_maximized_dialog_geometry
    _INSTALLED = True
