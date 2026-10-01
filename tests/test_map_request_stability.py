from __future__ import annotations

from concurrent.futures import Future, ThreadPoolExecutor
from datetime import datetime, timedelta, timezone
from io import BytesIO
import os
import tempfile
import threading
import unittest
from unittest.mock import Mock, patch

from PIL import Image
import requests

from SleufBase.models import Bounds
from SleufBase.pdok import PdokError, PdokWmsClient
from SleufBase.web_tiles import TileClientError, WebMercatorTileClient


class _TileClient(WebMercatorTileClient):
    def build_tile_url(self, zoom, x, y):
        return f"https://example.invalid/{zoom}/{x}/{y}.png"


def _response(size=(256, 256)):
    with Image.new("RGBA", size, (10, 20, 30, 255)) as image:
        payload = BytesIO()
        image.save(payload, format="PNG")
    return Mock(content=payload.getvalue(), headers={"content-type": "image/png"})


class MapRequestStabilityTests(unittest.TestCase):
    def setUp(self):
        root = tempfile.TemporaryDirectory()
        self.addCleanup(root.cleanup)
        env = patch.dict(os.environ, {"LOCALAPPDATA": root.name})
        env.start()
        self.addCleanup(env.stop)
        self.client = _TileClient("concurrent-test", "test", retries=1)

    def _overlapping_fetches(self, response_or_error):
        entered = threading.Event()
        waiting = threading.Event()
        release = threading.Event()
        self.addCleanup(release.set)

        class WaitingFuture(Future):
            def result(self, timeout=None):
                waiting.set()
                return super().result(timeout=timeout)

        session = Mock()

        def get(*args, **kwargs):
            entered.set()
            if not release.wait(3):
                raise AssertionError("test did not release download")
            if isinstance(response_or_error, Exception):
                raise response_or_error
            return response_or_error

        session.get.side_effect = get
        with patch.object(self.client, "_get_session", return_value=session), patch(
            "SleufBase.web_tiles.Future", WaitingFuture
        ), ThreadPoolExecutor(max_workers=2) as pool:
            first = pool.submit(self.client._fetch_tile, 4, 1, 2)
            self.assertTrue(entered.wait(3))
            second = pool.submit(self.client._fetch_tile, 4, 1, 2)
            try:
                self.assertTrue(waiting.wait(3), "overlapping request must join pending download")
            finally:
                release.set()
            results = []
            for future in (first, second):
                try:
                    results.append(future.result(timeout=3))
                except TileClientError as exc:
                    results.append(exc)
        self.assertEqual(session.get.call_count, 1)
        self.assertEqual(self.client._inflight_tiles, {})
        return results

    def test_overlapping_tile_requests_download_once_and_own_independent_images(self):
        first, second = self._overlapping_fetches(_response())
        self.addCleanup(first.close)
        self.addCleanup(second.close)
        self.assertIsNot(first, second)
        first.putpixel((0, 0), (255, 0, 0, 255))
        first.close()
        self.assertEqual(second.getpixel((0, 0)), (10, 20, 30, 255))

    def test_failed_shared_download_releases_waiters_and_allows_retry(self):
        errors = self._overlapping_fetches(requests.ConnectionError("offline"))
        self.assertTrue(all(isinstance(exc, TileClientError) for exc in errors))
        session = Mock()
        session.get.return_value = _response()
        with patch.object(self.client, "_get_session", return_value=session):
            image = self.client._fetch_tile(4, 1, 2)
        with image:
            self.assertEqual(image.getpixel((0, 0)), (10, 20, 30, 255))
        self.assertEqual(session.get.call_count, 1)

    def test_stale_preview_cache_write_cannot_change_shared_download_pixels(self):
        path = self.client._tile_path(4, 1, 2)
        old_color = (255, 0, 0, 255)
        with Image.new("RGBA", (256, 256), old_color) as image:
            image.save(path)
        timestamp = (datetime.now(timezone.utc) - timedelta(days=10)).timestamp()
        os.utime(path, (timestamp, timestamp))
        old_opened = threading.Event()
        allow_old_remember = threading.Event()
        old_remembered = threading.Event()
        download_entered = threading.Event()
        allow_download = threading.Event()
        waiter_entered = threading.Event()
        self.addCleanup(allow_old_remember.set)
        self.addCleanup(allow_download.set)
        remember = self.client._remember_tile

        class WaitingFuture(Future):
            def result(self, timeout=None):
                waiter_entered.set()
                return super().result(timeout=timeout)

        def delayed_remember(key, tile, **kwargs):
            if tile.getpixel((0, 0)) == old_color:
                old_opened.set()
                self.assertTrue(allow_old_remember.wait(3))
                remember(key, tile, **kwargs)
                old_remembered.set()
            else:
                remember(key, tile, **kwargs)
                # The preview opened old pixels before the atomic disk replace,
                # but inserts them after the download remembered fresh pixels.
                allow_old_remember.set()
                self.assertTrue(old_remembered.wait(3))

        session = Mock()

        def get(*args, **kwargs):
            download_entered.set()
            self.assertTrue(allow_download.wait(3))
            return _response()

        session.get.side_effect = get
        with patch.object(self.client, "_remember_tile", side_effect=delayed_remember), patch.object(
            self.client, "_get_session", return_value=session
        ), patch("SleufBase.web_tiles.Future", WaitingFuture), ThreadPoolExecutor(max_workers=3) as pool:
            preview = pool.submit(self.client._load_cached_tile, 4, 1, 2, allow_stale=True)
            self.assertTrue(old_opened.wait(3))
            owner = pool.submit(self.client._fetch_tile, 4, 1, 2)
            self.assertTrue(download_entered.wait(3))
            waiter = pool.submit(self.client._fetch_tile, 4, 1, 2)
            try:
                self.assertTrue(waiter_entered.wait(3))
            finally:
                allow_download.set()
            images = [future.result(timeout=3) for future in (preview, owner, waiter)]
        try:
            self.assertEqual(images[0].getpixel((0, 0)), old_color)
            self.assertEqual(images[1].getpixel((0, 0)), (10, 20, 30, 255))
            self.assertEqual(images[2].getpixel((0, 0)), (10, 20, 30, 255))
            self.assertEqual(session.get.call_count, 1)
            self.assertEqual(self.client._inflight_tiles, {})
        finally:
            for image in images:
                image.close()

    def test_distinct_tile_requests_run_in_parallel(self):
        barrier = threading.Barrier(2)
        session = Mock()

        def get(*args, **kwargs):
            barrier.wait(timeout=3)
            return _response()

        session.get.side_effect = get
        with patch.object(self.client, "_get_session", return_value=session), ThreadPoolExecutor(max_workers=2) as pool:
            futures = [pool.submit(self.client._fetch_tile, 4, x, 2) for x in (1, 2)]
            for future in futures:
                future.result(timeout=3).close()
        self.assertEqual(session.get.call_count, 2)

    def test_wrong_tile_dimensions_are_rejected_before_decoding(self):
        session = Mock()
        session.get.return_value = _response((512, 512))
        with patch.object(self.client, "_get_session", return_value=session), patch.object(
            Image.Image, "convert", side_effect=AssertionError("wrong dimensions must not be decoded")
        ):
            with self.assertRaisesRegex(TileClientError, "256x256 verwacht"):
                self.client._fetch_tile(4, 1, 2)
        self.assertFalse(self.client._tile_path(4, 1, 2).exists())

    def test_wrong_disk_tile_dimensions_are_removed_before_decoding(self):
        path = self.client._tile_path(4, 1, 2)
        with Image.new("RGBA", (512, 512)) as image:
            image.save(path)
        with patch.object(Image.Image, "convert", side_effect=AssertionError("wrong dimensions must not be decoded")):
            self.assertIsNone(self.client._load_cached_tile(4, 1, 2, allow_stale=True))
        self.assertFalse(path.exists())

    def test_wrong_wms_dimensions_are_retried_and_never_cached(self):
        client = PdokWmsClient(retries=2)
        session = Mock()
        session.get.return_value = _response((16, 16))
        with patch.object(client, "_get_session", return_value=session), patch(
            "SleufBase.pdok.time.sleep"
        ), patch.object(Image.Image, "convert", side_effect=AssertionError("wrong dimensions must not be decoded")):
            with self.assertRaisesRegex(PdokError, "kaartgrootte"):
                client.fetch_map(Bounds(0, 0, 1, 1), (32, 32))
        self.assertEqual(session.get.call_count, 2)
        self.assertEqual(client._cache, {})

    def test_transient_wrong_wms_dimensions_recover_with_exact_image(self):
        client = PdokWmsClient(retries=2)
        session = Mock()
        session.get.side_effect = [_response((16, 16)), _response((32, 32))]
        with patch.object(client, "_get_session", return_value=session), patch("SleufBase.pdok.time.sleep"):
            image = client.fetch_map(Bounds(0, 0, 1, 1), (32, 32))
        with image:
            self.assertEqual(image.size, (32, 32))
            self.assertEqual(image.getpixel((0, 0)), (10, 20, 30, 255))
        self.assertEqual(session.get.call_count, 2)
        self.assertEqual(len(client._cache), 1)


if __name__ == "__main__":
    unittest.main()
