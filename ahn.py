from __future__ import annotations

from io import BytesIO
import math
import time

import numpy as np
import requests
from PIL import Image


class PdokAhnError(RuntimeError):
    """Raised when PDOK AHN data cannot be retrieved."""


class PdokAhnClient:
    BASE_URL = "https://service.pdok.nl/rws/ahn/wcs/v1_0"
    DEFAULT_NODATA = 3.4028234663852886e38

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
        self.session = requests.Session()
        self.session.headers.update({"User-Agent": "KLIC-TIFF-Kaarten/1.0"})
        self._cache: dict[tuple[float, float, str], float | None] = {}

    def fetch_ground_level(self, x: float, y: float) -> float | None:
        key = (round(float(x), 2), round(float(y), 2), self.coverage_id)
        if key in self._cache:
            return self._cache[key]

        value = self._request_ground_level(float(x), float(y))
        self._cache[key] = value
        if len(self._cache) > 512:
            first_key = next(iter(self._cache))
            self._cache.pop(first_key, None)
        return value

    def lookup_cached_ground_level(self, x: float, y: float) -> tuple[bool, float | None]:
        key = (round(float(x), 2), round(float(y), 2), self.coverage_id)
        if key not in self._cache:
            return False, None
        return True, self._cache[key]

    def _request_ground_level(self, x: float, y: float) -> float | None:
        last_error: Exception | None = None
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
                    response = self.session.get(self.BASE_URL, params=params, timeout=self.timeout)
                    response.raise_for_status()
                    content_type = response.headers.get("content-type", "")
                    if "image" not in content_type.lower():
                        snippet = response.text[:300].strip()
                        raise PdokAhnError(f"PDOK AHN gaf geen raster terug: {snippet}")
                    image = Image.open(BytesIO(response.content))
                    nearest = self._nearest_valid_value(image)
                    if nearest is not None:
                        return round(nearest, 3)
                    break
                except (requests.RequestException, OSError, ValueError, PdokAhnError) as exc:
                    last_error = exc
                    if attempt >= self.retries:
                        break
                    time.sleep(0.25 * attempt)
        if last_error is not None:
            return None
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
