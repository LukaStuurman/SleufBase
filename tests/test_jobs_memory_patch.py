from __future__ import annotations

import sys
import threading
import types
import unittest

import SleufBase
from SleufBase import jobs_memory_patch


class _FakeSession:
    def __init__(self) -> None:
        self.closed = False

    def close(self) -> None:
        self.closed = True


class _FakeThreadLocal:
    pass


class _FakeTileClient:
    def __init__(self) -> None:
        self.memory_cache_limit = 768
        self._memory_cache = {("tile", index): object() for index in range(12)}
        self._memory_cache_lock = threading.RLock()
        self._thread_local = _FakeThreadLocal()
        self._thread_local.session = _FakeSession()


class _FakeMapView:
    def __init__(self) -> None:
        self._map_request_id = 4
        self._pending_map_result = object()
        self.current_image = object()
        self.photo_image = object()
        self._current_image_key = object()
        self._font_cache = object()
        self._drag_start = object()
        self._drag_last = object()
        self._selection_start = object()
        self._marker_positions = [1, 2]
        self.points = [1, 2, 3]
        self._job_label_cache = {1: ("a", "b")}
        self.tile_client = _FakeTileClient()


class JobsMemoryPatchTests(unittest.TestCase):
    def test_jobs_map_cache_is_bounded_far_below_legacy_default(self) -> None:
        client = _FakeTileClient()
        jobs_memory_patch._limit_tile_client_memory(client)
        self.assertEqual(client.memory_cache_limit, jobs_memory_patch.JOBS_MAP_TILE_CACHE_LIMIT)
        self.assertLessEqual(client.memory_cache_limit, 128)

    def test_releasing_map_view_drops_large_images_tiles_and_session(self) -> None:
        view = _FakeMapView()
        session = view.tile_client._thread_local.session

        jobs_memory_patch._release_map_view_memory(view)

        self.assertTrue(view._sleufbase_jobs_destroyed)
        self.assertEqual(view._map_request_id, 5)
        self.assertIsNone(view._pending_map_result)
        self.assertIsNone(view.current_image)
        self.assertIsNone(view.photo_image)
        self.assertEqual(view._marker_positions, [])
        self.assertEqual(view.points, [])
        self.assertEqual(view._job_label_cache, {})
        self.assertEqual(view.tile_client._memory_cache, {})
        self.assertTrue(session.closed)
        self.assertFalse(hasattr(view.tile_client._thread_local, "session"))

    def test_jobs_patch_disables_automatic_hidden_browser_prelogin(self) -> None:
        module_name = "SleufBase.kickthemap_jobs_browser"
        fake_module = types.ModuleType(module_name)

        class FakeMapView:
            def __init__(self, *args, **kwargs) -> None:
                self.tile_client = _FakeTileClient()
                self._map_request_id = 0
                self._pending_map_result = None

            def _load_map_worker(self, *args, **kwargs) -> None:
                return

            def destroy(self) -> None:
                return

        class FakeWindow:
            def __init__(self) -> None:
                self._browser_prelogin_started = False
                self.map_view = None

            def _prelogin_kickthemap_browser(self) -> None:
                raise AssertionError("legacy hidden prelogin should have been replaced")

            def destroy(self) -> None:
                return

        fake_module.KickTheMapJobsMapView = FakeMapView
        fake_module.KickTheMapJobsWindow = FakeWindow

        previous_module = sys.modules.get(module_name)
        had_package_attr = hasattr(SleufBase, "kickthemap_jobs_browser")
        previous_package_attr = getattr(SleufBase, "kickthemap_jobs_browser", None)
        sys.modules[module_name] = fake_module
        SleufBase.kickthemap_jobs_browser = fake_module
        try:
            jobs_memory_patch.install_jobs_memory_patch()
            dummy = FakeWindow()
            dummy._prelogin_kickthemap_browser()
            self.assertTrue(dummy._browser_prelogin_started)
            self.assertEqual(
                FakeWindow._sleufbase_jobs_memory_patch_version,
                jobs_memory_patch.PATCH_VERSION,
            )
        finally:
            if previous_module is None:
                sys.modules.pop(module_name, None)
            else:
                sys.modules[module_name] = previous_module
            if had_package_attr:
                SleufBase.kickthemap_jobs_browser = previous_package_attr
            else:
                try:
                    delattr(SleufBase, "kickthemap_jobs_browser")
                except AttributeError:
                    pass

    def test_launcher_guard_reuses_existing_jobs_process_instead_of_spawning_another(self) -> None:
        class RunningProcess:
            def poll(self):
                return None

        class Viewer:
            def __init__(self) -> None:
                self._sleufbase_jobs_browser_process = RunningProcess()
                self.status = ""
                self.launches = 0

            def open_kickthemap_jobs_browser_window(self) -> None:
                self.launches += 1

            def set_status(self, text: str) -> None:
                self.status = text

        jobs_memory_patch.install_jobs_launcher_guard(Viewer)
        viewer = Viewer()
        viewer.open_kickthemap_jobs_browser_window()

        self.assertEqual(viewer.launches, 0)
        self.assertEqual(viewer.status, "KickTheMap Jobs staat al open.")
        self.assertEqual(
            Viewer._sleufbase_jobs_launcher_guard_version,
            jobs_memory_patch.PATCH_VERSION,
        )


if __name__ == "__main__":
    unittest.main()
