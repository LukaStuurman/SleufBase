from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Iterable

import numpy as np
from PIL import Image


@dataclass(frozen=True)
class Bounds:
    min_x: float
    min_y: float
    max_x: float
    max_y: float

    @property
    def width(self) -> float:
        return self.max_x - self.min_x

    @property
    def height(self) -> float:
        return self.max_y - self.min_y

    @property
    def center_x(self) -> float:
        return (self.min_x + self.max_x) / 2.0

    @property
    def center_y(self) -> float:
        return (self.min_y + self.max_y) / 2.0

    def padded(self, padding: float) -> "Bounds":
        return Bounds(
            self.min_x - padding,
            self.min_y - padding,
            self.max_x + padding,
            self.max_y + padding,
        )

    def intersects(self, other: "Bounds") -> bool:
        return not (
            self.max_x < other.min_x
            or self.min_x > other.max_x
            or self.max_y < other.min_y
            or self.min_y > other.max_y
        )

    def intersection(self, other: "Bounds") -> "Bounds | None":
        if not self.intersects(other):
            return None
        return Bounds(
            max(self.min_x, other.min_x),
            max(self.min_y, other.min_y),
            min(self.max_x, other.max_x),
            min(self.max_y, other.max_y),
        )

    def union(self, other: "Bounds") -> "Bounds":
        return Bounds(
            min(self.min_x, other.min_x),
            min(self.min_y, other.min_y),
            max(self.max_x, other.max_x),
            max(self.max_y, other.max_y),
        )

    def contains(self, x: float, y: float) -> bool:
        return self.min_x <= x <= self.max_x and self.min_y <= y <= self.max_y

    def expand_to_aspect_ratio(self, aspect_ratio: float) -> "Bounds":
        if self.width <= 0 or self.height <= 0 or aspect_ratio <= 0:
            return self
        current = self.width / self.height
        if abs(current - aspect_ratio) < 1e-9:
            return self
        if current < aspect_ratio:
            target_width = self.height * aspect_ratio
            padding = (target_width - self.width) / 2.0
            return Bounds(self.min_x - padding, self.min_y, self.max_x + padding, self.max_y)
        target_height = self.width / aspect_ratio
        padding = (target_height - self.height) / 2.0
        return Bounds(self.min_x, self.min_y - padding, self.max_x, self.max_y + padding)


@dataclass(frozen=True)
class GeoTransform:
    a: float
    b: float
    c: float
    d: float
    e: float
    f: float

    def pixel_to_world(self, col: float, row: float) -> tuple[float, float]:
        x = self.a * col + self.b * row + self.c
        y = self.d * col + self.e * row + self.f
        return x, y

    def world_to_pixel(self, x: float, y: float) -> tuple[float, float]:
        """Invert the affine 2x2 part directly; this is a hot render path."""
        determinant = (self.a * self.e) - (self.b * self.d)
        if abs(determinant) <= 1e-15:
            raise ValueError("GeoTIFF-transformatie is singulier en kan niet worden geïnverteerd.")
        dx = float(x) - self.c
        dy = float(y) - self.f
        inverse = 1.0 / determinant
        col = ((self.e * dx) - (self.b * dy)) * inverse
        row = ((-self.d * dx) + (self.a * dy)) * inverse
        return float(col), float(row)

    def to_matrix(self) -> np.ndarray:
        return np.array(
            [
                [self.a, self.b, self.c],
                [self.d, self.e, self.f],
                [0.0, 0.0, 1.0],
            ],
            dtype=float,
        )

    def is_axis_aligned(self, tolerance: float = 1e-9) -> bool:
        return abs(self.b) <= tolerance and abs(self.d) <= tolerance


