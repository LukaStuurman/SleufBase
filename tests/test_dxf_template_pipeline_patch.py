from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
import tempfile
import threading
import time
import unittest
from unittest.mock import patch

from SleufBase import dxf_template_pipeline_patch as pipeline
from SleufBase import template_reverse_patch as reverse_patch


class _ConcurrencyTracker:
    def __init__(self) -> None:
        self._lock = threading.Lock()
        self.active = 0
        self.max_active = 0

    def enter(self) -> None:
        with self._lock:
            self.active += 1
            self.max_active = max(self.max_active, self.active)

    def leave(self) -> None:
        with self._lock:
            self.active -= 1


@dataclass(frozen=True)
class _Prepared:
    formatted_address: str | None
    comments_text: str
    raster_path: Path
    map_raster_path: Path


class _Image:
    def __init__(self, size=(500, 500)) -> None:
        self.size = size


class _Layer:
    def __init__(self, size=(500, 500)) -> None:
        self.image = _Image(size)


class _FakeExporter:
    VIRTUAL_TRENCH_EXPORT_QUALITY_MULTIPLIER = 2.5

    def __init__(self) -> None:
        self.map_tracker = _ConcurrencyTracker()
        self.tiff_tracker = _ConcurrencyTracker()
        self.all_tracker = _ConcurrencyTracker()

    def _prepare_template_slot_assets_batch(self, tasks, *, status_callback=None):
        raise AssertionError("fallback should not be used")

    def _clone_template_location_client(self, value):
        return value

    def _clone_template_background_provider(self, value):
        return value

    def _clone_template_page_exporter(self, value, background_provider):
        return value

    def _reverse_geocoded_template_address(self, layer, client):
        return "Adres"

    def _template_comments_text(self, layer, comments):
        return "Opmerkingen"

    def _build_template_map_raster(self, asset_dir, layer, label, index, **kwargs):
        self.map_tracker.enter()
        self.all_tracker.enter()
        try:
            time.sleep(0.04)
            return Path(asset_dir) / f"{label}_{index}_map.png"
        finally:
            self.all_tracker.leave()
            self.map_tracker.leave()

    def _build_template_tiff_raster(
        self,
        asset_dir,
        layer,
        label,
        index,
        road_paths,
        terrain_paths,
        *,
        profile=None,
        reverse_orientation=False,
    ):
        self.tiff_tracker.enter()
        self.all_tracker.enter()
        try:
            time.sleep(0.04)
            return Path(asset_dir) / f"{label}_{index}_tiff.png"
        finally:
            self.all_tracker.leave()
            self.tiff_tracker.leave()


class _SlotCacheExporter:
    calls = 0

    def _detect_template_slots(self, document):
        type(self).calls += 1
        return ["slot-a", "slot-b"]


class _Document:
    def __init__(self, filename: str) -> None:
        self.filename = filename


