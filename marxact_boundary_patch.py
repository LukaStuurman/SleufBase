from __future__ import annotations

import sys
from math import isfinite
from typing import Any

from PIL import Image, ImageChops, ImageDraw


PATCH_VERSION = 2
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


def _clip_render_to_measured_boundary(
    image: Image.Image,
    bounds,
    boundary: list[tuple[float, float, float | None]],
    *,
    border_rgba: tuple[int, int, int, int],
    border_width: int,
) -> Image.Image:
    if image.mode != "RGBA" or image.width < 2 or image.height < 2 or len(boundary) < 3:
        return image

    def to_pixel(x: float, y: float) -> tuple[float, float]:
        px = ((float(x) - bounds.min_x) / max(bounds.width, 1e-9)) * (image.width - 1)
        py = ((bounds.max_y - float(y)) / max(bounds.height, 1e-9)) * (image.height - 1)
        return px, py

    polygon_pixels = [to_pixel(x, y) for x, y, _z in boundary]
    mask = Image.new("L", image.size, 0)
    try:
        mask_draw = ImageDraw.Draw(mask)
        mask_draw.polygon(polygon_pixels, fill=255)
        alpha = image.getchannel("A")
        clipped_alpha = ImageChops.multiply(alpha, mask)
        image.putalpha(clipped_alpha)
        alpha.close()
        clipped_alpha.close()

        # Redraw only the measured black boundary after clipping. Cable/pipe
        # colours can therefore never protrude past the real 3D-POLYLINE while
        # the contour itself stays crisp and fully visible.
        draw = ImageDraw.Draw(image, "RGBA")
        draw.line(
            [*polygon_pixels, polygon_pixels[0]],
            fill=border_rgba,
            width=max(1, int(border_width)),
        )
    finally:
        mask.close()
    return image


def install_marxact_boundary_patch() -> None:
    from . import cadastral_export as cadastral_export_module
    from . import marxact_import as marxact_import_module
    from . import virtual_trench as virtual_trench_module

    exporter_cls = cadastral_export_module.CadastralDxfExporter
    if int(getattr(exporter_cls, "_sleufbase_marxact_boundary_patch_version", 0) or 0) >= PATCH_VERSION:
        return

    original_virtual_trench_polygon = virtual_trench_module.virtual_trench_polygon
    original_build_virtual_trench_render = virtual_trench_module.build_virtual_trench_render

    def virtual_trench_boundary_3d(layer) -> list[tuple[float, float, float | None]]:
        payload = virtual_trench_module.virtual_trench_payload(layer)
        generic_boundary = None
        if isinstance(payload, dict):
            generic_boundary = payload.get(VIRTUAL_TRENCH_BOUNDARY_3D_KEY)
        normalized = _normalized_boundary_3d(generic_boundary)
        if normalized:
            return normalized

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

    def clipped_virtual_trench_render(layer, *, quality_multiplier: float = 1.0):
        image, bounds, transform = original_build_virtual_trench_render(
            layer,
            quality_multiplier=quality_multiplier,
        )
        boundary = virtual_trench_boundary_3d(layer)
        if not boundary:
            return image, bounds, transform
        border_width = max(2, int(round(2.0 * max(0.1, float(quality_multiplier)))))
        _clip_render_to_measured_boundary(
            image,
            bounds,
            boundary,
            border_rgba=virtual_trench_module.VIRTUAL_TRENCH_BORDER_RGBA,
            border_width=border_width,
        )
        return image, bounds, transform

    virtual_trench_module.VIRTUAL_TRENCH_BOUNDARY_3D_KEY = VIRTUAL_TRENCH_BOUNDARY_3D_KEY
    virtual_trench_module.virtual_trench_boundary_3d = virtual_trench_boundary_3d
    virtual_trench_module.virtual_trench_polygon = measured_virtual_trench_polygon
    virtual_trench_module.build_virtual_trench_render = clipped_virtual_trench_render

    cadastral_export_module.virtual_trench_polygon = measured_virtual_trench_polygon
    cadastral_export_module.build_virtual_trench_render = clipped_virtual_trench_render
    marxact_import_module.build_virtual_trench_render = clipped_virtual_trench_render

    # If the runtime import patch has already been imported, it may hold the old
    # renderer by value. Update that alias without forcing app.py to load early.
    import_patch = sys.modules.get("SleufBase.marxact_import_patch")
    if import_patch is not None:
        setattr(import_patch, "build_virtual_trench_render", clipped_virtual_trench_render)

    exporter_cls._sleufbase_marxact_boundary_patch_version = PATCH_VERSION
