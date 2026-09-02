from __future__ import annotations

import math
import unittest

from PIL import Image

from SleufBase import cadastral_export, marxact_import, virtual_trench
from SleufBase.marxact_boundary_patch import (
    _clip_render_to_measured_boundary,
)
from SleufBase.marxact_direction_patch import (
    alignment_rotation_degrees,
    apply_virtual_layer_alignment_rotation,
    recalculate_virtual_layer_automatic_alignment,
    trench_centerline,
)
from SleufBase.marxact_import import (
    MarXactObject,
    MarXactTrench,
    build_marxact_virtual_layer,
)
from SleufBase.models import Bounds
from SleufBase.virtual_trench import VIRTUAL_TRENCH_METADATA_KEY


def _rotated_rectangle(
    angle_degrees: float,
    *,
    half_length: float = 2.0,
    half_width: float = 0.6,
    center_x: float = 0.0,
    center_y: float = 0.0,
) -> tuple[tuple[float, float, float], ...]:
    angle = math.radians(angle_degrees)
    axis_x, axis_y = math.cos(angle), math.sin(angle)
    normal_x, normal_y = -axis_y, axis_x
    corners = [
        (-half_length, -half_width),
        (half_length, -half_width),
        (half_length, half_width),
        (-half_length, half_width),
    ]
    return tuple(
        (
            center_x + (axis_x * along) + (normal_x * across),
            center_y + (axis_y * along) + (normal_y * across),
            10.0 + (0.2 * along),
        )
        for along, across in corners
    )


def _line_angle_degrees(start: tuple[float, float], end: tuple[float, float]) -> float:
    angle = math.degrees(math.atan2(end[1] - start[1], end[0] - start[0]))
    while angle <= -90.0:
        angle += 180.0
    while angle > 90.0:
        angle -= 180.0
    return angle


def _angle_difference_degrees(first: float, second: float) -> float:
    difference = abs((first - second) % 180.0)
    return min(difference, 180.0 - difference)


def _distance_point_to_segment(
    point: tuple[float, float],
    start: tuple[float, float],
    end: tuple[float, float],
) -> float:
    px, py = point
    sx, sy = start
    ex, ey = end
    dx, dy = ex - sx, ey - sy
    length_sq = (dx * dx) + (dy * dy)
    if length_sq <= 1e-12:
        return math.hypot(px - sx, py - sy)
    t = max(0.0, min(1.0, (((px - sx) * dx) + ((py - sy) * dy)) / length_sq))
    return math.hypot(px - (sx + t * dx), py - (sy + t * dy))


class MarXactAlignmentTests(unittest.TestCase):
    def _layer(self):
        polygon = _rotated_rectangle(10.0)
        object_angle = math.radians(13.0)
        objects = [
            MarXactObject(
                x=math.cos(object_angle) * t,
                y=math.sin(object_angle) * t,
                z=9.0 + (0.1 * t),
                layer_name="Laagspanning",
                name=f"LS {index}",
                height=9.0 + (0.1 * t),
                block_name="marxact_point",
            )
            for index, t in enumerate((-1.3, -0.4, 0.5, 1.25), start=1)
        ]
        trench = MarXactTrench(name="PS42", polygon=polygon, objects=objects)
        return trench, build_marxact_virtual_layer(
            trench,
            source_path="alignment-test.dxf",
            source_name_resolver=lambda item: item.mapping_name,
            fallback_index=42,
        )

    def test_automatic_alignment_snaps_small_block_skew_to_measured_boundary(self) -> None:
        trench, _layer = self._layer()
        start, end, _width = trench_centerline(trench)
        angle = _line_angle_degrees((start[0], start[1]), (end[0], end[1]))
        self.assertLess(_angle_difference_degrees(angle, 10.0), 0.25)

    def test_manual_rotation_reclips_endpoints_to_3d_boundary(self) -> None:
        trench, layer = self._layer()
        self.assertTrue(recalculate_virtual_layer_automatic_alignment(layer))
        payload = layer.metadata[VIRTUAL_TRENCH_METADATA_KEY]
        start = next(point for point in payload["points"] if point.get("role") == "start")
        end = next(point for point in payload["points"] if point.get("role") == "end")
        auto_angle = _line_angle_degrees(
            (float(start["x"]), float(start["y"])),
            (float(end["x"]), float(end["y"])),
        )

        self.assertTrue(apply_virtual_layer_alignment_rotation(layer, 5.0))
        self.assertAlmostEqual(alignment_rotation_degrees(layer), 5.0, places=6)
        rotated_angle = _line_angle_degrees(
            (float(start["x"]), float(start["y"])),
            (float(end["x"]), float(end["y"])),
        )
        self.assertLess(_angle_difference_degrees(rotated_angle, auto_angle + 5.0), 0.15)

        polygon_xy = [(x, y) for x, y, _z in trench.polygon]
        for point in (start, end):
            xy = (float(point["x"]), float(point["y"]))
            edge_distance = min(
                _distance_point_to_segment(
                    xy,
                    polygon_xy[index],
                    polygon_xy[(index + 1) % len(polygon_xy)],
                )
                for index in range(len(polygon_xy))
            )
            self.assertLess(edge_distance, 1e-6)
            self.assertIsNotNone(point.get("z"))

        self.assertTrue(apply_virtual_layer_alignment_rotation(layer, 0.0))
        reset_angle = _line_angle_degrees(
            (float(start["x"]), float(start["y"])),
            (float(end["x"]), float(end["y"])),
        )
        self.assertLess(_angle_difference_degrees(reset_angle, auto_angle), 0.15)

    def test_measured_boundary_clips_coloured_render_pixels(self) -> None:
        image = Image.new("RGBA", (101, 101), (255, 0, 0, 255))
        boundary = [
            (2.0, 2.0, 1.0),
            (8.0, 2.0, 1.0),
            (8.0, 8.0, 1.0),
            (2.0, 8.0, 1.0),
        ]
        try:
            _clip_render_to_measured_boundary(
                image,
                Bounds(0.0, 0.0, 10.0, 10.0),
                boundary,
                border_rgba=(0, 0, 0, 255),
                border_width=2,
            )
            self.assertEqual(image.getpixel((5, 5))[3], 0)
            self.assertEqual(image.getpixel((50, 50))[3], 255)
            self.assertEqual(image.getpixel((95, 95))[3], 0)
        finally:
            image.close()

    def test_app_and_template_export_use_same_clipped_renderer(self) -> None:
        self.assertIs(
            virtual_trench.build_virtual_trench_render,
            cadastral_export.build_virtual_trench_render,
        )
        self.assertIs(
            virtual_trench.build_virtual_trench_render,
            marxact_import.build_virtual_trench_render,
        )


if __name__ == "__main__":
    unittest.main()
