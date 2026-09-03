from __future__ import annotations

import math
import unittest

from SleufBase import cadastral_export, marxact_import, virtual_trench
from SleufBase.marxact_local_cross_section_patch import local_cross_section_segment


class MarXactLocalCrossSectionTests(unittest.TestCase):
    def setUp(self) -> None:
        # Irregular measured trench: left side is 2.0 m wide, right side only
        # 0.6 m. A single global/max width would visibly protrude on the right.
        self.polygon = [
            (-2.0, -1.0),
            (2.0, -0.3),
            (2.0, 0.3),
            (-2.0, 1.0),
        ]

    def test_each_marker_uses_local_polygon_width(self) -> None:
        left = local_cross_section_segment(
            self.polygon,
            -1.5,
            0.0,
            0.0,
            1.0,
        )
        right = local_cross_section_segment(
            self.polygon,
            1.5,
            0.0,
            0.0,
            1.0,
        )
        self.assertIsNotNone(left)
        self.assertIsNotNone(right)
        assert left is not None and right is not None

        left_width = math.dist(left[0], left[1])
        right_width = math.dist(right[0], right[1])
        self.assertGreater(left_width, right_width + 0.8)
        self.assertAlmostEqual(left[0][0], -1.5, places=7)
        self.assertAlmostEqual(left[1][0], -1.5, places=7)
        self.assertAlmostEqual(right[0][0], 1.5, places=7)
        self.assertAlmostEqual(right[1][0], 1.5, places=7)

    def test_original_object_offset_selects_correct_concave_inside_span(self) -> None:
        # Two inside spans exist at x=0. The measured object lies in the upper
        # span, so preferred_t must select that local piece instead of another
        # part of the concave polygon.
        polygon = [
            (-2.0, -2.0),
            (2.0, -2.0),
            (2.0, -0.5),
            (-0.5, -0.5),
            (-0.5, 0.5),
            (2.0, 0.5),
            (2.0, 2.0),
            (-2.0, 2.0),
        ]
        segment = local_cross_section_segment(
            polygon,
            0.0,
            0.0,
            0.0,
            1.0,
            preferred_t=1.0,
        )
        self.assertIsNotNone(segment)
        assert segment is not None
        ys = sorted((segment[0][1], segment[1][1]))
        self.assertAlmostEqual(ys[0], 0.5, places=7)
        self.assertAlmostEqual(ys[1], 2.0, places=7)

    def test_all_render_paths_use_local_cross_section_renderer(self) -> None:
        self.assertIs(
            virtual_trench.build_virtual_trench_render,
            cadastral_export.build_virtual_trench_render,
        )
        self.assertIs(
            virtual_trench.build_virtual_trench_render,
            marxact_import.build_virtual_trench_render,
        )
        self.assertGreaterEqual(
            int(getattr(virtual_trench, "_marxact_local_cross_section_patch_version", 0)),
            1,
        )


if __name__ == "__main__":
    unittest.main()
