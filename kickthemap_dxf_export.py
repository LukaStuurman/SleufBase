from __future__ import annotations

import json
import math
import re
from dataclasses import dataclass
from fnmatch import fnmatchcase
from pathlib import Path

import ezdxf
from ezdxf.colors import rgb2int
from ezdxf.enums import TextEntityAlignment

from .models import Bounds


DEFAULT_POINT_HALF_LENGTH = 0.5
APP_ID = "KTM_DXF_EXPORT"
DEFAULT_OBJECT_LAYER_RULES: tuple[tuple[str, str, int, str], ...] = (
    ("water", "N-OI-KL-WATER_DISTRIBUTIELEIDING_PVCBV_110-G", 5, "Waterleiding"),
    ("data", "N-OI-KL-DATA-G", 3, "Datakabel"),
    ("glasvezel,gv", "N-OI-KL-DATA_GLASVEZEL-G", 100, "Glasvezel"),
    ("ld", "N-OI-KL-GAS_LD-G", 50, "Gasleiding LD"),
    ("hd", "N-OI-KL-GAS_HD-G", 30, "Gasleiding HD"),
    ("ls", "N-OI-KL-ET_LS-G", 190, "Laagspanning"),
    ("ms", "N-OI-KL-ET_MS-G", 4, "Middenspanning"),
    ("monitoring,x", "N-OI-KL-MONITORING_KABEL-G", 1, "Monitoring"),
    ("rio,druk", "N-OI-RI-DRUK-G", 210, "Riool druk"),
    ("verval,vrijverval", "N-OI-RI-VRIJVERVAL-G", 210, "Riool vrijverval"),
    ("gevaarlijke inhoud,gs", "N-OI-KL-BUISLEIDING GEVAARLIJKE INHOUD-G", 20, "Buisleiding GS"),
)


class KickTheMapDxfExportError(RuntimeError):
    """Raised when de KickTheMap DXF-export mislukt."""


@dataclass(frozen=True)
class ObjectLayerRule:
    keywords: tuple[str, ...]
    target_layer: str
    color: int
    profile_label: str = ""

    def matches(self, source_name: str | None) -> bool:
        if not source_name:
            return False
        normalized_source = source_name.strip().upper()
        if not normalized_source:
            return False
        for keyword in self.keywords:
            normalized_keyword = keyword.strip().upper()
            if not normalized_keyword:
                continue
            if "*" in normalized_keyword or "?" in normalized_keyword:
                if fnmatchcase(normalized_source, normalized_keyword):
                    return True
                continue
            if normalized_keyword in normalized_source:
                return True
        return False


@dataclass(frozen=True)
class KickTheMapObjectPoint:
    object_name: str
    source_name: str
    x: float
    y: float
    z: float | None = None
    attribute_1: str = ""
    attribute_2: str = ""
    attribute_3: str = ""


@dataclass(frozen=True)
class KickTheMapPolylineVertex:
    x: float
    y: float
    z: float | None = None


@dataclass(frozen=True)
class KickTheMapObjectPolyline:
    object_name: str
    source_name: str
    vertices: tuple[KickTheMapPolylineVertex, ...]
    attribute_1: str = ""
    attribute_2: str = ""
    attribute_3: str = ""


KickTheMapObjectFeature = KickTheMapObjectPoint | KickTheMapObjectPolyline


@dataclass(frozen=True)
class KickTheMapObjectDataset:
    job_id: int
    job_title: str
    source_path: Path
    points: tuple[KickTheMapObjectPoint, ...]
    polylines: tuple[KickTheMapObjectPolyline, ...] = ()
    cross_section_start_xy: tuple[float, float] | None = None


