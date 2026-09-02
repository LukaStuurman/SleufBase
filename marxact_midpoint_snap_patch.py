from __future__ import annotations

import math
from typing import Any

from . import marxact_direction_patch as _direction
from .virtual_trench import VIRTUAL_TRENCH_METADATA_KEY


PATCH_VERSION = 1
_SEGMENT_TOLERANCE_METERS = 1e-6


def _distance_point_to_segment(
    point_x: float,
    point_y: float,
    start_x: float,
    start_y: float,
    end_x: float,
    end_y: float,
) -> float:
    dx = end_x - start_x
    dy = end_y - start_y
    length_sq = (dx * dx) + (dy * dy)
    if length_sq <= 1e-16:
        return math.hypot(point_x - start_x, point_y - start_y)
    fraction = max(
        0.0,
        min(
            1.0,
            (((point_x - start_x) * dx) + ((point_y - start_y) * dy)) / length_sq,
        ),
    )
    nearest_x = start_x + (fraction * dx)
    nearest_y = start_y + (fraction * dy)
    return math.hypot(point_x - nearest_x, point_y - nearest_y)


def _segment_midpoint(
    polygon: tuple[tuple[float, float, float | None], ...],
    segment_index: int,
) -> tuple[float, float, float | None]:
    start_x, start_y, _start_z = polygon[segment_index]
    end_x, end_y, _end_z = polygon[(segment_index + 1) % len(polygon)]
    midpoint_x = (start_x + end_x) * 0.5
    midpoint_y = (start_y + end_y) * 0.5
    # Keep the same height source as the rest of MarXact: the measured 3D
    # POLYLINE. At the segment midpoint this is the exact halfway interpolation
    # of the two surrounding vertices when both heights are available.
    midpoint_z = _direction._marxact._polyline_z_at_xy(
        midpoint_x,
        midpoint_y,
        polygon,
    )
    return midpoint_x, midpoint_y, midpoint_z


def _snap_intersection_to_nearest_segment_midpoint(
    polygon: tuple[tuple[float, float, float | None], ...],
    intersection: tuple[float, float, float | None],
) -> tuple[float, float, float | None]:
    """Use an axis/boundary intersection to select the edge, then use its midpoint."""

    if len(polygon) < 2:
        return intersection
    point_x = float(intersection[0])
    point_y = float(intersection[1])

    candidates: list[tuple[float, float, int]] = []
    for index, (start_x, start_y, _start_z) in enumerate(polygon):
        end_x, end_y, _end_z = polygon[(index + 1) % len(polygon)]
        edge_distance = _distance_point_to_segment(
            point_x,
            point_y,
            start_x,
            start_y,
            end_x,
            end_y,
        )
        midpoint_x = (start_x + end_x) * 0.5
        midpoint_y = (start_y + end_y) * 0.5
        midpoint_distance = math.hypot(point_x - midpoint_x, point_y - midpoint_y)
        candidates.append((edge_distance, midpoint_distance, index))

    if not candidates:
        return intersection
    minimum_edge_distance = min(item[0] for item in candidates)
    # An ordinary intersection lies exactly on one edge. At a polygon vertex two
    # edges can both be valid; in that case choose the closest edge midpoint, as
    # requested, instead of depending on DXF vertex order.
    allowed_distance = max(
        _SEGMENT_TOLERANCE_METERS,
        minimum_edge_distance + _SEGMENT_TOLERANCE_METERS,
    )
    valid = [item for item in candidates if item[0] <= allowed_distance]
    _edge_distance, _midpoint_distance, segment_index = min(
        valid or candidates,
        key=lambda item: (item[1], item[0], item[2]),
    )
    return _segment_midpoint(polygon, segment_index)


