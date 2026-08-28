from __future__ import annotations

import re
import tkinter as tk
from tkinter import font as tkfont
from tkinter import ttk


_ORIGINAL_TK_TOPLEVEL = tk.Toplevel
_INSTALLED = False
_WINDOW_BG = "#f5f7fb"
_SETTINGS_BG = "#ffffff"
_WORDS_SOURCE_TITLE = "KickTheMap woord-naar-laag"
_WORDS_DISPLAY_TITLE = "KickTheMap – woorden"
_WORDS_VIEW_TITLE = "KickTheMap – woordenlijst"

_SECTION_TITLES = {
    "KickTheMap kabeltype, materiaal en DXF-laag": "KickTheMap – kabels",
    _WORDS_SOURCE_TITLE: _WORDS_DISPLAY_TITLE,
    "Proefsleuven-sjabloon": "Proefsleuven & sjabloon",
}

_TEXT_REWRITES = {
    "Gebruik kaartpunten voor maaiveldtekst en -kleur in sjabloonexport": (
        "Kaartpunten gebruiken voor maaiveldtekst en kleur"
    ),
    "Vul de drie maaiveldvakken automatisch met BGT fysiek_voorkomen": (
        "Maaiveld automatisch invullen vanuit BGT"
    ),
    "Materiaalkeuzes in de KickTheMap-browser": "Materiaalkeuzes",
    (
        "Bepaalt de middelste tekst over de volledige proefsleuflijn. Voor links en rechts "
        "wordt de lijn aan beide uiteinden denkbeeldig 1 meter verlengd. Handmatig ingevulde "
        "teksten blijven behouden."
    ): "Vult midden, links en rechts vanuit BGT. Handmatige tekst blijft staan.",
}

_HELP_TEXTS = {
    "Vult midden, links en rechts vanuit BGT. Handmatige tekst blijft staan.",
}


def _widget_text(widget: tk.Misc) -> str:
    try:
        return str(widget.cget("text") or "")
    except (AttributeError, tk.TclError):
        return ""


def _set_widget_text(widget: tk.Misc, text: str) -> None:
    try:
        widget.configure(text=text)
    except (AttributeError, tk.TclError):
        pass


def _descendants(widget: tk.Misc):
    for child in widget.winfo_children():
        yield child
        yield from _descendants(child)


def _widget_class(widget: tk.Misc) -> str:
    try:
        return str(widget.winfo_class() or "")
    except tk.TclError:
        return ""


def _is_label_frame(widget: tk.Misc) -> bool:
    return _widget_class(widget) in {"TLabelframe", "Labelframe"} or widget.__class__.__name__.endswith("LabelFrame")


def _is_label(widget: tk.Misc) -> bool:
    return _widget_class(widget) in {"TLabel", "Label"} and not _is_label_frame(widget)


def _is_checkbutton(widget: tk.Misc) -> bool:
    class_name = widget.__class__.__name__.casefold()
    return (
        "checkbutton" in class_name
        or _widget_class(widget) in {"TCheckbutton", "Checkbutton"}
    )


def _is_button(widget: tk.Misc) -> bool:
    class_name = widget.__class__.__name__.casefold()
    return "button" in class_name and "checkbutton" not in class_name


def _is_expandable_list(widget: tk.Misc) -> bool:
    return isinstance(widget, tk.Listbox) or _widget_class(widget) == "Treeview"


def _is_plain_frame(widget: tk.Misc) -> bool:
    return _widget_class(widget) in {"TFrame", "Frame"} and not _is_label_frame(widget)


def _compact_help_text(text: str, limit: int = 112) -> str:
    """Keep settings help useful without turning sections into paragraphs."""

    compact = " ".join(str(text or "").split())
    if len(compact) <= limit:
        return compact

    sentences = re.split(r"(?<=[.!?])\s+", compact)
    first = sentences[0].strip() if sentences else compact
    if 35 <= len(first) <= limit:
        return first

    clipped = compact[: limit + 1]
    if " " in clipped:
        clipped = clipped.rsplit(" ", 1)[0]
    return clipped.rstrip(" .;:") + "…"


