from __future__ import annotations

from collections import OrderedDict
from io import BytesIO
import threading
import time

import numpy as np
import requests
from PIL import Image


class PdokAhnError(RuntimeError):
    """Raised when PDOK AHN data cannot be retrieved."""


class PdokAhnClient:
    BASE_URL = "https://service.pdok.nl/rws/ahn/wcs/v1_0"
    DEFAULT_NODATA = 3.4028234663852886e38
    CACHE_LIMIT = 512

    def __init__(
        self,
        *,
        coverage_id: str = "dtm_05m",
        timeout: int = 12,
        retries: int = 1,
        sample_size_meters: float = 1.0,
    ) -> None:
        self.coverage_id = str(coverage_id or "dtm_05m").strip() or "dtm_05m"
        self.timeout = max(1, int(timeout))
        self.retries = max(1, int(retries))
        self.sample_size_meters = max(0.2, float(sample_size_meters))
        self._thread_local = threading.local()
        self._cache_lock = threading.RLock()
        self._cache: OrderedDict[tuple[float, float, str], float | None] = OrderedDict()

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

    def _cache_key(self, x: float, y: float) -> tuple[float, float, str]:
        return (round(float(x), 2), round(float(y), 2), self.coverage_id)

    def _cache_get(self, key: tuple[float, float, str]) -> tuple[bool, float | None]:
        with self._cache_lock:
            if key not in self._cache:
                return False, None
            value = self._cache.pop(key)
            self._cache[key] = value
            return True, value

    def _cache_put(self, key: tuple[float, float, str], value: float | None) -> None:
        with self._cache_lock:
            self._cache.pop(key, None)
            self._cache[key] = value
            while len(self._cache) > self.CACHE_LIMIT:
                self._cache.popitem(last=False)

    def fetch_ground_level(self, x: float, y: float) -> float | None:
        key = self._cache_key(x, y)
        found, cached = self._cache_get(key)
        if found:
            return cached

        value = self._request_ground_level(float(x), float(y))
        self._cache_put(key, value)
        return value

    def lookup_cached_ground_level(self, x: float, y: float) -> tuple[bool, float | None]:
        return self._cache_get(self._cache_key(x, y))

    def _request_ground_level(self, x: float, y: float) -> float | None:
        search_windows = (
            (max(self.sample_size_meters, 2.0), 5),
            (max(self.sample_size_meters * 4.0, 8.0), 9),
            (max(self.sample_size_meters * 12.0, 24.0), 13),
        )
        for window_size_meters, pixel_size in search_windows:
            half_size = window_size_meters * 0.5
            params = [
                ("SERVICE", "WCS"),
                ("VERSION", "2.0.1"),
                ("REQUEST", "GetCoverage"),
                ("CoverageID", self.coverage_id),
                ("format", "image/tiff"),
                ("crs", "http://www.opengis.net/def/crs/EPSG/0/28992"),
                ("scalesize", f"x({pixel_size}),y({pixel_size})"),
                ("subset", f"x({x - half_size:.3f},{x + half_size:.3f})"),
                ("subset", f"y({y - half_size:.3f},{y + half_size:.3f})"),
            ]
            for attempt in range(1, self.retries + 1):
                try:
                    response = self._get_session().get(self.BASE_URL, params=params, timeout=self.timeout)
                    response.raise_for_status()
                    content_type = response.headers.get("content-type", "")
                    if "image" not in content_type.lower():
                        snippet = response.text[:300].strip()
                        raise PdokAhnError(f"PDOK AHN gaf geen raster terug: {snippet}")
                    with Image.open(BytesIO(response.content)) as image:
                        nearest = self._nearest_valid_value(image)
                    if nearest is not None:
                        return round(nearest, 3)
                    break
                except (requests.RequestException, OSError, ValueError, PdokAhnError):
                    if attempt >= self.retries:
                        break
                    time.sleep(min(1.5, 0.2 * (2 ** (attempt - 1))))
        return None

    def _nodata_value(self, image: Image.Image) -> float:
        raw_value = image.tag_v2.get(42113, self.DEFAULT_NODATA)
        try:
            return float(raw_value)
        except (TypeError, ValueError):
            return float(self.DEFAULT_NODATA)

    def _nearest_valid_value(self, image: Image.Image) -> float | None:
        no_data = self._nodata_value(image)
        pixels = np.asarray(image, dtype=float)
        if pixels.ndim == 0:
            pixels = np.array([[float(pixels)]], dtype=float)
        elif pixels.ndim == 1:
            pixels = pixels.reshape((1, pixels.shape[0]))

        valid_mask = np.isfinite(pixels)
        valid_mask &= np.abs(pixels - no_data) > 1e-6
        valid_mask &= pixels < (no_data * 0.99)
        if not bool(valid_mask.any()):
            return None

        center_row = (pixels.shape[0] - 1) / 2.0
        center_col = (pixels.shape[1] - 1) / 2.0
        valid_rows, valid_cols = np.where(valid_mask)
        distances = ((valid_rows - center_row) ** 2) + ((valid_cols - center_col) ** 2)
        nearest_index = int(np.argmin(distances))
        nearest_row = int(valid_rows[nearest_index])
        nearest_col = int(valid_cols[nearest_index])
        return float(pixels[nearest_row, nearest_col])
