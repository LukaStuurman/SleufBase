"""Categorized settings using the existing widgets and save callbacks."""
from __future__ import annotations

import tkinter as tk
from tkinter import ttk

from . import settings_ui
from .settings_general_layout_patch import _find_general_panel

CATEGORIES = {
    "Kaarten": "Kies de achtergrond voor het kaartscherm en voor je kaartuitvoer.",
    "DXF-export": "Stel de proefsleufweergave, kleuren en gekoppelde rasters in.",
    "Proefsleuven": "Stel dwarsprofielen, maaiveld en het opdrachtgeverlogo in.",
    "KickTheMap": "Beheer materiaalkeuzes, kabelnamen en de woordenlijst voor DXF-lagen.",
    "Back-ups": "Stel automatisch opslaan in en herstel een eerdere versie.",
}
_INSTALLED = False


def _texts(widget):
    return " ".join(settings_ui._widget_text(w) for w in (widget, *settings_ui._descendants(widget)))


def _category(text: str, previous: str) -> str:
    text = text.casefold()
    if "marxact" in text:
        return "KickTheMap"
    if "klic-overlays" in text or "achtergrondkaart" in text:
        return "Kaarten"
    if "weergave proefsleuf" in text:
        return "DXF-export"
    if "proefsleuven-sjabloon" in text or "vanuit bgt" in text or "discipline" in text:
        return "Proefsleuven"
    return previous


def _select_category(dialog, category, *, reset_scroll=True):
    state = dialog._settings_navigation
    state["selected"].set(category)
    state["help"].configure(text=CATEGORIES[category])
    for widget, (group, info) in state["widgets"].items():
        if group == category:
            widget.grid(**info)
        else:
            widget.grid_remove()
    state["canvas"].update_idletasks()
    state["canvas"].configure(scrollregion=state["canvas"].bbox("all"))
    if reset_scroll:
        state["canvas"].yview_moveto(0)


def _install_viewer_choice(dialog, content, app):
    if not hasattr(app, "background_source_var"):
        return None
    choice = ttk.Frame(content, style="Settings.Content.TFrame", padding=(0, 0, 0, 12))
    choice.columnconfigure(0, weight=1)
    ttk.Label(choice, text="Achtergrond op het kaartscherm", style="Settings.Subtitle.TLabel").grid(sticky="w")
    value = tk.StringVar(dialog, value=app.background_source_var.get())
    dialog._settings_viewer_choice_var = value
    ttk.Combobox(choice, textvariable=value, state="readonly", values=(
        app.PDOK_LABEL, app.CADASTRAL_LABEL, app.OSM_LABEL, app.CYCLOMEDIA_AERIAL_LABEL,
    )).grid(row=1, sticky="ew", pady=(5, 4))
    ttk.Label(choice, text="SleufBase start standaard met PDOK Luchtfoto. Hiervoor is geen account nodig.",
              style="Settings.Help.TLabel", wraplength=620).grid(row=2, sticky="w")
    return choice


def _wrap_viewer_save(dialog):
    if getattr(dialog, "_settings_viewer_save_wrapped", False):
        return
    value = getattr(dialog, "_settings_viewer_choice_var", None)
    if value is None:
        return
    app = dialog.master
    save = next((w for w in settings_ui._descendants(dialog)
                 if settings_ui._is_button(w) and settings_ui._widget_text(w) == "Opslaan"), None)
    if save is not None:
        original = save.cget("command")
        def save_with_viewer_choice():
            selected = value.get()
            result = original() if callable(original) else dialog.tk.call(str(original))
            # An invalid field keeps the dialog open. Apply the map choice only
            # after the existing save callback has succeeded and closed it.
            if not dialog.winfo_exists():
                app.background_source_var.set(selected)
                app._on_background_source_changed()
            return result
        save.configure(command=save_with_viewer_choice)
        dialog._settings_viewer_save_wrapped = True


