from __future__ import annotations

from dataclasses import replace
from functools import wraps
import inspect
from typing import Any, Callable
from tkinter import messagebox

from .virtual_trench import (
    build_virtual_trench_dataset,
    is_virtual_trench_layer,
    virtual_trench_centerline,
    virtual_trench_endpoints,
)


PATCH_VERSION = 2
VIRTUAL_TEMPLATE_DATASET_ID_KEY = "_sleufbase_virtual_template_dataset_id"
SYNTHETIC_DATASET_ID_START = -1_500_000_000
MISSING_PROFILE_WARNING_TITLE = "Dwarsprofielen deels overgeslagen"
MISSING_KICKTHEMAP_JOB_TEXT = "geen KickTheMap job gekoppeld voor dwarsprofiel"


def _filter_virtual_missing_job_warning(message: object) -> str:
    """Remove obsolete MarXact missing-job warnings.

    MarXact layers intentionally have no KickTheMap job. Their cross-section
    dataset is reconstructed from the virtual-trench payload by this patch, so
    warning that those layers are skipped is no longer true. Keep every other
    warning line intact.
    """

    text = str(message or "")
    retained: list[str] = []
    missing_text = MISSING_KICKTHEMAP_JOB_TEXT.casefold()
    for line in text.splitlines():
        normalized = line.casefold()
        if "marxact-virtual.tif" in normalized and missing_text in normalized:
            continue
        retained.append(line)
    return "\n".join(retained).strip()


def _virtual_axis_pixel_vector(layer: Any) -> tuple[float, float] | None:
    """Return the exact virtual-trench start->end vector in raster pixels."""

    if layer is None or not is_virtual_trench_layer(layer):
        return None
    centerline = virtual_trench_centerline(layer)
    if len(centerline) < 2:
        return None
    start = centerline[0]
    end = centerline[-1]
    try:
        start_px = layer.transform.world_to_pixel(float(start[0]), float(start[1]))
        end_px = layer.transform.world_to_pixel(float(end[0]), float(end[1]))
        dx = float(end_px[0]) - float(start_px[0])
        dy = float(end_px[1]) - float(start_px[1])
    except (AttributeError, IndexError, TypeError, ValueError):
        return None
    if (dx * dx + dy * dy) <= 1e-6:
        return None
    return dx, dy


def _point_xy(point: dict[str, Any] | None) -> tuple[float, float] | None:
    if point is None:
        return None
    try:
        return float(point.get("x")), float(point.get("y"))
    except (AttributeError, TypeError, ValueError):
        return None


def _augment_virtual_template_datasets(
    exporter: Any,
    tiff_layers: list[Any],
    cross_section_datasets: dict[int, Any] | None,
    original_job_id: Callable[[Any, Any], int | None],
    *,
    reverse_cross_sections: bool = False,
) -> tuple[dict[int, Any], list[tuple[Any, bool, object]]]:
    """Add datasets for virtual trenches that do not have a usable KTK dataset.

    The core exporter indexes cross-section datasets by KickTheMap job id. A
    virtual trench does not need or have such a job. During this export only we
    give it a private negative id and let the existing, well-tested profile
    builder consume `build_virtual_trench_dataset()` unchanged.

    Virtual datasets always carry a forced start point. The core profile builder
    deliberately refuses its generic reverse operation when a forced start is
    present. For a reverse export we therefore force the *measured end point*
    instead. That makes the same profile builder walk the exact MarXact axis in
    the opposite direction, including the correct endpoint Z values.
    """

    datasets = dict(cross_section_datasets or {})
    restore_entries: list[tuple[Any, bool, object]] = []
    seen_layers: set[int] = set()
    next_dataset_id = SYNTHETIC_DATASET_ID_START

    for layer in tiff_layers:
        if layer is None or not is_virtual_trench_layer(layer):
            continue
        identity = id(layer)
        if identity in seen_layers:
            continue
        seen_layers.add(identity)

        try:
            current_job_id = original_job_id(exporter, layer)
        except Exception:
            current_job_id = None
        if current_job_id is not None and datasets.get(current_job_id) is not None:
            # Preserve the existing path for a virtual trench that really is
            # backed by a downloaded KickTheMap dataset.
            continue

        dataset = build_virtual_trench_dataset(layer, include_endpoints=True)
        if dataset is None:
            continue

        if reverse_cross_sections:
            _start_point, end_point = virtual_trench_endpoints(layer)
            reverse_start_xy = _point_xy(end_point)
            if reverse_start_xy is not None:
                dataset = replace(dataset, cross_section_start_xy=reverse_start_xy)

        while next_dataset_id in datasets:
            next_dataset_id -= 1
        synthetic_id = next_dataset_id
        next_dataset_id -= 1

        metadata = getattr(layer, "metadata", None)
        if not isinstance(metadata, dict):
            continue
        had_marker = VIRTUAL_TEMPLATE_DATASET_ID_KEY in metadata
        old_marker = metadata.get(VIRTUAL_TEMPLATE_DATASET_ID_KEY)
        restore_entries.append((layer, had_marker, old_marker))
        metadata[VIRTUAL_TEMPLATE_DATASET_ID_KEY] = synthetic_id
        datasets[synthetic_id] = replace(dataset, job_id=synthetic_id)

    return datasets, restore_entries