class DxfTemplatePipelinePatchTests(unittest.TestCase):
    def test_streaming_pair_io_preserves_bom_and_crlf(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            path = Path(temporary_directory) / "sample.dxf"
            path.write_bytes(
                b"\xef\xbb\xbf  0\r\nSECTION\r\n  2\r\nHEADER\r\n  0\r\nENDSEC\r\n  0\r\nEOF\r\n"
            )
            pairs, newline, had_bom = pipeline._read_pairs_streaming(path)
            self.assertTrue(had_bom)
            self.assertEqual(newline, "\r\n")
            self.assertEqual(pairs[0], (0, "SECTION"))
            pipeline._write_pairs_streaming(
                path,
                pairs,
                newline=newline,
                had_bom=had_bom,
            )
            written = path.read_bytes()
            self.assertTrue(written.startswith(b"\xef\xbb\xbf"))
            self.assertIn(b"\r\n", written)
            self.assertEqual(pipeline._read_pairs_streaming(path)[0], pairs)

    def test_pixel_budget_serializes_large_tasks_but_allows_small_overlap(self) -> None:
        def run_pair(weight: int) -> int:
            budget = pipeline._PixelBudget(10)
            tracker = _ConcurrencyTracker()
            barrier = threading.Barrier(3)

            def worker():
                barrier.wait()
                acquired = budget.acquire(weight)
                tracker.enter()
                try:
                    time.sleep(0.03)
                finally:
                    tracker.leave()
                    budget.release(acquired)

            threads = [threading.Thread(target=worker) for _ in range(2)]
            for thread in threads:
                thread.start()
            barrier.wait()
            for thread in threads:
                thread.join()
            return tracker.max_active

        self.assertEqual(run_pair(6), 1)
        self.assertEqual(run_pair(4), 2)

    def test_record_inspection_handles_multiple_dynamic_blocks_in_one_context(self) -> None:
        records = [
            [(0, "BLOCK_RECORD"), (5, "B1"), (2, "WRAP1"), (102, "{ACAD_XDICTIONARY"), (360, "X1"), (102, "}"), (1001, "AcDbBlockRepETag"), (1070, "1")],
            [(0, "BLOCK_RECORD"), (5, "B2"), (2, "WRAP2"), (102, "{ACAD_XDICTIONARY"), (360, "X2"), (102, "}"), (1001, "AcDbBlockRepETag"), (1070, "1")],
            [(0, "DICTIONARY"), (5, "X1"), (330, "B1")],
            [(0, "BLOCKVISIBILITYPARAMETER"), (5, "P1"), (330, "X1"), (301, "Versie"), (303, "Normaal"), (332, "N1"), (303, "Reverse"), (332, "R1")],
            [(0, "DICTIONARY"), (5, "X2"), (330, "B2")],
            [(0, "BLOCKVISIBILITYPARAMETER"), (5, "P2"), (330, "X2"), (301, "Versie"), (303, "Normaal"), (332, "N2"), (303, "Reverse"), (332, "R2")],
        ]
        sections = ["TABLES", "TABLES", "OBJECTS", "OBJECTS", "OBJECTS", "OBJECTS"]
        details = pipeline._inspect_records(records, sections, ["WRAP1", "WRAP2"])
        self.assertEqual(set(details), {"WRAP1", "WRAP2"})
        self.assertTrue(details["WRAP1"]["is_dynamic"])
        self.assertEqual(details["WRAP2"]["states"], ("Normaal", "Reverse"))

    def test_template_slot_detection_is_cached_by_template_file(self) -> None:
        pipeline._TEMPLATE_SLOT_CACHE.clear()
        _SlotCacheExporter.calls = 0
        pipeline._install_template_slot_cache(_SlotCacheExporter)
        exporter = _SlotCacheExporter()
        with tempfile.TemporaryDirectory() as temporary_directory:
            path = Path(temporary_directory) / "template.dxf"
            path.write_text("stub", encoding="utf-8")
            first = exporter._detect_template_slots(_Document(str(path)))
            second = exporter._detect_template_slots(_Document(str(path)))
        self.assertEqual(first, ["slot-a", "slot-b"])
        self.assertEqual(second, first)
        self.assertEqual(_SlotCacheExporter.calls, 1)

    def test_map_and_tiff_work_overlap_with_hardware_aware_tiff_parallelism(self) -> None:
        exporter = _FakeExporter()
        pipeline._install_overlapped_asset_pipeline(_FakeExporter, _Prepared)
        tasks = []
        for index in range(4):
            tasks.append(
                (
                    index,
                    {
                        "asset_dir": Path("assets"),
                        "layer": _Layer((400, 400)),
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
        with patch.object(pipeline, "_contains_virtual_template_task", return_value=True):
            result = exporter._prepare_template_slot_assets_batch(tasks)
        self.assertEqual(set(result), {0, 1, 2, 3})
        self.assertGreaterEqual(exporter.all_tracker.max_active, 2)
        self.assertGreaterEqual(exporter.map_tracker.max_active, 2)

        expected_tiff_cap = max(1, min(pipeline.MAX_ADAPTIVE_TIFF_WORKERS, len(tasks)))
        self.assertGreaterEqual(exporter.tiff_tracker.max_active, 1)
        self.assertLessEqual(exporter.tiff_tracker.max_active, expected_tiff_cap)
        if expected_tiff_cap >= 2:
            self.assertGreaterEqual(exporter.tiff_tracker.max_active, 2)
        else:
            self.assertEqual(exporter.tiff_tracker.max_active, 1)

    def test_pair_session_reuses_one_cache_and_publishes_metrics(self) -> None:
        class Exporter:
            pass

        exporter = Exporter()
        seen_cache_ids = []

        def fake_export(instance, **kwargs):
            cache = pipeline._pair_cache()
            self.assertIsNotNone(cache)
            seen_cache_ids.append(id(cache))
            return Path(kwargs["output_path"])

        with tempfile.TemporaryDirectory() as temporary_directory:
            normal_path = Path(temporary_directory) / "normal.dxf"
            reverse_path = Path(temporary_directory) / ".normal.reverse.dxf"
            arguments = {"output_path": normal_path, "reverse_cross_sections": False}
            reverse_patch._call_original_export(
                fake_export,
                exporter,
                arguments,
                mode=reverse_patch.NORMAL_MODE,
                output_path=normal_path,
                reverse_cross_sections=False,
            )
            reverse_patch._call_original_export(
                fake_export,
                exporter,
                arguments,
                mode=reverse_patch.REVERSE_MODE,
                output_path=reverse_path,
                reverse_cross_sections=True,
            )
            reverse_patch._cleanup_reverse_source(reverse_path)

        self.assertEqual(len(set(seen_cache_ids)), 1)
        self.assertIsNone(getattr(reverse_patch._EXPORT_CONTEXT, "pair_cache", None))
        metrics = pipeline.get_last_template_export_metrics(exporter)
        self.assertGreaterEqual(float(metrics.get("total_seconds", 0.0)), 0.0)
        self.assertIn("normal_export", metrics.get("phases", {}))
        self.assertIn("reverse_export", metrics.get("phases", {}))


if __name__ == "__main__":
    unittest.main()
