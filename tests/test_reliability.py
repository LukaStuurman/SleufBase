from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
import os
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch

from PIL import Image
import requests

# GitHub checks out this repository as <workspace>/SleufBase. Add the parent so
# imports use the same package layout as the packaged desktop application.
REPO_ROOT = Path(__file__).resolve().parents[1]
PACKAGE_PARENT = REPO_ROOT.parent
if str(PACKAGE_PARENT) not in sys.path:
    sys.path.insert(0, str(PACKAGE_PARENT))

from SleufBase.ahn import PdokAhnClient
from SleufBase.bgt_roadpart import BgtRoadPartClient, BgtRoadPartError
from SleufBase.bgt_terrain_boundary import BgtTerrainBoundaryClient, BgtTerrainBoundaryError
from SleufBase.bgt_vector_tiles import BgtVectorTileClient
from SleufBase.cadastral_wfs import CadastralWfsClient, CadastralWfsError
from SleufBase.geotiff import GeoTiffError, MAX_GEOTIFF_PIXELS, load_geotiff
from SleufBase.location_search import PdokLocationClient
from SleufBase.models import Bounds, GeoTransform, ViewportTransform
from SleufBase.pdok import PdokWmsClient
from SleufBase.road_centerline import RoadCenterlineClient, RoadCenterlineError
from SleufBase.web_tiles import WebMercatorTileClient


class DummyTileClient(WebMercatorTileClient):
    def build_tile_url(self, zoom: int, x: int, y: int) -> str:
        return f"https://example.invalid/{zoom}/{x}/{y}.png"


class CadastralWfsReliabilityTests(unittest.TestCase):
    def test_repeated_full_page_is_detected(self) -> None:
        client = CadastralWfsClient(page_size=50, retries=1)
        self.addCleanup(client.close)
        page = [{"id": f"feature-{index}", "geometry": None} for index in range(50)]
        client._get_json = Mock(return_value={"features": page})  # type: ignore[method-assign]

        with self.assertRaisesRegex(CadastralWfsError, "herhaalt dezelfde pagina"):
            client._fetch_features_direct("test:Feature", Bounds(0.0, 0.0, 10.0, 10.0))

        self.assertEqual(client._get_json.call_count, 2)  # type: ignore[attr-defined]

    def test_feature_cache_never_exceeds_limit(self) -> None:
        client = CadastralWfsClient(retries=1)
        self.addCleanup(client.close)
        for index in range(client.CACHE_LIMIT + 25):
            key = ("test:Feature", float(index), 0.0, float(index + 1), 1.0)
            client._cache_put(key, [{"id": str(index)}])

        self.assertEqual(len(client._feature_cache), client.CACHE_LIMIT)
        self.assertNotIn(("test:Feature", 0.0, 0.0, 1.0, 1.0), client._feature_cache)


class TransformReliabilityTests(unittest.TestCase):
    def test_degenerate_viewport_is_rejected(self) -> None:
        transform = ViewportTransform(Bounds(5.0, 5.0, 5.0, 10.0), 100, 100)
        with self.assertRaisesRegex(ValueError, "positieve breedte"):
            transform.world_to_screen(5.0, 7.0)

    def test_singular_geotransform_is_rejected(self) -> None:
        transform = GeoTransform(a=1.0, b=2.0, c=0.0, d=2.0, e=4.0, f=0.0)
        with self.assertRaisesRegex(ValueError, "singulier"):
            transform.world_to_pixel(10.0, 20.0)


class GeoTiffReliabilityTests(unittest.TestCase):
    def test_global_pillow_limit_is_not_disabled(self) -> None:
        self.assertEqual(Image.MAX_IMAGE_PIXELS, MAX_GEOTIFF_PIXELS)
        self.assertGreater(MAX_GEOTIFF_PIXELS, 0)

    def test_oversized_tiff_is_rejected_and_closed(self) -> None:
        class FakeImage:
            width = 20_000
            height = 10_000
            tag_v2 = {}

            def __init__(self) -> None:
                self.closed = False

            def close(self) -> None:
                self.closed = True

        fake = FakeImage()
        with patch("SleufBase.geotiff.Image.open", return_value=fake):
            with self.assertRaisesRegex(GeoTiffError, "te groot"):
                load_geotiff("oversized.tif")
        self.assertTrue(fake.closed)


