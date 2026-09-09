from __future__ import annotations

from datetime import datetime, timedelta, timezone
import os
import tempfile
import unittest
from unittest import mock

from PIL import Image

from SleufBase.web_tiles import WebMercatorTileClient


class _TileClient(WebMercatorTileClient):
    def build_tile_url(self, zoom: int, x: int, y: int) -> str:
        return f"https://example.invalid/{zoom}/{x}/{y}.png"


class WebTileMemoryPerformancePatchTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp_dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp_dir.cleanup)
        self.env_patch = mock.patch.dict(os.environ, {"LOCALAPPDATA": self.temp_dir.name})
        self.env_patch.start()
        self.addCleanup(self.env_patch.stop)
        self.client = _TileClient("buffer-test", "SleufBase buffer test")

    def test_patch_is_installed(self) -> None:
        self.assertGreaterEqual(
            int(getattr(WebMercatorTileClient, "_sleufbase_tile_buffer_cache_version", 0) or 0),
            1,
        )
        self.assertTrue(WebMercatorTileClient.SLEUFBASE_IMMUTABLE_TILE_BUFFER_CACHE)

    def test_memory_cache_stores_bytes_and_disposable_views_keep_pixels(self) -> None:
        source = Image.new("RGBA", (256, 256), (10, 20, 30, 255))
        try:
            self.client._remember_tile((4, 1, 2), source)
        finally:
            source.close()

        cached_value, _timestamp = self.client._memory_cache[(4, 1, 2)]
        self.assertIsInstance(cached_value, bytes)
        self.assertEqual(len(cached_value), 256 * 256 * 4)

        first = self.client._load_cached_tile(4, 1, 2, allow_stale=False)
        self.assertIsNotNone(first)
        if first is not None:
            self.assertEqual(first.getpixel((100, 100)), (10, 20, 30, 255))
            first.close()

        second = self.client._load_cached_tile(4, 1, 2, allow_stale=False)
        self.assertIsNotNone(second)
        if second is not None:
            self.assertEqual(second.getpixel((100, 100)), (10, 20, 30, 255))
            second.close()

    def test_stale_buffer_remains_available_after_fresh_lookup_miss(self) -> None:
        source = Image.new("RGBA", (256, 256), (200, 10, 20, 255))
        try:
            self.client._remember_tile(
                (4, 3, 4),
                source,
                cached_at=datetime.now(timezone.utc) - timedelta(days=10),
            )
        finally:
            source.close()

        self.assertIsNone(self.client._load_cached_tile(4, 3, 4, allow_stale=False))
        stale = self.client._load_cached_tile(4, 3, 4, allow_stale=True)
        self.assertIsNotNone(stale)
        if stale is not None:
            self.assertEqual(stale.getpixel((0, 0)), (200, 10, 20, 255))
            stale.close()


if __name__ == "__main__":
    unittest.main()
