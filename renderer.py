from __future__ import annotations

from collections.abc import Iterable
from dataclasses import dataclass

import numpy as np
from PIL import Image, ImageDraw, ImageFont

from . import native_accel
from .models import Bounds, CableFeature, DxfOverlay, GeoTiffLayer, MapComment, MapMarker, ViewportTransform


_NATIVE_DXF_MIN_FEATURES = 1000
_NATIVE_DXF_MIN_POINTS = 6000
_NATIVE_TIFF_MIN_LAYERS = 2
_NATIVE_TIFF_MIN_DEST_PIXELS = 900_000


@dataclass(frozen=True)
class _NativeDxfRenderCache:
    key: tuple[tuple[object, ...], ...]
    points_xy: np.ndarray
    feature_offsets: np.ndarray
    feature_bounds: np.ndarray
    feature_colors: np.ndarray
    feature_ids: tuple[str, ...]


@dataclass(frozen=True)
class _NativeTiffImageCache:
    key: tuple[int, int, int, str]
    rgba: np.ndarray


class MapRenderer:
    def render(
        self,
        view_bounds: Bounds,
        size: tuple[int, int],
        tiff_layers: list[GeoTiffLayer],
        dxf_overlays: list[DxfOverlay],
        background: Image.Image | None = None,
        selected_feature_ids: Iterable[str] | None = None,
        highlight_feature_ids: Iterable[str] | None = None,
        map_comments: list[MapComment] | None = None,
        map_markers: list[MapMarker] | None = None,
    ) -> Image.Image:
        width, height = size
        if width <= 0 or height <= 0:
            return Image.new("RGBA", (1, 1), (255, 255, 255, 255))
        canvas = background.copy().convert("RGBA") if background is not None else Image.new("RGBA", size, (245, 245, 245, 255))
        transform = ViewportTransform(view_bounds, width, height)
        selected_ids = set(selected_feature_ids or [])
        highlight_ids = set(highlight_feature_ids or [])
        native_tiff_canvas = self._paint_tiff_layers_native(canvas, transform, tiff_layers)
        if native_tiff_canvas is not None:
            canvas = native_tiff_canvas
        else:
            for layer in tiff_layers:
                self._paint_tiff(canvas, transform, layer)

        native_canvas = self._render_dxf_overlays_native(canvas, transform, dxf_overlays, selected_ids, highlight_ids)
        if native_canvas is not None:
            canvas = native_canvas
        else:
            draw = ImageDraw.Draw(canvas, "RGBA")
            self._draw_dxf_overlays_python(draw, transform, dxf_overlays, selected_ids, highlight_ids)

        draw = ImageDraw.Draw(canvas, "RGBA")
        if map_comments:
            self._draw_comments(draw, transform, map_comments)
        if map_markers:
            self._draw_markers(draw, transform, map_markers)
        return canvas

    def _draw_dxf_overlays_python(
        self,
        draw: ImageDraw.ImageDraw,
        transform: ViewportTransform,
        dxf_overlays: list[DxfOverlay],
        selected_ids: set[str],
        highlight_ids: set[str],
    ) -> None:
        for overlay in dxf_overlays:
            if not overlay.visible:
                continue
            for feature in overlay.features:
                if not feature.bounds.intersects(transform.bounds):
                    continue
                self._draw_feature(
                    draw,
                    transform,
                    feature,
                    is_selected=feature.feature_id in selected_ids,
                    is_highlighted=feature.feature_id in highlight_ids,
                )

    def _render_dxf_overlays_native(
        self,
        canvas: Image.Image,
        transform: ViewportTransform,
        dxf_overlays: list[DxfOverlay],
        selected_ids: set[str],
        highlight_ids: set[str],
    ) -> Image.Image | None:
        visible_overlays = [overlay for overlay in dxf_overlays if overlay.visible and overlay.features]
        if not visible_overlays:
            return canvas
        if not native_accel.is_available():
            return None
        total_features = sum(len(overlay.features) for overlay in visible_overlays)
        total_points = sum(len(feature.points) for overlay in visible_overlays for feature in overlay.features)
        if total_features < _NATIVE_DXF_MIN_FEATURES and total_points < _NATIVE_DXF_MIN_POINTS:
            return None

        rgba = np.array(canvas.convert("RGBA"), dtype=np.uint8, copy=True)
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

    def _native_dxf_render_cache(self, overlay: DxfOverlay) -> _NativeDxfRenderCache | None:
        key = overlay.native_render_signature()
        cache = overlay.native_render_cache
        if isinstance(cache, _NativeDxfRenderCache) and cache.key == key:
            return cache

        feature_count = len(overlay.features)
        point_count = sum(len(feature.points) for feature in overlay.features)
        if feature_count <= 0 or point_count <= 0 or point_count > np.iinfo(np.int32).max:
            return None

        points_xy = np.empty((point_count, 2), dtype=np.float64)
        feature_offsets = np.empty(feature_count + 1, dtype=np.int32)
        feature_bounds = np.empty((feature_count, 4), dtype=np.float64)
        feature_colors = np.empty((feature_count, 3), dtype=np.uint8)
        feature_ids: list[str] = []

        cursor = 0
        for index, feature in enumerate(overlay.features):
            points = feature.points
            feature_offsets[index] = cursor
            if len(points) < 2:
                feature_offsets[index + 1] = cursor
                continue
            next_cursor = cursor + len(points)
            points_xy[cursor:next_cursor] = np.asarray(points, dtype=np.float64)
            feature_bounds[index] = (
                feature.bounds.min_x,
                feature.bounds.min_y,
                feature.bounds.max_x,
                feature.bounds.max_y,
            )
            feature_colors[index] = feature.color
            feature_ids.append(feature.feature_id)
            cursor = next_cursor
            feature_offsets[index + 1] = cursor

        if cursor != point_count or len(feature_ids) != feature_count:
            return None
        cache = _NativeDxfRenderCache(
            key=key,
            points_xy=np.ascontiguousarray(points_xy, dtype=np.float64),
            feature_offsets=np.ascontiguousarray(feature_offsets, dtype=np.int32),
            feature_bounds=np.ascontiguousarray(feature_bounds, dtype=np.float64),
            feature_colors=np.ascontiguousarray(feature_colors, dtype=np.uint8),
            feature_ids=tuple(feature_ids),
        )
        overlay.native_render_cache = cache
        return cache

    @staticmethod
    def _feature_flags(feature_ids: tuple[str, ...], enabled_ids: set[str], feature_count: int) -> np.ndarray:
        if not enabled_ids:
            return np.zeros(feature_count, dtype=np.uint8)
        return np.fromiter((1 if feature_id in enabled_ids else 0 for feature_id in feature_ids), dtype=np.uint8, count=feature_count)

    def _paint_tiff_layers_native(
        self,
        canvas: Image.Image,
        transform: ViewportTransform,
        tiff_layers: list[GeoTiffLayer],
    ) -> Image.Image | None:
        if not tiff_layers or not native_accel.is_available():
            return None
        paint_jobs: list[tuple[GeoTiffLayer, tuple[float, float, float, float], tuple[int, int, int, int]]] = []
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

        rgba = np.array(canvas.convert("RGBA"), dtype=np.uint8, copy=True)
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

    def _native_tiff_rgba_cache(self, layer: GeoTiffLayer) -> np.ndarray | None:
        image = layer.image
        key = (id(image), int(image.width), int(image.height), str(image.mode))
        cache = layer.native_rgba_cache
        if isinstance(cache, _NativeTiffImageCache) and cache.key == key:
            return cache.rgba
        converted: Image.Image | None = None
        try:
            source = image
            if image.mode != "RGBA":
                converted = image.convert("RGBA")
                source = converted
            rgba = np.ascontiguousarray(np.array(source, dtype=np.uint8, copy=True))
        except Exception:
            return None
        finally:
            if converted is not None:
                converted.close()
        cache = _NativeTiffImageCache(key=key, rgba=rgba)
        layer.native_rgba_cache = cache
        return cache.rgba

    def _axis_aligned_tiff_paint_job(
        self,
        transform: ViewportTransform,
        layer: GeoTiffLayer,
    ) -> tuple[tuple[float, float, float, float], tuple[int, int, int, int]] | None:
        visible_bounds = layer.bounds.intersection(transform.bounds)
        if visible_bounds is None:
            return None
        top_left = layer.transform.world_to_pixel(visible_bounds.min_x, visible_bounds.max_y)
        bottom_right = layer.transform.world_to_pixel(visible_bounds.max_x, visible_bounds.min_y)
        left = max(0, int(np.floor(min(top_left[0], bottom_right[0]))))
        upper = max(0, int(np.floor(min(top_left[1], bottom_right[1]))))
        right = min(layer.image.width, int(np.ceil(max(top_left[0], bottom_right[0]))))
        lower = min(layer.image.height, int(np.ceil(max(top_left[1], bottom_right[1]))))
        if right <= left or lower <= upper:
            return None

        screen_top_left = transform.world_to_screen(visible_bounds.min_x, visible_bounds.max_y)
        screen_bottom_right = transform.world_to_screen(visible_bounds.max_x, visible_bounds.min_y)
        dest_left = int(np.floor(min(screen_top_left[0], screen_bottom_right[0])))
        dest_top = int(np.floor(min(screen_top_left[1], screen_bottom_right[1])))
        dest_right = int(np.ceil(max(screen_top_left[0], screen_bottom_right[0])))
        dest_bottom = int(np.ceil(max(screen_top_left[1], screen_bottom_right[1])))
        if dest_right <= dest_left or dest_bottom <= dest_top:
            return None
        return (float(left), float(upper), float(right), float(lower)), (dest_left, dest_top, dest_right, dest_bottom)

    def _paint_tiff(self, canvas: Image.Image, transform: ViewportTransform, layer: GeoTiffLayer) -> None:
        if layer.bounds.width <= 0 or layer.bounds.height <= 0:
            return
        if layer.transform.is_axis_aligned():
            self._paint_axis_aligned_tiff(canvas, transform, layer)
            return
        self._paint_affine_tiff(canvas, transform, layer)

    def _paint_axis_aligned_tiff(self, canvas: Image.Image, transform: ViewportTransform, layer: GeoTiffLayer) -> None:
        visible_bounds = layer.bounds.intersection(transform.bounds)
        if visible_bounds is None:
            return
        top_left = layer.transform.world_to_pixel(visible_bounds.min_x, visible_bounds.max_y)
        bottom_right = layer.transform.world_to_pixel(visible_bounds.max_x, visible_bounds.min_y)
        left = max(0, int(np.floor(min(top_left[0], bottom_right[0]))))
        upper = max(0, int(np.floor(min(top_left[1], bottom_right[1]))))
        right = min(layer.image.width, int(np.ceil(max(top_left[0], bottom_right[0]))))
        lower = min(layer.image.height, int(np.ceil(max(top_left[1], bottom_right[1]))))
        if right <= left or lower <= upper:
            return
        crop = layer.image.crop((left, upper, right, lower)).convert("RGBA")
        resized: Image.Image | None = None
        try:
            if layer.opacity < 1.0:
                alpha = crop.getchannel("A").point(lambda value: int(value * layer.opacity))
                try:
                    crop.putalpha(alpha)
                finally:
                    alpha.close()
            screen_top_left = transform.world_to_screen(visible_bounds.min_x, visible_bounds.max_y)
            screen_bottom_right = transform.world_to_screen(visible_bounds.max_x, visible_bounds.min_y)
            dest_left = int(np.floor(min(screen_top_left[0], screen_bottom_right[0])))
            dest_top = int(np.floor(min(screen_top_left[1], screen_bottom_right[1])))
            dest_right = int(np.ceil(max(screen_top_left[0], screen_bottom_right[0])))
            dest_bottom = int(np.ceil(max(screen_top_left[1], screen_bottom_right[1])))
            if dest_right <= dest_left or dest_bottom <= dest_top:
                return
            resized = crop.resize((dest_right - dest_left, dest_bottom - dest_top), Image.Resampling.BILINEAR)
            canvas.alpha_composite(resized, (dest_left, dest_top))
        finally:
            if resized is not None:
                resized.close()
            crop.close()

    def _paint_affine_tiff(self, canvas: Image.Image, transform: ViewportTransform, layer: GeoTiffLayer) -> None:
        pixel_to_world = layer.transform.to_matrix()
        world_to_screen = transform.world_to_screen_matrix()
        pixel_to_screen = world_to_screen @ pixel_to_world
        screen_to_pixel = np.linalg.inv(pixel_to_screen)
        coefficients = (
            float(screen_to_pixel[0, 0]),
            float(screen_to_pixel[0, 1]),
            float(screen_to_pixel[0, 2]),
            float(screen_to_pixel[1, 0]),
            float(screen_to_pixel[1, 1]),
            float(screen_to_pixel[1, 2]),
        )
        source = layer.image.convert("RGBA")
        warped: Image.Image | None = None
        try:
            if layer.opacity < 1.0:
                alpha = source.getchannel("A").point(lambda value: int(value * layer.opacity))
                try:
                    source.putalpha(alpha)
                finally:
                    alpha.close()
            warped = source.transform(
                canvas.size,
                Image.Transform.AFFINE,
                coefficients,
                resample=Image.Resampling.BILINEAR,
                fillcolor=(0, 0, 0, 0),
            )
            canvas.alpha_composite(warped)
        finally:
            if warped is not None:
                warped.close()
            source.close()

    def _draw_feature(
        self,
        draw: ImageDraw.ImageDraw,
        transform: ViewportTransform,
        feature: CableFeature,
        is_selected: bool,
        is_highlighted: bool,
    ) -> None:
        screen_points = transform.world_points_to_screen(feature.points)
        if len(screen_points) < 2:
            return
        if is_highlighted:
            draw.line(screen_points, fill=(0, 160, 255, 105), width=10)
        if is_selected:
            draw.line(screen_points, fill=(255, 215, 0, 215), width=7)
        line_width = 4 if transform.meters_per_pixel < 0.05 else 3 if transform.meters_per_pixel < 0.2 else 2
        draw.line(screen_points, fill=(*feature.color, 230), width=line_width)
        if len(screen_points) == 2 and screen_points[0] == screen_points[1]:
            x, y = screen_points[0]
            radius = max(4, line_width + 2)
            if is_highlighted:
                draw.ellipse((x - radius - 4, y - radius - 4, x + radius + 4, y + radius + 4), fill=(0, 160, 255, 105))
            if is_selected:
                draw.ellipse((x - radius - 3, y - radius - 3, x + radius + 3, y + radius + 3), fill=(255, 215, 0, 215))
            draw.ellipse((x - radius, y - radius, x + radius, y + radius), fill=(*feature.color, 230))

    def _draw_comments(
        self,
        draw: ImageDraw.ImageDraw,
        transform: ViewportTransform,
        comments: list[MapComment],
    ) -> None:
        font = ImageFont.load_default()
        view_padding = max(transform.meters_per_pixel * 30.0, 1.0)
        visible_bounds = transform.bounds.padded(view_padding)
        for comment in comments:
            if not visible_bounds.contains(comment.x, comment.y):
                continue
            screen_x, screen_y = transform.world_to_screen(comment.x, comment.y)
            marker_radius = 5
            box_left = screen_x + 12
            box_top = screen_y - 30
            text_bbox = draw.textbbox((0, 0), comment.text, font=font)
            text_width = text_bbox[2] - text_bbox[0]
            text_height = text_bbox[3] - text_bbox[1]
            box_width = text_width + 18
            box_height = text_height + 14
            box_right = box_left + box_width
            box_bottom = box_top + box_height

            draw.line(
                [(screen_x, screen_y), (box_left, box_top + box_height / 2.0)],
                fill=(255, 122, 0, 230),
                width=2,
            )
            draw.ellipse(
                (
                    screen_x - marker_radius,
                    screen_y - marker_radius,
                    screen_x + marker_radius,
                    screen_y + marker_radius,
                ),
                fill=(255, 122, 0, 240),
                outline=(255, 255, 255, 240),
                width=2,
            )
            draw.rounded_rectangle(
                (box_left, box_top, box_right, box_bottom),
                radius=10,
                fill=(255, 255, 255, 235),
                outline=(255, 122, 0, 235),
                width=2,
            )
            draw.text(
                (box_left + 9, box_top + 7),
                comment.text,
                font=font,
                fill=(17, 17, 17, 255),
            )

    def _draw_markers(
        self,
        draw: ImageDraw.ImageDraw,
        transform: ViewportTransform,
        markers: list[MapMarker],
    ) -> None:
        view_padding = max(transform.meters_per_pixel * 18.0, 0.5)
        visible_bounds = transform.bounds.padded(view_padding)
        for marker in markers:
            if not visible_bounds.contains(marker.x, marker.y):
                continue
            screen_x, screen_y = transform.world_to_screen(marker.x, marker.y)
            radius = max(5, int(marker.radius_px))
            outline = marker.outline_color + (255,)
            fill = marker.fill_color + (235,)
            draw.ellipse(
                (
                    screen_x - radius,
                    screen_y - radius,
                    screen_x + radius,
                    screen_y + radius,
                ),
                fill=fill,
                outline=outline,
                width=3,
            )


def pick_features(
    x: float,
    y: float,
    overlays: list[DxfOverlay],
    tolerance_meters: float,
) -> list[CableFeature]:
    matches: list[tuple[float, str, str, CableFeature]] = []
    for overlay in overlays:
        if not overlay.visible:
            continue
        for feature in overlay.features:
            if not feature.bounds.padded(tolerance_meters).contains(x, y):
                continue
            distance = feature.distance_to(x, y)
            if distance <= tolerance_meters:
                matches.append((distance, feature.display_name.lower(), feature.source_path.name.lower(), feature))
    matches.sort(key=lambda item: (item[0], item[1], item[2], item[3].feature_id))
    return [feature for _, _, _, feature in matches]


def pick_feature(
    x: float,
    y: float,
    overlays: list[DxfOverlay],
    tolerance_meters: float,
) -> CableFeature | None:
    features = pick_features(x, y, overlays, tolerance_meters)
    return features[0] if features else None
