from __future__ import annotations

import unittest

from SleufBase.workflow_usability_patch import (
    CommandAction,
    MAX_ACTIVITY_ITEMS,
    append_activity,
    available_shortcuts,
    collect_actions_from_specs,
    install_workflow_usability_patch,
    score_action,
)


class WorkflowUsabilityPureTests(unittest.TestCase):
    def test_score_prefers_exact_label_over_path_and_fuzzy_matches(self) -> None:
        noop = lambda: None
        exact = CommandAction("Bestand › Export", "Export", noop)
        contains = CommandAction("Bestand › DXF exporteren", "DXF exporteren", noop)
        fuzzy = CommandAction("KickTheMap › Jobs", "Jobs", noop)
        self.assertGreater(score_action("export", exact), score_action("export", contains))
        self.assertGreater(score_action("export", contains), 0)
        self.assertGreater(score_action("ktm jobs", fuzzy), 0)

    def test_collect_actions_ignores_disabled_and_palette_recursion(self) -> None:
        calls = []
        specs = [
            {
                "label": "Bestand",
                "items": [
                    {"type": "command", "label": "Exporteren", "state": "normal", "command": lambda: calls.append("export")},
                    {"type": "command", "label": "Niet beschikbaar", "state": "disabled", "command": lambda: None},
                ],
            },
            {
                "label": "Snel",
                "items": [
                    {"type": "command", "label": "Actie zoeken…", "state": "normal", "command": lambda: None},
                    {"type": "command", "label": "Activiteiten", "state": "normal", "command": lambda: calls.append("activity")},
                ],
            },
        ]
        actions = collect_actions_from_specs(specs)
        self.assertEqual([action.path for action in actions], ["Bestand › Exporteren", "Snel › Activiteiten"])
        actions[0].command()
        self.assertEqual(calls, ["export"])

    def test_activity_history_deduplicates_and_stays_bounded(self) -> None:
        history = []
        self.assertTrue(append_activity(history, "Start"))
        self.assertFalse(append_activity(history, "Start"))
        for index in range(MAX_ACTIVITY_ITEMS + 15):
            append_activity(history, f"Melding {index}")
        self.assertEqual(len(history), MAX_ACTIVITY_ITEMS)
        self.assertEqual(history[-1][1], f"Melding {MAX_ACTIVITY_ITEMS + 14}")


class WorkflowUsabilityInstallerTests(unittest.TestCase):
    def test_installer_wraps_status_and_registers_shortcuts_without_gui(self) -> None:
        class Viewer:
            def __init__(self):
                self.original_statuses = []
                self.bindings = {}

            def _build_menu(self):
                return None

            def set_status(self, message):
                self.original_statuses.append(message)

            def bind_all(self, pattern, callback, add=None):
                self.bindings[pattern] = (callback, add)

            def after_idle(self, callback):
                # Menu discovery is allowed to fail on this headless fake.
                try:
                    callback()
                except Exception:
                    pass

            def cget(self, _name):
                raise RuntimeError("no tk menu in headless fake")

        install_workflow_usability_patch(Viewer)
        viewer = Viewer()
        viewer.set_status("Bezig...")
        viewer.set_status("Bezig...")
        viewer.set_status("Klaar.")

        self.assertEqual(viewer.original_statuses, ["Bezig...", "Bezig...", "Klaar."])
        self.assertEqual([message for _time, message in viewer._activity_history()], ["Bezig...", "Klaar."])
        self.assertIn("<Control-k>", viewer.bindings)
        self.assertIn("<F1>", viewer.bindings)
        self.assertTrue(callable(viewer.show_command_palette))
        self.assertEqual(Viewer._sleufbase_workflow_usability_version, 1)

    def test_optional_shortcuts_only_show_when_feature_exists(self) -> None:
        class App:
            def open_settings_dialog(self):
                pass

            def export_cadastral_template_dxf(self):
                pass

        descriptions = [spec.description for spec in available_shortcuts(App())]
        self.assertIn("Instellingen openen", descriptions)
        self.assertIn("DXF-sjabloon exporteren", descriptions)
        self.assertNotIn("marXact-DXF importeren", descriptions)
        self.assertNotIn("KickTheMap Jobs openen", descriptions)


if __name__ == "__main__":
    unittest.main()