def _configure_styles(dialog: tk.Misc) -> None:
    try:
        style = ttk.Style(dialog)
        style.configure(
            "Settings.Content.TFrame",
            borderwidth=0,
            relief="flat",
            background=_SETTINGS_BG,
        )
        style.configure(
            "Settings.Section.TLabelframe",
            borderwidth=0,
            relief="flat",
            padding=(0, 5),
            background=_SETTINGS_BG,
        )
        style.configure(
            "Settings.Section.TLabelframe.Label",
            font=("Segoe UI", 10, "bold"),
            foreground="#111827",
            background=_SETTINGS_BG,
        )
        style.configure(
            "Settings.Help.TLabel",
            font=("Segoe UI", 8),
            foreground="#6b7280",
            background=_SETTINGS_BG,
        )
        style.configure(
            "Settings.Subtitle.TLabel",
            font=("Segoe UI", 9, "bold"),
            foreground="#374151",
            background=_SETTINGS_BG,
        )
        style.configure(
            "Settings.Option.TCheckbutton",
            font=("Segoe UI", 9),
            foreground="#111827",
            background=_SETTINGS_BG,
        )
        style.configure(
            "Settings.Launcher.TFrame",
            borderwidth=0,
            relief="flat",
            background=_SETTINGS_BG,
        )
        style.configure(
            "Settings.LauncherTitle.TLabel",
            font=("Segoe UI", 10, "bold"),
            foreground="#111827",
            background=_SETTINGS_BG,
        )
        style.configure(
            "Settings.LauncherHelp.TLabel",
            font=("Segoe UI", 8),
            foreground="#6b7280",
            background=_SETTINGS_BG,
        )
        style.configure(
            "Settings.Primary.TButton",
            font=("Segoe UI", 9, "bold"),
            background="#f97316",
            foreground="#ffffff",
        )
        style.map(
            "Settings.Primary.TButton",
            background=[("active", "#ea580c"), ("pressed", "#c2410c")],
            foreground=[("disabled", "#f3f4f6")],
        )
        style.configure(
            "Settings.Secondary.TButton",
            font=("Segoe UI", 9),
        )
    except tk.TclError:
        pass


def _compact_grid(widget: tk.Misc, *, pady=None, padx=None, sticky=None) -> None:
    try:
        if widget.winfo_manager() != "grid":
            return
        options = {}
        if pady is not None:
            options["pady"] = pady
        if padx is not None:
            options["padx"] = padx
        if sticky is not None:
            options["sticky"] = sticky
        if options:
            widget.grid_configure(**options)
    except tk.TclError:
        pass


def _is_help_label(widget: tk.Misc, text: str) -> bool:
    if text in _HELP_TEXTS:
        return True
    if len(text) < 62:
        return False
    lowered = text.casefold()
    if lowered.endswith(":"):
        return False
    return any(mark in text for mark in (".", ";", " — ")) or any(
        word in lowered
        for word in (
            "wordt ",
            "gebruik ",
            "hiermee ",
            "alleen ",
            "automatisch ",
            "bepaalt ",
        )
    )


def _indent_help_below_checkbox(widget: tk.Misc) -> None:
    try:
        if widget.winfo_manager() != "grid":
            return
        info = widget.grid_info()
        row = int(info.get("row", 0))
        if row <= 0:
            return
        previous = widget.master.grid_slaves(row=row - 1)
        if any(_is_checkbutton(item) for item in previous):
            widget.grid_configure(padx=(24, 0))
    except (TypeError, ValueError, tk.TclError):
        pass


def _style_plain_frame(widget: tk.Misc) -> None:
    try:
        if _widget_class(widget) == "TFrame":
            widget.configure(style="Settings.Content.TFrame")
        elif _widget_class(widget) == "Frame":
            widget.configure(background=_SETTINGS_BG, borderwidth=0, highlightthickness=0)
    except (AttributeError, tk.TclError):
        pass


