from __future__ import annotations

import tkinter as tk
from tkinter import messagebox, ttk
from typing import Any

from . import settings_general_layout_patch as general_layout
from . import settings_ui
from .autosave_backup_patch import (
    AutosaveManager,
    AutosaveSettings,
    load_autosave_settings,
    prune_backups,
    save_autosave_settings,
)
from .autosave_restore_ui import list_autosaves, show_autosave_selector
from .settings import (
    KICKTHEMAP_PROFILE_EXTRA_CHOICES_KEY,
    normalize_kickthemap_profile_extra_choices,
)


PATCH_VERSION = 1
_INSTALLED = False


def _app_for_dialog(dialog: tk.Misc) -> Any | None:
    current = getattr(dialog, "master", None)
    visited: set[int] = set()
    while current is not None and id(current) not in visited:
        visited.add(id(current))
        if hasattr(current, "settings"):
            return current
        current = getattr(current, "master", None)
    return None


def _find_save_button(dialog: tk.Misc) -> tk.Misc | None:
    return next(
        (
            widget
            for widget in settings_ui._descendants(dialog)
            if settings_ui._is_button(widget) and settings_ui._widget_text(widget) == "Opslaan"
        ),
        None,
    )


def _section_frame(parent: tk.Misc, *, top_padding: int = 10) -> ttk.Frame:
    frame = ttk.Frame(
        parent,
        style="Settings.Launcher.TFrame",
        padding=(0, top_padding, 0, 0),
    )
    frame.columnconfigure(0, weight=1)
    return frame


def _grid_section(parent: tk.Misc, section: tk.Misc) -> None:
    row = general_layout._next_grid_row(parent)
    columns = max(1, settings_ui._occupied_grid_columns(parent))
    try:
        for column in range(columns):
            parent.grid_columnconfigure(column, weight=1, minsize=0)
        section.grid(
            row=row,
            column=0,
            columnspan=columns,
            sticky="ew",
            pady=(4, 0),
        )
    except tk.TclError:
        pass


def _profile_extra_values(app: Any) -> list[str]:
    return normalize_kickthemap_profile_extra_choices(
        getattr(app.settings, KICKTHEMAP_PROFILE_EXTRA_CHOICES_KEY, [])
    )


def _install_profile_choices_editor(
    dialog: tk.Misc,
    general_panel: tk.Misc,
    app: Any,
) -> None:
    existing = getattr(dialog, "_settings_profile_choices_editor", None)
    if existing is not None:
        try:
            if bool(existing.winfo_exists()):
                return
        except (AttributeError, tk.TclError):
            pass

    editor = _section_frame(general_panel)
    ttk.Separator(editor, orient="horizontal").grid(
        row=0, column=0, columnspan=3, sticky="ew", pady=(0, 8)
    )
    ttk.Label(
        editor,
        text="KickTheMap Kabel/Leiding-keuzes",
        style="Settings.Subtitle.TLabel",
    ).grid(row=1, column=0, columnspan=3, sticky="w")
    ttk.Label(
        editor,
        text="Extra woorden voor de Kabel/Leiding-dropdown; keuzes uit Woordenlijst beheren blijven automatisch beschikbaar.",
        style="Settings.Help.TLabel",
        wraplength=760,
        justify="left",
    ).grid(row=2, column=0, columnspan=3, sticky="w", pady=(1, 5))

    choice_var = tk.StringVar(editor)
    entry = ttk.Entry(editor, textvariable=choice_var)
    entry.grid(row=3, column=0, sticky="ew")

    choices = tk.Listbox(
        editor,
        height=4,
        exportselection=False,
        background="#fbfcfe",
        foreground="#111827",
        selectbackground="#f97316",
        selectforeground="#ffffff",
        borderwidth=0,
        highlightthickness=1,
        highlightbackground="#e5e7eb",
        relief=tk.FLAT,
    )
    choices.grid(row=4, column=0, sticky="ew", pady=(6, 0))
    scrollbar = ttk.Scrollbar(editor, orient="vertical", command=choices.yview)
    scrollbar.grid(row=4, column=1, sticky="ns", padx=(6, 0), pady=(6, 0))
    choices.configure(yscrollcommand=scrollbar.set)

    for value in _profile_extra_values(app):
        choices.insert(tk.END, value)

    def add_choice() -> None:
        value = choice_var.get().strip()
        if not value:
            entry.focus_set()
            return
        existing_values = {
            str(choices.get(index)).strip().casefold()
            for index in range(int(choices.size()))
        }
        if value.casefold() not in existing_values:
            choices.insert(tk.END, value)
            choices.see(tk.END)
        choice_var.set("")
        entry.focus_set()

    def remove_choice() -> None:
        selection = choices.curselection()
        if selection:
            choices.delete(selection[0])

    ttk.Button(
        editor,
        text="Toevoegen",
        command=add_choice,
        padding=(10, 5),
    ).grid(row=3, column=1, columnspan=2, padx=(8, 0), sticky="e")
    ttk.Button(
        editor,
        text="Verwijderen",
        command=remove_choice,
        padding=(10, 5),
    ).grid(row=4, column=2, padx=(8, 0), pady=(6, 0), sticky="n")

    entry.bind("<Return>", lambda _event: add_choice())
    choices.bind("<Delete>", lambda _event: remove_choice())
    _grid_section(general_panel, editor)

    dialog._settings_profile_choices_editor = editor
    dialog._settings_profile_extra_list = choices


