from __future__ import annotations

from dataclasses import dataclass
import time
from typing import Any

import requests

from .models import Bounds


class RoadCenterlineError(RuntimeError):
    """Raised when wegdeel-hartlijnen niet kunnen worden opgehaald."""


@dataclass(frozen=True)
class RoadCenterlinePath:
    points: list[tuple[float, float]]


class RoadCenterlineClient:
    BASE_URL = "https://api.pdok.nl/brt/top10nl/ogc/v1"
    COLLECTION_ID = "wegdeel_hartlijn"
    CRS_28992 = "http://www.opengis.net/def/crs/EPSG/0/28992"

    def __init__(self, timeout: int = 45, page_size: int = 1500, retries: int = 4) -> None:
        self.timeout = timeout
        self.page_size = max(50, page_size)
        self.retries = max(1, int(retries))
        self.session = requests.Session()
        self.session.headers.update({"User-Agent": "SleufBase/1.0"})
        self._cache: dict[tuple[float, float, float, float], list[list[tuple[float, float]]]] = {}

    def fetch_paths(self, bounds: Bounds) -> list[list[tuple[float, float]]]:
        cache_key = (
            round(bounds.min_x, 3),
            round(bounds.min_y, 3),
            round(bounds.max_x, 3),
            round(bounds.max_y, 3),
        )
        cached = self._cache.get(cache_key)
        if cached is not None:
            return cached

        url = f"{self.BASE_URL}/collections/{self.COLLECTION_ID}/items"
        params: dict[str, object] | None = {
            "f": "json",
            "bbox": f"{bounds.min_x:.3f},{bounds.min_y:.3f},{bounds.max_x:.3f},{bounds.max_y:.3f}",
            "bbox-crs": self.CRS_28992,
            "crs": self.CRS_28992,
            "limit": self.page_size,
        }

        paths: list[list[tuple[float, float]]] = []
        while url:
            try:
                payload = self._get_json(url, params)
            except Exception as exc:
                raise RoadCenterlineError(f"Wegdeel-hartlijnen ophalen mislukt: {exc}") from exc

            features = payload.get("features", [])
            if not isinstance(features, list):
                raise RoadCenterlineError("Ongeldige respons voor wegdeel-hartlijnen.")
            for feature in features:
                geometry = feature.get("geometry")
                paths.extend(self._geometry_to_paths(geometry))

            url = self._next_link(payload.get("links"))
            params = None
        self._cache[cache_key] = paths
        if len(self._cache) > 24:
            first_key = next(iter(self._cache))
            self._cache.pop(first_key, None)
        return paths

    def _get_json(self, url: str, params: dict[str, object] | None) -> dict[str, Any]:
        last_error: Exception | None = None
        for attempt in range(1, self.retries + 1):
            try:
                response = self.session.get(url, params=params, timeout=self.timeout)
                response.raise_for_status()
                return response.json()
            except Exception as exc:
                last_error = exc
                if attempt < self.retries:
                    time.sleep(0.7 * attempt)
        raise RoadCenterlineError(str(last_error))

    def _next_link(self, links: object) -> str | None:
        if not isinstance(links, list):
            return None
        for link in links:
            if not isinstance(link, dict):
                continue
            if str(link.get("rel", "")).lower() != "next":
                continue
            href = str(link.get("href", "")).strip()
            if href:
                return href
        return None

    def _geometry_to_paths(self, geometry: dict[str, Any] | None) -> list[list[tuple[float, float]]]:
        if not geometry:
            return []
        geometry_type = geometry.get("type")
        coordinates = geometry.get("coordinates")
        if not geometry_type or coordinates is None:
            return []
        if geometry_type == "LineString":
            return [self._coords_to_path(coordinates)]
        if geometry_type == "MultiLineString":
            return [self._coords_to_path(part) for part in coordinates if len(part) >= 2]
        return []

    def _coords_to_path(self, coordinates: list[Any]) -> list[tuple[float, float]]:
        return [(float(item[0]), float(item[1])) for item in coordinates]
