from __future__ import annotations

from math import isfinite
from typing import Any


PATCH_VERSION = 1
VIRTUAL_TRENCH_BOUNDARY_3D_KEY = "boundary_3d"
LEGACY_MARXACT_BOUNDARY_3D_KEY = "marxact_boundary_3d"


def _normalized_boundary_3d(raw_boundary: object) -> list[tuple[float, float, float | None]]:
    if not isinstance(raw_boundary, (list, tuple)):
        return []

    points: list[tuple[float, float, float | None]] = []
    for raw_point in raw_boundary:
        if not isinstance(raw_point, (list, tuple)) or len(raw_point) < 2:
            return []
        try:
            x = float(raw_point[0])
            y = float(raw_point[1])
        except (TypeError, ValueError):
            return []
        if not isfinite(x) or not isfinite(y):
            return []

        z: float | None = None
        if len(raw_point) >= 3 and raw_point[2] is not None:
            try:
                candidate_z = float(raw_point[2])
            except (TypeError, ValueError):
                candidate_z = None
            if candidate_z is not None and isfinite(candidate_z):
                z = candidate_z
        points.append((x, y, z))

    # A repeated closing vertex is harmless in DXF, but keeping only one copy
    # avoids drawing a zero-length final segment before the renderer closes it.
    if len(points) >= 2 and points[0][:2] == points[-1][:2]:
        points.pop()
    if len(points) < 3:
        return []

    distinct_xy = {(round(point[0], 9), round(point[1], 9)) for point in points}
    if len(distinct_xy) < 3:
        return []

    signed_double_area = sum(
        points[index][0] * points[(index + 1) % len(points)][1]
        - points[(index + 1) % len(points)][0] * points[index][1]
        for index in range(len(points))
    )
    if abs(signed_double_area) <= 1e-9:
        return []
    return points


def install_marxact_boundary_patch() -> None:
    from . import cadastral_export as cadastral_export_module
    from . import virtual_trench as virtual_trench_module

    exporter_cls = cadastral_export_module.CadastralDxfExporter
    if int(getattr(exporter_cls, "_sleufbase_marxact_boundary_patch_version", 0) or 0) >= PATCH_VERSION:
        return

    original_virtual_trench_polygon = virtual_trench_module.virtual_trench_polygon

    def virtual_trench_boundary_3d(layer) -> list[tuple[float, float, float | None]]:
        payload = virtual_trench_module.virtual_trench_payload(layer)
        generic_boundary = None
        if isinstance(payload, dict):
            generic_boundary = payload.get(VIRTUAL_TRENCH_BOUNDARY_3D_KEY)
        normalized = _normalized_boundary_3d(generic_boundary)
        if normalized:
            return normalized

        # MarXact already preserved the original measured 3D POLYLINE at layer
        # level. Promote that legacy field into the generic virtual-trench payload
        # so every renderer/exporter can use the exact same measured contour.
        legacy_boundary = layer.metadata.get(LEGACY_MARXACT_BOUNDARY_3D_KEY)
        normalized = _normalized_boundary_3d(legacy_boundary)
        if normalized and isinstance(payload, dict):
            payload[VIRTUAL_TRENCH_BOUNDARY_3D_KEY] = [
                [x, y, z] for x, y, z in normalized
            ]
        return normalized

    def measured_virtual_trench_polygon(layer) -> list[tuple[float, float]]:
        boundary = virtual_trench_boundary_3d(layer)
        if boundary:
            return [(x, y) for x, y, _z in boundary]
        return original_virtual_trench_polygon(layer)

    # build_virtual_trench_render resolves this global at runtime, so replacing it
    # here updates the normal application view immediately after MarXact import.
    virtual_trench_module.VIRTUAL_TRENCH_BOUNDARY_3D_KEY = VIRTUAL_TRENCH_BOUNDARY_3D_KEY
    virtual_trench_module.virtual_trench_boundary_3d = virtual_trench_boundary_3d
    virtual_trench_module.virtual_trench_polygon = measured_virtual_trench_polygon

    # cadastral_export imports virtual_trench_polygon by value, so update that
    # module alias too. The DXF template overview/slot then uses the identical
    # measured MarXact outline rather than a generated rectangle.
    cadastral_export_module.virtual_trench_polygon = measured_virtual_trench_polygon

    exporter_cls._sleufbase_marxact_boundary_patch_version = PATCH_VERSION
