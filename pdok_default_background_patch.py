from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor, as_completed
import time

from .cyclomedia import CyclomediaAerialClient
from .pdok import PdokError, PdokWmsClient


PATCH_VERSION = 1
DEFAULT_BACKGROUND_LAYER = "Actueel_orthoHR"
DEFAULT_BACKGROUND_LABEL = "Luchtfoto NL actueel (8 cm, deels 5 cm)"
DEFAULT_PDOK_INNER_WORKERS = 4
DEFAULT_PREFETCH_WORKERS = 4
PDOK_RETRY_COOLDOWN_SECONDS = 30.0


def _default_pdok_client(client: CyclomediaAerialClient) -> PdokWmsClient:
    existing = getattr(client, "_sleufbase_pdok_default_client", None)
    if isinstance(existing, PdokWmsClient) and existing.layer_name == DEFAULT_BACKGROUND_LAYER:
        return existing

    provider = PdokWmsClient(
        layer_name=DEFAULT_BACKGROUND_LAYER,
        timeout=max(1, int(getattr(client, "timeout", 30) or 30)),
        retries=max(1, int(getattr(client, "retries", 3) or 3)),
        max_workers=DEFAULT_PDOK_INNER_WORKERS,
        transparent=False,
    )
    setattr(client, "_sleufbase_pdok_default_client", provider)
    return provider


def _pdok_retry_allowed(client: CyclomediaAerialClient) -> bool:
    return time.monotonic() >= float(
        getattr(client, "_sleufbase_pdok_default_retry_after", 0.0) or 0.0
    )


def _mark_pdok_success(client: CyclomediaAerialClient) -> None:
    setattr(client, "_sleufbase_pdok_default_active", True)
    setattr(client, "_sleufbase_pdok_default_retry_after", 0.0)


def _mark_pdok_failure(client: CyclomediaAerialClient) -> None:
    setattr(client, "_sleufbase_pdok_default_active", False)
    setattr(
        client,
        "_sleufbase_pdok_default_retry_after",
        time.monotonic() + PDOK_RETRY_COOLDOWN_SECONDS,
    )


def _fetch_and_close(provider: PdokWmsClient, bounds, size) -> None:
    image = provider.fetch_map(bounds, size)
    try:
        return None
    finally:
        image.close()


def install_pdok_default_background_patch() -> bool:
    """Use public PDOK Actueel_orthoHR as the initial/default aerial provider.

    Cyclomedia remains a fallback when PDOK is temporarily unavailable. Existing
    callers therefore keep the same provider object/API while template exports
    start with the requested 8 cm / partly 5 cm Dutch aerial imagery.
    """

    cls = CyclomediaAerialClient
    if int(getattr(cls, "_sleufbase_pdok_default_background_version", 0) or 0) >= PATCH_VERSION:
        return True

    original_fetch_map = cls.fetch_map
    original_prefetch_maps = cls.prefetch_maps
    original_current_layer_label = cls.current_layer_label

    def fetch_map_pdok_default(self, bounds, size, max_tile_size=2048, on_progress=None):
        if _pdok_retry_allowed(self):
            try:
                image = _default_pdok_client(self).fetch_map(
                    bounds,
                    size,
                    max_tile_size=max_tile_size,
                    on_progress=on_progress,
                )
                _mark_pdok_success(self)
                return image
            except PdokError:
                _mark_pdok_failure(self)
        return original_fetch_map(
            self,
            bounds,
            size,
            max_tile_size=max_tile_size,
            on_progress=on_progress,
        )

    def prefetch_maps_pdok_default(self, requests_to_prepare):
        requests_list = [
            (bounds, (max(1, int(size[0])), max(1, int(size[1]))))
            for bounds, size in requests_to_prepare
            if int(size[0]) > 0 and int(size[1]) > 0
        ]
        if not requests_list:
            return {"maps": 0, "clusters": 0, "tiles": 0}
        if not _pdok_retry_allowed(self):
            return original_prefetch_maps(self, requests_list)

        provider = _default_pdok_client(self)
        worker_count = max(1, min(DEFAULT_PREFETCH_WORKERS, len(requests_list)))
        try:
            with ThreadPoolExecutor(
                max_workers=worker_count,
                thread_name_prefix="pdok-aerial-default",
            ) as executor:
                futures = [
                    executor.submit(_fetch_and_close, provider, bounds, size)
                    for bounds, size in requests_list
                ]
                for future in as_completed(futures):
                    future.result()
        except PdokError:
            _mark_pdok_failure(self)
            return original_prefetch_maps(self, requests_list)

        _mark_pdok_success(self)
        count = len(requests_list)
        return {"maps": count, "clusters": count, "tiles": count}

    def current_layer_label_pdok_default(self):
        active = getattr(self, "_sleufbase_pdok_default_active", None)
        if active is not False:
            return DEFAULT_BACKGROUND_LABEL
        return original_current_layer_label(self)

    cls.fetch_map = fetch_map_pdok_default
    cls.prefetch_maps = prefetch_maps_pdok_default
    cls.current_layer_label = current_layer_label_pdok_default
    cls._sleufbase_pdok_default_background_version = PATCH_VERSION
    cls.SLEUFBASE_DEFAULT_BACKGROUND_LAYER = DEFAULT_BACKGROUND_LAYER
    cls.SLEUFBASE_DEFAULT_BACKGROUND_LABEL = DEFAULT_BACKGROUND_LABEL
    cls.SLEUFBASE_DEFAULT_BACKGROUND_PREFETCH_WORKERS = DEFAULT_PREFETCH_WORKERS

    # Keep the existing auth/fallback patch label consistent when it is installed
    # later by the launcher.
    try:
        from . import cyclomedia_fallback

        cyclomedia_fallback.PDOK_FALLBACK_LABEL = DEFAULT_BACKGROUND_LABEL
    except Exception:
        pass
    return True
