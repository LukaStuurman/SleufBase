from __future__ import annotations

from pathlib import Path
import unittest
from unittest import mock

from SleufBase.models import Bounds, CableFeature, DxfOverlay
from SleufBase import renderer


def _feature(feature_id: str, y: float, *, display: str) -> CableFeature:
    return CableFeature(
        feature_id=feature_id,
        source_path=Path(f"{feature_id}.dxf"),
        points=[(0.0, y), (10.0, y)],
        bounds=Bounds(0.0, y, 10.0, y),
        color=(1, 2, 3),
        metadata={"Laag": display},
    )


def _reference_pick_features(x, y, overlays, tolerance):
    matches = []
    for overlay in overlays:
        if not overlay.visible:
            continue
        for feature in overlay.features:
            if not feature.bounds.padded(tolerance).contains(x, y):
                continue
            distance = feature.distance_to(x, y)
            if distance <= tolerance:
                matches.append(
                    (
                        distance,
                        feature.display_name.lower(),
                        feature.source_path.name.lower(),
                        feature.feature_id,
                        feature,
                    )
                )
    matches.sort(key=lambda item: item[:4])
    return [item[4] for item in matches]


class PickFeaturePerformanceTests(unittest.TestCase):
    def setUp(self) -> None:
        self.a = _feature("a", 0.0, display="B")
        self.b = _feature("b", 0.1, display="A")
        self.c = _feature("c", 5.0, display="C")
        self.hidden = DxfOverlay(Path("hidden.dxf"), [self.c], visible=False)
        self.visible = DxfOverlay(Path("visible.dxf"), [self.a, self.b, self.c], visible=True)

    def test_pick_features_preserves_reference_order(self) -> None:
        expected = _reference_pick_features(5.0, 0.05, [self.hidden, self.visible], 0.2)
        actual = renderer.pick_features(5.0, 0.05, [self.hidden, self.visible], 0.2)
        self.assertEqual([item.feature_id for item in actual], [item.feature_id for item in expected])

    def test_pick_feature_returns_reference_best_candidate(self) -> None:
        expected = _reference_pick_features(5.0, 0.05, [self.visible], 0.2)[0]
        actual = renderer.pick_feature(5.0, 0.05, [self.visible], 0.2)
        self.assertIs(actual, expected)

    def test_core_picker_does_not_allocate_padded_bounds(self) -> None:
        with mock.patch.object(
            Bounds,
            "padded",
            side_effect=AssertionError("core hit test must use scalar bounds"),
        ):
            result = renderer.pick_feature(5.0, 0.05, [self.visible], 0.2)
        self.assertIsNotNone(result)

    def test_pick_feature_does_not_sort_all_matches(self) -> None:
        with mock.patch.object(
            renderer,
            "pick_features",
            side_effect=AssertionError("single pick must not build/sort the full match list"),
        ):
            result = renderer.pick_feature(5.0, 0.05, [self.visible], 0.2)
        self.assertIsNotNone(result)


if __name__ == "__main__":
    unittest.main()
