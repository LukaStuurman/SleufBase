from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor, as_completed
from io import BytesIO
from math import ceil
import re
import threading
import time
from typing import Callable

import requests
from PIL import Image

from .models import Bounds


class CyclomediaAerialError(RuntimeError):
    """Raised when Cyclomedia aerial imagery cannot be retrieved."""


class CyclomediaAerialClient:
    BASE_URL = "https://atlasapi.cyclomedia.com/api/geodata/wms"
    CAPABILITIES_TTL_SECONDS = 12 * 60 * 60
    EXACT_CACHE_MAX_PIXELS = 32_000_000
    COVERAGE_CACHE_MAX_PIXELS = 48_000_000
    PREFETCH_MAX_DIMENSION = 8192
    PREFETCH_MAX_PIXELS = 32_000_000
    PREFETCH_MAX_CLUSTER_WORKERS = 2
    LAYER_PATTERN = re.compile(r"<Name>(NL_aerial_(\d{4})_(\d+)cm)</Name>", re.IGNORECASE)

    def __init__(
        self,
        username_getter: Callable[[], str],
        password_getter: Callable[[], str],
        *,
        timeout: int = 30,
        retries: int = 3,
    ) -> None:
        self._username_getter = username_getter
        self._password_getter = password_getter
        self.timeout = timeout
        self.retries = max(1, retries)
        self._thread_local = threading.local()
        self.session = self._create_session()
        self._cache: dict[tuple[str, float, float, float, float, int, int], Image.Image] = {}
        self._cache_pixels = 0
        self._coverage_cache: list[dict[str, object]] = []
        self._coverage_cache_pixels = 0
        self._cache_lock = threading.RLock()
        self._resolve_lock = threading.Lock()
        self._resolved_layer_name: str | None = None
        self._resolved_layer_label: str | None = None
        self._resolved_at: float = 0.0

    def _create_session(self) -> requests.Session:
        session = requests.Session()
        session.headers.update({"User-Agent": "KLIC-TIFF-Kaarten/1.0"})
        return session

    def _get_session(self) -> requests.Session:
        session = getattr(self._thread_local, "session", None)
        if session is None:
            session = self._create_session()
            self._thread_local.session = session
        return session

    def _cache_key(self, layer_name: str, bounds: Bounds, size: tuple[int, int]) -> tuple[str, float, float, float, float, int, int]:
        return (
            layer_name,
            round(bounds.min_x, 3),
            round(bounds.min_y, 3),
            round(bounds.max_x, 3),
            round(bounds.max_y, 3),
            int(size[0]),
            int(size[1]),
        )

    def _cached_image(self, key) -> Image.Image | None:
        with self._cache_lock:
            cached = self._cache.pop(key, None)
            if cached is None:
                return None
            self._cache[key] = cached
            return cached.copy()

    def _remember_cached_image(self, key, image: Image.Image) -> None:
        pixel_count = int(image.width) * int(image.height)
        if pixel_count > self.EXACT_CACHE_MAX_PIXELS:
            return
        stored = image.copy()
        with self._cache_lock:
            previous = self._cache.pop(key, None)
            if previous is not None:
                self._cache_pixels -= int(previous.width) * int(previous.height)
            self._cache[key] = stored
            self._cache_pixels += pixel_count
            while self._cache and self._cache_pixels > self.EXACT_CACHE_MAX_PIXELS:
                oldest_key = next(iter(self._cache))
                oldest = self._cache.pop(oldest_key)
                self._cache_pixels -= int(oldest.width) * int(oldest.height)

    @staticmethod
    def _bounds_contains(outer: Bounds, inner: Bounds, tolerance: float = 1e-6) -> bool:
        return (
            outer.min_x <= inner.min_x + tolerance
            and outer.min_y <= inner.min_y + tolerance
            and outer.max_x >= inner.max_x - tolerance
            and outer.max_y >= inner.max_y - tolerance
        )

    def _image_from_coverage(
        self,
        layer_name: str,
        bounds: Bounds,
        size: tuple[int, int],
    ) -> Image.Image | None:
        target_x_resolution = float(bounds.width) / max(1, int(size[0]))
        target_y_resolution = float(bounds.height) / max(1, int(size[1]))
        with self._cache_lock:
            candidates = list(reversed(self._coverage_cache))
        for coverage in candidates:
            if coverage.get("layer_name") != layer_name:
                continue
            source_bounds = coverage.get("bounds")
            source_image = coverage.get("image")
            if not isinstance(source_bounds, Bounds) or not isinstance(source_image, Image.Image):
                continue
            if not self._bounds_contains(source_bounds, bounds):
                continue
            source_x_resolution = float(source_bounds.width) / max(1, source_image.width)
            source_y_resolution = float(source_bounds.height) / max(1, source_image.height)
            if (
                source_x_resolution > target_x_resolution * 1.02
                or source_y_resolution > target_y_resolution * 1.02
            ):
                continue
            if (
                source_image.size == (int(size[0]), int(size[1]))
                and abs(source_bounds.min_x - bounds.min_x) <= 1e-6
                and abs(source_bounds.min_y - bounds.min_y) <= 1e-6
                and abs(source_bounds.max_x - bounds.max_x) <= 1e-6
                and abs(source_bounds.max_y - bounds.max_y) <= 1e-6
            ):
                return source_image.copy()
            left = ((bounds.min_x - source_bounds.min_x) / source_bounds.width) * source_image.width
            right = ((bounds.max_x - source_bounds.min_x) / source_bounds.width) * source_image.width
            top = ((source_bounds.max_y - bounds.max_y) / source_bounds.height) * source_image.height
            bottom = ((source_bounds.max_y - bounds.min_y) / source_bounds.height) * source_image.height
            return source_image.transform(
                (int(size[0]), int(size[1])),
                Image.Transform.EXTENT,
                (left, top, right, bottom),
                resample=Image.Resampling.BICUBIC,
            )
        return None

    def _remember_coverage(
        self,
        layer_name: str,
        bounds: Bounds,
        image: Image.Image,
    ) -> None:
        pixel_count = int(image.width) * int(image.height)
        if pixel_count > self.COVERAGE_CACHE_MAX_PIXELS:
            return
        coverage = {
            "layer_name": layer_name,
            "bounds": bounds,
            "image": image.copy(),
            "pixels": pixel_count,
        }
        with self._cache_lock:
            exact_key = self._cache_key(layer_name, bounds, image.size)
            duplicate = self._cache.pop(exact_key, None)
            if duplicate is not None:
                self._cache_pixels -= int(duplicate.width) * int(duplicate.height)
            self._coverage_cache.append(coverage)
            self._coverage_cache_pixels += pixel_count
            while self._coverage_cache and self._coverage_cache_pixels > self.COVERAGE_CACHE_MAX_PIXELS:
                oldest = self._coverage_cache.pop(0)
                self._coverage_cache_pixels -= int(oldest.get("pixels", 0) or 0)

    def _credentials(self) -> tuple[str, str]:
        username = str(self._username_getter() or "").strip()
        password = str(self._password_getter() or "")
        if not username or not password:
            raise CyclomediaAerialError(
                "Vul eerst StreetSmart gebruikersnaam en wachtwoord in voor Cyclomedia Luchtfoto NL."
            )
        return username, password

    def _resolve_latest_layer(self, username: str, password: str) -> tuple[str, str]:
        with self._resolve_lock:
            return self._resolve_latest_layer_serial(username, password)

    def _resolve_latest_layer_serial(self, username: str, password: str) -> tuple[str, str]:
        now = time.time()
        if (
            self._resolved_layer_name
            and self._resolved_layer_label
            and (now - self._resolved_at) < self.CAPABILITIES_TTL_SECONDS
        ):
            return self._resolved_layer_name, self._resolved_layer_label

        params = {
            "SERVICE": "WMS",
            "VERSION": "1.1.1",
            "REQUEST": "GetCapabilities",
        }
        last_error: Exception | None = None
        for attempt in range(1, self.retries + 1):
            try:
                response = self.session.get(
                    self.BASE_URL,
                    params=params,
                    auth=(username, password),
                    timeout=self.timeout,
                )
                response.raise_for_status()
                matches = [
                    (int(year), int(cm), layer_name)
                    for layer_name, year, cm in self.LAYER_PATTERN.findall(response.text)
                ]
                if not matches:
                    raise CyclomediaAerialError("Geen Cyclomedia Luchtfoto NL-lagen gevonden in WMS capabilities.")
                latest_year = max(year for year, _cm, _name in matches)
                latest_matches = [item for item in matches if item[0] == latest_year]
                best_year, best_cm, best_name = min(latest_matches, key=lambda item: item[1])
                label = f"Luchtfoto NL {best_year} {best_cm}cm"
                self._resolved_layer_name = best_name
                self._resolved_layer_label = label
                self._resolved_at = now
                return best_name, label
            except (requests.RequestException, CyclomediaAerialError) as exc:
                last_error = exc
                if attempt >= self.retries:
                    break
                time.sleep(0.5 * attempt)
        raise CyclomediaAerialError(f"Cyclomedia capabilities ophalen mislukt: {last_error}")

    def current_layer_label(self) -> str | None:
        if not self._resolved_layer_label:
            return None
        return self._resolved_layer_label

    def _request_image(
        self,
        layer_name: str,
        bounds: Bounds,
        size: tuple[int, int],
        username: str,
        password: str,
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
        last_error: Exception | None = None
        for attempt in range(1, self.retries + 1):
            try:
                response = self._get_session().get(
                    self.BASE_URL,
                    params=params,
                    auth=(username, password),
                    timeout=self.timeout,
                )
                response.raise_for_status()
                content_type = response.headers.get("content-type", "")
                if "image" not in content_type:
                    snippet = response.text[:300].strip()
                    raise CyclomediaAerialError(f"Cyclomedia gaf geen luchtfoto terug: {snippet}")
                return Image.open(BytesIO(response.content)).convert("RGBA")
            except (requests.RequestException, OSError, CyclomediaAerialError) as exc:
                last_error = exc
                if attempt >= self.retries:
                    break
                time.sleep(0.6 * attempt)
        raise CyclomediaAerialError(f"Cyclomedia luchtfoto ophalen mislukt: {last_error}")

    @staticmethod
    def _combined_bounds(first: Bounds, second: Bounds) -> Bounds:
        return Bounds(
            min(first.min_x, second.min_x),
            min(first.min_y, second.min_y),
            max(first.max_x, second.max_x),
            max(first.max_y, second.max_y),
        )

    @staticmethod
    def _tile_count(size: tuple[int, int], tile_size: int = 2048) -> int:
        return max(1, ceil(int(size[0]) / tile_size)) * max(1, ceil(int(size[1]) / tile_size))

    def _prefetch_clusters(
        self,
        requests_to_prepare: list[tuple[Bounds, tuple[int, int]]],
    ) -> list[tuple[Bounds, tuple[int, int]]]:
        clusters: list[dict[str, object]] = []
        for bounds, size in requests_to_prepare:
            width = max(1, int(size[0]))
            height = max(1, int(size[1]))
            x_resolution = float(bounds.width) / width
            y_resolution = float(bounds.height) / height
            request_tiles = self._tile_count((width, height))
            best_index: int | None = None
            best_area: int | None = None
            best_values: tuple[Bounds, int, int, float, float] | None = None
            for cluster_index, cluster in enumerate(clusters):
                cluster_bounds = cluster["bounds"]
                if not isinstance(cluster_bounds, Bounds):
                    continue
                combined = self._combined_bounds(cluster_bounds, bounds)
                combined_x_resolution = min(float(cluster["x_resolution"]), x_resolution)
                combined_y_resolution = min(float(cluster["y_resolution"]), y_resolution)
                combined_width = max(1, ceil(float(combined.width) / combined_x_resolution))
                combined_height = max(1, ceil(float(combined.height) / combined_y_resolution))
                combined_pixels = combined_width * combined_height
                if (
                    combined_width > self.PREFETCH_MAX_DIMENSION
                    or combined_height > self.PREFETCH_MAX_DIMENSION
                    or combined_pixels > self.PREFETCH_MAX_PIXELS
                ):
                    continue
                separate_tiles = int(cluster["source_tiles"]) + request_tiles
                if self._tile_count((combined_width, combined_height)) >= separate_tiles:
                    continue
                if best_area is None or combined_pixels < best_area:
                    best_index = cluster_index
                    best_area = combined_pixels
                    best_values = (
                        combined,
                        combined_width,
                        combined_height,
                        combined_x_resolution,
                        combined_y_resolution,
                    )
            if best_index is None or best_values is None:
                clusters.append(
                    {
                        "bounds": bounds,
                        "width": width,
                        "height": height,
                        "x_resolution": x_resolution,
                        "y_resolution": y_resolution,
                        "source_tiles": request_tiles,
                    }
                )
                continue
            combined, combined_width, combined_height, combined_x_resolution, combined_y_resolution = best_values
            cluster = clusters[best_index]
            cluster["bounds"] = combined
            cluster["width"] = combined_width
            cluster["height"] = combined_height
            cluster["x_resolution"] = combined_x_resolution
            cluster["y_resolution"] = combined_y_resolution
            cluster["source_tiles"] = int(cluster["source_tiles"]) + request_tiles

        return [
            (
                cluster["bounds"],
                (int(cluster["width"]), int(cluster["height"])),
            )
            for cluster in clusters
            if isinstance(cluster.get("bounds"), Bounds)
        ]

    def prefetch_maps(
        self,
        requests_to_prepare: list[tuple[Bounds, tuple[int, int]]],
    ) -> dict[str, int]:
        normalized_requests = [
            (bounds, (max(1, int(size[0])), max(1, int(size[1]))))
            for bounds, size in requests_to_prepare
            if int(size[0]) > 0 and int(size[1]) > 0
        ]
        if not normalized_requests:
            return {"maps": 0, "clusters": 0, "tiles": 0}
        username, password = self._credentials()
        layer_name, _layer_label = self._resolve_latest_layer(username, password)
        clusters = self._prefetch_clusters(normalized_requests)

        def fetch_cluster(cluster: tuple[Bounds, tuple[int, int]]) -> int:
            cluster_bounds, cluster_size = cluster
            image = self.fetch_map(cluster_bounds, cluster_size)
            self._remember_coverage(layer_name, cluster_bounds, image)
            return self._tile_count(cluster_size)

        tile_total = 0
        max_workers = max(1, min(self.PREFETCH_MAX_CLUSTER_WORKERS, len(clusters)))
        with ThreadPoolExecutor(max_workers=max_workers, thread_name_prefix="cyclomedia-coverages") as executor:
            futures = [executor.submit(fetch_cluster, cluster) for cluster in clusters]
            for future in as_completed(futures):
                tile_total += int(future.result())
        return {"maps": len(normalized_requests), "clusters": len(clusters), "tiles": tile_total}

    def fetch_map(
        self,
        bounds: Bounds,
        size: tuple[int, int],
        max_tile_size: int = 2048,
        on_progress: Callable[[Image.Image], None] | None = None,
    ) -> Image.Image:
        if size[0] <= 0 or size[1] <= 0:
            raise CyclomediaAerialError("Kaartgrootte moet groter dan nul zijn.")

        username, password = self._credentials()
        layer_name, _layer_label = self._resolve_latest_layer(username, password)
        key = self._cache_key(layer_name, bounds, size)
        cached = self._cached_image(key)
        if cached is not None:
            return cached
        covered = self._image_from_coverage(layer_name, bounds, size)
        if covered is not None:
            if on_progress is not None:
                on_progress(covered.copy())
            return covered

        width, height = size
        effective_tile_size = min(max_tile_size, 512) if on_progress is not None else max_tile_size
        if width <= effective_tile_size and height <= effective_tile_size:
            image = self._request_image(layer_name, bounds, size, username, password)
            self._remember_cached_image(key, image)
            if on_progress is not None:
                on_progress(image.copy())
            return image

        stitched = Image.new("RGBA", size, (243, 243, 243, 255))
        tasks: list[tuple[int, int, int, int, Bounds]] = []
        columns = ceil(width / effective_tile_size)
        rows = ceil(height / effective_tile_size)
        for row in range(rows):
            for column in range(columns):
                left_px = column * effective_tile_size
                top_px = row * effective_tile_size
                right_px = min(width, left_px + effective_tile_size)
                bottom_px = min(height, top_px + effective_tile_size)
                tile_bounds = Bounds(
                    min_x=bounds.min_x + (left_px / width) * bounds.width,
                    min_y=bounds.max_y - (bottom_px / height) * bounds.height,
                    max_x=bounds.min_x + (right_px / width) * bounds.width,
                    max_y=bounds.max_y - (top_px / height) * bounds.height,
                )
                tasks.append((left_px, top_px, right_px - left_px, bottom_px - top_px, tile_bounds))

        progress_interval = max(1, min(6, len(tasks) // 3 or 1))
        with ThreadPoolExecutor(max_workers=min(6, len(tasks))) as executor:
            future_map = {
                executor.submit(
                    self._request_image,
                    layer_name,
                    tile_bounds,
                    (tile_width, tile_height),
                    username,
                    password,
                ): (left_px, top_px)
                for left_px, top_px, tile_width, tile_height, tile_bounds in tasks
            }
            completed = 0
            for future in as_completed(future_map):
                left_px, top_px = future_map[future]
                tile = future.result()
                stitched.alpha_composite(tile, (left_px, top_px))
                completed += 1
                if on_progress is not None and (
                    completed == 1
                    or completed == len(tasks)
                    or completed % progress_interval == 0
                ):
                    on_progress(stitched.copy())
        self._remember_cached_image(key, stitched)
        return stitched
