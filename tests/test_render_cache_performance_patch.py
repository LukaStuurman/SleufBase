from __future__ import annotations

from pathlib import Path
import unittest

import numpy as np

from SleufBase.models import Bounds, CableFeature
from SleufBase.renderer import MapRenderer
from SleufBase.render_cache_performance_patch import PATCH_VERSION, _flag_index_cache


class _CountingFeatureIds(tuple):
    def __new__(cls, values):
        instance = super().__new__(cls, values)
        instance.iterations = 0
        return instance

    def __iter__(self):
        self.iterations += 1
        return super().__iter__()


class RenderCachePerformancePatchTests(unittest.TestCase):
    def test_patch_is_installed(self) -> None:
        self.assertGreaterEqual(
            int(getattr(MapRenderer, "_sleufbase_render_cache_performance_patch_version", 0) or 0),
            PATCH_VERSION,
        )
        self.assertGreaterEqual(
            int(getattr(CableFeature, "_sleufbase_render_cache_performance_patch_version", 0) or 0),
            PATCH_VERSION,
        )

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

    def test_fast_distance_matches_original_implementation(self) -> None:
        feature = CableFeature(
            feature_id="distance",
            source_path=Path("distance.dxf"),
            points=[(0.0, 0.0), (3.0, 4.0), (3.0, 4.0), (8.0, 1.0)],
            bounds=Bounds(0.0, 0.0, 8.0, 4.0),
            color=(12, 34, 56),
        )
        original = getattr(CableFeature, "_sleufbase_original_distance_to")
        for x, y in ((0.0, 0.0), (1.0, 2.0), (3.0, 4.0), (6.5, 2.0), (20.0, 20.0)):
            self.assertAlmostEqual(feature.distance_to(x, y), original(feature, x, y), places=12)


if __name__ == "__main__":
    unittest.main()