def _style_label_frame(widget: tk.Misc) -> None:
    text = _widget_text(widget)
    if not hasattr(widget, "_settings_original_section_title"):
        try:
            widget._settings_original_section_title = text
        except Exception:
            pass
    rewritten = _SECTION_TITLES.get(text)
    if rewritten:
        _set_widget_text(widget, rewritten)
    try:
        widget.configure(style="Settings.Section.TLabelframe", padding=(0, 5))
    except (AttributeError, tk.TclError):
        try:
            widget.configure(
                borderwidth=0,
                relief="flat",
                highlightthickness=0,
                background=_SETTINGS_BG,
            )
        except (AttributeError, tk.TclError):
            pass
    _compact_grid(widget, pady=(0, 6), sticky="ew")
    try:
        widget.columnconfigure(0, weight=1)
    except tk.TclError:
        pass


def _style_checkbutton(widget: tk.Misc) -> None:
    text = _widget_text(widget)
    rewritten = _TEXT_REWRITES.get(text)
    if rewritten:
        _set_widget_text(widget, rewritten)
    try:
        widget.configure(style="Settings.Option.TCheckbutton", padding=(0, 2))
    except (AttributeError, tk.TclError):
        pass

    try:
        if hasattr(widget, "_font"):
            compact_font = tkfont.Font(widget, family="Segoe UI", size=9)
            widget._font = compact_font
            widget._settings_compact_font = compact_font
            redraw = getattr(widget, "_redraw", None)
            if callable(redraw):
                redraw()
    except (AttributeError, tk.TclError):
        pass
    _compact_grid(widget, pady=(2, 1), sticky="w")


def _style_label(widget: tk.Misc) -> None:
    text = _widget_text(widget)
    rewritten = _TEXT_REWRITES.get(text)
    if rewritten:
        _set_widget_text(widget, rewritten)
        text = rewritten

    if text == "Materiaalkeuzes":
        try:
            widget.configure(style="Settings.Subtitle.TLabel")
        except (AttributeError, tk.TclError):
            pass
        _compact_grid(widget, pady=(5, 2), sticky="w")
        return

    if not _is_help_label(widget, text):
        return

    compact_text = _compact_help_text(text)
    if compact_text != text:
        _set_widget_text(widget, compact_text)
    try:
        widget.configure(
            style="Settings.Help.TLabel",
            wraplength=620,
            justify="left",
        )
    except (AttributeError, tk.TclError):
        pass
    _compact_grid(widget, pady=(0, 3), sticky="w")
    _indent_help_below_checkbox(widget)


def _style_button(widget: tk.Misc) -> None:
    text = _widget_text(widget)
    try:
        if text == "Opslaan":
            widget.configure(style="Settings.Primary.TButton", padding=(14, 7))
        elif text in {"Annuleren", "Sluiten"}:
            widget.configure(style="Settings.Secondary.TButton", padding=(12, 7))
    except (AttributeError, tk.TclError):
        pass


def _expand_list_widget(widget: tk.Misc, *, minimum_height: int = 10) -> None:
    try:
        current_height = int(widget.cget("height"))
        if current_height < minimum_height:
            widget.configure(height=minimum_height)
    except (AttributeError, TypeError, ValueError, tk.TclError):
        pass

    try:
        manager = widget.winfo_manager()
        if manager == "grid":
            info = widget.grid_info()
            row = int(info.get("row", 0))
            column = int(info.get("column", 0))
            widget.grid_configure(sticky="nsew")
            widget.master.rowconfigure(row, weight=1, minsize=150)
            widget.master.columnconfigure(column, weight=1)
        elif manager == "pack":
            widget.pack_configure(fill="both", expand=True)
    except (TypeError, ValueError, tk.TclError):
        pass


def _is_words_panel(panel: tk.Misc) -> bool:
    original_title = str(getattr(panel, "_settings_original_section_title", "") or "")
    current_title = _widget_text(panel)
    return original_title == _WORDS_SOURCE_TITLE or current_title in {
        _WORDS_SOURCE_TITLE,
        _WORDS_DISPLAY_TITLE,
    }


