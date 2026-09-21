from __future__ import annotations

import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

REPO_ROOT = Path(__file__).resolve().parents[1]
PACKAGE_PARENT = REPO_ROOT.parent
if str(PACKAGE_PARENT) not in sys.path:
    sys.path.insert(0, str(PACKAGE_PARENT))

from SleufBase.atomic_io import atomic_write_json, atomic_write_text
from SleufBase.web_tiles import WebMercatorTileClient


class _DummyTileClient(WebMercatorTileClient):
    def build_tile_url(self, zoom: int, x: int, y: int) -> str:
        return f"https://example.invalid/{zoom}/{x}/{y}.png"


class AtomicIoTests(unittest.TestCase):
    def test_atomic_write_text_replaces_target_and_cleans_temp_file(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            target = Path(temp_dir) / "state.json"
            target.write_text("old", encoding="utf-8")

            returned = atomic_write_text(target, "new")

            self.assertEqual(returned, target)
            self.assertEqual(target.read_text(encoding="utf-8"), "new")
            self.assertEqual(list(target.parent.glob(f".{target.name}.*.tmp")), [])

    def test_atomic_write_text_preserves_existing_target_when_replace_fails(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            target = Path(temp_dir) / "settings.json"
            target.write_text("stable", encoding="utf-8")

            with mock.patch("SleufBase.atomic_io.os.replace", side_effect=OSError("locked")):
                with self.assertRaises(OSError):
                    atomic_write_text(target, "partial")

            self.assertEqual(target.read_text(encoding="utf-8"), "stable")
            self.assertEqual(list(target.parent.glob(f".{target.name}.*.tmp")), [])

    def test_atomic_write_json_keeps_expected_serialization(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            target = Path(temp_dir) / "manifest.json"
            payload = {"name": "SleufBase", "items": [1, 2]}

            atomic_write_json(target, payload, ensure_ascii=False, indent=2, trailing_newline=True)

            self.assertEqual(
                target.read_text(encoding="utf-8"),
                json.dumps(payload, ensure_ascii=False, indent=2) + "\n",
            )


class DiskCacheQuotaTests(unittest.TestCase):
    def test_disk_cache_prunes_oldest_tiles_to_target_size(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            with mock.patch.dict(os.environ, {"LOCALAPPDATA": temp_dir}, clear=False):
                client = _DummyTileClient(
                    cache_namespace="quota-test",
                    user_agent="SleufBase tests",
                    disk_cache_limit_bytes=100,
                )

            paths = [client.cache_dir / f"tile-{index}.png" for index in range(3)]
            for index, path in enumerate(paths):
                path.write_bytes(bytes([index + 1]) * 40)
                timestamp = 1000 + index * 100
                os.utime(path, (timestamp, timestamp))

            client._prune_disk_cache(force=True)

            self.assertFalse(paths[0].exists())
            self.assertTrue(paths[1].exists())
            self.assertTrue(paths[2].exists())
            remaining_size = sum(path.stat().st_size for path in client.cache_dir.glob("*.png"))
            self.assertLessEqual(remaining_size, 90)

    def test_disk_cache_does_not_prune_when_under_limit(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            with mock.patch.dict(os.environ, {"LOCALAPPDATA": temp_dir}, clear=False):
                client = _DummyTileClient(
                    cache_namespace="quota-under-limit",
                    user_agent="SleufBase tests",
                    disk_cache_limit_bytes=100,
                )

            tile = client.cache_dir / "tile.png"
            tile.write_bytes(b"x" * 60)
            client._prune_disk_cache(force=True)
            self.assertTrue(tile.exists())


if __name__ == "__main__":
    unittest.main()
