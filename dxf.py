from __future__ import annotations

from pathlib import Path

import ezdxf
from ezdxf import colors

from .models import Bounds, CableFeature, DxfOverlay


class DxfError(RuntimeError):
    """Raised when the DXF file cannot be parsed."""


def _points_bounds(points: list[tuple[float, float]]) -> Bounds:
    xs = [point[0] for point in points]
    ys = [point[1] for point in points]
    return Bounds(min(xs), min(ys), max(xs), max(ys))


def _resolve_entity_color(entity, document) -> tuple[int, int, int]:
    if entity.dxf.hasattr("true_color") and entity.dxf.true_color:
        value = int(entity.dxf.true_color)
        return ((value >> 16) & 255, (value >> 8) & 255, value & 255)
    aci = int(entity.dxf.color) if entity.dxf.hasattr("color") else 256
    if aci in {0, 256}:
        try:
            aci = int(document.layers.get(entity.dxf.layer).color)
        except Exception:
            aci = 1
    red, green, blue = colors.aci2rgb(max(1, min(255, aci)))
    if (red, green, blue) == (255, 255, 255):
        return 220, 20, 60
    return int(red), int(green), int(blue)


def _format_xdata(entity) -> dict[str, str]:
    if not getattr(entity, "xdata", None):
        return {}
    details: dict[str, str] = {}
    for appid, tags in entity.xdata.data.items():
        values = []
        for tag in tags:
            if tag.code == 1001:
                continue
            values.append(str(tag.value))
        if values:
            details[f"XDATA {appid}"] = ", ".join(values)
    return details


def _format_insert_attributes(entity) -> dict[str, str]:
    if entity.dxftype() != "INSERT":
        return {}
    details: dict[str, str] = {}
    for attrib in entity.attribs:
        details[f"Attribuut {attrib.dxf.tag}"] = str(attrib.dxf.text)
    return details


def _entity_metadata(entity, path: Path) -> dict[str, str]:
    metadata = {
        "Bestand": path.name,
        "Type": entity.dxftype(),
        "Handle": entity.dxf.handle,
        "Laag": entity.dxf.layer,
    }
    if entity.dxf.hasattr("linetype"):
        metadata["Lijntype"] = str(entity.dxf.linetype)
    if entity.dxf.hasattr("lineweight"):
        metadata["Lijndikte"] = str(entity.dxf.lineweight)
    metadata.update(_format_xdata(entity))
    metadata.update(_format_insert_attributes(entity))
    return metadata


def _polyline_from_entity(entity) -> list[list[tuple[float, float]]]:
    entity_type = entity.dxftype()
    if entity_type == "LINE":
        start = entity.dxf.start
        end = entity.dxf.end
        return [[(float(start.x), float(start.y)), (float(end.x), float(end.y))]]
    if entity_type == "LWPOLYLINE":
        points = [(float(x), float(y)) for x, y in entity.get_points("xy")]
        if entity.closed and points:
            points.append(points[0])
        return [points]
    if entity_type == "POLYLINE":
        points = [(float(vertex.dxf.location.x), float(vertex.dxf.location.y)) for vertex in entity.vertices]
        if entity.is_closed and points:
            points.append(points[0])
        return [points]
    if entity_type in {"ARC", "CIRCLE", "ELLIPSE", "SPLINE"}:
        points = [(float(point.x), float(point.y)) for point in entity.flattening(0.25)]
        return [points]
    if entity_type == "INSERT":
        insert = entity.dxf.insert
        return [[(float(insert.x), float(insert.y)), (float(insert.x), float(insert.y))]]
    return []


def load_dxf(path: str | Path) -> DxfOverlay:
    file_path = Path(path)
    try:
        document = ezdxf.readfile(file_path)
    except Exception as exc:  # pragma: no cover - ezdxf exceptions differ per file state.
        raise DxfError(f"DXF kon niet worden gelezen: {exc}") from exc

    features: list[CableFeature] = []
    for entity in document.modelspace():
        point_sets = _polyline_from_entity(entity)
        if not point_sets:
            continue
        color = _resolve_entity_color(entity, document)
        base_metadata = _entity_metadata(entity, file_path)
        for index, points in enumerate(point_sets):
            if len(points) < 2:
                continue
            bounds = _points_bounds(points)
            feature_id = f"{entity.dxf.handle}:{index}"
            metadata = dict(base_metadata)
            metadata["Punten"] = str(len(points))
            features.append(
                CableFeature(
                    feature_id=feature_id,
                    source_path=file_path,
                    points=points,
                    bounds=bounds,
                    color=color,
                    metadata=metadata,
                )
            )
    return DxfOverlay(path=file_path, features=features)

