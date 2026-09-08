from __future__ import annotations

from pathlib import Path
import unittest

import numpy as np

from SleufBase.models import Bounds, CableFeature, DxfOverlay
from SleufBase.renderer import MapRenderer
from SleufBase.render_cache_performance_patch import PATCH_VERSION


class _SignatureProbe:
    def __init__(self, name: str) -> None:
        self.name = name
        self.calls = 0

    def native_render_signature(self):
        self.calls += 1
        return (self.name, self.calls)


class RenderCachePerformancePatchTests(unittest.TestCase):
    def test_patch_is_installed(self) -> None:
        self.assertGreaterEqual(
            int(getattr(MapRenderer, "_sleufbase_render_cache_performance_patch_version", 0) or 0),
            PATCH_VERSION,
        )
        self.assertGreaterEqual(
            int(getattr(DxfOverlay, "_sleufbase_render_cache_performance_patch_version", 0) or 0),
            PATCH_VERSION,
        )

    def test_overlay_signature_is_reused_until_feature_list_changes(self) -> None:
        probes = [_SignatureProbe("a"), _SignatureProbe("b"), _SignatureProbe("c")]
        overlay = DxfOverlay(path=Path("cache.dxf"), features=probes)  # type: ignore[arg-type]

        first = overlay.native_render_signature()
        second = overlay.native_render_signature()
        self.assertIs(first, second)
        self.assertEqual([probe.calls for probe in probes], [1, 1, 1])

        extra = _SignatureProbe("d")
        overlay.features.append(extra)  # type: ignore[arg-type]
        third = overlay.native_render_signature()
        self.assertIsNot(first, third)
        self.assertEqual([probe.calls for probe in probes], [2, 2, 2])
        self.assertEqual(extra.calls, 1)

    def test_explicit_native_cache_invalidation_also_invalidates_signature(self) -> None:
        probes = [_SignatureProbe("a"), _SignatureProbe("b")]
        overlay = DxfOverlay(path=Path("cache.dxf"), features=probes)  # type: ignore[arg-type]
        first = overlay.native_render_signature()
        self.assertIs(first, overlay.native_render_signature())

        overlay.native_render_cache = object()
        overlay.invalidate_native_render_cache()
        self.assertIsNone(overlay.native_render_cache)
        third = overlay.native_render_signature()
        self.assertIsNot(first, third)
        self.assertEqual([probe.calls for probe in probes], [2, 2])

    def test_feature_flags_use_cached_index_and_preserve_duplicate_ids(self) -> None:
        feature_ids = ("a", "b", "a", "c", "d")
        flags = MapRenderer._feature_flags(feature_ids, {"a", "c", "missing"}, len(feature_ids))
        np.testing.assert_array_equal(flags, np.array([1, 0, 1, 1, 0], dtype=np.uint8))

        empty = MapRenderer._feature_flags(feature_ids, set(), len(feature_ids))
        np.testing.assert_array_equal(empty, np.zeros(len(feature_ids), dtype=np.uint8))

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
