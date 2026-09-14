from __future__ import annotations

from typing import Any

from . import dxf_template_pipeline_v8_patch as pipeline_v8


PATCH_VERSION = 1


def _slot_asset_label(label: object, index: object) -> str:
    """Return a deterministic per-slot raster label.

    Template assets used to rely on ``_unique_raster_copy_path`` to add suffixes
    such as ``(2)`` when two slots had the same visible proefsleuf label. During
    parallel template export, two workers can choose the same still-unused suffix
    before either worker has written its PNG. Both workers then save to the same
    file, which can leave a partially overwritten/black map image.

    The slot index is already unique within one template export, so include it in
    every map/TIFF raster label before any parallel work starts. This removes the
    race instead of relying on filesystem timing.
    """

    base = str(label or "PS").strip() or "PS"
    try:
        slot_index = int(index)
    except (TypeError, ValueError):
        slot_index = 0
    return f"{base}__slot{slot_index:03d}"


def install_dxf_template_pipeline_v9_patch() -> None:
    """Give every template map/TIFF raster a unique slot-based filename.

    Duplicate labels such as two different ``PS2`` trenches are valid. V8 and
    older code generated preferred names from only the label, for example
    ``PS2_kaart.png``. With parallel map preparation that could race in
    ``_unique_raster_copy_path`` and make two slots write to the same PNG.

    V9 keeps the visible proefsleuf label unchanged, but internally passes a
    deterministic label containing the slot index to the raster builders. Normal
    and reverse passes use the same slot-derived label, so all existing reuse and
    reverse-asset logic remains intact.
    """

    from .cadastral_export import CadastralDxfExporter

    pipeline_v8.install_dxf_template_pipeline_v8_patch()
    if int(
        getattr(CadastralDxfExporter, "_sleufbase_dxf_template_pipeline_v9_version", 0)
        or 0
    ) >= PATCH_VERSION:
        return

    previous_tiff = CadastralDxfExporter._build_template_tiff_raster
    previous_map = CadastralDxfExporter._build_template_map_raster

    def _build_template_tiff_raster_v9(
        self,
        asset_dir,
        layer,
        label,
        index,
        road_orientation_paths,
        terrain_boundary_paths,
        profile=None,
        reverse_orientation=False,
    ):
        return previous_tiff(
            self,
            asset_dir,
            layer,
            _slot_asset_label(label, index),
            index,
            road_orientation_paths,
            terrain_boundary_paths,
            profile=profile,
            reverse_orientation=reverse_orientation,
        )

    def _build_template_map_raster_v9(
        self,
        asset_dir,
        layer,
        label,
        index,
        trench_mode,
        centerline_color,
        label_color,
        page_exporter,
        dxf_overlays,
        map_comments,
        background_provider,
        background_attribution,
        reference_annotation=None,
    ):
        return previous_map(
            self,
            asset_dir,
            layer,
            _slot_asset_label(label, index),
            index,
            trench_mode,
            centerline_color,
            label_color,
            page_exporter,
            dxf_overlays,
            map_comments,
            background_provider,
            background_attribution,
            reference_annotation=reference_annotation,
        )

    CadastralDxfExporter._build_template_tiff_raster = _build_template_tiff_raster_v9
    CadastralDxfExporter._build_template_map_raster = _build_template_map_raster_v9
    CadastralDxfExporter._sleufbase_dxf_template_pipeline_v9_version = PATCH_VERSION
    CadastralDxfExporter.SLEUFBASE_TEMPLATE_RASTER_SLOT_NAMES = True
