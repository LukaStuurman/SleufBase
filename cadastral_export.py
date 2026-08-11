from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
import re
import shutil
from math import atan2, ceil, cos, degrees, floor, hypot, radians, sin
from typing import Iterable

import ezdxf
import numpy as np
from ezdxf.colors import rgb2int
from ezdxf.enums import TextEntityAlignment
from ezdxf.lldxf.types import DXFTag
from PIL import Image, ImageDraw, ImageFont
from shapely.geometry import LineString

from . import native_accel
from .bgt_roadpart import BgtRoadPartClient, BgtRoadPartError
from .bgt_terrain_boundary import BgtTerrainBoundaryClient, BgtTerrainBoundaryError
from .bgt_vector_tiles import BgtSurfaceFeature, BgtVectorTileClient, BgtVectorTileError
from .cadastral_wfs import CadastralLinework, CadastralTextLabel, CadastralWfsClient, CadastralWfsError
from .exporting import MapExporter
from .kickthemap_dxf_export import (
    KickTheMapObjectDataset,
    KickTheMapObjectFeature,
    KickTheMapObjectPoint,
    KickTheMapObjectPolyline,
    KickTheMapPolylineVertex,
    ObjectLayerRule,
    build_object_layer_rules,
)
from .location_search import PdokLocationClient
from .models import Bounds, CableFeature, DxfOverlay, GeoTiffLayer, MapComment, ProfileReferenceAnnotation
from .osm import OpenStreetMapTileClient
from .pdok import PdokKadastralekaartWmtsTileClient, PdokWmsClient, PdokWmtsTileClient
from .road_centerline import RoadCenterlineClient, RoadCenterlineError
from .virtual_trench import (
    build_virtual_trench_render,
    build_virtual_trench_dataset,
    is_virtual_trench_layer,
    virtual_trench_centerline,
    virtual_trench_polygon,
)


class CadastralExportError(RuntimeError):
    """Raised when de kadastrale DXF-export mislukt."""


@dataclass(frozen=True)
class TemplateSlot:
    label_handle: str
    address_handle: str | None
    comments_handle: str | None
    row_box: Bounds
    tiff_box: Bounds
    map_box: Bounds
    sort_x: float
    sort_y: float


TemplateSlotLayer = GeoTiffLayer | None


@dataclass(frozen=True)
class PreparedTemplateSlotAssets:
    formatted_address: str | None
    comments_text: str
    raster_path: Path
    map_raster_path: Path


@dataclass(frozen=True)
class PreparedTiffRaster:
    layer: GeoTiffLayer
    raster_path: Path


@dataclass(frozen=True)
class TemplateServerData:
    linework: list[CadastralLinework]
    text_labels: list[CadastralTextLabel]
    road_centerline_paths: list[list[tuple[float, float]]]
    terrain_boundary_paths: list[list[tuple[float, float]]]
    bgt_surface_features: list[BgtSurfaceFeature]


@dataclass(frozen=True)
class TemplateCrossSectionPoint:
    point: KickTheMapObjectFeature
    chainage: float
    point_z: float
    layer_name: str
    color: int
    description: str
    is_endpoint: bool = False


@dataclass(frozen=True)
class TemplateCrossSectionProfile:
    start_point: KickTheMapObjectPoint
    end_point: KickTheMapObjectPoint
    axis_dx: float
    axis_dy: float
    axis_length: float
    reference_level: float
    points: tuple[TemplateCrossSectionPoint, ...]


