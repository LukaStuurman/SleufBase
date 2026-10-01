from __future__ import annotations

from pathlib import Path
import unittest
from unittest import mock

from PIL import Image

from SleufBase.models import Bounds, GeoTiffLayer, GeoTransform
from SleufBase import virtual_trench as vt


def _layer(source: str, object_count: int) -> GeoTiffLayer:
    points = [{"role": "start", "x": 0.0, "y": 0.0}]
    points.extend(
        {
            "role": "object",
            "x": (index - 2) * 10.0 / max(1, object_count - 4),
            "y": (-1.0 if index % 2 else 1.0) * 0.2,
            "display_rgb": [20 + index % 100, 40, 200],
        }
        for index in range(object_count)
    )
    points.append({"role": "end", "x": 10.0, "y": 0.0})
    payload = {"source": source, "width_meters": 0.6, "points": points}
    if source == "marxact":
        payload["boundary_3d"] = [[0, -0.3, 2], [10, -0.3, 2], [9.8, 0.3, 2], [0.2, 0.3, 2]]
    return GeoTiffLayer(
        path=Path(f"{source}.virtual.tif"),
        image=Image.new("RGBA", (1, 1), (0, 0, 0, 0)),
        transform=GeoTransform(1, 0, 0, 0, -1, 1),
        bounds=Bounds(0, -0.3, 10, 0.3),
        epsg=28992,
        opacity=1.0,
        metadata={vt.VIRTUAL_TRENCH_METADATA_KEY: payload},
    )


class VirtualTrenchRenderScalingTests(unittest.TestCase):
    def test_measurement_matches_active_renderer_with_tiny_stale_live_image(self) -> None:
        for source in ("kickthemap", "marxact"):
            for quality in (1.0, 2.5):
                with self.subTest(source=source, quality=quality):
                    layer = _layer(source, 25)
                    try:
                        with mock.patch.object(Image, "new", side_effect=AssertionError("measurement must not allocate a raster")):
                            estimated = vt.virtual_trench_render_size(layer, quality_multiplier=quality)
                        image, _bounds, _transform = vt.build_virtual_trench_render(layer, quality_multiplier=quality)
                        try:
                            self.assertEqual(estimated, image.size)
                            self.assertGreater(estimated[0] * estimated[1], layer.image.width * layer.image.height)
                        finally:
                            image.close()
                    finally:
                        layer.image.close()

    def test_measurement_does_not_repair_original_metadata(self) -> None:
        layer = _layer("marxact", 2)
        payload = layer.metadata[vt.VIRTUAL_TRENCH_METADATA_KEY]
        layer.metadata["marxact_boundary_3d"] = payload.pop("boundary_3d")
        original_points = [42, *payload["points"]]
        payload["points"] = original_points
        try:
            vt.virtual_trench_render_size(layer, quality_multiplier=2.5)
            self.assertIs(payload["points"], original_points)
            self.assertEqual(payload["points"][0], 42)
            self.assertNotIn("boundary_3d", payload)
        finally:
            layer.image.close()

    def test_missing_or_empty_payload_measures_placeholder_without_allocation(self) -> None:
        layer = _layer("kickthemap", 2)
        try:
            for metadata in ({}, {vt.VIRTUAL_TRENCH_METADATA_KEY: {"points": []}}):
                layer.metadata = metadata
                with mock.patch.object(Image, "new", side_effect=AssertionError("measurement must not allocate a raster")):
                    self.assertEqual(vt.virtual_trench_render_size(layer), (1, 1))
        finally:
            layer.image.close()

    def test_active_render_endpoint_scans_do_not_grow_with_object_count(self) -> None:
        for source in ("kickthemap", "marxact"):
            counts = []
            for object_count in (25, 250):
                with self.subTest(source=source, objects=object_count):
                    layer = _layer(source, object_count)
                    try:
                        with mock.patch.object(vt, "virtual_trench_endpoints", wraps=vt.virtual_trench_endpoints) as endpoints:
                            image, _bounds, _transform = vt.build_virtual_trench_render(layer, quality_multiplier=2.5)
                        image.close()
                        counts.append(endpoints.call_count)
                        self.assertLessEqual(endpoints.call_count, 8)
                    finally:
                        layer.image.close()
            self.assertEqual(counts[0], counts[1])

    def test_active_normal_and_high_resolution_renders_match_per_object_endpoint_lookup(self) -> None:
        project = vt._project_point_from_endpoints
        for source in ("kickthemap", "marxact"):
            for quality in (1.0, 2.5):
                with self.subTest(source=source, quality=quality):
                    layer = _layer(source, 40)
                    fast = slow = None
                    try:
                        fast, fast_bounds, fast_transform = vt.build_virtual_trench_render(layer, quality_multiplier=quality)

                        def resolve_for_every_object(_start, _end, x, y):
                            start, end = vt.virtual_trench_endpoints(layer)
                            return project(start, end, x, y)

                        with mock.patch.object(vt, "_project_point_from_endpoints", side_effect=resolve_for_every_object):
                            slow, slow_bounds, slow_transform = vt.build_virtual_trench_render(layer, quality_multiplier=quality)
                        self.assertEqual(fast_bounds, slow_bounds)
                        self.assertEqual(fast_transform, slow_transform)
                        self.assertEqual(fast.size, slow.size)
                        self.assertEqual(fast.tobytes(), slow.tobytes())
                    finally:
                        if fast is not None:
                            fast.close()
                        if slow is not None:
                            slow.close()
                        layer.image.close()

    def test_projection_preserves_clamping_and_degenerate_endpoints(self) -> None:
        start, end = {"x": 0.0, "y": 0.0}, {"x": 10.0, "y": 0.0}
        projected = vt._project_point_from_endpoints(start, end, 12.0, 5.0)
        self.assertEqual(projected[:3], (10.0, 0.0, 10.0))
        self.assertAlmostEqual(projected[3], 2.0)
        self.assertEqual(vt._project_point_from_endpoints(start, end, -2.0, 5.0), (0.0, 0.0, 0.0, 2.0))
        self.assertEqual(vt._project_point_from_endpoints(start, start, 2.0, 5.0), (0.0, 0.0, 0.0, 0.0))
        self.assertEqual(vt._project_point_from_endpoints(None, None, 2.0, 5.0), (2.0, 5.0, 0.0, 0.0))


if __name__ == "__main__":
    unittest.main()
