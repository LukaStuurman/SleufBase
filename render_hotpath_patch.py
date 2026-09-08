from __future__ import annotations

from typing import Any

import numpy as np
from PIL import Image

from . import native_accel
from .renderer import (
    MapRenderer,
    _NATIVE_DXF_MIN_FEATURES,
    _NATIVE_DXF_MIN_POINTS,
    _NATIVE_TIFF_MIN_DEST_PIXELS,
    _NATIVE_TIFF_MIN_LAYERS,
)


PATCH_VERSION = 1


def _rgba_array(canvas: Image.Image) -> np.ndarray:
    """Return a writable RGBA copy without converting an already-RGBA canvas."""
    if canvas.mode == "RGBA":
        return np.array(canvas, dtype=np.uint8, copy=True)
    converted = canvas.convert("RGBA")
    try:
        return np.array(converted, dtype=np.uint8, copy=True)
    finally:
        converted.close()


def _draw_dxf_overlays_python_fast(
    self: MapRenderer,
    draw: Any,
    transform: Any,
    dxf_overlays: list[Any],
    selected_ids: set[str],
    highlight_ids: set[str],
) -> None:
    """Python fallback renderer with frame-invariant math hoisted out of loops."""
    transform._validate()
    bounds = transform.bounds
    scale_x = transform.width_px / bounds.width
    scale_y = transform.height_px / bounds.height
    min_x = bounds.min_x
    min_y = bounds.min_y
    height_px = transform.height_px
    meters_per_pixel = bounds.width / transform.width_px
    line_width = 4 if meters_per_pixel < 0.05 else 3 if meters_per_pixel < 0.2 else 2

    for overlay in dxf_overlays:
        if not overlay.visible:
            continue
        for feature in overlay.features:
            feature_bounds = feature.bounds
            if (
                feature_bounds.max_x < bounds.min_x
                or feature_bounds.min_x > bounds.max_x
                or feature_bounds.max_y < bounds.min_y
                or feature_bounds.min_y > bounds.max_y
            ):
                continue
            points = feature.points
            if len(points) < 2:
                continue
            screen_points = [
                ((float(x) - min_x) * scale_x, height_px - ((float(y) - min_y) * scale_y))
                for x, y in points
            ]
            is_highlighted = feature.feature_id in highlight_ids
            is_selected = feature.feature_id in selected_ids
            if is_highlighted:
                draw.line(screen_points, fill=(0, 160, 255, 105), width=10)
            if is_selected:
                draw.line(screen_points, fill=(255, 215, 0, 215), width=7)
            draw.line(screen_points, fill=(*feature.color, 230), width=line_width)
            if len(screen_points) == 2 and screen_points[0] == screen_points[1]:
                x, y = screen_points[0]
                radius = max(4, line_width + 2)
                if is_highlighted:
                    draw.ellipse(
                        (x - radius - 4, y - radius - 4, x + radius + 4, y + radius + 4),
                        fill=(0, 160, 255, 105),
                    )
                if is_selected:
                    draw.ellipse(
                        (x - radius - 3, y - radius - 3, x + radius + 3, y + radius + 3),
                        fill=(255, 215, 0, 215),
                    )
                draw.ellipse(
                    (x - radius, y - radius, x + radius, y + radius),
                    fill=(*feature.color, 230),
                )