@dataclass(frozen=True)
class ViewportTransform:
    bounds: Bounds
    width_px: int
    height_px: int

    def _validate(self) -> None:
        if self.width_px <= 0 or self.height_px <= 0:
            raise ValueError("Viewport-afmetingen moeten groter dan nul zijn.")
        if self.bounds.width <= 0 or self.bounds.height <= 0:
            raise ValueError("Viewport-grenzen moeten een positieve breedte en hoogte hebben.")

    @property
    def meters_per_pixel(self) -> float:
        self._validate()
        return self.bounds.width / self.width_px

    def world_to_screen(self, x: float, y: float) -> tuple[float, float]:
        self._validate()
        screen_x = ((x - self.bounds.min_x) / self.bounds.width) * self.width_px
        screen_y = self.height_px - ((y - self.bounds.min_y) / self.bounds.height) * self.height_px
        return screen_x, screen_y

    def world_points_to_screen(self, points: Iterable[tuple[float, float]]) -> list[tuple[float, float]]:
        """Convert many world points while validating and computing scale only once."""
        self._validate()
        scale_x = self.width_px / self.bounds.width
        scale_y = self.height_px / self.bounds.height
        min_x = self.bounds.min_x
        min_y = self.bounds.min_y
        height_px = self.height_px
        return [
            ((float(x) - min_x) * scale_x, height_px - ((float(y) - min_y) * scale_y))
            for x, y in points
        ]

    def screen_to_world(self, screen_x: float, screen_y: float) -> tuple[float, float]:
        self._validate()
        x = self.bounds.min_x + (screen_x / self.width_px) * self.bounds.width
        y = self.bounds.min_y + ((self.height_px - screen_y) / self.height_px) * self.bounds.height
        return x, y

    def world_to_screen_matrix(self) -> np.ndarray:
        self._validate()
        scale_x = self.width_px / self.bounds.width
        scale_y = self.height_px / self.bounds.height
        return np.array(
            [
                [scale_x, 0.0, -self.bounds.min_x * scale_x],
                [0.0, -scale_y, self.bounds.max_y * scale_y],
                [0.0, 0.0, 1.0],
            ],
            dtype=float,
        )


def _segment_distance(point: tuple[float, float], start: tuple[float, float], end: tuple[float, float]) -> float:
    px, py = point
    x1, y1 = start
    x2, y2 = end
    dx = x2 - x1
    dy = y2 - y1
    if dx == 0 and dy == 0:
        return ((px - x1) ** 2 + (py - y1) ** 2) ** 0.5
    projection = ((px - x1) * dx + (py - y1) * dy) / (dx * dx + dy * dy)
    projection = max(0.0, min(1.0, projection))
    nearest_x = x1 + projection * dx
    nearest_y = y1 + projection * dy
    return ((px - nearest_x) ** 2 + (py - nearest_y) ** 2) ** 0.5


@dataclass
class CableFeature:
    feature_id: str
    source_path: Path
    points: list[tuple[float, float]]
    bounds: Bounds
    color: tuple[int, int, int]
    metadata: dict[str, str] = field(default_factory=dict)

    def distance_to(self, x: float, y: float) -> float:
        if len(self.points) < 2:
            return float("inf")
        return min(
            _segment_distance((x, y), self.points[index], self.points[index + 1])
            for index in range(len(self.points) - 1)
        )

    @property
    def display_name(self) -> str:
        layer_name = self.metadata.get("Laag")
        if layer_name:
            return layer_name
        return self.metadata.get("Type", self.feature_id)

    def native_render_signature(self) -> tuple[object, ...]:
        """Cheap identity/state signature for native render-cache validation."""
        points = self.points
        first_point = points[0] if points else None
        middle_point = points[len(points) // 2] if len(points) > 2 else None
        last_point = points[-1] if points else None
        return (
            self.feature_id,
            id(self),
            id(points),
            len(points),
            first_point,
            middle_point,
            last_point,
            self.bounds.min_x,
            self.bounds.min_y,
            self.bounds.max_x,
            self.bounds.max_y,
            self.color,
        )


@dataclass
class DxfOverlay:
    path: Path
    features: list[CableFeature]
    visible: bool = True
    native_render_cache: Any | None = field(default=None, repr=False, compare=False)

    @property
    def bounds(self) -> Bounds | None:
        if not self.features:
            return None
        combined = self.features[0].bounds
        for feature in self.features[1:]:
            combined = combined.union(feature.bounds)
        return combined

    def native_render_signature(self) -> tuple[tuple[object, ...], ...]:
        """O(feature-count) signature; avoids re-hashing every DXF point per frame."""
        return tuple(feature.native_render_signature() for feature in self.features)

    def invalidate_native_render_cache(self) -> None:
        """Invalidate native render data after replacing or editing overlay geometry."""
        self.native_render_cache = None


@dataclass
class MapComment:
    x: float
    y: float
    text: str


@dataclass(frozen=True)
class MapMarker:
    marker_id: str
    x: float
    y: float
    fill_color: tuple[int, int, int] = (255, 255, 255)
    outline_color: tuple[int, int, int] = (0, 0, 0)
    radius_px: int = 8


@dataclass(frozen=True)
class ProfileReferenceAnnotation:
    x: float
    y: float


@dataclass
class GeoTiffLayer:
    path: Path
    image: Image.Image
    transform: GeoTransform
    bounds: Bounds
    epsg: int | None
    opacity: float = 0.8
    metadata: dict[str, Any] = field(default_factory=dict)
    native_rgba_cache: Any | None = field(default=None, repr=False, compare=False)

    @property
    def name(self) -> str:
        return self.path.name

    def invalidate_native_rgba_cache(self) -> None:
        """Invalidate native RGBA data after mutating or replacing the image."""
        self.native_rgba_cache = None
