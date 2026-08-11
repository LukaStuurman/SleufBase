from __future__ import annotations

from math import ceil, hypot
from pathlib import Path
from typing import Any

from PIL import Image, ImageDraw

from .kickthemap_dxf_export import KickTheMapObjectDataset, KickTheMapObjectPoint
from .models import Bounds, GeoTiffLayer, GeoTransform


VIRTUAL_TRENCH_METADATA_KEY = "virtual_trench_payload"
TEMPLATE_DEKBAND_METADATA_KEY = "template_dekband_lines"
DEFAULT_VIRTUAL_TRENCH_WIDTH_METERS = 0.6
MIN_VIRTUAL_TRENCH_IMAGE_PX = 220
MAX_VIRTUAL_TRENCH_IMAGE_PX = 1600
TARGET_VIRTUAL_TRENCH_IMAGE_PX = 900
VIRTUAL_TRENCH_BORDER_RGBA = (0, 0, 0, 255)
VIRTUAL_TRENCH_DEKBAND_RGBA = (17, 24, 39, 235)
VIRTUAL_TRENCH_DEFAULT_POINT_RGB = (37, 99, 235)


def is_virtual_trench_layer(layer: GeoTiffLayer) -> bool:
    return isinstance(layer.metadata.get(VIRTUAL_TRENCH_METADATA_KEY), dict)


def virtual_trench_payload(layer: GeoTiffLayer) -> dict[str, Any] | None:
    payload = layer.metadata.get(VIRTUAL_TRENCH_METADATA_KEY)
    if isinstance(payload, dict):
        return payload
    return None


def virtual_trench_points(layer: GeoTiffLayer) -> list[dict[str, Any]]:
    payload = virtual_trench_payload(layer)
    if payload is None:
        return []
    raw_points = payload.get("points")
    if not isinstance(raw_points, list):
        raw_points = []
        payload["points"] = raw_points
        return raw_points
    if any(not isinstance(item, dict) for item in raw_points):
        raw_points = [item for item in raw_points if isinstance(item, dict)]
        payload["points"] = raw_points
    return raw_points


def virtual_trench_width(layer: GeoTiffLayer) -> float:
    payload = virtual_trench_payload(layer)
    if payload is None:
        return DEFAULT_VIRTUAL_TRENCH_WIDTH_METERS
    try:
        return max(0.1, float(payload.get("width_meters", DEFAULT_VIRTUAL_TRENCH_WIDTH_METERS)))
    except (TypeError, ValueError):
        return DEFAULT_VIRTUAL_TRENCH_WIDTH_METERS


def virtual_trench_endpoints(layer: GeoTiffLayer) -> tuple[dict[str, Any] | None, dict[str, Any] | None]:
    points = virtual_trench_points(layer)
    start_point = next((point for point in points if str(point.get("role", "")).lower() == "start"), None)
    end_point = next((point for point in points if str(point.get("role", "")).lower() == "end"), None)
    if start_point is not None and end_point is not None:
        return start_point, end_point
    if len(points) >= 2:
        return points[0], points[-1]
    return None, None


def ordered_virtual_trench_points(layer: GeoTiffLayer) -> list[dict[str, Any]]:
    points = virtual_trench_points(layer)
    start_point, end_point = virtual_trench_endpoints(layer)
    if start_point is None or end_point is None:
        return points
    ordered_points = [point for point in points if point is not start_point and point is not end_point]
    ordered_points.sort(key=lambda point: point_chainage(point, start_point, end_point))
    return [start_point, *ordered_points, end_point]


def point_chainage(
    point: dict[str, Any],
    start_point: dict[str, Any],
    end_point: dict[str, Any],
) -> float:
    start_x = _to_float(start_point.get("x"), 0.0)
    start_y = _to_float(start_point.get("y"), 0.0)
    end_x = _to_float(end_point.get("x"), start_x)
    end_y = _to_float(end_point.get("y"), start_y)
    point_x = _to_float(point.get("x"), start_x)
    point_y = _to_float(point.get("y"), start_y)
    dx = end_x - start_x
    dy = end_y - start_y
    length_sq = (dx * dx) + (dy * dy)
    if length_sq <= 1e-9:
        return 0.0
    return ((point_x - start_x) * dx + (point_y - start_y) * dy) / (length_sq ** 0.5)


