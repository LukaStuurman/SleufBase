from __future__ import annotations

import math
from pathlib import Path
import unittest

import numpy as np

from SleufBase.models import Bounds, CableFeature
from SleufBase.renderer import MapRenderer, _flag_index_cache


class _CountingFeatureIds(tuple):
    def __new__(cls, values):
        instance = super().__new__(cls, values)
        instance.iterations = 0
        return instance

    def __iter__(self):
        self.iterations += 1
        return super().__iter__()


def _reference_distance(points, x: float, y: float) -> float:
    if len(points) < 2:
        return float("inf")
    best = float("inf")
    for index in range(len(points) - 1):
        x1, y1 = points[index]
        x2, y2 = points[index + 1]
        dx = x2 - x1
        dy = y2 - y1
        if dx == 0 and dy == 0:
            distance = math.hypot(x - x1, y - y1)
        else:
            projection = ((x - x1) * dx + (y - y1) * dy) / (dx * dx + dy * dy)
            projection = max(0.0, min(1.0, projection))
            nearest_x = x1 + projection * dx
            nearest_y = y1 + projection * dy
            distance = math.hypot(x - nearest_x, y - nearest_y)
        best = min(best, distance)
    return best


class RendererCachePerformanceTests(unittest.TestCase):
    def test_feature_flags_use_cached_index_and_preserve_duplicate_ids(self) -> None:
        feature_ids = ("a", "b", "a", "c", "d")
        flags = MapRenderer._feature_flags(feature_ids, {"a", "c", "missing"}, len(feature_ids))
        np.testing.assert_array_equal(flags, np.array([1, 0, 1, 1, 0], dtype=np.uint8))

        empty = MapRenderer._feature_flags(feature_ids, set(), len(feature_ids))
        np.testing.assert_array_equal(empty, np.zeros(len(feature_ids), dtype=np.uint8))

    def test_feature_id_index_is_built_once_for_repeated_nonempty_selections(self) -> None:
        _flag_index_cache.clear()
        feature_ids = _CountingFeatureIds(("a", "b", "c", "d"))
        first = MapRenderer._feature_flags(feature_ids, {"a"}, len(feature_ids))
        second = MapRenderer._feature_flags(feature_ids, {"d"}, len(feature_ids))

        np.testing.assert_array_equal(first, np.array([1, 0, 0, 0], dtype=np.uint8))
        np.testing.assert_array_equal(second, np.array([0, 0, 0, 1], dtype=np.uint8))
        self.assertEqual(feature_ids.iterations, 1)

    def test_core_fast_distance_matches_reference_geometry(self) -> None:
        feature = CableFeature(
            feature_id="distance",
            source_path=Path("distance.dxf"),
            points=[(0.0, 0.0), (3.0, 4.0), (3.0, 4.0), (8.0, 1.0)],
            bounds=Bounds(0.0, 0.0, 8.0, 4.0),
            color=(12, 34, 56),
        )
        for x, y in ((0.0, 0.0), (1.0, 2.0), (3.0, 4.0), (6.5, 2.0), (20.0, 20.0)):
            self.assertAlmostEqual(
                feature.distance_to(x, y),
                _reference_distance(feature.points, x, y),
                places=12,
            )


if __name__ == "__main__":
    unittest.main()
