import unittest

from shapely import make_valid
from shapely.geometry import Polygon, box, LineString
from shapely.ops import unary_union

from SleufBase.bgt_vector_tiles import BgtVectorTileClient, BgtSurfaceFeature
from SleufBase.models import Bounds


class BgtGeometryRepairTests(unittest.TestCase):
    def test_invalid_tile_part_does_not_discard_valid_part_of_same_object(self):
        parts = {
            (0, 0): Polygon([(0, 0), (4, 4), (4, 0), (0, 3), (0, 0)]),
            (0, 1): box(8, 0, 10, 2),
        }
        # Assert retained geometry across GEOS versions. The pinned fixture
        # generator separately proves the old union-before-repair exception.
        expected = unary_union([make_valid(g) for g in parts.values()])
        client = BgtVectorTileClient()
        client._tiles_for_bounds = lambda _: list(parts)
        client._tile_features = lambda row, col: [('wegdeel', 'same-id', parts[row, col])]
        client._tile_surface_features = lambda row, col: [
            BgtSurfaceFeature('wegdeel', 'same-id', 'asfalt', parts[row, col])
        ]
        bounds = Bounds(-1, -1, 11, 7)
        result = client.fetch_surface_features(bounds)
        self.assertEqual(len(result), 1)
        self.assertTrue(result[0].geometry.equals(expected))
        self.assertTrue(result[0].geometry.covers(parts[0, 1]))
        actual_paths = unary_union([LineString(p) for p in client.fetch_paths(bounds)])
        self.assertTrue(actual_paths.equals(expected.boundary))
