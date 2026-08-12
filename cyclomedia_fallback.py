from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor, as_completed
from io import BytesIO
import time

import requests
from PIL import Image

from .cyclomedia import CyclomediaAerialClient, CyclomediaAerialError
from .pdok import PdokWmsClient
from .streetsmart_bearer import (
    bearer_authorization_header,
    clear_streetsmart_bearer_token,
    load_streetsmart_bearer_token,
)


PDOK_FALLBACK_LAYER = "Actueel_orthoHR"
PDOK_FALLBACK_LABEL = "PDOK Luchtfoto Actueel HR (8 cm / deels 5 cm)"
PDOK_AUTH_FAILURE_COOLDOWN_SECONDS = 30


def _is_cyclomedia_auth_error(exc: BaseException) -> bool:
    """Return True only for Cyclomedia authentication/authorization failures."""

    text = str(exc or "").casefold()
    return (
        "401" in text
        or "403" in text
        or "unauthorized" in text
        or "not authorized" in text
        or "not authorised" in text
        or "forbidden" in text
    )


def _bearer_headers(token: str) -> dict[str, str]:
    return {"Authorization": bearer_authorization_header(token)}


def _mark_bearer_success(client: CyclomediaAerialClient) -> None:
    setattr(client, "_sleufbase_cyclomedia_auth_mode", "streetsmart-bearer")
    setattr(client, "_sleufbase_pdok_until", 0.0)


def _handle_rejected_bearer(response: requests.Response) -> None:
    if response.status_code in (401, 403):
        clear_streetsmart_bearer_token()
        raise CyclomediaAerialError(
            f"StreetSmart-autorisatie gaf geen toegang tot Cyclomedia luchtfoto: "
            f"{response.status_code} {response.reason}"
        )


def _resolve_latest_layer_via_streetsmart_bearer(
    client: CyclomediaAerialClient,
    token: str,
) -> tuple[str, str]:
    params = {
        "SERVICE": "WMS",
        "VERSION": "1.1.1",
        "REQUEST": "GetCapabilities",
    }
    try:
        response = client.session.get(
            client.BASE_URL,
            params=params,
            headers=_bearer_headers(token),
            timeout=client.timeout,
        )
        _handle_rejected_bearer(response)
        response.raise_for_status()
    except CyclomediaAerialError:
        raise
    except requests.RequestException as exc:
        raise CyclomediaAerialError(
            f"Cyclomedia capabilities via StreetSmart-autorisatie ophalen mislukt: {exc}"
        ) from exc

    matches = [
        (int(year), int(cm), layer_name)
        for layer_name, year, cm in client.LAYER_PATTERN.findall(response.text)
    ]
    if not matches:
        raise CyclomediaAerialError(
            "Geen Cyclomedia Luchtfoto NL-lagen gevonden met de StreetSmart-autorisatie."
        )
    latest_year = max(year for year, _cm, _name in matches)
    latest_matches = [item for item in matches if item[0] == latest_year]
    best_year, best_cm, best_name = min(latest_matches, key=lambda item: item[1])
    label = f"Luchtfoto NL {best_year} {best_cm}cm"
    client._resolved_layer_name = best_name
    client._resolved_layer_label = label
    client._resolved_at = time.time()
    _mark_bearer_success(client)
    return best_name, label


def _request_image_via_streetsmart_bearer(
    client: CyclomediaAerialClient,
    layer_name: str,
    bounds,
    size: tuple[int, int],
    token: str,
) -> Image.Image:
    params = {
        "SERVICE": "WMS",
        "VERSION": "1.1.1",
        "REQUEST": "GetMap",
        "LAYERS": layer_name,
        "STYLES": "",
        "FORMAT": "image/png",
        "TRANSPARENT": "FALSE",
        "SRS": "EPSG:28992",
        "WIDTH": int(size[0]),
        "HEIGHT": int(size[1]),
        "BBOX": f"{bounds.min_x},{bounds.min_y},{bounds.max_x},{bounds.max_y}",
    }
    try:
        response = client._get_session().get(
            client.BASE_URL,
            params=params,
            headers=_bearer_headers(token),
            timeout=client.timeout,
        )
        _handle_rejected_bearer(response)
        response.raise_for_status()
        content_type = response.headers.get("content-type", "")
        if "image" not in content_type.casefold():
            snippet = response.text[:300].strip()
            raise CyclomediaAerialError(
                f"Cyclomedia gaf via StreetSmart-autorisatie geen luchtfoto terug: {snippet}"
            )
        image = Image.open(BytesIO(response.content)).convert("RGBA")
    except CyclomediaAerialError:
        raise
    except (requests.RequestException, OSError) as exc:
        raise CyclomediaAerialError(
            f"Cyclomedia luchtfoto via StreetSmart-autorisatie ophalen mislukt: {exc}"
        ) from exc
    _mark_bearer_success(client)
    return image


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

    return {"maps": len(normalized), "clusters": len(normalized), "tiles": len(normalized)}


