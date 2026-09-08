from __future__ import annotations

from collections import OrderedDict
import math
import threading
from typing import Any

import numpy as np

from .models import CableFeature, DxfOverlay
from .renderer import MapRenderer


PATCH_VERSION = 1
_FLAG_INDEX_CACHE_LIMIT = 32
_flag_index_cache: OrderedDict[
    tuple[int, int], tuple[tuple[str, ...], dict[str, tuple[int, ...]]]
] = OrderedDict()
_flag_index_cache_lock = threading.RLock()


def _overlay_signature_state(overlay: DxfOverlay) -> tuple[object, ...]:
    """O(1) state check for the effectively immutable DXF feature list."""
    features = overlay.features
    feature_count = len(features)
    if feature_count == 0:
        return (id(features), 0)
    return (
        id(features),
        feature_count,
        id(features[0]),
        id(features[feature_count // 2]),
        id(features[-1]),
    )


def _feature_index(feature_ids: tuple[str, ...]) -> dict[str, tuple[int, ...]]:
    """Build an ID -> indices lookup once per native render-cache feature tuple."""
    key = (id(feature_ids), len(feature_ids))
    with _flag_index_cache_lock:
        cached = _flag_index_cache.get(key)
        if cached is not None and cached[0] is feature_ids:
            _flag_index_cache.move_to_end(key)
            return cached[1]

    mutable_index: dict[str, list[int]] = {}
    for index, feature_id in enumerate(feature_ids):
        mutable_index.setdefault(feature_id, []).append(index)
    index_map = {feature_id: tuple(indices) for feature_id, indices in mutable_index.items()}

    with _flag_index_cache_lock:
        _flag_index_cache[key] = (feature_ids, index_map)
        _flag_index_cache.move_to_end(key)
        while len(_flag_index_cache) > _FLAG_INDEX_CACHE_LIMIT:
            _flag_index_cache.popitem(last=False)
    return index_map


def _feature_flags_fast(
    feature_ids: tuple[str, ...],
    enabled_ids: set[str],
    feature_count: int,
) -> np.ndarray:
    """Create native flags in O(enabled IDs) after the first lookup build."""
    flags = np.zeros(feature_count, dtype=np.uint8)
    if not enabled_ids or feature_count <= 0:
        return flags

    index_map = _feature_index(feature_ids)
    for feature_id in enabled_ids:
        for index in index_map.get(feature_id, ()):
            flags[index] = 1
    return flags


def _distance_to_fast(self: CableFeature, x: float, y: float) -> float:
    """Avoid generator/function-call overhead in frequent map hit testing."""
    points = self.points
    if len(points) < 2:
        return float("inf")

    px = float(x)
    py = float(y)
    best = float("inf")
    for index in range(len(points) - 1):
        x1, y1 = points[index]
        x2, y2 = points[index + 1]
        dx = x2 - x1
        dy = y2 - y1
        if dx == 0 and dy == 0:
            distance = math.hypot(px - x1, py - y1)
        else:
            projection = ((px - x1) * dx + (py - y1) * dy) / ((dx * dx) + (dy * dy))
            if projection <= 0.0:
                nearest_x = x1
                nearest_y = y1
            elif projection >= 1.0:
                nearest_x = x2
                nearest_y = y2
            else:
                nearest_x = x1 + projection * dx
                nearest_y = y1 + projection * dy
            distance = math.hypot(px - nearest_x, py - nearest_y)
        if distance < best:
            best = distance
            if best == 0.0:
                return 0.0
    return best


def install_render_cache_performance_patch() -> None:
    current = int(getattr(MapRenderer, "_sleufbase_render_cache_performance_patch_version", 0) or 0)
    if current >= PATCH_VERSION:
        return

    if not hasattr(DxfOverlay, "_sleufbase_original_native_render_signature"):
        DxfOverlay._sleufbase_original_native_render_signature = DxfOverlay.native_render_signature
    if not hasattr(DxfOverlay, "_sleufbase_original_invalidate_native_render_cache"):
        DxfOverlay._sleufbase_original_invalidate_native_render_cache = DxfOverlay.invalidate_native_render_cache
    if not hasattr(CableFeature, "_sleufbase_original_distance_to"):
        CableFeature._sleufbase_original_distance_to = CableFeature.distance_to
    if not hasattr(MapRenderer, "_sleufbase_original_feature_flags"):
        MapRenderer._sleufbase_original_feature_flags = MapRenderer._feature_flags

    original_signature = DxfOverlay._sleufbase_original_native_render_signature
    original_invalidate = DxfOverlay._sleufbase_original_invalidate_native_render_cache

    def native_render_signature_cached(self: DxfOverlay) -> tuple[tuple[object, ...], ...]:
        state = _overlay_signature_state(self)
        cached_state = getattr(self, "_sleufbase_native_signature_state", None)
        cached_signature = getattr(self, "_sleufbase_native_signature_cache", None)
        if cached_signature is not None and cached_state == state:
            return cached_signature
        signature = original_signature(self)
        self._sleufbase_native_signature_state = state
        self._sleufbase_native_signature_cache = signature
        return signature

    def invalidate_native_render_cache_fast(self: DxfOverlay) -> None:
        original_invalidate(self)
        self.__dict__.pop("_sleufbase_native_signature_state", None)
        self.__dict__.pop("_sleufbase_native_signature_cache", None)

    DxfOverlay.native_render_signature = native_render_signature_cached
    DxfOverlay.invalidate_native_render_cache = invalidate_native_render_cache_fast
    CableFeature.distance_to = _distance_to_fast
    MapRenderer._feature_flags = staticmethod(_feature_flags_fast)

    DxfOverlay._sleufbase_render_cache_performance_patch_version = PATCH_VERSION
    CableFeature._sleufbase_render_cache_performance_patch_version = PATCH_VERSION
    MapRenderer._sleufbase_render_cache_performance_patch_version = PATCH_VERSION
