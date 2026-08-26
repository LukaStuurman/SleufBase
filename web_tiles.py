from __future__ import annotations

from collections import OrderedDict
from concurrent.futures import ThreadPoolExecutor, as_completed
import math
import os
import threading
import time
from datetime import datetime, timedelta, timezone
from io import BytesIO
from pathlib import Path
from typing import Callable

import requests
from PIL import Image, UnidentifiedImageError
from pyproj import Transformer

from .models import Bounds


class TileClientError(RuntimeError):
    """Raised when a tile-based background cannot be retrieved."""


class WebMercatorTileClient:
    WEB_MERCATOR_HALF_WORLD = 20037508.342789244
    WEB_MERCATOR_RESOLUTION_0 = (2 * WEB_MERCATOR_HALF_WORLD) / 256.0

    def __init__(
        self,
        cache_namespace: str,
        user_agent: str,
        timeout: int = 30,
        min_zoom: int = 0,
        max_zoom: int = 19,
        min_cache_ttl_days: int = 7,
        max_workers: int = 8,
        memory_cache_limit: int = 768,
        retries: int = 3,
    ) -> None:
        self.user_agent = user_agent
        self.timeout = timeout
        self.min_zoom = min_zoom
        self.max_zoom = max_zoom
        self.max_workers = max(1, int(max_workers))
        self.memory_cache_limit = max(64, int(memory_cache_limit))
        self.retries = max(1, int(retries))
        self.transformer = Transformer.from_crs("EPSG:28992", "EPSG:3857", always_xy=True)
        self._thread_local = threading.local()
        local_appdata = Path(os.environ.get("LOCALAPPDATA", Path.home() / "AppData" / "Local"))
        self.cache_dir = local_appdata / "SleufBase" / "cache" / cache_namespace
        self.cache_dir.mkdir(parents=True, exist_ok=True)
        self.min_cache_ttl = timedelta(days=min_cache_ttl_days)
        self._memory_cache: OrderedDict[tuple[int, int, int], Image.Image] = OrderedDict()
        self._memory_cache_lock = threading.RLock()

    def build_tile_url(self, zoom: int, x: int, y: int) -> str:
        raise NotImplementedError

    def configure_session(self, session: requests.Session) -> None:
        return

    def _create_session(self) -> requests.Session:
        session = requests.Session()
        session.headers.update({"User-Agent": self.user_agent})
        self.configure_session(session)
        return session

    def _get_session(self) -> requests.Session:
        session = getattr(self._thread_local, "session", None)
        if session is None:
            session = self._create_session()
            self._thread_local.session = session
        return session

    def fetch_map(
        self,
        bounds: Bounds,
        size: tuple[int, int],
        on_progress: Callable[[Image.Image], None] | None = None,
    ) -> Image.Image:
        if size[0] <= 0 or size[1] <= 0:
            raise TileClientError("Kaartgrootte moet groter dan nul zijn.")

        tile_request = self._prepare_tile_request(bounds, size)
        stitched, preview_tile_count = self._build_preview_stitched(tile_request, return_tile_count=True)
        total_tiles = len(tile_request["tile_coords"])
        if on_progress is not None and preview_tile_count > 0:
            on_progress(self._render_tile_request(tile_request, stitched))
        if total_tiles == 0:
            return self._render_tile_request(tile_request, stitched)

        progress_interval = max(1, min(8, total_tiles // 4 or 1))
        max_workers = min(self.max_workers, total_tiles)
        with ThreadPoolExecutor(max_workers=max_workers) as executor:
            future_map = {
                executor.submit(self._fetch_tile, int(tile_request["zoom"]), tile_x, tile_y): (tile_x, tile_y)
                for tile_x, tile_y in tile_request["tile_coords"]
            }
            completed = 0
            last_error: Exception | None = None
            for future in as_completed(future_map):
                tile_x, tile_y = future_map[future]
                try:
                    tile = future.result()
                except Exception as exc:
                    last_error = exc
                    continue
                stitched.alpha_composite(
                    tile,
                    (
                        (tile_x - int(tile_request["min_tile_x"])) * 256,
                        (tile_y - int(tile_request["min_tile_y"])) * 256,
                    ),
                )
                completed += 1
                if on_progress is not None and (
                    completed == 1
                    or completed == total_tiles
                    or completed % progress_interval == 0
                ):
                    on_progress(self._render_tile_request(tile_request, stitched))
        if completed == 0 and preview_tile_count == 0 and last_error is not None:
            raise TileClientError(f"Geen kaarttegels konden worden geladen: {last_error}") from last_error
        return self._render_tile_request(tile_request, stitched)

    def preview_map(self, bounds: Bounds, size: tuple[int, int]) -> Image.Image | None:
        if size[0] <= 0 or size[1] <= 0:
            return None
        tile_request = self._prepare_tile_request(bounds, size)
        preview, preview_tile_count = self._build_preview_stitched(tile_request, return_tile_count=True)
        if preview_tile_count <= 0:
            return None
        return self._render_tile_request(tile_request, preview)

    def _to_mercator_bounds(self, bounds: Bounds) -> Bounds:
        corners = [
            self.transformer.transform(bounds.min_x, bounds.min_y),
            self.transformer.transform(bounds.min_x, bounds.max_y),
            self.transformer.transform(bounds.max_x, bounds.min_y),
            self.transformer.transform(bounds.max_x, bounds.max_y),
        ]
        xs = [max(-self.WEB_MERCATOR_HALF_WORLD, min(self.WEB_MERCATOR_HALF_WORLD, x)) for x, _ in corners]
        ys = [max(-self.WEB_MERCATOR_HALF_WORLD, min(self.WEB_MERCATOR_HALF_WORLD, y)) for _, y in corners]
        return Bounds(min(xs), min(ys), max(xs), max(ys))

    def _prepare_tile_request(self, bounds: Bounds, size: tuple[int, int]) -> dict[str, object]:
        mercator_bounds = self._to_mercator_bounds(bounds)
        zoom = self._choose_zoom(mercator_bounds, size)
        resolution = self.WEB_MERCATOR_RESOLUTION_0 / (2**zoom)
        tile_span = resolution * 256.0
        tiles_per_axis = 2**zoom
        min_tile_x = max(0, min(tiles_per_axis - 1, int(math.floor((mercator_bounds.min_x + self.WEB_MERCATOR_HALF_WORLD) / tile_span))))
        max_tile_x = max(0, min(tiles_per_axis - 1, int(math.floor((mercator_bounds.max_x + self.WEB_MERCATOR_HALF_WORLD - 1e-9) / tile_span))))
        min_tile_y = max(0, min(tiles_per_axis - 1, int(math.floor((self.WEB_MERCATOR_HALF_WORLD - mercator_bounds.max_y) / tile_span))))
        max_tile_y = max(0, min(tiles_per_axis - 1, int(math.floor((self.WEB_MERCATOR_HALF_WORLD - mercator_bounds.min_y - 1e-9) / tile_span))))
        tile_coords = [
            (tile_x, tile_y)
            for tile_y in range(min_tile_y, max_tile_y + 1)
            for tile_x in range(min_tile_x, max_tile_x + 1)
        ]
        return {
            "mercator_bounds": mercator_bounds,
            "zoom": int(zoom),
            "resolution": float(resolution),
            "tile_span": float(tile_span),
            "min_tile_x": int(min_tile_x),
            "max_tile_x": int(max_tile_x),
            "min_tile_y": int(min_tile_y),
            "max_tile_y": int(max_tile_y),
            "tile_coords": tile_coords,
            "stitched_size": (
                (max_tile_x - min_tile_x + 1) * 256,
                (max_tile_y - min_tile_y + 1) * 256,
            ),
            "target_size": size,
        }

    def _build_preview_stitched(
        self,
        tile_request: dict[str, object],
        *,
        return_tile_count: bool = False,
    ):
        stitched = Image.new("RGBA", tuple(tile_request["stitched_size"]), (243, 243, 243, 255))
        zoom = int(tile_request["zoom"])
        min_tile_x = int(tile_request["min_tile_x"])
        min_tile_y = int(tile_request["min_tile_y"])
        preview_tile_count = 0
        for tile_x, tile_y in tile_request["tile_coords"]:
            preview_tile = self._best_available_tile(zoom, tile_x, tile_y)
            if preview_tile is None:
                continue
            preview_tile_count += 1
            stitched.alpha_composite(
                preview_tile,
                (
                    (tile_x - min_tile_x) * 256,
                    (tile_y - min_tile_y) * 256,
                ),
            )
        if return_tile_count:
            return stitched, preview_tile_count
        return stitched

    def _render_tile_request(self, tile_request: dict[str, object], stitched: Image.Image) -> Image.Image:
        mercator_bounds = tile_request["mercator_bounds"]
        resolution = float(tile_request["resolution"])
        min_tile_x = int(tile_request["min_tile_x"])
        min_tile_y = int(tile_request["min_tile_y"])
        tile_span = float(tile_request["tile_span"])
        target_size = tuple(tile_request["target_size"])
        tile_origin_x = min_tile_x * tile_span - self.WEB_MERCATOR_HALF_WORLD
        tile_origin_y = self.WEB_MERCATOR_HALF_WORLD - min_tile_y * tile_span
        left = int(round((mercator_bounds.min_x - tile_origin_x) / resolution))
        top = int(round((tile_origin_y - mercator_bounds.max_y) / resolution))
        right = int(round((mercator_bounds.max_x - tile_origin_x) / resolution))
        bottom = int(round((tile_origin_y - mercator_bounds.min_y) / resolution))
        cropped = stitched.crop((left, top, right, bottom))
        if cropped.size != target_size:
            cropped = cropped.resize(target_size, Image.Resampling.BILINEAR)
        return cropped

    def _choose_zoom(self, mercator_bounds: Bounds, size: tuple[int, int]) -> int:
        meters_per_pixel = max(mercator_bounds.width / max(size[0], 1), mercator_bounds.height / max(size[1], 1), 0.01)
        zoom = int(round(math.log2(self.WEB_MERCATOR_RESOLUTION_0 / meters_per_pixel)))
        return max(self.min_zoom, min(self.max_zoom, zoom))

    def _tile_path(self, zoom: int, x: int, y: int) -> Path:
        return self.cache_dir / f"{zoom}_{x}_{y}.png"

    def _load_cached_tile(self, zoom: int, x: int, y: int, *, allow_stale: bool) -> Image.Image | None:
        cache_key = (zoom, x, y)
        with self._memory_cache_lock:
            cached_image = self._memory_cache.get(cache_key)
            if cached_image is not None:
                self._memory_cache.move_to_end(cache_key)
                return cached_image.copy()

        tile_path = self._tile_path(zoom, x, y)
        try:
            stat = tile_path.stat()
        except FileNotFoundError:
            return None
        except OSError:
            return None

        age = datetime.now(timezone.utc) - datetime.fromtimestamp(stat.st_mtime, tz=timezone.utc)
        if not allow_stale and age > self.min_cache_ttl:
            return None
        try:
            with Image.open(tile_path) as image:
                tile = image.convert("RGBA")
                tile.load()
        except (OSError, UnidentifiedImageError, ValueError):
            try:
                tile_path.unlink(missing_ok=True)
            except OSError:
                pass
            return None
        if tile.size != (256, 256):
            try:
                tile_path.unlink(missing_ok=True)
            except OSError:
                pass
            return None
        self._remember_tile(cache_key, tile)
        return tile.copy()

    def _best_available_tile(self, zoom: int, x: int, y: int) -> Image.Image | None:
        exact_tile = self._load_cached_tile(zoom, x, y, allow_stale=True)
        if exact_tile is not None:
            return exact_tile
        for levels_up in range(1, min(zoom, 4) + 1):
            parent_zoom = zoom - levels_up
            parent_x = x >> levels_up
            parent_y = y >> levels_up
            parent_tile = self._load_cached_tile(parent_zoom, parent_x, parent_y, allow_stale=True)
            if parent_tile is None:
                continue
            scale = 2**levels_up
            sub_tile_size = 256 / scale
            offset_x = x % scale
            offset_y = y % scale
            left = int(round(offset_x * sub_tile_size))
            top = int(round(offset_y * sub_tile_size))
            right = int(round((offset_x + 1) * sub_tile_size))
            bottom = int(round((offset_y + 1) * sub_tile_size))
            if right <= left or bottom <= top:
                continue
            cropped = parent_tile.crop((left, top, right, bottom))
            return cropped.resize((256, 256), Image.Resampling.BILINEAR)
        return None

    def _write_tile_atomically(self, tile_path: Path, tile: Image.Image) -> None:
        temp_path = tile_path.with_name(
            f".{tile_path.name}.{os.getpid()}.{threading.get_ident()}.{time.time_ns()}.tmp"
        )
        try:
            tile.save(temp_path, format="PNG")
            os.replace(temp_path, tile_path)
        finally:
            try:
                temp_path.unlink(missing_ok=True)
            except OSError:
                pass

    def _fetch_tile(self, zoom: int, x: int, y: int) -> Image.Image:
        cache_key = (zoom, x, y)
        cached_tile = self._load_cached_tile(zoom, x, y, allow_stale=False)
        if cached_tile is not None:
            return cached_tile

        tile_path = self._tile_path(zoom, x, y)
        url = self.build_tile_url(zoom, x, y)
        last_error: Exception | None = None
        for attempt in range(1, self.retries + 1):
            try:
                response = self._get_session().get(url, timeout=self.timeout)
                response.raise_for_status()
                content_type = response.headers.get("content-type", "")
                if "image" not in content_type.casefold():
                    raise TileClientError(f"Geen afbeelding ontvangen voor tegel {zoom}/{x}/{y}.")
                with Image.open(BytesIO(response.content)) as source:
                    tile = source.convert("RGBA")
                    tile.load()
                if tile.size != (256, 256):
                    raise TileClientError(
                        f"Ongeldige tegelgrootte {tile.size} voor {zoom}/{x}/{y}; 256x256 verwacht."
                    )
                self._write_tile_atomically(tile_path, tile)
                self._remember_tile(cache_key, tile)
                return tile.copy()
            except (requests.RequestException, OSError, UnidentifiedImageError, ValueError, TileClientError) as exc:
                last_error = exc
                if attempt < self.retries:
                    time.sleep(0.4 * attempt)
        stale_tile = self._load_cached_tile(zoom, x, y, allow_stale=True)
        if stale_tile is not None:
            return stale_tile
        raise TileClientError(f"Tegel {zoom}/{x}/{y} kon niet worden geladen: {last_error}") from last_error

    def _remember_tile(self, cache_key: tuple[int, int, int], tile: Image.Image) -> None:
        with self._memory_cache_lock:
            self._memory_cache[cache_key] = tile.copy()
            self._memory_cache.move_to_end(cache_key)
            while len(self._memory_cache) > self.memory_cache_limit:
                self._memory_cache.popitem(last=False)
