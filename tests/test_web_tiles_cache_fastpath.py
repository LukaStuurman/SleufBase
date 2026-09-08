from __future__ import annotations

from datetime import datetime, timedelta, timezone
import os
from pathlib import Path
import tempfile
import unittest
from unittest import mock

from PIL import Image

from SleufBase.models import Bounds
from SleufBase.web_tiles import WebMercatorTileClient


class _TileClient(WebMercatorTileClient):
    def build_tile_url(self, zoom: int, x: int, y: int) -> str:
        return f"https://example.invalid/{zoom}/{x}/{y}.png"


class WebTileCacheFastPathTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp_dir = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp_dir.cleanup)
        self.env_patch = mock.patch.dict(os.environ, {"LOCALAPPDATA": self.temp_dir.name})
        self.env_patch.start()
        self.addCleanup(self.env_patch.stop)
        self.client = _TileClient(
            "performance-test",
            "SleufBase performance test",
            min_cache_ttl_days=7,
            max_workers=4,
        )

    @staticmethod
    def _request(coords: list[tuple[int, int]]) -> dict[str, object]:
        min_x = min(x for x, _ in coords)
        max_x = max(x for x, _ in coords)
        min_y = min(y for _, y in coords)
        max_y = max(y for _, y in coords)
        return {
            "mercator_bounds": Bounds(0.0, 0.0, 1.0, 1.0),
            "zoom": 4,
            "resolution": 1.0,
            "tile_span": 256.0,
            "min_tile_x": min_x,
            "max_tile_x": max_x,
            "min_tile_y": min_y,
            "max_tile_y": max_y,
            "tile_coords": coords,
            "stitched_size": ((max_x - min_x + 1) * 256, (max_y - min_y + 1) * 256),
            "target_size": ((max_x - min_x + 1) * 256, (max_y - min_y + 1) * 256),
        }

    @staticmethod
    def _image(color: tuple[int, int, int, int]) -> Image.Image:
        return Image.new("RGBA", (256, 256), color)

    def _remember(self, coord: tuple[int, int], color: tuple[int, int, int, int], *, age_days: int = 0) -> None:
        image = self._image(color)
        try:
            cached_at = datetime.now(timezone.utc) - timedelta(days=age_days)
            self.client._remember_tile((4, coord[0], coord[1]), image, cached_at=cached_at)
        finally:
            image.close()

    def test_all_fresh_exact_hits_skip_fetch_workers_entirely(self) -> None:
        request = self._request([(0, 0), (1, 0)])
        self._remember((0, 0), (255, 0, 0, 255))
        self._remember((1, 0), (0, 255, 0, 255))

        with mock.patch.object(self.client, "_prepare_tile_request", return_value=request), mock.patch.object(
            self.client, "_render_tile_request", side_effect=lambda _request, stitched: stitched.copy()
        ), mock.patch.object(
            self.client, "_fetch_tile", side_effect=AssertionError("fresh cache hit must not be submitted")
        ) as fetch_tile:
            result = self.client.fetch_map(Bounds(0.0, 0.0, 1.0, 1.0), (512, 256))
        try:
            fetch_tile.assert_not_called()
            self.assertEqual(result.getpixel((128, 128)), (255, 0, 0, 255))
            self.assertEqual(result.getpixel((384, 128)), (0, 255, 0, 255))
        finally:
            result.close()

    def test_mixed_cache_submits_only_missing_coordinates(self) -> None:
        request = self._request([(0, 0), (1, 0), (2, 0)])
        self._remember((0, 0), (255, 0, 0, 255))
        self._remember((2, 0), (0, 0, 255, 255))
        fetched_coords: list[tuple[int, int]] = []

        def fetch_tile(zoom: int, x: int, y: int) -> Image.Image:
            self.assertEqual(zoom, 4)
            fetched_coords.append((x, y))
            return self._image((0, 255, 0, 255))

        with mock.patch.object(self.client, "_prepare_tile_request", return_value=request), mock.patch.object(
            self.client, "_render_tile_request", side_effect=lambda _request, stitched: stitched.copy()
        ), mock.patch.object(self.client, "_best_available_tile", return_value=None), mock.patch.object(
            self.client, "_fetch_tile", side_effect=fetch_tile
        ):
            result = self.client.fetch_map(Bounds(0.0, 0.0, 1.0, 1.0), (768, 256))
        try:
            self.assertEqual(fetched_coords, [(1, 0)])
            self.assertEqual(result.getpixel((128, 128)), (255, 0, 0, 255))
            self.assertEqual(result.getpixel((384, 128)), (0, 255, 0, 255))
            self.assertEqual(result.getpixel((640, 128)), (0, 0, 255, 255))
        finally:
            result.close()

    def test_stale_memory_tile_is_previewed_but_still_refreshed(self) -> None:
        request = self._request([(0, 0)])
        self._remember((0, 0), (255, 0, 0, 255), age_days=10)
        fetched_coords: list[tuple[int, int]] = []
        progress_pixels: list[tuple[int, int, int, int]] = []

        def fetch_tile(zoom: int, x: int, y: int) -> Image.Image:
            fetched_coords.append((x, y))
            return self._image((0, 255, 0, 255))

        def progress(image: Image.Image) -> None:
            try:
                progress_pixels.append(image.getpixel((128, 128)))
            finally:
                image.close()

        with mock.patch.object(self.client, "_prepare_tile_request", return_value=request), mock.patch.object(
            self.client, "_render_tile_request", side_effect=lambda _request, stitched: stitched.copy()
        ), mock.patch.object(self.client, "_fetch_tile", side_effect=fetch_tile):
            result = self.client.fetch_map(
                Bounds(0.0, 0.0, 1.0, 1.0),
                (256, 256),
                on_progress=progress,
            )
        try:
            self.assertEqual(fetched_coords, [(0, 0)])
            self.assertTrue(progress_pixels)
            self.assertEqual(progress_pixels[0], (255, 0, 0, 255))
            self.assertEqual(result.getpixel((128, 128)), (0, 255, 0, 255))
        finally:
            result.close()

    def test_stale_memory_entry_is_not_returned_as_fresh(self) -> None:
        self._remember((0, 0), (255, 0, 0, 255), age_days=10)
        self.assertIsNone(self.client._load_cached_tile(4, 0, 0, allow_stale=False))
        stale = self.client._load_cached_tile(4, 0, 0, allow_stale=True)
        self.assertIsNotNone(stale)
        if stale is not None:
            stale.close()


if __name__ == "__main__":
    unittest.main()
