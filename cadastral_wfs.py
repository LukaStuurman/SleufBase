from __future__ import annotations

from collections import OrderedDict
from dataclasses import dataclass
import hashlib
import json
import threading
import time
from typing import Any

import requests

from .models import Bounds


class CadastralWfsError(RuntimeError):
    """Raised when kadastrale WFS-data niet kan worden opgehaald."""


@dataclass(frozen=True)
class CadastralLinework:
    layer_name: str
    paths: list[list[tuple[float, float]]]


@dataclass(frozen=True)
class CadastralTextLabel:
    layer_name: str
    text: str
    position: tuple[float, float]
    rotation: float


class CadastralWfsClient:
    BASE_URL = "https://service.pdok.nl/kadaster/kadastralekaart/wfs/v5_0"
    TILE_SIZE_METERS = 350.0
    TILE_OVERLAP_METERS = 8.0
    DIRECT_MAX_SPAN_METERS = 700.0
    SERVER_LIMIT_HINT = 1000
    MIN_SPLIT_SPAN_METERS = 90.0
    CACHE_LIMIT = 96
    MAX_PAGE_COUNT = 100
    FEATURE_TYPES = {
        "kadastralekaart:Perceel": "KAD_PERCEEL",
        "kadastralekaart:KadastraleGrens": "KAD_GRENS",
        "kadastralekaart:Bebouwing": "KAD_BEBOUWING",
    }
    TEXT_TYPES = {
        "kadastralekaart:OpenbareRuimteNaam": "KAD_STRAATNAAM",
        "kadastralekaart:Nummeraanduidingreeks": "KAD_HUISNUMMER",
    }

    def __init__(self, timeout: int = 45, page_size: int = 1500, retries: int = 4) -> None:
        self.timeout = timeout
        self.page_size = max(50, page_size)
        self.retries = max(1, int(retries))
        self.session = requests.Session()
        self.session.headers.update({"User-Agent": "SleufBase/0.2"})
        self._feature_cache: OrderedDict[
            tuple[str, float, float, float, float], list[dict[str, Any]]
        ] = OrderedDict()
        self._feature_cache_lock = threading.RLock()

    def close(self) -> None:
        self.session.close()

    def fetch_linework(self, bounds: Bounds) -> list[CadastralLinework]:
        result: list[CadastralLinework] = []
        for feature_type, layer_name in self.FEATURE_TYPES.items():
            features = self._fetch_features(feature_type, bounds)
            paths: list[list[tuple[float, float]]] = []
            for feature in features:
                geometry = feature.get("geometry")
                paths.extend(self._geometry_to_paths(geometry))
            if paths:
                result.append(CadastralLinework(layer_name=layer_name, paths=paths))
        return result

    def fetch_parcel_boundaries(self, bounds: Bounds) -> CadastralLinework | None:
        feature_type = "kadastralekaart:KadastraleGrens"
        features = self._fetch_features(feature_type, bounds)
        paths: list[list[tuple[float, float]]] = []
        for feature in features:
            paths.extend(self._geometry_to_paths(feature.get("geometry")))
        if not paths:
            return None
        return CadastralLinework(layer_name="KAD_GRENS", paths=paths)

    def fetch_text_labels(self, bounds: Bounds) -> list[CadastralTextLabel]:
        labels: list[CadastralTextLabel] = []
        for feature_type, layer_name in self.TEXT_TYPES.items():
            features = self._fetch_features(feature_type, bounds)
            for feature in features:
                label = self._feature_to_text_label(feature, layer_name)
                if label is not None:
                    labels.append(label)
        return labels

    def _cache_get(
        self, cache_key: tuple[str, float, float, float, float]
    ) -> list[dict[str, Any]] | None:
        with self._feature_cache_lock:
            cached = self._feature_cache.get(cache_key)
            if cached is None:
                return None
            self._feature_cache.move_to_end(cache_key)
            return cached

    def _cache_put(
        self,
        cache_key: tuple[str, float, float, float, float],
        features: list[dict[str, Any]],
    ) -> None:
        with self._feature_cache_lock:
            self._feature_cache[cache_key] = features
            self._feature_cache.move_to_end(cache_key)
            while len(self._feature_cache) > self.CACHE_LIMIT:
                self._feature_cache.popitem(last=False)

    def _fetch_features(self, feature_type: str, bounds: Bounds) -> list[dict[str, Any]]:
        cache_key = self._feature_cache_key(feature_type, bounds)
        cached = self._cache_get(cache_key)
        if cached is not None:
            return cached

        if self._should_tile_bounds(bounds):
            all_features = self._fetch_features_tiled(feature_type, bounds)
        else:
            all_features = self._fetch_features_direct(feature_type, bounds)
            if len(all_features) >= self.SERVER_LIMIT_HINT and self._can_split_bounds(bounds):
                all_features = self._fetch_features_tiled(feature_type, bounds)
        self._cache_put(cache_key, all_features)
        return all_features

    def _fetch_features_direct(self, feature_type: str, bounds: Bounds) -> list[dict[str, Any]]:
        all_features: list[dict[str, Any]] = []
        start_index = 0
        previous_page_signature: bytes | None = None
        for page_number in range(1, self.MAX_PAGE_COUNT + 1):
            params = {
                "service": "WFS",
                "version": "2.0.0",
                "request": "GetFeature",
                "typeNames": feature_type,
                "srsName": "EPSG:28992",
                "bbox": f"{bounds.min_x},{bounds.min_y},{bounds.max_x},{bounds.max_y},EPSG:28992",
                "count": self.page_size,
                "startIndex": start_index,
                "outputFormat": "application/json",
            }
            try:
                payload = self._get_json(params)
            except CadastralWfsError:
                raise
            except Exception as exc:
                raise CadastralWfsError(
                    f"Kadastrale WFS ophalen mislukt voor {feature_type}: {exc}"
                ) from exc

            features = payload.get("features", [])
            if not isinstance(features, list):
                raise CadastralWfsError(f"Ongeldige WFS-respons voor {feature_type}.")
            if not features:
                break

            page_signature = self._page_signature(features)
            if previous_page_signature is not None and page_signature == previous_page_signature:
                raise CadastralWfsError(
                    "De kadastrale WFS-server herhaalt dezelfde pagina en lijkt "
                    "startIndex te negeren. Ophalen is gestopt om een vastloper te voorkomen."
                )
            previous_page_signature = page_signature

            all_features.extend(features)
            if len(features) < self.page_size:
                break
            start_index += len(features)
        else:
            raise CadastralWfsError(
                f"Kadastrale WFS voor {feature_type} overschreed de veiligheidslimiet "
                f"van {self.MAX_PAGE_COUNT} pagina's."
            )
        return all_features

    @classmethod
    def _page_signature(cls, features: list[dict[str, Any]]) -> bytes:
        digest = hashlib.blake2b(digest_size=16)
        digest.update(str(len(features)).encode("ascii"))
        for feature in features:
            digest.update(cls._feature_key(feature).encode("utf-8", errors="replace"))
            digest.update(b"\0")
        return digest.digest()

    def _fetch_features_tiled(self, feature_type: str, bounds: Bounds) -> list[dict[str, Any]]:
        deduped: list[dict[str, Any]] = []
        seen: set[str] = set()
        for tile_bounds in self._tile_bounds(bounds):
            for feature in self._fetch_features_direct_or_split(feature_type, tile_bounds):
                key = self._feature_key(feature)
                if key in seen:
                    continue
                seen.add(key)
                deduped.append(feature)
        return deduped

    def _fetch_features_direct_or_split(self, feature_type: str, bounds: Bounds) -> list[dict[str, Any]]:
        cache_key = self._feature_cache_key(feature_type, bounds)
        cached = self._cache_get(cache_key)
        if cached is not None:
            return cached
        features = self._fetch_features_direct(feature_type, bounds)
        if len(features) >= self.SERVER_LIMIT_HINT and self._can_split_bounds(bounds):
            features = self._fetch_features_from_subtiles(feature_type, bounds)
        self._cache_put(cache_key, features)
        return features

    def _fetch_features_from_subtiles(self, feature_type: str, bounds: Bounds) -> list[dict[str, Any]]:
        deduped: list[dict[str, Any]] = []
        seen: set[str] = set()
        for tile_bounds in self._split_bounds(bounds):
            for feature in self._fetch_features_direct_or_split(feature_type, tile_bounds):
                key = self._feature_key(feature)
                if key in seen:
                    continue
                seen.add(key)
                deduped.append(feature)
        return deduped

    def _tile_bounds(self, bounds: Bounds) -> list[Bounds]:
        tiles: list[Bounds] = []
        x = bounds.min_x
        while x < bounds.max_x - 1e-9:
            next_x = min(bounds.max_x, x + self.TILE_SIZE_METERS)
            y = bounds.min_y
            while y < bounds.max_y - 1e-9:
                next_y = min(bounds.max_y, y + self.TILE_SIZE_METERS)
                tiles.append(
                    Bounds(
                        max(bounds.min_x, x - self.TILE_OVERLAP_METERS),
                        max(bounds.min_y, y - self.TILE_OVERLAP_METERS),
                        min(bounds.max_x, next_x + self.TILE_OVERLAP_METERS),
                        min(bounds.max_y, next_y + self.TILE_OVERLAP_METERS),
                    )
                )
                y = next_y
            x = next_x
        return tiles or [bounds]

    def _split_bounds(self, bounds: Bounds) -> list[Bounds]:
        center_x = bounds.center_x
        center_y = bounds.center_y
        overlap = min(self.TILE_OVERLAP_METERS, max(bounds.width, bounds.height) * 0.1)
        return [
            Bounds(bounds.min_x, bounds.min_y, min(bounds.max_x, center_x + overlap), min(bounds.max_y, center_y + overlap)),
            Bounds(max(bounds.min_x, center_x - overlap), bounds.min_y, bounds.max_x, min(bounds.max_y, center_y + overlap)),
            Bounds(bounds.min_x, max(bounds.min_y, center_y - overlap), min(bounds.max_x, center_x + overlap), bounds.max_y),
            Bounds(max(bounds.min_x, center_x - overlap), max(bounds.min_y, center_y - overlap), bounds.max_x, bounds.max_y),
        ]

    def _should_tile_bounds(self, bounds: Bounds) -> bool:
        return bounds.width > self.DIRECT_MAX_SPAN_METERS or bounds.height > self.DIRECT_MAX_SPAN_METERS

    def _can_split_bounds(self, bounds: Bounds) -> bool:
        return bounds.width > self.MIN_SPLIT_SPAN_METERS or bounds.height > self.MIN_SPLIT_SPAN_METERS

    @staticmethod
    def _feature_cache_key(feature_type: str, bounds: Bounds) -> tuple[str, float, float, float, float]:
        return (
            feature_type,
            round(bounds.min_x, 3),
            round(bounds.min_y, 3),
            round(bounds.max_x, 3),
            round(bounds.max_y, 3),
        )

    @staticmethod
    def _feature_key(feature: dict[str, Any]) -> str:
        feature_id = str(feature.get("id") or "").strip()
        if feature_id:
            return feature_id
        return json.dumps(
            {
                "properties": feature.get("properties") or {},
                "geometry": feature.get("geometry") or {},
            },
            sort_keys=True,
            separators=(",", ":"),
        )

    def _get_json(self, params: dict[str, object]) -> dict[str, Any]:
        last_error: Exception | None = None
        for attempt in range(1, self.retries + 1):
            try:
                response = self.session.get(self.BASE_URL, params=params, timeout=self.timeout)
                response.raise_for_status()
                payload = response.json()
                if not isinstance(payload, dict):
                    raise CadastralWfsError("De WFS-server gaf geen JSON-object terug.")
                return payload
            except (requests.RequestException, ValueError, CadastralWfsError) as exc:
                last_error = exc
                if attempt < self.retries:
                    time.sleep(0.7 * attempt)
        raise CadastralWfsError(str(last_error))

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

    def _feature_to_text_label(self, feature: dict[str, Any], layer_name: str) -> CadastralTextLabel | None:
        geometry = feature.get("geometry") or {}
        if geometry.get("type") != "Point":
            return None
        coordinates = geometry.get("coordinates")
        if not isinstance(coordinates, list) or len(coordinates) < 2:
            return None
        properties = feature.get("properties") or {}
        text = str(properties.get("tekst") or "").strip()
        if not text:
            return None
        try:
            rotation = float(properties.get("hoek") or 0.0)
            position = (float(coordinates[0]), float(coordinates[1]))
        except (TypeError, ValueError, IndexError):
            return None
        return CadastralTextLabel(
            layer_name=layer_name,
            text=text,
            position=position,
            rotation=rotation,
        )

    def _coords_to_path(self, coordinates: list[Any], close: bool = False) -> list[tuple[float, float]]:
        path: list[tuple[float, float]] = []
        for item in coordinates:
            try:
                if len(item) < 2:
                    continue
                path.append((float(item[0]), float(item[1])))
            except (TypeError, ValueError, IndexError):
                continue
        if close and path and path[0] != path[-1]:
            path.append(path[0])
        return path
