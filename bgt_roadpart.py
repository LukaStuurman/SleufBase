from __future__ import annotations

from collections import OrderedDict
from dataclasses import dataclass
import threading
import time
from typing import Any

import requests

from .models import Bounds


class BgtRoadPartError(RuntimeError):
    """Raised when BGT wegdelen niet kunnen worden opgehaald."""


@dataclass(frozen=True)
class BgtRoadPartPath:
    points: list[tuple[float, float]]


class BgtRoadPartClient:
    BASE_URL = "https://api.pdok.nl/lv/bgt/ogc/v1"
    COLLECTION_ID = "wegdeel"
    CRS_28992 = "http://www.opengis.net/def/crs/EPSG/0/28992"
    CACHE_LIMIT = 24
    MAX_PAGES = 200

    def __init__(self, timeout: int = 45, page_size: int = 1500, retries: int = 4) -> None:
        self.timeout = max(1, int(timeout))
        self.page_size = max(50, int(page_size))
        self.retries = max(1, int(retries))
        self._thread_local = threading.local()
        self._cache_lock = threading.RLock()
        self._cache: OrderedDict[tuple[float, float, float, float], list[list[tuple[float, float]]]] = OrderedDict()

    @staticmethod
    def _create_session() -> requests.Session:
        session = requests.Session()
        session.headers.update({"User-Agent": "SleufBase/1.0"})
        return session

    def _get_session(self) -> requests.Session:
        session = getattr(self._thread_local, "session", None)
        if session is None:
            session = self._create_session()
            self._thread_local.session = session
        return session

    def _cache_get(self, key: tuple[float, float, float, float]) -> list[list[tuple[float, float]]] | None:
        with self._cache_lock:
            cached = self._cache.pop(key, None)
            if cached is None:
                return None
            self._cache[key] = cached
            return [list(path) for path in cached]

    def _cache_put(self, key: tuple[float, float, float, float], paths: list[list[tuple[float, float]]]) -> None:
        stored = [list(path) for path in paths]
        with self._cache_lock:
            self._cache.pop(key, None)
            self._cache[key] = stored
            while len(self._cache) > self.CACHE_LIMIT:
                self._cache.popitem(last=False)

    def fetch_paths(self, bounds: Bounds) -> list[list[tuple[float, float]]]:
        cache_key = (
            round(bounds.min_x, 3),
            round(bounds.min_y, 3),
            round(bounds.max_x, 3),
            round(bounds.max_y, 3),
        )
        cached = self._cache_get(cache_key)
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
        seen_urls: set[str] = set()
        page_count = 0
        while url:
            if url in seen_urls:
                raise BgtRoadPartError("BGT wegdelen server herhaalt dezelfde vervolgpagina.")
            seen_urls.add(url)
            page_count += 1
            if page_count > self.MAX_PAGES:
                raise BgtRoadPartError(f"BGT wegdelen overschrijdt de veilige paginalimiet ({self.MAX_PAGES}).")
            try:
                payload = self._get_json(url, params)
            except Exception as exc:
                raise BgtRoadPartError(f"BGT wegdelen ophalen mislukt: {exc}") from exc

            features = payload.get("features", [])
            if not isinstance(features, list):
                raise BgtRoadPartError("Ongeldige respons voor BGT wegdelen.")
            for feature in features:
                if not isinstance(feature, dict):
                    continue
                geometry = feature.get("geometry")
                paths.extend(self._geometry_to_paths(geometry))

            url = self._next_link(payload.get("links"))
            params = None

        deduped = self._dedupe_paths(paths)
        self._cache_put(cache_key, deduped)
        return deduped

    def _get_json(self, url: str, params: dict[str, object] | None) -> dict[str, Any]:
        last_error: Exception | None = None
        for attempt in range(1, self.retries + 1):
            try:
                response = self._get_session().get(url, params=params, timeout=self.timeout)
                response.raise_for_status()
                payload = response.json()
                if not isinstance(payload, dict):
                    raise ValueError("BGT gaf geen JSON-object terug")
                return payload
            except (requests.RequestException, ValueError) as exc:
                last_error = exc
                if attempt < self.retries:
                    time.sleep(min(2.0, 0.4 * (2 ** (attempt - 1))))
        raise BgtRoadPartError(str(last_error))

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
        if geometry_type == "Polygon":
            return [self._coords_to_path(ring, close=True) for ring in coordinates if len(ring) >= 3]
        if geometry_type == "MultiPolygon":
            paths: list[list[tuple[float, float]]] = []
            for polygon in coordinates:
                for ring in polygon:
                    if len(ring) >= 3:
                        paths.append(self._coords_to_path(ring, close=True))
            return paths
        return []

    def _coords_to_path(self, coordinates: list[Any], close: bool = False) -> list[tuple[float, float]]:
        path = [(float(item[0]), float(item[1])) for item in coordinates]
        if close and path and path[0] != path[-1]:
            path.append(path[0])
        return path

    @staticmethod
    def _dedupe_paths(paths: list[list[tuple[float, float]]]) -> list[list[tuple[float, float]]]:
        deduped: list[list[tuple[float, float]]] = []
        seen: set[tuple[tuple[float, float], ...]] = set()
        for path in paths:
            if len(path) < 2:
                continue
            key = tuple((round(float(x), 3), round(float(y), 3)) for x, y in path)
            reverse = tuple(reversed(key))
            canonical = key if key <= reverse else reverse
            if canonical in seen:
                continue
            seen.add(canonical)
            deduped.append(path)
        return deduped