def _current_autosave_settings(app: Any) -> AutosaveSettings:
    manager = getattr(app, "_sleufbase_autosave_manager", None)
    if isinstance(manager, AutosaveManager):
        return manager.settings
    return load_autosave_settings()


def _install_autosave_editor(
    dialog: tk.Misc,
    general_panel: tk.Misc,
    app: Any,
) -> None:
    existing = getattr(dialog, "_settings_autosave_editor", None)
    if existing is not None:
        try:
            if bool(existing.winfo_exists()):
                return
        except (AttributeError, tk.TclError):
            pass

    settings = _current_autosave_settings(app)
    editor = _section_frame(general_panel)
    ttk.Separator(editor, orient="horizontal").grid(
        row=0, column=0, columnspan=8, sticky="ew", pady=(0, 8)
    )
    ttk.Label(
        editor,
        text="Autosave",
        style="Settings.Subtitle.TLabel",
    ).grid(row=1, column=0, columnspan=8, sticky="w")

    enabled_var = tk.BooleanVar(editor, value=settings.enabled)
    interval_var = tk.IntVar(editor, value=settings.interval_minutes)
    max_var = tk.IntVar(editor, value=settings.max_backups)

    ttk.Checkbutton(
        editor,
        text="Automatisch opslaan",
        variable=enabled_var,
        style="Settings.Option.TCheckbutton",
    ).grid(row=2, column=0, sticky="w", pady=(5, 4))

    ttk.Label(editor, text="Elke").grid(row=2, column=1, padx=(18, 4), sticky="e")
    ttk.Spinbox(
        editor,
        from_=1,
        to=1440,
        textvariable=interval_var,
        width=7,
    ).grid(row=2, column=2, sticky="w")
    ttk.Label(editor, text="minuten").grid(row=2, column=3, padx=(4, 18), sticky="w")

    ttk.Label(editor, text="Maximaal").grid(row=2, column=4, sticky="e")
    ttk.Spinbox(
        editor,
        from_=1,
        to=200,
        textvariable=max_var,
        width=7,
    ).grid(row=2, column=5, padx=(4, 4), sticky="w")
    ttk.Label(editor, text="autosaves").grid(row=2, column=6, sticky="w")

    backup_count = len(list_autosaves())
    ttk.Label(
        editor,
        text=f"Beschikbaar: {backup_count}",
        style="Settings.Help.TLabel",
    ).grid(row=3, column=0, sticky="w", pady=(4, 0))
    ttk.Button(
        editor,
        text="Autosave laden…",
        command=lambda: show_autosave_selector(app, parent=dialog),
        padding=(10, 5),
    ).grid(row=3, column=6, columnspan=2, sticky="e", pady=(4, 0))

    editor.columnconfigure(0, weight=1)
    _grid_section(general_panel, editor)

    dialog._settings_autosave_editor = editor
    dialog._settings_autosave_enabled_var = enabled_var
    dialog._settings_autosave_interval_var = interval_var
    dialog._settings_autosave_max_var = max_var


