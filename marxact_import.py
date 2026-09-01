from __future__ import annotations

import math
import re
from dataclasses import dataclass, field
from pathlib import Path
from typing import Callable, Iterable

import ezdxf
from PIL import Image

from .models import Bounds, GeoTiffLayer, GeoTransform
from .virtual_trench import VIRTUAL_TRENCH_METADATA_KEY, build_virtual_trench_render


LEADER_MATCH_TOLERANCE = 0.08
BOUNDARY_MIN_AREA = 0.02
MARXACT_SOURCE_KEY = "marxact_source_path"
MARXACT_TRENCH_NAME_KEY = "marxact_trench_name"
MARXACT_BOUNDARY_KEY = "marxact_boundary_3d"
MARXACT_OBJECT_LAYER_KEY = "marxact_layer_name"
MARXACT_OBJECT_NAME_KEY = "marxact_name"
MARXACT_IMPORT_VERSION_KEY = "marxact_import_version"
MARXACT_IMPORT_VERSION = 1


class MarXactImportError(RuntimeError):
    """Raised when a MarXact DXF cannot be interpreted as proefsleuven."""


@dataclass(frozen=True)
class MarXactLeader:
    x: float
    y: float
    name: str
    height: float | None
    raw_content: str
    layer_name: str


@dataclass(frozen=True)
class MarXactObject:
    x: float
    y: float
    z: float | None
    layer_name: str
    name: str
    height: float | None
    block_name: str

    @property
    def mapping_name(self) -> str:
        """Name used to map MarXact data to the existing Kabel/Leiding choices."""

        layer_name = self.layer_name.strip()
        if not layer_name or layer_name == "0":
            return self.name.strip()
        return layer_name


@dataclass
class MarXactTrench:
    name: str
    polygon: tuple[tuple[float, float, float | None], ...]
    objects: list[MarXactObject] = field(default_factory=list)

    @property
    def area(self) -> float:
        return abs(_signed_area([(x, y) for x, y, _z in self.polygon]))


@dataclass(frozen=True)
class MarXactParseResult:
    source_path: Path
    trenches: tuple[MarXactTrench, ...]
    leader_count: int
    insert_count: int
    assigned_insert_count: int

    @property
    def mapping_names(self) -> tuple[str, ...]:
        names: list[str] = []
        seen: set[str] = set()
        for trench in self.trenches:
            for item in trench.objects:
                name = item.mapping_name.strip()
                normalized = normalize_marxact_name(name)
                if not normalized or normalized in seen:
                    continue
                seen.add(normalized)
                names.append(name)
        return tuple(names)


def normalize_marxact_name(value: object) -> str:
    return " ".join(str(value or "").split()).casefold()


def parse_marxact_leader_content(content: str) -> tuple[str, float | None]:
    text = str(content or "").replace("\\P", "^J")
    name_match = re.search(
        r"(?:^|\^J|\r?\n)\s*Name\s*=\s*([^\^\r\n]+)",
        text,
        flags=re.IGNORECASE,
    )
    height_match = re.search(
        r"(?:^|\^J|\r?\n)\s*Height\s*=\s*([-+]?\d+(?:[.,]\d+)?)",
        text,
        flags=re.IGNORECASE,
    )
    name = name_match.group(1).strip() if name_match else ""
    height: float | None = None
    if height_match:
        try:
            height = float(height_match.group(1).replace(",", "."))
        except ValueError:
            height = None
    return name, height


def _mleader_content(entity) -> str:
    try:
        mtext = entity.context.mtext
        if mtext is None:
            return ""
        return str(getattr(mtext, "default_content", "") or "")
    except Exception:
        return ""


def _mleader_tip(entity) -> tuple[float, float] | None:
    """Return the arrow end; MarXact places that point exactly on the measured object."""

    try:
        for leader in entity.context.leaders:
            for line in leader.lines:
                vertices = list(getattr(line, "vertices", ()) or ())
                if vertices:
                    return float(vertices[0].x), float(vertices[0].y)
    except Exception:
        return None
    return None