def build_object_layer_rules(rule_rows: list[tuple] | None = None) -> tuple[ObjectLayerRule, ...]:
    source_rows = rule_rows if rule_rows is not None else list(DEFAULT_OBJECT_LAYER_RULES)
    rules: list[ObjectLayerRule] = []
    for row in source_rows:
        if len(row) >= 4:
            keywords_text, target_layer, color, profile_label = row[:4]
        elif len(row) == 3:
            keywords_text, target_layer, color = row
            profile_label = ""
        else:
            continue
        keywords = tuple(
            keyword.strip()
            for keyword in str(keywords_text).replace(";", ",").replace("\n", ",").split(",")
            if keyword.strip()
        )
        layer_name = str(target_layer).strip()
        profile_text = str(profile_label).strip()
        if not keywords or not layer_name:
            continue
        try:
            normalized_color = int(color)
        except (TypeError, ValueError):
            normalized_color = 256
        normalized_color = max(1, min(256, normalized_color))
        rules.append(
            ObjectLayerRule(
                keywords=keywords,
                target_layer=layer_name,
                color=normalized_color,
                profile_label=profile_text,
            )
        )
    return tuple(rules)


def load_job_feature_dataset(path: str | Path, job_id: int, job_title: str) -> KickTheMapObjectDataset:
    source_path = Path(path)
    try:
        text = source_path.read_text(encoding="utf-8", errors="ignore")
    except OSError as exc:
        raise KickTheMapDxfExportError(f"Objectbestand kon niet worden gelezen: {exc}") from exc
    points, polylines = parse_job_features_dataset_text(text)
    return KickTheMapObjectDataset(
        job_id=job_id,
        job_title=job_title,
        source_path=source_path,
        points=tuple(points),
        polylines=tuple(polylines),
    )


def parse_job_features_text(text: str) -> list[KickTheMapObjectPoint]:
    points, _polylines = parse_job_features_dataset_text(text)
    return points


def parse_job_features_dataset_text(
    text: str,
) -> tuple[list[KickTheMapObjectPoint], list[KickTheMapObjectPolyline]]:
    try:
        payload = json.loads(text)
    except json.JSONDecodeError as exc:
        raise KickTheMapDxfExportError(f"jobFeatures.json heeft geen geldig JSON-formaat: {exc}") from exc

    raw_features = payload.get("features")
    if not isinstance(raw_features, list):
        raise KickTheMapDxfExportError("jobFeatures.json bevat geen features-lijst.")

    points: list[KickTheMapObjectPoint] = []
    polylines: list[KickTheMapObjectPolyline] = []
    for index, feature in enumerate(raw_features, start=1):
        if not isinstance(feature, dict):
            continue

        geometry = feature.get("geometry")
        geometry_type = str((geometry or {}).get("type", "")).strip().lower()
        if not isinstance(geometry, dict) or geometry_type not in {"point", "polyline", "linestring"}:
            continue

        properties = feature.get("properties")
        if not isinstance(properties, dict):
            properties = {}
        if str(properties.get("groupType", "")).strip().lower() not in {"", "objects"}:
            continue

        source_name = str(properties.get("idName", "") or properties.get("name", "")).strip()
        object_name = source_name or f"Object {index}"
        attribute_1 = str(properties.get("properties1", "") or "").strip()
        attribute_2 = str(properties.get("properties2", "") or "").strip()
        attribute_3 = str(properties.get("properties3", "") or "").strip()
        coordinates = geometry.get("coordinates")
        if geometry_type == "point":
            if not isinstance(coordinates, list) or len(coordinates) < 2:
                continue
            try:
                x = float(coordinates[0])
                y = float(coordinates[1])
            except (TypeError, ValueError):
                continue

            z = None
            if len(coordinates) > 2:
                try:
                    z = float(coordinates[2])
                except (TypeError, ValueError):
                    z = None
            points.append(
                KickTheMapObjectPoint(
                    object_name=object_name,
                    source_name=source_name,
                    x=x,
                    y=y,
                    z=z,
                    attribute_1=attribute_1,
                    attribute_2=attribute_2,
                    attribute_3=attribute_3,
                )
            )
            continue

        if not isinstance(coordinates, list) or len(coordinates) < 2:
            continue
        vertices: list[KickTheMapPolylineVertex] = []
        for raw_vertex in coordinates:
            if not isinstance(raw_vertex, list) or len(raw_vertex) < 2:
                continue
            try:
                x = float(raw_vertex[0])
                y = float(raw_vertex[1])
            except (TypeError, ValueError):
                continue
            z = None
            if len(raw_vertex) > 2:
                try:
                    z = float(raw_vertex[2])
                except (TypeError, ValueError):
                    z = None
            vertices.append(KickTheMapPolylineVertex(x=x, y=y, z=z))
        if len(vertices) < 2:
            continue
        polylines.append(
            KickTheMapObjectPolyline(
                object_name=object_name,
                source_name=source_name,
                vertices=tuple(vertices),
                attribute_1=attribute_1,
                attribute_2=attribute_2,
                attribute_3=attribute_3,
            )
        )

    if not points and not polylines:
        raise KickTheMapDxfExportError(
            "jobFeatures.json bevat geen handmatig geplaatste objectpunten of objectpolylines voor export."
        )
    return points, polylines


