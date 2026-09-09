from __future__ import annotations

from pathlib import Path
import unittest
from unittest import mock

from SleufBase.models import Bounds, CableFeature, DxfOverlay
from SleufBase import renderer
from SleufBase.render_cache_performance_patch import (
    _pick_feature_fast,
    _pick_features_fast,
)


def _feature(feature_id: str, y: float, *, display: str) -> CableFeature:
    return CableFeature(
        feature_id=feature_id,
        source_path=Path(f"{feature_id}.dxf"),
        points=[(0.0, y), (10.0, y)],
        bounds=Bounds(0.0, y, 10.0, y),
        color=(1, 2, 3),
        metadata={"Laag": display},
    )


class PickFeaturePerformancePatchTests(unittest.TestCase):
    def setUp(self) -> None:
        self.a = _feature("a", 0.0, display="B")
        self.b = _feature("b", 0.1, display="A")
        self.c = _feature("c", 5.0, display="C")
        self.hidden = DxfOverlay(Path("hidden.dxf"), [self.c], visible=False)
        self.visible = DxfOverlay(Path("visible.dxf"), [self.a, self.b, self.c], visible=True)

    def test_fast_pick_features_matches_original_order(self) -> None:
        original = renderer._sleufbase_original_pick_features
        expected = original(5.0, 0.05, [self.hidden, self.visible], 0.2)
        actual = _pick_features_fast(5.0, 0.05, [self.hidden, self.visible], 0.2)
        self.assertEqual([item.feature_id for item in actual], [item.feature_id for item in expected])

    def test_fast_pick_feature_matches_original_best_candidate(self) -> None:
        original = renderer._sleufbase_original_pick_feature
        expected = original(5.0, 0.05, [self.visible], 0.2)
        actual = _pick_feature_fast(5.0, 0.05, [self.visible], 0.2)
        self.assertIs(actual, expected)

    def test_fast_picker_does_not_allocate_padded_bounds(self) -> None:
        with mock.patch.object(
            Bounds,
            "padded",
            side_effect=AssertionError("fast hit test must use scalar bounds"),
        ):
            result = _pick_feature_fast(5.0, 0.05, [self.visible], 0.2)
        self.assertIsNotNone(result)

    def test_renderer_exports_use_fast_functions(self) -> None:
        self.assertIs(renderer.pick_features, _pick_features_fast)
        self.assertIs(renderer.pick_feature, _pick_feature_fast)


if __name__ == "__main__":
    unittest.main()