class CadastralDxfExporter:
    TRENCH_MODE_POLYGON = "polygon"
    TRENCH_MODE_CENTERLINE = "centerline"
    LABEL_HEIGHT = 6.0
    LABEL_GAP = 6.0
    LABEL_LINE_HEIGHT = 10.0
    LABEL_STYLE = "PROEFSLEUVEN_LIBMONO"
    CADASTRAL_LABEL_STYLE = "KADASTRALE_LABELS"
    BGT_ROADPART_LAYER = "BGT_WEGDEEL"
    BGT_ROADPART_RGB = (145, 145, 145)
    BGT_VECTOR_OUTLINE_LAYER = "BGT_VECTOR_OMTREK"
    BGT_VECTOR_OUTLINE_RGB = (155, 155, 155)
    MASK_ALPHA_THRESHOLD = 15
    MIN_TRENCH_AREA_RATIO = 0.005
    MAX_TRENCH_AREA_RATIO = 0.45
    STREET_LABEL_HEIGHT = 1.0
    HOUSE_NUMBER_HEIGHT = 1.0
    CENTERLINE_WIDTH = 0.6
    TEMPLATE_COORD_TEXT_HEIGHT = 0.045
    TEMPLATE_SCALE_TEXT_HEIGHT = 0.05
    TEMPLATE_NORTH_TEXT_HEIGHT = 0.09
    VIRTUAL_TRENCH_EXPORT_QUALITY_MULTIPLIER = 6.0
    TEMPLATE_LOGO_LAYOUT_NAME = "Blad1"
    TEMPLATE_LAYOUT_NAME_PREFIX = "Blad"
    TEMPLATE_SLOTS_PER_LAYOUT = 8
    TEMPLATE_PAGE_CONTENT_MARGIN = 2.0
    TEMPLATE_WIREFRAME_SCALE_DENOMINATORS = (250, 500, 1000, 2000, 5000, 10000)
    TEMPLATE_TECHBASE_LOGO_LOCAL_BOX = Bounds(
        -178.6673353950342,
        1.223957895626142,
        -162.6727998461477,
        17.66716785422732,
    )
    TEMPLATE_CLIENT_LOGO_LOCAL_CELL = Bounds(
        -180.0,
        34.0,
        -116.858581,
        53.96902090226447,
    )
    TEMPLATE_CLIENT_LOGO_PLACEHOLDER_SIZE = (37.30787274584635, 12.40736127731883)
    TEMPLATE_CLIENT_LOGO_LEFT_PADDING = 1.3326646049658
    TEMPLATE_CONTACT_STREET = "De Bloemendaal 10"
    TEMPLATE_CONTACT_POSTCODE = "5221 EC"
    TEMPLATE_CONTACT_CITY = "'s-Hertogenbosch"
    TEMPLATE_PROFILE_LEFT_EXTENSION = 2.0
    TEMPLATE_PROFILE_START_TO_TIFF_OFFSET = 6.7895
    TEMPLATE_PROFILE_ROW_BOTTOM_TO_TIFF_OFFSET = 0.8883
    TEMPLATE_PROFILE_REFERENCE_Y_TO_TIFF_OFFSET = 0.4283
    TEMPLATE_PROFILE_TITLE_Y_TO_TIFF_OFFSET = 0.4683
    TEMPLATE_PROFILE_TOP_BAND_Y_TO_TIFF_OFFSET = 0.4883
    TEMPLATE_PROFILE_BOTTOM_BAND_Y_TO_TIFF_OFFSET = 0.6883
    TEMPLATE_PROFILE_RIGHT_TEXT_MARGIN = 1.705
    TEMPLATE_PROFILE_REFERENCE_MARGIN = 0.04
    TEMPLATE_PROFILE_MAAIVELD_TEXT_HEIGHT = 0.035
    TEMPLATE_PROFILE_FILL_TEXT_HEIGHT = 0.035
    TEMPLATE_PROFILE_FILL_TEXT_OFFSET = 0.04
    TEMPLATE_PROFILE_FILL_TEXT_GAP = 0.05
    TEMPLATE_PROFILE_SOIL_BOX_WIDTH = 0.4
    TEMPLATE_PROFILE_SOIL_TEXT_HEIGHT = 0.04
    TEMPLATE_PROFILE_SOIL_BOTTOM_OFFSET = 0.18040671
    TEMPLATE_PROFILE_BAND_VALUE_TEXT_HEIGHT = 0.03
    TEMPLATE_PROFILE_BAND_VALUE_MIN_GAP = 0.03
    TEMPLATE_PROFILE_REFERENCE_MARKER_RADIUS = 0.04
    TEMPLATE_PROFILE_REFERENCE_ARROW_LENGTH = 0.18
    TEMPLATE_PROFILE_REFERENCE_ARROW_HEAD = 0.05
    TEMPLATE_PROFILE_REFERENCE_TEXT_HEIGHT = 0.03
    TEMPLATE_PROFILE_REFERENCE_TEXT_OFFSET = 0.08
    TEMPLATE_PROFILE_REFERENCE_TEXT_GAP = 0.05
    TEMPLATE_PROFILE_REFERENCE_LABEL_LEADER_LENGTH = 0.22
    TEMPLATE_PROFILE_REFERENCE_LABEL_LEADER_RISE = 0.08
    TEMPLATE_PROFILE_REFERENCE_MIDDLE_TEXT_OFFSET = 0.025
    TEMPLATE_PROFILE_LEADER_TEXT_HEIGHT = 0.03
    TEMPLATE_PROFILE_LEADER_TEXT_GAP = 0.05
    TEMPLATE_PROFILE_LEADER_VERTICAL_STEP = 0.09
    TEMPLATE_PROFILE_LINEWEIGHT = 0
    TEMPLATE_PROFILE_LEADER_LINE_COLOR = 8
    TEMPLATE_PROFILE_LEGACY_DYNAMIC_LEADER_BLOCK_NAME = "SAL-VERWIJZING_LEIDING_BOVENKANT-SOD"
    TEMPLATE_PROFILE_LEADER_BLOCK_NAME = "SAL-VERWIJZING_LEIDING_BOVENKANT_INFO_DIEPTE-SOD"
    TEMPLATE_MAAIVELD_METADATA_KEY = "template_maaiveld_segments"
    TEMPLATE_REFERENCE_POINT_METADATA_KEY = "template_reference_point"
    TEMPLATE_CROSS_SECTION_START_POINT_METADATA_KEY = "template_cross_section_start_point"
    TEMPLATE_DEKBAND_METADATA_KEY = "template_dekband_lines"
    TEMPLATE_PROEFSLEUF_LABEL_METADATA_KEY = "template_proefsleuf_label"
    TEMPLATE_MAAIVELD_DEFAULT_HEX = "#808080"
    TEMPLATE_DEKBAND_COLOR = 8

    def __init__(
        self,
        wfs_client: CadastralWfsClient,
        road_centerline_client: RoadCenterlineClient | None = None,
        terrain_boundary_client: BgtTerrainBoundaryClient | None = None,
        bgt_roadpart_client: BgtRoadPartClient | None = None,
        bgt_vector_tile_client: BgtVectorTileClient | None = None,
    ) -> None:
        self.wfs_client = wfs_client
        self.road_centerline_client = road_centerline_client
        self.terrain_boundary_client = terrain_boundary_client
        self.bgt_roadpart_client = bgt_roadpart_client
        self.bgt_vector_tile_client = bgt_vector_tile_client or BgtVectorTileClient()

    def export_overview(
        self,
        output_path: str | Path,
        tiff_layers: list[GeoTiffLayer],
        status_callback=None,
        trench_mode: str = TRENCH_MODE_POLYGON,
        include_tiff_images: bool = False,
        label_gap: float = LABEL_GAP,
        centerline_color: tuple[int, int, int] = (0, 0, 0),
        label_color: tuple[int, int, int] = (0, 0, 0),
        include_bgt_roadparts: bool = False,
    ) -> Path:
        if not tiff_layers:
            raise CadastralExportError("Laad eerst minimaal een GeoTIFF-bestand.")

        output_file = Path(output_path)
        output_file.parent.mkdir(parents=True, exist_ok=True)

        overview_bounds = self._combined_bounds(tiff_layers)
        fetch_bounds = overview_bounds.padded(self._overview_padding(overview_bounds))

        try:
            if status_callback is not None:
                status_callback("Haal kadastrale WFS-data op...")
            linework = self.wfs_client.fetch_linework(fetch_bounds)
            text_labels = self.wfs_client.fetch_text_labels(fetch_bounds)
            linework = self._linework_with_bgt_roadparts(
                linework,
                fetch_bounds,
                include_bgt_roadparts=include_bgt_roadparts,
                status_callback=status_callback,
            )
        except CadastralWfsError as exc:
            raise CadastralExportError(str(exc)) from exc
        except BgtRoadPartError as exc:
            raise CadastralExportError(str(exc)) from exc

        if status_callback is not None:
            status_callback("Schrijf DXF...")
        self._write_dxf(
            output_file,
            tiff_layers,
            linework,
            text_labels,
            fetch_bounds,
            trench_mode=trench_mode,
            include_tiff_images=include_tiff_images,
            label_gap=max(0.0, float(label_gap)),
            centerline_color=centerline_color,
            label_color=label_color,
        )
        return output_file

    def export_template_sheet(
        self,
        output_path: str | Path,
        template_path: str | Path,
        tiff_layers: list[TemplateSlotLayer],
        status_callback=None,
        trench_mode: str = TRENCH_MODE_POLYGON,
        include_tiff_images: bool = False,
        label_gap: float = LABEL_GAP,
        centerline_color: tuple[int, int, int] = (0, 0, 0),
        label_color: tuple[int, int, int] = (0, 0, 0),
        page_exporter: MapExporter | None = None,
        dxf_overlays: list[DxfOverlay] | None = None,
        map_comments: list[MapComment] | None = None,
        background_provider=None,
        background_attribution: str | None = None,
        location_client=None,
        techbase_logo_path: str | Path | None = None,
        client_logo_path: str | Path | None = None,
        cross_section_datasets: dict[int, KickTheMapObjectDataset] | None = None,
        cross_section_layer_rules: list[tuple[str, str, int]] | None = None,
        include_cross_sections: bool = False,
        show_profile_direction: bool = True,
        use_custom_maaiveld_points: bool = True,
        auto_fill_bgt_fysiek_voorkomen: bool | None = None,
        avoid_cross_section_multileader_collisions: bool = True,
        clip_cross_section_markers_to_profile: bool = False,
        cross_section_marker_diameter: float = 0.02,
        cross_section_scale_denominators: dict[int, int] | None = None,
        reverse_cross_sections: bool = False,
        include_bgt_roadparts: bool = True,
        template_drawn_by: str = "",
        cross_section_diameter_text: str = "Ø",
    ) -> Path:
        if not tiff_layers:
            raise CadastralExportError("Laad eerst minimaal een GeoTIFF-bestand.")
        filled_layers = self._filled_template_layers(tiff_layers)
        if not filled_layers:
            raise CadastralExportError("Voeg minimaal één gevuld proefsleufvak toe voor de sjabloonexport.")

        output_file = Path(output_path)
        output_file.parent.mkdir(parents=True, exist_ok=True)

        template_file = Path(template_path)
        if not template_file.exists():
            raise CadastralExportError(f"DXF-sjabloon niet gevonden: {template_file}")

        try:
            document = ezdxf.readfile(template_file)
        except Exception as exc:
            raise CadastralExportError(f"DXF-sjabloon kon niet worden gelezen: {exc}") from exc

        self._remove_template_legacy_profile_leader_blocks(document)
        slots = self._detect_template_slots(document)
        if len(tiff_layers) > len(slots):
            self._extend_template_for_slot_count(document, len(tiff_layers))
            slots = self._detect_template_slots(document)
        if len(tiff_layers) > len(slots):
            raise CadastralExportError(
                f"Het sjabloon heeft na uitbreiding {len(slots)} beschikbare proefsleufvakken, maar er zijn {len(tiff_layers)} TIFF-bestanden geladen."
            )
        used_layout_names = self._used_template_layout_names(len(tiff_layers))

        overview_bounds = self._combined_bounds(filled_layers)
        fetch_bounds = overview_bounds.padded(self._overview_padding(overview_bounds))
        resolved_auto_fill_bgt = (
            bool(getattr(self, "auto_fill_bgt_fysiek_voorkomen", True))
            if auto_fill_bgt_fysiek_voorkomen is None
            else bool(auto_fill_bgt_fysiek_voorkomen)
        )

        if status_callback is not None:
            status_callback("Haal serverdata op...")
        orientation_bounds = self._template_orientation_fetch_bounds(filled_layers, fetch_bounds)
        server_data = self._fetch_template_server_data_single(
            fetch_bounds,
            orientation_bounds=orientation_bounds,
            include_terrain_boundaries=bool(cross_section_datasets and (include_cross_sections or show_profile_direction)),
            include_bgt_roadparts=include_bgt_roadparts,
            include_bgt_surface_features=bool(resolved_auto_fill_bgt and include_cross_sections),
            status_callback=status_callback,
        )
        linework = server_data.linework
        text_labels = server_data.text_labels
        road_centerline_paths = server_data.road_centerline_paths
        terrain_boundary_paths = server_data.terrain_boundary_paths
        bgt_surface_features = server_data.bgt_surface_features

        if status_callback is not None:
            status_callback("Bereid kadastraal sjabloon voor...")

        document.units = 6
        document.set_raster_variables(frame=0, quality=1, units="m")
        self._ensure_template_image_layer(document)
        self._setup_layers(document, centerline_color=centerline_color, label_color=label_color)
        self._setup_text_styles(document)
        self._ensure_template_layout_legends(document, used_layout_names)

        modelspace = document.modelspace()
        self._populate_overview_modelspace(
            document,
            modelspace,
            output_file,
            filled_layers,
            linework,
            text_labels,
            fetch_bounds,
            trench_mode=trench_mode,
            include_tiff_images=include_tiff_images,
            label_gap=max(0.0, float(label_gap)),
            centerline_color=centerline_color,
            label_color=label_color,
        )
        asset_dir = output_file.parent / f"{output_file.stem}_assets"
        asset_dir.mkdir(parents=True, exist_ok=True)
        self._restore_template_layout_logos(
            document,
            asset_dir,
            used_layout_names,
            techbase_logo_path=techbase_logo_path,
            client_logo_path=client_logo_path,
        )
        self._restore_template_layout_contact_text(document)
        self._update_template_layout_page_numbers(document, used_layout_names)
        self._update_template_title_block_attributes(
            document,
            used_layout_names,
            template_drawn_by=template_drawn_by,
        )
        self._update_template_scale_list(document)
        self._configure_template_wireframe_viewports(document, tiff_layers, used_layout_names, label_gap)
        resolved_cross_section_rules = build_object_layer_rules(cross_section_layer_rules)
        resolved_marker_scale = max(0.005, float(cross_section_marker_diameter))
        resolved_scale_denominators = cross_section_scale_denominators or {}
        cross_section_profiles: dict[int, TemplateCrossSectionProfile] = {}
        if cross_section_datasets and (include_cross_sections or show_profile_direction):
            for index, layer in enumerate(tiff_layers, start=1):
                if layer is None:
                    continue
                job_id = self._kickthemap_job_id(layer)
                if job_id is None:
                    continue
                dataset = cross_section_datasets.get(job_id)
                if dataset is None:
                    continue
                prepared_dataset = self._dataset_with_template_dekbanden(
                    layer,
                    dataset,
                    resolved_cross_section_rules,
                    road_centerline_paths,
                    terrain_boundary_paths,
                    resolved_marker_scale,
                    bool(reverse_cross_sections),
                )
                profile = self._build_template_cross_section_profile(
                    prepared_dataset,
                    resolved_cross_section_rules,
                    road_centerline_paths,
                    terrain_boundary_paths,
                    resolved_marker_scale,
                    bool(reverse_cross_sections),
                )
                if profile is not None:
                    cross_section_profiles[index - 1] = profile
        prepared_assets: dict[int, PreparedTemplateSlotAssets] = {}

        if status_callback is not None:
            status_callback("Bereid sjabloonafbeeldingen voor...")
        self._prefetch_template_background_maps(
            page_exporter,
            background_provider,
            filled_layers,
            status_callback=status_callback,
        )
        asset_tasks = [
            (
                index - 1,
                {
                    "asset_dir": asset_dir,
                    "layer": layer,
                    "label": self._template_proefsleuf_label(layer, index),
                    "index": index,
                    "road_centerline_paths": road_centerline_paths,
                    "terrain_boundary_paths": terrain_boundary_paths,
                    "profile": cross_section_profiles.get(index - 1),
                    "trench_mode": trench_mode,
                    "centerline_color": centerline_color,
                    "label_color": label_color,
                    "page_exporter": page_exporter,
                    "dxf_overlays": dxf_overlays or [],
                    "map_comments": map_comments,
                    "background_provider": background_provider,
                    "background_attribution": background_attribution,
                    "location_client": location_client,
                    "reference_annotation": (
                        self._template_profile_reference_annotation(layer, cross_section_profiles.get(index - 1))
                        if show_profile_direction
                        else None
                    ),
                    "reverse_tiff_orientation": reverse_cross_sections,
                },
            )
            for index, layer in enumerate(tiff_layers, start=1)
            if layer is not None
        ]
        prepared_assets.update(
            self._prepare_template_slot_assets_batch(asset_tasks, status_callback=status_callback)
        )

        for index, (layer, slot) in enumerate(zip(tiff_layers, slots), start=1):
            if layer is None:
                if status_callback is not None:
                    status_callback(f"Houd sjabloonvak {index}/{len(tiff_layers)} leeg.")
                self._clear_template_slot(document, slot)
                continue
            label = self._template_proefsleuf_label(layer, index)
            title = self._template_title(layer, index)
            slot_assets = prepared_assets[index - 1]
            if status_callback is not None:
                status_callback(f"Vul sjabloonvak {index}/{len(tiff_layers)}: {title}")
            self._set_template_text(document, slot.label_handle, title)
            formatted_address = slot_assets.formatted_address
            if formatted_address and slot.address_handle:
                self._set_template_mtext(document, slot.address_handle, formatted_address)
            elif slot.address_handle:
                self._set_template_mtext(document, slot.address_handle, "Adres:")
            if slot.comments_handle:
                self._set_template_mtext(document, slot.comments_handle, slot_assets.comments_text)
            if status_callback is not None:
                status_callback(f"Vul sjabloonvak {index}/{len(tiff_layers)}: TIFF-afbeelding")
            self._add_box_image(document, modelspace, slot_assets.raster_path, slot.tiff_box, f"{label}_tiff")
            if status_callback is not None:
                status_callback(f"Vul sjabloonvak {index}/{len(tiff_layers)}: kaartafbeelding")
            self._add_box_image(
                document,
                modelspace,
                slot_assets.map_raster_path,
                slot.map_box,
                f"{label}_kaart",
                inset=0.0,
                preserve_aspect=False,
            )
            profile = cross_section_profiles.get(index - 1)
            if include_cross_sections and profile is not None:
                if status_callback is not None:
                    status_callback(f"Vul sjabloonvak {index}/{len(tiff_layers)}: dwarsprofiel")
                self._add_template_cross_section(
                    document,
                    modelspace,
                    slot,
                    layer,
                    label,
                    profile,
                    show_profile_direction=bool(show_profile_direction),
                    use_custom_maaiveld_points=bool(use_custom_maaiveld_points),
                    bgt_surface_features=bgt_surface_features if resolved_auto_fill_bgt else [],
                    avoid_multileader_collisions=bool(avoid_cross_section_multileader_collisions),
                    clip_markers_to_profile=bool(clip_cross_section_markers_to_profile),
                    cross_section_marker_diameter=resolved_marker_scale,
                    manual_scale_denominator=resolved_scale_denominators.get(index - 1),
                    cross_section_diameter_text=str(cross_section_diameter_text or "Ø").strip() or "Ø",
                )

        if status_callback is not None:
            status_callback("Sla DXF-sjabloon op...")
        try:
            document.saveas(output_file)
        except Exception as exc:
            raise CadastralExportError(f"DXF kon niet worden opgeslagen: {exc}") from exc
        return output_file

    def _filled_template_layers(self, tiff_layers: list[TemplateSlotLayer]) -> list[GeoTiffLayer]:
        return [layer for layer in tiff_layers if layer is not None]

    def _clear_template_slot(self, document: ezdxf.EzDxfDocument, slot: TemplateSlot) -> None:
        self._set_template_text(document, slot.label_handle, "")
        if slot.address_handle:
            self._set_template_mtext(document, slot.address_handle, "Adres:")
        if slot.comments_handle:
            self._set_template_mtext(document, slot.comments_handle, "Opmerkingen:")

    def _combined_bounds(self, tiff_layers: list[GeoTiffLayer]) -> Bounds:
        combined = tiff_layers[0].bounds
        for layer in tiff_layers[1:]:
            combined = combined.union(layer.bounds)
        return combined

    def _linework_with_bgt_roadparts(
        self,
        linework: list[CadastralLinework],
        bounds: Bounds,
        *,
        include_bgt_roadparts: bool,
        status_callback=None,
    ) -> list[CadastralLinework]:
        if not include_bgt_roadparts or self.bgt_roadpart_client is None:
            return linework
        if status_callback is not None:
            status_callback("Haal BGT-wegdelen op...")
        roadpart_paths = self.bgt_roadpart_client.fetch_paths(bounds)
        if not roadpart_paths:
            return linework
        return [
            *linework,
            CadastralLinework(layer_name=self.BGT_ROADPART_LAYER, paths=roadpart_paths),
        ]

    def _fetch_template_server_data_single(
        self,
        bounds: Bounds,
        *,
        orientation_bounds: list[Bounds] | None = None,
        include_terrain_boundaries: bool = True,
        include_bgt_roadparts: bool = True,
        include_bgt_surface_features: bool = False,
        status_callback=None,
    ) -> TemplateServerData:
        try:
            if status_callback is not None:
                status_callback("Haal BGT-vector tiles op...")
            bgt_paths = self.bgt_vector_tile_client.fetch_paths(bounds)
            linework = []
            if bgt_paths:
                linework.append(
                    CadastralLinework(layer_name=self.BGT_VECTOR_OUTLINE_LAYER, paths=bgt_paths)
                )
            if include_bgt_surface_features:
                if status_callback is not None:
                    status_callback("Haal BGT-ondergrondnamen op...")
                bgt_surface_features = self.bgt_vector_tile_client.fetch_surface_features(bounds)
            else:
                bgt_surface_features = []
            if status_callback is not None:
                status_callback("Haal kadastrale perceelgrenzen op...")
            parcel_boundaries = self.wfs_client.fetch_parcel_boundaries(bounds)
            if parcel_boundaries is not None:
                linework.append(parcel_boundaries)
            if status_callback is not None:
                status_callback("Haal kadastrale teksten op...")
            text_labels = self.wfs_client.fetch_text_labels(bounds)
            local_orientation_bounds = orientation_bounds or [bounds]
            if self.road_centerline_client is not None:
                road_centerline_paths, terrain_fetch_bounds = self._fetch_template_paths_for_bounds_with_empty(
                    self.road_centerline_client,
                    local_orientation_bounds,
                    status_callback=status_callback,
                    status_label="Haal weg-hartlijnen op",
                )
            else:
                road_centerline_paths = []
                terrain_fetch_bounds = local_orientation_bounds
            terrain_boundary_paths = (
                self._fetch_template_paths_for_bounds(
                    self.terrain_boundary_client,
                    terrain_fetch_bounds,
                    status_callback=status_callback,
                    status_label="Haal BGT-terreinranden op",
                )
                if include_terrain_boundaries and self.terrain_boundary_client is not None and terrain_fetch_bounds
                else []
            )
        except CadastralWfsError as exc:
            raise CadastralExportError(str(exc)) from exc
        except RoadCenterlineError as exc:
            raise CadastralExportError(str(exc)) from exc
        except BgtTerrainBoundaryError as exc:
            raise CadastralExportError(str(exc)) from exc
        except BgtRoadPartError as exc:
            raise CadastralExportError(str(exc)) from exc
        except BgtVectorTileError as exc:
            raise CadastralExportError(str(exc)) from exc
        except Exception as exc:
            raise CadastralExportError(f"Serverdata ophalen mislukt: {exc}") from exc
        return TemplateServerData(
            linework=linework,
            text_labels=text_labels,
            road_centerline_paths=road_centerline_paths,
            terrain_boundary_paths=terrain_boundary_paths,
            bgt_surface_features=bgt_surface_features,
        )

    def _template_orientation_fetch_bounds(self, layers: list[GeoTiffLayer], fallback_bounds: Bounds) -> list[Bounds]:
        boxes: list[Bounds] = []
        for layer in layers:
            span = max(float(layer.bounds.width), float(layer.bounds.height))
            padding = max(35.0, min(75.0, span * 8.0))
            box = layer.bounds.padded(padding)
            clipped = box.intersection(fallback_bounds)
            boxes.append(clipped or box)
        if not boxes:
            return [fallback_bounds]
        return self._merge_template_fetch_bounds(boxes, max_count=12)

    def _merge_template_fetch_bounds(self, bounds_list: list[Bounds], max_count: int) -> list[Bounds]:
        merged = self._merge_intersecting_bounds(bounds_list)
        max_count = max(1, int(max_count))
        while len(merged) > max_count:
            best_pair: tuple[int, int] | None = None
            best_gap: float | None = None
            for left_index in range(len(merged)):
                for right_index in range(left_index + 1, len(merged)):
                    gap = self._bounds_gap(merged[left_index], merged[right_index])
                    if best_gap is None or gap < best_gap:
                        best_gap = gap
                        best_pair = (left_index, right_index)
            if best_pair is None:
                break
            left_index, right_index = best_pair
            merged[left_index] = merged[left_index].union(merged[right_index])
            del merged[right_index]
            merged = self._merge_intersecting_bounds(merged)
        return merged

    def _merge_intersecting_bounds(self, bounds_list: list[Bounds]) -> list[Bounds]:
        merged: list[Bounds] = []
        for bounds in bounds_list:
            current = bounds
            changed = True
            while changed:
                changed = False
                remaining: list[Bounds] = []
                for existing in merged:
                    if current.intersects(existing):
                        current = current.union(existing)
                        changed = True
                    else:
                        remaining.append(existing)
                merged = remaining
            merged.append(current)
        return merged

    @staticmethod
    def _bounds_gap(left: Bounds, right: Bounds) -> float:
        if left.max_x < right.min_x:
            gap_x = right.min_x - left.max_x
        elif right.max_x < left.min_x:
            gap_x = left.min_x - right.max_x
        else:
            gap_x = 0.0
        if left.max_y < right.min_y:
            gap_y = right.min_y - left.max_y
        elif right.max_y < left.min_y:
            gap_y = left.min_y - right.max_y
        else:
            gap_y = 0.0
        return hypot(gap_x, gap_y)

    def _fetch_template_paths_for_bounds(
        self,
        client,
        bounds_list: list[Bounds],
        *,
        status_callback=None,
        status_label: str,
    ) -> list[list[tuple[float, float]]]:
        paths, _ = self._fetch_template_paths_for_bounds_with_empty(
            client,
            bounds_list,
            status_callback=status_callback,
            status_label=status_label,
        )
        return paths

    def _fetch_template_paths_for_bounds_with_empty(
        self,
        client,
        bounds_list: list[Bounds],
        *,
        status_callback=None,
        status_label: str,
    ) -> tuple[list[list[tuple[float, float]]], list[Bounds]]:
        paths: list[list[tuple[float, float]]] = []
        empty_bounds: list[Bounds] = []
        total = len(bounds_list)
        for index, bounds in enumerate(bounds_list, start=1):
            if status_callback is not None:
                if total > 1:
                    status_callback(f"{status_label}... {index}/{total}")
                else:
                    status_callback(f"{status_label}...")
            fetched_paths = client.fetch_paths(bounds)
            if not fetched_paths:
                empty_bounds.append(bounds)
            paths.extend(fetched_paths)
        return self._dedupe_template_paths(paths), empty_bounds

    @staticmethod
    def _dedupe_template_paths(paths: list[list[tuple[float, float]]]) -> list[list[tuple[float, float]]]:
        deduped: list[list[tuple[float, float]]] = []
        seen: set[tuple[tuple[float, float], ...]] = set()
        for path in paths:
            if len(path) < 2:
                continue
            key = tuple((round(float(x), 3), round(float(y), 3)) for x, y in path)
            if key in seen:
                continue
            seen.add(key)
            deduped.append(path)
        return deduped

    def _linework_to_dxf_overlay(
        self,
        source_path: Path,
        linework: list[CadastralLinework],
        *,
        layer_names: set[str] | None = None,
    ) -> DxfOverlay | None:
        features: list[CableFeature] = []
        for group in linework:
            if layer_names is not None and group.layer_name not in layer_names:
                continue
            color = self._linework_overlay_color(group.layer_name)
            for index, path in enumerate(group.paths):
                if len(path) < 2:
                    continue
                bounds = self._bounds_from_points(path)
                features.append(
                    CableFeature(
                        feature_id=f"{group.layer_name}:{index}",
                        source_path=source_path,
                        points=path,
                        bounds=bounds,
                        color=color,
                        metadata={
                            "Bestand": source_path.name,
                            "Type": "LWPOLYLINE",
                            "Laag": group.layer_name,
                            "Punten": str(len(path)),
                        },
                    )
                )
        if not features:
            return None
        return DxfOverlay(path=source_path, features=features, visible=True)

    def _linework_overlay_color(self, layer_name: str) -> tuple[int, int, int]:
        if layer_name == self.BGT_VECTOR_OUTLINE_LAYER:
            return self.BGT_VECTOR_OUTLINE_RGB
        if layer_name == self.BGT_ROADPART_LAYER:
            return self.BGT_ROADPART_RGB
        if layer_name == "KAD_GRENS":
            return 110, 110, 110
        if layer_name == "KAD_BEBOUWING":
            return 175, 175, 175
        return 150, 150, 150

    def _detect_template_slots(self, document: ezdxf.EzDxfDocument) -> list[TemplateSlot]:
        modelspace = document.modelspace()
        raw_small_boxes: list[Bounds] = []
        raw_large_boxes: list[Bounds] = []
        raw_row_boxes: list[Bounds] = []

        for entity in modelspace:
            if entity.dxftype() != "LWPOLYLINE":
                continue
            bounds = self._polyline_bounds(entity)
            if bounds is None:
                continue
            if entity.dxf.layer == "Hulplijn kader":
                if 1.95 <= bounds.width <= 2.2 and 1.45 <= bounds.height <= 1.65:
                    raw_small_boxes.append(bounds)
                elif 2.35 <= bounds.width <= 2.7 and 1.7 <= bounds.height <= 1.9:
                    raw_large_boxes.append(bounds)
            elif entity.dxf.layer == "KADER" and 28.5 <= bounds.width <= 29.5 and 2.5 <= bounds.height <= 3.0:
                raw_row_boxes.append(bounds)

        if not raw_small_boxes or not raw_large_boxes or not raw_row_boxes:
            raise CadastralExportError("Kon in het DXF-sjabloon geen proefsleufvakken herkennen.")

        right_limit = self._standard_template_right_limit(raw_large_boxes)
        small_boxes = [box for box in raw_small_boxes if box.max_x <= right_limit]
        large_boxes = [box for box in raw_large_boxes if box.max_x <= right_limit]
        row_boxes = [box for box in raw_row_boxes if box.max_x <= right_limit]
        label_entities = [
            entity
            for entity in modelspace
            if entity.dxftype() == "TEXT"
            and entity.dxf.layer == "KADER"
            and str(entity.dxf.text).strip().lower().startswith("proefsleuf")
            and float(entity.dxf.insert.x) <= right_limit
        ]
        address_entities = [
            entity
            for entity in modelspace
            if entity.dxftype() == "MTEXT"
            and entity.dxf.layer == "KADER"
            and self._plain_mtext(entity).strip().lower().startswith("adres")
            and float(entity.dxf.insert.x) <= right_limit
        ]
        comment_entities = [
            entity
            for entity in modelspace
            if entity.dxftype() == "MTEXT"
            and entity.dxf.layer == "0"
            and self._plain_mtext(entity).strip().lower().startswith("opmerkingen")
            and float(entity.dxf.insert.x) <= right_limit
        ]

        used_large: set[int] = set()
        used_labels: set[str] = set()
        used_addresses: set[str] = set()
        used_comments: set[str] = set()
        slots: list[TemplateSlot] = []

        for small_box in sorted(small_boxes, key=lambda box: (-box.center_y, box.center_x)):
            pair = self._match_large_template_box(small_box, large_boxes, used_large)
            if pair is None:
                continue
            large_index, large_box = pair
            row_box = self._match_template_row_box(small_box, large_box, row_boxes)
            if row_box is None:
                continue
            label_entity = self._match_template_text(
                label_entities,
                used_labels,
                center_x=(small_box.center_x + large_box.center_x) / 2.0,
                center_y=(small_box.center_y + large_box.center_y) / 2.0,
            )
            if label_entity is None:
                continue
            used_large.add(large_index)
            used_labels.add(label_entity.dxf.handle)
            address_entity = self._match_template_address(
                address_entities,
                used_addresses,
                center_x=(small_box.center_x + large_box.center_x) / 2.0,
                center_y=(small_box.center_y + large_box.center_y) / 2.0,
            )
            if address_entity is not None:
                used_addresses.add(address_entity.dxf.handle)
            comment_entity = self._match_template_comment(
                comment_entities,
                used_comments,
                center_x=(small_box.center_x + large_box.center_x) / 2.0,
                center_y=(small_box.center_y + large_box.center_y) / 2.0,
            )
            if comment_entity is not None:
                used_comments.add(comment_entity.dxf.handle)
            slots.append(
                TemplateSlot(
                    label_handle=label_entity.dxf.handle,
                    address_handle=None if address_entity is None else address_entity.dxf.handle,
                    comments_handle=None if comment_entity is None else comment_entity.dxf.handle,
                    row_box=row_box,
                    tiff_box=small_box,
                    map_box=large_box,
                    sort_x=small_box.center_x,
                    sort_y=small_box.center_y,
                )
            )

        slots = self._sort_template_slots(slots)
        if not slots:
            raise CadastralExportError("Kon geen geldige proefsleufvakken uit het DXF-sjabloon afleiden.")
        return slots

    def _standard_template_right_limit(self, large_boxes: list[Bounds]) -> float:
        centers = sorted(box.center_x for box in large_boxes)
        if not centers:
            return float("inf")
        clustered: list[list[float]] = []
        for center_x in centers:
            if not clustered or center_x - clustered[-1][-1] > 5.0:
                clustered.append([center_x])
            else:
                clustered[-1].append(center_x)
        if len(clustered) >= 2:
            return max(clustered[1]) + 2.0
        return max(centers) + 1.0

    def _polyline_bounds(self, entity) -> Bounds | None:
        points = list(entity.get_points("xy"))
        if not points:
            return None
        xs = [float(point[0]) for point in points]
        ys = [float(point[1]) for point in points]
        return Bounds(min(xs), min(ys), max(xs), max(ys))

    def _match_large_template_box(
        self,
        small_box: Bounds,
        large_boxes: list[Bounds],
        used_large: set[int],
    ) -> tuple[int, Bounds] | None:
        best: tuple[float, float, int, Bounds] | None = None
        for index, large_box in enumerate(large_boxes):
            if index in used_large:
                continue
            dx = large_box.center_x - small_box.center_x
            dy = abs(large_box.center_y - small_box.center_y)
            if dx <= 0.0 or dx > 4.0 or dy > 0.35:
                continue
            candidate = (dy, dx, index, large_box)
            if best is None or candidate < best:
                best = candidate
        if best is None:
            return None
        return best[2], best[3]

    def _match_template_row_box(self, small_box: Bounds, large_box: Bounds, row_boxes: list[Bounds]) -> Bounds | None:
        best: tuple[float, float, Bounds] | None = None
        for row_box in row_boxes:
            if row_box.min_x > small_box.min_x or row_box.max_x < large_box.max_x:
                continue
            dy = abs(row_box.center_y - small_box.center_y)
            if dy > 0.45:
                continue
            dx = abs(row_box.min_x - small_box.min_x)
            candidate = (dy, dx, row_box)
            if best is None or candidate < best:
                best = candidate
        return None if best is None else best[2]

    def _match_template_text(self, text_entities: list, used_handles: set[str], center_x: float, center_y: float):
        best: tuple[float, float, str, object] | None = None
        for entity in text_entities:
            handle = entity.dxf.handle
            if handle in used_handles:
                continue
            entity_x = float(entity.dxf.insert.x)
            entity_y = float(entity.dxf.insert.y)
            dy = abs(entity_y - center_y)
            dx = abs(entity_x - center_x)
            if dy > 1.25 or dx > 4.5:
                continue
            candidate = (dy, dx, handle, entity)
            if best is None or candidate < best:
                best = candidate
        return None if best is None else best[3]

    def _match_template_address(self, address_entities: list, used_handles: set[str], center_x: float, center_y: float):
        best: tuple[float, float, str, object] | None = None
        for entity in address_entities:
            handle = entity.dxf.handle
            if handle in used_handles:
                continue
            entity_x = float(entity.dxf.insert.x)
            entity_y = float(entity.dxf.insert.y)
            dy = abs(entity_y - center_y)
            if dy > 1.25 or entity_x >= center_x:
                continue
            candidate = (dy, abs(center_x - entity_x), handle, entity)
            if best is None or candidate < best:
                best = candidate
        return None if best is None else best[3]

    def _match_template_comment(self, comment_entities: list, used_handles: set[str], center_x: float, center_y: float):
        best: tuple[float, float, str, object] | None = None
        for entity in comment_entities:
            handle = entity.dxf.handle
            if handle in used_handles:
                continue
            entity_x = float(entity.dxf.insert.x)
            entity_y = float(entity.dxf.insert.y)
            dy = abs(entity_y - center_y)
            dx = abs(entity_x - center_x)
            if dy > 1.35 or dx > 3.0:
                continue
            candidate = (dy, dx, handle, entity)
            if best is None or candidate < best:
                best = candidate
        return None if best is None else best[3]

    def _sort_template_slots(self, slots: list[TemplateSlot]) -> list[TemplateSlot]:
        if not slots:
            return []

        rows = sorted(slots, key=lambda slot: -slot.sort_y)
        blocks: list[list[TemplateSlot]] = []
        current_block: list[TemplateSlot] = [rows[0]]
        previous_y = rows[0].sort_y

        for slot in rows[1:]:
            if previous_y - slot.sort_y > 6.0:
                blocks.append(current_block)
                current_block = [slot]
            else:
                current_block.append(slot)
            previous_y = slot.sort_y
        blocks.append(current_block)

        ordered: list[TemplateSlot] = []
        for block in blocks:
            columns: list[list[TemplateSlot]] = []
            for slot in sorted(block, key=lambda item: item.sort_x):
                if not columns or slot.sort_x - columns[-1][-1].sort_x > 5.0:
                    columns.append([slot])
                else:
                    columns[-1].append(slot)
            for column in columns:
                ordered.extend(sorted(column, key=lambda item: -item.sort_y))
        return ordered

    def _extend_template_for_slot_count(
        self,
        document: ezdxf.EzDxfDocument,
        required_slot_count: int,
    ) -> None:
        required_pages = max(1, (required_slot_count + self.TEMPLATE_SLOTS_PER_LAYOUT - 1) // self.TEMPLATE_SLOTS_PER_LAYOUT)
        while True:
            slots = self._detect_template_slots(document)
            current_pages = max(1, (len(slots) + self.TEMPLATE_SLOTS_PER_LAYOUT - 1) // self.TEMPLATE_SLOTS_PER_LAYOUT)
            if current_pages >= required_pages:
                return
            page_translation = self._append_template_modelspace_page(document, slots)
            self._ensure_template_layout_exists(
                document,
                current_pages + 1,
                main_viewport_translation=page_translation,
            )

    def _append_template_modelspace_page(
        self,
        document: ezdxf.EzDxfDocument,
        slots: list[TemplateSlot],
    ) -> tuple[float, float]:
        pages = self._template_slot_pages(slots)
        if not pages:
            raise CadastralExportError("Kon geen bestaand proefsleufblad vinden om het sjabloon uit te breiden.")

        source_page_slots = pages[-1]
        source_bounds = self._template_page_content_bounds(source_page_slots)
        dx, dy = self._template_page_translation(pages)
        source_entities = [
            entity
            for entity in document.modelspace()
            if self._template_page_contains_entity(source_bounds, entity)
        ]
        if not source_entities:
            raise CadastralExportError("Kon geen modelspace-onderdelen van het laatste proefsleufblad vinden om te kopieren.")

        modelspace = document.modelspace()
        for entity in source_entities:
            copied_entity = entity.copy()
            copied_entity.translate(dx, dy, 0.0)
            modelspace.add_entity(copied_entity)
        return dx, dy

    def _template_slot_pages(self, slots: list[TemplateSlot]) -> list[list[TemplateSlot]]:
        return [
            slots[index : index + self.TEMPLATE_SLOTS_PER_LAYOUT]
            for index in range(0, len(slots), self.TEMPLATE_SLOTS_PER_LAYOUT)
            if slots[index : index + self.TEMPLATE_SLOTS_PER_LAYOUT]
        ]

    def _template_page_content_bounds(self, page_slots: list[TemplateSlot]) -> Bounds:
        combined = page_slots[0].row_box
        for slot in page_slots[1:]:
            combined = combined.union(slot.row_box)
        return combined.padded(self.TEMPLATE_PAGE_CONTENT_MARGIN)

    def _template_page_translation(self, pages: list[list[TemplateSlot]]) -> tuple[float, float]:
        last_anchor = self._template_page_anchor(pages[-1])
        if len(pages) >= 2:
            previous_anchor = self._template_page_anchor(pages[-2])
            return last_anchor[0] - previous_anchor[0], last_anchor[1] - previous_anchor[1]

        source_bounds = self._template_page_content_bounds(pages[-1])
        return 0.0, -max(source_bounds.height * 2.0, 25.0)

    def _template_page_anchor(self, page_slots: list[TemplateSlot]) -> tuple[float, float]:
        min_x = min(slot.row_box.min_x for slot in page_slots)
        min_y = min(slot.row_box.min_y for slot in page_slots)
        return min_x, min_y

    def _template_page_contains_entity(self, page_bounds: Bounds, entity) -> bool:
        anchor = self._template_page_entity_anchor(entity)
        if anchor is None:
            return False
        return page_bounds.contains(anchor[0], anchor[1])

    def _template_page_entity_anchor(self, entity) -> tuple[float, float] | None:
        entity_type = entity.dxftype()
        if entity_type == "TEXT":
            return float(entity.dxf.insert.x), float(entity.dxf.insert.y)
        if entity_type == "MTEXT":
            return float(entity.dxf.insert.x), float(entity.dxf.insert.y)
        if entity_type == "LINE":
            return (
                (float(entity.dxf.start.x) + float(entity.dxf.end.x)) / 2.0,
                (float(entity.dxf.start.y) + float(entity.dxf.end.y)) / 2.0,
            )
        if entity_type == "LWPOLYLINE":
            bounds = self._polyline_bounds(entity)
            if bounds is None:
                return None
            return bounds.center_x, bounds.center_y
        return None

    def _plain_mtext(self, entity) -> str:
        return str(entity.text).replace("\\P", " ").strip()

    def _ensure_template_image_layer(self, document: ezdxf.EzDxfDocument) -> None:
        if "PROEFSLEUF_TEMPLATE_IMAGES" not in document.layers:
            document.layers.add(name="PROEFSLEUF_TEMPLATE_IMAGES", dxfattribs={"color": 7})

    def _update_template_scale_list(self, document: ezdxf.EzDxfDocument) -> None:
        try:
            scale_dict = document.rootdict["ACAD_SCALELIST"]
        except Exception:
            return

        desired_entries = {
            "A0": ("1:2000", 2000),
            "A1": ("1:1000", 1000),
            "A2": ("1:500", 500),
            "A3": ("1:250", 250),
            "A8": ("1:5000", 5000),
            "A9": ("1:10000", 10000),
        }

        for key in list(scale_dict.keys()):
            if key not in desired_entries:
                scale_dict.discard(key)
                continue
            scale_object = scale_dict.get(key)
            if scale_object is None:
                continue
            display_name, denominator = desired_entries[key]
            self._set_scale_object(scale_object, display_name, denominator)

    def _set_scale_object(self, scale_object, display_name: str, denominator: int) -> None:
        drawing_units = float(denominator) / 1000.0
        tags = scale_object.xtags.subclasses[1]
        for index, tag in enumerate(tags):
            if tag.code == 300:
                tags[index] = DXFTag(300, display_name)
            elif tag.code == 140:
                tags[index] = DXFTag(140, 1.0)
            elif tag.code == 141:
                tags[index] = DXFTag(141, drawing_units)
            elif tag.code == 290:
                tags[index] = DXFTag(290, 0)

    def _template_layout_name(self, page_number: int) -> str:
        return f"{self.TEMPLATE_LAYOUT_NAME_PREFIX}{page_number}"

    def _used_template_layout_names(self, filled_slot_count: int) -> list[str]:
        total_pages = max(1, (filled_slot_count + self.TEMPLATE_SLOTS_PER_LAYOUT - 1) // self.TEMPLATE_SLOTS_PER_LAYOUT)
        return [self._template_layout_name(page_number) for page_number in range(1, total_pages + 1)]

    def _ensure_template_layout_exists(
        self,
        document: ezdxf.EzDxfDocument,
        page_number: int,
        main_viewport_translation: tuple[float, float] = (0.0, 0.0),
    ) -> None:
        layout_name = self._template_layout_name(page_number)
        try:
            document.layouts.get(layout_name)
            return
        except Exception:
            pass

        source_layout = self._previous_template_layout(document, page_number - 1)
        if source_layout is None:
            raise CadastralExportError(f"Kon geen bronlayout vinden om {layout_name} aan te maken.")

        dxfattribs = source_layout.dxf.all_existing_dxf_attribs()
        for key in ("handle", "owner", "name", "block_record_handle", "viewport_handle", "taborder"):
            dxfattribs.pop(key, None)
        target_layout = document.layouts.new(layout_name, dxfattribs=dxfattribs)
        self._copy_template_layout_entities(
            source_layout,
            target_layout,
            main_viewport_translation=main_viewport_translation,
        )

    def _previous_template_layout(self, document: ezdxf.EzDxfDocument, page_number: int):
        for candidate in range(page_number, 0, -1):
            layout_name = self._template_layout_name(candidate)
            try:
                return document.layouts.get(layout_name)
            except Exception:
                continue
        return None

    def _copy_template_layout_entities(
        self,
        source_layout,
        target_layout,
        main_viewport_translation: tuple[float, float] = (0.0, 0.0),
    ) -> None:
        handle_map: dict[str, str] = {}
        viewport_copies: list[tuple[object, object]] = []
        for entity in source_layout:
            copied_entity = entity.copy()
            target_layout.add_entity(copied_entity)
            source_handle = str(getattr(entity.dxf, "handle", "") or "").strip()
            copied_handle = str(getattr(copied_entity.dxf, "handle", "") or "").strip()
            if source_handle and copied_handle:
                handle_map[source_handle] = copied_handle
            if entity.dxftype() == "VIEWPORT":
                viewport_copies.append((entity, copied_entity))

        for source_viewport, copied_viewport in viewport_copies:
            source_clip_handle = str(getattr(source_viewport.dxf, "clipping_boundary_handle", "") or "").strip()
            if not source_clip_handle:
                continue
            target_clip_handle = handle_map.get(source_clip_handle)
            if target_clip_handle:
                copied_viewport.dxf.clipping_boundary_handle = target_clip_handle

        target_main_viewport = self._find_template_main_content_viewport(target_layout)
        if target_main_viewport is not None and (abs(main_viewport_translation[0]) > 1e-9 or abs(main_viewport_translation[1]) > 1e-9):
            view_center = target_main_viewport.dxf.view_center_point
            target_main_viewport.dxf.view_center_point = (
                float(view_center.x) + float(main_viewport_translation[0]),
                float(view_center.y) + float(main_viewport_translation[1]),
                float(view_center.z),
            )

    def _ensure_template_layout_legends(self, document: ezdxf.EzDxfDocument, layout_names: list[str]) -> None:
        if not layout_names:
            return
        try:
            source_layout = document.layouts.get(self.TEMPLATE_LOGO_LAYOUT_NAME)
        except Exception:
            return

        source_entities = self._template_layout_legend_entities(source_layout)
        if not source_entities:
            return

        for layout_name in layout_names[1:]:
            try:
                target_layout = document.layouts.get(layout_name)
            except Exception:
                continue
            if self._find_template_layout_insert(target_layout) is not None:
                continue
            self._copy_template_layout_legend(source_entities, target_layout)

    def _template_layout_legend_entities(self, layout) -> list:
        entities: list = []
        started = False
        for entity in layout:
            if not started and entity.dxftype() == "INSERT" and entity.dxf.layer == "X-XX-AL-LEGENDA":
                started = True
            if started:
                entities.append(entity)
        return entities

    def _copy_template_layout_legend(self, source_entities: list, target_layout) -> None:
        for entity in source_entities:
            if entity.dxftype() == "INSERT" and entity.dxf.layer == "X-XX-AL-LEGENDA":
                self._copy_template_layout_insert(entity, target_layout)
                continue
            target_layout.add_entity(entity.copy())

    def _copy_template_layout_insert(self, source_insert, target_layout) -> None:
        copied_insert = target_layout.add_blockref(
            source_insert.dxf.name,
            source_insert.dxf.insert,
            dxfattribs={
                "layer": source_insert.dxf.layer,
                "xscale": source_insert.dxf.xscale,
                "yscale": source_insert.dxf.yscale,
                "zscale": source_insert.dxf.zscale,
                "rotation": source_insert.dxf.rotation,
            },
        )
        copied_insert.add_auto_attribs({attrib.dxf.tag: attrib.dxf.text for attrib in source_insert.attribs})

    def _restore_template_layout_logos(
        self,
        document: ezdxf.EzDxfDocument,
        asset_dir: Path,
        layout_names: list[str],
        techbase_logo_path: str | Path | None,
        client_logo_path: str | Path | None,
    ) -> None:
        for layout_name in layout_names:
            try:
                layout = document.layouts.get(layout_name)
            except Exception:
                continue

            title_block = self._find_template_layout_insert(layout)
            if title_block is None:
                continue

            if techbase_logo_path is not None:
                prepared = self._prepare_template_layout_logo(asset_dir, techbase_logo_path, f"{layout_name.lower()}_techbase_logo")
                if prepared is not None:
                    self._add_box_image(
                        document,
                        layout,
                        prepared,
                        self._transform_local_template_bounds(title_block, self.TEMPLATE_TECHBASE_LOGO_LOCAL_BOX),
                        f"{layout_name.upper()}_TECHBASE_LOGO",
                        inset=0.0,
                    )

            if client_logo_path is not None:
                prepared = self._prepare_template_layout_logo(asset_dir, client_logo_path, f"{layout_name.lower()}_opdrachtgever_logo")
                if prepared is not None:
                    self._add_box_image(
                        document,
                        layout,
                        prepared,
                        self._transform_local_template_bounds(title_block, self._template_client_logo_local_box()),
                        f"{layout_name.upper()}_OPDRACHTGEVER_LOGO",
                        inset=0.0,
                    )

    def _restore_template_layout_contact_text(self, document: ezdxf.EzDxfDocument) -> None:
        replacements = {
            "De Diepteweg 16": self.TEMPLATE_CONTACT_STREET,
            "5236 PV": self.TEMPLATE_CONTACT_POSTCODE,
            "'s-Hertogenbosch": self.TEMPLATE_CONTACT_CITY,
        }
        for entity in document.entitydb.values():
            if entity.dxftype() != "TEXT":
                continue
            current_text = str(entity.dxf.text)
            if current_text not in replacements:
                continue
            entity.dxf.text = replacements[current_text]

    def _update_template_layout_page_numbers(self, document: ezdxf.EzDxfDocument, layout_names: list[str]) -> None:
        total_pages = len(layout_names)
        for page_number, layout_name in enumerate(layout_names, start=1):
            try:
                layout = document.layouts.get(layout_name)
            except Exception:
                continue
            title_block = self._find_template_layout_insert(layout)
            if title_block is None:
                continue
            for attrib in title_block.attribs:
                if attrib.dxf.tag == "BLAD":
                    attrib.dxf.text = str(page_number)
                elif attrib.dxf.tag == "BLADEN":
                    attrib.dxf.text = str(total_pages)

    def _update_template_title_block_attributes(
        self,
        document: ezdxf.EzDxfDocument,
        layout_names: list[str],
        *,
        template_drawn_by: str,
    ) -> None:
        values = {
            "DATUM1": datetime.now().strftime("%d-%m-%Y"),
            "OMSCHRIJVING1": "Proefsleuven",
            "TEKENAAR1": str(template_drawn_by or "").strip(),
        }
        for layout_name in layout_names:
            try:
                layout = document.layouts.get(layout_name)
            except Exception:
                continue
            for entity in layout:
                if entity.dxftype() != "INSERT":
                    continue
                for attrib in getattr(entity, "attribs", []):
                    tag = str(attrib.dxf.tag or "").strip().upper()
                    if tag in values:
                        attrib.dxf.text = values[tag]

    def _configure_template_wireframe_viewports(
        self,
        document: ezdxf.EzDxfDocument,
        tiff_layers: list[TemplateSlotLayer],
        layout_names: list[str],
        label_gap: float,
    ) -> None:
        for page_number, layout_name in enumerate(layout_names, start=1):
            try:
                layout = document.layouts.get(layout_name)
            except Exception:
                continue
            viewport = self._find_template_wireframe_viewport(layout)
            title_block = self._find_template_layout_insert(layout)
            if viewport is None or title_block is None:
                continue
            page_layers = self._template_layout_page_layers(tiff_layers, page_number)
            if not page_layers:
                continue
            scale_denominator = self._fit_template_wireframe_viewport(viewport, page_layers, label_gap)
            for attrib in title_block.attribs:
                if attrib.dxf.tag == "SCHAAL":
                    attrib.dxf.text = f"1:{scale_denominator}"

    def _find_template_wireframe_viewport(self, layout):
        candidates = [entity for entity in layout if entity.dxftype() == "VIEWPORT" and entity.dxf.layer == "KADER"]
        if not candidates:
            return None
        # The sheet overview next to the legend is the lower KADER viewport.
        return min(candidates, key=lambda entity: (float(entity.dxf.center[1]), float(entity.dxf.height)))

    def _find_template_main_content_viewport(self, layout):
        candidates = [entity for entity in layout if entity.dxftype() == "VIEWPORT" and entity.dxf.layer == "KADER"]
        if not candidates:
            return None
        return max(candidates, key=lambda entity: (float(entity.dxf.center[1]), float(entity.dxf.height)))

    def _template_layout_page_layers(self, tiff_layers: list[TemplateSlotLayer], page_number: int) -> list[GeoTiffLayer]:
        start_index = (page_number - 1) * self.TEMPLATE_SLOTS_PER_LAYOUT
        end_index = start_index + self.TEMPLATE_SLOTS_PER_LAYOUT
        return [layer for layer in tiff_layers[start_index:end_index] if layer is not None]

    def _fit_template_wireframe_viewport(self, viewport, page_layers: list[GeoTiffLayer], label_gap: float) -> int:
        bounds = self._combined_bounds(page_layers)
        padding = max(2.0, float(label_gap) + (self.LABEL_HEIGHT * 2.0))
        padded_bounds = bounds.padded(padding)
        scale_denominator = self._choose_template_wireframe_scale(
            padded_bounds,
            float(viewport.dxf.width),
            float(viewport.dxf.height),
        )
        viewport.dxf.view_target_point = (0.0, 0.0, 0.0)
        viewport.dxf.view_center_point = (padded_bounds.center_x, padded_bounds.center_y, 0.0)
        viewport.dxf.view_direction_vector = (0.0, 0.0, 1.0)
        viewport.dxf.view_twist_angle = 0.0
        viewport.dxf.view_height = (float(viewport.dxf.height) * scale_denominator) / 1000.0
        return scale_denominator

    def _choose_template_wireframe_scale(self, bounds: Bounds, viewport_width: float, viewport_height: float) -> int:
        for denominator in self.TEMPLATE_WIREFRAME_SCALE_DENOMINATORS:
            world_width = (viewport_width * denominator) / 1000.0
            world_height = (viewport_height * denominator) / 1000.0
            if bounds.width <= world_width and bounds.height <= world_height:
                return denominator
        return self.TEMPLATE_WIREFRAME_SCALE_DENOMINATORS[-1]

    def _find_template_layout_insert(self, layout):
        for entity in layout:
            if entity.dxftype() != "INSERT":
                continue
            return entity
        return None

    def _prepare_template_layout_logo(self, asset_dir: Path, source_path: str | Path, stem: str) -> Path | None:
        source = Path(source_path)
        if not source.exists():
            return None
        destination = self._unique_raster_copy_path(asset_dir, f"{stem}.png")
        try:
            with Image.open(source) as image:
                image.convert("RGBA").save(destination, format="PNG")
        except OSError:
            try:
                shutil.copyfile(source, destination)
            except OSError:
                return None
        return destination.resolve()

    def _transform_local_template_bounds(self, insert_entity, local_bounds: Bounds) -> Bounds:
        insert_x = float(insert_entity.dxf.insert[0])
        insert_y = float(insert_entity.dxf.insert[1])
        scale_x = float(getattr(insert_entity.dxf, "xscale", 1.0) or 1.0)
        scale_y = float(getattr(insert_entity.dxf, "yscale", 1.0) or 1.0)
        x_values = [insert_x + local_bounds.min_x * scale_x, insert_x + local_bounds.max_x * scale_x]
        y_values = [insert_y + local_bounds.min_y * scale_y, insert_y + local_bounds.max_y * scale_y]
        return Bounds(min(x_values), min(y_values), max(x_values), max(y_values))

    def _template_client_logo_local_box(self) -> Bounds:
        width, height = self.TEMPLATE_CLIENT_LOGO_PLACEHOLDER_SIZE
        cell = self.TEMPLATE_CLIENT_LOGO_LOCAL_CELL
        left = cell.min_x + self.TEMPLATE_CLIENT_LOGO_LEFT_PADDING
        bottom = cell.center_y - (height / 2.0)
        return Bounds(left, bottom, left + width, bottom + height)

    def _set_template_text(self, document: ezdxf.EzDxfDocument, handle: str, value: str) -> None:
        entity = document.entitydb.get(handle)
        if entity is None or entity.dxftype() != "TEXT":
            return
        entity.dxf.text = value

    def _set_template_mtext(self, document: ezdxf.EzDxfDocument, handle: str, value: str) -> None:
        entity = document.entitydb.get(handle)
        if entity is None or entity.dxftype() != "MTEXT":
            return
        entity.text = value

    def _kickthemap_job_id(self, layer: GeoTiffLayer) -> int | None:
        candidate = layer.metadata.get("kickthemap_job_id")
        try:
            if candidate is not None:
                return int(candidate)
        except (TypeError, ValueError):
            pass
        match = re.search(r"_(\d+)$", layer.path.stem)
        if match is None:
            return None
        try:
            return int(match.group(1))
        except ValueError:
            return None

    def _add_template_cross_section(
        self,
        document: ezdxf.EzDxfDocument,
        modelspace,
        slot: TemplateSlot,
        layer: GeoTiffLayer,
        label: str,
        profile: TemplateCrossSectionProfile,
        show_profile_direction: bool,
        use_custom_maaiveld_points: bool,
        bgt_surface_features: list[BgtSurfaceFeature],
        avoid_multileader_collisions: bool,
        clip_markers_to_profile: bool,
        cross_section_marker_diameter: float,
        manual_scale_denominator: int | None,
        cross_section_diameter_text: str,
    ) -> None:
        marker_scale = max(0.005, float(cross_section_marker_diameter))

        row_bottom_y = slot.tiff_box.min_y - self.TEMPLATE_PROFILE_ROW_BOTTOM_TO_TIFF_OFFSET
        start_x = slot.tiff_box.min_x - self.TEMPLATE_PROFILE_START_TO_TIFF_OFFSET
        left_edge_x = start_x - self.TEMPLATE_PROFILE_LEFT_EXTENSION
        reference_y = slot.tiff_box.min_y - self.TEMPLATE_PROFILE_REFERENCE_Y_TO_TIFF_OFFSET
        available_profile_width = slot.tiff_box.min_x - self.TEMPLATE_PROFILE_RIGHT_TEXT_MARGIN - start_x
        if available_profile_width <= 1e-6:
            return
        fitted_horizontal_scale = min(1.0, available_profile_width / max(profile.axis_length, 1e-9))
        fitted_vertical_scale = self._template_cross_section_vertical_fit_scale(profile, slot, reference_y)
        if manual_scale_denominator is None:
            scale_denominator = self._template_cross_section_scale_denominator(
                min(fitted_horizontal_scale, fitted_vertical_scale)
            )
        else:
            scale_denominator = max(20, min(5000, int(manual_scale_denominator)))
        horizontal_scale = 20.0 / float(scale_denominator)
        vertical_scale = horizontal_scale
        display_axis_length = profile.axis_length * horizontal_scale
        profile_end_x = start_x + display_axis_length

        self._ensure_template_profile_layers(document, profile.points)
        start_z = float(profile.start_point.z or 0.0)
        end_z = float(profile.end_point.z or 0.0)
        terrain_start_y = reference_y + ((start_z - profile.reference_level) * vertical_scale)
        terrain_end_y = reference_y + ((end_z - profile.reference_level) * vertical_scale)
        maaiveld_text = f"Maaiveld: {start_z:.2f} N.A.P."
        profile_id = self._profile_identifier(label)
        profile_scale_text = f"Schaal: 1:{scale_denominator}"
        band_pairs: list[dict[str, object]] = []
        leader_entries: list[dict[str, object]] = []
        leader_hard_segments: list[tuple[float, float, float, float]] = []
        leader_soft_segments: list[tuple[float, float, float, float]] = []
        leader_text_obstacles: list[tuple[float, float, float, float]] = []
        top_band_y = slot.tiff_box.min_y - self.TEMPLATE_PROFILE_TOP_BAND_Y_TO_TIFF_OFFSET
        band_key_counter = 0
        fill_segments = self._template_maaiveld_fill_segments(
            layer,
            use_custom_values=use_custom_maaiveld_points,
            automatic_values=self._template_bgt_surface_texts(profile, bgt_surface_features),
        )

        self._insert_template_block(
            modelspace,
            "SAL-VERWIJZING_PROFIELRASTER_TITEL_LINKS-SOD",
            (start_x, slot.tiff_box.min_y - self.TEMPLATE_PROFILE_TITLE_Y_TO_TIFF_OFFSET),
            layer_name="X-XX-AL-PROFIELBALK-SD",
            attributes={
                "NUMMER": profile_id,
                "GROEP": "Proefsleuven",
                "OMSCHRIJVING": self._profile_description(label),
                "SCHAAL": profile_scale_text,
            },
        )
        self._insert_template_block(
            modelspace,
            "SAL-VERWIJZING_PROFIELRASTER_TITEL_RECHTS-SOD",
            (profile_end_x, slot.tiff_box.min_y - self.TEMPLATE_PROFILE_TITLE_Y_TO_TIFF_OFFSET),
            layer_name="X-XX-AL-PROFIELBALK-SD",
        )
        self._insert_template_block(
            modelspace,
            "SAL-VERWIJZING_PROFIELRASTER_HOOGTEREFERENTIE_ONDER-SOD",
            (start_x, reference_y),
            layer_name="X-XX-AL-PROFIELBALK-SD",
            attributes={"REFERENTIE": f"{profile.reference_level:.2f} N.A.P."},
        )
        reference_chainage = self._template_reference_chainage(layer, profile) if show_profile_direction else 0.0
        distance_description = self._template_profile_reference_rd_text(layer, profile) if show_profile_direction else ""
        self._insert_template_block(
            modelspace,
            "SAL-VERWIJZING_PROFIELRASTER_BAND_TITEL_LINKS-SOD",
            (start_x, slot.tiff_box.min_y - self.TEMPLATE_PROFILE_TOP_BAND_Y_TO_TIFF_OFFSET),
            layer_name="X-XX-AL-PROFIELBALK-SD",
            attributes={"TITEL": "Afstand", "OMSCHRIJVING": distance_description},
        )
        self._insert_template_block(
            modelspace,
            "SAL-VERWIJZING_PROFIELRASTER_BAND_TITEL_RECHTS-SOD",
            (profile_end_x, slot.tiff_box.min_y - self.TEMPLATE_PROFILE_TOP_BAND_Y_TO_TIFF_OFFSET),
            layer_name="X-XX-AL-PROFIELBALK-SD",
            attributes={"TITEL": "Afstand", "OMSCHRIJVING": ""},
        )
        self._insert_template_block(
            modelspace,
            "SAL-VERWIJZING_PROFIELRASTER_BAND_TITEL_LINKS-SOD",
            (start_x, slot.tiff_box.min_y - self.TEMPLATE_PROFILE_BOTTOM_BAND_Y_TO_TIFF_OFFSET),
            layer_name="X-XX-AL-PROFIELBALK-SD",
            attributes={"TITEL": "N.A.P.", "OMSCHRIJVING": ""},
        )
        self._insert_template_block(
            modelspace,
            "SAL-VERWIJZING_PROFIELRASTER_BAND_TITEL_RECHTS-SOD",
            (profile_end_x, slot.tiff_box.min_y - self.TEMPLATE_PROFILE_BOTTOM_BAND_Y_TO_TIFF_OFFSET),
            layer_name="X-XX-AL-PROFIELBALK-SD",
            attributes={"TITEL": "N.A.P.", "OMSCHRIJVING": ""},
        )

        for band_y in (
            reference_y,
            slot.tiff_box.min_y - self.TEMPLATE_PROFILE_TOP_BAND_Y_TO_TIFF_OFFSET,
            slot.tiff_box.min_y - self.TEMPLATE_PROFILE_BOTTOM_BAND_Y_TO_TIFF_OFFSET,
            row_bottom_y,
        ):
            modelspace.add_lwpolyline(
                [(start_x, band_y), (profile_end_x, band_y)],
                dxfattribs={
                    "layer": "X-XX-AL-PROFIELBALK-SD",
                    "lineweight": self.TEMPLATE_PROFILE_LINEWEIGHT,
                },
            )
            leader_hard_segments.append((start_x, band_y, profile_end_x, band_y))

        modelspace.add_lwpolyline(
            [(left_edge_x, terrain_start_y), (start_x, terrain_start_y)],
            dxfattribs={
                "layer": "0",
                "lineweight": self.TEMPLATE_PROFILE_LINEWEIGHT,
                "true_color": rgb2int(tuple(fill_segments["start"]["rgb"])),
            },
        )
        leader_soft_segments.append((left_edge_x, terrain_start_y, start_x, terrain_start_y))
        modelspace.add_lwpolyline(
            [(profile_end_x, terrain_end_y), (slot.tiff_box.min_x, terrain_end_y)],
            dxfattribs={
                "layer": "0",
                "lineweight": self.TEMPLATE_PROFILE_LINEWEIGHT,
                "true_color": rgb2int(tuple(fill_segments["end"]["rgb"])),
            },
        )
        leader_soft_segments.append((profile_end_x, terrain_end_y, slot.tiff_box.min_x, terrain_end_y))
        modelspace.add_lwpolyline(
            [
                (start_x, terrain_start_y),
                (profile_end_x, terrain_end_y),
            ],
            dxfattribs={
                "layer": "B-WE-OG-TERREIN_PROFIELLIJN-GD",
                "lineweight": self.TEMPLATE_PROFILE_LINEWEIGHT,
                "true_color": rgb2int(tuple(fill_segments["middle"]["rgb"])),
            },
        )
        leader_soft_segments.append((start_x, terrain_start_y, profile_end_x, terrain_end_y))
        modelspace.add_text(
            maaiveld_text,
            dxfattribs={
                "layer": "0",
                "height": self.TEMPLATE_PROFILE_MAAIVELD_TEXT_HEIGHT,
                "style": self.CADASTRAL_LABEL_STYLE,
            },
        ).set_placement(
            (left_edge_x, terrain_start_y + self.TEMPLATE_PROFILE_MAAIVELD_TEXT_HEIGHT * 0.45),
            align=TextEntityAlignment.LEFT,
        )
        maaiveld_box = self._left_aligned_text_box(
            maaiveld_text,
            (left_edge_x, terrain_start_y + self.TEMPLATE_PROFILE_MAAIVELD_TEXT_HEIGHT * 0.45),
            self.TEMPLATE_PROFILE_MAAIVELD_TEXT_HEIGHT,
        )
        leader_text_obstacles.append(maaiveld_box)
        fill_box = self._add_template_fill_text(
            modelspace,
            (left_edge_x, terrain_start_y),
            (start_x, terrain_start_y),
            text=str(fill_segments["start"]["text"]),
            avoid_box=maaiveld_box,
        )
        if fill_box is not None:
            leader_text_obstacles.append(fill_box)
        fill_box = self._add_template_fill_text(
            modelspace,
            (start_x, terrain_start_y),
            (profile_end_x, terrain_end_y),
            text=str(fill_segments["middle"]["text"]),
        )
        if fill_box is not None:
            leader_text_obstacles.append(fill_box)
        fill_box = self._add_template_fill_text(
            modelspace,
            (profile_end_x, terrain_end_y),
            (slot.tiff_box.min_x, terrain_end_y),
            text=str(fill_segments["end"]["text"]),
        )
        if fill_box is not None:
            leader_text_obstacles.append(fill_box)
        self._add_template_soil_box(
            modelspace,
            start_x=start_x,
            top_y=terrain_start_y,
            bottom_y=(slot.tiff_box.min_y - self.TEMPLATE_PROFILE_TITLE_Y_TO_TIFF_OFFSET)
            + self.TEMPLATE_PROFILE_SOIL_BOTTOM_OFFSET,
        )
        if show_profile_direction:
            reference_segments, reference_boxes = self._add_template_profile_reference_indicator(
                modelspace,
                layer,
                profile,
                start_x=start_x,
                left_edge_x=left_edge_x,
                right_edge_x=slot.tiff_box.min_x,
                profile_end_x=profile_end_x,
                horizontal_scale=horizontal_scale,
                terrain_start_y=terrain_start_y,
                terrain_end_y=terrain_end_y,
            )
            leader_soft_segments.extend(reference_segments)
            leader_text_obstacles.extend(reference_boxes)

        for profile_point in profile.points:
            point = profile_point.point
            point_z = profile_point.point_z
            base_x_coord = start_x + (profile_point.chainage * horizontal_scale)
            y_coord = reference_y + ((float(point_z) - profile.reference_level) * vertical_scale)
            if profile_point.is_endpoint or isinstance(profile_point.point, KickTheMapObjectPolyline):
                point_marker_scale = marker_scale
                profile_instance_x_values = (base_x_coord,)
            else:
                point_marker_scale = self._cross_section_point_marker_scale(profile_point.point, marker_scale)
                profile_instance_x_values = self._template_profile_marker_x_values(
                    profile_point.point,
                    base_x_coord,
                    point_marker_scale,
                )

            for x_coord in profile_instance_x_values:
                modelspace.add_lwpolyline(
                    [(x_coord, reference_y), (x_coord, y_coord)],
                    dxfattribs={
                        "layer": "X-XX-AL-PROFIELBALK-SD",
                        "lineweight": self.TEMPLATE_PROFILE_LINEWEIGHT,
                    },
                )
                leader_hard_segments.append((x_coord, reference_y, x_coord, y_coord))
                top_band_ref = self._insert_template_block(
                    modelspace,
                    "SAL-VERWIJZING_PROFIELRASTER_BAND_WAARDE-SOD",
                    (x_coord, top_band_y),
                    layer_name="X-XX-AL-PROFIELBALK-SD",
                    attributes={"WAARDE": self._format_template_profile_distance(profile_point.chainage - reference_chainage)},
                )
                bottom_band_y = slot.tiff_box.min_y - self.TEMPLATE_PROFILE_BOTTOM_BAND_Y_TO_TIFF_OFFSET
                bottom_band_ref = self._insert_template_block(
                    modelspace,
                    "SAL-VERWIJZING_PROFIELRASTER_BAND_WAARDE-SOD",
                    (x_coord, bottom_band_y),
                    layer_name="X-XX-AL-PROFIELBALK-SD",
                    attributes={"WAARDE": f"{float(point_z):.2f}"},
                )
                top_band_attr = self._template_block_attribute(top_band_ref, "WAARDE")
                bottom_band_attr = self._template_block_attribute(bottom_band_ref, "WAARDE")
                if top_band_attr is not None and bottom_band_attr is not None:
                    band_key_counter += 1
                    band_pair = {
                        "band_key": band_key_counter,
                        "anchor_x": x_coord,
                        "pin_to_anchor": bool(profile_point.is_endpoint),
                        "top_ref": top_band_ref,
                        "top_attr": top_band_attr,
                        "top_y": top_band_y,
                        "top_offset_x": float(top_band_attr.dxf.insert.x) - float(top_band_ref.dxf.insert.x),
                        "top_offset_y": float(top_band_attr.dxf.insert.y) - float(top_band_ref.dxf.insert.y),
                        "top_height": float(getattr(top_band_attr.dxf, "height", self.TEMPLATE_PROFILE_BAND_VALUE_TEXT_HEIGHT)),
                        "bottom_ref": bottom_band_ref,
                        "bottom_attr": bottom_band_attr,
                        "bottom_y": bottom_band_y,
                        "bottom_offset_x": float(bottom_band_attr.dxf.insert.x) - float(bottom_band_ref.dxf.insert.x),
                        "bottom_offset_y": float(bottom_band_attr.dxf.insert.y) - float(bottom_band_ref.dxf.insert.y),
                        "bottom_height": float(getattr(bottom_band_attr.dxf, "height", self.TEMPLATE_PROFILE_BAND_VALUE_TEXT_HEIGHT)),
                    }
                    band_pairs.append(band_pair)
                else:
                    band_pair = None
                if profile_point.is_endpoint:
                    continue
                depth = max(0.0, start_z - float(point_z))
                if isinstance(profile_point.point, KickTheMapObjectPolyline):
                    polyline_vertices = self._cross_section_polyline_profile_vertices(
                        profile_point.point,
                        profile,
                        start_x,
                        reference_y,
                        horizontal_scale=horizontal_scale,
                        vertical_scale=vertical_scale,
                    )
                    if len(polyline_vertices) >= 2:
                        modelspace.add_lwpolyline(
                            polyline_vertices,
                            dxfattribs={
                                "layer": profile_point.layer_name,
                                "color": profile_point.color,
                                "lineweight": self.TEMPLATE_PROFILE_LINEWEIGHT,
                            },
                        )
                        for first_vertex, second_vertex in zip(polyline_vertices, polyline_vertices[1:]):
                            leader_hard_segments.append(
                                (first_vertex[0], first_vertex[1], second_vertex[0], second_vertex[1])
                            )
                else:
                    self._add_template_profile_leader_marker(
                        modelspace,
                        insert_x=x_coord,
                        insert_y=y_coord,
                        marker_scale=point_marker_scale,
                        layer_name=profile_point.layer_name,
                        color=profile_point.color,
                        clip_left=start_x if clip_markers_to_profile else None,
                        clip_right=profile_end_x if clip_markers_to_profile else None,
                        clip_bottom=reference_y if clip_markers_to_profile else None,
                    )
                leader_entries.append(
                    {
                        "band_key": None if band_pair is None else band_pair["band_key"],
                        "anchor_x": x_coord,
                        "leader_line_start_y": y_coord,
                        "leader_block_start_y": y_coord + (1.0 * marker_scale),
                        "leader_top_y": y_coord + (10.0 * marker_scale),
                        "leader_marker_scale": point_marker_scale,
                        "band_y": top_band_y,
                        "band_connector_y": top_band_y + 0.04,
                        "leader_scale": marker_scale,
                        "leader_layer_name": profile_point.layer_name,
                        "leader_color": profile_point.color,
                        "description": self._cross_section_leader_description(
                            profile_point.point,
                            profile_point.description,
                            cross_section_diameter_text,
                        ),
                        "depth_text": f"Diepte: {depth:.2f}",
                    }
                )

        self._distribute_template_band_labels(
            modelspace,
            band_pairs,
            min_x=start_x,
            max_x=profile_end_x,
        )
        placed_band_x = {
            int(item["band_key"]): float(item.get("placed_x", item["anchor_x"]))
            for item in band_pairs
            if item.get("band_key") is not None
        }
        for entry in leader_entries:
            band_key = entry.get("band_key")
            entry["band_x"] = placed_band_x.get(int(band_key), float(entry["anchor_x"])) if band_key is not None else float(entry["anchor_x"])
        self._distribute_template_leader_labels(
            document,
            modelspace,
            leader_entries,
            marker_scale=marker_scale,
            hard_static_line_segments=leader_hard_segments,
            soft_static_line_segments=leader_soft_segments,
            static_text_boxes=leader_text_obstacles,
            min_text_x=start_x,
            max_text_x=profile_end_x,
            avoid_collisions=avoid_multileader_collisions,
        )

    def _template_cross_section_vertical_fit_scale(
        self,
        profile: TemplateCrossSectionProfile,
        slot: TemplateSlot,
        reference_y: float,
    ) -> float:
        max_profile_z = self._template_cross_section_profile_max_z(profile)
        profile_height = max(0.0, max_profile_z - float(profile.reference_level))
        if profile_height <= 1e-9:
            return 1.0
        top_padding = max(0.08, self.TEMPLATE_PROFILE_MAAIVELD_TEXT_HEIGHT * 2.0)
        max_top_y = float(slot.row_box.max_y) - top_padding
        available_height = max_top_y - float(reference_y)
        if available_height <= 1e-9:
            return 1.0
        return max(0.01, min(1.0, available_height / profile_height))

    def _template_cross_section_profile_max_z(self, profile: TemplateCrossSectionProfile) -> float:
        max_z = max(
            float(profile.start_point.z or profile.reference_level),
            float(profile.end_point.z or profile.reference_level),
            *(float(point.point_z) for point in profile.points),
        )
        for profile_point in profile.points:
            if not isinstance(profile_point.point, KickTheMapObjectPolyline):
                continue
            for vertex in profile_point.point.vertices:
                if vertex.z is None:
                    continue
                max_z = max(max_z, float(vertex.z))
        return max_z

    def _template_cross_section_scale_text(self, horizontal_scale: float) -> str:
        scale_denominator = self._template_cross_section_scale_denominator(horizontal_scale)
        return f"Schaal: 1:{scale_denominator}"

    def _template_cross_section_scale_denominator(self, horizontal_scale: float) -> int:
        normalized_scale = max(1e-6, float(horizontal_scale))
        raw_denominator = max(20.0, 20.0 / normalized_scale)
        return max(20, int(ceil(raw_denominator / 20.0) * 20))

    def _add_template_profile_reference_indicator(
        self,
        modelspace,
        layer: GeoTiffLayer,
        profile: TemplateCrossSectionProfile,
        *,
        start_x: float,
        left_edge_x: float,
        right_edge_x: float,
        profile_end_x: float,
        horizontal_scale: float,
        terrain_start_y: float,
        terrain_end_y: float,
    ) -> tuple[list[tuple[float, float, float, float]], list[tuple[float, float, float, float]]]:
        if horizontal_scale <= 1e-9:
            return [], []
        reference_point = self._template_reference_raw_point(layer, profile)
        if reference_point is None:
            return [], []

        reference_chainage = self._template_reference_chainage(layer, profile)
        visible_min_chainage = (float(left_edge_x) - float(start_x)) / float(horizontal_scale)
        visible_max_chainage = float(profile.axis_length) + ((float(right_edge_x) - float(profile_end_x)) / float(horizontal_scale))
        line_segments: list[tuple[float, float, float, float]] = []
        obstacle_boxes: list[tuple[float, float, float, float]] = []

        marker_radius = self.TEMPLATE_PROFILE_REFERENCE_MARKER_RADIUS
        layer_name = "X-XX-AL-PROFIELBALK-SD"
        color = self.TEMPLATE_PROFILE_LEADER_LINE_COLOR
        label_text = "Referentiepunt"

        if visible_min_chainage <= reference_chainage <= visible_max_chainage:
            marker_x = float(start_x) + (float(reference_chainage) * float(horizontal_scale))
            marker_y = self._template_profile_terrain_y(
                float(reference_chainage),
                profile,
                terrain_start_y=terrain_start_y,
                terrain_end_y=terrain_end_y,
            )
            modelspace.add_circle(
                center=(marker_x, marker_y),
                radius=marker_radius,
                dxfattribs={
                    "layer": layer_name,
                    "color": color,
                    "lineweight": self.TEMPLATE_PROFILE_LINEWEIGHT,
                },
            )
            plus_reach = marker_radius * 0.98
            modelspace.add_lwpolyline(
                [(marker_x - plus_reach, marker_y), (marker_x + plus_reach, marker_y)],
                dxfattribs={
                    "layer": layer_name,
                    "color": color,
                    "lineweight": self.TEMPLATE_PROFILE_LINEWEIGHT,
                },
            )
            modelspace.add_lwpolyline(
                [(marker_x, marker_y - plus_reach), (marker_x, marker_y + plus_reach)],
                dxfattribs={
                    "layer": layer_name,
                    "color": color,
                    "lineweight": self.TEMPLATE_PROFILE_LINEWEIGHT,
                },
            )
            obstacle_boxes.append(
                (
                    marker_x - marker_radius,
                    marker_y - marker_radius,
                    marker_x + marker_radius,
                    marker_y + marker_radius,
                )
            )
            text_width = self._estimate_text_width(label_text, self.TEMPLATE_PROFILE_REFERENCE_TEXT_HEIGHT)
            if float(reference_chainage) < 0.0:
                leader_dx = self.TEMPLATE_PROFILE_REFERENCE_LABEL_LEADER_LENGTH
                leader_dy = self.TEMPLATE_PROFILE_REFERENCE_LABEL_LEADER_RISE
                leader_length = hypot(leader_dx, leader_dy)
                leader_start_x = marker_x + ((leader_dx / leader_length) * marker_radius)
                leader_start_y = marker_y + ((leader_dy / leader_length) * marker_radius)
                leader_end_x = leader_start_x + leader_dx
                leader_end_y = leader_start_y + leader_dy
                text_insert = (
                    leader_end_x + self.TEMPLATE_PROFILE_REFERENCE_TEXT_GAP + (text_width * 0.5),
                    leader_end_y,
                )
            elif float(reference_chainage) > float(profile.axis_length):
                leader_dx = -self.TEMPLATE_PROFILE_REFERENCE_LABEL_LEADER_LENGTH
                leader_dy = self.TEMPLATE_PROFILE_REFERENCE_LABEL_LEADER_RISE
                leader_length = hypot(leader_dx, leader_dy)
                leader_start_x = marker_x + ((leader_dx / leader_length) * marker_radius)
                leader_start_y = marker_y + ((leader_dy / leader_length) * marker_radius)
                leader_end_x = leader_start_x + leader_dx
                leader_end_y = leader_start_y + leader_dy
                text_insert = (
                    leader_end_x - self.TEMPLATE_PROFILE_REFERENCE_TEXT_GAP - (text_width * 0.5),
                    leader_end_y,
                )
            else:
                terrain_dx = float(profile_end_x) - float(start_x)
                terrain_dy = float(terrain_end_y) - float(terrain_start_y)
                normal_x = -terrain_dy
                normal_y = terrain_dx
                normal_length = hypot(normal_x, normal_y)
                if normal_length <= 1e-9:
                    unit_x = 0.0
                    unit_y = 1.0
                else:
                    unit_x = normal_x / normal_length
                    unit_y = normal_y / normal_length
                    if unit_y < 0.0:
                        unit_x = -unit_x
                        unit_y = -unit_y
                if abs(unit_x) < 0.25:
                    unit_x = 0.35 if float(reference_chainage) <= (float(profile.axis_length) * 0.5) else -0.35
                    adjusted_length = hypot(unit_x, unit_y)
                    if adjusted_length > 1e-9:
                        unit_x /= adjusted_length
                        unit_y /= adjusted_length
                leader_dx = unit_x * self.TEMPLATE_PROFILE_REFERENCE_LABEL_LEADER_LENGTH
                leader_dy = unit_y * self.TEMPLATE_PROFILE_REFERENCE_LABEL_LEADER_LENGTH
                leader_length = hypot(leader_dx, leader_dy)
                if leader_length <= 1e-9:
                    leader_dx = 0.06
                    leader_dy = self.TEMPLATE_PROFILE_REFERENCE_LABEL_LEADER_RISE
                    leader_length = hypot(leader_dx, leader_dy)
                leader_start_x = marker_x + ((leader_dx / leader_length) * marker_radius)
                leader_start_y = marker_y + ((leader_dy / leader_length) * marker_radius)
                leader_end_x = leader_start_x + leader_dx
                leader_end_y = leader_start_y + leader_dy
                text_insert = (
                    (leader_start_x + leader_end_x) * 0.5,
                    max(leader_start_y, leader_end_y) + self.TEMPLATE_PROFILE_REFERENCE_MIDDLE_TEXT_OFFSET,
                )

            modelspace.add_lwpolyline(
                [(leader_start_x, leader_start_y), (leader_end_x, leader_end_y)],
                dxfattribs={
                    "layer": layer_name,
                    "color": color,
                    "lineweight": self.TEMPLATE_PROFILE_LINEWEIGHT,
                },
            )
            line_segments.append((leader_start_x, leader_start_y, leader_end_x, leader_end_y))
            text_entity = modelspace.add_text(
                label_text,
                dxfattribs={
                    "layer": layer_name,
                    "height": self.TEMPLATE_PROFILE_REFERENCE_TEXT_HEIGHT,
                    "style": self.CADASTRAL_LABEL_STYLE,
                    "color": color,
                },
            )
            text_entity.set_placement(text_insert, align=TextEntityAlignment.MIDDLE_CENTER)
            obstacle_boxes.append(
                self._centered_rotated_text_box(
                    text=label_text,
                    insert=text_insert,
                    height=self.TEMPLATE_PROFILE_REFERENCE_TEXT_HEIGHT,
                    rotation_degrees=0.0,
                )
            )
            return line_segments, obstacle_boxes

        is_left_side = float(reference_chainage) < float(visible_min_chainage)
        boundary_chainage = float(visible_min_chainage) if is_left_side else float(visible_max_chainage)
        boundary_x = float(left_edge_x) if is_left_side else float(right_edge_x)
        boundary_y = self._template_profile_terrain_y(
            boundary_chainage,
            profile,
            terrain_start_y=terrain_start_y,
            terrain_end_y=terrain_end_y,
        )
        extra_distance = (
            float(visible_min_chainage) - float(reference_chainage)
            if is_left_side
            else float(reference_chainage) - float(visible_max_chainage)
        )
        if extra_distance <= 1e-9:
            return line_segments, obstacle_boxes

        arrow_length = self.TEMPLATE_PROFILE_REFERENCE_ARROW_LENGTH
        arrow_head = self.TEMPLATE_PROFILE_REFERENCE_ARROW_HEAD
        if is_left_side:
            arrow_tail_x = boundary_x + arrow_length
            arrow_points = [
                (arrow_tail_x, boundary_y),
                (boundary_x, boundary_y),
                (boundary_x + arrow_head, boundary_y + (arrow_head * 0.7)),
                (boundary_x, boundary_y),
                (boundary_x + arrow_head, boundary_y - (arrow_head * 0.7)),
            ]
            text = f"{self._format_template_profile_distance(extra_distance)} m"
            text_width = self._estimate_text_width(text, self.TEMPLATE_PROFILE_REFERENCE_TEXT_HEIGHT)
            text_center_x = min(
                float(start_x) - (text_width * 0.5),
                arrow_tail_x + self.TEMPLATE_PROFILE_REFERENCE_TEXT_GAP + (text_width * 0.5),
            )
        else:
            arrow_tail_x = boundary_x - arrow_length
            arrow_points = [
                (arrow_tail_x, boundary_y),
                (boundary_x, boundary_y),
                (boundary_x - arrow_head, boundary_y + (arrow_head * 0.7)),
                (boundary_x, boundary_y),
                (boundary_x - arrow_head, boundary_y - (arrow_head * 0.7)),
            ]
            text = f"{self._format_template_profile_distance(extra_distance)} m"
            text_width = self._estimate_text_width(text, self.TEMPLATE_PROFILE_REFERENCE_TEXT_HEIGHT)
            text_center_x = max(
                float(profile_end_x) + (text_width * 0.5),
                arrow_tail_x - self.TEMPLATE_PROFILE_REFERENCE_TEXT_GAP - (text_width * 0.5),
            )

        modelspace.add_lwpolyline(
            arrow_points,
            dxfattribs={
                "layer": layer_name,
                "color": color,
                "lineweight": self.TEMPLATE_PROFILE_LINEWEIGHT,
            },
        )
        line_segments.append((arrow_tail_x, boundary_y, boundary_x, boundary_y))
        line_segments.append((arrow_points[2][0], arrow_points[2][1], boundary_x, boundary_y))
        line_segments.append((arrow_points[4][0], arrow_points[4][1], boundary_x, boundary_y))

        text_insert = (
            float(text_center_x),
            float(boundary_y + self.TEMPLATE_PROFILE_REFERENCE_TEXT_OFFSET),
        )
        text_entity = modelspace.add_text(
            text,
            dxfattribs={
                "layer": layer_name,
                "height": self.TEMPLATE_PROFILE_REFERENCE_TEXT_HEIGHT,
                "style": self.CADASTRAL_LABEL_STYLE,
                "color": color,
            },
        )
        text_entity.set_placement(text_insert, align=TextEntityAlignment.MIDDLE_CENTER)
        obstacle_boxes.append(
            self._centered_rotated_text_box(
                text=text,
                insert=text_insert,
                height=self.TEMPLATE_PROFILE_REFERENCE_TEXT_HEIGHT,
                rotation_degrees=0.0,
            )
        )
        return line_segments, obstacle_boxes

    def _template_profile_terrain_y(
        self,
        chainage: float,
        profile: TemplateCrossSectionProfile,
        *,
        terrain_start_y: float,
        terrain_end_y: float,
    ) -> float:
        normalized_chainage = float(chainage)
        if normalized_chainage <= 0.0:
            return float(terrain_start_y)
        if normalized_chainage >= float(profile.axis_length):
            return float(terrain_end_y)
        if float(profile.axis_length) <= 1e-9:
            return float(terrain_start_y)
        ratio = normalized_chainage / float(profile.axis_length)
        return float(terrain_start_y) + ((float(terrain_end_y) - float(terrain_start_y)) * ratio)

    def _build_template_cross_section_profile(
        self,
        dataset: KickTheMapObjectDataset,
        layer_rules: tuple[ObjectLayerRule, ...],
        road_centerline_paths: list[list[tuple[float, float]]],
        terrain_boundary_paths: list[list[tuple[float, float]]],
        fallback_marker_scale: float,
        reverse_profile_direction: bool = False,
    ) -> TemplateCrossSectionProfile | None:
        points_with_z = [point for point in dataset.points if point.z is not None]
        if len(points_with_z) < 2:
            return None

        endpoint_candidates = self._cross_section_endpoint_candidates(points_with_z, layer_rules)
        if len(endpoint_candidates) < 2:
            endpoint_candidates = points_with_z
        forced_start_point = self._dataset_forced_cross_section_start_point(dataset, endpoint_candidates)
        if forced_start_point is not None:
            start_point = forced_start_point
        else:
            start_point = min(
                endpoint_candidates,
                key=lambda point: self._road_distance_to_point(point, road_centerline_paths),
            )
        remaining_candidates = [point for point in endpoint_candidates if not self._same_job_point(point, start_point)]
        if not remaining_candidates:
            return None
        end_point = max(
            remaining_candidates,
            key=lambda point: hypot(point.x - start_point.x, point.y - start_point.y),
        )
        base_axis_dx = end_point.x - start_point.x
        base_axis_dy = end_point.y - start_point.y
        base_axis_length = hypot(base_axis_dx, base_axis_dy)
        if base_axis_length <= 1e-6:
            return None

        midpoint = (
            (float(start_point.x) + float(end_point.x)) * 0.5,
            (float(start_point.y) + float(end_point.y)) * 0.5,
        )
        preferred_projection = self._preferred_template_orientation_projection(
            midpoint,
            road_centerline_paths,
            terrain_boundary_paths,
        )
        if preferred_projection is not None and forced_start_point is None:
            preferred_chainage = (
                ((preferred_projection[0] - float(start_point.x)) * base_axis_dx)
                + ((preferred_projection[1] - float(start_point.y)) * base_axis_dy)
            ) / base_axis_length
            if preferred_chainage > (base_axis_length * 0.5):
                start_point, end_point = end_point, start_point
                base_axis_dx = end_point.x - start_point.x
                base_axis_dy = end_point.y - start_point.y
                base_axis_length = hypot(base_axis_dx, base_axis_dy)
                if base_axis_length <= 1e-6:
                    return None
        if reverse_profile_direction and forced_start_point is None:
            start_point, end_point = end_point, start_point
            base_axis_dx = end_point.x - start_point.x
            base_axis_dy = end_point.y - start_point.y
            base_axis_length = hypot(base_axis_dx, base_axis_dy)
            if base_axis_length <= 1e-6:
                return None

        unit_dx = base_axis_dx / base_axis_length
        unit_dy = base_axis_dy / base_axis_length
        features_with_z: list[KickTheMapObjectFeature] = list(points_with_z)
        features_with_z.extend(
            polyline
            for polyline in dataset.polylines
            if self._cross_section_feature_position(polyline) is not None
        )
        raw_entries: list[tuple[KickTheMapObjectFeature, float, float]] = []
        feature_chainage_ranges: list[tuple[float, float]] = []
        for point in features_with_z:
            feature_position = self._cross_section_feature_position(point)
            if feature_position is None:
                continue
            feature_x, feature_y, feature_z = feature_position
            chainage = (
                ((feature_x - start_point.x) * base_axis_dx)
                + ((feature_y - start_point.y) * base_axis_dy)
            ) / base_axis_length
            raw_entries.append((point, chainage, float(feature_z)))
            chainage_range = self._cross_section_feature_chainage_range(
                point,
                axis_start_x=float(start_point.x),
                axis_start_y=float(start_point.y),
                axis_dx=base_axis_dx,
                axis_dy=base_axis_dy,
                axis_length=base_axis_length,
                fallback_marker_scale=fallback_marker_scale,
            )
            if chainage_range is not None:
                feature_chainage_ranges.append(chainage_range)

        if not raw_entries:
            return None

        if feature_chainage_ranges:
            min_feature_chainage = min(chainage_range[0] for chainage_range in feature_chainage_ranges)
            max_feature_chainage = max(chainage_range[1] for chainage_range in feature_chainage_ranges)
        else:
            min_feature_chainage = min(entry[1] for entry in raw_entries)
            max_feature_chainage = max(entry[1] for entry in raw_entries)
        boundary_start_chainage = 0.0 if forced_start_point is not None else min(0.0, min_feature_chainage)
        boundary_end_chainage = max(base_axis_length, max_feature_chainage)
        axis_length = boundary_end_chainage - boundary_start_chainage
        if axis_length <= 1e-6:
            return None

        synthetic_start_point = KickTheMapObjectPoint(
            object_name=start_point.object_name,
            source_name=start_point.source_name,
            x=float(start_point.x + (unit_dx * boundary_start_chainage)),
            y=float(start_point.y + (unit_dy * boundary_start_chainage)),
            z=start_point.z,
            attribute_1=start_point.attribute_1,
            attribute_2=start_point.attribute_2,
            attribute_3=start_point.attribute_3,
        )
        synthetic_end_point = KickTheMapObjectPoint(
            object_name=end_point.object_name,
            source_name=end_point.source_name,
            x=float(start_point.x + (unit_dx * boundary_end_chainage)),
            y=float(start_point.y + (unit_dy * boundary_end_chainage)),
            z=end_point.z,
            attribute_1=end_point.attribute_1,
            attribute_2=end_point.attribute_2,
            attribute_3=end_point.attribute_3,
        )
        axis_dx = synthetic_end_point.x - synthetic_start_point.x
        axis_dy = synthetic_end_point.y - synthetic_start_point.y

        ordered_points: list[TemplateCrossSectionPoint] = [
            TemplateCrossSectionPoint(
                point=synthetic_start_point,
                chainage=0.0,
                point_z=float(synthetic_start_point.z or 0.0),
                layer_name="0",
                color=7,
                description=self._cross_section_description(synthetic_start_point, "0"),
                is_endpoint=True,
            ),
            TemplateCrossSectionPoint(
                point=synthetic_end_point,
                chainage=axis_length,
                point_z=float(synthetic_end_point.z or 0.0),
                layer_name="0",
                color=7,
                description=self._cross_section_description(synthetic_end_point, "0"),
                is_endpoint=True,
            ),
        ]

        for point, raw_chainage, feature_z in raw_entries:
            if self._same_job_point(point, start_point) or self._same_job_point(point, end_point):
                continue
            chainage = raw_chainage - boundary_start_chainage
            matched_rule = self._resolve_cross_section_point_rule(point, layer_rules)
            if matched_rule is None:
                if self._is_dekband_feature(point):
                    layer_name, color, profile_label = "0", self.TEMPLATE_DEKBAND_COLOR, "Dekband"
                else:
                    layer_name, color, profile_label = "0", 7, ""
            else:
                layer_name = matched_rule.target_layer
                color = matched_rule.color
                profile_label = matched_rule.profile_label
            ordered_points.append(
                TemplateCrossSectionPoint(
                    point=point,
                    chainage=chainage,
                    point_z=feature_z,
                    layer_name=layer_name,
                    color=color,
                    description=self._cross_section_description(point, layer_name, profile_label),
                    is_endpoint=False,
                )
            )
        ordered_points.sort(key=lambda item: (item.chainage, 1 if item.is_endpoint else 0, item.description))
        if not ordered_points:
            return None
        min_z = min(item.point_z for item in ordered_points)
        reference_level = round((floor(min_z * 10.0) / 10.0) - self.TEMPLATE_PROFILE_REFERENCE_MARGIN, 2)
        return TemplateCrossSectionProfile(
            start_point=synthetic_start_point,
            end_point=synthetic_end_point,
            axis_dx=axis_dx,
            axis_dy=axis_dy,
            axis_length=axis_length,
            reference_level=reference_level,
            points=tuple(ordered_points),
        )

    def _dataset_forced_cross_section_start_point(
        self,
        dataset: KickTheMapObjectDataset,
        candidates: list[KickTheMapObjectPoint],
    ) -> KickTheMapObjectPoint | None:
        forced_start_xy = dataset.cross_section_start_xy
        if forced_start_xy is None:
            return None
        try:
            target_x = float(forced_start_xy[0])
            target_y = float(forced_start_xy[1])
        except (TypeError, ValueError, IndexError):
            return None
        best_point: KickTheMapObjectPoint | None = None
        best_distance: float | None = None
        for candidate in candidates:
            distance = hypot(candidate.x - target_x, candidate.y - target_y)
            if best_distance is None or distance < best_distance:
                best_point = candidate
                best_distance = distance
        if best_distance is None or best_distance > 0.5:
            return None
        return best_point

    def _cross_section_endpoint_candidates(
        self,
        points: list[KickTheMapObjectPoint],
        layer_rules: tuple[ObjectLayerRule, ...],
    ) -> list[KickTheMapObjectPoint]:
        marker_points: list[KickTheMapObjectPoint] = []
        unmatched_points: list[KickTheMapObjectPoint] = []
        for point in points:
            layer_name, _color = self._resolve_cross_section_point_style(point, layer_rules)
            if layer_name != "0":
                continue
            unmatched_points.append(point)
            source_name = str(point.source_name or "").strip().upper()
            if not source_name or source_name.isdigit() or source_name in {"POINT", "PUNT"}:
                marker_points.append(point)
        return marker_points if len(marker_points) >= 2 else unmatched_points

    def _resolve_cross_section_point_style(
        self,
        point: KickTheMapObjectFeature,
        layer_rules: tuple[ObjectLayerRule, ...],
    ) -> tuple[str, int]:
        rule = self._resolve_cross_section_point_rule(point, layer_rules)
        if rule is not None:
            return rule.target_layer, rule.color
        return "0", 7

    def _resolve_cross_section_point_rule(
        self,
        point: KickTheMapObjectFeature,
        layer_rules: tuple[ObjectLayerRule, ...],
    ) -> ObjectLayerRule | None:
        for rule in layer_rules:
            if rule.matches(point.source_name):
                return rule
        return None

    def _cross_section_feature_position(
        self,
        point: KickTheMapObjectFeature,
    ) -> tuple[float, float, float] | None:
        if isinstance(point, KickTheMapObjectPoint):
            if point.z is None:
                return None
            return float(point.x), float(point.y), float(point.z)
        return self._cross_section_polyline_midpoint(point)

    def _cross_section_feature_chainage_range(
        self,
        point: KickTheMapObjectFeature,
        axis_start_x: float,
        axis_start_y: float,
        axis_dx: float,
        axis_dy: float,
        axis_length: float,
        fallback_marker_scale: float,
    ) -> tuple[float, float] | None:
        if axis_length <= 1e-9:
            return None
        if isinstance(point, KickTheMapObjectPoint):
            if point.z is None:
                return None
            chainage = (
                ((float(point.x) - axis_start_x) * axis_dx)
                + ((float(point.y) - axis_start_y) * axis_dy)
            ) / axis_length
            marker_scale = self._cross_section_point_marker_scale(point, fallback_marker_scale)
            marker_radius = max(0.0, 0.5 * marker_scale)
            max_marker_offset = max(
                (abs(offset) for offset in self._template_profile_marker_x_offsets(point, marker_scale)),
                default=0.0,
            )
            marker_half_extent = marker_radius + max_marker_offset
            return chainage - marker_half_extent, chainage + marker_half_extent
        vertex_chainages: list[float] = []
        for vertex in point.vertices:
            if vertex.z is None:
                continue
            vertex_chainages.append(
                (
                    ((float(vertex.x) - axis_start_x) * axis_dx)
                    + ((float(vertex.y) - axis_start_y) * axis_dy)
                ) / axis_length
            )
        if not vertex_chainages:
            return None
        return min(vertex_chainages), max(vertex_chainages)

    def _cross_section_polyline_midpoint(
        self,
        polyline: KickTheMapObjectPolyline,
    ) -> tuple[float, float, float] | None:
        vertices = [vertex for vertex in polyline.vertices if vertex.z is not None]
        if len(vertices) < 2:
            return None
        segment_lengths: list[float] = []
        total_length = 0.0
        for first, second in zip(vertices, vertices[1:]):
            segment_length = hypot(second.x - first.x, second.y - first.y)
            segment_lengths.append(segment_length)
            total_length += segment_length
        if total_length <= 1e-9:
            midpoint = vertices[len(vertices) // 2]
            return float(midpoint.x), float(midpoint.y), float(midpoint.z or 0.0)
        target_length = total_length * 0.5
        traversed_length = 0.0
        for index, segment_length in enumerate(segment_lengths):
            first = vertices[index]
            second = vertices[index + 1]
            if traversed_length + segment_length >= target_length and segment_length > 1e-9:
                ratio = (target_length - traversed_length) / segment_length
                first_z = float(first.z or 0.0)
                second_z = float(second.z or 0.0)
                return (
                    float(first.x + ((second.x - first.x) * ratio)),
                    float(first.y + ((second.y - first.y) * ratio)),
                    float(first_z + ((second_z - first_z) * ratio)),
                )
            traversed_length += segment_length
        last_vertex = vertices[-1]
        return float(last_vertex.x), float(last_vertex.y), float(last_vertex.z or 0.0)

    def _cross_section_polyline_profile_vertices(
        self,
        polyline: KickTheMapObjectPolyline,
        profile: TemplateCrossSectionProfile,
        start_x: float,
        reference_y: float,
        display_min_chainage: float = 0.0,
        horizontal_scale: float = 1.0,
        vertical_scale: float = 1.0,
    ) -> list[tuple[float, float]]:
        if profile.axis_length <= 1e-9:
            return []
        profile_vertices: list[tuple[float, float]] = []
        for vertex in polyline.vertices:
            if vertex.z is None:
                continue
            chainage = (
                ((float(vertex.x) - profile.start_point.x) * profile.axis_dx)
                + ((float(vertex.y) - profile.start_point.y) * profile.axis_dy)
            ) / profile.axis_length
            profile_vertices.append(
                (
                    start_x + ((chainage - display_min_chainage) * horizontal_scale),
                    reference_y + ((float(vertex.z) - profile.reference_level) * vertical_scale),
                )
            )
        return profile_vertices

    def _cross_section_visible_chainage_range(
        self,
        profile: TemplateCrossSectionProfile,
        fallback_marker_scale: float,
    ) -> tuple[float, float]:
        _ = fallback_marker_scale
        min_chainage = 0.0
        max_chainage = profile.axis_length
        for profile_point in profile.points:
            if profile_point.is_endpoint:
                continue
            min_chainage = min(min_chainage, profile_point.chainage)
            max_chainage = max(max_chainage, profile_point.chainage)
        return min_chainage, max_chainage

    def _cross_section_description(
        self,
        point: KickTheMapObjectFeature,
        layer_name: str,
        configured_label: str = "",
    ) -> str:
        custom_label = str(configured_label or "").strip()
        if custom_label:
            description = custom_label
        else:
            upper_layer = layer_name.upper()
            upper_source = str(point.source_name or "").strip().upper()
            if "WARM" in upper_layer or "WARM" in upper_source:
                description = "Warmtenet"
            elif layer_name == "0":
                description = "ONBEKEND"
            elif "GLASVEZEL" in upper_layer:
                description = "Glasvezel"
            elif "DATA" in upper_layer:
                description = "Datakabel"
            elif "WATER" in upper_layer:
                description = "Waterleiding"
            elif "ET_LS" in upper_layer:
                description = "Laagspanning"
            elif "ET_MS" in upper_layer:
                description = "Middenspanning"
            elif "GAS_LD" in upper_layer:
                description = "Gasleiding LD"
            elif "GAS_HD" in upper_layer:
                description = "Gasleiding HD"
            elif "MONITORING" in upper_layer:
                description = "Monitoring"
            elif "VRIJVERVAL" in upper_layer:
                description = "Riool vrijverval"
            elif "DRUK" in upper_layer:
                description = "Riool druk"
            elif "GEVAARLIJKE" in upper_layer:
                description = "Buisleiding GS"
            else:
                source_name = str(point.source_name or "").strip()
                description = source_name.upper() if source_name else layer_name
        attribute_3 = str(point.attribute_3 or "").strip().strip("\"'")
        if isinstance(point, KickTheMapObjectPolyline) and attribute_3.lower() == "dekband":
            source_name = str(point.source_name or "").strip()
            if source_name:
                description = source_name
            if description.casefold().endswith("dekband"):
                return description
            return f"{description} dekband"
        return description

    def _cross_section_leader_description(
        self,
        point: KickTheMapObjectFeature,
        base_description: str,
        diameter_prefix: str,
    ) -> str:
        description = str(base_description or "").strip() or "ONBEKEND"
        if self._cross_section_is_mantelbuis(point) and "mantelbuis" not in description.casefold():
            description = f"{description} mantelbuis"
        parts = [description]
        bundle_text = self._cross_section_bundle_label(point)
        if bundle_text:
            parts.append(bundle_text)
        diameter_text = self._cross_section_diameter_label(point, diameter_prefix)
        material_text = str(point.attribute_1 or "").strip()
        if diameter_text:
            parts.append(diameter_text)
        if material_text:
            parts.append(material_text)
        return " ".join(parts)

    def _cross_section_diameter_label(
        self,
        point: KickTheMapObjectFeature,
        diameter_prefix: str,
    ) -> str:
        diameter_mm = self._cross_section_point_diameter_mm(point)
        if diameter_mm is None or diameter_mm <= 0.0:
            return ""
        if abs(diameter_mm - round(diameter_mm)) <= 1e-6:
            normalized = str(int(round(diameter_mm)))
        else:
            normalized = f"{diameter_mm:.3f}".rstrip("0").rstrip(".").replace(".", ",")
        prefix = str(diameter_prefix or "Ø").strip() or "Ø"
        return f"{prefix}{normalized}"

    def _cross_section_point_marker_scale(
        self,
        point: KickTheMapObjectFeature,
        fallback_scale: float,
    ) -> float:
        diameter_mm = self._cross_section_point_diameter_mm(point)
        if diameter_mm is None or diameter_mm <= 0.0:
            return max(0.005, float(fallback_scale))
        return max(0.005, diameter_mm / 1000.0)

    def _cross_section_point_diameter_mm(self, point: KickTheMapObjectFeature) -> float | None:
        raw_value = str(point.attribute_2 or "").strip()
        if not raw_value:
            return None
        match = re.search(r"[-+]?\d+(?:[.,]\d+)?", raw_value)
        if match is None:
            return None
        try:
            return float(match.group(0).replace(",", "."))
        except ValueError:
            return None

    def _cross_section_bundle_label(self, point: KickTheMapObjectFeature) -> str:
        raw_value = str(point.attribute_3 or "").strip().strip("\"'")
        if not raw_value:
            return ""
        match = re.search(r"\d+(?:[.,]\d+)?", raw_value)
        if match is None:
            return ""
        bundle_value = match.group(0).replace(",", ".")
        try:
            bundle_number = float(bundle_value)
        except ValueError:
            return ""
        if abs(bundle_number - round(bundle_number)) <= 1e-6:
            normalized = str(int(round(bundle_number)))
        else:
            normalized = f"{bundle_number:.3f}".rstrip("0").rstrip(".").replace(".", ",")
        return f"Bundel: {normalized}X"

    def _cross_section_attribute3_text(self, point: KickTheMapObjectFeature) -> str:
        return str(point.attribute_3 or "").strip().strip("\"'")

    def _cross_section_is_mantelbuis(self, point: KickTheMapObjectFeature) -> bool:
        return "mantelbuis" in self._cross_section_attribute3_text(point).casefold()

    def _cross_section_is_double(self, point: KickTheMapObjectFeature) -> bool:
        return bool(re.search(r"(?i)(?:^|[\s,;])dubbel(?:$|[\s,;])", self._cross_section_attribute3_text(point)))

    def _road_distance_to_point(
        self,
        point: KickTheMapObjectPoint,
        road_centerline_paths: list[list[tuple[float, float]]],
    ) -> float:
        if not road_centerline_paths:
            return 0.0
        best_distance = float("inf")
        for path in road_centerline_paths:
            if len(path) < 2:
                continue
            for start, end in zip(path, path[1:]):
                distance = self._segment_distance(point.x, point.y, start, end)
                if distance < best_distance:
                    best_distance = distance
        return best_distance

    def _segment_distance(
        self,
        x: float,
        y: float,
        start: tuple[float, float],
        end: tuple[float, float],
    ) -> float:
        x1, y1 = start
        x2, y2 = end
        dx = x2 - x1
        dy = y2 - y1
        if abs(dx) <= 1e-9 and abs(dy) <= 1e-9:
            return hypot(x - x1, y - y1)
        projection = ((x - x1) * dx + (y - y1) * dy) / ((dx * dx) + (dy * dy))
        projection = max(0.0, min(1.0, projection))
        nearest_x = x1 + projection * dx
        nearest_y = y1 + projection * dy
        return hypot(x - nearest_x, y - nearest_y)

    def _same_job_point(
        self,
        first: KickTheMapObjectFeature,
        second: KickTheMapObjectPoint,
        tolerance: float = 1e-6,
    ) -> bool:
        feature_position = self._cross_section_feature_position(first)
        if feature_position is None:
            return False
        return abs(feature_position[0] - second.x) <= tolerance and abs(feature_position[1] - second.y) <= tolerance

    def _profile_identifier(self, label: str) -> str:
        numbered_label = re.search(r"(?i)(\d+)([A-Z]+)?", label)
        if numbered_label is not None:
            suffix = str(numbered_label.group(2) or "").upper()
            return f"DP{int(numbered_label.group(1))}{suffix}"
        return label.replace("PS", "DP", 1)

    def _profile_description(self, label: str) -> str:
        numbered_label = re.search(r"(?i)(\d+)([A-Z]+)?", label)
        if numbered_label is not None:
            suffix = str(numbered_label.group(2) or "").upper()
            return f"PS {int(numbered_label.group(1))}{suffix}"
        return label

    def _ensure_template_profile_layers(
        self,
        document: ezdxf.EzDxfDocument,
        profile_points: tuple[TemplateCrossSectionPoint, ...],
    ) -> None:
        layer_specs = {
            "B-WE-OG-TERREIN_PROFIELLIJN-GD": {
                "color": 8,
                "lineweight": self.TEMPLATE_PROFILE_LINEWEIGHT,
            },
            "X-XX-AL-PROFIELBALK-SD": {
                "color": 7,
                "lineweight": self.TEMPLATE_PROFILE_LINEWEIGHT,
            },
            "X-XX-AL-VERWIJZING-SD": {
                "color": 7,
                "lineweight": self.TEMPLATE_PROFILE_LINEWEIGHT,
            },
        }
        for layer_name, attributes in layer_specs.items():
            if layer_name not in document.layers:
                document.layers.add(name=layer_name, dxfattribs=attributes)
            layer = document.layers.get(layer_name)
            for attribute_name, value in attributes.items():
                setattr(layer.dxf, attribute_name, value)
        for point in profile_points:
            if point.layer_name == "0":
                continue
            if point.layer_name in document.layers:
                layer = document.layers.get(point.layer_name)
                layer.dxf.color = point.color
                layer.dxf.lineweight = self.TEMPLATE_PROFILE_LINEWEIGHT
                continue
            document.layers.add(
                name=point.layer_name,
                dxfattribs={
                    "color": point.color,
                    "lineweight": self.TEMPLATE_PROFILE_LINEWEIGHT,
                },
            )

    def _add_template_profile_leader_marker(
        self,
        modelspace,
        insert_x: float,
        insert_y: float,
        marker_scale: float,
        layer_name: str,
        color: int,
        clip_left: float | None = None,
        clip_right: float | None = None,
        clip_bottom: float | None = None,
    ) -> None:
        if (
            clip_left is not None
            and clip_right is not None
            and clip_bottom is not None
            and self._template_profile_marker_needs_clipping(
                insert_x=insert_x,
                insert_y=insert_y,
                marker_scale=marker_scale,
                clip_left=clip_left,
                clip_right=clip_right,
                clip_bottom=clip_bottom,
            )
        ):
            self._add_clipped_template_profile_marker(
                modelspace,
                insert_x=insert_x,
                insert_y=insert_y,
                marker_scale=marker_scale,
                layer_name=layer_name,
                color=color,
                clip_left=clip_left,
                clip_right=clip_right,
                clip_bottom=clip_bottom,
            )
            return
        document = modelspace.doc
        if document is None:
            modelspace.add_circle(
                center=(insert_x, insert_y - (0.5 * marker_scale)),
                radius=0.5 * marker_scale,
                dxfattribs={
                    "layer": layer_name,
                    "color": color,
                    "lineweight": self.TEMPLATE_PROFILE_LINEWEIGHT,
                },
            )
            return
        block_name = self._ensure_template_profile_marker_block(document)
        modelspace.add_blockref(
            block_name,
            (insert_x, insert_y),
            dxfattribs={
                "layer": layer_name,
                "color": color,
                "xscale": marker_scale,
                "yscale": marker_scale,
                "rotation": 0.0,
                "lineweight": self.TEMPLATE_PROFILE_LINEWEIGHT,
            },
        )

    def _template_profile_marker_x_values(
        self,
        point: KickTheMapObjectFeature,
        insert_x: float,
        marker_scale: float,
    ) -> tuple[float, ...]:
        return tuple(
            float(insert_x) + offset
            for offset in self._template_profile_marker_x_offsets(point, marker_scale)
        )

    def _template_profile_marker_x_offsets(
        self,
        point: KickTheMapObjectFeature,
        marker_scale: float,
    ) -> tuple[float, ...]:
        if not self._cross_section_is_double(point):
            return (0.0,)
        offset = max(0.01, float(marker_scale) * 0.58)
        return (-offset, offset)

    def _template_profile_marker_needs_clipping(
        self,
        insert_x: float,
        insert_y: float,
        marker_scale: float,
        clip_left: float,
        clip_right: float,
        clip_bottom: float,
    ) -> bool:
        radius = max(1e-6, 0.5 * float(marker_scale))
        center_x = float(insert_x)
        center_y = float(insert_y) - radius
        marker_left = center_x - radius
        marker_right = center_x + radius
        marker_bottom = center_y - radius
        return (
            marker_left < float(clip_left) - 1e-9
            or marker_right > float(clip_right) + 1e-9
            or marker_bottom < float(clip_bottom) - 1e-9
        )

    def _add_clipped_template_profile_marker(
        self,
        modelspace,
        insert_x: float,
        insert_y: float,
        marker_scale: float,
        layer_name: str,
        color: int,
        clip_left: float,
        clip_right: float,
        clip_bottom: float,
    ) -> None:
        radius = max(1e-6, 0.5 * float(marker_scale))
        center_x = float(insert_x)
        center_y = float(insert_y) - radius
        segment_count = 64
        points: list[tuple[float, float]] = []
        for index in range(segment_count + 1):
            angle = (2.0 * np.pi * index) / segment_count
            points.append(
                (
                    center_x + (radius * np.cos(angle)),
                    center_y + (radius * np.sin(angle)),
                )
            )
        for start_point, end_point in zip(points, points[1:]):
            clipped = self._clip_profile_marker_segment(
                start_point,
                end_point,
                clip_left=clip_left,
                clip_right=clip_right,
                clip_bottom=clip_bottom,
            )
            if clipped is None:
                continue
            modelspace.add_lwpolyline(
                [clipped[0], clipped[1]],
                dxfattribs={
                    "layer": layer_name,
                    "color": color,
                    "lineweight": self.TEMPLATE_PROFILE_LINEWEIGHT,
                },
            )

    def _clip_profile_marker_segment(
        self,
        start_point: tuple[float, float],
        end_point: tuple[float, float],
        clip_left: float,
        clip_right: float,
        clip_bottom: float,
    ) -> tuple[tuple[float, float], tuple[float, float]] | None:
        x0, y0 = start_point
        x1, y1 = end_point
        dx = x1 - x0
        dy = y1 - y0
        t0 = 0.0
        t1 = 1.0

        def clip(p: float, q: float) -> bool:
            nonlocal t0, t1
            if abs(p) <= 1e-12:
                return q >= 0.0
            ratio = q / p
            if p < 0.0:
                if ratio > t1:
                    return False
                if ratio > t0:
                    t0 = ratio
            else:
                if ratio < t0:
                    return False
                if ratio < t1:
                    t1 = ratio
            return True

        if not clip(-dx, x0 - clip_left):
            return None
        if not clip(dx, clip_right - x0):
            return None
        if not clip(-dy, y0 - clip_bottom):
            return None
        start_clipped = (x0 + (t0 * dx), y0 + (t0 * dy))
        end_clipped = (x0 + (t1 * dx), y0 + (t1 * dy))
        if hypot(end_clipped[0] - start_clipped[0], end_clipped[1] - start_clipped[1]) <= 1e-9:
            return None
        return start_clipped, end_clipped

    def _ensure_template_profile_marker_block(self, document: ezdxf.EzDxfDocument) -> str:
        block_name = "SAL-VERWIJZING_PROFIELPUNT-MARKER-SOD"
        if block_name in document.blocks:
            return block_name
        block = document.blocks.new(name=block_name, base_point=(0.0, 0.0, 0.0))
        block.add_circle(
            center=(0.0, -0.5),
            radius=0.5,
            dxfattribs={
                "layer": "0",
                "color": 0,
                "lineweight": self.TEMPLATE_PROFILE_LINEWEIGHT,
            },
        )
        return block_name

    def _ensure_template_profile_leader_block(self, document: ezdxf.EzDxfDocument) -> str:
        block_name = self.TEMPLATE_PROFILE_LEADER_BLOCK_NAME
        if block_name in document.blocks:
            return block_name
        block = document.blocks.new(name=block_name, base_point=(0.0, 0.0, 0.0))
        block.add_attdef(
            "OMSCHRIJVING",
            insert=(-0.5, 0.0),
            dxfattribs={
                "height": 1.5,
                "rotation": 90.0,
                "style": self.CADASTRAL_LABEL_STYLE,
                "layer": "0",
                "color": 7,
                "lineweight": self.TEMPLATE_PROFILE_LINEWEIGHT,
                "lock_position": 1,
            },
        )
        block.add_attdef(
            "HOOGTE",
            insert=(2.0, 0.0),
            dxfattribs={
                "height": 1.5,
                "rotation": 90.0,
                "style": self.CADASTRAL_LABEL_STYLE,
                "layer": "0",
                "color": 7,
                "lineweight": self.TEMPLATE_PROFILE_LINEWEIGHT,
                "lock_position": 1,
            },
        )
        return block_name

    def _remove_template_legacy_profile_leader_blocks(self, document: ezdxf.EzDxfDocument) -> None:
        legacy_name = self.TEMPLATE_PROFILE_LEGACY_DYNAMIC_LEADER_BLOCK_NAME
        if legacy_name not in document.blocks:
            return
        dependent_names = [
            block.name
            for block in list(document.blocks)
            if block.name != legacy_name
            and any(
                entity.dxftype() == "INSERT" and str(entity.dxf.name) == legacy_name
                for entity in block
            )
        ]
        for block_name in dependent_names:
            try:
                document.blocks.delete_block(block_name, safe=True)
            except Exception:
                pass
        try:
            document.blocks.delete_block(legacy_name, safe=True)
        except Exception:
            pass

    def _add_template_profile_multileader(
        self,
        modelspace,
        description: str,
        depth_text: str,
        leader_start: tuple[float, float],
        text_insert: tuple[float, float],
        marker_scale: float,
        color: int | None = None,
    ) -> None:
        document = modelspace.doc
        if document is None:
            return
        normalized_scale = max(0.005, float(marker_scale))
        leader_start_x = float(leader_start[0])
        leader_start_y = float(leader_start[1])
        text_insert_x = float(text_insert[0])
        text_insert_y = float(text_insert[1])
        block_name = self._ensure_template_profile_leader_block(document)
        block_ref = self._insert_template_block(
            modelspace,
            block_name,
            insert=(text_insert_x, text_insert_y),
            layer_name="X-XX-AL-VERWIJZING-SD",
            attributes={"OMSCHRIJVING": description, "HOOGTE": depth_text},
            color=color,
            scale=normalized_scale,
        )
        leader = modelspace.add_leader(
            [(leader_start_x, leader_start_y, 0.0), (text_insert_x, text_insert_y, 0.0)],
            dimstyle="Standard",
            dxfattribs={
                "layer": "X-XX-AL-VERWIJZING-SD",
                "color": self.TEMPLATE_PROFILE_LEADER_LINE_COLOR,
                "lineweight": self.TEMPLATE_PROFILE_LINEWEIGHT,
                "has_arrowhead": 0,
                "path_type": 0,
                "annotation_type": 2,
                "has_hookline": 0,
                "block_color": color or 7,
            },
        )
        leader.dxf.annotation_handle = block_ref.dxf.handle
        leader.dxf.leader_offset_block_ref = (0.0, 0.0, 0.0)

    def _insert_template_block(
        self,
        modelspace,
        block_name: str,
        insert: tuple[float, float],
        layer_name: str,
        attributes: dict[str, str] | None = None,
        color: int | None = None,
        scale: float = 0.02,
    ):
        dxfattribs = {
            "layer": layer_name,
            "xscale": scale,
            "yscale": scale,
            "rotation": 0.0,
        }
        if color is not None:
            dxfattribs["color"] = color
        block_ref = modelspace.add_blockref(block_name, insert, dxfattribs=dxfattribs)
        if attributes:
            block_ref.add_auto_attribs(attributes)
        return block_ref

    def _template_block_attribute(self, block_ref, tag: str):
        normalized_tag = str(tag).strip().upper()
        for attribute in getattr(block_ref, "attribs", []):
            if str(attribute.dxf.tag).strip().upper() == normalized_tag:
                return attribute
        return None

    def _set_template_text_position(
        self,
        entity,
        insert: tuple[float, float],
        height: float,
        rotation: float = 90.0,
    ) -> None:
        entity.dxf.insert = (insert[0], insert[1], 0.0)
        if entity.dxftype() in {"ATTRIB", "ATTDEF", "TEXT"} and hasattr(entity.dxf, "align_point"):
            entity.dxf.align_point = (insert[0], insert[1], 0.0)
        entity.dxf.height = height
        entity.dxf.rotation = rotation

    def _estimate_text_width(self, text: str, height: float) -> float:
        normalized = str(text or "").strip()
        if not normalized:
            return height
        return max(height, len(normalized) * height * 0.58)

    def _centered_rotated_text_box(
        self,
        text: str,
        insert: tuple[float, float],
        height: float,
        rotation_degrees: float,
    ) -> tuple[float, float, float, float]:
        width = self._estimate_text_width(text, height)
        radians = np.deg2rad(rotation_degrees)
        axis_x = np.cos(radians)
        axis_y = np.sin(radians)
        normal_x = -axis_y
        normal_y = axis_x
        half_width = width * 0.5
        half_height = height * 0.5
        corners: list[tuple[float, float]] = []
        for width_sign in (-1.0, 1.0):
            for height_sign in (-1.0, 1.0):
                corners.append(
                    (
                        insert[0] + (axis_x * half_width * width_sign) + (normal_x * half_height * height_sign),
                        insert[1] + (axis_y * half_width * width_sign) + (normal_y * half_height * height_sign),
                    )
                )
        xs = [corner[0] for corner in corners]
        ys = [corner[1] for corner in corners]
        return (min(xs), min(ys), max(xs), max(ys))

    def _left_aligned_text_box(
        self,
        text: str,
        insert: tuple[float, float],
        height: float,
    ) -> tuple[float, float, float, float]:
        width = self._estimate_text_width(text, height)
        return (
            insert[0],
            insert[1] - (height * 0.25),
            insert[0] + width,
            insert[1] + height,
        )

    def _add_template_fill_text(
        self,
        modelspace,
        start: tuple[float, float],
        end: tuple[float, float],
        text: str,
        avoid_box: tuple[float, float, float, float] | None = None,
    ) -> tuple[float, float, float, float] | None:
        normalized_text = str(text or "").strip() or "INVULLEN"
        dx = end[0] - start[0]
        dy = end[1] - start[1]
        length = hypot(dx, dy)
        if length <= 1e-9:
            return None
        direction_x = dx / length
        direction_y = dy / length
        normal_x = -direction_y
        normal_y = direction_x
        text_width = self._estimate_text_width(normalized_text, self.TEMPLATE_PROFILE_FILL_TEXT_HEIGHT)
        center_x = (start[0] + end[0]) * 0.5
        center_y = (start[1] + end[1]) * 0.5
        if avoid_box is not None and abs(direction_y) <= 1e-9:
            min_center_x = avoid_box[2] + (text_width * 0.5) + self.TEMPLATE_PROFILE_FILL_TEXT_GAP
            max_center_x = end[0] - (text_width * 0.5)
            center_x = min(max(center_x, min_center_x), max_center_x)
        insert = (
            center_x + (normal_x * self.TEMPLATE_PROFILE_FILL_TEXT_OFFSET),
            center_y + (normal_y * self.TEMPLATE_PROFILE_FILL_TEXT_OFFSET),
        )
        text_entity = modelspace.add_text(
            normalized_text,
            dxfattribs={
                "layer": "0",
                "height": self.TEMPLATE_PROFILE_FILL_TEXT_HEIGHT,
                "style": self.CADASTRAL_LABEL_STYLE,
                "rotation": degrees(atan2(dy, dx)),
            },
        )
        text_entity.set_placement(insert, align=TextEntityAlignment.MIDDLE_CENTER)
        return self._centered_rotated_text_box(
            text=normalized_text,
            insert=insert,
            height=self.TEMPLATE_PROFILE_FILL_TEXT_HEIGHT,
            rotation_degrees=degrees(atan2(dy, dx)),
        )

    def _add_template_soil_box(
        self,
        modelspace,
        start_x: float,
        top_y: float,
        bottom_y: float,
    ) -> None:
        if top_y <= bottom_y + 1e-6:
            return
        left_x = start_x - self.TEMPLATE_PROFILE_SOIL_BOX_WIDTH
        right_x = start_x
        modelspace.add_lwpolyline(
            [
                (right_x, top_y),
                (left_x, top_y),
                (left_x, bottom_y),
                (right_x, bottom_y),
                (right_x, top_y),
            ],
            dxfattribs={"layer": "0", "lineweight": self.TEMPLATE_PROFILE_LINEWEIGHT},
        )
        text_entity = modelspace.add_text(
            "INVULLEN",
            dxfattribs={
                "layer": "0",
                "height": self.TEMPLATE_PROFILE_SOIL_TEXT_HEIGHT,
                "style": self.CADASTRAL_LABEL_STYLE,
            },
        )
        text_entity.set_placement(
            ((left_x + right_x) * 0.5, (top_y + bottom_y) * 0.5),
            align=TextEntityAlignment.MIDDLE_CENTER,
        )

    def _estimate_rotated_text_length(self, text: str, height: float) -> float:
        normalized = str(text or "").strip()
        if not normalized:
            return height
        return max(height, len(normalized) * height * 0.58)

    def _distribute_template_band_labels(
        self,
        modelspace,
        entries: list[dict[str, object]],
        min_x: float,
        max_x: float,
    ) -> None:
        if not entries:
            return
        sorted_entries = sorted(entries, key=lambda item: float(item["anchor_x"]))
        clearance_padding = max(0.007, self.TEMPLATE_PROFILE_BAND_VALUE_MIN_GAP * 0.48)

        def band_text_width(item: dict[str, object]) -> float:
            top_height = float(item.get("top_height", self.TEMPLATE_PROFILE_BAND_VALUE_TEXT_HEIGHT))
            bottom_height = float(item.get("bottom_height", self.TEMPLATE_PROFILE_BAND_VALUE_TEXT_HEIGHT))
            reference_height = max(top_height, bottom_height)
            # Vertical text still needs roughly a character-height of horizontal footprint.
            return max(reference_height * 0.92, reference_height * 0.72)

        anchors = [float(item["anchor_x"]) for item in sorted_entries]
        widths = [band_text_width(item) for item in sorted_entries]
        pinned_edges = [bool(item.get("pin_to_anchor", False)) for item in sorted_entries]

        def pair_gap(left_index: int, right_index: int) -> float:
            return (widths[left_index] * 0.5) + (widths[right_index] * 0.5) + clearance_padding

        def clamped_center(index: int, value: float) -> float:
            min_center = min_x + (widths[index] * 0.5)
            max_center = max_x - (widths[index] * 0.5)
            if max_center < min_center:
                max_center = min_center
            if pinned_edges[index]:
                return anchors[index]
            return min(max_center, max(min_center, value))

        def place(default_shift: float = 0.0) -> list[float]:
            positions = [clamped_center(index, anchor_x + default_shift) for index, anchor_x in enumerate(anchors)]
            for index, is_pinned in enumerate(pinned_edges):
                if is_pinned:
                    positions[index] = anchors[index]

            for _ in range(3):
                for index in range(1, len(positions)):
                    if pinned_edges[index]:
                        positions[index] = anchors[index]
                        continue
                    minimum_center = positions[index - 1] + pair_gap(index - 1, index)
                    positions[index] = clamped_center(index, max(positions[index], minimum_center))

                for index in range(len(positions) - 2, -1, -1):
                    if pinned_edges[index]:
                        positions[index] = anchors[index]
                        continue
                    maximum_center = positions[index + 1] - pair_gap(index, index + 1)
                    positions[index] = clamped_center(index, min(positions[index], maximum_center))
            return positions

        positions = place()

        for item, shifted_x in zip(sorted_entries, positions):
            item["placed_x"] = shifted_x
            self._move_template_band_value_block(
                item["top_ref"],
                item["top_attr"],
                shifted_x,
                float(item["top_y"]),
                float(item["top_offset_x"]),
                float(item["top_offset_y"]),
                float(item["top_height"]),
            )
            self._move_template_band_value_block(
                item["bottom_ref"],
                item["bottom_attr"],
                shifted_x,
                float(item["bottom_y"]),
                float(item["bottom_offset_x"]),
                float(item["bottom_offset_y"]),
                float(item["bottom_height"]),
            )
            anchor_x = float(item["anchor_x"])
            if abs(shifted_x - anchor_x) > 1e-6:
                modelspace.add_lwpolyline(
                    [
                        (anchor_x, float(item["top_y"]) + 0.04),
                        (shifted_x, float(item["top_y"])),
                    ],
                    dxfattribs={
                        "layer": "X-XX-AL-PROFIELBALK-SD",
                        "color": self.TEMPLATE_PROFILE_LEADER_LINE_COLOR,
                        "lineweight": self.TEMPLATE_PROFILE_LINEWEIGHT,
                    },
                )

    def _boxes_overlap(
        self,
        first: tuple[float, float, float, float],
        second: tuple[float, float, float, float],
    ) -> bool:
        return not (
            first[2] <= second[0]
            or second[2] <= first[0]
            or first[3] <= second[1]
            or second[3] <= first[1]
        )

    def _distribute_template_leader_labels(
        self,
        document: ezdxf.EzDxfDocument,
        modelspace,
        entries: list[dict[str, object]],
        marker_scale: float,
        hard_static_line_segments: list[tuple[float, float, float, float]] | None = None,
        soft_static_line_segments: list[tuple[float, float, float, float]] | None = None,
        static_text_boxes: list[tuple[float, float, float, float]] | None = None,
        min_text_x: float | None = None,
        max_text_x: float | None = None,
        avoid_collisions: bool = True,
    ) -> None:
        if not entries:
            return
        self._ensure_template_profile_leader_block(document)
        placed_boxes: list[tuple[float, float, float, float]] = []
        max_entry_scale = max(
            max(0.005, float(entry.get("leader_scale", marker_scale))) for entry in entries
        )
        line_clearance = max(self.TEMPLATE_PROFILE_LEADER_TEXT_HEIGHT * 0.75, max_entry_scale * 0.75)
        text_padding_y = max(0.01, self.TEMPLATE_PROFILE_LEADER_TEXT_HEIGHT * 0.35)

        def expanded_line_box(
            start_x: float,
            start_y: float,
            end_x: float,
            end_y: float,
        ) -> tuple[float, float, float, float]:
            return (
                min(start_x, end_x) - line_clearance,
                min(start_y, end_y) - line_clearance,
                max(start_x, end_x) + line_clearance,
                max(start_y, end_y) + line_clearance,
            )

        def overlap_area(
            first: tuple[float, float, float, float],
            second: tuple[float, float, float, float],
        ) -> float:
            overlap_width = min(first[2], second[2]) - max(first[0], second[0])
            overlap_height = min(first[3], second[3]) - max(first[1], second[1])
            if overlap_width <= 0.0 or overlap_height <= 0.0:
                return 0.0
            return overlap_width * overlap_height

        def boundary_penalty(
            candidate_box: tuple[float, float, float, float],
        ) -> float:
            penalty = 0.0
            if min_text_x is not None and candidate_box[0] < float(min_text_x):
                penalty += (float(min_text_x) - candidate_box[0]) * 5000.0
            if max_text_x is not None and candidate_box[2] > float(max_text_x):
                penalty += (candidate_box[2] - float(max_text_x)) * 5000.0
            return penalty

        hard_line_boxes: list[tuple[float, float, float, float]] = [
            expanded_line_box(*segment) for segment in (hard_static_line_segments or [])
        ]
        soft_line_boxes: list[tuple[float, float, float, float]] = [
            expanded_line_box(*segment) for segment in (soft_static_line_segments or [])
        ]
        hard_text_boxes = list(static_text_boxes or [])

        sorted_entries = sorted(entries, key=lambda item: (float(item["anchor_x"]), float(item["leader_top_y"])))
        effective_avoid_collisions = bool(avoid_collisions) and len(sorted_entries) <= 80
        for entry_index, entry in enumerate(sorted_entries):
            entry_marker_scale = max(0.005, float(entry.get("leader_scale", marker_scale)))
            column_gap = max(0.05, entry_marker_scale * 2.5)
            horizontal_step = max(self.TEMPLATE_PROFILE_LEADER_TEXT_GAP, column_gap)
            text_padding_x = max(0.02, self.TEMPLATE_PROFILE_LEADER_TEXT_HEIGHT * 0.9, entry_marker_scale * 0.6)
            base_y = float(entry["leader_top_y"])
            base_description_x = float(entry["anchor_x"]) - (0.5 * entry_marker_scale)
            base_depth_x = base_description_x + column_gap
            column_texts = [str(entry["description"]), str(entry["depth_text"])]
            max_text_length = max(
                self._estimate_rotated_text_length(text, self.TEMPLATE_PROFILE_LEADER_TEXT_HEIGHT)
                for text in column_texts
            )
            x_shift_candidates = [0.0]
            max_horizontal_steps = min(max(12, (len(entries) * 2) + 2), 28)
            for step_index in range(1, max_horizontal_steps + 1):
                shift = horizontal_step * step_index
                x_shift_candidates.extend((shift, -shift))
            y_shift_candidates = [0.0]
            max_vertical_steps = min(max(8, len(entries) + 2), 18)
            for step_index in range(1, max_vertical_steps + 1):
                shift = self.TEMPLATE_PROFILE_LEADER_VERTICAL_STEP * step_index
                y_shift_candidates.extend((shift, -shift))

            other_entry_line_boxes: list[tuple[float, float, float, float]] = []
            if effective_avoid_collisions:
                for other_index, other_entry in enumerate(sorted_entries):
                    if other_index == entry_index:
                        continue
                    other_anchor_x = float(other_entry["anchor_x"])
                    other_band_x = float(other_entry.get("band_x", other_entry["anchor_x"]))
                    other_band_y = float(other_entry["band_y"])
                    other_connector_end_y = (
                        float(other_entry["band_connector_y"])
                        if abs(other_band_x - other_anchor_x) > 1e-6
                        else other_band_y
                    )
                    other_entry_line_boxes.append(
                        expanded_line_box(
                            other_anchor_x,
                            float(other_entry["leader_line_start_y"]),
                            other_anchor_x,
                            other_connector_end_y,
                        )
                    )
                    if abs(other_band_x - other_anchor_x) > 1e-6:
                        other_entry_line_boxes.append(
                            expanded_line_box(
                                other_anchor_x,
                                float(other_entry["band_connector_y"]),
                                other_band_x,
                                other_band_y,
                            )
                        )

            selected: tuple[float, float, tuple[float, float, float, float]] | None = None
            best_candidate: tuple[float, float, tuple[float, float, float, float], float] | None = None
            for allow_soft_overlap in (False, True):
                for y_shift in y_shift_candidates:
                    for x_shift in x_shift_candidates:
                        description_x = base_description_x + x_shift
                        depth_x = base_depth_x + x_shift
                        column_xs = [description_x, depth_x]
                        candidate_y = base_y + y_shift
                        candidate_box = (
                            min(column_xs) - text_padding_x,
                            candidate_y - text_padding_y,
                            max(column_xs) + text_padding_x,
                            candidate_y + max_text_length + text_padding_y,
                        )
                        candidate_line_boxes = [
                            expanded_line_box(
                                float(entry["anchor_x"]),
                                float(entry["leader_line_start_y"]),
                                float(entry["anchor_x"]),
                                float(entry["band_connector_y"]),
                            ),
                            expanded_line_box(
                                float(entry["anchor_x"]),
                                float(entry["leader_block_start_y"]),
                                description_x + (0.5 * entry_marker_scale),
                                candidate_y,
                            ),
                        ]
                        candidate_penalty = boundary_penalty(candidate_box)
                        candidate_penalty += sum(
                            overlap_area(candidate_box, existing_box) * 25000.0
                            for existing_box in placed_boxes
                        )
                        candidate_penalty += sum(
                            overlap_area(candidate_box, existing_box) * 25000.0
                            for existing_box in hard_text_boxes
                        )
                        candidate_penalty += sum(
                            overlap_area(candidate_box, existing_box) * 20000.0
                            for existing_box in hard_line_boxes
                        )
                        candidate_penalty += sum(
                            overlap_area(candidate_line_box, existing_box) * 20000.0
                            for candidate_line_box in candidate_line_boxes
                            for existing_box in hard_text_boxes
                        )
                        candidate_penalty += sum(
                            overlap_area(candidate_line_box, existing_box) * 18000.0
                            for candidate_line_box in candidate_line_boxes
                            for existing_box in hard_line_boxes
                        )
                        candidate_penalty += sum(
                            overlap_area(candidate_box, existing_box) * 22000.0
                            for existing_box in other_entry_line_boxes
                        )
                        candidate_penalty += sum(
                            overlap_area(candidate_line_box, existing_box) * 22000.0
                            for candidate_line_box in candidate_line_boxes
                            for existing_box in other_entry_line_boxes
                        )
                        if not allow_soft_overlap:
                            candidate_penalty += sum(
                                overlap_area(candidate_box, existing_box) * 8000.0
                                for existing_box in soft_line_boxes
                            )
                            candidate_penalty += sum(
                                overlap_area(candidate_line_box, existing_box) * 6000.0
                                for candidate_line_box in candidate_line_boxes
                                for existing_box in soft_line_boxes
                            )
                        candidate_penalty += abs(x_shift) * 20.0
                        candidate_penalty += abs(y_shift) * 40.0
                        if not allow_soft_overlap:
                            candidate_penalty -= 1.0
                        candidate_snapshot = (description_x, candidate_y, candidate_box, candidate_penalty)
                        if best_candidate is None or candidate_penalty < best_candidate[3]:
                            best_candidate = candidate_snapshot
                        if effective_avoid_collisions and candidate_penalty > 1e-9:
                            continue
                        selected = (description_x, candidate_y, candidate_box)
                        break
                    if selected is not None:
                        break
                if selected is not None:
                    break

            if selected is None:
                if best_candidate is not None:
                    description_x, candidate_y, candidate_box, _ = best_candidate
                    depth_x = description_x + column_gap
                else:
                    description_x = base_description_x
                    depth_x = base_depth_x
                    candidate_y = base_y
                    candidate_box = (
                        min(description_x, depth_x) - text_padding_x,
                        candidate_y - text_padding_y,
                        max(description_x, depth_x) + text_padding_x,
                        candidate_y + max_text_length + text_padding_y,
                    )
            else:
                description_x, candidate_y, candidate_box = selected
                depth_x = description_x + column_gap

            self._add_template_profile_multileader(
                modelspace,
                description=str(entry["description"]),
                depth_text=str(entry["depth_text"]),
                leader_start=(float(entry["anchor_x"]), float(entry["leader_block_start_y"])),
                text_insert=(description_x + (0.5 * entry_marker_scale), candidate_y),
                marker_scale=entry_marker_scale,
                color=int(entry["leader_color"]),
            )

            band_x = float(entry.get("band_x", entry["anchor_x"]))
            band_y = float(entry["band_y"])
            connector_end_y = (
                float(entry["band_connector_y"])
                if abs(band_x - float(entry["anchor_x"])) > 1e-6
                else band_y
            )
            modelspace.add_lwpolyline(
                [
                    (float(entry["anchor_x"]), float(entry["leader_line_start_y"])),
                    (float(entry["anchor_x"]), connector_end_y),
                ],
                dxfattribs={
                    "layer": "X-XX-AL-PROFIELBALK-SD",
                    "color": self.TEMPLATE_PROFILE_LEADER_LINE_COLOR,
                    "lineweight": self.TEMPLATE_PROFILE_LINEWEIGHT,
                },
            )

            placed_boxes.append(candidate_box)
            hard_line_boxes.append(
                expanded_line_box(
                    float(entry["anchor_x"]),
                    float(entry["leader_line_start_y"]),
                    float(entry["anchor_x"]),
                    connector_end_y,
                )
            )
            hard_line_boxes.append(
                expanded_line_box(
                    float(entry["anchor_x"]),
                    float(entry["leader_block_start_y"]),
                    description_x + (0.5 * entry_marker_scale),
                    candidate_y,
                )
            )

    def _move_template_band_value_block(
        self,
        block_ref,
        attribute,
        insert_x: float,
        insert_y: float,
        attr_offset_x: float,
        attr_offset_y: float,
        attr_height: float,
    ) -> None:
        block_ref.dxf.insert = (insert_x, insert_y, 0.0)
        self._set_template_text_position(
            attribute,
            (insert_x + attr_offset_x, insert_y + attr_offset_y),
            height=attr_height,
            rotation=90.0,
        )

    def _build_template_tiff_raster(
        self,
        asset_dir: Path,
        layer: GeoTiffLayer,
        label: str,
        index: int,
        road_orientation_paths: list[list[tuple[float, float]]],
        terrain_boundary_paths: list[list[tuple[float, float]]],
        profile: TemplateCrossSectionProfile | None = None,
        reverse_orientation: bool = False,
    ) -> Path:
        image_path = self._unique_raster_copy_path(asset_dir, f"{label}_geotiff.png")
        export_layer = self._prepared_virtual_trench_export_layer(layer)
        image = export_layer.image.convert("RGBA")
        orientation_vector = self._template_tiff_orientation_pixel_vector(
            export_layer,
            road_orientation_paths,
            terrain_boundary_paths,
            profile=profile,
        )
        if orientation_vector is not None:
            dx, dy = orientation_vector
            if (dx * dx + dy * dy) > 1e-6:
                current_bearing = self._template_bearing_from_pixel_vector(dx, dy)
                # A built profile already reflects the requested reverse direction.
                # Reversing its target bearing as well would cancel that 180-degree turn.
                # Keep the legacy fallback for exports without a usable profile vector.
                target_bearing = (
                    270.0
                    if profile is not None
                    else (90.0 if reverse_orientation else 270.0)
                )
                rotation_degrees = self._normalize_rotation_degrees(target_bearing - current_bearing)
                image = image.rotate(
                    rotation_degrees,
                    resample=Image.Resampling.BICUBIC,
                    expand=True,
                    fillcolor=(255, 255, 255, 0),
                )
        image = self._crop_template_tiff_to_visible_bounds(image)
        image = self._normalize_template_tiff_raster_alpha(image)
        try:
            image.save(image_path, format="PNG")
        except OSError as exc:
            raise CadastralExportError(f"GeoTIFF-afbeelding kon niet worden opgeslagen voor {label}: {exc}") from exc
        return image_path.resolve()

    def _prepared_virtual_trench_export_layer(self, layer: GeoTiffLayer) -> GeoTiffLayer:
        if not is_virtual_trench_layer(layer):
            return layer
        image, bounds, transform = build_virtual_trench_render(
            layer,
            quality_multiplier=self.VIRTUAL_TRENCH_EXPORT_QUALITY_MULTIPLIER,
        )
        return GeoTiffLayer(
            path=layer.path,
            image=image,
            transform=transform,
            bounds=bounds,
            epsg=layer.epsg or 28992,
            opacity=layer.opacity,
            metadata=dict(layer.metadata),
        )

    def _template_tiff_orientation_pixel_vector(
        self,
        layer: GeoTiffLayer,
        road_orientation_paths: list[list[tuple[float, float]]],
        terrain_boundary_paths: list[list[tuple[float, float]]],
        profile: TemplateCrossSectionProfile | None = None,
    ) -> tuple[float, float] | None:
        if profile is not None:
            start_px = layer.transform.world_to_pixel(float(profile.start_point.x), float(profile.start_point.y))
            end_px = layer.transform.world_to_pixel(float(profile.end_point.x), float(profile.end_point.y))
            dx = end_px[0] - start_px[0]
            dy = end_px[1] - start_px[1]
            if (dx * dx + dy * dy) > 1e-6:
                return dx, dy
        center_world = layer.transform.pixel_to_world(layer.image.width / 2.0, layer.image.height / 2.0)
        projection_world = self._preferred_template_orientation_projection(
            center_world,
            road_orientation_paths,
            terrain_boundary_paths,
        )
        if projection_world is None:
            return None
        projection_px = layer.transform.world_to_pixel(projection_world[0], projection_world[1])
        # Use the line from the road-side touch point back into the trench so the road side lands on the left.
        return (
            (layer.image.width / 2.0) - projection_px[0],
            (layer.image.height / 2.0) - projection_px[1],
        )

    def _preferred_template_orientation_projection(
        self,
        point: tuple[float, float],
        road_paths: list[list[tuple[float, float]]],
        terrain_boundary_paths: list[list[tuple[float, float]]],
    ) -> tuple[float, float] | None:
        road_projection, road_distance = self._nearest_perpendicular_projection_on_paths(point, road_paths)
        terrain_projection, terrain_distance = self._nearest_perpendicular_projection_on_paths(point, terrain_boundary_paths)
        if terrain_projection is None:
            return road_projection
        if road_projection is None:
            return terrain_projection
        # BGT terreinranden tellen 20% dichterbij mee dan hun werkelijke afstand.
        if (terrain_distance * 0.8) < road_distance:
            return terrain_projection
        return road_projection

    def _crop_template_tiff_to_visible_bounds(self, image: Image.Image) -> Image.Image:
        alpha = image.getchannel("A")
        bbox = alpha.point(lambda value: 255 if value > self.MASK_ALPHA_THRESHOLD else 0).getbbox()
        if bbox is None:
            return image
        padding = 2
        left = max(0, bbox[0] - padding)
        top = max(0, bbox[1] - padding)
        right = min(image.width, bbox[2] + padding)
        bottom = min(image.height, bbox[3] + padding)
        return image.crop((left, top, right, bottom))

    def _normalize_template_tiff_raster_alpha(self, image: Image.Image) -> Image.Image:
        rgba = image.convert("RGBA")
        pixels = np.array(rgba, dtype=np.uint8)
        alpha = pixels[:, :, 3]
        rgb = pixels[:, :, :3].astype(np.int16)
        visible = alpha > self.MASK_ALPHA_THRESHOLD
        rgb_min = rgb.min(axis=2)
        rgb_range = rgb.max(axis=2) - rgb_min
        near_white = visible & (rgb_min >= 240) & (rgb_range <= 30)
        edge_background = self._edge_connected_mask((~visible) | near_white)
        pixels[:, :, 3] = np.where(edge_background, 0, np.where(visible, 255, 0)).astype(np.uint8)
        return Image.fromarray(pixels, mode="RGBA")

    @staticmethod
    def _edge_connected_mask(mask: np.ndarray) -> np.ndarray:
        accelerated = native_accel.edge_connected_mask(mask)
        if accelerated is not None:
            return accelerated
        if mask.size == 0:
            return np.zeros(mask.shape, dtype=bool)
        height, width = mask.shape
        connected = np.zeros(mask.shape, dtype=bool)
        stack: list[tuple[int, int]] = []

        def add_seed(x: int, y: int) -> None:
            if mask[y, x] and not connected[y, x]:
                stack.append((x, y))

        for x in range(width):
            add_seed(x, 0)
            add_seed(x, height - 1)
        for y in range(1, height - 1):
            add_seed(0, y)
            add_seed(width - 1, y)

        while stack:
            seed_x, seed_y = stack.pop()
            if connected[seed_y, seed_x] or not mask[seed_y, seed_x]:
                continue
            left = seed_x
            while left > 0 and mask[seed_y, left - 1] and not connected[seed_y, left - 1]:
                left -= 1
            right = seed_x
            while right + 1 < width and mask[seed_y, right + 1] and not connected[seed_y, right + 1]:
                right += 1
            connected[seed_y, left : right + 1] = True
            if seed_y > 0:
                for x in range(left, right + 1):
                    if mask[seed_y - 1, x] and not connected[seed_y - 1, x]:
                        stack.append((x, seed_y - 1))
            if seed_y + 1 < height:
                for x in range(left, right + 1):
                    if mask[seed_y + 1, x] and not connected[seed_y + 1, x]:
                        stack.append((x, seed_y + 1))
        return connected

    def _nearest_perpendicular_projection_on_paths(
        self,
        point: tuple[float, float],
        paths: list[list[tuple[float, float]]],
    ) -> tuple[tuple[float, float] | None, float]:
        best_point: tuple[float, float] | None = None
        best_distance_sq: float | None = None
        for path in paths:
            if len(path) < 2:
                continue
            for start, end in zip(path, path[1:]):
                projection = self._project_point_perpendicular_to_segment(point, start, end)
                if projection is None:
                    continue
                dx = projection[0] - point[0]
                dy = projection[1] - point[1]
                distance_sq = dx * dx + dy * dy
                if best_distance_sq is None or distance_sq < best_distance_sq:
                    best_distance_sq = distance_sq
                    best_point = projection
        if best_point is None or best_distance_sq is None:
            return None, float("inf")
        return best_point, hypot(best_point[0] - point[0], best_point[1] - point[1])

    def _nearest_perpendicular_projection_on_linework(
        self,
        point: tuple[float, float],
        linework: list[CadastralLinework],
    ) -> tuple[float, float] | None:
        best_point: tuple[float, float] | None = None
        best_distance_sq: float | None = None
        for group in linework:
            for path in group.paths:
                if len(path) < 2:
                    continue
                for start, end in zip(path, path[1:]):
                    projection = self._project_point_perpendicular_to_segment(point, start, end)
                    if projection is None:
                        continue
                    dx = projection[0] - point[0]
                    dy = projection[1] - point[1]
                    distance_sq = dx * dx + dy * dy
                    if best_distance_sq is None or distance_sq < best_distance_sq:
                        best_distance_sq = distance_sq
                        best_point = projection
        return best_point

    def _project_point_perpendicular_to_segment(
        self,
        point: tuple[float, float],
        start: tuple[float, float],
        end: tuple[float, float],
    ) -> tuple[float, float] | None:
        dx = end[0] - start[0]
        dy = end[1] - start[1]
        length_sq = dx * dx + dy * dy
        if length_sq <= 1e-12:
            return None
        factor = ((point[0] - start[0]) * dx + (point[1] - start[1]) * dy) / length_sq
        if factor < 0.0 or factor > 1.0:
            return None
        return (start[0] + factor * dx, start[1] + factor * dy)

    def _template_bearing_from_pixel_vector(self, dx: float, dy: float) -> float:
        return (degrees(atan2(-dx, -dy)) + 360.0) % 360.0

    def _normalize_rotation_degrees(self, angle: float) -> float:
        normalized = (angle + 180.0) % 360.0 - 180.0
        return normalized

    @staticmethod
    def _template_asset_worker_count(asset_count: int) -> int:
        # Four simultaneous pages keeps memory use bounded while allowing four
        # independent background maps to download/render at the same time.
        return max(1, min(4, int(asset_count)))

    def _prefetch_template_background_maps(
        self,
        page_exporter: MapExporter | None,
        background_provider,
        layers: list[GeoTiffLayer],
        *,
        status_callback=None,
    ) -> dict[str, int] | None:
        if page_exporter is None or not layers:
            return None
        provider = background_provider or page_exporter.default_background_provider
        prefetch_maps = getattr(provider, "prefetch_maps", None)
        if not callable(prefetch_maps):
            return None
        requests_to_prepare: list[tuple[Bounds, tuple[int, int]]] = []
        for layer in layers:
            padded_bounds = layer.bounds.padded(
                max(1.0, min(4.0, max(layer.bounds.width, layer.bounds.height) * 0.1))
            )
            map_bounds = page_exporter._determine_map_bounds(padded_bounds)
            requests_to_prepare.append(
                (map_bounds, (page_exporter.map_width_px, page_exporter.map_height_px))
            )
        if status_callback is not None:
            status_callback(
                f"Haal Cyclomedia-luchtfoto's gebundeld op voor {len(requests_to_prepare)} kaart(en)..."
            )
        result = prefetch_maps(requests_to_prepare)
        if status_callback is not None and isinstance(result, dict):
            status_callback(
                "Cyclomedia-luchtfoto's opgehaald: "
                f"{int(result.get('maps', 0))} kaart(en) via "
                f"{int(result.get('clusters', 0))} gecombineerd(e) gebied(en)."
            )
        return result if isinstance(result, dict) else None

    def _prepare_template_slot_assets_batch(
        self,
        tasks: list[tuple[int, dict[str, object]]],
        *,
        status_callback=None,
    ) -> dict[int, PreparedTemplateSlotAssets]:
        if not tasks:
            return {}
        prepared_assets: dict[int, PreparedTemplateSlotAssets] = {}
        max_workers = self._template_asset_worker_count(len(tasks))
        with ThreadPoolExecutor(max_workers=max_workers, thread_name_prefix="template-assets") as executor:
            future_map = {
                executor.submit(self._prepare_template_slot_assets, **task_kwargs): layer_index
                for layer_index, task_kwargs in tasks
            }
            completed_assets = 0
            for future in as_completed(future_map):
                layer_index = future_map[future]
                try:
                    prepared_assets[layer_index] = future.result()
                except CadastralExportError:
                    raise
                except Exception as exc:
                    raise CadastralExportError(
                        f"Sjabloonvak {layer_index + 1} kon niet worden voorbereid: {exc}"
                    ) from exc
                completed_assets += 1
                if status_callback is not None:
                    status_callback(
                        f"Bereid sjabloonafbeeldingen voor... {completed_assets}/{len(tasks)}"
                    )
        return prepared_assets

    def _prepare_template_slot_assets(
        self,
        asset_dir: Path,
        layer: GeoTiffLayer,
        label: str,
        index: int,
        road_centerline_paths: list[list[tuple[float, float]]],
        terrain_boundary_paths: list[list[tuple[float, float]]],
        profile: TemplateCrossSectionProfile | None,
        trench_mode: str,
        centerline_color: tuple[int, int, int],
        label_color: tuple[int, int, int],
        page_exporter: MapExporter | None,
        dxf_overlays: list[DxfOverlay],
        map_comments: list[MapComment] | None,
        background_provider,
        background_attribution: str | None,
        location_client,
        reference_annotation: ProfileReferenceAnnotation | None = None,
        reverse_tiff_orientation: bool = False,
    ) -> PreparedTemplateSlotAssets:
        local_location_client = self._clone_template_location_client(location_client)
        local_background_provider = self._clone_template_background_provider(background_provider)
        local_page_exporter = self._clone_template_page_exporter(page_exporter, local_background_provider)
        return PreparedTemplateSlotAssets(
            formatted_address=self._reverse_geocoded_template_address(layer, local_location_client),
            comments_text=self._template_comments_text(layer, map_comments),
            raster_path=self._build_template_tiff_raster(
                asset_dir,
                layer,
                label,
                index,
                road_centerline_paths,
                terrain_boundary_paths,
                profile=profile,
                reverse_orientation=reverse_tiff_orientation,
            ),
            map_raster_path=self._build_template_map_raster(
                asset_dir,
                layer,
                label,
                index,
                trench_mode=trench_mode,
                centerline_color=centerline_color,
                label_color=label_color,
                page_exporter=local_page_exporter,
                dxf_overlays=dxf_overlays,
                map_comments=map_comments,
                background_provider=local_background_provider,
                background_attribution=background_attribution,
                reference_annotation=reference_annotation,
            ),
        )

    def _clone_template_background_provider(self, provider):
        if provider is None:
            return None
        if isinstance(provider, PdokWmsClient):
            return PdokWmsClient(
                layer_name=provider.layer_name,
                timeout=provider.timeout,
                retries=provider.retries,
                max_workers=min(getattr(provider, "max_workers", 6), 2),
                base_url=provider.base_url,
                transparent=provider.transparent,
            )
        if isinstance(provider, PdokWmtsTileClient):
            return PdokWmtsTileClient(
                layer_name=provider.layer_name,
                timeout=provider.timeout,
                max_workers=min(getattr(provider, "max_workers", 8), 2),
                retries=max(getattr(provider, "retries", 3), 4),
            )
        if isinstance(provider, PdokKadastralekaartWmtsTileClient):
            return PdokKadastralekaartWmtsTileClient(
                timeout=provider.timeout,
                max_workers=min(getattr(provider, "max_workers", 8), 2),
                retries=max(getattr(provider, "retries", 3), 4),
            )
        if isinstance(provider, OpenStreetMapTileClient):
            return OpenStreetMapTileClient(
                timeout=provider.timeout,
                min_zoom=provider.min_zoom,
                max_zoom=provider.max_zoom,
                max_workers=min(getattr(provider, "max_workers", 8), 2),
                retries=max(getattr(provider, "retries", 3), 4),
            )
        return provider

    def _clone_template_location_client(self, location_client):
        if location_client is None:
            return None
        if isinstance(location_client, PdokLocationClient):
            return PdokLocationClient(timeout=location_client.timeout)
        return location_client

    def _clone_template_page_exporter(
        self,
        page_exporter: MapExporter | None,
        background_provider_override,
    ) -> MapExporter | None:
        if page_exporter is None:
            return None
        return MapExporter(
            default_background_provider=background_provider_override or page_exporter.default_background_provider,
            renderer=page_exporter.renderer,
            dpi=page_exporter.dpi,
            scale=page_exporter.scale,
        )

    def _build_template_map_raster(
        self,
        asset_dir: Path,
        layer: GeoTiffLayer,
        label: str,
        index: int,
        trench_mode: str,
        centerline_color: tuple[int, int, int],
        label_color: tuple[int, int, int],
        page_exporter: MapExporter | None,
        dxf_overlays: list[DxfOverlay],
        map_comments: list[MapComment] | None,
        background_provider,
        background_attribution: str | None,
        reference_annotation: ProfileReferenceAnnotation | None = None,
    ) -> Path:
        image_path = self._unique_raster_copy_path(asset_dir, f"{label}_kaart.png")
        if page_exporter is not None:
            rendered = page_exporter.build_page_image(
                layer,
                dxf_overlays,
                map_comments=None,
                background_provider=background_provider,
                background_attribution=background_attribution,
                reference_annotation=reference_annotation,
                force_tiff_opacity=1.0,
            )
        else:
            rendered = self._render_template_map_image(
                layer,
                trench_mode=trench_mode,
                centerline_color=centerline_color,
                label_color=label_color,
            )
        try:
            rendered.save(image_path, format="PNG")
        except OSError as exc:
            raise CadastralExportError(f"Kaartafbeelding kon niet worden opgeslagen voor {label}: {exc}") from exc
        return image_path.resolve()

    @staticmethod
    def _format_template_rd_coordinate(value: float) -> str:
        return str(int(round(float(value))))

    def _template_profile_rd_text(self, x_coord: float, y_coord: float) -> str:
        return (
            f"RD {self._format_template_rd_coordinate(x_coord)}, "
            f"{self._format_template_rd_coordinate(y_coord)}"
        )

    @staticmethod
    def _format_template_profile_distance(value: float) -> str:
        rounded_value = round(float(value), 2)
        if abs(rounded_value) < 0.005:
            rounded_value = 0.0
        return f"{rounded_value:.2f}"

    def _template_reference_metadata_point(self, layer: GeoTiffLayer) -> tuple[float, float] | None:
        payload = layer.metadata.get(self.TEMPLATE_REFERENCE_POINT_METADATA_KEY)
        if not isinstance(payload, dict):
            return None
        try:
            x_coord = float(payload.get("x"))
            y_coord = float(payload.get("y"))
        except (TypeError, ValueError):
            return None
        return x_coord, y_coord

    def _template_reference_raw_point(
        self,
        layer: GeoTiffLayer,
        profile: TemplateCrossSectionProfile | None,
    ) -> tuple[float, float] | None:
        metadata_point = self._template_reference_metadata_point(layer)
        if metadata_point is not None:
            return metadata_point
        if profile is None:
            return None
        return float(profile.start_point.x), float(profile.start_point.y)

    def _template_reference_display_point(
        self,
        layer: GeoTiffLayer,
        profile: TemplateCrossSectionProfile | None,
    ) -> tuple[float, float] | None:
        reference_point = self._template_reference_raw_point(layer, profile)
        if reference_point is None:
            return None
        x_coord, y_coord = reference_point
        try:
            rotation_degrees = float(layer.metadata.get("rotation_degrees", 0.0) or 0.0)
        except (TypeError, ValueError):
            rotation_degrees = 0.0
        if abs(rotation_degrees) < 1e-9:
            return x_coord, y_coord
        center_x, center_y = layer.transform.pixel_to_world(layer.image.width / 2.0, layer.image.height / 2.0)
        return self._rotate_template_map_point(x_coord, y_coord, center_x, center_y, rotation_degrees)

    def _template_reference_chainage(
        self,
        layer: GeoTiffLayer,
        profile: TemplateCrossSectionProfile,
    ) -> float:
        reference_point = self._template_reference_raw_point(layer, profile)
        if reference_point is None or profile.axis_length <= 1e-9:
            return 0.0
        reference_x, reference_y = reference_point
        return (
            ((reference_x - float(profile.start_point.x)) * float(profile.axis_dx))
            + ((reference_y - float(profile.start_point.y)) * float(profile.axis_dy))
        ) / float(profile.axis_length)

    def _template_profile_reference_rd_text(
        self,
        layer: GeoTiffLayer,
        profile: TemplateCrossSectionProfile | None,
    ) -> str:
        reference_point = self._template_reference_raw_point(layer, profile)
        if reference_point is None:
            return ""
        return self._template_profile_rd_text(reference_point[0], reference_point[1])

    def _template_profile_reference_annotation(
        self,
        layer: GeoTiffLayer,
        profile: TemplateCrossSectionProfile | None,
    ) -> ProfileReferenceAnnotation | None:
        reference_point = self._template_reference_display_point(layer, profile)
        if reference_point is None:
            return None
        return ProfileReferenceAnnotation(x=reference_point[0], y=reference_point[1])

    @staticmethod
    def _rotate_template_map_point(
        x: float,
        y: float,
        center_x: float,
        center_y: float,
        rotation_degrees: float,
    ) -> tuple[float, float]:
        angle_radians = radians(rotation_degrees)
        offset_x = float(x) - float(center_x)
        offset_y = float(y) - float(center_y)
        rotated_x = float(center_x) + (offset_x * cos(angle_radians)) - (offset_y * sin(angle_radians))
        rotated_y = float(center_y) + (offset_x * sin(angle_radians)) + (offset_y * cos(angle_radians))
        return rotated_x, rotated_y

    @classmethod
    def _normalize_template_hex_color(cls, value: object, default: str | None = None) -> str:
        candidate = str(value or "").strip().upper()
        fallback = str(default or cls.TEMPLATE_MAAIVELD_DEFAULT_HEX).strip().upper()
        if len(candidate) == 7 and candidate.startswith("#"):
            try:
                int(candidate[1:], 16)
                return candidate
            except ValueError:
                pass
        return fallback

    @classmethod
    def _template_hex_to_rgb(cls, value: object, default: str | None = None) -> tuple[int, int, int]:
        normalized = cls._normalize_template_hex_color(value, default)
        return (
            int(normalized[1:3], 16),
            int(normalized[3:5], 16),
            int(normalized[5:7], 16),
        )

    def _template_bgt_surface_texts(
        self,
        profile: TemplateCrossSectionProfile,
        surface_features: list[BgtSurfaceFeature],
    ) -> dict[str, str]:
        if not surface_features or profile.axis_length <= 1e-9:
            return {}
        start = (float(profile.start_point.x), float(profile.start_point.y))
        end = (float(profile.end_point.x), float(profile.end_point.y))
        unit_dx = (end[0] - start[0]) / float(profile.axis_length)
        unit_dy = (end[1] - start[1]) / float(profile.axis_length)
        probes = {
            "start": LineString(
                [
                    (start[0] - unit_dx, start[1] - unit_dy),
                    start,
                ]
            ),
            "middle": LineString([start, end]),
            "end": LineString(
                [
                    end,
                    (end[0] + unit_dx, end[1] + unit_dy),
                ]
            ),
        }
        result: dict[str, str] = {}
        for key, probe in probes.items():
            dominant = self._dominant_bgt_physical_appearance(probe, surface_features)
            if dominant:
                result[key] = dominant
        return result

    @staticmethod
    def _dominant_bgt_physical_appearance(
        probe: LineString,
        surface_features: list[BgtSurfaceFeature],
    ) -> str | None:
        lengths: dict[str, float] = {}
        display_values: dict[str, str] = {}
        for feature in surface_features:
            physical_appearance = str(feature.physical_appearance or "").strip()
            if not physical_appearance:
                continue
            try:
                overlap_length = float(probe.intersection(feature.geometry).length)
            except Exception:
                continue
            if overlap_length <= 1e-8:
                continue
            normalized = physical_appearance.casefold()
            lengths[normalized] = lengths.get(normalized, 0.0) + overlap_length
            display_values.setdefault(normalized, physical_appearance)
        if not lengths:
            return None
        winning_key = min(lengths, key=lambda key: (-lengths[key], key))
        return display_values[winning_key]

    def _template_maaiveld_fill_segments(
        self,
        layer: GeoTiffLayer,
        *,
        use_custom_values: bool,
        automatic_values: dict[str, str] | None = None,
    ) -> dict[str, dict[str, object]]:
        default_text = "INVULLEN"
        default_hex = self.TEMPLATE_MAAIVELD_DEFAULT_HEX
        resolved_automatic_values = automatic_values or {}
        segments: dict[str, dict[str, object]] = {
            key: {
                "text": str(resolved_automatic_values.get(key) or "").strip() or default_text,
                "color": default_hex,
                "rgb": self._template_hex_to_rgb(default_hex),
            }
            for key in ("start", "middle", "end")
        }
        if not use_custom_values:
            return segments
        payload = layer.metadata.get(self.TEMPLATE_MAAIVELD_METADATA_KEY)
        if not isinstance(payload, dict):
            return segments
        for key in segments:
            item = payload.get(key)
            if not isinstance(item, dict):
                continue
            color_hex = self._normalize_template_hex_color(item.get("color"), default_hex)
            text_value = str(item.get("text", default_text) or "").strip()
            if not text_value or text_value.casefold() == default_text.casefold():
                text_value = str(segments[key]["text"])
            segments[key] = {
                "text": text_value,
                "color": color_hex,
                "rgb": self._template_hex_to_rgb(color_hex),
            }
        return segments

    def _template_dekband_rows(self, layer: GeoTiffLayer) -> tuple[dict[str, float], ...]:
        payload = layer.metadata.get(self.TEMPLATE_DEKBAND_METADATA_KEY)
        if not isinstance(payload, list):
            return ()
        rows: list[dict[str, float]] = []
        for item in payload:
            if not isinstance(item, dict):
                continue
            try:
                start_chainage = float(item.get("start_chainage"))
                end_chainage = float(item.get("end_chainage"))
                start_depth = max(0.0, float(item.get("start_depth", 0.0)))
                end_depth = max(0.0, float(item.get("end_depth", start_depth)))
            except (TypeError, ValueError):
                continue
            source_name = str(item.get("source_name", "") or item.get("label", "") or "").strip() or "Dekband"
            if abs(end_chainage - start_chainage) <= 1e-6:
                continue
            if end_chainage < start_chainage:
                start_chainage, end_chainage = end_chainage, start_chainage
                start_depth, end_depth = end_depth, start_depth
            rows.append(
                {
                    "start_chainage": start_chainage,
                    "end_chainage": end_chainage,
                    "start_depth": start_depth,
                    "end_depth": end_depth,
                    "source_name": source_name,
                }
            )
        return tuple(rows)

    def _dataset_with_template_dekbanden(
        self,
        layer: GeoTiffLayer,
        dataset: KickTheMapObjectDataset,
        layer_rules: tuple[ObjectLayerRule, ...],
        road_centerline_paths: list[list[tuple[float, float]]],
        terrain_boundary_paths: list[list[tuple[float, float]]],
        fallback_marker_scale: float,
        reverse_profile_direction: bool = False,
    ) -> KickTheMapObjectDataset:
        dekband_rows = self._template_dekband_rows(layer)
        if not dekband_rows:
            return dataset
        base_profile = self._build_template_cross_section_profile(
            dataset,
            layer_rules,
            road_centerline_paths,
            terrain_boundary_paths,
            fallback_marker_scale,
            reverse_profile_direction,
        )
        if base_profile is None:
            return dataset
        combined_polylines = list(dataset.polylines)
        for index, row in enumerate(dekband_rows, start=1):
            polyline = self._template_dekband_polyline(row, base_profile, index)
            if polyline is not None:
                combined_polylines.append(polyline)
        if len(combined_polylines) == len(dataset.polylines):
            return dataset
        return KickTheMapObjectDataset(
            job_id=dataset.job_id,
            job_title=dataset.job_title,
            source_path=dataset.source_path,
            points=dataset.points,
            polylines=tuple(combined_polylines),
            cross_section_start_xy=dataset.cross_section_start_xy,
        )

    def _template_dekband_polyline(
        self,
        row: dict[str, float],
        profile: TemplateCrossSectionProfile,
        index: int,
    ) -> KickTheMapObjectPolyline | None:
        if profile.axis_length <= 1e-9:
            return None
        try:
            start_chainage = max(0.0, min(float(profile.axis_length), float(row["start_chainage"])))
            end_chainage = max(0.0, min(float(profile.axis_length), float(row["end_chainage"])))
            start_depth = max(0.0, float(row["start_depth"]))
            end_depth = max(0.0, float(row["end_depth"]))
        except (KeyError, TypeError, ValueError):
            return None
        if end_chainage < start_chainage:
            start_chainage, end_chainage = end_chainage, start_chainage
            start_depth, end_depth = end_depth, start_depth
        if abs(end_chainage - start_chainage) <= 1e-6:
            return None
        start_world = self._template_profile_world_point(profile, start_chainage)
        end_world = self._template_profile_world_point(profile, end_chainage)
        start_surface_z = self._template_profile_surface_z(profile, start_chainage)
        end_surface_z = self._template_profile_surface_z(profile, end_chainage)
        source_name = str(row.get("source_name", "") or "").strip() or "Dekband"
        object_name = source_name if source_name.casefold() != "dekband" else f"Dekband {index}"
        return KickTheMapObjectPolyline(
            object_name=object_name,
            source_name=source_name,
            vertices=(
                KickTheMapPolylineVertex(
                    x=float(start_world[0]),
                    y=float(start_world[1]),
                    z=float(start_surface_z - start_depth),
                ),
                KickTheMapPolylineVertex(
                    x=float(end_world[0]),
                    y=float(end_world[1]),
                    z=float(end_surface_z - end_depth),
                ),
            ),
            attribute_3="dekband",
        )

    def _template_profile_world_point(
        self,
        profile: TemplateCrossSectionProfile,
        chainage: float,
    ) -> tuple[float, float]:
        if profile.axis_length <= 1e-9:
            return float(profile.start_point.x), float(profile.start_point.y)
        normalized_chainage = max(0.0, min(float(profile.axis_length), float(chainage)))
        ratio = normalized_chainage / float(profile.axis_length)
        return (
            float(profile.start_point.x) + (float(profile.axis_dx) * ratio),
            float(profile.start_point.y) + (float(profile.axis_dy) * ratio),
        )

    def _template_profile_surface_z(
        self,
        profile: TemplateCrossSectionProfile,
        chainage: float,
    ) -> float:
        start_z = float(profile.start_point.z or 0.0)
        end_z = float(profile.end_point.z or 0.0)
        if profile.axis_length <= 1e-9:
            return start_z
        normalized_chainage = max(0.0, min(float(profile.axis_length), float(chainage)))
        ratio = normalized_chainage / float(profile.axis_length)
        return start_z + ((end_z - start_z) * ratio)

    @staticmethod
    def _is_dekband_feature(point: KickTheMapObjectFeature) -> bool:
        if not isinstance(point, KickTheMapObjectPolyline):
            return False
        return str(point.attribute_3 or "").strip().strip("\"'").lower() == "dekband"

    def _template_title(self, layer: GeoTiffLayer, fallback_index: int) -> str:
        base_name = self._template_proefsleuf_label(layer, fallback_index)
        numbered_label = re.search(r"(?i)(\d+)([A-Z]+)?", base_name)
        if numbered_label:
            suffix = str(numbered_label.group(2) or "").upper()
            return f"Proefsleuf {int(numbered_label.group(1))}{suffix}"
        return f"Proefsleuf {base_name}"

    def _reverse_geocoded_template_address(self, layer: GeoTiffLayer, location_client) -> str | None:
        if location_client is None:
            return None
        try:
            results = location_client.reverse_lookup(layer.bounds.center_x, layer.bounds.center_y, rows=1)
        except Exception:
            return None
        if not results:
            return None
        label = str(results[0].label).strip()
        if not label:
            return None
        if "," in label:
            head, tail = label.split(",", 1)
            return f"{head.strip()},\\P{tail.strip()}"
        return label

    def _template_comments_text(self, layer: GeoTiffLayer, map_comments: list[MapComment] | None) -> str:
        if not map_comments:
            return "Opmerkingen:"
        span = max(layer.bounds.width, layer.bounds.height)
        hit_bounds = layer.bounds.padded(max(0.5, min(2.0, span * 0.08)))
        lines: list[str] = []
        seen: set[str] = set()
        for comment in map_comments:
            if not hit_bounds.contains(comment.x, comment.y):
                continue
            for raw_line in str(comment.text).splitlines():
                text = raw_line.strip()
                if not text:
                    continue
                normalized = text.casefold()
                if normalized in seen:
                    continue
                seen.add(normalized)
                lines.append(text)
        if not lines:
            return "Opmerkingen:"
        return "Opmerkingen:\\P" + "\\P".join(lines)

    def _render_template_map_image(
        self,
        layer: GeoTiffLayer,
        trench_mode: str,
        centerline_color: tuple[int, int, int],
        label_color: tuple[int, int, int],
    ) -> Image.Image:
        target_width = 1400
        target_height = 990
        aspect_ratio = target_width / target_height
        span = max(layer.bounds.width, layer.bounds.height, 0.1)
        padding = max(2.0, min(12.0, span * 0.35))
        render_bounds = layer.bounds.padded(padding).expand_to_aspect_ratio(aspect_ratio)

        try:
            linework = self.wfs_client.fetch_linework(render_bounds)
            text_labels = self.wfs_client.fetch_text_labels(render_bounds)
        except CadastralWfsError as exc:
            raise CadastralExportError(str(exc)) from exc

        image = Image.new("RGBA", (target_width, target_height), (255, 255, 255, 255))
        draw = ImageDraw.Draw(image)
        font = self._template_font(24)

        for group in linework:
            color, width = self._template_line_style(group.layer_name)
            for path in group.paths:
                points = [self._world_to_image_point(render_bounds, target_width, target_height, x, y) for x, y in path]
                if len(points) >= 2:
                    draw.line(points, fill=color, width=width)

        for text_label in text_labels:
            position = self._world_to_image_point(
                render_bounds,
                target_width,
                target_height,
                text_label.position[0],
                text_label.position[1],
            )
            self._draw_rotated_map_text(draw, image, position, text_label.text, text_label.rotation, font)

        if trench_mode == self.TRENCH_MODE_CENTERLINE:
            centerline = self._proefsleuf_centerline(layer, linework)
            centerline_points = [
                self._world_to_image_point(render_bounds, target_width, target_height, x, y) for x, y in centerline
            ]
            if len(centerline_points) >= 2:
                draw.line(centerline_points, fill=centerline_color + (255,), width=6)
        else:
            polygon = self._proefsleuf_polygon(layer)
            polygon_points = [
                self._world_to_image_point(render_bounds, target_width, target_height, x, y) for x, y in polygon
            ]
            if len(polygon_points) >= 3:
                draw.polygon(polygon_points, fill=centerline_color + (75,), outline=centerline_color + (255,))

        frame_color = tuple(min(255, channel + 20) for channel in label_color) + (255,)
        draw.rectangle((0, 0, target_width - 1, target_height - 1), outline=frame_color, width=4)
        return image.convert("RGB")

    def _template_line_style(self, layer_name: str) -> tuple[tuple[int, int, int, int], int]:
        style_map = {
            self.BGT_VECTOR_OUTLINE_LAYER: (self.BGT_VECTOR_OUTLINE_RGB + (255,), 2),
            self.BGT_ROADPART_LAYER: (self.BGT_ROADPART_RGB + (255,), 2),
            "KAD_PERCEEL": ((185, 185, 185, 255), 2),
            "KAD_GRENS": ((125, 125, 125, 255), 3),
            "KAD_BEBOUWING": ((165, 165, 165, 255), 3),
            "KAD_STRAATNAAM": ((115, 115, 115, 255), 2),
            "KAD_HUISNUMMER": ((100, 100, 100, 255), 2),
        }
        return style_map.get(layer_name, ((140, 140, 140, 255), 2))

    def _template_font(self, size: int) -> ImageFont.ImageFont:
        for candidate in ("C:/Windows/Fonts/segoeui.ttf", "C:/Windows/Fonts/arial.ttf"):
            try:
                return ImageFont.truetype(candidate, size)
            except OSError:
                continue
        return ImageFont.load_default()

    def _draw_rotated_map_text(
        self,
        draw: ImageDraw.ImageDraw,
        image: Image.Image,
        position: tuple[float, float],
        text: str,
        rotation: float,
        font: ImageFont.ImageFont,
    ) -> None:
        normalized = text.strip()
        if not normalized:
            return
        bbox = font.getbbox(normalized)
        width = max(1, bbox[2] - bbox[0] + 16)
        height = max(1, bbox[3] - bbox[1] + 12)
        overlay = Image.new("RGBA", (width, height), (255, 255, 255, 0))
        overlay_draw = ImageDraw.Draw(overlay)
        overlay_draw.text((8 - bbox[0], 6 - bbox[1]), normalized, fill=(110, 110, 110, 255), font=font)
        rotated = overlay.rotate((-rotation) % 360.0, expand=True, resample=Image.Resampling.BICUBIC)
        paste_x = int(round(position[0] - rotated.width / 2.0))
        paste_y = int(round(position[1] - rotated.height / 2.0))
        image.alpha_composite(rotated, (paste_x, paste_y))

    def _world_to_image_point(
        self,
        bounds: Bounds,
        width: int,
        height: int,
        x: float,
        y: float,
    ) -> tuple[float, float]:
        px = ((x - bounds.min_x) / max(bounds.width, 1e-9)) * (width - 1)
        py = (1.0 - ((y - bounds.min_y) / max(bounds.height, 1e-9))) * (height - 1)
        return px, py

    def _add_box_image(
        self,
        document: ezdxf.EzDxfDocument,
        modelspace,
        image_path: Path,
        box_bounds: Bounds,
        image_def_name: str,
        inset: float = 0.04,
        preserve_aspect: bool = True,
    ) -> None:
        with Image.open(image_path) as image:
            pixel_size = (max(1, image.width), max(1, image.height))
        if preserve_aspect:
            fitted_bounds = self._fit_image_to_box(box_bounds, pixel_size, inset=inset)
        elif inset > 0.0:
            fitted_bounds = Bounds(
                box_bounds.min_x + inset,
                box_bounds.min_y + inset,
                box_bounds.max_x - inset,
                box_bounds.max_y - inset,
            )
        else:
            fitted_bounds = box_bounds
        image_def = document.add_image_def(
            filename=str(image_path),
            size_in_pixel=pixel_size,
            name=self._unique_image_def_name(document, image_def_name),
        )
        modelspace.add_image(
            image_def,
            insert=(fitted_bounds.min_x, fitted_bounds.min_y, 0.0),
            size_in_units=(fitted_bounds.width, fitted_bounds.height),
            dxfattribs={
                "layer": "PROEFSLEUF_TEMPLATE_IMAGES",
                "flags": 11,
            },
        )

    def _fit_image_to_box(self, box_bounds: Bounds, pixel_size: tuple[int, int], inset: float) -> Bounds:
        padded = Bounds(
            box_bounds.min_x + inset,
            box_bounds.min_y + inset,
            box_bounds.max_x - inset,
            box_bounds.max_y - inset,
        )
        if padded.width <= 0 or padded.height <= 0:
            return box_bounds
        image_aspect = pixel_size[0] / max(pixel_size[1], 1)
        box_aspect = padded.width / max(padded.height, 1e-9)
        if image_aspect >= box_aspect:
            fitted_width = padded.width
            fitted_height = fitted_width / max(image_aspect, 1e-9)
        else:
            fitted_height = padded.height
            fitted_width = fitted_height * image_aspect
        return Bounds(
            padded.center_x - fitted_width / 2.0,
            padded.center_y - fitted_height / 2.0,
            padded.center_x + fitted_width / 2.0,
            padded.center_y + fitted_height / 2.0,
        )

    def _overview_padding(self, bounds: Bounds) -> float:
        span = max(bounds.width, bounds.height)
        return max(60.0, min(180.0, span * 0.35))

    def _write_dxf(
        self,
        output_path: Path,
        tiff_layers: list[GeoTiffLayer],
        linework: list[CadastralLinework],
        text_labels: list[CadastralTextLabel],
        export_bounds: Bounds,
        trench_mode: str,
        include_tiff_images: bool,
        label_gap: float,
        centerline_color: tuple[int, int, int],
        label_color: tuple[int, int, int],
    ) -> None:
        document = ezdxf.new("R2018", setup=True)
        document.units = 6
        document.header["$EXTMIN"] = (export_bounds.min_x, export_bounds.min_y, 0.0)
        document.header["$EXTMAX"] = (export_bounds.max_x, export_bounds.max_y, 0.0)
        modelspace = document.modelspace()
        if include_tiff_images:
            document.set_raster_variables(frame=0, quality=1, units="m")

        self._setup_layers(document, centerline_color=centerline_color, label_color=label_color)
        self._setup_text_styles(document)
        self._populate_overview_modelspace(
            document,
            modelspace,
            output_path,
            tiff_layers,
            linework,
            text_labels,
            export_bounds,
            trench_mode=trench_mode,
            include_tiff_images=include_tiff_images,
            label_gap=label_gap,
            centerline_color=centerline_color,
            label_color=label_color,
        )
        try:
            document.saveas(output_path)
        except Exception as exc:
            raise CadastralExportError(f"DXF kon niet worden opgeslagen: {exc}") from exc

    def _populate_overview_modelspace(
        self,
        document: ezdxf.EzDxfDocument,
        modelspace,
        output_path: Path,
        tiff_layers: list[GeoTiffLayer],
        linework: list[CadastralLinework],
        text_labels: list[CadastralTextLabel],
        export_bounds: Bounds,
        trench_mode: str,
        include_tiff_images: bool,
        label_gap: float,
        centerline_color: tuple[int, int, int],
        label_color: tuple[int, int, int],
    ) -> None:
        if include_tiff_images:
            prepared_rasters = self._prepare_tiff_raster_files(output_path, tiff_layers)
            for index, prepared_raster in enumerate(prepared_rasters, start=1):
                self._add_tiff_image(document, modelspace, prepared_raster.layer, prepared_raster.raster_path, index)

        for group in linework:
            for path in group.paths:
                if len(path) < 2:
                    continue
                dxfattribs = {
                    "layer": group.layer_name,
                    "color": 8,
                    "lineweight": 13,
                }
                if group.layer_name == self.BGT_VECTOR_OUTLINE_LAYER:
                    dxfattribs["true_color"] = rgb2int(self.BGT_VECTOR_OUTLINE_RGB)
                    dxfattribs["lineweight"] = 9
                elif group.layer_name == self.BGT_ROADPART_LAYER:
                    dxfattribs["true_color"] = rgb2int(self.BGT_ROADPART_RGB)
                    dxfattribs["lineweight"] = 9
                modelspace.add_lwpolyline(
                    path,
                    dxfattribs=dxfattribs,
                )

        for text_label in text_labels:
            self._add_cadastral_text_label(modelspace, text_label)

        proefsleuf_bounds: list[Bounds] = []
        placed_label_bounds: list[Bounds] = []
        use_centerline = trench_mode == self.TRENCH_MODE_CENTERLINE
        geometry_results: list[tuple[GeoTiffLayer, list[tuple[float, float]], Bounds]] = [None] * len(tiff_layers)  # type: ignore[assignment]

        def prepare_layer_geometry(layer: GeoTiffLayer) -> tuple[list[tuple[float, float]], Bounds]:
            if use_centerline:
                centerline = self._proefsleuf_centerline(layer, linework)
                return centerline, self._bounds_from_points(centerline)
            polygon = self._proefsleuf_polygon(layer)
            return polygon, self._bounds_from_points(polygon)

        max_geometry_workers = max(1, min(4, len(tiff_layers)))
        with ThreadPoolExecutor(max_workers=max_geometry_workers, thread_name_prefix="overview-geometry") as executor:
            future_map = {
                executor.submit(prepare_layer_geometry, layer): index
                for index, layer in enumerate(tiff_layers)
            }
            for future in as_completed(future_map):
                layer_index = future_map[future]
                geometry, trench_bounds = future.result()
                geometry_results[layer_index] = (tiff_layers[layer_index], geometry, trench_bounds)

        for prepared in geometry_results:
            if prepared is None:
                continue
            layer, geometry, trench_bounds = prepared
            proefsleuf_bounds.append(trench_bounds)
            if use_centerline:
                self._add_proefsleuf_centerline(modelspace, geometry, centerline_color=centerline_color)
            else:
                self._add_proefsleuf_polygon(modelspace, geometry, fill=not include_tiff_images)

        for index, trench_bounds in enumerate(proefsleuf_bounds, start=1):
            layer = tiff_layers[index - 1]
            label = self._proefsleuf_label(layer, index)
            label_center = self._pick_label_position(
                trench_bounds,
                label,
                proefsleuf_bounds,
                placed_label_bounds,
                label_gap=label_gap,
            )
            self._add_proefsleuf_label(modelspace, label, label_center, label_color=label_color)
            placed_label_bounds.append(self._label_bounds(label, label_center))

    def _setup_layers(
        self,
        document: ezdxf.EzDxfDocument,
        centerline_color: tuple[int, int, int],
        label_color: tuple[int, int, int],
    ) -> None:
        layer_specs = {
            self.BGT_VECTOR_OUTLINE_LAYER: {
                "color": 8,
                "true_color": rgb2int(self.BGT_VECTOR_OUTLINE_RGB),
                "lineweight": 9,
            },
            self.BGT_ROADPART_LAYER: {
                "color": 8,
                "true_color": rgb2int(self.BGT_ROADPART_RGB),
                "lineweight": 9,
            },
            "KAD_PERCEEL": {"color": 8, "true_color": rgb2int((150, 150, 150)), "lineweight": 13},
            "KAD_GRENS": {"color": 8, "true_color": rgb2int((110, 110, 110)), "lineweight": 18},
            "KAD_BEBOUWING": {"color": 8, "true_color": rgb2int((175, 175, 175)), "lineweight": 18},
            "KAD_STRAATNAAM": {"color": 8, "true_color": rgb2int((115, 115, 115))},
            "KAD_HUISNUMMER": {"color": 8, "true_color": rgb2int((95, 95, 95))},
            "PROEFSLEUVEN_VLAK": {"color": 7, "true_color": rgb2int((255, 255, 255))},
            "PROEFSLEUVEN_OMTREK": {"color": 8, "true_color": rgb2int((120, 120, 120)), "lineweight": 25},
            "PROEFSLEUVEN_HARTLIJN": {
                "color": 7,
                "true_color": rgb2int(centerline_color),
                "lineweight": 25,
            },
            "PROEFSLEUVEN_TIFF": {"color": 7},
            "PROEFSLEUVEN_LABEL": {"color": 7, "true_color": rgb2int(label_color)},
        }
        for layer_name, attributes in layer_specs.items():
            if layer_name in document.layers:
                continue
            document.layers.add(name=layer_name, dxfattribs=attributes)

    def _setup_text_styles(self, document: ezdxf.EzDxfDocument) -> None:
        if self.LABEL_STYLE not in document.styles:
            document.styles.add(self.LABEL_STYLE, font="LiberationMono-Regular.ttf")
        if self.CADASTRAL_LABEL_STYLE not in document.styles:
            document.styles.add(self.CADASTRAL_LABEL_STYLE, font="LiberationMono-Regular.ttf")

    def _proefsleuf_polygon(self, layer: GeoTiffLayer) -> list[tuple[float, float]]:
        if is_virtual_trench_layer(layer):
            return virtual_trench_polygon(layer)
        pixel_polygon = self._extract_pixel_polygon(layer)
        if len(pixel_polygon) < 3:
            return self._rectangle_polygon(layer.bounds)
        return [layer.transform.pixel_to_world(float(col), float(row)) for col, row in pixel_polygon]

    def _proefsleuf_centerline(
        self,
        layer: GeoTiffLayer,
        linework: list[CadastralLinework],
    ) -> list[tuple[float, float]]:
        if is_virtual_trench_layer(layer):
            return virtual_trench_centerline(layer)
        visible_mask = self._extract_component_mask(layer, prefer_trench=False)
        if visible_mask is not None:
            centerline = self._centerline_from_kadastral_projection(layer, visible_mask, linework)
            if len(centerline) >= 2:
                return centerline

        trench_mask = self._extract_component_mask(layer, prefer_trench=True)
        if trench_mask is None:
            trench_mask = visible_mask
        if trench_mask is None:
            return self._fallback_world_centerline(layer.bounds)

        pixel_centerline = self._centerline_from_mask(trench_mask)
        if len(pixel_centerline) < 2:
            return self._fallback_world_centerline(layer.bounds)
        return [
            layer.transform.pixel_to_world(float(col) + 0.5, float(row) + 0.5)
            for col, row in pixel_centerline
        ]

    def _centerline_from_kadastral_projection(
        self,
        layer: GeoTiffLayer,
        visible_mask: np.ndarray,
        linework: list[CadastralLinework],
    ) -> list[tuple[float, float]]:
        center_px = (layer.image.width / 2.0, layer.image.height / 2.0)
        center_world = layer.transform.pixel_to_world(center_px[0], center_px[1])
        projection_world = self._nearest_perpendicular_projection_on_linework(center_world, linework)
        if projection_world is None:
            return []

        projection_px = layer.transform.world_to_pixel(projection_world[0], projection_world[1])
        direction = np.array(
            [projection_px[0] - center_px[0], projection_px[1] - center_px[1]],
            dtype=float,
        )
        endpoints = self._mask_line_endpoints(visible_mask, center_px, direction)
        if endpoints is None:
            return []
        return [
            layer.transform.pixel_to_world(float(point[0]), float(point[1]))
            for point in endpoints
        ]

    def _mask_line_endpoints(
        self,
        mask: np.ndarray,
        center_px: tuple[float, float],
        direction: np.ndarray,
    ) -> list[tuple[float, float]] | None:
        norm = float(np.linalg.norm(direction))
        if norm <= 1e-6:
            return None
        unit = direction / norm
        max_span = hypot(mask.shape[0], mask.shape[1]) + 2.0
        step = 0.25

        runs: list[tuple[float, float]] = []
        current_start: float | None = None
        previous_t: float | None = None
        t = -max_span
        while t <= max_span + 1e-9:
            sample_x = center_px[0] + unit[0] * t
            sample_y = center_px[1] + unit[1] * t
            inside = self._mask_contains(mask, sample_x, sample_y)
            if inside and current_start is None:
                current_start = t
            elif not inside and current_start is not None and previous_t is not None:
                runs.append((current_start, previous_t))
                current_start = None
            previous_t = t
            t += step
        if current_start is not None and previous_t is not None:
            runs.append((current_start, previous_t))

        if not runs:
            return None

        run = next((item for item in runs if item[0] <= 0.0 <= item[1]), None)
        if run is None:
            run = min(
                runs,
                key=lambda item: (
                    min(abs(item[0]), abs(item[1])),
                    -(item[1] - item[0]),
                ),
            )

        start_t, end_t = run
        start_point = (center_px[0] + unit[0] * start_t, center_px[1] + unit[1] * start_t)
        end_point = (center_px[0] + unit[0] * end_t, center_px[1] + unit[1] * end_t)
        return [start_point, end_point]

    def _mask_contains(self, mask: np.ndarray, x: float, y: float) -> bool:
        col = int(round(x))
        row = int(round(y))
        if row < 0 or col < 0 or row >= mask.shape[0] or col >= mask.shape[1]:
            return False
        return bool(mask[row, col])

    def _extract_pixel_polygon(self, layer: GeoTiffLayer) -> list[tuple[int, int]]:
        mask = self._extract_component_mask(layer, prefer_trench=False)
        if mask is None:
            return []
        loops = self._mask_to_loops(mask)
        if not loops:
            return []
        polygon = max(loops, key=lambda loop: abs(self._polygon_area(loop)))
        return self._simplify_loop(polygon)

    def _extract_component_mask(self, layer: GeoTiffLayer, prefer_trench: bool) -> np.ndarray | None:
        rgba = np.array(layer.image.convert("RGBA"), dtype=np.uint8)
        alpha_mask = rgba[:, :, 3] > self.MASK_ALPHA_THRESHOLD
        if not alpha_mask.any():
            return None
        if prefer_trench:
            grayscale = (
                rgba[:, :, 0].astype(np.float32) * 0.299
                + rgba[:, :, 1].astype(np.float32) * 0.587
                + rgba[:, :, 2].astype(np.float32) * 0.114
            )
            trench_component = self._extract_trench_component(alpha_mask, grayscale)
            if trench_component is not None:
                return trench_component
        return self._extract_alpha_component(alpha_mask)

    def _extract_trench_component(self, alpha_mask: np.ndarray, grayscale: np.ndarray) -> np.ndarray | None:
        central_values = grayscale[self._central_window_mask(alpha_mask.shape) & alpha_mask]
        if central_values.size < 64:
            central_values = grayscale[alpha_mask]
        if central_values.size == 0:
            return None

        base_threshold = float(min(145.0, max(85.0, np.percentile(central_values, 20) + 18.0)))

        for threshold in (base_threshold, min(155.0, base_threshold + 12.0), min(165.0, base_threshold + 24.0)):
            candidate_mask = alpha_mask & (grayscale <= threshold)
            if not candidate_mask.any():
                continue
            candidate_mask = self._binary_open(candidate_mask, iterations=1)
            component, _score = self._best_component(candidate_mask)
            if component is None:
                continue
            component = self._fill_holes(component)
            component = self._binary_close(component, iterations=1)
            area_ratio = float(component.sum()) / float(component.size)
            if not (self.MIN_TRENCH_AREA_RATIO <= area_ratio <= self.MAX_TRENCH_AREA_RATIO):
                continue
            return component

        return None

    def _extract_alpha_component(self, alpha_mask: np.ndarray) -> np.ndarray | None:
        rows, cols = np.nonzero(alpha_mask)
        if rows.size == 0:
            return None
        center_row = (alpha_mask.shape[0] - 1) / 2.0
        center_col = (alpha_mask.shape[1] - 1) / 2.0
        distances = (rows - center_row) ** 2 + (cols - center_col) ** 2
        seed_index = int(np.argmin(distances))
        seed_row = int(rows[seed_index])
        seed_col = int(cols[seed_index])
        return self._component_from_seed(alpha_mask, seed_row, seed_col)

    def _central_window_mask(self, shape: tuple[int, int]) -> np.ndarray:
        height, width = shape
        mask = np.zeros(shape, dtype=bool)
        min_row = int(height * 0.2)
        max_row = int(height * 0.8)
        min_col = int(width * 0.2)
        max_col = int(width * 0.8)
        mask[min_row:max_row, min_col:max_col] = True
        return mask

    def _best_component(self, mask: np.ndarray) -> tuple[np.ndarray | None, float]:
        accelerated = native_accel.best_component(
            mask,
            self.MIN_TRENCH_AREA_RATIO,
            self.MAX_TRENCH_AREA_RATIO,
        )
        if accelerated is not None:
            return accelerated
        height, width = mask.shape
        image_area = float(height * width)
        center_y = height / 2.0
        center_x = width / 2.0
        seen = np.zeros_like(mask, dtype=bool)
        best_component: np.ndarray | None = None
        best_score = float("inf")

        for row in range(height):
            for col in range(width):
                if not mask[row, col] or seen[row, col]:
                    continue
                component, area, bounds, centroid = self._collect_component(mask, seen, row, col)
                area_ratio = area / image_area
                if area_ratio < self.MIN_TRENCH_AREA_RATIO or area_ratio > self.MAX_TRENCH_AREA_RATIO:
                    continue
                min_col, min_row, max_col, max_row = bounds
                bbox_width = max_col - min_col + 1
                bbox_height = max_row - min_row + 1
                fill_ratio = area / float(bbox_width * bbox_height)
                aspect_ratio = max(bbox_width, bbox_height) / max(1.0, min(bbox_width, bbox_height))
                center_distance = hypot(centroid[0] - center_x, centroid[1] - center_y)
                score = (
                    center_distance
                    + abs(aspect_ratio - 2.0) * 18.0
                    + abs(fill_ratio - 0.55) * 80.0
                )
                if score < best_score:
                    best_score = score
                    best_component = component
        return best_component, best_score

    def _collect_component(
        self,
        mask: np.ndarray,
        seen: np.ndarray,
        seed_row: int,
        seed_col: int,
    ) -> tuple[np.ndarray, int, tuple[int, int, int, int], tuple[float, float]]:
        height, width = mask.shape
        component = np.zeros_like(mask, dtype=bool)
        stack: list[tuple[int, int]] = [(seed_row, seed_col)]
        seen[seed_row, seed_col] = True
        component[seed_row, seed_col] = True
        area = 0
        sum_x = 0.0
        sum_y = 0.0
        min_row = max_row = seed_row
        min_col = max_col = seed_col

        while stack:
            row, col = stack.pop()
            area += 1
            sum_x += col
            sum_y += row
            min_row = min(min_row, row)
            max_row = max(max_row, row)
            min_col = min(min_col, col)
            max_col = max(max_col, col)
            for next_row, next_col in (
                (row - 1, col),
                (row + 1, col),
                (row, col - 1),
                (row, col + 1),
            ):
                if (
                    0 <= next_row < height
                    and 0 <= next_col < width
                    and mask[next_row, next_col]
                    and not seen[next_row, next_col]
                ):
                    seen[next_row, next_col] = True
                    component[next_row, next_col] = True
                    stack.append((next_row, next_col))

        centroid = (sum_x / max(1, area), sum_y / max(1, area))
        return component, area, (min_col, min_row, max_col, max_row), centroid

    def _component_from_seed(self, mask: np.ndarray, seed_row: int, seed_col: int) -> np.ndarray:
        accelerated = native_accel.component_from_seed(mask, seed_row, seed_col)
        if accelerated is not None:
            return accelerated
        height, width = mask.shape
        component = np.zeros_like(mask, dtype=bool)
        stack: list[tuple[int, int]] = [(seed_row, seed_col)]
        component[seed_row, seed_col] = True
        while stack:
            row, col = stack.pop()
            for next_row, next_col in (
                (row - 1, col),
                (row + 1, col),
                (row, col - 1),
                (row, col + 1),
            ):
                if (
                    0 <= next_row < height
                    and 0 <= next_col < width
                    and mask[next_row, next_col]
                    and not component[next_row, next_col]
                ):
                    component[next_row, next_col] = True
                    stack.append((next_row, next_col))
        return component

    def _fill_holes(self, mask: np.ndarray) -> np.ndarray:
        accelerated = native_accel.fill_holes(mask)
        if accelerated is not None:
            return accelerated
        height, width = mask.shape
        outside = np.zeros_like(mask, dtype=bool)
        stack: list[tuple[int, int]] = []

        for col in range(width):
            if not mask[0, col]:
                outside[0, col] = True
                stack.append((0, col))
            if not mask[height - 1, col]:
                outside[height - 1, col] = True
                stack.append((height - 1, col))
        for row in range(height):
            if not mask[row, 0]:
                outside[row, 0] = True
                stack.append((row, 0))
            if not mask[row, width - 1]:
                outside[row, width - 1] = True
                stack.append((row, width - 1))

        while stack:
            row, col = stack.pop()
            for next_row, next_col in (
                (row - 1, col),
                (row + 1, col),
                (row, col - 1),
                (row, col + 1),
            ):
                if (
                    0 <= next_row < height
                    and 0 <= next_col < width
                    and not mask[next_row, next_col]
                    and not outside[next_row, next_col]
                ):
                    outside[next_row, next_col] = True
                    stack.append((next_row, next_col))

        return mask | ~outside

    def _binary_open(self, mask: np.ndarray, iterations: int = 1) -> np.ndarray:
        result = mask.copy()
        for _ in range(iterations):
            result = self._binary_dilate(self._binary_erode(result))
        return result

    def _binary_close(self, mask: np.ndarray, iterations: int = 1) -> np.ndarray:
        result = mask.copy()
        for _ in range(iterations):
            result = self._binary_erode(self._binary_dilate(result))
        return result

    def _binary_dilate(self, mask: np.ndarray) -> np.ndarray:
        padded = np.pad(mask, 1, mode="constant", constant_values=False)
        slices = [
            padded[1 + row_offset : 1 + row_offset + mask.shape[0], 1 + col_offset : 1 + col_offset + mask.shape[1]]
            for row_offset in (-1, 0, 1)
            for col_offset in (-1, 0, 1)
        ]
        result = slices[0].copy()
        for item in slices[1:]:
            result |= item
        return result

    def _binary_erode(self, mask: np.ndarray) -> np.ndarray:
        padded = np.pad(mask, 1, mode="constant", constant_values=False)
        slices = [
            padded[1 + row_offset : 1 + row_offset + mask.shape[0], 1 + col_offset : 1 + col_offset + mask.shape[1]]
            for row_offset in (-1, 0, 1)
            for col_offset in (-1, 0, 1)
        ]
        result = slices[0].copy()
        for item in slices[1:]:
            result &= item
        return result

    def _mask_to_loops(self, mask: np.ndarray) -> list[list[tuple[int, int]]]:
        accelerated = native_accel.mask_to_loops(mask)
        if accelerated is not None:
            return accelerated
        edges: dict[tuple[int, int], list[tuple[int, int]]] = {}

        def add_edge(start: tuple[int, int], end: tuple[int, int]) -> None:
            edges.setdefault(start, []).append(end)

        height, width = mask.shape
        for row in range(height):
            for col in np.flatnonzero(mask[row]):
                col = int(col)
                if row == 0 or not mask[row - 1, col]:
                    add_edge((col, row), (col + 1, row))
                if col == width - 1 or not mask[row, col + 1]:
                    add_edge((col + 1, row), (col + 1, row + 1))
                if row == height - 1 or not mask[row + 1, col]:
                    add_edge((col + 1, row + 1), (col, row + 1))
                if col == 0 or not mask[row, col - 1]:
                    add_edge((col, row + 1), (col, row))

        loops: list[list[tuple[int, int]]] = []
        used: set[tuple[tuple[int, int], tuple[int, int]]] = set()
        for start, end_points in edges.items():
            for end in end_points:
                segment = (start, end)
                if segment in used:
                    continue
                used.add(segment)
                loop = [start, end]
                current = end
                safety = 0
                while current != start:
                    next_points = edges.get(current, [])
                    next_point = next(
                        (candidate for candidate in next_points if (current, candidate) not in used),
                        None,
                    )
                    if next_point is None:
                        loop = []
                        break
                    used.add((current, next_point))
                    loop.append(next_point)
                    current = next_point
                    safety += 1
                    if safety > len(edges) + 5:
                        loop = []
                        break
                if len(loop) >= 4 and loop[0] == loop[-1]:
                    loops.append(loop[:-1])
        return loops

    def _centerline_from_mask(self, mask: np.ndarray) -> list[tuple[float, float]]:
        rows, cols = np.nonzero(mask)
        if rows.size < 2:
            return []

        coordinates = np.column_stack((cols.astype(float), rows.astype(float)))
        centroid = coordinates.mean(axis=0)
        centered = coordinates - centroid
        if centered.shape[0] < 2:
            return []

        _u, _s, vh = np.linalg.svd(centered, full_matrices=False)
        direction = vh[0]
        norm = float(np.linalg.norm(direction))
        if norm <= 1e-9:
            return self._fallback_pixel_centerline(mask)
        direction = direction / norm
        projections = centered @ direction
        span = float(projections.max() - projections.min())
        if span < 1.0:
            return self._fallback_pixel_centerline(mask, direction=direction, centroid=centroid, projections=projections)

        perpendicular = np.array([-direction[1], direction[0]])
        return self._straight_centerline_endpoints(centered, centroid, direction, perpendicular, projections)

    def _straight_centerline_endpoints(
        self,
        centered: np.ndarray,
        centroid: np.ndarray,
        direction: np.ndarray,
        perpendicular: np.ndarray,
        projections: np.ndarray,
    ) -> list[tuple[float, float]]:
        min_projection = float(projections.min())
        max_projection = float(projections.max())
        span = max_projection - min_projection
        if span < 1e-6:
            return []

        endpoint_window = max(1.5, span * 0.1)
        start_selection = projections <= (min_projection + endpoint_window)
        end_selection = projections >= (max_projection - endpoint_window)
        if not np.any(start_selection) or not np.any(end_selection):
            return []

        start_axis = float(projections[start_selection].mean())
        end_axis = float(projections[end_selection].mean())
        start_offset = float((centered[start_selection] @ perpendicular).mean())
        end_offset = float((centered[end_selection] @ perpendicular).mean())

        start_point = centroid + direction * start_axis + perpendicular * start_offset
        end_point = centroid + direction * end_axis + perpendicular * end_offset
        return [
            (float(start_point[0]), float(start_point[1])),
            (float(end_point[0]), float(end_point[1])),
        ]

    def _fallback_pixel_centerline(
        self,
        mask: np.ndarray,
        direction: np.ndarray | None = None,
        centroid: np.ndarray | None = None,
        projections: np.ndarray | None = None,
    ) -> list[tuple[float, float]]:
        rows, cols = np.nonzero(mask)
        if rows.size < 2:
            return []

        coordinates = np.column_stack((cols.astype(float), rows.astype(float)))
        if centroid is None:
            centroid = coordinates.mean(axis=0)
        if direction is None or projections is None:
            centered = coordinates - centroid
            if centered.shape[0] < 2:
                return []
            _u, _s, vh = np.linalg.svd(centered, full_matrices=False)
            direction = vh[0]
            norm = float(np.linalg.norm(direction))
            if norm <= 1e-9:
                return []
            direction = direction / norm
            projections = centered @ direction

        span = float(projections.max() - projections.min())
        if span < 1e-6:
            min_col = float(cols.min())
            max_col = float(cols.max())
            min_row = float(rows.min())
            max_row = float(rows.max())
            if (max_col - min_col) >= (max_row - min_row):
                return [(min_col, float(centroid[1])), (max_col, float(centroid[1]))]
            return [(float(centroid[0]), min_row), (float(centroid[0]), max_row)]

        start = centroid + direction * float(projections.min())
        end = centroid + direction * float(projections.max())
        return [(float(start[0]), float(start[1])), (float(end[0]), float(end[1]))]

    def _simplify_loop(self, points: list[tuple[int, int]]) -> list[tuple[int, int]]:
        if len(points) < 4:
            return points
        simplified: list[tuple[int, int]] = []
        for point in points:
            simplified.append(point)
            while len(simplified) >= 3 and self._collinear(simplified[-3], simplified[-2], simplified[-1]):
                simplified.pop(-2)
        changed = True
        while changed and len(simplified) >= 3:
            changed = False
            if self._collinear(simplified[-2], simplified[-1], simplified[0]):
                simplified.pop(-1)
                changed = True
            if len(simplified) >= 3 and self._collinear(simplified[-1], simplified[0], simplified[1]):
                simplified.pop(0)
                changed = True
        return simplified

    def _collinear(self, first: tuple[int, int], second: tuple[int, int], third: tuple[int, int]) -> bool:
        return (first[0] == second[0] == third[0]) or (first[1] == second[1] == third[1])

    def _polygon_area(self, points: Iterable[tuple[int, int]]) -> float:
        ordered = list(points)
        area = 0.0
        for index, point in enumerate(ordered):
            next_point = ordered[(index + 1) % len(ordered)]
            area += (point[0] * next_point[1]) - (next_point[0] * point[1])
        return area / 2.0

    def _bounds_from_points(self, points: list[tuple[float, float]]) -> Bounds:
        xs = [point[0] for point in points]
        ys = [point[1] for point in points]
        return Bounds(min(xs), min(ys), max(xs), max(ys))

    def _rectangle_polygon(self, bounds: Bounds) -> list[tuple[float, float]]:
        return [
            (bounds.min_x, bounds.min_y),
            (bounds.max_x, bounds.min_y),
            (bounds.max_x, bounds.max_y),
            (bounds.min_x, bounds.max_y),
        ]

    def _fallback_world_centerline(self, bounds: Bounds) -> list[tuple[float, float]]:
        if bounds.width >= bounds.height:
            return [(bounds.min_x, bounds.center_y), (bounds.max_x, bounds.center_y)]
        return [(bounds.center_x, bounds.min_y), (bounds.center_x, bounds.max_y)]

    def _add_proefsleuf_polygon(self, modelspace, polygon: list[tuple[float, float]], fill: bool = True) -> None:
        if fill:
            hatch = modelspace.add_hatch(
                color=7,
                dxfattribs={
                    "layer": "PROEFSLEUVEN_VLAK",
                    "true_color": rgb2int((255, 255, 255)),
                },
            )
            hatch.paths.add_polyline_path(polygon, is_closed=True)
        modelspace.add_lwpolyline(
            polygon + [polygon[0]],
            dxfattribs={
                "layer": "PROEFSLEUVEN_OMTREK",
                "color": 7,
                "true_color": rgb2int((120, 120, 120)),
                "lineweight": 25,
            },
        )

    def _prepare_tiff_raster_files(self, output_path: Path, tiff_layers: list[GeoTiffLayer]) -> list[PreparedTiffRaster]:
        asset_dir = output_path.parent / f"{output_path.stem}_tiffs"
        asset_dir.mkdir(parents=True, exist_ok=True)

        def copy_single(index_layer: tuple[int, GeoTiffLayer]) -> tuple[int, PreparedTiffRaster]:
            index, layer = index_layer
            export_layer = self._prepared_virtual_trench_export_layer(layer)
            target_path = self._unique_raster_copy_path(asset_dir, self._proefsleuf_raster_name(layer, index))
            if is_virtual_trench_layer(layer) or not Path(layer.path).exists():
                try:
                    export_layer.image.save(target_path, format="TIFF")
                except OSError as exc:
                    raise CadastralExportError(
                        f"Virtuele proefsleuf kon niet als raster worden opgeslagen voor DXF-export: {layer.path.name}: {exc}"
                    ) from exc
            else:
                try:
                    shutil.copy2(layer.path, target_path)
                except OSError as exc:
                    raise CadastralExportError(
                        f"TIFF kon niet worden gekopieerd voor DXF-export: {layer.path.name}: {exc}"
                    ) from exc
            return index, PreparedTiffRaster(layer=export_layer, raster_path=target_path.resolve())

        raster_paths: list[PreparedTiffRaster | None] = [None] * len(tiff_layers)
        max_copy_workers = max(1, min(4, len(tiff_layers)))
        with ThreadPoolExecutor(max_workers=max_copy_workers, thread_name_prefix="tiff-copy") as executor:
            future_map = {
                executor.submit(copy_single, (index, layer)): index - 1
                for index, layer in enumerate(tiff_layers, start=1)
            }
            for future in as_completed(future_map):
                result_index, prepared_raster = future.result()
                raster_paths[result_index - 1] = prepared_raster
        return [path for path in raster_paths if path is not None]

    def _unique_raster_copy_path(self, asset_dir: Path, preferred_name: str) -> Path:
        candidate = asset_dir / preferred_name
        if not candidate.exists():
            return candidate
        stem = Path(preferred_name).stem
        suffix = Path(preferred_name).suffix
        index = 2
        while True:
            candidate = asset_dir / f"{stem} ({index}){suffix}"
            if not candidate.exists():
                return candidate
            index += 1

    def _add_tiff_image(
        self,
        document: ezdxf.EzDxfDocument,
        modelspace,
        layer: GeoTiffLayer,
        raster_path: Path,
        index: int,
    ) -> None:
        image_def_name = self._unique_image_def_name(document, self._proefsleuf_raster_name(layer, index))
        image_def = document.add_image_def(
            filename=str(raster_path),
            size_in_pixel=(layer.image.width, layer.image.height),
            name=image_def_name,
        )
        image = modelspace.add_image(
            image_def,
            insert=(0.0, 0.0, 0.0),
            size_in_units=(1.0, 1.0),
            dxfattribs={
                "layer": "PROEFSLEUVEN_TIFF",
                "flags": 11,
            },
        )
        insert_x, insert_y = layer.transform.pixel_to_world(0.0, float(layer.image.height))
        image.dxf.insert = (insert_x, insert_y, 0.0)
        image.dxf.u_pixel = (layer.transform.a, layer.transform.d, 0.0)
        image.dxf.v_pixel = (-layer.transform.b, -layer.transform.e, 0.0)

    def _unique_image_def_name(self, document: ezdxf.EzDxfDocument, preferred_name: str) -> str:
        image_dict = document.rootdict.get_required_dict("ACAD_IMAGE_DICT")
        candidate = preferred_name
        suffix = 2
        while candidate in image_dict:
            candidate = f"{preferred_name} ({suffix})"
            suffix += 1
        return candidate

    def _add_proefsleuf_centerline(
        self,
        modelspace,
        centerline: list[tuple[float, float]],
        centerline_color: tuple[int, int, int],
    ) -> None:
        modelspace.add_lwpolyline(
            centerline,
            dxfattribs={
                "layer": "PROEFSLEUVEN_HARTLIJN",
                "color": 7,
                "true_color": rgb2int(centerline_color),
                "const_width": self.CENTERLINE_WIDTH,
            },
        )

    def _proefsleuf_label(self, layer: GeoTiffLayer, fallback_index: int) -> str:
        return self._proefsleuf_base_name(layer, fallback_index)

    def _template_proefsleuf_label(self, layer: GeoTiffLayer, fallback_index: int) -> str:
        explicit_label = str(layer.metadata.get(self.TEMPLATE_PROEFSLEUF_LABEL_METADATA_KEY, "") or "").strip().upper()
        if re.fullmatch(r"PS\d+[A-Z]*", explicit_label):
            return explicit_label
        return self._proefsleuf_base_name(layer, fallback_index)

    def _proefsleuf_raster_name(self, layer: GeoTiffLayer, fallback_index: int) -> str:
        suffix = layer.path.suffix or ".tiff"
        return f"{self._proefsleuf_base_name(layer, fallback_index)}{suffix}"

    def _proefsleuf_base_name(self, layer: GeoTiffLayer, fallback_index: int) -> str:
        stem = layer.path.stem
        match = re.search(r"(?i)\bps[\s._-]*(\d+)\b", stem)
        if match:
            return f"PS{int(match.group(1))}"
        digits = re.search(r"(\d+)", stem)
        if digits:
            return f"PS{int(digits.group(1))}"
        compact_stem = re.sub(r"\s+", " ", stem).strip().upper()
        if compact_stem:
            return compact_stem[:32]
        return f"PS{fallback_index}"

    def _add_proefsleuf_label(
        self,
        modelspace,
        label: str,
        insert: tuple[float, float],
        label_color: tuple[int, int, int],
    ) -> None:
        entity = modelspace.add_text(
            label,
            dxfattribs={
                "layer": "PROEFSLEUVEN_LABEL",
                "style": self.LABEL_STYLE,
                "height": self.LABEL_HEIGHT,
                "color": 7,
                "true_color": rgb2int(label_color),
            },
        )
        entity.set_placement(insert, align=TextEntityAlignment.MIDDLE_CENTER)

    def _add_cadastral_text_label(self, modelspace, label: CadastralTextLabel) -> None:
        height = self.STREET_LABEL_HEIGHT if label.layer_name == "KAD_STRAATNAAM" else self.HOUSE_NUMBER_HEIGHT
        entity = modelspace.add_text(
            label.text,
            dxfattribs={
                "layer": label.layer_name,
                "style": self.CADASTRAL_LABEL_STYLE,
                "height": height,
                "rotation": (-label.rotation) % 360.0,
                "color": 8,
            },
        )
        entity.set_placement(label.position, align=TextEntityAlignment.MIDDLE_CENTER)

    def _pick_label_position(
        self,
        own_bounds: Bounds,
        label: str,
        tiff_bounds: list[Bounds],
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
                overlaps_tiffs = sum(
                    1 for other in tiff_bounds if other is not own_bounds and label_bounds.intersects(other)
                )
                distance = self._distance_to_bounds(center, own_bounds)
                score = (float(overlaps_labels), float(overlaps_tiffs), distance, float(priority))
                if best_score is None or score < best_score:
                    best_score = score
                    best_center = center
                if overlaps_labels == 0 and overlaps_tiffs == 0:
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
        return hypot(point[0] - x, point[1] - y)