class TileCacheReliabilityTests(unittest.TestCase):
    def test_atomic_tile_write_produces_valid_png(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            with patch.dict(os.environ, {"LOCALAPPDATA": temp_dir}):
                client = DummyTileClient("test-cache", "SleufBaseTests/1.0")
                path = client._tile_path(1, 2, 3)
                image = Image.new("RGBA", (256, 256), (10, 20, 30, 255))
                client._write_tile_atomically(path, image)

                with Image.open(path) as written:
                    self.assertEqual(written.size, (256, 256))
                    self.assertEqual(written.convert("RGBA").getpixel((0, 0)), (10, 20, 30, 255))
                self.assertEqual(list(path.parent.glob("*.tmp")), [])

    def test_memory_cache_remains_bounded_under_parallel_writes(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            with patch.dict(os.environ, {"LOCALAPPDATA": temp_dir}):
                client = DummyTileClient(
                    "parallel-cache",
                    "SleufBaseTests/1.0",
                    memory_cache_limit=64,
                )
                image = Image.new("RGBA", (256, 256), (1, 2, 3, 255))
                with ThreadPoolExecutor(max_workers=8) as executor:
                    futures = [
                        executor.submit(client._remember_tile, (1, index, 0), image)
                        for index in range(200)
                    ]
                    for future in futures:
                        future.result()

                self.assertLessEqual(len(client._memory_cache), client.memory_cache_limit)


class NetworkConcurrencyReliabilityTests(unittest.TestCase):
    def test_bgt_cache_is_bounded_under_parallel_writes(self) -> None:
        client = BgtVectorTileClient(max_workers=8)
        with ThreadPoolExecutor(max_workers=8) as executor:
            futures = [
                executor.submit(client._cache_put, (index, 0), {"tile": {"features": []}})
                for index in range(client.TILE_CACHE_LIMIT + 100)
            ]
            for future in futures:
                future.result()
        self.assertLessEqual(len(client._tile_cache), client.TILE_CACHE_LIMIT)

    def test_bgt_sessions_are_not_shared_between_threads(self) -> None:
        client = BgtVectorTileClient()
        main_session = client._get_session()
        with ThreadPoolExecutor(max_workers=1) as executor:
            worker_session = executor.submit(client._get_session).result()
        self.assertIsNot(main_session, worker_session)
        main_session.close()
        worker_session.close()

    def test_ahn_cache_is_bounded_under_parallel_writes(self) -> None:
        client = PdokAhnClient()
        with ThreadPoolExecutor(max_workers=8) as executor:
            futures = [
                executor.submit(client._cache_put, (float(index), 0.0, client.coverage_id), float(index))
                for index in range(client.CACHE_LIMIT + 100)
            ]
            for future in futures:
                future.result()
        self.assertLessEqual(len(client._cache), client.CACHE_LIMIT)

    def test_location_client_retries_transient_failure(self) -> None:
        client = PdokLocationClient(retries=2)
        session = Mock()
        good_response = Mock()
        good_response.raise_for_status.return_value = None
        good_response.json.return_value = {"response": {"docs": []}}
        session.get.side_effect = [requests.ConnectionError("temporary"), good_response]
        client._get_session = Mock(return_value=session)  # type: ignore[method-assign]

        with patch("SleufBase.location_search.time.sleep", return_value=None):
            self.assertEqual(client.search("Utrecht"), [])
        self.assertEqual(session.get.call_count, 2)

    def test_pdok_wms_cache_is_bounded_under_parallel_writes(self) -> None:
        client = PdokWmsClient()
        images = [Image.new("RGBA", (16, 16), (index % 255, 0, 0, 255)) for index in range(80)]
        try:
            with ThreadPoolExecutor(max_workers=8) as executor:
                futures = []
                for index, image in enumerate(images):
                    key = (float(index), 0.0, float(index + 1), 1.0, 16, 16)
                    futures.append(executor.submit(client._cache_put, key, image))
                for future in futures:
                    future.result()
            self.assertLessEqual(len(client._cache), client.CACHE_LIMIT)
        finally:
            for image in images:
                image.close()
            with client._cache_lock:
                for cached in client._cache.values():
                    cached.close()
                client._cache.clear()


class OgcPaginationReliabilityTests(unittest.TestCase):
    def test_repeated_next_link_is_detected_for_all_ogc_clients(self) -> None:
        cases = [
            (BgtRoadPartClient(retries=1), BgtRoadPartError, "BGT wegdelen"),
            (BgtTerrainBoundaryClient(retries=1), BgtTerrainBoundaryError, "BGT terreinserver"),
            (RoadCenterlineClient(retries=1), RoadCenterlineError, "Wegdeel-hartlijnen"),
        ]
        bounds = Bounds(100000.0, 400000.0, 100010.0, 400010.0)
        for client, error_type, expected_text in cases:
            with self.subTest(client=type(client).__name__):
                first_url = f"{client.BASE_URL}/collections/{client.COLLECTION_ID}/items"
                client._get_json = Mock(  # type: ignore[method-assign]
                    return_value={
                        "features": [],
                        "links": [{"rel": "next", "href": first_url}],
                    }
                )
                with self.assertRaisesRegex(error_type, expected_text):
                    client.fetch_paths(bounds)
                self.assertEqual(client._get_json.call_count, 1)  # type: ignore[attr-defined]


if __name__ == "__main__":
    unittest.main()
