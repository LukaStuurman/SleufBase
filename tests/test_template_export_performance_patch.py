from __future__ import annotations

from pathlib import Path
import threading
import time
import unittest
from unittest.mock import patch

from SleufBase.cadastral_export import CadastralDxfExporter, PreparedTemplateSlotAssets
from SleufBase.cadastral_wfs import CadastralLinework, CadastralTextLabel, CadastralWfsClient
from SleufBase.models import Bounds
from SleufBase.template_bgt_fetch_patch import _LocalBoundsWfsClient
from SleufBase import template_export_performance_patch as perf_patch


class _ConcurrencyTracker:
    def __init__(self) -> None:
        self.lock = threading.Lock()
        self.active = 0
        self.max_active = 0

    def enter(self) -> None:
        with self.lock:
            self.active += 1
            self.max_active = max(self.max_active, self.active)

    def leave(self) -> None:
        with self.lock:
            self.active -= 1


class _ParallelWfsDelegate:
    def __init__(self) -> None:
        self.parcel_tracker = _ConcurrencyTracker()
        self.text_tracker = _ConcurrencyTracker()

    def fetch_parcel_boundaries(self, bounds: Bounds):
        self.parcel_tracker.enter()
        try:
            time.sleep(0.035)
            return CadastralLinework(
                layer_name="KAD_GRENS",
                paths=[[(bounds.min_x, bounds.min_y), (bounds.max_x, bounds.max_y)]],
            )
        finally:
            self.parcel_tracker.leave()

    def fetch_text_labels(self, bounds: Bounds):
        self.text_tracker.enter()
        try:
            time.sleep(0.035)
            return [
                CadastralTextLabel(
                    layer_name="KAD_STRAATNAAM",
                    text=f"{bounds.min_x:.0f}",
                    position=(bounds.center_x, bounds.center_y),
                    rotation=0.0,
                )
            ]
        finally:
            self.text_tracker.leave()


class _ParallelPathClient:
    def __init__(self) -> None:
        self.tracker = _ConcurrencyTracker()

    def fetch_paths(self, bounds: Bounds):
        self.tracker.enter()
        try:
            time.sleep(0.035)
            return [[(bounds.min_x, bounds.min_y), (bounds.max_x, bounds.max_y)]]
        finally:
            self.tracker.leave()


class _FakeResponse:
    def raise_for_status(self) -> None:
        return

    def json(self):
        return {"features": []}


class _FakeSession:
    def __init__(self) -> None:
        self.headers = {"User-Agent": "fake"}
        self.calls = 0
        self.closed = False

    def get(self, *args, **kwargs):
        self.calls += 1
        return _FakeResponse()

    def close(self) -> None:
        self.closed = True


