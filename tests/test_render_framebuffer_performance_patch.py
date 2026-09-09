from __future__ import annotations

from pathlib import Path
import unittest
from unittest import mock

from PIL import Image

from SleufBase.models import Bounds, CableFeature, DxfOverlay, GeoTiffLayer, GeoTransform
from SleufBase.renderer import MapRenderer
from SleufBase import render_framebuffer_performance_patch as framebuffer


class RenderFramebufferPerformancePatchTests(unittest.TestCase):
    def _tiff_layer(self) -> GeoTiffLayer:
        image = Image.new("RGBA", (4, 4), (10, 20, 30, 255))
        return GeoTiffLayer(
            path=Path("layer.tif"),
            image=image,
            transform=GeoTransform(1.0, 0.0, 0.0, 0.0, -1.0, 4.0),
            bounds=Bounds(0.0, 0.0, 4.0, 4.0),
            epsg=28992,
            opacity=1.0,
        )

    def _overlay(self) -> DxfOverlay:
        feature = CableFeature(
            feature_id="f1",
            source_path=Path("overlay.dxf"),
            points=[(0.0, 0.0), (4.0, 4.0)],
            bounds=Bounds(0.0, 0.0, 4.0, 4.0),
            color=(255, 0, 0),
        )
        return DxfOverlay(Path("overlay.dxf"), [feature])

    def test_patch_is_installed(self) -> None:
        self.assertGreaterEqual(
            int(getattr(MapRenderer, "_sleufbase_render_framebuffer_performance_version", 0) or 0),
            1,
        )
        self.assertTrue(MapRenderer.SLEUFBASE_SHARED_NATIVE_FRAMEBUFFER)

    def test_combined_native_path_converts_canvas_to_numpy_once(self) -> None:
        renderer = MapRenderer()
        layer = self._tiff_layer()
        try:
            with mock.patch.object(framebuffer, "_NATIVE_TIFF_MIN_LAYERS", 0), mock.patch.object(
                framebuffer, "_NATIVE_TIFF_MIN_DEST_PIXELS", 0
            ), mock.patch.object(framebuffer, "_NATIVE_DXF_MIN_FEATURES", 0), mock.patch.object(
                framebuffer, "_NATIVE_DXF_MIN_POINTS", 0
            ), mock.patch.object(
                framebuffer.native_accel, "is_available", return_value=True
            ), mock.patch.object(
                framebuffer.native_accel, "paint_axis_aligned_tiff", return_value=1
            ) as paint, mock.patch.object(
                framebuffer.native_accel, "render_dxf_overlay", return_value=1
            ) as draw_dxf, mock.patch.object(
                framebuffer, "_rgba_array", wraps=framebuffer._rgba_array
            ) as rgba_array, mock.patch.object(
                framebuffer,
                "_ORIGINAL_RENDER",
                side_effect=AssertionError("combined native path should not fall back"),
            ):
                result = renderer.render(
                    Bounds(0.0, 0.0, 4.0, 4.0),
                    (4, 4),
                    [layer],
                    [self._overlay()],
                )
            try:
                self.assertEqual(result.size, (4, 4))
                self.assertEqual(rgba_array.call_count, 1)
                self.assertEqual(paint.call_count, 1)
                self.assertEqual(draw_dxf.call_count, 1)
            finally:
                result.close()
        finally:
            layer.image.close()

    def test_non_native_path_uses_existing_renderer_unchanged(self) -> None:
        renderer = MapRenderer()
        sentinel = Image.new("RGBA", (2, 2), (1, 2, 3, 255))
        try:
            with mock.patch.object(framebuffer.native_accel, "is_available", return_value=False), mock.patch.object(
                framebuffer, "_ORIGINAL_RENDER", return_value=sentinel
            ) as original:
                result = renderer.render(
                    Bounds(0.0, 0.0, 1.0, 1.0),
                    (2, 2),
                    [],
                    [],
                )
            self.assertIs(result, sentinel)
            original.assert_called_once()
        finally:
            sentinel.close()


if __name__ == "__main__":
    unittest.main()