def _collect_leaders(modelspace) -> list[MarXactLeader]:
    result: list[MarXactLeader] = []
    for entity in modelspace:
        if entity.dxftype() != "MULTILEADER":
            continue
        tip = _mleader_tip(entity)
        if tip is None:
            continue
        raw_content = _mleader_content(entity)
        name, height = parse_marxact_leader_content(raw_content)
        result.append(
            MarXactLeader(
                x=tip[0],
                y=tip[1],
                name=name,
                height=height,
                raw_content=raw_content,
                layer_name=str(entity.dxf.layer or "0"),
            )
        )
    return result


def _leader_lookup(leaders: Iterable[MarXactLeader]):
    scale = 1.0 / LEADER_MATCH_TOLERANCE
    buckets: dict[tuple[int, int], list[MarXactLeader]] = {}
    for leader in leaders:
        key = (round(leader.x * scale), round(leader.y * scale))
        buckets.setdefault(key, []).append(leader)

    def nearest(x: float, y: float) -> MarXactLeader | None:
        base_x = round(float(x) * scale)
        base_y = round(float(y) * scale)
        best: MarXactLeader | None = None
        best_distance_sq = LEADER_MATCH_TOLERANCE * LEADER_MATCH_TOLERANCE
        for dx in (-1, 0, 1):
            for dy in (-1, 0, 1):
                for candidate in buckets.get((base_x + dx, base_y + dy), ()):
                    distance_sq = (candidate.x - x) ** 2 + (candidate.y - y) ** 2
                    if distance_sq <= best_distance_sq:
                        best = candidate
                        best_distance_sq = distance_sq
        return best

    return nearest


def _signed_area(points: list[tuple[float, float]]) -> float:
    if len(points) < 3:
        return 0.0
    return 0.5 * sum(
        points[index][0] * points[(index + 1) % len(points)][1]
        - points[(index + 1) % len(points)][0] * points[index][1]
        for index in range(len(points))
    )