class KickTheMapDxfExporter:
    LABEL_HEIGHT = 6.0
    LABEL_GAP = 6.0
    LABEL_LINE_HEIGHT = 10.0
    LABEL_STYLE = "PROEFSLEUVEN_LIBMONO"
    LABEL_LAYER = "PROEFSLEUVEN_LABEL"

    def __init__(
        self,
        point_half_length: float = DEFAULT_POINT_HALF_LENGTH,
        layer_rules: list[tuple[str, str, int]] | None = None,
        label_gap: float = LABEL_GAP,
    ) -> None:
        self.point_half_length = max(0.01, float(point_half_length))
        self.layer_rules = build_object_layer_rules(layer_rules)
        self.label_gap = max(0.0, float(label_gap))

    def export(
        self,
        output_path: str | Path,
        datasets: list[KickTheMapObjectDataset],
        status_callback=None,
    ) -> Path:
        if not datasets:
            raise KickTheMapDxfExportError("Selecteer minimaal een KickTheMap job voor de DXF-export.")

        valid_datasets = [dataset for dataset in datasets if dataset.points]
        if not valid_datasets:
            raise KickTheMapDxfExportError("Geen handmatig geplaatste objectpunten gevonden voor export.")

        output_file = Path(output_path)
        output_file.parent.mkdir(parents=True, exist_ok=True)

        if status_callback is not None:
            status_callback("KickTheMap object-DXF opbouwen...")

        document = ezdxf.new("R2018", setup=True)
        document.units = 6
        if APP_ID not in document.appids:
            document.appids.add(APP_ID)
        self._setup_label_layer(document)
        self._setup_text_style(document)
        modelspace = document.modelspace()

        all_points: list[tuple[float, float, float]] = []
        dataset_bounds: list[tuple[KickTheMapObjectDataset, Bounds]] = []
        for dataset in valid_datasets:
            if status_callback is not None:
                status_callback(f"KickTheMap job verwerken: {dataset.job_title}")
            job_line_points = self._add_dataset_lines(document, modelspace, dataset)
            all_points.extend(job_line_points)
            bounds = self._bounds_from_points(job_line_points)
            if bounds is not None:
                dataset_bounds.append((dataset, bounds))

        if not all_points:
            raise KickTheMapDxfExportError("Er konden geen lijnsegmenten uit de KickTheMap objectpunten worden gemaakt.")

        placed_label_bounds: list[Bounds] = []
        all_job_bounds = [bounds for _dataset, bounds in dataset_bounds]
        for fallback_index, (dataset, bounds) in enumerate(dataset_bounds, start=1):
            label = self._job_label(dataset, fallback_index)
            label_center = self._pick_label_position(
                bounds,
                label,
                all_job_bounds,
                placed_label_bounds,
                self.label_gap,
            )
            self._add_job_label(modelspace, label, label_center)
            placed_label_bounds.append(self._label_bounds(label, label_center))

        xs = [point[0] for point in all_points]
        ys = [point[1] for point in all_points]
        zs = [point[2] for point in all_points]
        document.header["$EXTMIN"] = (min(xs), min(ys), min(zs))
        document.header["$EXTMAX"] = (max(xs), max(ys), max(zs))

        try:
            document.saveas(output_file)
        except Exception as exc:
            raise KickTheMapDxfExportError(f"DXF kon niet worden opgeslagen: {exc}") from exc
        return output_file

    def _ensure_layer(self, document: ezdxf.EzDxfDocument, layer_name: str, color: int) -> None:
        if layer_name == "0":
            return
        if layer_name in document.layers:
            layer = document.layers.get(layer_name)
            layer.color = color
            return
        document.layers.add(
            name=layer_name,
            dxfattribs={
                "color": color,
            },
        )

    def _setup_label_layer(self, document: ezdxf.EzDxfDocument) -> None:
        if self.LABEL_LAYER in document.layers:
            layer = document.layers.get(self.LABEL_LAYER)
            layer.color = 7
            layer.rgb = (255, 255, 255)
            return
        document.layers.add(
            name=self.LABEL_LAYER,
            dxfattribs={
                "color": 7,
                "true_color": rgb2int((255, 255, 255)),
            },
        )

    def _setup_text_style(self, document: ezdxf.EzDxfDocument) -> None:
        if self.LABEL_STYLE not in document.styles:
            document.styles.add(self.LABEL_STYLE, font="LiberationMono-Regular.ttf")

    def _add_dataset_lines(
        self,
        document: ezdxf.EzDxfDocument,
        modelspace,
        dataset: KickTheMapObjectDataset,
    ) -> list[tuple[float, float, float]]:
        perp_angle = self._perpendicular_angle(dataset.points)
        all_points: list[tuple[float, float, float]] = []
        delta_x = math.cos(perp_angle) * self.point_half_length
        delta_y = math.sin(perp_angle) * self.point_half_length

        for point in dataset.points:
            layer_name, color = self._resolve_point_style(point)
            self._ensure_layer(document, layer_name, color)

            z_value = point.z if point.z is not None else 0.0
            start = (point.x + delta_x, point.y + delta_y, z_value)
            end = (point.x - delta_x, point.y - delta_y, z_value)
            line = modelspace.add_line(
                start,
                end,
                dxfattribs={
                    "layer": layer_name,
                    "color": color,
                },
            )
            line.set_xdata(
                APP_ID,
                [
                    (1000, f"Job: {dataset.job_title}"),
                    (1000, f"JobId: {dataset.job_id}"),
                    (1000, f"Object: {point.object_name}"),
                    (1000, f"Coding: {point.source_name or '0'}"),
                    (1000, f"Layer: {layer_name}"),
                ],
            )
            all_points.extend((start, end))
        return all_points

    def _resolve_point_style(self, point: KickTheMapObjectPoint) -> tuple[str, int]:
        for rule in self.layer_rules:
            if rule.matches(point.source_name):
                return rule.target_layer, rule.color
        return "0", 256

    def _job_label(self, dataset: KickTheMapObjectDataset, fallback_index: int) -> str:
        for source_name in (dataset.job_title, dataset.source_path.stem):
            if not source_name:
                continue
            match = re.search(r"(?i)\bps[\s._-]*(\d+)\b", source_name)
            if match:
                return f"PS{int(match.group(1))}"
            digits = re.search(r"(\d+)", source_name)
            if digits:
                return f"PS{int(digits.group(1))}"
        return f"PS{fallback_index}"

    def _add_job_label(self, modelspace, label: str, insert: tuple[float, float]) -> None:
        entity = modelspace.add_text(
            label,
            dxfattribs={
                "layer": self.LABEL_LAYER,
                "style": self.LABEL_STYLE,
                "height": self.LABEL_HEIGHT,
                "color": 7,
                "true_color": rgb2int((255, 255, 255)),
            },
        )
        entity.set_placement(insert, align=TextEntityAlignment.MIDDLE_CENTER)

    def _pick_label_position(
        self,
        own_bounds: Bounds,
        label: str,
        dataset_bounds: list[Bounds],
        placed_label_bounds: list[Bounds],
        label_gap: float,
    ) -> tuple[float, float]:
        best_center: tuple[float, float] | None = None
        best_score: tuple[float, float, float, float] | None = None
        label_width = self._label_width(label)
        half_width = label_width / 2.0
        half_height = self.LABEL_LINE_HEIGHT / 2.0

        for ring in range(5):
            gap = label_gap + ring * (self.LABEL_HEIGHT * 0.75)
            candidates = [
                (own_bounds.center_x, own_bounds.max_y + gap + half_height),
                (own_bounds.center_x, own_bounds.min_y - gap - half_height),
                (own_bounds.max_x + gap + half_width, own_bounds.center_y),
                (own_bounds.min_x - gap - half_width, own_bounds.center_y),
                (own_bounds.max_x + gap + half_width, own_bounds.max_y + gap + half_height),
                (own_bounds.min_x - gap - half_width, own_bounds.max_y + gap + half_height),
                (own_bounds.max_x + gap + half_width, own_bounds.min_y - gap - half_height),
                (own_bounds.min_x - gap - half_width, own_bounds.min_y - gap - half_height),
            ]
            for priority, center in enumerate(candidates):
                label_bounds = self._label_bounds(label, center)
                overlaps_labels = sum(1 for other in placed_label_bounds if label_bounds.intersects(other))
                overlaps_jobs = sum(
                    1 for other in dataset_bounds if other is not own_bounds and label_bounds.intersects(other)
                )
                distance = self._distance_to_bounds(center, own_bounds)
                score = (float(overlaps_labels), float(overlaps_jobs), distance, float(priority))
                if best_score is None or score < best_score:
                    best_score = score
                    best_center = center
                if overlaps_labels == 0 and overlaps_jobs == 0:
                    return center

        return best_center if best_center is not None else (own_bounds.center_x, own_bounds.max_y + label_gap + half_height)

    def _label_width(self, label: str) -> float:
        return max(self.LABEL_HEIGHT * 2.0, len(label) * self.LABEL_HEIGHT * 0.72)

    def _label_bounds(self, label: str, center: tuple[float, float]) -> Bounds:
        half_width = self._label_width(label) / 2.0
        half_height = self.LABEL_LINE_HEIGHT / 2.0
        return Bounds(
            center[0] - half_width,
            center[1] - half_height,
            center[0] + half_width,
            center[1] + half_height,
        )

    def _distance_to_bounds(self, point: tuple[float, float], bounds: Bounds) -> float:
        x = min(max(point[0], bounds.min_x), bounds.max_x)
        y = min(max(point[1], bounds.min_y), bounds.max_y)
        return math.hypot(point[0] - x, point[1] - y)

    def _bounds_from_points(self, points: list[tuple[float, ...]]) -> Bounds | None:
        if not points:
            return None
        xs = [point[0] for point in points]
        ys = [point[1] for point in points]
        return Bounds(min(xs), min(ys), max(xs), max(ys))

    def _perpendicular_angle(self, points: tuple[KickTheMapObjectPoint, ...]) -> float:
        first = points[0]
        last = next((point for point in reversed(points) if not self._same_point(first, point)), None)
        if last is None:
            return math.pi / 2.0
        angle = math.atan2(last.y - first.y, last.x - first.x)
        return angle + (math.pi / 2.0)

    @staticmethod
    def _same_point(first: KickTheMapObjectPoint, second: KickTheMapObjectPoint, tolerance: float = 1e-6) -> bool:
        return abs(first.x - second.x) <= tolerance and abs(first.y - second.y) <= tolerance
