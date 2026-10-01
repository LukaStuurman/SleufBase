from __future__ import annotations

from dataclasses import replace
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest
from unittest import mock

from PIL import Image

from SleufBase.cadastral_export import CadastralDxfExporter, CadastralExportError
from SleufBase.models import Bounds, GeoTiffLayer, GeoTransform
from SleufBase import template_asset_memory_patch as assets
from SleufBase.virtual_trench import VIRTUAL_TRENCH_METADATA_KEY


class TemplateRasterLifecycleTests(unittest.TestCase):
    def setUp(self) -> None:
        self.live_image = Image.new("RGBA", (4, 4), (10, 20, 30, 255))
        self.addCleanup(self.live_image.close)
        self.layer = GeoTiffLayer(
            path=Path("virtual.tif"),
            image=self.live_image,
            transform=GeoTransform(1, 0, 0, 0, -1, 4),
            bounds=Bounds(0, 0, 4, 4),
            epsg=28992,
            opacity=1.0,
            metadata={VIRTUAL_TRENCH_METADATA_KEY: {"points": []}},
        )
        self.exporter = CadastralDxfExporter(wfs_client=object())

    def _prepare(self, image: Image.Image) -> None:
        self.addCleanup(image.close)
        prepared = replace(self.layer, image=image)
        patcher = mock.patch.object(self.exporter, "_prepared_virtual_trench_export_layer", return_value=prepared)
        patcher.start()
        self.addCleanup(patcher.stop)

    def _build(self, root: Path) -> Path:
        return self.exporter._build_template_tiff_raster(root, self.layer, "PS1", 1, [], [])

    def _assert_closed(self, image: Image.Image) -> None:
        with self.assertRaises(ValueError):
            image.getpixel((0, 0))
        self.assertEqual(self.live_image.getpixel((0, 0)), (10, 20, 30, 255))

    def test_fresh_rgba_virtual_raster_transfers_ownership_without_copy(self) -> None:
        fresh = Image.new("RGBA", (64, 64), (40, 50, 60, 255))
        self._prepare(fresh)
        with TemporaryDirectory() as directory, mock.patch.object(
            self.exporter, "_template_tiff_orientation_pixel_vector", return_value=None
        ), mock.patch.object(fresh, "convert", side_effect=AssertionError("fresh RGBA raster must not be copied")):
            path = self._build(Path(directory))
            with Image.open(path) as output:
                self.assertEqual(output.size, (64, 64))
                self.assertEqual(output.getpixel((0, 0)), (40, 50, 60, 255))
        self._assert_closed(fresh)

    def test_orientation_failure_closes_fresh_source_and_preserves_live_image(self) -> None:
        fresh = Image.new("RGBA", (64, 64))
        self._prepare(fresh)
        with TemporaryDirectory() as directory, mock.patch.object(
            self.exporter, "_template_tiff_orientation_pixel_vector", side_effect=ValueError("invalid orientation")
        ):
            with self.assertRaisesRegex(ValueError, "invalid orientation"):
                self._build(Path(directory))
        self._assert_closed(fresh)

    def test_conversion_failure_closes_owned_non_rgba_source(self) -> None:
        fresh = Image.new("RGB", (64, 64))
        self._prepare(fresh)
        with TemporaryDirectory() as directory, mock.patch.object(
            self.exporter, "_template_tiff_orientation_pixel_vector", return_value=None
        ), mock.patch.object(fresh, "convert", side_effect=ValueError("conversion failed")):
            with self.assertRaisesRegex(ValueError, "conversion failed"):
                self._build(Path(directory))
        self._assert_closed(fresh)

    def test_non_rgba_virtual_source_is_converted_and_saved(self) -> None:
        fresh = Image.new("RGB", (64, 64), (40, 50, 60))
        self._prepare(fresh)
        with TemporaryDirectory() as directory, mock.patch.object(
            self.exporter, "_template_tiff_orientation_pixel_vector", return_value=None
        ):
            path = self._build(Path(directory))
            with Image.open(path) as output:
                self.assertEqual(output.mode, "RGBA")
                self.assertEqual(output.getpixel((0, 0)), (40, 50, 60, 255))
        self._assert_closed(fresh)

    def test_save_failure_closes_source_and_keeps_export_error(self) -> None:
        fresh = Image.new("RGBA", (64, 64))
        self._prepare(fresh)
        with TemporaryDirectory() as directory, mock.patch.object(
            self.exporter, "_template_tiff_orientation_pixel_vector", return_value=None
        ), mock.patch.object(assets, "save_template_png", side_effect=OSError("disk full")):
            with self.assertRaisesRegex(CadastralExportError, "disk full"):
                self._build(Path(directory))
        self._assert_closed(fresh)


if __name__ == "__main__":
    unittest.main()
