from __future__ import annotations

import threading
import unittest

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
        from SleufBase import kickthemap_jobs_browser as jobs_module

        window_class = jobs_module.KickTheMapJobsWindow
        map_class = jobs_module.KickTheMapJobsMapView
        original_prelogin = window_class._prelogin_kickthemap_browser
        original_window_destroy = window_class.destroy
        original_map_init = map_class.__init__
        original_map_worker = map_class._load_map_worker
        original_map_destroy = map_class.destroy
        old_window_version = getattr(window_class, "_sleufbase_jobs_memory_patch_version", None)
        old_map_version = getattr(map_class, "_sleufbase_jobs_memory_patch_version", None)

        try:
            jobs_memory_patch.install_jobs_memory_patch()

            class DummyWindow:
                _browser_prelogin_started = False

            dummy = DummyWindow()
            window_class._prelogin_kickthemap_browser(dummy)
            self.assertTrue(dummy._browser_prelogin_started)
            self.assertEqual(
                window_class._sleufbase_jobs_memory_patch_version,
                jobs_memory_patch.PATCH_VERSION,
            )
        finally:
            window_class._prelogin_kickthemap_browser = original_prelogin
            window_class.destroy = original_window_destroy
            map_class.__init__ = original_map_init
            map_class._load_map_worker = original_map_worker
            map_class.destroy = original_map_destroy
            if old_window_version is None:
                try:
                    delattr(window_class, "_sleufbase_jobs_memory_patch_version")
                except AttributeError:
                    pass
            else:
                window_class._sleufbase_jobs_memory_patch_version = old_window_version
            if old_map_version is None:
                try:
                    delattr(map_class, "_sleufbase_jobs_memory_patch_version")
                except AttributeError:
                    pass
            else:
                map_class._sleufbase_jobs_memory_patch_version = old_map_version

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
