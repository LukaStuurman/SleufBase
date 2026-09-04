from __future__ import annotations

from pathlib import Path
import unittest

from ezdxf import colors as dxf_colors
from PIL import Image

from SleufBase.kickthemap_dxf_export import build_object_layer_rules
from SleufBase.marxact_import import MarXactObject, MarXactTrench, build_marxact_virtual_layer
from SleufBase.marxact_live_render_patch import (
    LIVE_RENDER_VERSION_KEY,
    PATCH_VERSION,
    apply_marxact_display_colors,
    refresh_marxact_live_render,
)
from SleufBase.models import Bounds, GeoTiffLayer, GeoTransform
from SleufBase.renderer import MapRenderer
from SleufBase.virtual_trench import VIRTUAL_TRENCH_METADATA_KEY


class MarXactLiveRenderTests(unittest.TestCase):
    @staticmethod
    def _rules():
        return build_object_layer_rules(
            [
                ("water", "WATER", 5, "Waterleiding"),
                ("data", "DATA", 3, "Datakabel"),
            ]
        )

    @staticmethod
    def _stale_layer() -> GeoTiffLayer:
        bounds = Bounds(-1.0, -1.5, 5.0, 1.5)
        image = Image.new("RGBA", (240, 120), (255, 0, 255, 255))
        transform = GeoTransform(
            bounds.width / image.width,
            0.0,
            bounds.min_x,
            0.0,
            -(bounds.height / image.height),
            bounds.max_y,
        )
        return GeoTiffLayer(
            path=Path("stale.marxact-virtual.tif"),
            image=image,
            transform=transform,
            bounds=bounds,
            epsg=28992,
            opacity=1.0,
            metadata={
                VIRTUAL_TRENCH_METADATA_KEY: {
                    "source": "marxact",
                    "width_meters": 1.0,
                    "points": [
                        {"role": "start", "x": 0.0, "y": 0.0, "z": 10.0},
                        {
                            "role": "object",
                            "source_name": "water",
                            "object_name": "Water",
                            "x": 1.2,
                            "y": 0.10,
                            "z": 9.0,
                        },
                        {
                            "role": "object",
                            "source_name": "data",
                            "object_name": "Data",
                            "x": 3.2,
                            "y": -0.05,
                            "z": 9.1,
                        },
                        {"role": "end", "x": 4.0, "y": 0.0, "z": 10.0},
                    ],
                },
                "marxact_source_path": "stale.dxf",
                "marxact_trench_name": "PS1",
                "marxact_boundary_3d": [
                    [0.0, -0.55, 10.0],
                    [4.0, -0.22, 10.1],
                    [4.0, 0.22, 10.2],
                    [0.0, 0.55, 10.1],
                ],
            },
        )

    @staticmethod
    def _sample_alpha(layer: GeoTiffLayer, world_x: float, world_y: float) -> int:
        px, py = layer.transform.world_to_pixel(world_x, world_y)
        ix = max(0, min(layer.image.width - 1, int(round(px))))
        iy = max(0, min(layer.image.height - 1, int(round(py))))
        return int(layer.image.getpixel((ix, iy))[3])

    def test_refresh_replaces_stale_app_image_and_clips_to_measured_polygon(self) -> None:
        layer = self._stale_layer()
        try:
            changed = refresh_marxact_live_render(layer, force=True, rules=self._rules())
            self.assertTrue(changed)
            self.assertEqual(layer.metadata[LIVE_RENDER_VERSION_KEY], PATCH_VERSION)
            # The upgraded image must contain visible trench/marker pixels. Do
            # not sample one exact world coordinate here: sub-pixel rasterization
            # can legitimately move a 2-3 px diagonal stroke by one pixel.
            alpha = layer.image.getchannel("A")
            try:
                self.assertIsNotNone(alpha.getbbox())
            finally:
                alpha.close()
            # These points lie well inside the raster bounds but outside the real
            # measured 3D-POLYLINE. A stale/global-width render used to leave a
            # coloured stroke here in the live SleufBase map.
            self.assertEqual(self._sample_alpha(layer, 1.2, 0.9), 0)
            self.assertEqual(self._sample_alpha(layer, 3.2, -0.9), 0)
        finally:
            layer.image.close()

    def test_map_renderer_upgrades_old_marxact_layer_before_painting(self) -> None:
        layer = self._stale_layer()
        renderer = MapRenderer()
        background = Image.new("RGBA", (420, 220), (245, 245, 245, 255))
        rendered = None
        try:
            self.assertNotIn(LIVE_RENDER_VERSION_KEY, layer.metadata)
            rendered = renderer.render(
                layer.bounds,
                background.size,
                [layer],
                [],
                background=background,
            )
            self.assertEqual(layer.metadata[LIVE_RENDER_VERSION_KEY], PATCH_VERSION)
            self.assertEqual(self._sample_alpha(layer, 2.0, 1.0), 0)
        finally:
            if rendered is not None:
                rendered.close()
            background.close()
            layer.image.close()

    def test_configured_aci_colours_are_persisted_per_marxact_object(self) -> None:
        layer = self._stale_layer()
        try:
            self.assertTrue(apply_marxact_display_colors(layer, self._rules(), overwrite=True))
            points = layer.metadata[VIRTUAL_TRENCH_METADATA_KEY]["points"]
            objects = [point for point in points if point.get("role") == "object"]
            self.assertEqual(len(objects), 2)
            water_rgb = list(dxf_colors.aci2rgb(5))
            data_rgb = list(dxf_colors.aci2rgb(3))
            self.assertEqual(objects[0]["display_rgb"], water_rgb)
            self.assertEqual(objects[1]["display_rgb"], data_rgb)
            self.assertNotEqual(objects[0]["display_rgb"], objects[1]["display_rgb"])
        finally:
            layer.image.close()

    def test_fresh_marxact_import_no_longer_falls_back_to_one_blue_colour(self) -> None:
        trench = MarXactTrench(
            name="PS2",
            polygon=(
                (0.0, -0.5, 10.0),
                (4.0, -0.5, 10.0),
                (4.0, 0.5, 10.0),
                (0.0, 0.5, 10.0),
            ),
            objects=[
                MarXactObject(1.0, 0.0, 9.0, "Water", "water", 9.0, "POINT"),
                MarXactObject(3.0, 0.0, 9.1, "Data", "data", 9.1, "POINT"),
            ],
        )
        layer = build_marxact_virtual_layer(
            trench,
            source_path="fresh.dxf",
            source_name_resolver=lambda item: item.name,
            fallback_index=2,
        )
        try:
            objects = [
                point
                for point in layer.metadata[VIRTUAL_TRENCH_METADATA_KEY]["points"]
                if point.get("role") == "object"
            ]
            self.assertEqual(len(objects), 2)
            self.assertIn("display_rgb", objects[0])
            self.assertIn("display_rgb", objects[1])
            self.assertNotEqual(objects[0]["display_rgb"], objects[1]["display_rgb"])
            self.assertEqual(layer.metadata[LIVE_RENDER_VERSION_KEY], PATCH_VERSION)
        finally:
            layer.image.close()


if __name__ == "__main__":
    unittest.main()
