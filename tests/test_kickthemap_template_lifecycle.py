from __future__ import annotations

from pathlib import Path
import threading
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from SleufBase.kickthemap_scalability_patch import (
    TemplateKickTheMapSnapshot,
    _patch_viewer_class,
)


_THREAD = threading.Thread
_MODULE = "SleufBase.kickthemap_scalability_patch"


class _TemplateApp:
    def __init__(self):
        self.owner_thread = threading.get_ident()
        self.alive = True
        self.pending = {}
        self.bindings = {}
        self.statuses = []
        self.exports = []
        self.layer = SimpleNamespace(metadata={"kickthemap_job_id": 1})
        self.tiff_layers = [self.layer]
        self.kickthemap_client = SimpleNamespace(is_logged_in=True)

    def _assert_ui_thread(self):
        if threading.get_ident() != self.owner_thread:
            raise AssertionError("worker touched Tk")
        if not self.alive:
            raise AssertionError("callback touched a closed window")

    def winfo_exists(self):
        if threading.get_ident() != self.owner_thread:
            raise AssertionError("worker touched Tk")
        return self.alive

    def after(self, delay, callback):
        self._assert_ui_thread()
        identifier = f"poll-{len(self.pending)}"
        self.pending[identifier] = callback
        return identifier

    def after_cancel(self, identifier):
        if threading.get_ident() != self.owner_thread:
            raise AssertionError("worker cancelled a Tk callback")
        self.pending.pop(identifier, None)

    def bind(self, event, callback, *, add):
        self._assert_ui_thread()
        self.bindings["destroy"] = callback
        return "destroy"

    def unbind(self, event, identifier):
        if threading.get_ident() != self.owner_thread:
            raise AssertionError("worker unbound a Tk event")
        self.bindings.pop(identifier, None)

    def close(self):
        self.alive = False
        for callback in tuple(self.bindings.values()):
            callback(SimpleNamespace(widget=self))

    def poll(self):
        for identifier, callback in tuple(self.pending.items()):
            self.pending.pop(identifier, None)
            callback()

    def set_status(self, status):
        self._assert_ui_thread()
        self.statuses.append(status)

    def update_idletasks(self):
        self._assert_ui_thread()

    def export_cadastral_template_dxf(self, *args, **kwargs):
        self._assert_ui_thread()
        self.exports.append((args, kwargs))

    @staticmethod
    def _is_virtual_trench_layer(layer):
        return False

    @staticmethod
    def _kickthemap_job_id_for_layer(layer):
        return layer.metadata["kickthemap_job_id"]

    @staticmethod
    def _kickthemap_tiff_metadata(job, path):
        return {"fresh_path": str(path)}


def _app():
    viewer_class = type("Viewer", (_TemplateApp,), {})
    _patch_viewer_class(viewer_class)
    return viewer_class()


def _snapshot(app):
    return TemplateKickTheMapSnapshot(
        jobs_by_id={1: SimpleNamespace(job_id=1)},
        layers_by_job={1: (app.layer,)},
        paths={1: Path("fresh.json")},
        errors={},
        missing_job_ids=(),
    )