def project_point_onto_virtual_trench(
    layer: GeoTiffLayer,
    x: float,
    y: float,
) -> tuple[float, float, float, float]:
    start_point, end_point = virtual_trench_endpoints(layer)
    if start_point is None or end_point is None:
        return float(x), float(y), 0.0, 0.0
    start_x = _to_float(start_point.get("x"), float(x))
    start_y = _to_float(start_point.get("y"), float(y))
    end_x = _to_float(end_point.get("x"), start_x)
    end_y = _to_float(end_point.get("y"), start_y)
    dx = end_x - start_x
    dy = end_y - start_y
    length_sq = (dx * dx) + (dy * dy)
    if length_sq <= 1e-9:
        return start_x, start_y, 0.0, 0.0
    raw_t = (((float(x) - start_x) * dx) + ((float(y) - start_y) * dy)) / length_sq
    clamped_t = max(0.0, min(1.0, raw_t))
    projected_x = start_x + (dx * clamped_t)
    projected_y = start_y + (dy * clamped_t)
    length = length_sq ** 0.5
    return projected_x, projected_y, clamped_t * length, abs(raw_t - clamped_t) * length


def virtual_trench_centerline(layer: GeoTiffLayer) -> list[tuple[float, float]]:
    start_point, end_point = virtual_trench_endpoints(layer)
    if start_point is None or end_point is None:
        return _rectangle_centerline(layer.bounds)
    return [
        (_to_float(start_point.get("x"), layer.bounds.min_x), _to_float(start_point.get("y"), layer.bounds.center_y)),
        (_to_float(end_point.get("x"), layer.bounds.max_x), _to_float(end_point.get("y"), layer.bounds.center_y)),
    ]


def virtual_trench_polygon(layer: GeoTiffLayer) -> list[tuple[float, float]]:
    centerline = virtual_trench_centerline(layer)
    if len(centerline) < 2:
        return _rectangle_polygon(layer.bounds)
    start_x, start_y = centerline[0]
    end_x, end_y = centerline[-1]
    dx = end_x - start_x
    dy = end_y - start_y
    length = hypot(dx, dy)
    if length <= 1e-9:
        return _rectangle_polygon(layer.bounds)
    half_width = virtual_trench_width(layer) * 0.5
    normal_x = -dy / length
    normal_y = dx / length
    return [
        (start_x + (normal_x * half_width), start_y + (normal_y * half_width)),
        (start_x - (normal_x * half_width), start_y - (normal_y * half_width)),
        (end_x - (normal_x * half_width), end_y - (normal_y * half_width)),
        (end_x + (normal_x * half_width), end_y + (normal_y * half_width)),
    ]


def build_virtual_trench_dataset(
    layer: GeoTiffLayer,
    *,
    include_endpoints: bool = True,
) -> KickTheMapObjectDataset | None:
    if not is_virtual_trench_layer(layer):
        return None
    ordered_points = ordered_virtual_trench_points(layer)
    if len(ordered_points) < 2:
        return None
    start_point, _end_point = virtual_trench_endpoints(layer)
    forced_start_xy: tuple[float, float] | None = None
    if start_point is not None:
        forced_start_xy = (
            _to_float(start_point.get("x"), 0.0),
            _to_float(start_point.get("y"), 0.0),
        )
    dataset_points: list[KickTheMapObjectPoint] = []
    for index, point in enumerate(ordered_points, start=1):
        role = str(point.get("role", "")).lower()
        if not include_endpoints and role in {"start", "end"}:
            continue
        default_name = "Beginpunt" if role == "start" else "Eindpunt" if role == "end" else f"Object {index}"
        source_name = str(point.get("source_name", "")).strip()
        if role in {"start", "end"} and not source_name:
            source_name = "PUNT"
        dataset_points.append(
            KickTheMapObjectPoint(
                object_name=str(point.get("object_name", "")).strip() or default_name,
                source_name=source_name,
                x=_to_float(point.get("x"), 0.0),
                y=_to_float(point.get("y"), 0.0),
                z=_to_optional_float(point.get("z")),
                attribute_1=str(point.get("attribute_1", "") or "").strip(),
                attribute_2=str(point.get("attribute_2", "") or "").strip(),
                attribute_3=str(point.get("attribute_3", "") or "").strip(),
            )
        )
    if not dataset_points:
        return None
    try:
        job_id = int(layer.metadata.get("kickthemap_job_id", -1))
    except (TypeError, ValueError):
        job_id = -1
    job_title = str(layer.metadata.get("kickthemap_job_title", "") or layer.path.stem).strip() or layer.path.stem
    return KickTheMapObjectDataset(
        job_id=job_id,
        job_title=job_title,
        source_path=Path(layer.path),
        points=tuple(dataset_points),
        polylines=(),
        cross_section_start_xy=forced_start_xy,
    )