def _restore_virtual_template_dataset_ids(entries: list[tuple[Any, bool, object]]) -> None:
    for layer, had_marker, old_marker in entries:
        metadata = getattr(layer, "metadata", None)
        if not isinstance(metadata, dict):
            continue
        if had_marker:
            metadata[VIRTUAL_TEMPLATE_DATASET_ID_KEY] = old_marker
        else:
            metadata.pop(VIRTUAL_TEMPLATE_DATASET_ID_KEY, None)


def install_virtual_trench_template_patch() -> None:
    from .cadastral_export import CadastralDxfExporter

    if int(
        getattr(CadastralDxfExporter, "_sleufbase_virtual_trench_template_patch_version", 0)
        or 0
    ) >= PATCH_VERSION:
        return

    original_export_template_sheet = CadastralDxfExporter.export_template_sheet
    original_job_id = CadastralDxfExporter._kickthemap_job_id
    original_orientation = CadastralDxfExporter._template_tiff_orientation_pixel_vector
    export_signature = inspect.signature(original_export_template_sheet)

    @wraps(original_job_id)
    def _kickthemap_job_id_with_virtual_dataset(self, layer):
        if is_virtual_trench_layer(layer):
            try:
                marker = layer.metadata.get(VIRTUAL_TEMPLATE_DATASET_ID_KEY)
                if marker is not None:
                    return int(marker)
            except (AttributeError, TypeError, ValueError):
                pass
        return original_job_id(self, layer)

    @wraps(original_orientation)
    def _template_tiff_orientation_with_virtual_axis(
        self,
        layer,
        road_orientation_paths,
        terrain_boundary_paths,
        profile=None,
    ):
        # If a profile exists, the core exporter already uses its exact axis.
        # Without a profile, virtual trenches must still use their own start/end
        # axis instead of the road/terrain fallback, which could be a few degrees
        # off and caused the MarXact overview image to look inconsistent.
        if profile is None:
            vector = _virtual_axis_pixel_vector(layer)
            if vector is not None:
                return vector
        return original_orientation(
            self,
            layer,
            road_orientation_paths,
            terrain_boundary_paths,
            profile=profile,
        )

    @wraps(original_export_template_sheet)
    def _export_template_sheet_with_virtual_datasets(self, *args, **kwargs):
        bound = export_signature.bind(self, *args, **kwargs)
        bound.apply_defaults()
        tiff_layers = list(bound.arguments.get("tiff_layers") or [])
        datasets, restore_entries = _augment_virtual_template_datasets(
            self,
            tiff_layers,
            bound.arguments.get("cross_section_datasets"),
            original_job_id,
            reverse_cross_sections=bool(bound.arguments.get("reverse_cross_sections", False)),
        )
        bound.arguments["cross_section_datasets"] = datasets
        try:
            return original_export_template_sheet(*bound.args, **bound.kwargs)
        finally:
            _restore_virtual_template_dataset_ids(restore_entries)

    original_showwarning = messagebox.showwarning

    @wraps(original_showwarning)
    def _showwarning_without_obsolete_marxact_missing_job(title, message, *args, **kwargs):
        if str(title or "").strip() == MISSING_PROFILE_WARNING_TITLE:
            filtered = _filter_virtual_missing_job_warning(message)
            if not filtered:
                return "ok"
            message = filtered
        return original_showwarning(title, message, *args, **kwargs)

    CadastralDxfExporter._kickthemap_job_id = _kickthemap_job_id_with_virtual_dataset
    CadastralDxfExporter._template_tiff_orientation_pixel_vector = (
        _template_tiff_orientation_with_virtual_axis
    )
    CadastralDxfExporter.export_template_sheet = _export_template_sheet_with_virtual_datasets
    CadastralDxfExporter._sleufbase_virtual_trench_template_patch_version = PATCH_VERSION
    CadastralDxfExporter.SLEUFBASE_VIRTUAL_TEMPLATE_DATASET_ID_KEY = (
        VIRTUAL_TEMPLATE_DATASET_ID_KEY
    )
    messagebox.showwarning = _showwarning_without_obsolete_marxact_missing_job
