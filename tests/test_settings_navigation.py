from __future__ import annotations

import sys
from pathlib import Path
from types import MethodType, SimpleNamespace
import tkinter as tk
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
from SleufBase import app, settings_ui
from SleufBase.settings import AppSettings
from SleufBase.settings_navigation_patch import CATEGORIES, _select_category, _texts, _install_viewer_choice, _wrap_viewer_save


class PdokViewerDefaultTests(unittest.TestCase):
    def test_streetsmart_credentials_do_not_change_startup_source(self):
        class FakeApp:
            PDOK_LABEL = "unused"
            def _build_layout(self):
                self.source_during_layout = self.background_source_var.get()
            def _refresh_dashboard_metrics(self):
                self.metric_source = self.background_source_var.get()
        with patch.object(app, "KlicViewerApp", FakeApp):
            app._install_pdok_viewer_default_patch()
        for username in ("", "saved-streetsmart-user"):
            value = ["Cyclomedia"]
            instance = FakeApp()
            instance.settings = SimpleNamespace(streetsmart_username=username, streetsmart_password="saved-password")
            instance.background_source_var = SimpleNamespace(get=lambda: value[0], set=lambda v: value.__setitem__(0, v))
            instance._build_layout()
            self.assertEqual(instance.source_during_layout, app.KlicViewerApp.PDOK_LABEL)
            self.assertEqual(instance.metric_source, app.KlicViewerApp.PDOK_LABEL)

    def test_explicit_map_choice_still_selects_requested_provider(self):
        instance = SimpleNamespace(
            background_source_var=SimpleNamespace(get=lambda: app.KlicViewerApp.CYCLOMEDIA_AERIAL_LABEL),
            CYCLOMEDIA_AERIAL_LABEL=app.KlicViewerApp.CYCLOMEDIA_AERIAL_LABEL,
            OSM_LABEL=app.KlicViewerApp.OSM_LABEL, CADASTRAL_LABEL=app.KlicViewerApp.CADASTRAL_LABEL,
            cyclomedia_aerial_client=object(), viewer_pdok_client=object(),
        )
        self.assertIs(app.KlicViewerApp._current_background_provider(instance), instance.cyclomedia_aerial_client)


