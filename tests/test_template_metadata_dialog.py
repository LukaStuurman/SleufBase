"""Exercise the actual source dialog without starting accounts/map services."""
import ast
from pathlib import Path
import re
from types import SimpleNamespace
import tkinter as tk
from tkinter import ttk, messagebox
import unittest


class TemplateMetadataDialogTests(unittest.TestCase):
    def test_project_entry_confirm_and_cancel(self):
        class Viewer(tk.Tk):
            APP_BG = INPUT_BG = "#ffffff"
            TEXT = BORDER = "#222222"
            ACCENT = "#0066cc"
            TEMPLATE_EXPORT_SCALE_OPTIONS = (100, 200)

            def _normalize_template_export_slot_entries(self, value):
                return value

            def _store_template_export_order_state(self, *args):
                pass

            def _initial_template_export_order_state(self):
                return [self.layer], [True], [None]

            def _template_export_layer_key(self, layer):
                return str(layer.path)

            def _format_template_export_scale_denominator(self, value):
                return "Auto" if value is None else f"1:{value}"

            def wait_window(self, dialog):
                def descendants(widget):
                    for child in widget.winfo_children():
                        yield child
                        yield from descendants(child)
                widgets = list(descendants(dialog))
                self.test_case.assertEqual(dialog.title(), "DXF-sjabloon exporteren")
                label = next(w for w in widgets if isinstance(w, ttk.Label) and w.cget("text") == "Projectnaam")
                entry = next(w for w in label.master.winfo_children() if isinstance(w, ttk.Entry))
                self.test_case.assertEqual(entry.get(), self.expected_initial)
                entry.delete(0, tk.END)
                entry.insert(0, self.entered_name)
                button = next(w for w in widgets if isinstance(w, ttk.Button) and w.cget("text") == self.action)
                button.invoke()

        # Only this source installer is loaded. No legacy module or login starts.
        source = Path(__file__).resolve().parents[1] / "app.py"
        tree = ast.parse(source.read_text(encoding="utf-8"))
        installer = next(n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name == "_install_template_variant_order_patch")
        namespace = dict(KlicViewerApp=Viewer, tk=tk, ttk=ttk, messagebox=messagebox, re=re, __package__="SleufBase")
        exec(compile(ast.Module(body=[installer], type_ignores=[]), str(source), "exec"), namespace)
        namespace[installer.name]()
        try:
            viewer = Viewer()
        except tk.TclError as exc:
            self.skipTest(f"Tk display unavailable: {exc}")
        try:
            viewer.withdraw()
            viewer.test_case = self
            viewer.layer = SimpleNamespace(path=Path("PS1.tif"), metadata={})
            viewer.cadastral_exporter = SimpleNamespace(_proefsleuf_base_name=lambda layer, index: "PS1")
            viewer.expected_initial = ""
            viewer.entered_name = "  Zuid Ø 漢😀  "
            viewer.action = "Exporteren"
            self.assertEqual(viewer._choose_template_export_order(), [viewer.layer])
            self.assertEqual(viewer.cadastral_exporter.template_project_name, "Zuid Ø 漢😀")
            viewer.expected_initial = "Zuid Ø 漢😀"
            viewer.entered_name = "Cancelled change"
            viewer.action = "Annuleren"
            self.assertIsNone(viewer._choose_template_export_order())
            self.assertEqual(viewer.cadastral_exporter.template_project_name, "Zuid Ø 漢😀")
        finally:
            viewer.destroy()