def _profile_values_from_dialog(dialog: tk.Misc) -> list[str]:
    listbox = getattr(dialog, "_settings_profile_extra_list", None)
    if not isinstance(listbox, tk.Listbox):
        return []
    try:
        return [str(listbox.get(index)) for index in range(int(listbox.size()))]
    except tk.TclError:
        return []


def _apply_autosave_settings_from_dialog(dialog: tk.Misc, app: Any) -> None:
    enabled_var = getattr(dialog, "_settings_autosave_enabled_var", None)
    interval_var = getattr(dialog, "_settings_autosave_interval_var", None)
    max_var = getattr(dialog, "_settings_autosave_max_var", None)
    if enabled_var is None or interval_var is None or max_var is None:
        return

    settings = AutosaveSettings(
        enabled=bool(enabled_var.get()),
        interval_minutes=int(interval_var.get()),
        max_backups=int(max_var.get()),
    ).normalized()
    manager = getattr(app, "_sleufbase_autosave_manager", None)
    if isinstance(manager, AutosaveManager):
        manager.apply_settings(settings)
    else:
        saved = save_autosave_settings(settings)
        prune_backups(saved.max_backups)


def _wrap_save_button(dialog: tk.Misc, app: Any) -> None:
    if bool(getattr(dialog, "_settings_extended_save_wrapped", False)):
        return
    save_button = _find_save_button(dialog)
    if save_button is None:
        return

    try:
        original_save_command = save_button.cget("command")
    except (AttributeError, tk.TclError):
        return

    def save_extended_settings() -> Any:
        try:
            setattr(
                app.settings,
                KICKTHEMAP_PROFILE_EXTRA_CHOICES_KEY,
                normalize_kickthemap_profile_extra_choices(
                    _profile_values_from_dialog(dialog)
                ),
            )
            _apply_autosave_settings_from_dialog(dialog, app)
        except Exception as exc:
            messagebox.showerror(
                "Instellingen",
                f"Instellingen konden niet worden opgeslagen:\n{exc}",
                parent=dialog,
            )
            return None

        if callable(original_save_command):
            return original_save_command()
        if original_save_command:
            return dialog.tk.call(str(original_save_command))
        return None

    try:
        save_button.configure(command=save_extended_settings)
        dialog._settings_extended_save_wrapped = True
    except (AttributeError, tk.TclError):
        pass


def _install_extended_controls(dialog: tk.Misc) -> None:
    try:
        if not dialog.winfo_exists() or str(dialog.title()) != "Instellingen":
            return
    except tk.TclError:
        return

    app = _app_for_dialog(dialog)
    general_panel = general_layout._find_general_panel(dialog)
    if app is None or general_panel is None:
        return

    _install_profile_choices_editor(dialog, general_panel, app)
    _install_autosave_editor(dialog, general_panel, app)
    _wrap_save_button(dialog, app)


def install_settings_kickthemap_autosave_patch() -> None:
    global _INSTALLED
    if _INSTALLED:
        return

    original_apply_settings_ui = settings_ui._apply_settings_ui

    def apply_settings_ui(dialog: tk.Misc) -> None:
        original_apply_settings_ui(dialog)
        _install_extended_controls(dialog)

    settings_ui._apply_settings_ui = apply_settings_ui
    _INSTALLED = True
