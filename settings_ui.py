from __future__ import annotations

import tkinter as tk
from tkinter import font as tkfont
from tkinter import ttk


_ORIGINAL_TK_TOPLEVEL = tk.Toplevel
_INSTALLED = False

_SECTION_TITLES = {
    "KickTheMap kabeltype, materiaal en DXF-laag": "KickTheMap",
    "KickTheMap woord-naar-laag": "KickTheMap",
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
    ): "Vult midden, links en rechts vanuit BGT. Handmatig ingevulde tekst blijft behouden.",
}

_HELP_TEXTS = {
    "Vult midden, links en rechts vanuit BGT. Handmatig ingevulde tekst blijft behouden.",
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


def _configure_styles(dialog: tk.Misc) -> None:
    try:
        style = ttk.Style(dialog)
        style.configure(
            "Settings.Section.TLabelframe",
            borderwidth=1,
            relief="solid",
            padding=(12, 8),
        )
        style.configure(
            "Settings.Section.TLabelframe.Label",
            font=("Segoe UI", 10, "bold"),
            foreground="#1f2937",
        )
        style.configure(
            "Settings.Help.TLabel",
            font=("Segoe UI", 8),
            foreground="#6b7280",
        )
        style.configure(
            "Settings.Subtitle.TLabel",
            font=("Segoe UI", 9, "bold"),
            foreground="#374151",
        )
        style.configure(
            "Settings.Option.TCheckbutton",
            font=("Segoe UI", 9),
            foreground="#111827",
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
            widget.grid_configure(padx=(26, 0))
    except (TypeError, ValueError, tk.TclError):
        pass


def _style_label_frame(widget: tk.Misc) -> None:
    text = _widget_text(widget)
    rewritten = _SECTION_TITLES.get(text)
    if rewritten:
        _set_widget_text(widget, rewritten)
    try:
        widget.configure(style="Settings.Section.TLabelframe", padding=(12, 8))
    except (AttributeError, tk.TclError):
        pass
    _compact_grid(widget, pady=(0, 10), sticky="ew")
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
        widget.configure(style="Settings.Option.TCheckbutton", padding=(0, 3))
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
    _compact_grid(widget, pady=(3, 2), sticky="w")


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
        _compact_grid(widget, pady=(7, 3), sticky="w")
        return

    if not _is_help_label(widget, text):
        return

    try:
        widget.configure(
            style="Settings.Help.TLabel",
            wraplength=500,
            justify="left",
        )
    except (AttributeError, tk.TclError):
        pass
    _compact_grid(widget, pady=(0, 5), sticky="w")
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


def _apply_settings_ui(dialog: tk.Misc) -> None:
    try:
        if not dialog.winfo_exists() or str(dialog.title()) != "Instellingen":
            return
    except tk.TclError:
        return

    _configure_styles(dialog)

    try:
        dialog.configure(background="#f5f7fb")
    except tk.TclError:
        pass

    for widget in list(_descendants(dialog)):
        text = _widget_text(widget)
        if text in _TEXT_REWRITES:
            _set_widget_text(widget, _TEXT_REWRITES[text])

        if _is_label_frame(widget):
            _style_label_frame(widget)
        elif _is_checkbutton(widget):
            _style_checkbutton(widget)
        elif _is_label(widget):
            _style_label(widget)
        elif _is_button(widget):
            _style_button(widget)

    # The settings dialog receives a few controls from runtime patches after
    # the original dialog has been built. Re-running is therefore deliberate
    # and keeps all sections visually consistent.
    try:
        dialog.update_idletasks()
    except tk.TclError:
        pass


def _schedule_settings_ui(dialog: tk.Misc) -> None:
    for delay in (0, 50, 180):
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