class TemplateExportPerformancePatchTests(unittest.TestCase):
    @staticmethod
    def _bounds_list() -> list[Bounds]:
        return [
            Bounds(0.0, 0.0, 10.0, 10.0),
            Bounds(20.0, 0.0, 30.0, 10.0),
            Bounds(40.0, 0.0, 50.0, 10.0),
            Bounds(60.0, 0.0, 70.0, 10.0),
        ]

    def test_patch_is_installed(self) -> None:
        self.assertGreaterEqual(
            int(getattr(CadastralDxfExporter, "_sleufbase_template_export_performance_version", 0) or 0),
            1,
        )
        self.assertGreaterEqual(
            int(getattr(CadastralWfsClient, "_sleufbase_parallel_session_version", 0) or 0),
            1,
        )
        self.assertGreaterEqual(perf_patch.MAX_LOCAL_WFS_WORKERS, 2)
        self.assertGreaterEqual(perf_patch.MAX_VIRTUAL_TEMPLATE_MAP_WORKERS, 2)

    def test_local_wfs_bounds_are_fetched_concurrently_and_keep_order(self) -> None:
        delegate = _ParallelWfsDelegate()
        proxy = _LocalBoundsWfsClient(delegate, self._bounds_list())

        parcel = proxy.fetch_parcel_boundaries(Bounds(0.0, 0.0, 70.0, 10.0))
        labels = proxy.fetch_text_labels(Bounds(0.0, 0.0, 70.0, 10.0))

        self.assertIsNotNone(parcel)
        self.assertEqual(len(parcel.paths), 4)
        self.assertEqual([label.text for label in labels], ["0", "20", "40", "60"])
        self.assertGreaterEqual(delegate.parcel_tracker.max_active, 2)
        self.assertGreaterEqual(delegate.text_tracker.max_active, 2)

    def test_orientation_path_bounds_are_fetched_concurrently(self) -> None:
        client = _ParallelPathClient()
        exporter = CadastralDxfExporter(wfs_client=object())
        paths, empty_bounds = exporter._fetch_template_paths_for_bounds_with_empty(
            client,
            self._bounds_list(),
            status_label="test",
        )

        self.assertEqual(len(paths), 4)
        self.assertEqual(empty_bounds, [])
        self.assertGreaterEqual(client.tracker.max_active, 2)

    def test_main_thread_wfs_keeps_explicit_session_compatibility(self) -> None:
        client = CadastralWfsClient(retries=1)
        fake_session = _FakeSession()
        original_session = client.session
        client.session = fake_session
        try:
            payload = client._get_json({"request": "GetFeature"})
            self.assertEqual(payload, {"features": []})
            self.assertEqual(fake_session.calls, 1)
        finally:
            try:
                original_session.close()
            except Exception:
                pass
            client.close()
        self.assertTrue(fake_session.closed)

    def test_virtual_maps_parallel_but_high_res_tiffs_remain_single_worker(self) -> None:
        exporter = CadastralDxfExporter(wfs_client=object())
        map_tracker = _ConcurrencyTracker()
        tiff_tracker = _ConcurrencyTracker()

        def build_map(asset_dir, layer, label, index, **kwargs):
            map_tracker.enter()
            try:
                time.sleep(0.04)
                return Path(asset_dir) / f"{label}_{index}_map.png"
            finally:
                map_tracker.leave()

        def build_tiff(asset_dir, layer, label, index, *args, **kwargs):
            tiff_tracker.enter()
            try:
                time.sleep(0.025)
                return Path(asset_dir) / f"{label}_{index}_tiff.png"
            finally:
                tiff_tracker.leave()

        tasks = []
        for index in range(4):
            tasks.append(
                (
                    index,
                    {
                        "asset_dir": Path("assets"),
                        "layer": object(),
                        "label": f"PS{index + 1}",
                        "index": index + 1,
                        "road_centerline_paths": [],
                        "terrain_boundary_paths": [],
                        "profile": None,
                        "trench_mode": "polygon",
                        "centerline_color": (0, 0, 0),
                        "label_color": (0, 0, 0),
                        "page_exporter": None,
                        "dxf_overlays": [],
                        "map_comments": None,
                        "background_provider": None,
                        "background_attribution": None,
                        "location_client": None,
                        "reference_annotation": None,
                        "reverse_tiff_orientation": False,
                    },
                )
            )

        with (
            patch.object(perf_patch, "_contains_virtual_template_task", return_value=True),
            patch.object(exporter, "_clone_template_location_client", return_value=None),
            patch.object(exporter, "_clone_template_background_provider", return_value=None),
            patch.object(exporter, "_clone_template_page_exporter", return_value=None),
            patch.object(exporter, "_reverse_geocoded_template_address", return_value="Adres"),
            patch.object(exporter, "_template_comments_text", return_value="Opmerkingen"),
            patch.object(exporter, "_build_template_map_raster", side_effect=build_map),
            patch.object(exporter, "_build_template_tiff_raster", side_effect=build_tiff),
        ):
            result = exporter._prepare_template_slot_assets_batch(tasks)

        self.assertEqual(set(result), {0, 1, 2, 3})
        self.assertTrue(all(isinstance(item, PreparedTemplateSlotAssets) for item in result.values()))
        self.assertGreaterEqual(map_tracker.max_active, 2)
        self.assertEqual(tiff_tracker.max_active, 1)


if __name__ == "__main__":
    unittest.main()
