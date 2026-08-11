from __future__ import annotations

from .web_tiles import WebMercatorTileClient


class OpenStreetMapTileClient(WebMercatorTileClient):
    TILE_URL = "https://tile.openstreetmap.org/{z}/{x}/{y}.png"

    def __init__(
        self,
        timeout: int = 30,
        min_zoom: int = 0,
        max_zoom: int = 19,
        max_workers: int = 8,
        retries: int = 3,
    ) -> None:
        super().__init__(
            cache_namespace="osm",
            user_agent="SleufBase/1.3",
            timeout=timeout,
            min_zoom=min_zoom,
            max_zoom=max_zoom,
            min_cache_ttl_days=7,
            max_workers=max_workers,
            retries=retries,
        )

    def build_tile_url(self, zoom: int, x: int, y: int) -> str:
        return self.TILE_URL.format(z=zoom, x=x, y=y)
