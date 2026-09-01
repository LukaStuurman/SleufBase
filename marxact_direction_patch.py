from __future__ import annotations

import math

from . import marxact_import as _marxact


PATCH_VERSION = 1
_EPSILON = 1e-10
_INTERSECTION_TOLERANCE = 1e-8


def _principal_axis(
    points: list[tuple[float, float]],
) -> tuple[float, float, float, float] | None:
    """Return center + unit PCA axis for a set of XY points."""

    if len(points) < 2:
        return None
    center_x = sum(point[0] for point in points) / len(points)
    center_y = sum(point[1] for point in points) / len(points)
    covariance_xx = sum((point[0] - center_x) ** 2 for point in points) / len(points)
    covariance_yy = sum((point[1] - center_y) ** 2 for point in points) / len(points)
    covariance_xy = sum(
        (point[0] - center_x) * (point[1] - center_y) for point in points
    ) / len(points)
    if covariance_xx + covariance_yy <= _EPSILON:
        return None

    angle = 0.5 * math.atan2(
        2.0 * covariance_xy,
        covariance_xx - covariance_yy,
    )
    return center_x, center_y, math.cos(angle), math.sin(angle)


def _cross(ax: float, ay: float, bx: float, by: float) -> float:
    return (ax * by) - (ay * bx)


def _line_polygon_intersections(
    origin_x: float,
    origin_y: float,
    axis_x: float,
    axis_y: float,
    polygon: tuple[tuple[float, float, float | None], ...],
) -> list[float]:
    """Return sorted line parameters where an infinite axis hits the polygon."""

    intersections: list[float] = []
    for index, (start_x, start_y, _start_z) in enumerate(polygon):
        end_x, end_y, _end_z = polygon[(index + 1) % len(polygon)]
        segment_x = end_x - start_x
        segment_y = end_y - start_y
        relative_x = start_x - origin_x
        relative_y = start_y - origin_y
        denominator = _cross(axis_x, axis_y, segment_x, segment_y)

        if abs(denominator) <= _EPSILON:
            # If the axis lies on a polygon edge, both edge vertices are valid
            # boundary intersections. This also makes vertex hits deterministic.
            if abs(_cross(relative_x, relative_y, axis_x, axis_y)) <= _INTERSECTION_TOLERANCE:
                intersections.append((relative_x * axis_x) + (relative_y * axis_y))
                end_relative_x = end_x - origin_x
                end_relative_y = end_y - origin_y
                intersections.append(
                    (end_relative_x * axis_x) + (end_relative_y * axis_y)
                )
            continue

        line_t = _cross(relative_x, relative_y, segment_x, segment_y) / denominator
        segment_t = _cross(relative_x, relative_y, axis_x, axis_y) / denominator
        if -_INTERSECTION_TOLERANCE <= segment_t <= 1.0 + _INTERSECTION_TOLERANCE:
            intersections.append(line_t)

    intersections.sort()
    deduped: list[float] = []
    for value in intersections:
        if deduped and abs(value - deduped[-1]) <= _INTERSECTION_TOLERANCE:
            continue
        deduped.append(value)
    return deduped


def _endpoint_parameters(
    intersections: list[float],
    object_projections: list[float],
) -> tuple[float, float] | None:
    if len(intersections) < 2:
        return None
    if not object_projections:
        return intersections[0], intersections[-1]

    object_min = min(object_projections)
    object_max = max(object_projections)
    lower_candidates = [
        value for value in intersections if value <= object_min + _INTERSECTION_TOLERANCE
    ]
    upper_candidates = [
        value for value in intersections if value >= object_max - _INTERSECTION_TOLERANCE
    ]
    lower = max(lower_candidates) if lower_candidates else intersections[0]
    upper = min(upper_candidates) if upper_candidates else intersections[-1]
    if upper - lower <= _INTERSECTION_TOLERANCE:
        return intersections[0], intersections[-1]
    return lower, upper


def _axis_from_polygon(
    polygon: tuple[tuple[float, float, float | None], ...],
) -> tuple[float, float, float, float]:
    fallback = _principal_axis([(x, y) for x, y, _z in polygon])
    if fallback is not None:
        return fallback
    first_x, first_y, _first_z = polygon[0]
    return first_x, first_y, 1.0, 0.0


def trench_centerline(
    trench: _marxact.MarXactTrench,
) -> tuple[
    tuple[float, float, float | None],
    tuple[float, float, float | None],
    float,
]:
    """Build the MarXact axis from measured cable/pipe blocks and clip it to the 3D boundary."""

    polygon = trench.polygon
    object_points = [(float(item.x), float(item.y)) for item in trench.objects]
    object_axis = _principal_axis(object_points)

    # MarXact INSERT blocks are the measured cables/pipes. Their fitted line is
    # therefore authoritative for the proefsleuf direction. Only when fewer than
    # two distinct block positions exist do we fall back to the polygon geometry.
    using_object_axis = object_axis is not None
    center_x, center_y, axis_x, axis_y = object_axis or _axis_from_polygon(polygon)

    intersections = _line_polygon_intersections(
        center_x,
        center_y,
        axis_x,
        axis_y,
        polygon,
    )
    object_projections = [
        ((x - center_x) * axis_x) + ((y - center_y) * axis_y)
        for x, y in object_points
    ] if using_object_axis else []
    endpoint_parameters = _endpoint_parameters(intersections, object_projections)

    # A very unusual concave polygon can have the mean of all object points
    # outside the polygon. If that prevents a usable intersection, preserve the
    # old polygon-PCA fallback rather than creating invalid endpoints.
    if endpoint_parameters is None and using_object_axis:
        center_x, center_y, axis_x, axis_y = _axis_from_polygon(polygon)
        intersections = _line_polygon_intersections(
            center_x,
            center_y,
            axis_x,
            axis_y,
            polygon,
        )
        endpoint_parameters = _endpoint_parameters(intersections, [])

    normal_x = -axis_y
    normal_y = axis_x
    normal_projections = [
        ((x - center_x) * normal_x) + ((y - center_y) * normal_y)
        for x, y, _z in polygon
    ]
    width = max(0.1, max(normal_projections) - min(normal_projections))

    if endpoint_parameters is None:
        # Last-resort fallback for malformed geometry: use the polygon projection
        # extremes on the selected axis. Normal MarXact polygons take the exact
        # boundary-intersection path above.
        axis_projections = [
            ((x - center_x) * axis_x) + ((y - center_y) * axis_y)
            for x, y, _z in polygon
        ]
        endpoint_parameters = min(axis_projections), max(axis_projections)

    def endpoint(axis_value: float) -> tuple[float, float, float | None]:
        x = center_x + (axis_x * axis_value)
        y = center_y + (axis_y * axis_value)
        # Keep the v0.3.21 behavior: ground level comes from the measured 3D
        # POLYLINE itself, interpolated on the boundary segment at the endpoint.
        z = _marxact._polyline_z_at_xy(x, y, polygon)
        return x, y, z

    return endpoint(endpoint_parameters[0]), endpoint(endpoint_parameters[1]), width


def install_marxact_direction_patch() -> None:
    if int(getattr(_marxact, "_marxact_direction_patch_version", 0) or 0) >= PATCH_VERSION:
        return
    _marxact.trench_centerline = trench_centerline
    _marxact.MARXACT_IMPORT_VERSION = max(
        2,
        int(getattr(_marxact, "MARXACT_IMPORT_VERSION", 1) or 1),
    )
    _marxact._marxact_direction_patch_version = PATCH_VERSION
