from __future__ import annotations

import unittest

from SleufBase.cadastral_export import CadastralDxfExporter
from SleufBase.cadastral_wfs import CadastralLinework, CadastralTextLabel
from SleufBase.models import Bounds
from SleufBase.template_bgt_fetch_patch import _LocalBoundsBgtClient, _LocalBoundsWfsClient


class _RecordingBgtClient:
    def __init__(self) -> None:
        self.path_calls: list[Bounds] = []
        self.surface_calls: list[Bounds] = []

    def fetch_paths(self, bounds: Bounds):
        self.path_calls.append(bounds)
        # Same first path on every request verifies cross-area deduplication.
        return [
            [(1.0, 1.0), (2.0, 2.0)],
            [(bounds.min_x, bounds.min_y), (bounds.max_x, bounds.max_y)],
        ]

    def fetch_surface_features(self, bounds: Bounds):
        self.surface_calls.append(bounds)
        return []


class _RecordingWfsClient:
    def __init__(self) -> None:
        self.boundary_calls: list[Bounds] = []
        self.text_calls: list[Bounds] = []

    def fetch_parcel_boundaries(self, bounds: Bounds):
        self.boundary_calls.append(bounds)
        return CadastralLinework(
            layer_name="KAD_GRENS",
            paths=[
                [(1.0, 1.0), (2.0, 2.0)],
                [(bounds.min_x, bounds.min_y), (bounds.max_x, bounds.max_y)],
            ],
        )

    def fetch_text_labels(self, bounds: Bounds):
        self.text_calls.append(bounds)
        return [
            CadastralTextLabel(
                layer_name="KAD_STRAATNAAM",
                text="Dubbele straat",
                position=(10.0, 20.0),
                rotation=0.0,
            ),
            CadastralTextLabel(
                layer_name="KAD_HUISNUMMER",
                text=str(int(bounds.min_x)),
                position=(bounds.min_x, bounds.min_y),
                rotation=0.0,
            ),
        ]


class TemplateBgtFetchPatchTests(unittest.TestCase):
    def test_bgt_template_fetch_uses_local_bounds_not_combined_extent(self) -> None:
        local = [
            Bounds(100.0, 200.0, 150.0, 250.0),
            Bounds(5000.0, 6000.0, 5050.0, 6050.0),
        ]
        combined = Bounds(100.0, 200.0, 5050.0, 6050.0)
        delegate = _RecordingBgtClient()
        client = _LocalBoundsBgtClient(delegate, local)

        paths = client.fetch_paths(combined)

        self.assertEqual(delegate.path_calls, local)
        self.assertNotIn(combined, delegate.path_calls)
        self.assertEqual(paths.count([(1.0, 1.0), (2.0, 2.0)]), 1)

    def test_cadastral_template_fetch_uses_local_bounds_not_combined_extent(self) -> None:
        local = [
            Bounds(100.0, 200.0, 150.0, 250.0),
            Bounds(5000.0, 6000.0, 5050.0, 6050.0),
        ]
        combined = Bounds(100.0, 200.0, 5050.0, 6050.0)
        delegate = _RecordingWfsClient()
        client = _LocalBoundsWfsClient(delegate, local)

        boundaries = client.fetch_parcel_boundaries(combined)
        labels = client.fetch_text_labels(combined)

        self.assertEqual(delegate.boundary_calls, local)
        self.assertEqual(delegate.text_calls, local)
        self.assertNotIn(combined, delegate.boundary_calls)
        self.assertNotIn(combined, delegate.text_calls)
        self.assertIsNotNone(boundaries)
        assert boundaries is not None
        self.assertEqual(boundaries.paths.count([(1.0, 1.0), (2.0, 2.0)]), 1)
        self.assertEqual(sum(label.text == "Dubbele straat" for label in labels), 1)

    def test_cadastral_local_fetch_reports_progress_per_area(self) -> None:
        local = [
            Bounds(100.0, 200.0, 150.0, 250.0),
            Bounds(5000.0, 6000.0, 5050.0, 6050.0),
        ]
        statuses: list[str] = []
        client = _LocalBoundsWfsClient(_RecordingWfsClient(), local, status_callback=statuses.append)

        client.fetch_parcel_boundaries(Bounds(100.0, 200.0, 5050.0, 6050.0))

        self.assertEqual(
            statuses,
            [
                "Haal kadastrale perceelgrenzen op... 1/2",
                "Haal kadastrale perceelgrenzen op... 2/2",
            ],
        )

    def test_runtime_patch_is_installed(self) -> None:
        self.assertGreaterEqual(
            int(getattr(CadastralDxfExporter, "_sleufbase_bgt_fetch_patch_version", 0) or 0),
            2,
        )


if __name__ == "__main__":
    unittest.main()
