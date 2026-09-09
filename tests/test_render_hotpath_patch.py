from __future__ import annotations

from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest import mock

import numpy as np
from PIL import Image, ImageDraw

from SleufBase import native_accel
from SleufBase.models import Bounds, CableFeature, DxfOverlay, ViewportTransform
from SleufBase.renderer import MapRenderer, _rgba_array


class RenderHotpathTests(unittest.TestCase):
    @staticmethod
    def _overlay() -> DxfOverlay:
        return DxfOverlay(
            path=Path("render-test.dxf"),
            features=[
                CableFeature(
                    feature_id="normal",
                    source_path=Path("render-test.dxf"),
                    points=[(0.0, 0.0), (4.0, 4.0), (8.0, 1.0)],
                    bounds=Bounds(0.0, 0.0, 8.0, 4.0),
                    color=(255, 0, 0),
                ),
                CableFeature(
                    feature_id="selected",
                    source_path=Path("render-test.dxf"),
                    points=[(1.0, 7.0), (9.0, 7.0)],
                    bounds=Bounds(1.0, 7.0, 9.0, 7.0),
                    color=(0, 128, 0),
                ),
                CableFeature(
                    feature_id="point",
                    source_path=Path("render-test.dxf"),
                    points=[(5.0, 5.0), (5.0, 5.0)],
                    bounds=Bounds(5.0, 5.0, 5.0, 5.0),
                    color=(0, 0, 255),
                ),
                CableFeature(
                    feature_id="outside",
                    source_path=Path("render-test.dxf"),
                    points=[(20.0, 20.0), (21.0, 21.0)],
                    bounds=Bounds(20.0, 20.0, 21.0, 21.0),
                    color=(0, 0, 0),
                ),
            ],
        )

    @staticmethod
    def _draw_reference(draw, transform, overlays, selected, highlighted) -> None:
        for overlay in overlays:
            if not overlay.visible:
                continue
            for feature in overlay.features:
                if not feature.bounds.intersects(transform.bounds):
                    continue
                screen_points = transform.world_points_to_screen(feature.points)
                if len(screen_points) < 2:
                    continue
                if feature.feature_id in highlighted:
                    draw.line(screen_points, fill=(0, 160, 255, 105), width=10)
                if feature.feature_id in selected:
                    draw.line(screen_points, fill=(255, 215, 0, 215), width=7)
                line_width = (
                    4
                    if transform.meters_per_pixel < 0.05
                    else 3
                    if transform.meters_per_pixel < 0.2
                    else 2
                )
                draw.line(screen_points, fill=(*feature.color, 230), width=line_width)
                if len(screen_points) == 2 and screen_points[0] == screen_points[1]:
                    x, y = screen_points[0]
                    radius = max(4, line_width + 2)
                    if feature.feature_id in highlighted:
                        draw.ellipse(
                            (x - radius - 4, y - radius - 4, x + radius + 4, y + radius + 4),
                            fill=(0, 160, 255, 105),
                        )
                    if feature.feature_id in selected:
                        draw.ellipse(
                            (x - radius - 3, y - radius - 3, x + radius + 3, y + radius + 3),
                            fill=(255, 215, 0, 215),
                        )
                    draw.ellipse(
                        (x - radius, y - radius, x + radius, y + radius),
                        fill=(*feature.color, 230),
                    )

    def test_python_fallback_is_pixel_identical_to_reference_renderer(self) -> None:
        renderer = MapRenderer()
        transform = ViewportTransform(Bounds(0.0, 0.0, 10.0, 10.0), 500, 400)
        overlay = self._overlay()
        selected = {"selected", "point"}
        highlighted = {"normal", "point"}
        baseline = Image.new("RGBA", (500, 400), (245, 245, 245, 255))
        optimized = baseline.copy()
        try:
            self._draw_reference(
                ImageDraw.Draw(baseline, "RGBA"), transform, [overlay], selected, highlighted
            )
            renderer._draw_dxf_overlays_python(
                ImageDraw.Draw(optimized, "RGBA"), transform, [overlay], selected, highlighted
            )
            self.assertEqual(baseline.tobytes(), optimized.tobytes())
        finally:
            baseline.close()
            optimized.close()

    def test_large_native_overlay_skips_redundant_point_count_pass(self) -> None:
        class FeatureWithoutReadablePoints:
            @property
            def points(self):
                raise AssertionError("point count pass should be skipped")

        features = [FeatureWithoutReadablePoints() for _ in range(1000)]
        overlay = SimpleNamespace(visible=True, features=features)
        cache = SimpleNamespace(
            feature_ids=("feature",),
            points_xy=np.array([[0.0, 0.0], [1.0, 1.0]], dtype=np.float64),
            feature_offsets=np.array([0, 2], dtype=np.int32),
            feature_bounds=np.array([[0.0, 0.0, 1.0, 1.0]], dtype=np.float64),
            feature_colors=np.array([[255, 0, 0]], dtype=np.uint8),
        )
        renderer = MapRenderer()
        transform = ViewportTransform(Bounds(0.0, 0.0, 10.0, 10.0), 100, 100)
        canvas = Image.new("RGBA", (100, 100), (255, 255, 255, 255))
        rendered = None
        try:
            with mock.patch.object(native_accel, "is_available", return_value=True), mock.patch.object(
                native_accel, "render_dxf_overlay", return_value=1
            ), mock.patch.object(renderer, "_native_dxf_render_cache", return_value=cache):
                rendered = renderer._render_dxf_overlays_native(
                    canvas, transform, [overlay], set(), set()
                )
            self.assertIsNotNone(rendered)
        finally:
            if rendered is not None and rendered is not canvas:
                rendered.close()
            canvas.close()

    def test_rgba_array_preserves_shape_and_writable_copy(self) -> None:
        canvas = Image.new("RGBA", (12, 8), (1, 2, 3, 4))
        try:
            rgba = _rgba_array(canvas)
            self.assertEqual(rgba.shape, (8, 12, 4))
            self.assertTrue(rgba.flags.writeable)
            rgba[0, 0] = (9, 8, 7, 6)
            self.assertEqual(canvas.getpixel((0, 0)), (1, 2, 3, 4))
        finally:
            canvas.close()


if __name__ == "__main__":
    unittest.main()