def _expand_word_rules_panel(panel: tk.Misc) -> None:
    if not _is_words_panel(panel):
        return
    try:
        panel.columnconfigure(0, weight=1)
    except tk.TclError:
        pass
    for child in _descendants(panel):
        if _is_expandable_list(child):
            _expand_list_widget(child, minimum_height=12)


def _grid_info_copy(widget: tk.Misc) -> dict[str, object]:
    try:
        info = dict(widget.grid_info())
    except tk.TclError:
        return {}
    info.pop("in", None)
    return info


def _occupied_grid_columns(parent: tk.Misc) -> int:
    maximum = 1
    try:
        children = parent.grid_slaves()
    except tk.TclError:
        return maximum
    for child in children:
        try:
            info = child.grid_info()
            column = int(info.get("column", 0))
            span = max(1, int(info.get("columnspan", 1)))
            maximum = max(maximum, column + span)
        except (TypeError, ValueError, tk.TclError):
            continue
    return maximum


def _snapshot_grid_configuration(parent: tk.Misc) -> tuple[dict[int, dict[str, object]], dict[int, dict[str, object]]]:
    rows: dict[int, dict[str, object]] = {}
    columns: dict[int, dict[str, object]] = {}
    try:
        children = parent.grid_slaves()
    except tk.TclError:
        return rows, columns

    row_indexes: set[int] = set()
    column_indexes: set[int] = set()
    for child in children:
        try:
            info = child.grid_info()
            row_indexes.add(int(info.get("row", 0)))
            column = int(info.get("column", 0))
            span = max(1, int(info.get("columnspan", 1)))
            column_indexes.update(range(column, column + span))
        except (TypeError, ValueError, tk.TclError):
            continue

    for row in row_indexes:
        try:
            rows[row] = dict(parent.grid_rowconfigure(row))
        except tk.TclError:
            pass
    for column in column_indexes:
        try:
            columns[column] = dict(parent.grid_columnconfigure(column))
        except tk.TclError:
            pass
    return rows, columns


def _restore_grid_configuration(
    parent: tk.Misc,
    rows: dict[int, dict[str, object]],
    columns: dict[int, dict[str, object]],
) -> None:
    for row, config in rows.items():
        try:
            parent.grid_rowconfigure(row, **config)
        except tk.TclError:
            pass
    for column, config in columns.items():
        try:
            parent.grid_columnconfigure(column, **config)
        except tk.TclError:
            pass


def _close_words_view(dialog: tk.Misc, panel: tk.Misc) -> None:
    state = getattr(dialog, "_settings_words_view_state", None)
    if not state:
        return
    parent = state["parent"]

    try:
        panel.grid_remove()
    except tk.TclError:
        pass
    toolbar = state.get("toolbar")
    if toolbar is not None:
        try:
            toolbar.grid_remove()
        except tk.TclError:
            pass

    for child, info in state.get("siblings", []):
        try:
            child.grid(**info)
        except tk.TclError:
            pass

    _restore_grid_configuration(
        parent,
        state.get("rows", {}),
        state.get("columns", {}),
    )

    original_info = state.get("panel_info", {})
    if original_info:
        try:
            panel.grid(**original_info)
            panel.grid_remove()
        except tk.TclError:
            pass

    try:
        dialog._settings_words_view_state = None
        dialog.title("Instellingen")
    except (AttributeError, tk.TclError):
        pass