class SettingsNavigationTests(unittest.TestCase):
    def setUp(self):
        try:
            self.root = tk.Tk()
        except tk.TclError as exc:
            self.skipTest(f"Tk display unavailable: {exc}")
        self.addCleanup(self.root.destroy)
        self.root.settings = AppSettings()
        self.root.use_background_var = tk.BooleanVar(self.root, value=True)
        self.root.background_source_var = tk.StringVar(self.root, value=app.KlicViewerApp.PDOK_LABEL)
        for name in dir(app.KlicViewerApp):
            if name.isupper():
                setattr(self.root, name, getattr(app.KlicViewerApp, name))
        for name in ("_create_rounded_combobox", "_create_rounded_entry", "_create_rounded_spinbox"):
            setattr(self.root, name, MethodType(getattr(app.KlicViewerApp, name), self.root))
        self.root._apply_native_window_chrome = lambda *args: None
        app.KlicViewerApp.open_settings_dialog(self.root)
        self.root.update()
        self.dialog = next(w for w in self.root.winfo_children() if isinstance(w, tk.Toplevel))
        settings_ui._apply_settings_ui(self.dialog)
        self.root.update()

    def test_categories_preserve_values_and_keep_footer_visible(self):
        state = self.dialog._settings_navigation
        choices = [w for w in settings_ui._descendants(self.dialog) if w.winfo_class() == "TCombobox"]
        before = [w.get() for w in choices]
        self.assertEqual(self.dialog._settings_viewer_choice_var.get(), app.KlicViewerApp.PDOK_LABEL)
        for category in CATEGORIES:
            _select_category(self.dialog, category)
            self.root.update()
            visible = [(w, group, info) for w, (group, info) in state["widgets"].items() if w.winfo_manager() == "grid"]
            self.assertTrue(visible, category)
            self.assertTrue(all(group == category for w, group, info in visible))
            rows = [info["row"] for w, group, info in visible]
            self.assertEqual(len(rows), len(set(rows)))
            self.assertGreater(state["canvas"].winfo_height(), 150)
            self.assertEqual([w.get() for w in choices], before)
            save = next(w for w in settings_ui._descendants(self.dialog) if settings_ui._widget_text(w) == "Opslaan")
            self.assertTrue(save.winfo_ismapped())

    def test_dxf_colors_and_klic_options_are_in_the_correct_categories(self):
        groups = {_texts(w): group for w, (group, info) in self.dialog._settings_navigation["widgets"].items()}
        self.assertEqual(next(group for text, group in groups.items() if "Kleur PS-tekst" in text), "DXF-export")
        self.assertEqual(next(group for text, group in groups.items() if "KLIC-overlays meenemen" in text), "Kaarten")
        self.assertEqual(next(group for text, group in groups.items() if "Opdrachtgeverlogo op Blad1" in text), "Proefsleuven")

    def test_repeated_styling_does_not_duplicate_controls(self):
        count = len(list(settings_ui._descendants(self.dialog)))
        for _ in range(3):
            settings_ui._apply_settings_ui(self.dialog)
        self.assertEqual(len(list(settings_ui._descendants(self.dialog))), count)

    def test_words_view_returns_to_selected_category(self):
        state = self.dialog._settings_navigation
        _select_category(self.dialog, "KickTheMap")
        panel = next(w for w in settings_ui._descendants(self.dialog) if settings_ui._is_words_panel(w))
        settings_ui._open_words_view(self.dialog, panel)
        self.root.update()
        self.assertTrue(panel.winfo_ismapped())
        settings_ui._close_words_view(self.dialog, panel)
        self.root.update()
        self.assertEqual(state["selected"].get(), "KickTheMap")
        self.assertTrue(state["navigation"].winfo_ismapped())

    def test_material_bgt_and_late_marxact_controls_remain_available(self):
        from SleufBase.marxact_import_patch import _settings_row
        _settings_row(self.root, self.dialog)
        settings_ui._apply_settings_ui(self.dialog)
        state = self.dialog._settings_navigation
        material = self.dialog._settings_material_editor
        self.assertEqual(state["widgets"][material][0], "KickTheMap")
        self.assertTrue(any(group == "Proefsleuven" and "Maaiveld automatisch invullen vanuit BGT" in _texts(w)
                            for w, (group, info) in state["widgets"].items()))
        marxact = next(w for w, (group, info) in state["widgets"].items() if "MarXact namen" in _texts(w))
        self.assertEqual(state["widgets"][marxact][0], "KickTheMap")
        _select_category(self.dialog, "KickTheMap")
        self.root.update()
        self.assertTrue(material.winfo_ismapped())
        self.assertTrue(marxact.winfo_ismapped())

    def test_viewer_choice_applies_only_after_successful_save(self):
        from tkinter import ttk
        events = []
        self.root._on_background_source_changed = lambda: events.append("changed")
        for succeeds in (False, True):
            dialog = tk.Toplevel(self.root)
            content = ttk.Frame(dialog)
            content.pack()
            _install_viewer_choice(dialog, content, self.root)
            save = ttk.Button(dialog, text="Opslaan", command=dialog.destroy if succeeds else lambda: None)
            save.pack()
            _wrap_viewer_save(dialog)
            dialog._settings_viewer_choice_var.set(app.KlicViewerApp.OSM_LABEL)
            save.invoke()
            if succeeds:
                self.assertEqual(self.root.background_source_var.get(), app.KlicViewerApp.OSM_LABEL)
                self.assertEqual(events, ["changed"])
            else:
                self.assertEqual(self.root.background_source_var.get(), app.KlicViewerApp.PDOK_LABEL)
                self.assertEqual(events, [])
                dialog.destroy()


if __name__ == "__main__":
    unittest.main()