def _install_navigation(dialog):
    if not dialog.winfo_exists() or dialog.title() != "Instellingen":
        return
    general = _find_general_panel(dialog)
    if general is None:
        return
    state = getattr(dialog, "_settings_navigation", None)
    if state is None:
        canvas = next((w for w in settings_ui._descendants(general)
                       if isinstance(w, tk.Canvas) and any(w.type(i) == "window" for i in w.find_all())), None)
        if canvas is None:
            return
        content = canvas.winfo_children()[0]
        shell = canvas.master
        shell.grid_configure(row=1)
        general.grid_rowconfigure(0, weight=0)
        general.grid_rowconfigure(1, weight=1)
        navigation = ttk.Frame(general, style="Settings.Content.TFrame", padding=(0, 0, 0, 14))
        navigation.grid(row=0, column=0, sticky="ew")
        selected = tk.StringVar(dialog, value="Kaarten")
        for column, category in enumerate(CATEGORIES):
            ttk.Radiobutton(navigation, text=category, value=category, variable=selected,
                            style="Settings.Tab.TRadiobutton",
                            command=lambda name=category: _select_category(dialog, name)).grid(
                                row=0, column=column, sticky="w", padx=(0, 12))
        help_label = ttk.Label(navigation, style="Settings.Help.TLabel", wraplength=760)
        help_label.grid(row=1, column=0, columnspan=len(CATEGORIES), sticky="w", pady=(10, 0))
        state = dict(canvas=canvas, content=content, shell=shell, navigation=navigation,
                     selected=selected, help=help_label, widgets={}, next_row=0, previous="Kaarten")
        dialog._settings_navigation = state
        # marXact adds its editor after the initial style passes (450/700 ms).
        late_pass = dialog.after(850, lambda: settings_ui._apply_settings_ui(dialog))
        def cancel_late_pass(event):
            if event.widget is dialog:
                dialog.after_cancel(late_pass)
        dialog.bind("<Destroy>", cancel_late_pass, add="+")
        app = dialog.master
        choice = _install_viewer_choice(dialog, content, app)
        if choice is not None:
            state["widgets"][choice] = ("Kaarten", dict(row=0, column=0, sticky="ew"))
            state["next_row"] = 1
        for w in settings_ui._descendants(dialog):
            if settings_ui._widget_text(w) == "Gemaakt door Luka Stuurman":
                w.configure(text="Kies een onderwerp. Klik op Opslaan om je wijzigingen toe te passen.")

    # Existing extension patches may add controls later. Collect each widget once.
    # Put extension editors inside the scroll area so the footer remains reachable.
    for widget in state["content"].winfo_children():
        if widget in state["widgets"] or widget.winfo_manager() != "grid":
            continue
        text = _texts(widget)
        group = _category(text, state["previous"])
        if "klic-overlays" not in text.casefold():
            state["previous"] = group
        _register_widget(state, widget, group)
    for attribute, group in (
        ("_settings_words_launcher", "KickTheMap"),
        ("_settings_material_editor", "KickTheMap"),
        ("_settings_profile_choices_editor", "KickTheMap"),
        ("_settings_autosave_editor", "Back-ups"),
    ):
        widget = getattr(dialog, attribute, None)
        if widget is not None and widget not in state["widgets"]:
            _register_widget(state, widget, group)
    for widget in general.winfo_children():
        if widget in state["widgets"] or widget in (state["shell"], state["navigation"]):
            continue
        if widget.winfo_manager() == "grid":
            _register_widget(state, widget, _category(_texts(widget), "Proefsleuven"))
    _select_category(dialog, state["selected"].get(), reset_scroll=False)
    general.grid_rowconfigure(1, weight=1)
    _wrap_viewer_save(dialog)


def _register_widget(state, widget, group):
    info = settings_ui._grid_info_copy(widget)
    info.update(row=state["next_row"], column=0, columnspan=1, sticky="ew")
    if widget.master is not state["content"]:
        info["in_"] = state["content"]
        general = widget.master
        old_row = int(widget.grid_info().get("row", 0))
        general.grid_rowconfigure(old_row, weight=0, minsize=0)
    state["next_row"] += 1
    state["widgets"][widget] = (group, info)


def install_settings_navigation_patch():
    global _INSTALLED
    if _INSTALLED:
        return
    original = settings_ui._apply_settings_ui
    def apply(dialog):
        original(dialog)
        _install_navigation(dialog)
    settings_ui._apply_settings_ui = apply
    _INSTALLED = True
