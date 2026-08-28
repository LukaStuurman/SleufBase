from __future__ import annotations

import re
import tkinter as tk
from tkinter import font as tkfont
from tkinter import ttk


_ORIGINAL_TK_TOPLEVEL = tk.Toplevel
_INSTALLED = False
_SETTINGS_BG = "#f5f7fb"

_SECTION_TITLES = {
    "KickTheMap kabeltype, materiaal en DXF-laag": "KickTheMap – kabels",
    "KickTheMap woord-naar-laag": "KickTheMap – woorden",
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


def _compact_help_text(text: str, limit: int = 118) -> str:
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
            "Settings.Section.TLabelframe",
            borderwidth=0,
            relief="flat",
            padding=(0, 5),
            background=_SETTINGS_BG,
        )
        style.configure(
            "Settings.Section.TLabelframe.Label",
            font=("Segoe UI", 10, "bold"),
            foreground="#1f2937",
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

    # RoundedCheckbutton is a Canvas wrapper and keeps its own font object.
    # Updating that private font keeps the settings-specific style compact
    # without changing checkboxes elsewhere in the application.
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
            wraplength=640,
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


def _expand_list_widget(widget: tk.Misc) -> None:
    try:
        current_height = int(widget.cget("height"))
        if current_height < 6:
            widget.configure(height=6)
    except (AttributeError, TypeError, ValueError, tk.TclError):
        pass

    try:
        manager = widget.winfo_manager()
        if manager == "grid":
            info = widget.grid_info()
            row = int(info.get("row", 0))
            column = int(info.get("column", 0))
            widget.grid_configure(sticky="nsew")
            widget.master.rowconfigure(row, weight=1, minsize=105)
            widget.master.columnconfigure(column, weight=1)
        elif manager == "pack":
            widget.pack_configure(fill="both", expand=True)
    except (TypeError, ValueError, tk.TclError):
        pass


def _expand_word_rules_panel(panel: tk.Misc) -> None:
    original_title = str(getattr(panel, "_settings_original_section_title", "") or "")
    if original_title != "KickTheMap woord-naar-laag":
        return
    try:
        if panel.winfo_manager() == "grid":
            info = panel.grid_info()
            row = int(info.get("row", 0))
            panel.grid_configure(sticky="nsew")
            panel.master.rowconfigure(row, weight=1, minsize=180)
    except (TypeError, ValueError, tk.TclError):
        pass
    for child in _descendants(panel):
        if _is_expandable_list(child):
            _expand_list_widget(child)


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

        width = min(max(requested_width, 940), available_width)
        height = min(max(requested_height, 840), available_height)
        x = max(20, (screen_width - width) // 2)
        y = max(20, (screen_height - height) // 2)

        dialog.resizable(True, True)
        dialog.minsize(min(820, width), min(680, height))
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
        dialog.configure(background=_SETTINGS_BG)
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
        elif _is_checkbutton(widget):
            _style_checkbutton(widget)
        elif _is_label(widget):
            _style_label(widget)
        elif _is_button(widget):
            _style_button(widget)

    for panel in section_panels:
        _expand_word_rules_panel(panel)

    # The settings dialog receives a few controls from runtime patches after
    # the original dialog has been built. Re-running is therefore deliberate
    # and keeps all sections visually consistent.
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