def build_virtual_trench_render(
    layer: GeoTiffLayer,
    *,
    quality_multiplier: float = 1.0,
) -> tuple[Image.Image, Bounds, GeoTransform]:
    if not is_virtual_trench_layer(layer):
        return (
            Image.new("RGBA", (1, 1), (0, 0, 0, 0)),
            Bounds(0.0, 0.0, 1.0, 1.0),
            GeoTransform(1.0, 0.0, 0.0, 0.0, -1.0, 1.0),
        )
    ordered_points = ordered_virtual_trench_points(layer)
    if len(ordered_points) < 2:
        return (
            Image.new("RGBA", (1, 1), (0, 0, 0, 0)),
            Bounds(0.0, 0.0, 1.0, 1.0),
            GeoTransform(1.0, 0.0, 0.0, 0.0, -1.0, 1.0),
        )

    width_meters = virtual_trench_width(layer)
    polygon = virtual_trench_polygon(layer)
    world_points = [(_to_float(point.get("x"), 0.0), _to_float(point.get("y"), 0.0)) for point in ordered_points]
    all_points = [*polygon, *world_points]
    min_x = min(point[0] for point in all_points)
    min_y = min(point[1] for point in all_points)
    max_x = max(point[0] for point in all_points)
    max_y = max(point[1] for point in all_points)
    padding = max(1.5, width_meters * 3.0)
    bounds = Bounds(min_x, min_y, max_x, max_y).padded(padding)
    span = max(bounds.width, bounds.height, 1.0)
    render_scale = max(0.1, float(quality_multiplier))
    target_px = TARGET_VIRTUAL_TRENCH_IMAGE_PX * render_scale
    min_px = max(2, int(round(MIN_VIRTUAL_TRENCH_IMAGE_PX * render_scale)))
    max_px = max(min_px, int(round(MAX_VIRTUAL_TRENCH_IMAGE_PX * render_scale)))
    meters_per_pixel = max(0.02 / render_scale, min(0.15 / render_scale, span / target_px))
    width_px = max(min_px, min(max_px, int(ceil(bounds.width / meters_per_pixel))))
    height_px = max(min_px, min(max_px, int(ceil(bounds.height / meters_per_pixel))))
    width_px = max(2, width_px)
    height_px = max(2, height_px)

    image = Image.new("RGBA", (width_px, height_px), (0, 0, 0, 0))
    draw = ImageDraw.Draw(image, "RGBA")
    centerline = virtual_trench_centerline(layer)
    if len(centerline) >= 2:
        start_x, start_y = centerline[0]
        end_x, end_y = centerline[-1]
        axis_dx = end_x - start_x
        axis_dy = end_y - start_y
        axis_length = hypot(axis_dx, axis_dy)
        if axis_length > 1e-9:
            unit_normal_x = -axis_dy / axis_length
            unit_normal_y = axis_dx / axis_length
        else:
            unit_normal_x = 0.0
            unit_normal_y = 1.0
    else:
        unit_normal_x = 0.0
        unit_normal_y = 1.0

    def to_pixel(x: float, y: float) -> tuple[float, float]:
        px = ((x - bounds.min_x) / max(bounds.width, 1e-9)) * (width_px - 1)
        py = ((bounds.max_y - y) / max(bounds.height, 1e-9)) * (height_px - 1)
        return px, py

    polygon_pixels = [to_pixel(x, y) for x, y in polygon]
    border_width = max(2, int(round(2 * render_scale)))
    dekband_line_width = max(border_width + 1, int(round(4.0 * render_scale)))
    point_line_width = max(2, int(round(2.5 * render_scale)))
    point_line_half_length = max(0.04, width_meters * 0.5)

    for row in virtual_trench_dekband_rows(layer):
        try:
            start_chainage = float(row["start_chainage"])
            end_chainage = float(row["end_chainage"])
        except (KeyError, TypeError, ValueError):
            continue
        start_world = _virtual_trench_chainage_world_point(
            start_x,
            start_y,
            axis_dx,
            axis_dy,
            axis_length,
            start_chainage,
        )
        end_world = _virtual_trench_chainage_world_point(
            start_x,
            start_y,
            axis_dx,
            axis_dy,
            axis_length,
            end_chainage,
        )
        if hypot(end_world[0] - start_world[0], end_world[1] - start_world[1]) <= 1e-9:
            continue
        display_rgb = _display_rgb(row) or VIRTUAL_TRENCH_DEKBAND_RGBA[:3]
        draw.line(
            [to_pixel(*start_world), to_pixel(*end_world)],
            fill=(*display_rgb, VIRTUAL_TRENCH_DEKBAND_RGBA[3]),
            width=dekband_line_width,
        )

    for point in ordered_points:
        role = str(point.get("role", "")).lower()
        if role in {"start", "end"}:
            continue
        point_x = _to_float(point.get("x"), 0.0)
        point_y = _to_float(point.get("y"), 0.0)
        projected_x, projected_y, _chainage, _outside_distance = project_point_onto_virtual_trench(layer, point_x, point_y)
        start_world = (
            projected_x + (unit_normal_x * point_line_half_length),
            projected_y + (unit_normal_y * point_line_half_length),
        )
        end_world = (
            projected_x - (unit_normal_x * point_line_half_length),
            projected_y - (unit_normal_y * point_line_half_length),
        )
        display_rgb = _display_rgb(point) or VIRTUAL_TRENCH_DEFAULT_POINT_RGB
        draw.line(
            [to_pixel(*start_world), to_pixel(*end_world)],
            fill=(*display_rgb, 235),
            width=point_line_width,
        )

    if len(polygon_pixels) >= 2:
        draw.line(
            [*polygon_pixels, polygon_pixels[0]],
            fill=VIRTUAL_TRENCH_BORDER_RGBA,
            width=border_width,
        )

    transform = GeoTransform(
        bounds.width / width_px,
        0.0,
        bounds.min_x,
        0.0,
        -(bounds.height / height_px),
        bounds.max_y,
    )
    return image, bounds, transform


