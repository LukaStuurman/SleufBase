from __future__ import annotations

import unittest
from unittest.mock import patch

from PIL import Image

from SleufBase.models import Bounds
from SleufBase.pdok import PdokWmtsTileClient
from SleufBase import pdok_high_quality_patch as quality_patch


class _FakeWmsClient:
    created: list["_FakeWmsClient"] = []

    def __init__(
        self,
        layer_name: str,
        timeout: int,
        retries: int,
        max_workers: int,
        transparent: bool,
    ) -> None:
        self.layer_name = layer_name
        self.timeout = timeout
        self.retries = retries
        self.max_workers = max_workers
        self.transparent = transparent
        self.fetch_calls: list[tuple[Bounds, tuple[int, int], int, object]] = []
        self.cached_image: Image.Image | None = None
        self.__class__.created.append(self)

    def fetch_map(self, bounds, size, max_tile_size=2048, on_progress=None):
        self.fetch_calls.append((bounds, size, max_tile_size, on_progress))
        image = Image.new("RGBA", size, (10, 20, 30, 255))
        self.cached_image = image.copy()
        return image

    @staticmethod
    def _cache_key(bounds, size):
        return (bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y, *size)

    def _cache_get(self, _key):
        return self.cached_image.copy() if self.cached_image is not None else None


class PdokHighQualityPatchTests(unittest.TestCase):
    def setUp(self) -> None:
        _FakeWmsClient.created.clear()

    @staticmethod
    def _client(layer_name: str = "Actueel_orthoHR") -> PdokWmtsTileClient:
        # Avoid touching the real tile cache/network; the quality patch only
        # needs these provider settings to construct its WMS counterpart.
        client = object.__new__(PdokWmtsTileClient)
        client.layer_name = layer_name
        client.timeout = 30
        client.retries = 3
        client.max_workers = 8
        return client

    def test_runtime_patch_is_installed(self) -> None:
        self.assertGreaterEqual(
            int(getattr(PdokWmtsTileClient, "_sleufbase_high_quality_patch_version", 0) or 0),
            1,
        )
        self.assertEqual(
            PdokWmtsTileClient.SLEUFBASE_HIGH_RESOLUTION_WMS_MAX_TILE_SIZE,
            quality_patch.HIGH_RESOLUTION_WMS_MAX_TILE_SIZE,
        )

    def test_actueel_hr_final_image_uses_exact_size_wms_path(self) -> None:
        client = self._client()
        bounds = Bounds(100000.0, 450000.0, 100080.0, 450060.0)
        progress = lambda _image: None

        with patch.object(quality_patch, "PdokWmsClient", _FakeWmsClient):
            image = client.fetch_map(bounds, (1600, 1200), on_progress=progress)
            try:
                self.assertEqual(image.size, (1600, 1200))
                self.assertEqual(len(_FakeWmsClient.created), 1)
                wms = _FakeWmsClient.created[0]
                self.assertEqual(wms.layer_name, "Actueel_orthoHR")
                self.assertEqual(len(wms.fetch_calls), 1)
                call_bounds, call_size, max_tile_size, call_progress = wms.fetch_calls[0]
                self.assertEqual(call_bounds, bounds)
                self.assertEqual(call_size, (1600, 1200))
                self.assertEqual(max_tile_size, quality_patch.HIGH_RESOLUTION_WMS_MAX_TILE_SIZE)
                self.assertIs(call_progress, progress)
            finally:
                image.close()

    def test_cached_wms_result_is_preferred_for_preview(self) -> None:
        client = self._client()
        bounds = Bounds(100000.0, 450000.0, 100040.0, 450030.0)

        with patch.object(quality_patch, "PdokWmsClient", _FakeWmsClient):
            rendered = client.fetch_map(bounds, (800, 600))
            rendered.close()
            preview = client.preview_map(bounds, (800, 600))
            try:
                self.assertIsNotNone(preview)
                assert preview is not None
                self.assertEqual(preview.size, (800, 600))
                self.assertEqual(preview.getpixel((0, 0)), (10, 20, 30, 255))
                self.assertEqual(len(_FakeWmsClient.created), 1)
            finally:
                if preview is not None:
                    preview.close()

    def test_only_hr_orthophoto_is_forced_to_wms(self) -> None:
        self.assertTrue(quality_patch._uses_high_resolution_aerial_layer(self._client("Actueel_orthoHR")))
        self.assertTrue(quality_patch._uses_high_resolution_aerial_layer(self._client("2025_orthoHR")))
        self.assertFalse(quality_patch._uses_high_resolution_aerial_layer(self._client("Actueel_ortho25")))


if __name__ == "__main__":
    unittest.main()
