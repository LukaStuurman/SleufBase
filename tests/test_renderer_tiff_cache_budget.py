from __future__ import annotations

from pathlib import Path
from tempfile import TemporaryDirectory
import unittest
from unittest import mock

from PIL import Image

from SleufBase.models import Bounds, GeoTiffLayer, GeoTransform, ViewportTransform
from SleufBase import renderer as renderer_module
from SleufBase.renderer import MapRenderer, _NativeTiffImageCache


class RendererTiffCacheBudgetTests(unittest.TestCase):
    @staticmethod
    def _layer(name: str, x: float = 0.0) -> GeoTiffLayer:
        image = Image.new("RGBA", (16, 16), (10, 20, 30, 255))
        return GeoTiffLayer(
            path=Path(name),
            image=image,
            transform=GeoTransform(1.0, 0.0, x, 0.0, -1.0, 16.0),
            bounds=Bounds(x, 0.0, x + 16.0, 16.0),
            epsg=28992,
            opacity=1.0,
        )

    def test_large_visible_batch_does_not_retain_full_rgba_copies(self) -> None:
        layers = [self._layer("first.tif"), self._layer("second.tif", 16.0)]
        renderer = MapRenderer()
        transform = ViewportTransform(Bounds(0.0, 0.0, 32.0, 16.0), 64, 32)
        try:
            for layer in layers:
                layer.native_rgba_cache = _NativeTiffImageCache(
                    key=(id(layer.image), 16, 16, "RGBA"),
                    rgba=renderer_module.np.zeros((16, 16, 4), dtype=renderer_module.np.uint8),
                )
            with mock.patch.object(renderer_module, "_NATIVE_TIFF_CACHE_PIXEL_BUDGET", 1), mock.patch.object(
                renderer_module.native_accel, "is_available", return_value=True
            ):
                jobs = renderer._prepared_native_tiff_jobs(transform, layers)

            self.assertIsNotNone(jobs)
            self.assertTrue(all(job[3] is False for job in jobs))
            self.assertTrue(all(layer.native_rgba_cache is None for layer in layers))
        finally:
            for layer in layers:
                layer.image.close()

    def test_bounded_mode_paints_sources_one_at_a_time_without_caching(self) -> None:
        layers = [self._layer("first.tif"), self._layer("second.tif", 16.0)]
        renderer = MapRenderer()
        canvas = Image.new("RGBA", (64, 32), (255, 255, 255, 255))
        try:
            with mock.patch.object(renderer_module, "_NATIVE_TIFF_CACHE_PIXEL_BUDGET", 1), mock.patch.object(
                renderer_module.native_accel, "is_available", return_value=True
            ), mock.patch.object(
                renderer_module.native_accel, "paint_axis_aligned_tiff", return_value=1
            ) as paint, mock.patch.object(
                renderer, "_native_tiff_rgba_cache", wraps=renderer._native_tiff_rgba_cache
            ) as source:
                result = renderer._paint_tiff_layers_native(
                    canvas,
                    ViewportTransform(Bounds(0.0, 0.0, 32.0, 16.0), 64, 32),
                    layers,
                )

            self.assertIsNotNone(result)
            self.assertEqual(paint.call_count, 2)
            self.assertEqual([call.kwargs["store"] for call in source.call_args_list], [False, False])
            self.assertTrue(all(layer.native_rgba_cache is None for layer in layers))
            if result is not None and result is not canvas:
                result.close()
        finally:
            canvas.close()
            for layer in layers:
                layer.image.close()

    def test_bounded_mode_reopens_file_backed_tiff_lazily_after_paint(self) -> None:
        with TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "layer.tiff"
            Image.new("RGBA", (16, 16), (10, 20, 30, 255)).save(path)
            layer = GeoTiffLayer(
                path=path,
                image=Image.open(path),
                transform=GeoTransform(1.0, 0.0, 0.0, 0.0, -1.0, 16.0),
                bounds=Bounds(0.0, 0.0, 16.0, 16.0),
                epsg=28992,
                opacity=1.0,
            )
            previous = layer.image
            canvas = Image.new("RGBA", (32, 32), (255, 255, 255, 255))
            renderer = MapRenderer()
            try:
                with mock.patch.object(
                    renderer_module.native_accel, "paint_axis_aligned_tiff", return_value=1
                ):
                    self.assertTrue(
                        renderer._paint_prepared_native_tiff_jobs(
                            renderer_module._rgba_array(canvas),
                            [(layer, (0.0, 0.0, 16.0, 16.0), (0, 0, 32, 32), False)],
                        )
                    )
                self.assertIsNot(layer.image, previous)
                self.assertIsNone(layer.native_rgba_cache)
                self.assertEqual(layer.image.size, (16, 16))
                self.assertIsNotNone(layer.image.fp)
            finally:
                canvas.close()
                layer.image.close()


if __name__ == "__main__":
    unittest.main()