def _open_words_view(dialog: tk.Misc, panel: tk.Misc) -> None:
    if getattr(dialog, "_settings_words_view_state", None):
        return
    parent = panel.master
    panel_info = dict(getattr(panel, "_settings_words_original_grid", {}) or {})
    if not panel_info:
        panel_info = _grid_info_copy(panel)
    if not panel_info:
        return

    rows, columns = _snapshot_grid_configuration(parent)
    siblings: list[tuple[tk.Misc, dict[str, object]]] = []
    try:
        for child in parent.grid_slaves():
            if child is panel:
                continue
            info = _grid_info_copy(child)
            if not info:
                continue
            siblings.append((child, info))
            child.grid_remove()
    except tk.TclError:
        return

    toolbar = getattr(dialog, "_settings_words_toolbar", None)
    if toolbar is None or not bool(toolbar.winfo_exists()):
        toolbar = ttk.Frame(parent, style="Settings.Content.TFrame", padding=(0, 0, 0, 8))
        toolbar.columnconfigure(1, weight=1)
        back_button = ttk.Button(
            toolbar,
            text="← Terug naar instellingen",
            style="Settings.Secondary.TButton",
            command=lambda target=dialog, rules=panel: _close_words_view(target, rules),
        )
        back_button.grid(row=0, column=0, sticky="w")
        ttk.Label(
            toolbar,
            text="Woorden, DXF-lagen, kleuren en dwarsprofielnamen",
            style="Settings.LauncherHelp.TLabel",
        ).grid(row=0, column=1, sticky="e")
        try:
            dialog._settings_words_toolbar = toolbar
        except Exception:
            pass

    total_columns = max(1, _occupied_grid_columns(parent))
    toolbar.grid(row=0, column=0, columnspan=total_columns, sticky="ew", padx=0, pady=(0, 6))
    panel.grid(row=1, column=0, columnspan=total_columns, sticky="nsew", padx=0, pady=0)

    try:
        parent.grid_rowconfigure(0, weight=0)
        parent.grid_rowconfigure(1, weight=1)
        for column in range(total_columns):
            parent.grid_columnconfigure(column, weight=1, minsize=0)
        panel.columnconfigure(0, weight=1)
    except tk.TclError:
        pass

    _expand_word_rules_panel(panel)
    try:
        dialog._settings_words_view_state = {
            "parent": parent,
            "panel_info": panel_info,
            "siblings": siblings,
            "rows": rows,
            "columns": columns,
            "toolbar": toolbar,
        }
        dialog.title(_WORDS_VIEW_TITLE)
        dialog.update_idletasks()
        screen_width = max(int(dialog.winfo_screenwidth()), 900)
        screen_height = max(int(dialog.winfo_screenheight()), 700)
        width = min(max(1040, int(dialog.winfo_reqwidth())), screen_width - 80)
        height = min(max(760, int(dialog.winfo_reqheight())), screen_height - 100)
        x = max(20, (screen_width - width) // 2)
        y = max(20, (screen_height - height) // 2)
        dialog.geometry(f"{width}x{height}+{x}+{y}")
    except (AttributeError, TypeError, ValueError, tk.TclError):
        pass


def _install_words_launcher(dialog: tk.Misc, panel: tk.Misc) -> None:
    if getattr(dialog, "_settings_words_launcher", None) is not None:
        return
    try:
        if panel.winfo_manager() != "grid":
            return
        panel_info = _grid_info_copy(panel)
        if not panel_info:
            return
        panel._settings_words_original_grid = dict(panel_info)
        panel.grid_remove()
    except (AttributeError, tk.TclError):
        return

    parent = panel.master
    launcher = ttk.Frame(parent, style="Settings.Launcher.TFrame", padding=(16, 12))
    launcher.columnconfigure(0, weight=1)
    ttk.Label(
        launcher,
        text="KickTheMap woordenlijst",
        style="Settings.LauncherTitle.TLabel",
    ).grid(row=0, column=0, sticky="w")
    ttk.Label(
        launcher,
        text="Beheer woorden/codes, DXF-lagen, kleuren en dwarsprofielen in een ruime aparte weergave.",
        style="Settings.LauncherHelp.TLabel",
        wraplength=260,
        justify="left",
    ).grid(row=1, column=0, sticky="w", pady=(3, 10))
    ttk.Button(
        launcher,
        text="Woordenlijst beheren…",
        command=lambda target=dialog, rules=panel: _open_words_view(target, rules),
        padding=(12, 7),
    ).grid(row=2, column=0, sticky="ew")

    try:
        launcher.grid(
            row=int(panel_info.get("row", 0)),
            column=int(panel_info.get("column", 0)),
            columnspan=max(1, int(panel_info.get("columnspan", 1))),
            sticky="new",
            padx=panel_info.get("padx", 0),
            pady=panel_info.get("pady", 0),
        )
        word_column = int(panel_info.get("column", 0))
        word_span = max(1, int(panel_info.get("columnspan", 1)))
        for column in range(word_column, word_column + word_span):
            parent.grid_columnconfigure(column, weight=0, minsize=250)
        for column in range(word_column):
            parent.grid_columnconfigure(column, weight=1, minsize=0)
        dialog._settings_words_launcher = launcher
    except (AttributeError, TypeError, ValueError, tk.TclError):
        try:
            launcher.destroy()
        except tk.TclError:
            pass


def _configure_dialog_geometry(dialog: tk.Misc) -> None:
    if bool(getattr(dialog, "_settings_geometry_done", False)):
        return
    try:
        dialog.update_idletasks()
        screen_width = max(int(dialog.winfo_screenwidth()), 800)
        screen_height = max(int(dialog.winfo_screenheight()), 700)
        available_width = max(760, screen_width - 80)
        available_height = max(620, screen_height - 100)
        requested_width = max(int(dialog.winfo_reqwidth()), int(dialog.winfo_width()))
        requested_height = max(int(dialog.winfo_reqheight()), int(dialog.winfo_height()))

        width = min(max(requested_width, 980), available_width)
        height = min(max(requested_height, 720), available_height)
        x = max(20, (screen_width - width) // 2)
        y = max(20, (screen_height - height) // 2)

        dialog.resizable(True, True)
        dialog.minsize(min(860, width), min(620, height))
        dialog.geometry(f"{width}x{height}+{x}+{y}")
        dialog._settings_geometry_done = True
    except (AttributeError, TypeError, ValueError, tk.TclError):
        pass


def _apply_settings_ui(dialog: tk.Misc) -> None:
    try:
        if not dialog.winfo_exists() or str(dialog.title()) != "Instellingen":
            return
    except tk.TclError:
        return

    _configure_styles(dialog)

    try:
        dialog.configure(background=_WINDOW_BG)
    except tk.TclError:
        pass

    descendants = list(_descendants(dialog))
    section_panels: list[tk.Misc] = []
    for widget in descendants:
        text = _widget_text(widget)
        if text in _TEXT_REWRITES:
            _set_widget_text(widget, _TEXT_REWRITES[text])

        if _is_label_frame(widget):
            _style_label_frame(widget)
            section_panels.append(widget)
        elif _is_plain_frame(widget):
            _style_plain_frame(widget)
        elif _is_checkbutton(widget):
            _style_checkbutton(widget)
        elif _is_label(widget):
            _style_label(widget)
        elif _is_button(widget):
            _style_button(widget)

    words_panel = next((panel for panel in section_panels if _is_words_panel(panel)), None)
    if words_panel is not None:
        _expand_word_rules_panel(words_panel)
        _install_words_launcher(dialog, words_panel)

    try:
        dialog.update_idletasks()
    except tk.TclError:
        pass
    _configure_dialog_geometry(dialog)


def _schedule_settings_ui(dialog: tk.Misc) -> None:
    for delay in (0, 50, 180, 400):
        try:
            if delay == 0:
                dialog.after_idle(lambda target=dialog: _apply_settings_ui(target))
            else:
                dialog.after(delay, lambda target=dialog: _apply_settings_ui(target))
        except tk.TclError:
            return


class SettingsAwareToplevel(_ORIGINAL_TK_TOPLEVEL):
    def title(self, string=None):  # type: ignore[override]
        if string is None:
            return super().title()
        result = super().title(string)
        if str(string) == "Instellingen":
            _schedule_settings_ui(self)
        return result

    wm_title = title


def install_settings_ui_patch() -> None:
    global _INSTALLED
    if _INSTALLED:
        return
    tk.Toplevel = SettingsAwareToplevel  # type: ignore[assignment]
    _INSTALLED = True