class TemplateRefreshLifecycleTests(unittest.TestCase):
    def _threads(self):
        threads = []

        def create_thread(**kwargs):
            thread = _THREAD(**kwargs)
            threads.append(thread)
            return thread

        return threads, patch(f"{_MODULE}.threading.Thread", side_effect=create_thread)

    def _join(self, threads):
        for thread in threads:
            thread.join(timeout=2)
            self.assertFalse(thread.is_alive(), "template worker did not finish")

    def test_fresh_data_is_applied_and_exported_only_by_ui_poll(self):
        app = _app()
        snapshot = _snapshot(app)
        threads, thread_patch = self._threads()

        def refresh(*args, progress_callback, **kwargs):
            for completed in range(1000):
                progress_callback(completed, 1000)
            return snapshot

        with thread_patch, patch(f"{_MODULE}._refresh_template_kickthemap_snapshot", side_effect=refresh):
            app.export_cadastral_template_dxf("output", scale=500)
            self._join(threads)

        self.assertEqual(app.exports, [])
        self.assertNotIn("fresh_path", app.layer.metadata)
        self.assertTrue(app._sleufbase_kickthemap_template_refresh_active)
        app.poll()

        self.assertEqual(app.exports, [(("output",), {"scale": 500})])
        self.assertEqual(app.layer.metadata["fresh_path"], "fresh.json")
        self.assertFalse(app._sleufbase_kickthemap_template_refresh_active)
        self.assertEqual(app.pending, {})
        self.assertEqual(app.bindings, {})
        self.assertIn("Nieuwste KickTheMap kabels/leidingen ophalen... 999/1000", app.statuses)
        self.assertEqual(len(app.statuses), 3)

    def test_close_during_refresh_discards_late_success_and_error(self):
        for failure in (None, RuntimeError("offline")):
            with self.subTest(failure=failure):
                app = _app()
                started, release = threading.Event(), threading.Event()
                threads, thread_patch = self._threads()

                def refresh(*args, progress_callback, **kwargs):
                    started.set()
                    if not release.wait(timeout=2):
                        raise AssertionError("test never released worker")
                    progress_callback(1, 1)
                    if failure is not None:
                        raise failure
                    return _snapshot(app)

                with (
                    thread_patch,
                    patch(f"{_MODULE}._refresh_template_kickthemap_snapshot", side_effect=refresh),
                    patch(f"{_MODULE}.messagebox.showerror") as showerror,
                ):
                    app.export_cadastral_template_dxf()
                    self.assertTrue(started.wait(timeout=2))
                    app.close()
                    release.set()
                    self._join(threads)
                    app.poll()

                self.assertEqual(app.exports, [])
                self.assertNotIn("fresh_path", app.layer.metadata)
                self.assertEqual(app.pending, {})
                self.assertEqual(app.bindings, {})
                self.assertFalse(app._sleufbase_kickthemap_template_refresh_active)
                showerror.assert_not_called()

    def test_refresh_error_is_shown_by_ui_poll_and_releases_guard(self):
        app = _app()
        threads, thread_patch = self._threads()

        def showerror(*args, **kwargs):
            app._assert_ui_thread()

        with (
            thread_patch,
            patch(f"{_MODULE}._refresh_template_kickthemap_snapshot", side_effect=RuntimeError("offline")),
            patch(f"{_MODULE}.messagebox.showerror", side_effect=showerror) as messagebox,
        ):
            app.export_cadastral_template_dxf()
            self._join(threads)
            messagebox.assert_not_called()
            app.poll()
            messagebox.assert_called_once()

        self.assertEqual(app.exports, [])
        self.assertFalse(app._sleufbase_kickthemap_template_refresh_active)
        self.assertEqual(app.pending, {})
        self.assertEqual(app.bindings, {})

    def test_child_widget_destruction_keeps_refresh_active(self):
        app = _app()
        threads, thread_patch = self._threads()
        with thread_patch, patch(f"{_MODULE}._refresh_template_kickthemap_snapshot", return_value=_snapshot(app)):
            app.export_cadastral_template_dxf()
            self._join(threads)

        for callback in tuple(app.bindings.values()):
            callback(SimpleNamespace(widget=object()))
        self.assertTrue(app._sleufbase_kickthemap_template_refresh_active)
        app.poll()
        self.assertEqual(len(app.exports), 1)

    def test_changed_loaded_trenches_require_new_refresh_before_export(self):
        def add_layer(app):
            app.tiff_layers.append(SimpleNamespace(metadata={"kickthemap_job_id": 2}))

        def replace_layer(app):
            app.tiff_layers = [SimpleNamespace(metadata={"kickthemap_job_id": 1})]

        def change_job(app):
            app.layer.metadata["kickthemap_job_id"] = 2

        for change in (add_layer, replace_layer, change_job, lambda app: app.tiff_layers.clear()):
            with self.subTest(change=change):
                app = _app()
                snapshot = _snapshot(app)
                threads, thread_patch = self._threads()
                with (
                    thread_patch,
                    patch(f"{_MODULE}._refresh_template_kickthemap_snapshot", return_value=snapshot),
                    patch(f"{_MODULE}.messagebox.showerror") as showerror,
                ):
                    app.export_cadastral_template_dxf()
                    self._join(threads)
                    change(app)
                    app.poll()

                showerror.assert_called_once()
                self.assertIn("Start de DXF-sjabloonexport opnieuw", showerror.call_args.args[1])
                self.assertEqual(app.exports, [])
                self.assertNotIn("fresh_path", app.layer.metadata)
                self.assertFalse(app._sleufbase_kickthemap_template_refresh_active)
                self.assertEqual(app.pending, {})

    def test_reordering_same_layers_preserves_fresh_snapshot(self):
        app = _app()
        second = SimpleNamespace(metadata={"kickthemap_job_id": 1})
        app.tiff_layers.append(second)
        snapshot = _snapshot(app)
        snapshot = TemplateKickTheMapSnapshot(
            jobs_by_id=snapshot.jobs_by_id,
            layers_by_job={1: (app.layer, second)},
            paths=snapshot.paths,
            errors={},
            missing_job_ids=(),
        )
        threads, thread_patch = self._threads()
        with thread_patch, patch(f"{_MODULE}._refresh_template_kickthemap_snapshot", return_value=snapshot):
            app.export_cadastral_template_dxf()
            self._join(threads)

        app.tiff_layers.reverse()
        app.poll()

        self.assertEqual(len(app.exports), 1)
        self.assertEqual(second.metadata["fresh_path"], "fresh.json")
        self.assertFalse(app._sleufbase_kickthemap_template_refresh_active)


if __name__ == "__main__":
    unittest.main()
