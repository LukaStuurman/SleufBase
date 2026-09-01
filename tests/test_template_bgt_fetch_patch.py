from __future__ import annotations

import unittest

from SleufBase.cadastral_export import CadastralDxfExporter
from SleufBase.models import Bounds
from SleufBase.template_bgt_fetch_patch import _LocalBoundsBgtClient


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

    def test_runtime_patch_is_installed(self) -> None:
        self.assertGreaterEqual(
            int(getattr(CadastralDxfExporter, "_sleufbase_bgt_fetch_patch_version", 0) or 0),
            1,
        )


if __name__ == "__main__":
    unittest.main()
