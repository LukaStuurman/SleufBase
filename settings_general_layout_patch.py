from __future__ import annotations

import tkinter as tk
from tkinter import ttk

from . import settings_ui


PATCH_VERSION = 1
_GENERAL_TITLE = "Algemeen"
_WORDS_BUTTON_TEXT = "Woordenlijst beheren…"


def _find_general_panel(dialog: tk.Misc) -> tk.Misc | None:
    for widget in settings_ui._descendants(dialog):
        if not settings_ui._is_label_frame(widget):
            continue
        original = str(getattr(widget, "_settings_original_section_title", "") or "")
        current = settings_ui._widget_text(widget)
        if original == _GENERAL_TITLE or current == _GENERAL_TITLE:
            return widget
    return None


def _grid_extent(parent: tk.Misc, *extra_widgets: tk.Misc) -> int:
    total = max(1, settings_ui._occupied_grid_columns(parent))
    for widget in extra_widgets:
        try:
            if widget.master is not parent or widget.winfo_manager() != "grid":
                continue
            info = widget.grid_info()
            column = int(info.get("column", 0))
            span = max(1, int(info.get("columnspan", 1)))
            total = max(total, column + span)
        except (AttributeError, TypeError, ValueError, tk.TclError):
            pass
    return total


def _make_general_full_width(general_panel: tk.Misc, words_panel: tk.Misc) -> int:
    parent = general_panel.master
    if words_panel.master is not parent:
        return 1

    total_columns = _grid_extent(parent, general_panel, words_panel)
    try:
        if general_panel.winfo_manager() == "grid":
            info = general_panel.grid_info()
            general_panel.grid_configure(
                column=0,
                columnspan=total_columns,
                sticky="nsew",
                padx=info.get("padx", 0),
                pady=info.get("pady", 0),
            )
        for column in range(total_columns):
            parent.grid_columnconfigure(column, weight=1, minsize=0)
    except (TypeError, ValueError, tk.TclError):
        pass
    return total_columns


def _next_grid_row(parent: tk.Misc) -> int:
    maximum = -1
    try:
        for child in parent.grid_slaves():
            if child is getattr(parent, "_settings_words_launcher", None):
                continue
            info = child.grid_info()
            row = int(info.get("row", 0))
            span = max(1, int(info.get("rowspan", 1)))
            maximum = max(maximum, row + span - 1)
    except (TypeError, ValueError, tk.TclError):
        pass
    return maximum + 1


def _place_launcher_in_general(
    dialog: tk.Misc,
    general_panel: tk.Misc,
    words_panel: tk.Misc,
) -> tk.Misc:
    launcher = ttk.Frame(
        general_panel,
        style="Settings.Launcher.TFrame",
        padding=(0, 8, 0, 0),
    )
    launcher.columnconfigure(0, weight=1)

    ttk.Label(
        launcher,
        text="KickTheMap woordenlijst",
        style="Settings.Subtitle.TLabel",
    ).grid(row=0, column=0, sticky="w")
    ttk.Button(
        launcher,
        text=_WORDS_BUTTON_TEXT,
        command=lambda target=dialog, rules=words_panel: settings_ui._open_words_view(target, rules),
        padding=(12, 7),
    ).grid(row=0, column=1, sticky="e", padx=(12, 0))

    child_managers = {
        child.winfo_manager()
        for child in general_panel.winfo_children()
        if child is not launcher and child.winfo_manager()
    }
    if child_managers == {"pack"}:
        launcher.pack(side="bottom", fill="x", pady=(8, 0))
    else:
        row = _next_grid_row(general_panel)
        internal_columns = max(1, settings_ui._occupied_grid_columns(general_panel))
        try:
            for column in range(internal_columns):
                general_panel.grid_columnconfigure(column, weight=1, minsize=0)
        except tk.TclError:
            pass
        launcher.grid(
            row=row,
            column=0,
            columnspan=internal_columns,
            sticky="ew",
            pady=(8, 0),
        )

    try:
        dialog._settings_words_launcher = launcher
    except Exception:
        pass
    return launcher


def _install_words_launcher_in_general(dialog: tk.Misc, panel: tk.Misc) -> None:
    existing = getattr(dialog, "_settings_words_launcher", None)
    if existing is not None:
        try:
            if bool(existing.winfo_exists()):
                return
        except (AttributeError, tk.TclError):
            pass

    try:
        if panel.winfo_manager() != "grid":
            return
        panel_info = settings_ui._grid_info_copy(panel)
        if not panel_info:
            return
        panel._settings_words_original_grid = dict(panel_info)
    except (AttributeError, tk.TclError):
        return

    general_panel = _find_general_panel(dialog)
    if general_panel is None:
        # Keep the proven existing layout as a safe fallback for unexpected settings dialogs.
        _ORIGINAL_INSTALL_WORDS_LAUNCHER(dialog, panel)
        return

    _make_general_full_width(general_panel, panel)
    try:
        panel.grid_remove()
    except tk.TclError:
        return

    _place_launcher_in_general(dialog, general_panel, panel)


def _apply_full_width_general_layout(dialog: tk.Misc) -> None:
    general_panel = _find_general_panel(dialog)
    words_panel = next(
        (
            panel
            for panel in settings_ui._descendants(dialog)
            if settings_ui._is_label_frame(panel) and settings_ui._is_words_panel(panel)
        ),
        None,
    )
    if general_panel is None or words_panel is None:
        return
    _make_general_full_width(general_panel, words_panel)


_ORIGINAL_INSTALL_WORDS_LAUNCHER = settings_ui._install_words_launcher
_ORIGINAL_APPLY_SETTINGS_UI = settings_ui._apply_settings_ui
_INSTALLED = False


def install_settings_general_layout_patch() -> None:
    global _INSTALLED
    if _INSTALLED:
        return

    settings_ui._install_words_launcher = _install_words_launcher_in_general

    def apply_settings_ui(dialog: tk.Misc) -> None:
        _ORIGINAL_APPLY_SETTINGS_UI(dialog)
        _apply_full_width_general_layout(dialog)

    settings_ui._apply_settings_ui = apply_settings_ui
    _INSTALLED = True
