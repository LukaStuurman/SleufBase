from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor, as_completed

from .cyclomedia import CyclomediaAerialClient, CyclomediaAerialError
from .pdok import PdokWmsClient


PDOK_FALLBACK_LAYER = "Actueel_orthoHR"
PDOK_FALLBACK_LABEL = "PDOK Luchtfoto Actueel HR (8 cm / deels 5 cm)"


def _is_cyclomedia_auth_error(exc: BaseException) -> bool:
    """Return True only for Cyclomedia authorization failures.

    Other failures are deliberately not hidden; network, parsing and server
    errors should still be reported to the user instead of silently changing
    the selected background source.
    """

    text = str(exc or "").casefold()
    return (
        "401" in text
        or "unauthorized" in text
        or "not authorized" in text
        or "not authorised" in text
    )


def _pdok_client(client: CyclomediaAerialClient) -> PdokWmsClient:
    fallback = getattr(client, "_sleufbase_pdok_fallback_client", None)
    if isinstance(fallback, PdokWmsClient):
        return fallback

    fallback = PdokWmsClient(
        layer_name=PDOK_FALLBACK_LAYER,
        timeout=int(getattr(client, "timeout", 30) or 30),
        retries=int(getattr(client, "retries", 3) or 3),
        max_workers=6,
    )
    setattr(client, "_sleufbase_pdok_fallback_client", fallback)
    return fallback


def _fetch_pdok_prefetch(
    client: CyclomediaAerialClient,
    requests_to_prepare: list[tuple[object, tuple[int, int]]],
) -> dict[str, int]:
    normalized = [
        (bounds, (max(1, int(size[0])), max(1, int(size[1]))))
        for bounds, size in requests_to_prepare
        if int(size[0]) > 0 and int(size[1]) > 0
    ]
    if not normalized:
        return {"maps": 0, "clusters": 0, "tiles": 0}

    pdok = _pdok_client(client)
    max_workers = max(1, min(2, len(normalized)))
    with ThreadPoolExecutor(max_workers=max_workers, thread_name_prefix="pdok-aerial-fallback") as executor:
        futures = [executor.submit(pdok.fetch_map, bounds, size) for bounds, size in normalized]
        for future in as_completed(futures):
            future.result()

    # The PDOK client internally tiles large WMS requests. These counts are only
    # used for progress/status reporting, so report one prepared coverage per map.
    return {"maps": len(normalized), "clusters": len(normalized), "tiles": len(normalized)}


def install_cyclomedia_pdok_fallback() -> bool:
    """Use PDOK Actueel_orthoHR when Cyclomedia rejects Aerial WMS access.

    StreetSmart credentials can be valid for the StreetSmart viewer while the
    same account has no Aerial Map Service entitlement. In that one case we
    transparently use the already-supported public PDOK high-resolution aerial
    layer. The patch is idempotent and leaves all non-authentication errors
    untouched.
    """

    cls = CyclomediaAerialClient
    if getattr(cls, "_sleufbase_pdok_fallback_installed", False):
        return True

    original_fetch_map = cls.fetch_map
    original_prefetch_maps = cls.prefetch_maps
    original_current_layer_label = cls.current_layer_label

    def fetch_map_with_pdok_fallback(self, bounds, size, max_tile_size=2048, on_progress=None):
        if getattr(self, "_sleufbase_using_pdok_fallback", False):
            return _pdok_client(self).fetch_map(
                bounds,
                size,
                max_tile_size=max_tile_size,
                on_progress=on_progress,
            )

        try:
            return original_fetch_map(
                self,
                bounds,
                size,
                max_tile_size=max_tile_size,
                on_progress=on_progress,
            )
        except CyclomediaAerialError as exc:
            if not _is_cyclomedia_auth_error(exc):
                raise
            setattr(self, "_sleufbase_using_pdok_fallback", True)
            return _pdok_client(self).fetch_map(
                bounds,
                size,
                max_tile_size=max_tile_size,
                on_progress=on_progress,
            )

    def prefetch_maps_with_pdok_fallback(self, requests_to_prepare):
        if getattr(self, "_sleufbase_using_pdok_fallback", False):
            return _fetch_pdok_prefetch(self, requests_to_prepare)

        try:
            return original_prefetch_maps(self, requests_to_prepare)
        except CyclomediaAerialError as exc:
            if not _is_cyclomedia_auth_error(exc):
                raise
            setattr(self, "_sleufbase_using_pdok_fallback", True)
            return _fetch_pdok_prefetch(self, requests_to_prepare)

    def current_layer_label_with_pdok_fallback(self):
        if getattr(self, "_sleufbase_using_pdok_fallback", False):
            return PDOK_FALLBACK_LABEL
        return original_current_layer_label(self)

    cls.fetch_map = fetch_map_with_pdok_fallback
    cls.prefetch_maps = prefetch_maps_with_pdok_fallback
    cls.current_layer_label = current_layer_label_with_pdok_fallback
    cls._sleufbase_pdok_fallback_installed = True
    return True
