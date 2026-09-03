from __future__ import annotations

import sys
from math import ceil, hypot
from typing import Any

from PIL import Image, ImageDraw

from .models import Bounds, GeoTransform


PATCH_VERSION = 2
_EPSILON = 1e-9
_INTERSECTION_TOLERANCE = 1e-8


def _cross(ax: float, ay: float, bx: float, by: float) -> float:
    return (ax * by) - (ay * bx)


def _point_in_polygon(
    x: float,
    y: float,
    polygon: list[tuple[float, float]],
) -> bool:
    if len(polygon) < 3:
        return False
    inside = False
    previous_index = len(polygon) - 1
    for index, (current_x, current_y) in enumerate(polygon):
        previous_x, previous_y = polygon[previous_index]
        if (current_y > y) != (previous_y > y):
            crossing_x = (
                (previous_x - current_x)
                * (y - current_y)
                / ((previous_y - current_y) or 1e-16)
                + current_x
            )
            if x < crossing_x:
                inside = not inside
        previous_index = index
    return inside


def _line_polygon_parameters(
    origin_x: float,
    origin_y: float,
    direction_x: float,
    direction_y: float,
    polygon: list[tuple[float, float]],
) -> list[float]:
    """Return sorted parameters where an infinite line crosses a polygon."""

    values: list[float] = []
    for index, (start_x, start_y) in enumerate(polygon):
        end_x, end_y = polygon[(index + 1) % len(polygon)]
        segment_x = end_x - start_x
        segment_y = end_y - start_y
        relative_x = start_x - origin_x
        relative_y = start_y - origin_y
        denominator = _cross(direction_x, direction_y, segment_x, segment_y)

        if abs(denominator) <= _EPSILON:
            # If the cross-section is collinear with a polygon edge, include both
            # edge vertices. The dedupe step below turns this into stable bounds.
            if abs(_cross(relative_x, relative_y, direction_x, direction_y)) <= _INTERSECTION_TOLERANCE:
                values.append((relative_x * direction_x) + (relative_y * direction_y))
                end_relative_x = end_x - origin_x
                end_relative_y = end_y - origin_y
                values.append((end_relative_x * direction_x) + (end_relative_y * direction_y))
            continue

        line_t = _cross(relative_x, relative_y, segment_x, segment_y) / denominator
        segment_t = _cross(relative_x, relative_y, direction_x, direction_y) / denominator
        if -_INTERSECTION_TOLERANCE <= segment_t <= 1.0 + _INTERSECTION_TOLERANCE:
            values.append(line_t)

    values.sort()
    deduped: list[float] = []
    for value in values:
        if deduped and abs(value - deduped[-1]) <= _INTERSECTION_TOLERANCE:
            continue
        deduped.append(value)
    return deduped


def local_cross_section_segment(
    polygon: list[tuple[float, float]],
    origin_x: float,
    origin_y: float,
    normal_x: float,
    normal_y: float,
    *,
    preferred_t: float = 0.0,
) -> tuple[tuple[float, float], tuple[float, float]] | None:
    """Return the local inside span of the polygon at one cable/pipe position.

    The old renderer used one global width for every cable/pipe marker. For an
    irregular MarXact polygon that is effectively the widest trench section and
    lets markers protrude at narrower sections. This function intersects the
    perpendicular marker line with the polygon at the marker's own chainage.

    Concave polygons can create more than one inside interval. In that case the
    interval containing the original measured object offset is preferred.
    """

    if len(polygon) < 3:
        return None
    direction_length = hypot(normal_x, normal_y)
    if direction_length <= _EPSILON:
        return None
    direction_x = normal_x / direction_length
    direction_y = normal_y / direction_length
    parameters = _line_polygon_parameters(
        origin_x,
        origin_y,
        direction_x,
        direction_y,
        polygon,
    )
    if len(parameters) < 2:
        return None

    inside_intervals: list[tuple[float, float]] = []
    for lower, upper in zip(parameters, parameters[1:]):
        if upper - lower <= _INTERSECTION_TOLERANCE:
            continue
        middle = (lower + upper) * 0.5
        middle_x = origin_x + (direction_x * middle)
        middle_y = origin_y + (direction_y * middle)
        if _point_in_polygon(middle_x, middle_y, polygon):
            inside_intervals.append((lower, upper))
    if not inside_intervals:
        return None

    containing = [
        interval
        for interval in inside_intervals
        if interval[0] - _INTERSECTION_TOLERANCE
        <= preferred_t
        <= interval[1] + _INTERSECTION_TOLERANCE
    ]
    if containing:
        lower, upper = min(
            containing,
            key=lambda interval: abs(((interval[0] + interval[1]) * 0.5) - preferred_t),
        )
    else:
        lower, upper = min(
            inside_intervals,
            key=lambda interval: min(
                abs(preferred_t - interval[0]),
                abs(preferred_t - interval[1]),
            ),
        )

    return (
        (origin_x + (direction_x * lower), origin_y + (direction_y * lower)),
        (origin_x + (direction_x * upper), origin_y + (direction_y * upper)),
    )


