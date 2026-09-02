from __future__ import annotations

import math
from statistics import median
from typing import Any

from . import marxact_import as _marxact
from .virtual_trench import VIRTUAL_TRENCH_METADATA_KEY


PATCH_VERSION = 2
_EPSILON = 1e-10
_INTERSECTION_TOLERANCE = 1e-8
_BOUNDARY_SNAP_TOLERANCE_DEGREES = 12.5
_MIN_ROBUST_RESIDUAL_METERS = 0.03

MARXACT_ALIGNMENT_ROTATION_KEY = "marxact_alignment_rotation_degrees"
MARXACT_ALIGNMENT_AUTO_START_KEY = "marxact_alignment_auto_start"
MARXACT_ALIGNMENT_AUTO_END_KEY = "marxact_alignment_auto_end"
MARXACT_ALIGNMENT_AUTO_WIDTH_KEY = "marxact_alignment_auto_width"


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


def _unique_xy(points: list[tuple[float, float]]) -> list[tuple[float, float]]:
    seen: set[tuple[int, int]] = set()
    result: list[tuple[float, float]] = []
    for x, y in points:
        key = (round(float(x) * 1_000_000), round(float(y) * 1_000_000))
        if key in seen:
            continue
        seen.add(key)
        result.append((float(x), float(y)))
    return result


def _robust_principal_axis(
    points: list[tuple[float, float]],
) -> tuple[float, float, float, float] | None:
    """Fit the measured blocks while reducing the effect of a stray block."""

    unique_points = _unique_xy(points)
    axis = _principal_axis(unique_points)
    if axis is None or len(unique_points) < 4:
        return axis

    center_x, center_y, axis_x, axis_y = axis
    residuals = [
        abs(_cross(x - center_x, y - center_y, axis_x, axis_y))
        for x, y in unique_points
    ]
    residual_median = median(residuals)
    deviations = [abs(value - residual_median) for value in residuals]
    mad = median(deviations)
    threshold = max(
        _MIN_ROBUST_RESIDUAL_METERS,
        residual_median + (2.75 * max(mad, 0.005)),
    )
    inliers = [
        point
        for point, residual in zip(unique_points, residuals)
        if residual <= threshold
    ]
    minimum_inliers = max(2, int(math.ceil(len(unique_points) * 0.6)))
    if len(inliers) < minimum_inliers or len(inliers) == len(unique_points):
        return axis
    return _principal_axis(inliers) or axis


def _axis_angle(axis_x: float, axis_y: float) -> float:
    return math.atan2(axis_y, axis_x)


def _axis_angle_difference(first: float, second: float) -> float:
    """Smallest undirected line-angle difference in radians (0..pi/2)."""

    difference = abs((first - second) % math.pi)
    return min(difference, math.pi - difference)


def _boundary_axis_near(
    polygon: tuple[tuple[float, float, float | None], ...],
    target_axis_x: float,
    target_axis_y: float,
) -> tuple[float, float] | None:
    """Return a length-weighted polygon-edge direction near the measured block axis."""

    if len(polygon) < 3:
        return None
    target_angle = _axis_angle(target_axis_x, target_axis_y)
    tolerance = math.radians(_BOUNDARY_SNAP_TOLERANCE_DEGREES)
    candidates: list[tuple[float, float]] = []

    for index, (start_x, start_y, _start_z) in enumerate(polygon):
        end_x, end_y, _end_z = polygon[(index + 1) % len(polygon)]
        dx = end_x - start_x
        dy = end_y - start_y
        length = math.hypot(dx, dy)
        if length <= 0.05:
            continue
        angle = math.atan2(dy, dx)
        if _axis_angle_difference(angle, target_angle) <= tolerance:
            candidates.append((angle, length))

    if len(candidates) < 2:
        return None

    doubled_x = sum(math.cos(2.0 * angle) * weight for angle, weight in candidates)
    doubled_y = sum(math.sin(2.0 * angle) * weight for angle, weight in candidates)
    if math.hypot(doubled_x, doubled_y) <= _EPSILON:
        return None
    snapped_angle = 0.5 * math.atan2(doubled_y, doubled_x)
    snapped_x = math.cos(snapped_angle)
    snapped_y = math.sin(snapped_angle)

    if (snapped_x * target_axis_x) + (snapped_y * target_axis_y) < 0.0:
        snapped_x *= -1.0
        snapped_y *= -1.0
    return snapped_x, snapped_y


