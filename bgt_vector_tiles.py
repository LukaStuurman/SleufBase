from __future__ import annotations

from collections import OrderedDict
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass
from json import dumps
from math import floor
import threading
import time
from typing import Any

import mapbox_vector_tile
import requests
from shapely import make_valid
from shapely.geometry import GeometryCollection, LineString, MultiLineString, MultiPolygon, Polygon, box, shape
from shapely.ops import unary_union

from .models import Bounds


class BgtVectorTileError(RuntimeError):
    """Raised when BGT OGC API vector tiles cannot be fetched or decoded."""


@dataclass(frozen=True)
class BgtSurfaceFeature:
    layer_name: str
    feature_id: str
    physical_appearance: str
    geometry: Any


class BgtVectorTileClient:
    BASE_URL = "https://api.pdok.nl/lv/bgt/ogc/v1/tiles/NetherlandsRDNewQuad"
    TILE_MATRIX = 12
    MATRIX_SIZE = 4096
    TILE_PIXEL_SIZE = 256
    CELL_SIZE_METERS = 0.84
    TILE_SPAN_METERS = TILE_PIXEL_SIZE * CELL_SIZE_METERS
    ORIGIN_X = -285401.92
    ORIGIN_Y = 903401.92
    TILE_CACHE_LIMIT = 128
    EXCLUDED_LAYERS = {
        "buurt",
        "gemeente",
        "landsgrens",
        "openbareruimtelabel",
        "pand_nummeraanduiding",
        "provincie",
        "wijk",
    }

    def __init__(self, timeout: int = 45, retries: int = 4, max_workers: int = 8) -> None:
        self.timeout = max(5, int(timeout))
        self.retries = max(1, int(retries))
        self.max_workers = max(1, min(12, int(max_workers)))
        self._thread_local = threading.local()
        self._cache_lock = threading.RLock()
        self._tile_cache: OrderedDict[tuple[int, int], dict[str, Any]] = OrderedDict()

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

    def _cache_get(self, key: tuple[int, int]) -> dict[str, Any] | None:
        with self._cache_lock:
            cached = self._tile_cache.pop(key, None)
            if cached is None:
                return None
            self._tile_cache[key] = cached
            return cached

    def _cache_put(self, key: tuple[int, int], decoded: dict[str, Any]) -> None:
        with self._cache_lock:
            self._tile_cache.pop(key, None)
            self._tile_cache[key] = decoded
            while len(self._tile_cache) > self.TILE_CACHE_LIMIT:
                self._tile_cache.popitem(last=False)

    def fetch_paths(self, bounds: Bounds) -> list[list[tuple[float, float]]]:
        tiles = self._tiles_for_bounds(bounds)
        if not tiles:
            return []
        features_by_id: dict[tuple[str, str], list[Any]] = {}
        max_workers = max(1, min(self.max_workers, len(tiles)))
        with ThreadPoolExecutor(max_workers=max_workers, thread_name_prefix="bgt-vector-tile") as executor:
            futures = {
                executor.submit(self._tile_features, row, col): (row, col)
                for row, col in tiles
            }
            for future in as_completed(futures):
                row, col = futures[future]
                try:
                    tile_features = future.result()
                except Exception as exc:
                    raise BgtVectorTileError(
                        f"BGT-vector tile {self.TILE_MATRIX}/{row}/{col} ophalen mislukt: {exc}"
                    ) from exc
                for layer_name, feature_id, geometry in tile_features:
                    features_by_id.setdefault((layer_name, feature_id), []).append(geometry)

        clipping_box = box(bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y)
        paths: list[list[tuple[float, float]]] = []
        for feature_key in sorted(features_by_id):
            geometries = features_by_id[feature_key]
            try:
                geometry = geometries[0] if len(geometries) == 1 else unary_union(geometries)
                if not geometry.is_valid:
                    geometry = make_valid(geometry)
                geometry = geometry.intersection(clipping_box)
            except Exception:
                continue
            paths.extend(self._geometry_paths(geometry))
        return self._dedupe_paths(paths)

    def fetch_surface_features(self, bounds: Bounds) -> list[BgtSurfaceFeature]:
        """Return BGT polygon surfaces carrying the `fysiek_voorkomen` property."""
        tiles = self._tiles_for_bounds(bounds)
        if not tiles:
            return []
        features_by_id: dict[tuple[str, str, str], list[Any]] = {}
        max_workers = max(1, min(self.max_workers, len(tiles)))
        with ThreadPoolExecutor(max_workers=max_workers, thread_name_prefix="bgt-surface-tile") as executor:
            futures = {
                executor.submit(self._tile_surface_features, row, col): (row, col)
                for row, col in tiles
            }
            for future in as_completed(futures):
                row, col = futures[future]
                try:
                    tile_features = future.result()
                except Exception as exc:
                    raise BgtVectorTileError(
                        f"BGT-ondergrond tile {self.TILE_MATRIX}/{row}/{col} ophalen mislukt: {exc}"
                    ) from exc
                for feature in tile_features:
                    key = (feature.layer_name, feature.feature_id, feature.physical_appearance)
                    features_by_id.setdefault(key, []).append(feature.geometry)

        clipping_box = box(bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y)
        result: list[BgtSurfaceFeature] = []
        for layer_name, feature_id, physical_appearance in sorted(features_by_id):
            geometries = features_by_id[(layer_name, feature_id, physical_appearance)]
            try:
                geometry = geometries[0] if len(geometries) == 1 else unary_union(geometries)
                if not geometry.is_valid:
                    geometry = make_valid(geometry)
                geometry = geometry.intersection(clipping_box)
            except Exception:
                continue
            if geometry.is_empty:
                continue
            result.append(
                BgtSurfaceFeature(
                    layer_name=layer_name,
                    feature_id=feature_id,
                    physical_appearance=physical_appearance,
                    geometry=geometry,
                )
            )
        return result

    def _tiles_for_bounds(self, bounds: Bounds) -> list[tuple[int, int]]:
        epsilon = 1e-9
        min_col = floor((bounds.min_x - self.ORIGIN_X) / self.TILE_SPAN_METERS)
        max_col = floor((bounds.max_x - self.ORIGIN_X - epsilon) / self.TILE_SPAN_METERS)
        min_row = floor((self.ORIGIN_Y - bounds.max_y) / self.TILE_SPAN_METERS)
        max_row = floor((self.ORIGIN_Y - bounds.min_y - epsilon) / self.TILE_SPAN_METERS)
        min_col = max(0, min(self.MATRIX_SIZE - 1, min_col))
        max_col = max(0, min(self.MATRIX_SIZE - 1, max_col))
        min_row = max(0, min(self.MATRIX_SIZE - 1, min_row))
        max_row = max(0, min(self.MATRIX_SIZE - 1, max_row))
        if min_col > max_col or min_row > max_row:
            return []
        return [
            (row, col)
            for row in range(min_row, max_row + 1)
            for col in range(min_col, max_col + 1)
        ]

    def _tile_features(self, row: int, col: int) -> list[tuple[str, str, Any]]:
        decoded = self._decoded_tile(row, col)
        tile_min_x = self.ORIGIN_X + (col * self.TILE_SPAN_METERS)
        tile_max_y = self.ORIGIN_Y - (row * self.TILE_SPAN_METERS)
        tile_min_y = tile_max_y - self.TILE_SPAN_METERS
        result: list[tuple[str, str, Any]] = []
        for layer_name, layer in decoded.items():
            normalized_layer = str(layer_name).strip().casefold()
            if normalized_layer in self.EXCLUDED_LAYERS:
                continue
            try:
                extent = max(1.0, float(layer.get("extent") or 4096.0))
            except (TypeError, ValueError):
                extent = 4096.0
            for feature_index, feature in enumerate(layer.get("features") or []):
                geometry_payload = feature.get("geometry") or {}
                geometry_type = str(geometry_payload.get("type") or "")
                if geometry_type in {"", "Point", "MultiPoint"}:
                    continue
                world_payload = {
                    "type": geometry_type,
                    "coordinates": self._world_coordinates(
                        geometry_payload.get("coordinates"),
                        tile_min_x,
                        tile_min_y,
                        extent,
                    ),
                }
                try:
                    geometry = shape(world_payload)
                except Exception:
                    continue
                if geometry.is_empty:
                    continue
                properties = feature.get("properties") or {}
                feature_id = str(
                    properties.get("lokaal_id")
                    or properties.get("external_fid")
                    or feature.get("id")
                    or f"{row}:{col}:{feature_index}:{dumps(geometry_payload, sort_keys=True)}"
                )
                result.append((normalized_layer, feature_id, geometry))
        return result

    def _tile_surface_features(self, row: int, col: int) -> list[BgtSurfaceFeature]:
        decoded = self._decoded_tile(row, col)
        tile_min_x = self.ORIGIN_X + (col * self.TILE_SPAN_METERS)
        tile_max_y = self.ORIGIN_Y - (row * self.TILE_SPAN_METERS)
        tile_min_y = tile_max_y - self.TILE_SPAN_METERS
        result: list[BgtSurfaceFeature] = []
        for layer_name, layer in decoded.items():
            normalized_layer = str(layer_name).strip().casefold()
            if normalized_layer in self.EXCLUDED_LAYERS:
                continue
            try:
                extent = max(1.0, float(layer.get("extent") or 4096.0))
            except (TypeError, ValueError):
                extent = 4096.0
            for feature_index, feature in enumerate(layer.get("features") or []):
                properties = feature.get("properties") or {}
                physical_appearance = str(properties.get("fysiek_voorkomen") or "").strip()
                if not physical_appearance:
                    continue
                geometry_payload = feature.get("geometry") or {}
                geometry_type = str(geometry_payload.get("type") or "")
                if geometry_type not in {"Polygon", "MultiPolygon"}:
                    continue
                world_payload = {
                    "type": geometry_type,
                    "coordinates": self._world_coordinates(
                        geometry_payload.get("coordinates"),
                        tile_min_x,
                        tile_min_y,
                        extent,
                    ),
                }
                try:
                    geometry = shape(world_payload)
                except Exception:
                    continue
                if geometry.is_empty:
                    continue
                feature_id = str(
                    properties.get("lokaal_id")
                    or properties.get("external_fid")
                    or feature.get("id")
                    or f"{row}:{col}:{feature_index}:{dumps(geometry_payload, sort_keys=True)}"
                )
                result.append(
                    BgtSurfaceFeature(
                        layer_name=normalized_layer,
                        feature_id=feature_id,
                        physical_appearance=physical_appearance,
                        geometry=geometry,
                    )
                )
        return result

    def _decoded_tile(self, row: int, col: int) -> dict[str, Any]:
        cache_key = (row, col)
        cached = self._cache_get(cache_key)
        if cached is not None:
            return cached
        url = f"{self.BASE_URL}/{self.TILE_MATRIX}/{row}/{col}?f=mvt"
        last_error: Exception | None = None
        for attempt in range(1, self.retries + 1):
            try:
                response = self._get_session().get(url, timeout=self.timeout)
                response.raise_for_status()
                decoded = mapbox_vector_tile.decode(response.content)
                self._cache_put(cache_key, decoded)
                return decoded
            except (requests.RequestException, ValueError, TypeError) as exc:
                last_error = exc
                if attempt < self.retries:
                    time.sleep(min(2.0, 0.4 * (2 ** (attempt - 1))))
        raise BgtVectorTileError(str(last_error))

    def _world_coordinates(
        self,
        value,
        tile_min_x: float,
        tile_min_y: float,
        extent: float,
    ):
        if (
            isinstance(value, (list, tuple))
            and len(value) >= 2
            and isinstance(value[0], (int, float))
            and isinstance(value[1], (int, float))
        ):
            return [
                tile_min_x + (float(value[0]) / extent * self.TILE_SPAN_METERS),
                tile_min_y + (float(value[1]) / extent * self.TILE_SPAN_METERS),
            ]
        if isinstance(value, (list, tuple)):
            return [self._world_coordinates(item, tile_min_x, tile_min_y, extent) for item in value]
        return value

    def _geometry_paths(self, geometry) -> list[list[tuple[float, float]]]:
        if geometry.is_empty:
            return []
        if isinstance(geometry, LineString):
            path = [(float(x), float(y)) for x, y, *_rest in geometry.coords]
            return [path] if len(path) >= 2 else []
        if isinstance(geometry, MultiLineString):
            paths: list[list[tuple[float, float]]] = []
            for part in geometry.geoms:
                paths.extend(self._geometry_paths(part))
            return paths
        if isinstance(geometry, Polygon):
            paths = [[(float(x), float(y)) for x, y, *_rest in geometry.exterior.coords]]
            paths.extend(
                [(float(x), float(y)) for x, y, *_rest in ring.coords]
                for ring in geometry.interiors
            )
            return [path for path in paths if len(path) >= 2]
        if isinstance(geometry, MultiPolygon):
            paths: list[list[tuple[float, float]]] = []
            for part in geometry.geoms:
                paths.extend(self._geometry_paths(part))
            return paths
        if isinstance(geometry, GeometryCollection):
            paths: list[list[tuple[float, float]]] = []
            for part in geometry.geoms:
                paths.extend(self._geometry_paths(part))
            return paths
        return []

    @staticmethod
    def _dedupe_paths(paths: list[list[tuple[float, float]]]) -> list[list[tuple[float, float]]]:
        deduped: list[list[tuple[float, float]]] = []
        seen: set[tuple[tuple[float, float], ...]] = set()
        for path in paths:
            if len(path) < 2:
                continue
            rounded = tuple((round(float(x), 3), round(float(y), 3)) for x, y in path)
            reverse = tuple(reversed(rounded))
            key = rounded if rounded <= reverse else reverse
            if key in seen:
                continue
            seen.add(key)
            deduped.append(path)
        return deduped