def install_marxact_local_cross_section_patch() -> None:
    from . import cadastral_export as cadastral_export_module
    from . import marxact_import as marxact_import_module
    from . import virtual_trench as vt
    from .marxact_boundary_patch import _clip_render_to_measured_boundary

    if int(getattr(vt, "_marxact_local_cross_section_patch_version", 0) or 0) >= PATCH_VERSION:
        return

    def build_virtual_trench_render_local(
        layer,
        *,
        quality_multiplier: float = 1.0,
    ):
        if not vt.is_virtual_trench_layer(layer):
            return (
                Image.new("RGBA", (1, 1), (0, 0, 0, 0)),
                Bounds(0.0, 0.0, 1.0, 1.0),
                GeoTransform(1.0, 0.0, 0.0, 0.0, -1.0, 1.0),
            )
        ordered_points = vt.ordered_virtual_trench_points(layer)
        if len(ordered_points) < 2:
            return (
                Image.new("RGBA", (1, 1), (0, 0, 0, 0)),
                Bounds(0.0, 0.0, 1.0, 1.0),
                GeoTransform(1.0, 0.0, 0.0, 0.0, -1.0, 1.0),
            )

        width_meters = vt.virtual_trench_width(layer)
        # This is the single source of truth for what may be visible. For a
        # MarXact layer the boundary patch returns the measured 3D-POLYLINE XY
        # contour here; for a normal virtual trench it returns the rectangle.
        polygon = vt.virtual_trench_polygon(layer)
        world_points = [
            (vt._to_float(point.get("x"), 0.0), vt._to_float(point.get("y"), 0.0))
            for point in ordered_points
        ]
        all_points = [*polygon, *world_points]
        min_x = min(point[0] for point in all_points)
        min_y = min(point[1] for point in all_points)
        max_x = max(point[0] for point in all_points)
        max_y = max(point[1] for point in all_points)
        padding = max(1.5, width_meters * 3.0)
        bounds = Bounds(min_x, min_y, max_x, max_y).padded(padding)
        span = max(bounds.width, bounds.height, 1.0)
        render_scale = max(0.1, float(quality_multiplier))
        target_px = vt.TARGET_VIRTUAL_TRENCH_IMAGE_PX * render_scale
        min_px = max(2, int(round(vt.MIN_VIRTUAL_TRENCH_IMAGE_PX * render_scale)))
        max_px = max(min_px, int(round(vt.MAX_VIRTUAL_TRENCH_IMAGE_PX * render_scale)))
        meters_per_pixel = max(
            0.02 / render_scale,
            min(0.15 / render_scale, span / target_px),
        )
        width_px = max(min_px, min(max_px, int(ceil(bounds.width / meters_per_pixel))))
        height_px = max(min_px, min(max_px, int(ceil(bounds.height / meters_per_pixel))))
        width_px = max(2, width_px)
        height_px = max(2, height_px)

        image = Image.new("RGBA", (width_px, height_px), (0, 0, 0, 0))
        draw = ImageDraw.Draw(image, "RGBA")
        centerline = vt.virtual_trench_centerline(layer)
        if len(centerline) >= 2:
            start_x, start_y = centerline[0]
            end_x, end_y = centerline[-1]
            axis_dx = end_x - start_x
            axis_dy = end_y - start_y
            axis_length = hypot(axis_dx, axis_dy)
            if axis_length > _EPSILON:
                unit_normal_x = -axis_dy / axis_length
                unit_normal_y = axis_dx / axis_length
            else:
                unit_normal_x = 0.0
                unit_normal_y = 1.0
        else:
            start_x = start_y = axis_dx = axis_dy = axis_length = 0.0
            unit_normal_x = 0.0
            unit_normal_y = 1.0

        def to_pixel(x: float, y: float) -> tuple[float, float]:
            px = ((x - bounds.min_x) / max(bounds.width, _EPSILON)) * (width_px - 1)
            py = ((bounds.max_y - y) / max(bounds.height, _EPSILON)) * (height_px - 1)
            return px, py

        polygon_pixels = [to_pixel(x, y) for x, y in polygon]
        border_width = max(2, int(round(2 * render_scale)))
        dekband_line_width = max(border_width + 1, int(round(4.0 * render_scale)))
        point_line_width = max(2, int(round(2.5 * render_scale)))
        fallback_half_length = max(0.04, width_meters * 0.5)

        for row in vt.virtual_trench_dekband_rows(layer):
            try:
                start_chainage = float(row["start_chainage"])
                end_chainage = float(row["end_chainage"])
            except (KeyError, TypeError, ValueError):
                continue
            start_world = vt._virtual_trench_chainage_world_point(
                start_x,
                start_y,
                axis_dx,
                axis_dy,
                axis_length,
                start_chainage,
            )
            end_world = vt._virtual_trench_chainage_world_point(
                start_x,
                start_y,
                axis_dx,
                axis_dy,
                axis_length,
                end_chainage,
            )
            if hypot(end_world[0] - start_world[0], end_world[1] - start_world[1]) <= _EPSILON:
                continue
            display_rgb = vt._display_rgb(row) or vt.VIRTUAL_TRENCH_DEKBAND_RGBA[:3]
            draw.line(
                [to_pixel(*start_world), to_pixel(*end_world)],
                fill=(*display_rgb, vt.VIRTUAL_TRENCH_DEKBAND_RGBA[3]),
                width=dekband_line_width,
            )

        for point in ordered_points:
            role = str(point.get("role", "")).lower()
            if role in {"start", "end"}:
                continue
            point_x = vt._to_float(point.get("x"), 0.0)
            point_y = vt._to_float(point.get("y"), 0.0)
            projected_x, projected_y, _chainage, _outside_distance = (
                vt.project_point_onto_virtual_trench(layer, point_x, point_y)
            )
            preferred_t = (
                ((point_x - projected_x) * unit_normal_x)
                + ((point_y - projected_y) * unit_normal_y)
            )
            local_segment = local_cross_section_segment(
                polygon,
                projected_x,
                projected_y,
                unit_normal_x,
                unit_normal_y,
                preferred_t=preferred_t,
            )
            if local_segment is None:
                start_world = (
                    projected_x + (unit_normal_x * fallback_half_length),
                    projected_y + (unit_normal_y * fallback_half_length),
                )
                end_world = (
                    projected_x - (unit_normal_x * fallback_half_length),
                    projected_y - (unit_normal_y * fallback_half_length),
                )
            else:
                start_world, end_world = local_segment

            display_rgb = vt._display_rgb(point) or vt.VIRTUAL_TRENCH_DEFAULT_POINT_RGB
            draw.line(
                [to_pixel(*start_world), to_pixel(*end_world)],
                fill=(*display_rgb, 235),
                width=point_line_width,
            )

        if len(polygon_pixels) >= 2:
            draw.line(
                [*polygon_pixels, polygon_pixels[0]],
                fill=vt.VIRTUAL_TRENCH_BORDER_RGBA,
                width=border_width,
            )

        # Hard-clip every render to the exact same polygon that was used for
        # the geometry. Previously this final safety net depended on a second
        # metadata lookup (boundary_reader). That allowed stale/imported layers
        # to render coloured marker strokes outside the black contour even when
        # virtual_trench_polygon() already returned the correct measured shape.
        # By masking unconditionally against `polygon`, no cable/pipe/dekband
        # pixel can survive outside the visible SleufBase trench contour.
        if len(polygon) >= 3:
            _clip_render_to_measured_boundary(
                image,
                bounds,
                [(float(x), float(y), None) for x, y in polygon],
                border_rgba=vt.VIRTUAL_TRENCH_BORDER_RGBA,
                border_width=border_width,
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

    vt.build_virtual_trench_render = build_virtual_trench_render_local
    cadastral_export_module.build_virtual_trench_render = build_virtual_trench_render_local
    marxact_import_module.build_virtual_trench_render = build_virtual_trench_render_local

    import_patch = sys.modules.get("SleufBase.marxact_import_patch")
    if import_patch is not None:
        setattr(import_patch, "build_virtual_trench_render", build_virtual_trench_render_local)

    vt._marxact_local_cross_section_patch_version = PATCH_VERSION
