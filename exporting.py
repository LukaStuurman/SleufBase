from __future__ import annotations

import math
from pathlib import Path
from typing import Protocol

import numpy as np
from PIL import Image, ImageDraw, ImageFont

from .models import Bounds, DxfOverlay, GeoTiffLayer, MapComment, ProfileReferenceAnnotation, ViewportTransform
from .renderer import MapRenderer


EXPORT_ASPECT_RATIO = 1.41421356
EXPORT_DPI = 300
EXPORT_SCALE = 225
EXPORT_SCALEBAR_METERS = 20
EXPORT_COORD_TICK_METERS = 20
EXPORT_PAGE_WIDTH_MM = 297
EXPORT_PAGE_HEIGHT_MM = 210
EXPORT_OUTER_MARGIN_PX = 18
EXPORT_COORD_BAND_PX = 64


class ExportBackgroundProvider(Protocol):
    def fetch_map(self, bounds: Bounds, size: tuple[int, int]) -> Image.Image:
        ...


def _load_font(size: int, bold: bool = False) -> ImageFont.ImageFont:
    font_candidates = [
        "C:/Windows/Fonts/segoeui.ttf",
        "C:/Windows/Fonts/segoeuib.ttf",
        "C:/Windows/Fonts/arial.ttf",
    ]
    if bold:
        font_candidates.insert(0, "C:/Windows/Fonts/segoeuib.ttf")
    for candidate in font_candidates:
        try:
            return ImageFont.truetype(candidate, size)
        except OSError:
            continue
    return ImageFont.load_default()


def _format_coordinate(value: float) -> str:
    return str(int(round(value)))


