from __future__ import annotations

from pathlib import Path
import unittest
from unittest import mock

from PIL import Image

from SleufBase.marxact_live_render_patch import (
    COLOR_PATCH_VERSION,
    LIVE_RENDER_COLOR_VERSION_KEY,
    LIVE_RENDER_VERSION_KEY,
    PATCH_VERSION,
    apply_marxact_display_colors,
    refresh_marxact_live_render,
)
from SleufBase.models import Bounds, GeoTiffLayer, GeoTransform
from SleufBase.virtual_trench import VIRTUAL_TRENCH_METADATA_KEY


class RenderHotPathPerformanceTests(unittest.TestCase):
    @staticmethod
    def _layer(*, colored: bool = True) -> GeoTiffLayer:
        bounds = Bounds(0.0, 0.0, 4.0, 1.0)
        image = Image.new("RGBA", (64, 16), (0, 0, 0, 0))
        transform = GeoTransform(
            bounds.width / image.width,
            0.0,
            bounds.min_x,
            0.0,
            -(bounds.height / image.height),
            bounds.max_y,
        )
        point = {
            "role": "object",
            "source_name": "water",
            "object_name": "Water",
            "x": 2.0,
            "y": 0.5,
            "z": 1.0,
        }
        if colored:
            point["display_rgb"] = [0, 0, 255]
        return GeoTiffLayer(
            path=Path("performance.marxact-virtual.tif"),
            image=image,
            transform=transform,
            bounds=bounds,
            epsg=28992,
            opacity=1.0,
            metadata={
                VIRTUAL_TRENCH_METADATA_KEY: {
                    "source": "marxact",
                    "points": [point],
                },
                "marxact_source_path": "performance.dxf",
            },
        )

    def test_already_colored_layer_does_not_resolve_settings_rules(self) -> None:
        layer = self._layer(colored=True)
        try:
            with mock.patch(
                "SleufBase.marxact_live_render_patch._configured_object_layer_rules",
                side_effect=AssertionError("settings rules must not be resolved"),
            ):
                self.assertFalse(apply_marxact_display_colors(layer))
        finally:
            layer.image.close()

    def test_current_layer_fast_path_skips_color_scan_and_raster_rebuild(self) -> None:
        layer = self._layer(colored=True)
        layer.metadata[LIVE_RENDER_VERSION_KEY] = PATCH_VERSION
        layer.metadata[LIVE_RENDER_COLOR_VERSION_KEY] = COLOR_PATCH_VERSION
        try:
            with mock.patch(
                "SleufBase.marxact_live_render_patch.apply_marxact_display_colors",
                side_effect=AssertionError("color scan must be skipped"),
            ):
                self.assertFalse(refresh_marxact_live_render(layer))
        finally:
            layer.image.close()

    def test_existing_render_version_gets_one_time_color_fast_path_marker(self) -> None:
        layer = self._layer(colored=True)
        layer.metadata[LIVE_RENDER_VERSION_KEY] = PATCH_VERSION
        try:
            self.assertNotIn(LIVE_RENDER_COLOR_VERSION_KEY, layer.metadata)
            with mock.patch(
                "SleufBase.marxact_live_render_patch._configured_object_layer_rules",
                side_effect=AssertionError("fully colored layer must not load settings"),
            ):
                self.assertFalse(refresh_marxact_live_render(layer))
            self.assertEqual(
                layer.metadata[LIVE_RENDER_COLOR_VERSION_KEY],
                COLOR_PATCH_VERSION,
            )
        finally:
            layer.image.close()


if __name__ == "__main__":
    unittest.main()