def _automatic_object_axis(
    object_points: list[tuple[float, float]],
    polygon: tuple[tuple[float, float, float | None], ...],
) -> tuple[float, float, float, float] | None:
    axis = _robust_principal_axis(object_points)
    if axis is None:
        return None
    center_x, center_y, axis_x, axis_y = axis
    snapped = _boundary_axis_near(polygon, axis_x, axis_y)
    if snapped is not None:
        axis_x, axis_y = snapped
    return center_x, center_y, axis_x, axis_y


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
            if (
                abs(_cross(relative_x, relative_y, axis_x, axis_y))
                <= _INTERSECTION_TOLERANCE
            ):
                intersections.append((relative_x * axis_x) + (relative_y * axis_y))
                end_relative_x = end_x - origin_x
                end_relative_y = end_y - origin_y
                intersections.append(
                    (end_relative_x * axis_x) + (end_relative_y * axis_y)
                )
            continue

        line_t = _cross(relative_x, relative_y, segment_x, segment_y) / denominator
        segment_t = _cross(relative_x, relative_y, axis_x, axis_y) / denominator
        if (
            -_INTERSECTION_TOLERANCE
            <= segment_t
            <= 1.0 + _INTERSECTION_TOLERANCE
        ):
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
        value
        for value in intersections
        if value <= object_min + _INTERSECTION_TOLERANCE
    ]
    upper_candidates = [
        value
        for value in intersections
        if value >= object_max - _INTERSECTION_TOLERANCE
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


def _width_for_axis(
    polygon: tuple[tuple[float, float, float | None], ...],
    center_x: float,
    center_y: float,
    axis_x: float,
    axis_y: float,
) -> float:
    normal_x = -axis_y
    normal_y = axis_x
    normal_projections = [
        ((x - center_x) * normal_x) + ((y - center_y) * normal_y)
        for x, y, _z in polygon
    ]
    if not normal_projections:
        return 0.1
    return max(0.1, max(normal_projections) - min(normal_projections))


def _centerline_from_axis(
    polygon: tuple[tuple[float, float, float | None], ...],
    object_points: list[tuple[float, float]],
    center_x: float,
    center_y: float,
    axis_x: float,
    axis_y: float,
    *,
    use_object_extent: bool,
) -> tuple[
    tuple[float, float, float | None],
    tuple[float, float, float | None],
    float,
]:
    intersections = _line_polygon_intersections(
        center_x,
        center_y,
        axis_x,
        axis_y,
        polygon,
    )
    object_projections = (
        [
            ((x - center_x) * axis_x) + ((y - center_y) * axis_y)
            for x, y in object_points
        ]
        if use_object_extent
        else []
    )
    endpoint_parameters = _endpoint_parameters(intersections, object_projections)

    if endpoint_parameters is None:
        axis_projections = [
            ((x - center_x) * axis_x) + ((y - center_y) * axis_y)
            for x, y, _z in polygon
        ]
        endpoint_parameters = min(axis_projections), max(axis_projections)

    def endpoint(axis_value: float) -> tuple[float, float, float | None]:
        x = center_x + (axis_x * axis_value)
        y = center_y + (axis_y * axis_value)
        z = _marxact._polyline_z_at_xy(x, y, polygon)
        return x, y, z

    return (
        endpoint(endpoint_parameters[0]),
        endpoint(endpoint_parameters[1]),
        _width_for_axis(polygon, center_x, center_y, axis_x, axis_y),
    )


def trench_centerline(
    trench: _marxact.MarXactTrench,
) -> tuple[
    tuple[float, float, float | None],
    tuple[float, float, float | None],
    float,
]:
    """Build a stable block-driven MarXact axis and clip it to the measured boundary."""

    polygon = trench.polygon
    object_points = [(float(item.x), float(item.y)) for item in trench.objects]
    object_axis = _automatic_object_axis(object_points, polygon)

    using_object_axis = object_axis is not None
    center_x, center_y, axis_x, axis_y = object_axis or _axis_from_polygon(polygon)
    try:
        return _centerline_from_axis(
            polygon,
            object_points,
            center_x,
            center_y,
            axis_x,
            axis_y,
            use_object_extent=using_object_axis,
        )
    except (ValueError, ZeroDivisionError):
        center_x, center_y, axis_x, axis_y = _axis_from_polygon(polygon)
        return _centerline_from_axis(
            polygon,
            [],
            center_x,
            center_y,
            axis_x,
            axis_y,
            use_object_extent=False,
        )


def _layer_boundary(
    layer: Any,
) -> tuple[tuple[float, float, float | None], ...] | None:
    payload = getattr(layer, "metadata", {}).get(VIRTUAL_TRENCH_METADATA_KEY)
    raw_boundary = payload.get("boundary_3d") if isinstance(payload, dict) else None
    if not isinstance(raw_boundary, (list, tuple)):
        raw_boundary = getattr(layer, "metadata", {}).get("marxact_boundary_3d")
    if not isinstance(raw_boundary, (list, tuple)):
        return None

    points: list[tuple[float, float, float | None]] = []
    for raw_point in raw_boundary:
        if not isinstance(raw_point, (list, tuple)) or len(raw_point) < 2:
            return None
        try:
            x = float(raw_point[0])
            y = float(raw_point[1])
        except (TypeError, ValueError):
            return None
        z: float | None = None
        if len(raw_point) > 2 and raw_point[2] not in (None, ""):
            try:
                z = float(raw_point[2])
            except (TypeError, ValueError):
                z = None
        points.append((x, y, z))
    if len(points) >= 2 and points[0][:2] == points[-1][:2]:
        points.pop()
    if len(points) < 3:
        return None
    return tuple(points)


def _layer_points(layer: Any) -> tuple[
    dict[str, Any] | None,
    dict[str, Any] | None,
    list[dict[str, Any]],
]:
    payload = getattr(layer, "metadata", {}).get(VIRTUAL_TRENCH_METADATA_KEY)
    points = payload.get("points") if isinstance(payload, dict) else None
    if not isinstance(points, list):
        return None, None, []
    start = next(
        (
            point
            for point in points
            if isinstance(point, dict)
            and str(point.get("role", "")).lower() == "start"
        ),
        None,
    )
    end = next(
        (
            point
            for point in points
            if isinstance(point, dict)
            and str(point.get("role", "")).lower() == "end"
        ),
        None,
    )
    objects = [
        point
        for point in points
        if isinstance(point, dict)
        and str(point.get("role", "")).lower() == "object"
    ]
    return start, end, objects


def _point_xyz(point: dict[str, Any]) -> list[float | None]:
    z: float | None = None
    if point.get("z") not in (None, ""):
        try:
            z = float(point["z"])
        except (TypeError, ValueError):
            z = None
    return [float(point["x"]), float(point["y"]), z]


def _ensure_alignment_baseline(layer: Any) -> bool:
    metadata = getattr(layer, "metadata", None)
    if not isinstance(metadata, dict):
        return False
    start, end, _objects = _layer_points(layer)
    if start is None or end is None:
        return False
    try:
        start_xyz = _point_xyz(start)
        end_xyz = _point_xyz(end)
    except (KeyError, TypeError, ValueError):
        return False

    if MARXACT_ALIGNMENT_AUTO_START_KEY not in metadata:
        metadata[MARXACT_ALIGNMENT_AUTO_START_KEY] = start_xyz
    if MARXACT_ALIGNMENT_AUTO_END_KEY not in metadata:
        metadata[MARXACT_ALIGNMENT_AUTO_END_KEY] = end_xyz
    payload = metadata.get(VIRTUAL_TRENCH_METADATA_KEY)
    if (
        MARXACT_ALIGNMENT_AUTO_WIDTH_KEY not in metadata
        and isinstance(payload, dict)
    ):
        try:
            metadata[MARXACT_ALIGNMENT_AUTO_WIDTH_KEY] = float(
                payload.get("width_meters", 0.6)
            )
        except (TypeError, ValueError):
            metadata[MARXACT_ALIGNMENT_AUTO_WIDTH_KEY] = 0.6
    return True


def alignment_rotation_degrees(layer: Any) -> float:
    try:
        return float(
            getattr(layer, "metadata", {}).get(MARXACT_ALIGNMENT_ROTATION_KEY, 0.0)
            or 0.0
        )
    except (TypeError, ValueError):
        return 0.0


def apply_virtual_layer_alignment_rotation(
    layer: Any,
    rotation_degrees: float,
) -> bool:
    """Rotate only the MarXact alignment axis; keep the measured boundary fixed."""

    polygon = _layer_boundary(layer)
    if polygon is None or not _ensure_alignment_baseline(layer):
        return False

    metadata = layer.metadata
    raw_auto_start = metadata.get(MARXACT_ALIGNMENT_AUTO_START_KEY)
    raw_auto_end = metadata.get(MARXACT_ALIGNMENT_AUTO_END_KEY)
    if (
        not isinstance(raw_auto_start, (list, tuple))
        or len(raw_auto_start) < 2
        or not isinstance(raw_auto_end, (list, tuple))
        or len(raw_auto_end) < 2
    ):
        return False
    try:
        auto_start_x = float(raw_auto_start[0])
        auto_start_y = float(raw_auto_start[1])
        auto_end_x = float(raw_auto_end[0])
        auto_end_y = float(raw_auto_end[1])
    except (TypeError, ValueError):
        return False

    start, end, object_rows = _layer_points(layer)
    if start is None or end is None:
        return False
    object_points: list[tuple[float, float]] = []
    for row in object_rows:
        try:
            object_points.append((float(row["x"]), float(row["y"])))
        except (KeyError, TypeError, ValueError):
            continue

    if object_points:
        center_x = sum(x for x, _y in object_points) / len(object_points)
        center_y = sum(y for _x, y in object_points) / len(object_points)
    else:
        center_x = (auto_start_x + auto_end_x) * 0.5
        center_y = (auto_start_y + auto_end_y) * 0.5

    base_angle = math.atan2(
        auto_end_y - auto_start_y,
        auto_end_x - auto_start_x,
    )
    normalized_rotation = max(-90.0, min(90.0, float(rotation_degrees)))
    angle = base_angle + math.radians(normalized_rotation)
    axis_x = math.cos(angle)
    axis_y = math.sin(angle)

    try:
        new_start, new_end, width = _centerline_from_axis(
            polygon,
            object_points,
            center_x,
            center_y,
            axis_x,
            axis_y,
            use_object_extent=bool(object_points),
        )
    except (ValueError, ZeroDivisionError):
        return False

    for row, point in ((start, new_start), (end, new_end)):
        row["x"] = point[0]
        row["y"] = point[1]
        row["z"] = point[2]

    payload = metadata.get(VIRTUAL_TRENCH_METADATA_KEY)
    if isinstance(payload, dict):
        payload["width_meters"] = width
    metadata[MARXACT_ALIGNMENT_ROTATION_KEY] = normalized_rotation
    return True


def recalculate_virtual_layer_automatic_alignment(layer: Any) -> bool:
    """Re-run the robust automatic MarXact alignment for an already loaded layer."""

    polygon = _layer_boundary(layer)
    if polygon is None:
        return False
    metadata = getattr(layer, "metadata", None)
    if not isinstance(metadata, dict):
        return False

    start, end, object_rows = _layer_points(layer)
    if start is None or end is None:
        return False
    object_points: list[tuple[float, float]] = []
    for row in object_rows:
        try:
            object_points.append((float(row["x"]), float(row["y"])))
        except (KeyError, TypeError, ValueError):
            continue

    object_axis = _automatic_object_axis(object_points, polygon)
    using_object_axis = object_axis is not None
    center_x, center_y, axis_x, axis_y = object_axis or _axis_from_polygon(polygon)
    try:
        new_start, new_end, width = _centerline_from_axis(
            polygon,
            object_points,
            center_x,
            center_y,
            axis_x,
            axis_y,
            use_object_extent=using_object_axis,
        )
    except (ValueError, ZeroDivisionError):
        return False

    for row, point in ((start, new_start), (end, new_end)):
        row["x"] = point[0]
        row["y"] = point[1]
        row["z"] = point[2]

    payload = metadata.get(VIRTUAL_TRENCH_METADATA_KEY)
    if isinstance(payload, dict):
        payload["width_meters"] = width

    metadata[MARXACT_ALIGNMENT_AUTO_START_KEY] = [
        new_start[0],
        new_start[1],
        new_start[2],
    ]
    metadata[MARXACT_ALIGNMENT_AUTO_END_KEY] = [
        new_end[0],
        new_end[1],
        new_end[2],
    ]
    metadata[MARXACT_ALIGNMENT_AUTO_WIDTH_KEY] = width
    metadata[MARXACT_ALIGNMENT_ROTATION_KEY] = 0.0
    return True


def install_marxact_direction_patch() -> None:
    if int(getattr(_marxact, "_marxact_direction_patch_version", 0) or 0) >= PATCH_VERSION:
        return
    _marxact.trench_centerline = trench_centerline
    _marxact.MARXACT_IMPORT_VERSION = max(
        3,
        int(getattr(_marxact, "MARXACT_IMPORT_VERSION", 1) or 1),
    )
    _marxact._marxact_direction_patch_version = PATCH_VERSION