def _render_dxf_overlays_native_fast(
    self: MapRenderer,
    canvas: Image.Image,
    transform: Any,
    dxf_overlays: list[Any],
    selected_ids: set[str],
    highlight_ids: set[str],
) -> Image.Image | None:
    visible_overlays = [overlay for overlay in dxf_overlays if overlay.visible and overlay.features]
    if not visible_overlays:
        return canvas
    if not native_accel.is_available():
        return None

    total_features = sum(len(overlay.features) for overlay in visible_overlays)
    # A feature count above the native threshold already decides this branch.
    # Avoid a second full nested pass over every feature just to count points.
    if total_features < _NATIVE_DXF_MIN_FEATURES:
        total_points = sum(
            len(feature.points)
            for overlay in visible_overlays
            for feature in overlay.features
        )
        if total_points < _NATIVE_DXF_MIN_POINTS:
            return None

    rgba = _rgba_array(canvas)
    view_tuple = (
        float(transform.bounds.min_x),
        float(transform.bounds.min_y),
        float(transform.bounds.max_x),
        float(transform.bounds.max_y),
    )
    for overlay in visible_overlays:
        cache = self._native_dxf_render_cache(overlay)
        if cache is None:
            return None
        feature_count = len(cache.feature_ids)
        selected_flags = self._feature_flags(cache.feature_ids, selected_ids, feature_count)
        highlighted_flags = self._feature_flags(cache.feature_ids, highlight_ids, feature_count)
        rendered = native_accel.render_dxf_overlay(
            rgba,
            cache.points_xy,
            cache.feature_offsets,
            cache.feature_bounds,
            cache.feature_colors,
            selected_flags,
            highlighted_flags,
            view_tuple,
            transform.meters_per_pixel,
        )
        if rendered is None:
            return None
    return Image.fromarray(rgba, mode="RGBA")


def _paint_tiff_layers_native_fast(
    self: MapRenderer,
    canvas: Image.Image,
    transform: Any,
    tiff_layers: list[Any],
) -> Image.Image | None:
    if not tiff_layers or not native_accel.is_available():
        return None
    paint_jobs: list[tuple[Any, tuple[float, float, float, float], tuple[int, int, int, int]]] = []
    total_dest_pixels = 0
    for layer in tiff_layers:
        if layer.bounds.intersection(transform.bounds) is None:
            continue
        if not layer.transform.is_axis_aligned():
            return None
        job = self._axis_aligned_tiff_paint_job(transform, layer)
        if job is None:
            continue
        _source_rect, dest_rect = job
        dest_width = max(0, dest_rect[2] - dest_rect[0])
        dest_height = max(0, dest_rect[3] - dest_rect[1])
        total_dest_pixels += dest_width * dest_height
        paint_jobs.append((layer, job[0], job[1]))

    if not paint_jobs:
        return canvas
    if len(paint_jobs) < _NATIVE_TIFF_MIN_LAYERS and total_dest_pixels < _NATIVE_TIFF_MIN_DEST_PIXELS:
        return None

    rgba = _rgba_array(canvas)
    for layer, source_rect, dest_rect in paint_jobs:
        source_rgba = self._native_tiff_rgba_cache(layer)
        if source_rgba is None:
            return None
        painted = native_accel.paint_axis_aligned_tiff(
            rgba,
            source_rgba,
            source_rect,
            dest_rect,
            layer.opacity,
        )
        if painted is None:
            return None
    return Image.fromarray(rgba, mode="RGBA")


def install_render_hotpath_patch() -> None:
    current = int(getattr(MapRenderer, "_sleufbase_render_hotpath_patch_version", 0) or 0)
    if current >= PATCH_VERSION:
        return

    if not hasattr(MapRenderer, "_sleufbase_original_draw_dxf_overlays_python"):
        MapRenderer._sleufbase_original_draw_dxf_overlays_python = MapRenderer._draw_dxf_overlays_python
    if not hasattr(MapRenderer, "_sleufbase_original_render_dxf_overlays_native"):
        MapRenderer._sleufbase_original_render_dxf_overlays_native = MapRenderer._render_dxf_overlays_native
    if not hasattr(MapRenderer, "_sleufbase_original_paint_tiff_layers_native"):
        MapRenderer._sleufbase_original_paint_tiff_layers_native = MapRenderer._paint_tiff_layers_native

    MapRenderer._draw_dxf_overlays_python = _draw_dxf_overlays_python_fast
    MapRenderer._render_dxf_overlays_native = _render_dxf_overlays_native_fast
    MapRenderer._paint_tiff_layers_native = _paint_tiff_layers_native_fast
    MapRenderer._sleufbase_render_hotpath_patch_version = PATCH_VERSION