def _distance_point_segment(
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
    projection = max(
        0.0,
        min(
            1.0,
            ((point_x - start_x) * dx + (point_y - start_y) * dy) / length_sq,
        ),
    )
    nearest_x = start_x + (projection * dx)
    nearest_y = start_y + (projection * dy)
    return math.hypot(point_x - nearest_x, point_y - nearest_y)


def _point_in_polygon(
    x: float,
    y: float,
    polygon: tuple[tuple[float, float, float | None], ...],
) -> bool:
    points = [(point[0], point[1]) for point in polygon]
    if len(points) < 3:
        return False

    for index, (start_x, start_y) in enumerate(points):
        end_x, end_y = points[(index + 1) % len(points)]
        if (
            _distance_point_segment(x, y, start_x, start_y, end_x, end_y)
            <= LEADER_MATCH_TOLERANCE
        ):
            return True

    inside = False
    previous_index = len(points) - 1
    for index, (current_x, current_y) in enumerate(points):
        previous_x, previous_y = points[previous_index]
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


def parse_marxact_dxf(path: str | Path) -> MarXactParseResult:
    source_path = Path(path)
    try:
        document = ezdxf.readfile(source_path)
    except Exception as exc:
        raise MarXactImportError(f"MarXact-DXF kon niet worden gelezen: {exc}") from exc

    modelspace = document.modelspace()
    leaders = _collect_leaders(modelspace)
    if not leaders:
        raise MarXactImportError("Geen MarXact MULTILEADER-labels gevonden.")
    nearest_leader = _leader_lookup(leaders)

    trenches: list[MarXactTrench] = []
    for entity in modelspace:
        if entity.dxftype() != "POLYLINE":
            continue
        try:
            if not entity.is_closed:
                continue
            raw_vertices = list(entity.vertices)
        except Exception:
            continue
        if len(raw_vertices) < 3:
            continue

        vertices: list[tuple[float, float, float | None]] = []
        vertex_names: list[str] = []
        for vertex in raw_vertices:
            location = vertex.dxf.location
            x = float(location.x)
            y = float(location.y)
            try:
                z = float(location.z)
            except (TypeError, ValueError):
                z = None
            vertices.append((x, y, z))
            leader = nearest_leader(x, y)
            vertex_names.append(leader.name.strip() if leader is not None else "")

        if abs(_signed_area([(x, y) for x, y, _z in vertices])) < BOUNDARY_MIN_AREA:
            continue
        normalized_names = [
            normalize_marxact_name(name) for name in vertex_names if name.strip()
        ]
        if len(normalized_names) != len(vertices) or len(set(normalized_names)) != 1:
            continue
        trenches.append(MarXactTrench(name=vertex_names[0].strip(), polygon=tuple(vertices)))

    if not trenches:
        raise MarXactImportError(
            "Geen MarXact-proefsleufgrenzen gevonden. Verwacht gesloten 3D-polylines "
            "met op iedere vertex een MULTILEADER met dezelfde Name-waarde."
        )

    insert_count = 0
    assigned_insert_count = 0
    for entity in modelspace:
        if entity.dxftype() != "INSERT":
            continue
        insert_count += 1
        insertion = entity.dxf.insert
        x = float(insertion.x)
        y = float(insertion.y)
        containing = [
            trench for trench in trenches if _point_in_polygon(x, y, trench.polygon)
        ]
        if not containing:
            continue
        trench = min(containing, key=lambda candidate: candidate.area)
        leader = nearest_leader(x, y)
        layer_name = str(entity.dxf.layer or "0").strip() or "0"
        name = leader.name.strip() if leader is not None else ""
        height = leader.height if leader is not None else None
        z = height
        if z is None:
            try:
                z = float(insertion.z)
            except (TypeError, ValueError):
                z = None
        trench.objects.append(
            MarXactObject(
                x=x,
                y=y,
                z=z,
                layer_name=layer_name,
                name=name,
                height=height,
                block_name=str(entity.dxf.name or ""),
            )
        )
        assigned_insert_count += 1

    return MarXactParseResult(
        source_path=source_path,
        trenches=tuple(trenches),
        leader_count=len(leaders),
        insert_count=insert_count,
        assigned_insert_count=assigned_insert_count,
    )


def trench_centerline(
    trench: MarXactTrench,
) -> tuple[
    tuple[float, float, float | None],
    tuple[float, float, float | None],
    float,
]:
    """Approximate a measured boundary by its long PCA axis and actual width."""

    points = list(trench.polygon)
    center_x = sum(point[0] for point in points) / len(points)
    center_y = sum(point[1] for point in points) / len(points)
    covariance_xx = sum((point[0] - center_x) ** 2 for point in points) / len(points)
    covariance_yy = sum((point[1] - center_y) ** 2 for point in points) / len(points)
    covariance_xy = sum(
        (point[0] - center_x) * (point[1] - center_y) for point in points
    ) / len(points)

    angle = 0.5 * math.atan2(2.0 * covariance_xy, covariance_xx - covariance_yy)
    axis_x = math.cos(angle)
    axis_y = math.sin(angle)
    normal_x = -axis_y
    normal_y = axis_x

    projections: list[tuple[float, float, float | None]] = []
    for x, y, z in points:
        relative_x = x - center_x
        relative_y = y - center_y
        projections.append(
            (
                (relative_x * axis_x) + (relative_y * axis_y),
                (relative_x * normal_x) + (relative_y * normal_y),
                z,
            )
        )

    axis_min = min(item[0] for item in projections)
    axis_max = max(item[0] for item in projections)
    normal_min = min(item[1] for item in projections)
    normal_max = max(item[1] for item in projections)
    normal_middle = (normal_min + normal_max) * 0.5
    width = max(0.1, normal_max - normal_min)
    end_tolerance = max(0.01, (axis_max - axis_min) * 0.08)

    def endpoint(axis_value: float, at_start: bool):
        z_values = [
            z
            for projected_axis, _projected_normal, z in projections
            if z is not None
            and (
                projected_axis <= axis_min + end_tolerance
                if at_start
                else projected_axis >= axis_max - end_tolerance
            )
        ]
        z = (sum(z_values) / len(z_values)) if z_values else None
        return (
            center_x + (axis_x * axis_value) + (normal_x * normal_middle),
            center_y + (axis_y * axis_value) + (normal_y * normal_middle),
            z,
        )

    return endpoint(axis_min, True), endpoint(axis_max, False), width


def _safe_virtual_name(name: str, fallback_index: int, occurrence: int) -> str:
    cleaned = re.sub(r"[^0-9A-Za-zÀ-ÿ._ -]+", "_", str(name or "").strip()).strip(" ._")
    if not cleaned:
        cleaned = f"PS{fallback_index}"
    if occurrence > 1:
        cleaned = f"{cleaned}_{occurrence}"
    return cleaned


def _ps_number(name: str) -> str:
    match = re.search(r"(?i)\bPS\s*(\d+)\b", str(name or ""))
    return match.group(1) if match else ""


def build_marxact_virtual_layer(
    trench: MarXactTrench,
    *,
    source_path: str | Path,
    source_name_resolver: Callable[[MarXactObject], str | None],
    fallback_index: int,
    occurrence: int = 1,
    epsg: int = 28992,
) -> GeoTiffLayer:
    start, end, width = trench_centerline(trench)
    payload_points: list[dict[str, object]] = [
        {
            "role": "start",
            "object_name": "Beginpunt",
            "source_name": "PUNT",
            "x": start[0],
            "y": start[1],
            "z": start[2],
        }
    ]

    for item in trench.objects:
        resolved_source_name = source_name_resolver(item)
        if not resolved_source_name:
            continue
        payload_points.append(
            {
                "role": "object",
                "object_name": item.name.strip() or item.layer_name.strip() or "Object",
                "source_name": resolved_source_name,
                "x": item.x,
                "y": item.y,
                "z": item.z,
                "attribute_1": item.name,
                "attribute_2": item.layer_name,
                "attribute_3": item.block_name,
                MARXACT_OBJECT_LAYER_KEY: item.layer_name,
                MARXACT_OBJECT_NAME_KEY: item.name,
            }
        )

    payload_points.append(
        {
            "role": "end",
            "object_name": "Eindpunt",
            "source_name": "PUNT",
            "x": end[0],
            "y": end[1],
            "z": end[2],
        }
    )

    source = Path(source_path)
    display_name = _safe_virtual_name(trench.name, fallback_index, occurrence)
    virtual_path = source.with_name(f"{display_name}.marxact-virtual.tif")
    polygon_xy = [(point[0], point[1]) for point in trench.polygon]
    min_x = min(point[0] for point in polygon_xy)
    max_x = max(point[0] for point in polygon_xy)
    min_y = min(point[1] for point in polygon_xy)
    max_y = max(point[1] for point in polygon_xy)
    initial_bounds = Bounds(min_x, min_y, max_x, max_y).padded(max(1.0, width))
    initial_image = Image.new("RGBA", (4, 4), (0, 0, 0, 0))
    initial_transform = GeoTransform(
        initial_bounds.width / 4.0,
        0.0,
        initial_bounds.min_x,
        0.0,
        -(initial_bounds.height / 4.0),
        initial_bounds.max_y,
    )
    metadata: dict[str, object] = {
        VIRTUAL_TRENCH_METADATA_KEY: {
            "width_meters": width,
            "points": payload_points,
            "source": "marxact",
        },
        MARXACT_SOURCE_KEY: str(source),
        MARXACT_TRENCH_NAME_KEY: trench.name,
        MARXACT_BOUNDARY_KEY: [list(point) for point in trench.polygon],
        MARXACT_IMPORT_VERSION_KEY: MARXACT_IMPORT_VERSION,
        "template_proefsleuf_label": trench.name,
    }
    ps_number = _ps_number(trench.name)
    if ps_number:
        metadata["template_export_ps_number"] = ps_number

    layer = GeoTiffLayer(
        path=virtual_path,
        image=initial_image,
        transform=initial_transform,
        bounds=initial_bounds,
        epsg=epsg,
        opacity=1.0,
        metadata=metadata,
    )
    image, bounds, transform = build_virtual_trench_render(layer)
    layer.image = image
    layer.bounds = bounds
    layer.transform = transform
    return layer