def refresh_virtual_trench_layer(
    layer: GeoTiffLayer,
    *,
    quality_multiplier: float = 1.0,
) -> None:
    image, bounds, transform = build_virtual_trench_render(layer, quality_multiplier=quality_multiplier)
    layer.image = image
    layer.bounds = bounds
    layer.transform = transform
    layer.epsg = 28992


def _rectangle_centerline(bounds: Bounds) -> list[tuple[float, float]]:
    return [(bounds.min_x, bounds.center_y), (bounds.max_x, bounds.center_y)]


def _rectangle_polygon(bounds: Bounds) -> list[tuple[float, float]]:
    return [
        (bounds.min_x, bounds.min_y),
        (bounds.min_x, bounds.max_y),
        (bounds.max_x, bounds.max_y),
        (bounds.max_x, bounds.min_y),
    ]


def _to_float(value: object, default: float) -> float:
    try:
        return float(value)
    except (TypeError, ValueError):
        return float(default)


def _to_optional_float(value: object) -> float | None:
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def virtual_trench_dekband_rows(layer: GeoTiffLayer) -> list[dict[str, object]]:
    payload = layer.metadata.get(TEMPLATE_DEKBAND_METADATA_KEY)
    if not isinstance(payload, list):
        return []
    rows: list[dict[str, object]] = []
    start_point, end_point = virtual_trench_endpoints(layer)
    if start_point is None or end_point is None:
        return rows
    axis_length = hypot(
        _to_float(end_point.get("x"), 0.0) - _to_float(start_point.get("x"), 0.0),
        _to_float(end_point.get("y"), 0.0) - _to_float(start_point.get("y"), 0.0),
    )
    if axis_length <= 1e-9:
        return rows
    for item in payload:
        if not isinstance(item, dict):
            continue
        try:
            start_chainage = float(item.get("start_chainage"))
            end_chainage = float(item.get("end_chainage"))
        except (TypeError, ValueError):
            continue
        if end_chainage < start_chainage:
            start_chainage, end_chainage = end_chainage, start_chainage
        start_chainage = max(0.0, min(axis_length, start_chainage))
        end_chainage = max(0.0, min(axis_length, end_chainage))
        if abs(end_chainage - start_chainage) <= 1e-6:
            continue
        rows.append(
            {
                "start_chainage": start_chainage,
                "end_chainage": end_chainage,
                "source_name": str(item.get("source_name", "") or "").strip() or "Dekband",
                "display_rgb": item.get("display_rgb"),
            }
        )
    rows.sort(key=lambda row: (float(row["start_chainage"]), float(row["end_chainage"])))
    return rows


def _virtual_trench_chainage_world_point(
    start_x: float,
    start_y: float,
    axis_dx: float,
    axis_dy: float,
    axis_length: float,
    chainage: float,
) -> tuple[float, float]:
    if axis_length <= 1e-9:
        return float(start_x), float(start_y)
    normalized_chainage = max(0.0, min(float(axis_length), float(chainage)))
    ratio = normalized_chainage / float(axis_length)
    return (
        float(start_x) + (float(axis_dx) * ratio),
        float(start_y) + (float(axis_dy) * ratio),
    )


def _display_rgb(point: dict[str, Any]) -> tuple[int, int, int] | None:
    raw_value = point.get("display_rgb")
    if not isinstance(raw_value, (list, tuple)) or len(raw_value) < 3:
        return None
    try:
        red = max(0, min(255, int(raw_value[0])))
        green = max(0, min(255, int(raw_value[1])))
        blue = max(0, min(255, int(raw_value[2])))
    except (TypeError, ValueError):
        return None
    return red, green, blue
