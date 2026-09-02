from __future__ import annotations

from pathlib import Path
from tempfile import TemporaryDirectory
import unittest

from PIL import Image

from SleufBase.cadastral_export import CadastralDxfExporter
from SleufBase.marxact_import import MarXactObject, MarXactTrench, build_marxact_virtual_layer
from SleufBase.template_asset_memory_patch import (
    MAX_VIRTUAL_TEMPLATE_ASSET_WORKERS,
    SAFE_VIRTUAL_TRENCH_EXPORT_QUALITY_MULTIPLIER,
    TEMPLATE_UI_PUMP_INTERVAL_SECONDS,
    VIRTUAL_TEMPLATE_PNG_COMPRESS_LEVEL,
    VIRTUAL_TEMPLATE_ROTATION_RESAMPLE,
    _contains_virtual_template_task,
    _pump_template_ui,
)
from SleufBase.virtual_trench import build_virtual_trench_render


class _FakeTkOwner:
    def __init__(self) -> None:
        self.idle_updates = 0
        self.full_updates = 0

    def status(self, _message: str) -> None:
        pass

    def update_idletasks(self) -> None:
        self.idle_updates += 1

    def update(self) -> None:
        self.full_updates += 1


class TemplateAssetMemoryPatchTests(unittest.TestCase):
    @staticmethod
    def _virtual_layer():
        trench = MarXactTrench(
            name="ps-memory",
            polygon=(
                (0.0, 0.0, 10.0),
                (8.0, 0.0, 10.2),
                (8.0, 2.0, 10.3),
                (0.0, 2.0, 10.1),
            ),
            objects=[
                MarXactObject(2.0, 1.0, 9.5, "Water", "water", 9.5, "marxact_point"),
                MarXactObject(6.0, 1.0, 9.4, "Laagspanning", "ls", 9.4, "marxact_point"),
            ],
        )
        return build_marxact_virtual_layer(
            trench,
            source_path="memory-test.dxf",
            source_name_resolver=lambda item: item.mapping_name,
            fallback_index=1,
        )

    def test_runtime_patch_uses_high_quality_and_is_installed(self) -> None:
        self.assertGreaterEqual(
            int(getattr(CadastralDxfExporter, "_sleufbase_template_asset_memory_patch_version", 0) or 0),
            3,
        )
        self.assertAlmostEqual(
            CadastralDxfExporter.VIRTUAL_TRENCH_EXPORT_QUALITY_MULTIPLIER,
            SAFE_VIRTUAL_TRENCH_EXPORT_QUALITY_MULTIPLIER,
        )
        self.assertAlmostEqual(SAFE_VIRTUAL_TRENCH_EXPORT_QUALITY_MULTIPLIER, 2.5)
        self.assertEqual(VIRTUAL_TEMPLATE_ROTATION_RESAMPLE, Image.Resampling.BICUBIC)
        self.assertEqual(MAX_VIRTUAL_TEMPLATE_ASSET_WORKERS, 1)
        self.assertLessEqual(VIRTUAL_TEMPLATE_PNG_COMPRESS_LEVEL, 1)
        self.assertLessEqual(TEMPLATE_UI_PUMP_INTERVAL_SECONDS, 0.1)

    def test_virtual_template_tasks_are_detected_for_sequential_rendering(self) -> None:
        layer = self._virtual_layer()
        tasks = [(0, {"layer": layer})]
        self.assertTrue(_contains_virtual_template_task(tasks))

    def test_ui_can_be_pumped_while_virtual_worker_is_running(self) -> None:
        owner = _FakeTkOwner()
        self.assertTrue(_pump_template_ui(owner.status))
        self.assertEqual(owner.idle_updates, 1)
        self.assertEqual(owner.full_updates, 1)

    def test_virtual_export_layer_is_about_twice_previous_125x_resolution(self) -> None:
        layer = self._virtual_layer()
        exporter = CadastralDxfExporter(wfs_client=object())
        baseline, _bounds, _transform = build_virtual_trench_render(
            layer,
            quality_multiplier=1.25,
        )
        prepared = exporter._prepared_virtual_trench_export_layer(layer)
        try:
            baseline_long_side = max(baseline.size)
            high_res_long_side = max(prepared.image.size)
            self.assertGreaterEqual(high_res_long_side, int(baseline_long_side * 1.9))
            self.assertLessEqual(high_res_long_side, 4000)
        finally:
            baseline.close()
            if prepared.image is not layer.image:
                prepared.image.close()

    def test_virtual_tiff_export_skips_expensive_alpha_normalizer_and_stays_bounded(self) -> None:
        layer = self._virtual_layer()
        exporter = CadastralDxfExporter(wfs_client=object())

        def fail_if_called(_image):
            raise AssertionError("virtuele proefsleuf mag alpha-normalizer niet gebruiken")

        exporter._normalize_template_tiff_raster_alpha = fail_if_called
        with TemporaryDirectory() as temp_dir:
            output = exporter._build_template_tiff_raster(
                Path(temp_dir),
                layer,
                "PS-MEMORY",
                1,
                [],
                [],
            )
            self.assertTrue(output.exists())
            with Image.open(output) as rendered:
                self.assertEqual(rendered.mode, "RGBA")
                # 2.5x caps the source raster at 4000 px. Rotation can grow the
                # diagonal, but one cropped raster at a time keeps memory bounded.
                self.assertLessEqual(max(rendered.size), 5700)
                self.assertGreater(rendered.width, 1)
                self.assertGreater(rendered.height, 1)

    def test_normal_alpha_cleanup_keeps_foreground_and_transparent_edge(self) -> None:
        exporter = CadastralDxfExporter(wfs_client=object())
        image = Image.new("RGBA", (32, 32), (255, 255, 255, 255))
        for x in range(8, 24):
            for y in range(8, 24):
                image.putpixel((x, y), (20, 30, 40, 255))

        normalized = exporter._normalize_template_tiff_raster_alpha(image)
        self.assertEqual(normalized.getpixel((0, 0))[3], 0)
        self.assertEqual(normalized.getpixel((16, 16))[3], 255)


if __name__ == "__main__":
    unittest.main()