def _tick_count(bounds: Bounds, interval: int, axis: str) -> int:
    if axis == "x":
        start = int(math.ceil(bounds.min_x / interval) * interval)
        end = int(math.floor(bounds.max_x / interval) * interval)
    else:
        start = int(math.ceil(bounds.min_y / interval) * interval)
        end = int(math.floor(bounds.max_y / interval) * interval)
    if end < start:
        return 0
    return ((end - start) // interval) + 1


def _choose_coordinate_interval(bounds: Bounds, axis: str) -> int:
    span = bounds.width if axis == "x" else bounds.height
    target = max(1.0, span / 3.0)
    candidates = [1, 2, 5, 10, 20, 25, 50, 100, 200]
    viable = [candidate for candidate in candidates if _tick_count(bounds, candidate, axis) >= 2]
    if not viable:
        return 1
    best = viable[0]
    best_delta = abs(viable[0] - target)
    for candidate in viable[1:]:
        delta = abs(candidate - target)
        if delta < best_delta:
            best = candidate
            best_delta = delta
    return best


def _draw_outlined_text(
    draw: ImageDraw.ImageDraw,
    position: tuple[float, float],
    text: str,
    font: ImageFont.ImageFont,
    fill: tuple[int, int, int] = (255, 255, 255),
    stroke_fill: tuple[int, int, int] = (0, 0, 0),
    stroke_width: int = 3,
    anchor: str | None = None,
) -> None:
    draw.text(
        position,
        text,
        font=font,
        fill=fill,
        stroke_width=stroke_width,
        stroke_fill=stroke_fill,
        anchor=anchor,
    )


def _draw_rotated_text(
    page: Image.Image,
    position: tuple[int, int],
    text: str,
    font: ImageFont.ImageFont,
    angle: int,
    fill: tuple[int, int, int] = (0, 0, 0),
) -> None:
    temp = Image.new("RGBA", (640, 220), (0, 0, 0, 0))
    temp_draw = ImageDraw.Draw(temp)
    text_bbox = temp_draw.textbbox((0, 0), text, font=font)
    text_width = text_bbox[2] - text_bbox[0]
    text_height = text_bbox[3] - text_bbox[1]
    temp_draw.text(((temp.width - text_width) / 2, (temp.height - text_height) / 2), text, font=font, fill=fill)
    rotated = temp.rotate(angle, expand=True)
    page.alpha_composite(rotated, (int(position[0] - rotated.width / 2), int(position[1] - rotated.height / 2)))


def _prepare_export_tiff_layer(tiff_layer: GeoTiffLayer, forced_opacity: float | None = None) -> GeoTiffLayer:
    rgba = tiff_layer.image.convert("RGBA")
    pixels = np.array(rgba)
    white_mask = pixels[:, :, :3].min(axis=2) >= 245
    pixels[white_mask, 3] = 0
    prepared_image = Image.fromarray(pixels, mode="RGBA")
    opacity = min(tiff_layer.opacity, 0.7) if forced_opacity is None else max(0.0, min(1.0, float(forced_opacity)))
    return GeoTiffLayer(
        path=tiff_layer.path,
        image=prepared_image,
        transform=tiff_layer.transform,
        bounds=tiff_layer.bounds,
        epsg=tiff_layer.epsg,
        opacity=opacity,
        metadata=dict(tiff_layer.metadata),
    )


class MapExporter:
    def __init__(
        self,
        default_background_provider: ExportBackgroundProvider,
        renderer: MapRenderer,
        dpi: int = EXPORT_DPI,
        scale: int = EXPORT_SCALE,
    ) -> None:
        self.default_background_provider = default_background_provider
        self.renderer = renderer
        self.dpi = dpi
        self.scale = scale
        self.pixels_per_meter = dpi * 39.37007874 / scale
        self.page_width_px = int(round((EXPORT_PAGE_WIDTH_MM / 25.4) * dpi))
        self.page_height_px = int(round((EXPORT_PAGE_HEIGHT_MM / 25.4) * dpi))
        self.outer_margin = EXPORT_OUTER_MARGIN_PX
        self.coord_band = EXPORT_COORD_BAND_PX
        self.map_width_px = self.page_width_px - 2 * (self.outer_margin + self.coord_band)
        self.map_height_px = self.page_height_px - 2 * (self.outer_margin + self.coord_band)
        self.map_aspect_ratio = self.map_width_px / self.map_height_px
        self.fixed_map_width_m = self.map_width_px / self.pixels_per_meter
        self.fixed_map_height_m = self.map_height_px / self.pixels_per_meter

    def export_all(
        self,
        output_directory: str | Path,
        tiff_layers: list[GeoTiffLayer],
        dxf_overlays: list[DxfOverlay],
        map_comments: list[MapComment] | None = None,
        status_callback=None,
        background_provider: ExportBackgroundProvider | None = None,
        background_attribution: str | None = None,
    ) -> list[Path]:
        output_path = Path(output_directory)
        output_path.mkdir(parents=True, exist_ok=True)
        results: list[Path] = []
        for index, layer in enumerate(tiff_layers, start=1):
            if status_callback is not None:
                status_callback(f"Exporteer kaart {index}/{len(tiff_layers)}: {layer.name}")
            results.append(
                self.export_single(
                    output_path,
                    layer,
                    dxf_overlays,
                    map_comments=map_comments,
                    background_provider=background_provider,
                    background_attribution=background_attribution,
                )
            )
        return results

    def export_single(
        self,
        output_directory: Path,
        tiff_layer: GeoTiffLayer,
        dxf_overlays: list[DxfOverlay],
        map_comments: list[MapComment] | None = None,
        background_provider: ExportBackgroundProvider | None = None,
        background_attribution: str | None = None,
    ) -> Path:
        page = self.build_page_image(
            tiff_layer,
            dxf_overlays,
            map_comments=map_comments,
            background_provider=background_provider,
            background_attribution=background_attribution,
        )

        png_path = output_directory / f"{tiff_layer.path.stem}_kaart.png"
        legacy_pdf_path = output_directory / f"{tiff_layer.path.stem}_kaart.pdf"
        page.convert("RGB").save(png_path, dpi=(self.dpi, self.dpi))
        if legacy_pdf_path.exists():
            legacy_pdf_path.unlink()
        return png_path

    def build_page_image(
        self,
        tiff_layer: GeoTiffLayer,
        dxf_overlays: list[DxfOverlay],
        map_comments: list[MapComment] | None = None,
        background_provider: ExportBackgroundProvider | None = None,
        background_attribution: str | None = None,
        reference_annotation: ProfileReferenceAnnotation | None = None,
        force_tiff_opacity: float | None = None,
    ) -> Image.Image:
        padded_bounds = tiff_layer.bounds.padded(max(1.0, min(4.0, max(tiff_layer.bounds.width, tiff_layer.bounds.height) * 0.1)))
        map_bounds = self._determine_map_bounds(padded_bounds)

        provider = background_provider or self.default_background_provider
        background = provider.fetch_map(map_bounds, (self.map_width_px, self.map_height_px))
        export_tiff = _prepare_export_tiff_layer(tiff_layer, forced_opacity=force_tiff_opacity)
        map_image = self.renderer.render(
            map_bounds,
            (self.map_width_px, self.map_height_px),
            [export_tiff],
            dxf_overlays,
            background=background,
            map_comments=map_comments,
        )
        page = self._compose_page(
            map_image,
            map_bounds,
            background_attribution=background_attribution,
            reference_annotation=reference_annotation,
        )
        return page

    def _determine_map_bounds(self, required_bounds: Bounds) -> Bounds:
        width_m = max(required_bounds.width, self.fixed_map_width_m)
        height_m = max(required_bounds.height, self.fixed_map_height_m)
        candidate = Bounds(
            required_bounds.center_x - width_m / 2.0,
            required_bounds.center_y - height_m / 2.0,
            required_bounds.center_x + width_m / 2.0,
            required_bounds.center_y + height_m / 2.0,
        ).expand_to_aspect_ratio(self.map_aspect_ratio)
        if candidate.min_x > required_bounds.min_x:
            dx = candidate.min_x - required_bounds.min_x
            candidate = Bounds(candidate.min_x - dx, candidate.min_y, candidate.max_x - dx, candidate.max_y)
        if candidate.max_x < required_bounds.max_x:
            dx = required_bounds.max_x - candidate.max_x
            candidate = Bounds(candidate.min_x + dx, candidate.min_y, candidate.max_x + dx, candidate.max_y)
        if candidate.min_y > required_bounds.min_y:
            dy = candidate.min_y - required_bounds.min_y
            candidate = Bounds(candidate.min_x, candidate.min_y - dy, candidate.max_x, candidate.max_y - dy)
        if candidate.max_y < required_bounds.max_y:
            dy = required_bounds.max_y - candidate.max_y
            candidate = Bounds(candidate.min_x, candidate.min_y + dy, candidate.max_x, candidate.max_y + dy)
        return candidate

    def _compose_page(
        self,
        map_image: Image.Image,
        map_bounds: Bounds,
        background_attribution: str | None = None,
        reference_annotation: ProfileReferenceAnnotation | None = None,
    ) -> Image.Image:
        outer_margin = self.outer_margin
        coord_band = self.coord_band

        page_width = self.page_width_px
        page_height = self.page_height_px
        page = Image.new("RGBA", (page_width, page_height), (255, 255, 255, 255))
        draw = ImageDraw.Draw(page, "RGBA")

        map_left = outer_margin + coord_band
        map_top = outer_margin + coord_band
        page.alpha_composite(map_image, (map_left, map_top))
        draw.rectangle(
            (map_left, map_top, map_left + map_image.width, map_top + map_image.height),
            outline=(0, 0, 0, 255),
            width=2,
        )

        coord_font = _load_font(36)
        overlay_font = _load_font(40, bold=True)
        north_font = _load_font(80, bold=True)

        self._draw_coordinate_frame(page, draw, map_bounds, map_left, map_top, map_image.width, map_image.height, coord_font)
        self._draw_grid_crosses(draw, map_bounds, map_left, map_top, map_image.width, map_image.height)
        self._draw_scale_bar(draw, map_bounds, map_left, map_top, map_image.width, map_image.height, overlay_font)
        legend_box: tuple[int, int, int, int] | None = None
        if reference_annotation is not None:
            self._draw_profile_reference_annotation(
                draw,
                map_bounds,
                map_left,
                map_top,
                map_image.width,
                map_image.height,
                reference_annotation,
            )
            legend_box = self._draw_profile_reference_legend(
                draw,
                map_left,
                map_top,
                map_image.width,
                map_image.height,
            )
        self._draw_north_arrow(draw, map_left, map_top, map_image.width, map_image.height, north_font, legend_box=legend_box)
        if background_attribution:
            self._draw_background_attribution(
                draw,
                map_left,
                map_top,
                map_image.width,
                map_image.height,
                background_attribution,
                legend_box=legend_box,
            )
        return page

    def _draw_profile_reference_annotation(
        self,
        draw: ImageDraw.ImageDraw,
        map_bounds: Bounds,
        map_left: int,
        map_top: int,
        map_width: int,
        map_height: int,
        annotation: ProfileReferenceAnnotation,
    ) -> None:
        transform = ViewportTransform(map_bounds, map_width, map_height)
        point_screen = transform.world_to_screen(annotation.x, annotation.y)
        point = (map_left + point_screen[0], map_top + point_screen[1])
        point_radius = 11
        outline_color = (0, 0, 0, 255)
        point_fill_color = (255, 255, 255, 240)
        draw.ellipse(
            (
                point[0] - point_radius,
                point[1] - point_radius,
                point[0] + point_radius,
                point[1] + point_radius,
            ),
            fill=point_fill_color,
            outline=outline_color,
            width=4,
        )

    def _draw_background_attribution(
        self,
        draw: ImageDraw.ImageDraw,
        map_left: int,
        map_top: int,
        map_width: int,
        map_height: int,
        attribution: str,
        legend_box: tuple[int, int, int, int] | None = None,
    ) -> None:
        font = _load_font(20)
        text_bbox = draw.textbbox((0, 0), attribution, font=font)
        text_width = text_bbox[2] - text_bbox[0]
        text_height = text_bbox[3] - text_bbox[1]
        left = map_left + map_width - text_width - 28
        top = map_top + map_height - text_height - 22
        if legend_box is not None:
            top = max(map_top + 18, int(legend_box[1] - text_height - 22))
        draw.rounded_rectangle(
            (left - 10, top - 6, left + text_width + 10, top + text_height + 6),
            radius=8,
            fill=(255, 255, 255, 220),
        )
        draw.text((left, top), attribution, font=font, fill=(35, 35, 35))

    def _draw_profile_reference_legend(
        self,
        draw: ImageDraw.ImageDraw,
        map_left: int,
        map_top: int,
        map_width: int,
        map_height: int,
    ) -> tuple[int, int, int, int]:
        title_font = _load_font(84, bold=True)
        item_font = _load_font(72)
        title = "Legenda"
        items = (
            ("Referentiepunt", "circle"),
        )
        padding_x = 54
        padding_y = 42
        section_gap = 42
        row_gap = 48
        symbol_width = 192
        symbol_gap = 42

        title_bbox = draw.textbbox((0, 0), title, font=title_font)
        title_width = title_bbox[2] - title_bbox[0]
        title_height = title_bbox[3] - title_bbox[1]
        row_metrics: list[tuple[str, str, int, int]] = []
        max_text_width = 0
        for label, symbol in items:
            row_bbox = draw.textbbox((0, 0), label, font=item_font)
            row_width = row_bbox[2] - row_bbox[0]
            row_height = row_bbox[3] - row_bbox[1]
            row_metrics.append((label, symbol, row_width, row_height))
            max_text_width = max(max_text_width, row_width)

        content_width = max(title_width, symbol_width + symbol_gap + max_text_width)
        width = content_width + (padding_x * 2)
        rows_height = sum(max(78, row_height) for _, _, _, row_height in row_metrics) + row_gap * (len(row_metrics) - 1)
        height = padding_y + title_height + section_gap + rows_height + padding_y
        right = map_left + map_width - 1
        bottom = map_top + map_height - 1
        left = right - width
        top = bottom - height

        draw.rectangle(
            (left, top, right, bottom),
            fill=(255, 255, 255, 255),
            outline=(0, 0, 0, 255),
            width=4,
        )
        draw.text((left + padding_x, top + padding_y), title, font=title_font, fill=(0, 0, 0, 255))

        current_y = top + padding_y + title_height + section_gap
        symbol_left = left + padding_x
        text_x = symbol_left + symbol_width + symbol_gap
        for label, symbol, _, row_height in row_metrics:
            row_center_y = current_y + (max(78, row_height) / 2.0)
            if symbol == "circle":
                radius = 30
                center_x = symbol_left + 42
                draw.ellipse(
                    (
                        center_x - radius,
                        row_center_y - radius,
                        center_x + radius,
                        row_center_y + radius,
                    ),
                    fill=(255, 255, 255, 240),
                    outline=(0, 0, 0, 255),
                    width=8,
                )
            draw.text((text_x, row_center_y), label, font=item_font, fill=(0, 0, 0, 255), anchor="lm")
            current_y += max(78, row_height) + row_gap
        return (int(left), int(top), int(right), int(bottom))

    def _draw_coordinate_frame(
        self,
        page: Image.Image,
        draw: ImageDraw.ImageDraw,
        map_bounds: Bounds,
        map_left: int,
        map_top: int,
        map_width: int,
        map_height: int,
        font: ImageFont.ImageFont,
    ) -> None:
        transform = ViewportTransform(map_bounds, map_width, map_height)
        x_tick = EXPORT_COORD_TICK_METERS
        y_tick = EXPORT_COORD_TICK_METERS

        start_x = int(math.ceil(map_bounds.min_x / x_tick) * x_tick)
        end_x = int(math.floor(map_bounds.max_x / x_tick) * x_tick)
        start_y = int(math.ceil(map_bounds.min_y / y_tick) * y_tick)
        end_y = int(math.floor(map_bounds.max_y / y_tick) * y_tick)

        for coordinate in range(start_x, end_x + x_tick, x_tick):
            if coordinate < map_bounds.min_x or coordinate > map_bounds.max_x:
                continue
            screen_x, _ = transform.world_to_screen(coordinate, map_bounds.min_y)
            x = map_left + int(round(screen_x))
            draw.line((x, map_top - 22, x, map_top), fill=(0, 0, 0), width=3)
            draw.line((x, map_top + map_height, x, map_top + map_height + 22), fill=(0, 0, 0), width=3)
            draw.text((x, map_top - 28), _format_coordinate(coordinate), font=font, fill=(0, 0, 0), anchor="ms")
            draw.text((x, map_top + map_height + 28), _format_coordinate(coordinate), font=font, fill=(0, 0, 0), anchor="mt")

        for coordinate in range(start_y, end_y + y_tick, y_tick):
            if coordinate < map_bounds.min_y or coordinate > map_bounds.max_y:
                continue
            _, screen_y = transform.world_to_screen(map_bounds.min_x, coordinate)
            y = map_top + int(round(screen_y))
            draw.line((map_left - 22, y, map_left, y), fill=(0, 0, 0), width=3)
            draw.line((map_left + map_width, y, map_left + map_width + 22, y), fill=(0, 0, 0), width=3)
            label = _format_coordinate(coordinate)
            _draw_rotated_text(page, (map_left - 50, y), label, font, angle=90)
            _draw_rotated_text(page, (map_left + map_width + 50, y), label, font, angle=270)

    def _draw_grid_crosses(
        self,
        draw: ImageDraw.ImageDraw,
        map_bounds: Bounds,
        map_left: int,
        map_top: int,
        map_width: int,
        map_height: int,
    ) -> None:
        transform = ViewportTransform(map_bounds, map_width, map_height)
        x_tick = EXPORT_COORD_TICK_METERS
        y_tick = EXPORT_COORD_TICK_METERS
        start_x = int(math.ceil(map_bounds.min_x / x_tick) * x_tick)
        end_x = int(math.floor(map_bounds.max_x / x_tick) * x_tick)
        start_y = int(math.ceil(map_bounds.min_y / y_tick) * y_tick)
        end_y = int(math.floor(map_bounds.max_y / y_tick) * y_tick)

        cross_half = 11
        for coordinate_x in range(start_x, end_x + x_tick, x_tick):
            if coordinate_x < map_bounds.min_x or coordinate_x > map_bounds.max_x:
                continue
            for coordinate_y in range(start_y, end_y + y_tick, y_tick):
                if coordinate_y < map_bounds.min_y or coordinate_y > map_bounds.max_y:
                    continue
                screen_x, screen_y = transform.world_to_screen(coordinate_x, coordinate_y)
                x = map_left + int(round(screen_x))
                y = map_top + int(round(screen_y))
                draw.line((x - cross_half, y, x + cross_half, y), fill=(0, 0, 0, 190), width=2)
                draw.line((x, y - cross_half, x, y + cross_half), fill=(0, 0, 0, 190), width=2)

    def _draw_scale_bar(
        self,
        draw: ImageDraw.ImageDraw,
        map_bounds: Bounds,
        map_left: int,
        map_top: int,
        map_width: int,
        map_height: int,
        font: ImageFont.ImageFont,
    ) -> None:
        transform = ViewportTransform(map_bounds, map_width, map_height)
        total_length_px = int(round(EXPORT_SCALEBAR_METERS / transform.meters_per_pixel))
        segment_count = 4
        segment_length_px = max(1, total_length_px // segment_count)
        bar_height = 40
        outline_width = 4
        x = map_left + 72
        y = map_top + map_height - 108

        for index in range(segment_count):
            left = x + index * segment_length_px
            right = x + (index + 1) * segment_length_px
            fill = (0, 0, 0) if index % 2 == 0 else (255, 255, 255)
            draw.rectangle((left, y, right, y + bar_height), fill=fill, outline=(0, 0, 0), width=outline_width)

        for index, label in enumerate(["0", "5", "10", "15", "20 m"]):
            label_x = x + index * segment_length_px
            _draw_outlined_text(draw, (label_x, y - 14), label, font=font, stroke_width=5, anchor="ms")

    def _draw_north_arrow(
        self,
        draw: ImageDraw.ImageDraw,
        map_left: int,
        map_top: int,
        map_width: int,
        map_height: int,
        font: ImageFont.ImageFont,
        legend_box: tuple[int, int, int, int] | None = None,
    ) -> None:
        outline_width = 6
        if legend_box is None:
            center_x = map_left + map_width - 146
            bottom_y = map_top + map_height - 28
            arrow_height = 270
            half_width = 84
            notch_height = 72
        else:
            legend_left, legend_top, _, legend_bottom = legend_box
            arrow_height = max(138, min(182, (legend_bottom - legend_top) + 10))
            half_width = max(42, int(round(arrow_height * 0.3)))
            notch_height = max(34, int(round(arrow_height * 0.26)))
            bottom_y = legend_bottom - 18
            center_x = max(map_left + half_width + 28, legend_left - half_width - 34)
        tip_y = bottom_y - arrow_height

        outer = [
            (center_x, tip_y),
            (center_x - half_width, bottom_y),
            (center_x, bottom_y - notch_height),
            (center_x + half_width, bottom_y),
        ]
        draw.polygon(outer, fill=(255, 255, 255))
        draw.line((*outer, outer[0]), fill=(0, 0, 0), width=outline_width)

        _draw_outlined_text(draw, (center_x, tip_y - 24), "N", font=font, stroke_width=5, anchor="ms")
