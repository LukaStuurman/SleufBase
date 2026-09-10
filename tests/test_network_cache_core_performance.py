from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor
import threading
import unittest
from unittest.mock import patch

from PIL import Image

from SleufBase.cadastral_wfs import CadastralWfsClient
from SleufBase.pdok import PdokWmsClient


class _FakeSession:
    def __init__(self) -> None:
        self.headers: dict[str, str] = {}
        self.closed = False

    def close(self) -> None:
        self.closed = True


class NetworkCacheCorePerformanceTests(unittest.TestCase):
    def test_parallel_wfs_sessions_are_core_code_and_worker_local(self) -> None:
        created: list[_FakeSession] = []

        def session_factory() -> _FakeSession:
            session = _FakeSession()
            created.append(session)
            return session

        with patch("SleufBase.cadastral_wfs.requests.Session", side_effect=session_factory):
            client = CadastralWfsClient(retries=1)
            self.assertEqual(CadastralWfsClient.__init__.__module__, "SleufBase.cadastral_wfs")
            self.assertTrue(CadastralWfsClient.SLEUFBASE_PARALLEL_SESSIONS)

            barrier = threading.Barrier(2)

            def worker():
                barrier.wait(timeout=2.0)
                return client._request_session()

            with ThreadPoolExecutor(max_workers=2) as executor:
                first_future = executor.submit(worker)
                second_future = executor.submit(worker)
                first = first_future.result(timeout=3.0)
                second = second_future.result(timeout=3.0)

            self.assertIsNot(first, second)
            self.assertIsNot(first, client.session)
            self.assertIsNot(second, client.session)
            self.assertEqual(first.headers.get("User-Agent"), "SleufBase/0.2")
            self.assertEqual(second.headers.get("User-Agent"), "SleufBase/0.2")
            self.assertEqual(len(client._sleufbase_wfs_worker_sessions), 2)

            client.close()

        self.assertEqual(len(created), 3)
        self.assertTrue(all(session.closed for session in created))

    def test_wms_cache_uses_immutable_buffers_without_image_copy_on_hit(self) -> None:
        client = PdokWmsClient(retries=1)
        key = (0.0, 0.0, 1.0, 1.0, 8, 8)
        image = Image.new("RGBA", (8, 8), (10, 20, 30, 255))
        try:
            client._cache_put(key, image)
        finally:
            image.close()

        payload = client._cache[key]
        self.assertIsInstance(payload[0], bytes)
        self.assertEqual(payload[1], (8, 8))
        self.assertEqual(client._cache_bytes, 8 * 8 * 4)

        with patch.object(Image.Image, "copy", side_effect=AssertionError("cache hit copied image")):
            cached = client._cache_get(key)
        self.assertIsNotNone(cached)
        assert cached is not None
        try:
            self.assertEqual(cached.getpixel((0, 0)), (10, 20, 30, 255))
        finally:
            cached.close()

    def test_wms_cache_evicts_by_byte_budget_as_well_as_item_count(self) -> None:
        client = PdokWmsClient(retries=1)
        client.CACHE_LIMIT = 24
        client.CACHE_MAX_BYTES = 64
        first_key = (0.0, 0.0, 1.0, 1.0, 4, 4)
        second_key = (1.0, 1.0, 2.0, 2.0, 4, 4)

        first = Image.new("RGBA", (4, 4), (1, 2, 3, 255))
        second = Image.new("RGBA", (4, 4), (4, 5, 6, 255))
        try:
            client._cache_put(first_key, first)
            client._cache_put(second_key, second)
        finally:
            first.close()
            second.close()

        self.assertNotIn(first_key, client._cache)
        self.assertIn(second_key, client._cache)
        self.assertEqual(client._cache_bytes, 64)

    def test_wms_cache_does_not_pin_single_image_larger_than_budget(self) -> None:
        client = PdokWmsClient(retries=1)
        client.CACHE_MAX_BYTES = 32
        key = (0.0, 0.0, 1.0, 1.0, 4, 4)
        image = Image.new("RGBA", (4, 4), (1, 2, 3, 255))
        try:
            client._cache_put(key, image)
        finally:
            image.close()

        self.assertNotIn(key, client._cache)
        self.assertEqual(client._cache_bytes, 0)


if __name__ == "__main__":
    unittest.main()
