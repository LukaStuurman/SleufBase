from __future__ import annotations

from collections import OrderedDict
import threading

import numpy as np

from . import renderer as renderer_module
from .renderer import MapRenderer


PATCH_VERSION = 3
_FLAG_INDEX_CACHE_LIMIT = 32
_flag_index_cache: OrderedDict[
    tuple[int, int], tuple[tuple[str, ...], dict[str, tuple[int, ...]]]
] = OrderedDict()
_flag_index_cache_lock = threading.RLock()


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
            if index < feature_count:
                flags[index] = 1
    return flags


def _bounds_contains_with_tolerance(feature, x: float, y: float, tolerance: float) -> bool:
    """Equivalent to feature.bounds.padded(tolerance).contains() without allocation."""
    bounds = feature.bounds
    return (
        float(x) >= float(bounds.min_x) - tolerance
        and float(x) <= float(bounds.max_x) + tolerance
        and float(y) >= float(bounds.min_y) - tolerance
        and float(y) <= float(bounds.max_y) + tolerance
    )


def _match_sort_key(distance: float, feature):
    return (
        float(distance),
        feature.display_name.lower(),
        feature.source_path.name.lower(),
        feature.feature_id,
    )


def _pick_features_fast(x: float, y: float, overlays, tolerance_meters: float):
    """Preserve pick order while avoiding one Bounds allocation per feature."""
    px = float(x)
    py = float(y)
    tolerance = float(tolerance_meters)
    matches = []
    for overlay in overlays:
        if not overlay.visible:
            continue
        for feature in overlay.features:
            if not _bounds_contains_with_tolerance(feature, px, py, tolerance):
                continue
            distance = feature.distance_to(px, py)
            if distance <= tolerance:
                matches.append((*_match_sort_key(distance, feature), feature))
    matches.sort(key=lambda item: item[:4])
    return [item[4] for item in matches]


def _pick_feature_fast(x: float, y: float, overlays, tolerance_meters: float):
    """Return the same best feature without allocating/sorting every matching item."""
    px = float(x)
    py = float(y)
    tolerance = float(tolerance_meters)
    best_key = None
    best_feature = None
    for overlay in overlays:
        if not overlay.visible:
            continue
        for feature in overlay.features:
            if not _bounds_contains_with_tolerance(feature, px, py, tolerance):
                continue
            distance = feature.distance_to(px, py)
            if distance > tolerance:
                continue
            key = _match_sort_key(distance, feature)
            if best_key is None or key < best_key:
                best_key = key
                best_feature = feature
    return best_feature


def install_render_cache_performance_patch() -> None:
    current = int(getattr(MapRenderer, "_sleufbase_render_cache_performance_patch_version", 0) or 0)
    if current >= PATCH_VERSION:
        return

    if not hasattr(MapRenderer, "_sleufbase_original_feature_flags"):
        MapRenderer._sleufbase_original_feature_flags = staticmethod(MapRenderer._feature_flags)
    if not hasattr(renderer_module, "_sleufbase_original_pick_features"):
        renderer_module._sleufbase_original_pick_features = renderer_module.pick_features
    if not hasattr(renderer_module, "_sleufbase_original_pick_feature"):
        renderer_module._sleufbase_original_pick_feature = renderer_module.pick_feature

    # CableFeature.distance_to is now optimized directly in models.py. Keeping a
    # second runtime monkey-patch here only duplicated code and obscured which
    # implementation was actually active.
    MapRenderer._feature_flags = staticmethod(_feature_flags_fast)
    renderer_module.pick_features = _pick_features_fast
    renderer_module.pick_feature = _pick_feature_fast

    MapRenderer._sleufbase_render_cache_performance_patch_version = PATCH_VERSION
