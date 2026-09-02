from __future__ import annotations

from typing import Any

from .pdok import PdokWmsClient, PdokWmtsTileClient


PATCH_VERSION = 1
HIGH_RESOLUTION_WMS_MAX_TILE_SIZE = 2048
HIGH_RESOLUTION_LAYER_MARKER = "orthohr"
_WMS_CLIENT_ATTRIBUTE = "_sleufbase_high_resolution_wms_client"


def _uses_high_resolution_aerial_layer(client: PdokWmtsTileClient) -> bool:
    """Return whether the selected PDOK aerial layer is the HR orthophoto."""

    return HIGH_RESOLUTION_LAYER_MARKER in str(getattr(client, "layer_name", "") or "").casefold()


def _high_resolution_wms_client(client: PdokWmtsTileClient) -> PdokWmsClient:
    """Return a WMS client mirroring the selected WMTS aerial provider.

    SleufBase historically displayed the manually selected PDOK aerial through
    GoogleMapsCompatible WMTS tiles capped at zoom 19. Around Dutch latitudes
    that no longer exposes all detail present in the 8 cm / partly 5 cm HR
    source. The Cyclomedia fallback already uses this WMS client and therefore
    looked visibly sharper at close zoom levels.
    """

    existing = getattr(client, _WMS_CLIENT_ATTRIBUTE, None)
    if isinstance(existing, PdokWmsClient) and existing.layer_name == client.layer_name:
        return existing

    high_resolution = PdokWmsClient(
        layer_name=client.layer_name,
        timeout=max(1, int(getattr(client, "timeout", 30) or 30)),
        retries=max(1, int(getattr(client, "retries", 3) or 3)),
        max_workers=max(1, min(int(getattr(client, "max_workers", 8) or 8), 6)),
        transparent=False,
    )
    setattr(client, _WMS_CLIENT_ATTRIBUTE, high_resolution)
    return high_resolution


def install_pdok_high_quality_patch() -> bool:
    """Use PDOK WMS for final HR aerial imagery while retaining WMTS previews.

    WMTS remains useful as a fast disk-cached preview. Final map requests for
    HR orthophotos are delegated to WMS at the exact requested pixel size, which
    is the same high-quality path already used by the Cyclomedia PDOK fallback.
    """

    cls = PdokWmtsTileClient
    if int(getattr(cls, "_sleufbase_high_quality_patch_version", 0) or 0) >= PATCH_VERSION:
        return True

    original_fetch_map = cls.fetch_map
    original_preview_map = cls.preview_map

    def fetch_map_high_quality(
        self: PdokWmtsTileClient,
        bounds,
        size: tuple[int, int],
        on_progress=None,
    ):
        if not _uses_high_resolution_aerial_layer(self):
            return original_fetch_map(self, bounds, size, on_progress=on_progress)

        return _high_resolution_wms_client(self).fetch_map(
            bounds,
            size,
            max_tile_size=HIGH_RESOLUTION_WMS_MAX_TILE_SIZE,
            on_progress=on_progress,
        )

    def preview_map_high_quality(
        self: PdokWmtsTileClient,
        bounds,
        size: tuple[int, int],
    ):
        if _uses_high_resolution_aerial_layer(self):
            # Prefer an exact cached WMS image once it exists. Before that, keep
            # the existing WMTS disk cache as a fast temporary preview while the
            # high-resolution WMS request is running.
            wms = _high_resolution_wms_client(self)
            try:
                cached = wms._cache_get(wms._cache_key(bounds, size))
            except Exception:
                cached = None
            if cached is not None:
                return cached
        return original_preview_map(self, bounds, size)

    cls.fetch_map = fetch_map_high_quality
    cls.preview_map = preview_map_high_quality
    cls._sleufbase_high_quality_patch_version = PATCH_VERSION
    cls.SLEUFBASE_HIGH_RESOLUTION_WMS_MAX_TILE_SIZE = HIGH_RESOLUTION_WMS_MAX_TILE_SIZE
    return True
