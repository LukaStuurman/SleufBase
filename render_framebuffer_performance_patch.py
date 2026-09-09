from __future__ import annotations

from typing import Any

from PIL import Image, ImageDraw

from . import native_accel
from .models import ViewportTransform
from .renderer import (
    MapRenderer,
    _NATIVE_DXF_MIN_FEATURES,
    _NATIVE_DXF_MIN_POINTS,
    _NATIVE_TIFF_MIN_DEST_PIXELS,
    _NATIVE_TIFF_MIN_LAYERS,
)
from .render_hotpath_patch import _rgba_array


PATCH_VERSION = 1
_ORIGINAL_RENDER = None


def _prepared_native_tiff_jobs(renderer: MapRenderer, transform: ViewportTransform, tiff_layers):
    jobs = []
    total_dest_pixels = 0
    for layer in tiff_layers:
        if layer.bounds.intersection(transform.bounds) is None:
            continue
        if not layer.transform.is_axis_aligned():
            return None
        job = renderer._axis_aligned_tiff_paint_job(transform, layer)
        if job is None:
            continue
        source_rect, dest_rect = job
        total_dest_pixels += max(0, dest_rect[2] - dest_rect[0]) * max(
            0, dest_rect[3] - dest_rect[1]
        )
        source_rgba = renderer._native_tiff_rgba_cache(layer)
        if source_rgba is None:
            return None
        jobs.append((layer, source_rect, dest_rect, source_rgba))

    if not jobs:
        return None
    if len(jobs) < _NATIVE_TIFF_MIN_LAYERS and total_dest_pixels < _NATIVE_TIFF_MIN_DEST_PIXELS:
        return None
    return jobs


def _prepared_native_dxf_jobs(renderer: MapRenderer, dxf_overlays):
    visible = [overlay for overlay in dxf_overlays if overlay.visible and overlay.features]
    if not visible:
        return None
    total_features = sum(len(overlay.features) for overlay in visible)
    if total_features < _NATIVE_DXF_MIN_FEATURES:
        total_points = sum(len(feature.points) for overlay in visible for feature in overlay.features)
        if total_points < _NATIVE_DXF_MIN_POINTS:
            return None

    jobs = []
    for overlay in visible:
        cache = renderer._native_dxf_render_cache(overlay)
        if cache is None:
            return None
        jobs.append(cache)
    return jobs


def _render_combined_native(
    renderer: MapRenderer,
    view_bounds,
    size,
    tiff_layers,
    dxf_overlays,
    background,
    selected_feature_ids,
    highlight_feature_ids,
    map_comments,
    map_markers,
):
    if not native_accel.is_available() or not tiff_layers or not dxf_overlays:
        return None
    width, height = size
    if width <= 0 or height <= 0:
        return None

    transform = ViewportTransform(view_bounds, width, height)
    tiff_jobs = _prepared_native_tiff_jobs(renderer, transform, tiff_layers)
    if tiff_jobs is None:
        return None
    dxf_jobs = _prepared_native_dxf_jobs(renderer, dxf_overlays)
    if dxf_jobs is None:
        return None

    canvas = (
        background.copy().convert("RGBA")
        if background is not None
        else Image.new("RGBA", size, (245, 245, 245, 255))
    )
    try:
        rgba = _rgba_array(canvas)
        for layer, source_rect, dest_rect, source_rgba in tiff_jobs:
            painted = native_accel.paint_axis_aligned_tiff(
                rgba,
                source_rgba,
                source_rect,
                dest_rect,
                layer.opacity,
            )
            if painted is None:
                return None

        selected_ids = set(selected_feature_ids or [])
        highlight_ids = set(highlight_feature_ids or [])
        view_tuple = (
            float(transform.bounds.min_x),
            float(transform.bounds.min_y),
            float(transform.bounds.max_x),
            float(transform.bounds.max_y),
        )
        for cache in dxf_jobs:
            feature_count = len(cache.feature_ids)
            selected_flags = renderer._feature_flags(cache.feature_ids, selected_ids, feature_count)
            highlighted_flags = renderer._feature_flags(
                cache.feature_ids, highlight_ids, feature_count
            )
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

        result = Image.fromarray(rgba, mode="RGBA")
        if map_comments or map_markers:
            draw = ImageDraw.Draw(result, "RGBA")
            if map_comments:
                renderer._draw_comments(draw, transform, map_comments)
            if map_markers:
                renderer._draw_markers(draw, transform, map_markers)
        return result
    finally:
        canvas.close()


def install_render_framebuffer_performance_patch() -> None:
    global _ORIGINAL_RENDER

    current = int(
        getattr(MapRenderer, "_sleufbase_render_framebuffer_performance_version", 0) or 0
    )
    if current >= PATCH_VERSION:
        return

    _ORIGINAL_RENDER = MapRenderer.render

    def render_shared_native_framebuffer(
        self: MapRenderer,
        view_bounds,
        size,
        tiff_layers,
        dxf_overlays,
        background=None,
        selected_feature_ids=None,
        highlight_feature_ids=None,
        map_comments=None,
        map_markers=None,
    ):
        combined = _render_combined_native(
            self,
            view_bounds,
            size,
            tiff_layers,
            dxf_overlays,
            background,
            selected_feature_ids,
            highlight_feature_ids,
            map_comments,
            map_markers,
        )
        if combined is not None:
            return combined
        return _ORIGINAL_RENDER(
            self,
            view_bounds,
            size,
            tiff_layers,
            dxf_overlays,
            background=background,
            selected_feature_ids=selected_feature_ids,
            highlight_feature_ids=highlight_feature_ids,
            map_comments=map_comments,
            map_markers=map_markers,
        )

    MapRenderer.render = render_shared_native_framebuffer
    MapRenderer._sleufbase_render_framebuffer_performance_version = PATCH_VERSION
    MapRenderer.SLEUFBASE_SHARED_NATIVE_FRAMEBUFFER = True
