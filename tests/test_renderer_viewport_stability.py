from __future__ import annotations

from contextlib import ExitStack
from pathlib import Path
import unittest
from unittest import mock

from PIL import Image

from SleufBase.models import Bounds, GeoTiffLayer, GeoTransform, ViewportTransform
from SleufBase import renderer as renderer_module
from SleufBase.renderer import MapRenderer


class RendererViewportStabilityTests(unittest.TestCase):
    @staticmethod
    def _layer(name: str, x: float) -> GeoTiffLayer:
        return GeoTiffLayer(
            path=Path(name),
            image=Image.new("RGBA", (16, 16), (10, 20, 30, 255)),
            transform=GeoTransform(1.0, 0.0, x, 0.0, -1.0, 16.0),
            bounds=Bounds(x, 0.0, x + 16.0, 16.0),
            epsg=28992,
            opacity=1.0,
        )

    def test_offscreen_affine_layers_do_not_decode_or_allocate_warps(self) -> None:
        layers = [self._layer(f"offscreen-{index}.tif", 100.0 + index * 20.0) for index in range(8)]
        for layer in layers:
            layer.transform = GeoTransform(1.0, 0.25, layer.bounds.min_x, 0.25, -1.0, 16.0)
        result = None
        try:
            with ExitStack() as stack:
                stack.enter_context(mock.patch.object(renderer_module.native_accel, "is_available", return_value=False))
                conversions = [
                    stack.enter_context(mock.patch.object(layer.image, "convert", wraps=layer.image.convert))
                    for layer in layers
                ]
                inverse = stack.enter_context(mock.patch.object(renderer_module.np.linalg, "inv", wraps=renderer_module.np.linalg.inv))
                result = MapRenderer().render(Bounds(0.0, 0.0, 32.0, 16.0), (64, 32), layers, [])
            self.assertEqual(sum(conversion.call_count for conversion in conversions), 0)
            self.assertEqual(inverse.call_count, 0)
            self.assertEqual(result.getextrema(), ((245, 245), (245, 245), (245, 245), (255, 255)))
        finally:
            if result is not None:
                result.close()
            for layer in layers:
                layer.image.close()

    def test_offscreen_singular_affine_transform_does_not_break_rendering(self) -> None:
        layer = self._layer("offscreen-singular.tif", 100.0)
        layer.transform = GeoTransform(1.0, 1.0, 100.0, 1.0, 1.0, 16.0)
        result = None
        try:
            with mock.patch.object(renderer_module.native_accel, "is_available", return_value=False):
                result = MapRenderer().render(Bounds(0.0, 0.0, 32.0, 16.0), (64, 32), [layer], [])
            self.assertEqual(result.getextrema(), ((245, 245), (245, 245), (245, 245), (255, 255)))
        finally:
            if result is not None:
                result.close()
            layer.image.close()

    def test_visible_affine_layer_still_paints(self) -> None:
        layer = self._layer("visible-affine.tif", 0.0)
        layer.transform = GeoTransform(1.0, 0.25, 0.0, 0.25, -1.0, 16.0)
        layer.bounds = Bounds(0.0, 0.0, 20.0, 20.0)
        result = None
        try:
            with mock.patch.object(renderer_module.native_accel, "is_available", return_value=False):
                result = MapRenderer().render(Bounds(-1.0, -1.0, 21.0, 21.0), (44, 44), [layer], [])
            self.assertEqual(result.getpixel((20, 20)), (10, 20, 30, 255))
            self.assertEqual(result.getpixel((0, 0)), (245, 245, 245, 255))
        finally:
            if result is not None:
                result.close()
            layer.image.close()

    def test_panning_releases_offscreen_cache_and_reuses_visible_sources(self) -> None:
        layers = [self._layer(f"pan-{index}.tif", x) for index, x in enumerate((0.0, 16.0, 100.0, 116.0))]
        renderer = MapRenderer()
        canvas = Image.new("RGBA", (64, 32), (255, 255, 255, 255))
        source_images = [layer.image for layer in layers]
        budget = 2 * 16 * 16

        def paint(view_bounds: Bounds) -> None:
            result = renderer._paint_tiff_layers_native(canvas, ViewportTransform(view_bounds, 64, 32), layers)
            self.assertIsNotNone(result)
            if result is not None and result is not canvas:
                result.close()

        def source_allocation_count(array_calls) -> int:
            return sum(
                any(call.args[0] is source for source in source_images)
                for call in array_calls
                if call.args
            )

        try:
            with mock.patch.object(renderer_module, "_NATIVE_TIFF_CACHE_PIXEL_BUDGET", budget), mock.patch.object(
                renderer_module.native_accel, "is_available", return_value=True
            ), mock.patch.object(renderer_module.native_accel, "paint_axis_aligned_tiff", return_value=1), mock.patch.object(
                renderer_module.np, "array", wraps=renderer_module.np.array
            ) as array:
                first_view = Bounds(0.0, 0.0, 32.0, 16.0)
                paint(first_view)
                self.assertEqual(source_allocation_count(array.call_args_list), 2)
                first_caches = [layer.native_rgba_cache for layer in layers[:2]]
                paint(first_view)
                self.assertEqual(source_allocation_count(array.call_args_list), 2)
                self.assertTrue(all(layer.native_rgba_cache is cache for layer, cache in zip(layers[:2], first_caches)))

                paint(Bounds(100.0, 0.0, 132.0, 16.0))
                self.assertEqual(source_allocation_count(array.call_args_list), 4)
                self.assertTrue(all(layer.native_rgba_cache is None for layer in layers[:2]))
                self.assertTrue(all(layer.native_rgba_cache is not None for layer in layers[2:]))
                retained_pixels = sum(
                    layer.native_rgba_cache.rgba.shape[0] * layer.native_rgba_cache.rgba.shape[1]
                    for layer in layers
                    if layer.native_rgba_cache is not None
                )
                self.assertEqual(retained_pixels, budget)
        finally:
            canvas.close()
            for layer in layers:
                layer.image.close()

    def test_tiff_touching_viewport_edge_releases_its_cache(self) -> None:
        layer = self._layer("touching.tif", 32.0)
        renderer = MapRenderer()
        try:
            renderer._native_tiff_rgba_cache(layer)
            self.assertIsNotNone(layer.native_rgba_cache)
            with mock.patch.object(renderer_module.native_accel, "is_available", return_value=True):
                jobs = renderer._prepared_native_tiff_jobs(
                    ViewportTransform(Bounds(0.0, 0.0, 32.0, 16.0), 64, 32), [layer]
                )
            self.assertEqual(jobs, [])
            self.assertIsNone(layer.native_rgba_cache)
        finally:
            layer.image.close()


if __name__ == "__main__":
    unittest.main()
