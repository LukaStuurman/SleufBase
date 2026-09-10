from __future__ import annotations

from .resource_policy import get_resource_policy
from .web_tiles import WebMercatorTileClient


_RESOURCE_POLICY = get_resource_policy()


class OpenStreetMapTileClient(WebMercatorTileClient):
    TILE_URL = "https://tile.openstreetmap.org/{z}/{x}/{y}.png"

    def __init__(
        self,
        timeout: int = 30,
        min_zoom: int = 0,
        max_zoom: int = 19,
        max_workers: int | None = None,
        retries: int = 3,
    ) -> None:
        # Keep the historical eight-request ceiling for the public OSM service,
        # but scale down automatically on small laptops.
        resolved_workers = (
            max(1, min(8, _RESOURCE_POLICY.network_workers))
            if max_workers is None
            else max(1, int(max_workers))
        )
        super().__init__(
            cache_namespace="osm",
            user_agent="SleufBase/1.3",
            timeout=timeout,
            min_zoom=min_zoom,
            max_zoom=max_zoom,
            min_cache_ttl_days=7,
            max_workers=resolved_workers,
            memory_cache_limit=_RESOURCE_POLICY.tile_memory_cache_entries,
            retries=retries,
        )

    def build_tile_url(self, zoom: int, x: int, y: int) -> str:
        return self.TILE_URL.format(z=zoom, x=x, y=y)