def _snap_centerline_result(
    polygon: tuple[tuple[float, float, float | None], ...],
    result: tuple[
        tuple[float, float, float | None],
        tuple[float, float, float | None],
        float,
    ],
) -> tuple[
    tuple[float, float, float | None],
    tuple[float, float, float | None],
    float,
]:
    exact_start, exact_end, original_width = result
    snapped_start = _snap_intersection_to_nearest_segment_midpoint(polygon, exact_start)
    snapped_end = _snap_intersection_to_nearest_segment_midpoint(polygon, exact_end)

    dx = snapped_end[0] - snapped_start[0]
    dy = snapped_end[1] - snapped_start[1]
    length = math.hypot(dx, dy)
    if length <= 1e-8:
        return result

    axis_x = dx / length
    axis_y = dy / length
    center_x = (snapped_start[0] + snapped_end[0]) * 0.5
    center_y = (snapped_start[1] + snapped_end[1]) * 0.5
    try:
        width = _direction._width_for_axis(
            polygon,
            center_x,
            center_y,
            axis_x,
            axis_y,
        )
    except Exception:
        width = original_width
    return snapped_start, snapped_end, width


def _snap_loaded_automatic_alignment(layer: Any) -> bool:
    polygon = _direction._layer_boundary(layer)
    if polygon is None:
        return False
    start, end, _objects = _direction._layer_points(layer)
    if start is None or end is None:
        return False
    try:
        current = (
            (float(start["x"]), float(start["y"]), start.get("z")),
            (float(end["x"]), float(end["y"]), end.get("z")),
            float(
                getattr(layer, "metadata", {})
                .get(VIRTUAL_TRENCH_METADATA_KEY, {})
                .get("width_meters", 0.6)
            ),
        )
    except (KeyError, TypeError, ValueError, AttributeError):
        return False

    snapped_start, snapped_end, width = _snap_centerline_result(polygon, current)
    for row, point in ((start, snapped_start), (end, snapped_end)):
        row["x"] = point[0]
        row["y"] = point[1]
        row["z"] = point[2]

    metadata = getattr(layer, "metadata", None)
    if not isinstance(metadata, dict):
        return False
    payload = metadata.get(VIRTUAL_TRENCH_METADATA_KEY)
    if isinstance(payload, dict):
        payload["width_meters"] = width
    metadata[_direction.MARXACT_ALIGNMENT_AUTO_START_KEY] = list(snapped_start)
    metadata[_direction.MARXACT_ALIGNMENT_AUTO_END_KEY] = list(snapped_end)
    metadata[_direction.MARXACT_ALIGNMENT_AUTO_WIDTH_KEY] = width
    metadata[_direction.MARXACT_ALIGNMENT_ROTATION_KEY] = 0.0
    return True


def install_marxact_midpoint_snap_patch() -> None:
    if int(getattr(_direction, "_marxact_midpoint_snap_patch_version", 0) or 0) >= PATCH_VERSION:
        return

    original_trench_centerline = _direction.trench_centerline
    original_recalculate = _direction.recalculate_virtual_layer_automatic_alignment

    def trench_centerline_midpoint_snap(trench):
        exact_result = original_trench_centerline(trench)
        return _snap_centerline_result(trench.polygon, exact_result)

    def recalculate_midpoint_snap(layer: Any) -> bool:
        if not original_recalculate(layer):
            return False
        return _snap_loaded_automatic_alignment(layer)

    # Initial imports resolve trench_centerline from marxact_import, while loaded
    # layers and the alignment dialog resolve the helpers from direction_patch.
    # Update both paths so the same automatic midpoint rule is used everywhere.
    _direction.trench_centerline = trench_centerline_midpoint_snap
    _direction.recalculate_virtual_layer_automatic_alignment = recalculate_midpoint_snap
    _direction._marxact.trench_centerline = trench_centerline_midpoint_snap

    # marxact_alignment_ui_patch imports the recalculate function by value. If it
    # is already loaded, update that alias as well; if it is not loaded this import
    # makes the binding deterministic before its UI hook is installed.
    try:
        from . import marxact_alignment_ui_patch as alignment_ui

        alignment_ui.recalculate_virtual_layer_automatic_alignment = recalculate_midpoint_snap
    except Exception:
        pass

    _direction._marxact_midpoint_snap_patch_version = PATCH_VERSION
