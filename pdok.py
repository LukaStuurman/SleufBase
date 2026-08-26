from __future__ import annotations

from collections import OrderedDict
from concurrent.futures import ThreadPoolExecutor, as_completed
from io import BytesIO
from math import ceil
import threading
import time
from typing import Callable

import requests
from PIL import Image

from .models import Bounds
from .web_tiles import WebMercatorTileClient


class PdokError(RuntimeError):
    """Raised when the PDOK background cannot be retrieved."""


class PdokWmtsTileClient(WebMercatorTileClient):
    BASE_URL = "https://service.pdok.nl/hwh/luchtfotorgb/wmts/v1_0"

    def __init__(
        self,
        layer_name: str = "Actueel_orthoHR",
        timeout: int = 30,
        max_workers: int = 8,
        retries: int = 3,
    ) -> None:
        super().__init__(
            cache_namespace=f"pdok_{layer_name.lower()}",
            user_agent="SleufBase/1.3",
            timeout=timeout,
            min_zoom=0,
            max_zoom=19,
            min_cache_ttl_days=7,
            max_workers=max_workers,
            retries=retries,
        )
        self.layer_name = layer_name

    def build_tile_url(self, zoom: int, x: int, y: int) -> str:
        matrix_id = f"{zoom:02d}"
        return (
            f"{self.BASE_URL}?SERVICE=WMTS&REQUEST=GetTile&VERSION=1.0.0"
            f"&LAYER={self.layer_name}&STYLE=default&FORMAT=image/png"
            f"&TILEMATRIXSET=OGC:1.0:GoogleMapsCompatible"
            f"&TILEMATRIX={matrix_id}&TILEROW={y}&TILECOL={x}"
        )


class PdokKadastralekaartWmtsTileClient(WebMercatorTileClient):
    BASE_URL = "https://service.pdok.nl/kadaster/kadastralekaart/wmts/v5_0"

    def __init__(self, timeout: int = 30, max_workers: int = 8, retries: int = 3) -> None:
        super().__init__(
            cache_namespace="pdok_kadastralekaart",
            user_agent="SleufBase/1.3",
            timeout=timeout,
            min_zoom=0,
            max_zoom=22,
            min_cache_ttl_days=7,
            max_workers=max_workers,
            retries=retries,
        )

    def build_tile_url(self, zoom: int, x: int, y: int) -> str:
        matrix_id = f"{zoom:02d}"
        return f"{self.BASE_URL}/Kadastralekaart/EPSG:3857/{matrix_id}/{x}/{y}.png"


class PdokWmsClient:
    BASE_URL = "https://service.pdok.nl/hwh/luchtfotorgb/wms/v1_0"
    CACHE_LIMIT = 24

    def __init__(
        self,
        layer_name: str = "Actueel_orthoHR",
        timeout: int = 30,
        retries: int = 3,
        max_workers: int = 6,
        base_url: str | None = None,
        transparent: bool = False,
    ) -> None:
        self.layer_name = layer_name
        self.timeout = max(1, int(timeout))
        self.retries = max(1, int(retries))
        self.max_workers = max(1, int(max_workers))
        self.base_url = base_url or self.BASE_URL
        self.transparent = transparent
        self._thread_local = threading.local()
        self._cache_lock = threading.RLock()
        self._cache: OrderedDict[tuple[float, float, float, float, int, int], Image.Image] = OrderedDict()

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

    def _cache_key(self, bounds: Bounds, size: tuple[int, int]) -> tuple[float, float, float, float, int, int]:
        return (
            round(bounds.min_x, 3),
            round(bounds.min_y, 3),
            round(bounds.max_x, 3),
            round(bounds.max_y, 3),
            int(size[0]),
            int(size[1]),
        )

    def _cache_get(self, key: tuple[float, float, float, float, int, int]) -> Image.Image | None:
        with self._cache_lock:
            cached = self._cache.pop(key, None)
            if cached is None:
                return None
            self._cache[key] = cached
            return cached.copy()

    def _cache_put(self, key: tuple[float, float, float, float, int, int], image: Image.Image) -> None:
        stored = image.copy()
        with self._cache_lock:
            previous = self._cache.pop(key, None)
            if previous is not None:
                previous.close()
            self._cache[key] = stored
            while len(self._cache) > self.CACHE_LIMIT:
                _old_key, old_image = self._cache.popitem(last=False)
                old_image.close()

    def _request_image(self, bounds: Bounds, size: tuple[int, int]) -> Image.Image:
        params = {
            "SERVICE": "WMS",
            "VERSION": "1.1.1",
            "REQUEST": "GetMap",
            "LAYERS": self.layer_name,
            "STYLES": "",
            "FORMAT": "image/png",
            "TRANSPARENT": "TRUE" if self.transparent else "FALSE",
            "SRS": "EPSG:28992",
            "WIDTH": int(size[0]),
            "HEIGHT": int(size[1]),
            "BBOX": f"{bounds.min_x},{bounds.min_y},{bounds.max_x},{bounds.max_y}",
        }
        last_error: Exception | None = None
        for attempt in range(1, self.retries + 1):
            try:
                response = self._get_session().get(self.base_url, params=params, timeout=self.timeout)
                response.raise_for_status()
                content_type = response.headers.get("content-type", "")
                if "image" not in content_type.lower():
                    snippet = response.text[:300].strip()
                    raise PdokError(f"PDOK gaf geen kaartbeeld terug: {snippet}")
                with Image.open(BytesIO(response.content)) as source:
                    return source.convert("RGBA")
            except (requests.RequestException, OSError, ValueError, PdokError) as exc:
                last_error = exc
                if attempt >= self.retries:
                    break
                time.sleep(min(2.0, 0.4 * (2 ** (attempt - 1))))
        raise PdokError(f"PDOK achtergrond ophalen mislukt: {last_error}")

    def fetch_map(
        self,
        bounds: Bounds,
        size: tuple[int, int],
        max_tile_size: int = 2048,
        on_progress: Callable[[Image.Image], None] | None = None,
    ) -> Image.Image:
        if size[0] <= 0 or size[1] <= 0:
            raise PdokError("Kaartgrootte moet groter dan nul zijn.")
        if bounds.width <= 0 or bounds.height <= 0:
            raise PdokError("Kaartuitsnede moet een positieve breedte en hoogte hebben.")
        key = self._cache_key(bounds, size)
        cached = self._cache_get(key)
        if cached is not None:
            return cached

        width, height = size
        effective_tile_size = max(64, int(max_tile_size))
        effective_tile_size = min(effective_tile_size, 512) if on_progress is not None else effective_tile_size
        if width <= effective_tile_size and height <= effective_tile_size:
            image = self._request_image(bounds, size)
            self._cache_put(key, image)
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
        with ThreadPoolExecutor(max_workers=min(self.max_workers, len(tasks))) as executor:
            future_map = {
                executor.submit(self._request_image, tile_bounds, (tile_width, tile_height)): (left_px, top_px)
                for left_px, top_px, tile_width, tile_height, tile_bounds in tasks
            }
            completed = 0
            for future in as_completed(future_map):
                left_px, top_px = future_map[future]
                tile = future.result()
                try:
                    stitched.alpha_composite(tile, (left_px, top_px))
                finally:
                    tile.close()
                completed += 1
                if on_progress is not None and (
                    completed == 1
                    or completed == len(tasks)
                    or completed % progress_interval == 0
                ):
                    on_progress(stitched.copy())
        self._cache_put(key, stitched)
        return stitched
