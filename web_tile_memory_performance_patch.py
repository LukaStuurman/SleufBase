from __future__ import annotations

from datetime import datetime, timezone
from typing import Any

from PIL import Image

from .web_tiles import WebMercatorTileClient


PATCH_VERSION = 1
_TILE_SIZE = (256, 256)


def _image_from_cached_rgba(data: bytes) -> Image.Image:
    """Create a disposable PIL view over immutable cached RGBA bytes."""
    return Image.frombuffer("RGBA", _TILE_SIZE, data, "raw", "RGBA", 0, 1)


def install_web_tile_memory_performance_patch() -> None:
    current = int(
        getattr(WebMercatorTileClient, "_sleufbase_tile_buffer_cache_version", 0) or 0
    )
    if current >= PATCH_VERSION:
        return

    original_load = WebMercatorTileClient._load_cached_tile

    def _remember_tile_buffered(
        self,
        cache_key: tuple[int, int, int],
        tile: Image.Image,
        *,
        cached_at: datetime | None = None,
    ) -> None:
        timestamp = cached_at or datetime.now(timezone.utc)
        if timestamp.tzinfo is None:
            timestamp = timestamp.replace(tzinfo=timezone.utc)

        rgba = tile if tile.mode == "RGBA" else tile.convert("RGBA")
        try:
            data = rgba.tobytes()
        finally:
            if rgba is not tile:
                rgba.close()

        with self._memory_cache_lock:
            previous = self._memory_cache.pop(cache_key, None)
            if previous is not None:
                previous_value = previous[0]
                if isinstance(previous_value, Image.Image):
                    try:
                        previous_value.close()
                    except Exception:
                        pass
            self._memory_cache[cache_key] = (data, timestamp)
            self._memory_cache.move_to_end(cache_key)
            while len(self._memory_cache) > self.memory_cache_limit:
                _old_key, (old_value, _old_timestamp) = self._memory_cache.popitem(last=False)
                if isinstance(old_value, Image.Image):
                    try:
                        old_value.close()
                    except Exception:
                        pass

    def _load_cached_tile_buffered(
        self,
        zoom: int,
        x: int,
        y: int,
        *,
        allow_stale: bool,
    ) -> Image.Image | None:
        cache_key = (zoom, x, y)
        now = datetime.now(timezone.utc)
        stale_buffer_entry: tuple[bytes, datetime] | None = None

        with self._memory_cache_lock:
            cached_entry = self._memory_cache.get(cache_key)
            if cached_entry is not None:
                cached_value, cached_at = cached_entry
                age = now - cached_at
                if isinstance(cached_value, bytes):
                    if allow_stale or age <= self.min_cache_ttl:
                        self._memory_cache.move_to_end(cache_key)
                        return _image_from_cached_rgba(cached_value)
                    # The legacy disk path must not see a bytes entry because it
                    # expects PIL.Image.copy(). Temporarily remove only this stale
                    # entry and restore it if no fresher disk tile is available.
                    stale_buffer_entry = (cached_value, cached_at)
                    self._memory_cache.pop(cache_key, None)
                elif isinstance(cached_value, Image.Image):
                    if allow_stale or age <= self.min_cache_ttl:
                        self._memory_cache.move_to_end(cache_key)
                        return cached_value.copy()

        result = original_load(self, zoom, x, y, allow_stale=allow_stale)
        if result is None and stale_buffer_entry is not None:
            with self._memory_cache_lock:
                if cache_key not in self._memory_cache:
                    self._memory_cache[cache_key] = stale_buffer_entry
                    self._memory_cache.move_to_end(cache_key)
        return result

    WebMercatorTileClient._remember_tile = _remember_tile_buffered
    WebMercatorTileClient._load_cached_tile = _load_cached_tile_buffered
    WebMercatorTileClient._sleufbase_tile_buffer_cache_version = PATCH_VERSION
    WebMercatorTileClient.SLEUFBASE_IMMUTABLE_TILE_BUFFER_CACHE = True