def _pdok_cooldown_active(client: CyclomediaAerialClient) -> bool:
    # A freshly captured StreetSmart token always gets a chance immediately,
    # even if a Basic-auth failure put the client in a short PDOK cooldown.
    if load_streetsmart_bearer_token() is not None:
        return False
    return time.time() < float(getattr(client, "_sleufbase_pdok_until", 0.0) or 0.0)


def _mark_pdok_fallback(client: CyclomediaAerialClient) -> None:
    setattr(
        client,
        "_sleufbase_pdok_until",
        time.time() + PDOK_AUTH_FAILURE_COOLDOWN_SECONDS,
    )


def install_cyclomedia_pdok_fallback() -> bool:
    """Install the authorized Cyclomedia -> StreetSmart -> PDOK chain.

    Order:
    1. Existing Cyclomedia Aerial WMS Basic authentication.
    2. If Cyclomedia returns 401/403, retry with the documented bearer token
       captured from the user's normally authenticated StreetSmart API session.
    3. If no valid bearer is available or Atlas rejects it too, use the already
       supported public PDOK Actueel_orthoHR layer for a short cooldown.

    No cookies, browser storage, hidden tile URLs or network requests are
    intercepted. The StreetSmart bearer is obtained only through the documented
    StreetSmartApi.getBearerToken() function.
    """

    cls = CyclomediaAerialClient
    if getattr(cls, "_sleufbase_pdok_fallback_installed", False):
        return True

    original_resolve_latest_layer_serial = cls._resolve_latest_layer_serial
    original_request_image = cls._request_image
    original_fetch_map = cls.fetch_map
    original_prefetch_maps = cls.prefetch_maps
    original_current_layer_label = cls.current_layer_label

    def resolve_latest_layer_serial_with_streetsmart(self, username, password):
        try:
            return original_resolve_latest_layer_serial(self, username, password)
        except CyclomediaAerialError as exc:
            if not _is_cyclomedia_auth_error(exc):
                raise
            token = load_streetsmart_bearer_token()
            if not token:
                raise
            return _resolve_latest_layer_via_streetsmart_bearer(self, token)

    def request_image_with_streetsmart(
        self,
        layer_name,
        bounds,
        size,
        username,
        password,
    ):
        try:
            return original_request_image(
                self,
                layer_name,
                bounds,
                size,
                username,
                password,
            )
        except CyclomediaAerialError as exc:
            if not _is_cyclomedia_auth_error(exc):
                raise
            token = load_streetsmart_bearer_token()
            if not token:
                raise
            return _request_image_via_streetsmart_bearer(
                self,
                layer_name,
                bounds,
                size,
                token,
            )

    def fetch_map_with_fallback(self, bounds, size, max_tile_size=2048, on_progress=None):
        if _pdok_cooldown_active(self):
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
            _mark_pdok_fallback(self)
            return _pdok_client(self).fetch_map(
                bounds,
                size,
                max_tile_size=max_tile_size,
                on_progress=on_progress,
            )

    def prefetch_maps_with_fallback(self, requests_to_prepare):
        if _pdok_cooldown_active(self):
            return _fetch_pdok_prefetch(self, requests_to_prepare)
        try:
            return original_prefetch_maps(self, requests_to_prepare)
        except CyclomediaAerialError as exc:
            if not _is_cyclomedia_auth_error(exc):
                raise
            _mark_pdok_fallback(self)
            return _fetch_pdok_prefetch(self, requests_to_prepare)

    def current_layer_label_with_fallback(self):
        if _pdok_cooldown_active(self):
            return PDOK_FALLBACK_LABEL
        label = original_current_layer_label(self)
        if label and getattr(self, "_sleufbase_cyclomedia_auth_mode", "") == "streetsmart-bearer":
            return f"{label} (via StreetSmart)"
        return label

    cls._resolve_latest_layer_serial = resolve_latest_layer_serial_with_streetsmart
    cls._request_image = request_image_with_streetsmart
    cls.fetch_map = fetch_map_with_fallback
    cls.prefetch_maps = prefetch_maps_with_fallback
    cls.current_layer_label = current_layer_label_with_fallback
    cls._sleufbase_pdok_fallback_installed = True
    cls._sleufbase_streetsmart_bearer_retry_installed = True
    return True
