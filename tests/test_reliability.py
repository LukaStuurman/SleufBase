from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
import os
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch

from PIL import Image

# GitHub checks out this repository as <workspace>/SleufBase. Add the parent so
# imports use the same package layout as the packaged desktop application.
REPO_ROOT = Path(__file__).resolve().parents[1]
PACKAGE_PARENT = REPO_ROOT.parent
if str(PACKAGE_PARENT) not in sys.path:
    sys.path.insert(0, str(PACKAGE_PARENT))

from SleufBase.cadastral_wfs import CadastralWfsClient, CadastralWfsError
from SleufBase.geotiff import GeoTiffError, MAX_GEOTIFF_PIXELS, load_geotiff
from SleufBase.models import Bounds, GeoTransform, ViewportTransform
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


if __name__ == "__main__":
    unittest.main()
