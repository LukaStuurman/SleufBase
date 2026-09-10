from __future__ import annotations

from .cyclomedia import CyclomediaAerialClient
from .exporting import MapExporter
from .pdok import PdokWmtsTileClient


PATCH_VERSION = 2
DEFAULT_BACKGROUND_LAYER = "Actueel_orthoHR"
DEFAULT_BACKGROUND_LABEL = "Luchtfoto NL actueel (8 cm, deels 5 cm)"
DEFAULT_BACKGROUND_TILE_WORKERS = 8


def _pdok_default_provider(source_provider) -> PdokWmtsTileClient:
    return PdokWmtsTileClient(
        layer_name=DEFAULT_BACKGROUND_LAYER,
        timeout=max(1, int(getattr(source_provider, "timeout", 30) or 30)),
        retries=max(1, int(getattr(source_provider, "retries", 3) or 3)),
        max_workers=DEFAULT_BACKGROUND_TILE_WORKERS,
    )


def install_pdok_default_background_patch() -> bool:
    """Make PDOK Actueel_orthoHR the default map-export background.

    Only the former Cyclomedia *default* passed into a newly created MapExporter
    is replaced. Explicit background-provider overrides remain untouched, so the
    user can still deliberately choose another source. The PDOK WMTS client uses
    its exact-size WMS route for the Actueel_orthoHR layer during final export.
    """

    if int(getattr(MapExporter, "_sleufbase_pdok_default_background_version", 0) or 0) >= PATCH_VERSION:
        return True

    original_init = MapExporter.__init__

    def __init__(self, default_background_provider, *args, **kwargs):
        provider = default_background_provider
        if isinstance(provider, CyclomediaAerialClient):
            provider = _pdok_default_provider(provider)
        original_init(self, provider, *args, **kwargs)

    MapExporter.__init__ = __init__
    MapExporter._sleufbase_pdok_default_background_version = PATCH_VERSION
    MapExporter.SLEUFBASE_DEFAULT_BACKGROUND_LAYER = DEFAULT_BACKGROUND_LAYER
    MapExporter.SLEUFBASE_DEFAULT_BACKGROUND_LABEL = DEFAULT_BACKGROUND_LABEL
    MapExporter.SLEUFBASE_DEFAULT_BACKGROUND_TILE_WORKERS = DEFAULT_BACKGROUND_TILE_WORKERS

    # Use the same wording when the legacy Cyclomedia->PDOK fallback is active.
    try:
        from . import cyclomedia_fallback

        cyclomedia_fallback.PDOK_FALLBACK_LABEL = DEFAULT_BACKGROUND_LABEL
    except Exception:
        pass
    return True
