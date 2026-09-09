from __future__ import annotations

import os
import shutil
import threading
import time
from pathlib import Path
from typing import Any

from PIL import Image

from . import dxf_template_pipeline_patch as pipeline
from . import dxf_template_pipeline_v3_patch as pipeline_v3
from . import exporting as exporting_module
from . import template_reverse_patch as reverse_patch
from .models import ProfileReferenceAnnotation


PATCH_VERSION = 1
_SESSION_ATTR = "_sleufbase_reverse_asset_reuse_session"
_MODE_ATTR = "_sleufbase_reverse_asset_reuse_mode"


def _session(exporter: Any) -> dict[str, Any] | None:
    value = getattr(exporter, _SESSION_ATTR, None)
    return value if isinstance(value, dict) else None


def _mode(exporter: Any) -> str:
    return str(getattr(exporter, _MODE_ATTR, "") or "").upper()


def _asset_key(layer: Any, label: object, index: object) -> tuple[int, str, int]:
    return id(layer), str(label), int(index)


def _annotation_key(annotation: object) -> tuple[float, float] | None:
    if annotation is None:
        return None
    try:
        return round(float(annotation.x), 6), round(float(annotation.y), 6)
    except Exception:
        return None


def _reverse_reference_annotation(
    exporter: Any,
    layer: Any,
    profile: Any,
    current_annotation: object,
) -> ProfileReferenceAnnotation | None:
    """Derive the reverse map dot without rebuilding the reverse profile map.

    A configured explicit reference point is orientation-independent. Otherwise
    the normal profile start becomes the reverse profile end and vice versa, so
    the reverse reference point is the normal profile end point.
    """

    if current_annotation is None:
        return None
    try:
        metadata_point = exporter._template_reference_metadata_point(layer)
    except Exception:
        metadata_point = None
    if metadata_point is not None:
        try:
            return ProfileReferenceAnnotation(
                x=float(current_annotation.x),
                y=float(current_annotation.y),
            )
        except Exception:
            return None
    if profile is None:
        return None
    try:
        x_coord = float(profile.end_point.x)
        y_coord = float(profile.end_point.y)
    except Exception:
        return None
    try:
        rotation_degrees = float(layer.metadata.get("rotation_degrees", 0.0) or 0.0)
    except Exception:
        rotation_degrees = 0.0
    if abs(rotation_degrees) >= 1e-9:
        try:
            center_x, center_y = layer.transform.pixel_to_world(
                layer.image.width / 2.0,
                layer.image.height / 2.0,
            )
            x_coord, y_coord = exporter._rotate_template_map_point(
                x_coord,
                y_coord,
                center_x,
                center_y,
                rotation_degrees,
            )
        except Exception:
            return None
    return ProfileReferenceAnnotation(x=x_coord, y=y_coord)


def _copy_or_rotate_reverse_tiff(
    exporter: Any,
    source_path: Path,
    asset_dir: Path,
    label: str,
    *,
    rotate_180: bool,
) -> Path:
    source = Path(source_path)
    if not rotate_180:
        return source
    destination = exporter._unique_raster_copy_path(asset_dir, f"{label}_geotiff.png")
    with Image.open(source) as image:
        rotated = image.transpose(Image.Transpose.ROTATE_180)
        try:
            rotated.save(destination, format="PNG")
        finally:
            if rotated is not image:
                rotated.close()
    return Path(destination).resolve()


def _render_normal_and_reverse_map_pair(
    exporter: Any,
    *,
    asset_dir: Path,
    layer: Any,
    label: str,
    index: int,
    page_exporter: Any,
    dxf_overlays: list[Any],
    background_provider: Any,
    background_attribution: str | None,
    reference_annotation: ProfileReferenceAnnotation,
    profile: Any,
) -> tuple[Path, Path | None, ProfileReferenceAnnotation | None]:
    """Render the expensive map once and compose normal/reverse pages from it."""

    padded_bounds = layer.bounds.padded(
        max(1.0, min(4.0, max(layer.bounds.width, layer.bounds.height) * 0.1))
    )
    map_bounds = page_exporter._determine_map_bounds(padded_bounds)
    provider = background_provider or page_exporter.default_background_provider
    background = provider.fetch_map(
        map_bounds,
        (page_exporter.map_width_px, page_exporter.map_height_px),
    )
    export_tiff = exporting_module._prepare_export_tiff_layer(layer, forced_opacity=1.0)
    map_image = page_exporter.renderer.render(
        map_bounds,
        (page_exporter.map_width_px, page_exporter.map_height_px),
        [export_tiff],
        dxf_overlays,
        background=background,
        map_comments=None,
    )
    reverse_annotation = _reverse_reference_annotation(
        exporter,
        layer,
        profile,
        reference_annotation,
    )
    normal_page = page_exporter._compose_page(
        map_image,
        map_bounds,
        background_attribution=background_attribution,
        reference_annotation=reference_annotation,
    )
    reverse_page = None
    if reverse_annotation is not None:
        reverse_page = page_exporter._compose_page(
            map_image,
            map_bounds,
            background_attribution=background_attribution,
            reference_annotation=reverse_annotation,
        )

    normal_path = exporter._unique_raster_copy_path(asset_dir, f"{label}_kaart.png")
    normal_page.save(normal_path, format="PNG")
    reverse_path: Path | None = None
    if reverse_page is not None:
        reverse_path = Path(asset_dir) / f".sleufbase-reverse-map-{int(index)}-{id(layer)}.png"
        reverse_page.save(reverse_path, format="PNG")
        reverse_path = reverse_path.resolve()

    try:
        normal_page.close()
    except Exception:
        pass
    if reverse_page is not None:
        try:
            reverse_page.close()
        except Exception:
            pass
    try:
        map_image.close()
    except Exception:
        pass
    try:
        export_tiff.image.close()
    except Exception:
        pass
    return Path(normal_path).resolve(), reverse_path, reverse_annotation


def _cleanup_session(exporter: Any) -> None:
    session = _session(exporter)
    if session is None:
        return
    for value in list(session.get("temporary_paths", ())):
        try:
            Path(value).unlink(missing_ok=True)
        except OSError:
            pass
    try:
        delattr(exporter, _SESSION_ATTR)
    except Exception:
        pass


def install_dxf_template_pipeline_v4_patch() -> None:
    """Reuse normal raster work for reverse DXF variants.

    The normal and reverse template exports share all expensive map content. The
    reverse TIFF is exactly the normal raster turned 180 degrees whenever an
    orientation vector exists. The map page shares the same rendered map and only
    changes the profile reference dot. This patch therefore keeps the proven
    second DXF/profile pass, but removes the duplicated high-resolution virtual
    trench render, background fetch and map renderer pass.
    """

    from .cadastral_export import CadastralDxfExporter

    pipeline_v3.install_dxf_template_pipeline_v3_patch()
    if int(
        getattr(CadastralDxfExporter, "_sleufbase_dxf_template_pipeline_v4_version", 0)
        or 0
    ) >= PATCH_VERSION:
        return

    previous_call = reverse_patch._call_original_export
    previous_cleanup = reverse_patch._cleanup_reverse_source
    previous_prefetch = CadastralDxfExporter._prefetch_template_background_maps
    previous_address = CadastralDxfExporter._reverse_geocoded_template_address
    previous_tiff = CadastralDxfExporter._build_template_tiff_raster
    previous_map = CadastralDxfExporter._build_template_map_raster

    def _call_original_export_v4(
        original_export,
        instance,
        call_arguments,
        *,
        mode,
        output_path,
        reverse_cross_sections,
    ):
        normalized_mode = str(mode).upper()
        previous_mode = getattr(instance, _MODE_ATTR, None)
        if normalized_mode == reverse_patch.NORMAL_MODE:
            _cleanup_session(instance)
            setattr(
                instance,
                _SESSION_ATTR,
                {
                    "lock": threading.RLock(),
                    "addresses": {},
                    "normal_tiffs": {},
                    "normal_maps": {},
                    "reverse_maps": {},
                    "map_ready_layers": set(),
                    "temporary_paths": set(),
                },
            )
        setattr(instance, _MODE_ATTR, normalized_mode)
        try:
            return previous_call(
                original_export,
                instance,
                call_arguments,
                mode=mode,
                output_path=output_path,
                reverse_cross_sections=reverse_cross_sections,
            )
        except Exception:
            if normalized_mode == reverse_patch.NORMAL_MODE:
                _cleanup_session(instance)
            raise
        finally:
            if previous_mode is None:
                try:
                    delattr(instance, _MODE_ATTR)
                except Exception:
                    pass
            else:
                setattr(instance, _MODE_ATTR, previous_mode)

    def _cleanup_reverse_source_v4(reverse_source_path: Path) -> None:
        session_exporter = None
        pair_cache = pipeline._pair_cache()
        if isinstance(pair_cache, dict):
            session_exporter = pair_cache.get("exporter")
        try:
            previous_cleanup(reverse_source_path)
        finally:
            if session_exporter is not None:
                _cleanup_session(session_exporter)

    def _prefetch_template_background_maps_v4(
        self,
        page_exporter,
        background_provider,
        layers,
        *,
        status_callback=None,
    ):
        session = _session(self)
        if session is not None and _mode(self) == reverse_patch.REVERSE_MODE:
            layer_ids = {id(layer) for layer in layers}
            with session["lock"]:
                ready = set(session.get("map_ready_layers") or ())
            if layer_ids and layer_ids.issubset(ready):
                pipeline._increment_pair_counter("reverse_map_prefetch_skipped")
                return {"maps": 0, "clusters": 0, "reused": len(layer_ids)}
        return previous_prefetch(
            self,
            page_exporter,
            background_provider,
            layers,
            status_callback=status_callback,
        )

    def _reverse_geocoded_template_address_v4(self, layer, location_client):
        session = _session(self)
        mode = _mode(self)
        if session is None or mode not in {reverse_patch.NORMAL_MODE, reverse_patch.REVERSE_MODE}:
            return previous_address(self, layer, location_client)
        key = id(layer)
        with session["lock"]:
            cached = session["addresses"].get(key)
        if mode == reverse_patch.REVERSE_MODE and cached is not None:
            pipeline._increment_pair_counter("reverse_geocode_reused")
            return cached
        result = previous_address(self, layer, location_client)
        if mode == reverse_patch.NORMAL_MODE:
            with session["lock"]:
                session["addresses"][key] = result
        return result

    def _build_template_tiff_raster_v4(
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
        session = _session(self)
        mode = _mode(self)
        if session is None or mode not in {reverse_patch.NORMAL_MODE, reverse_patch.REVERSE_MODE}:
            return previous_tiff(
                self,
                asset_dir,
                layer,
                label,
                index,
                road_orientation_paths,
                terrain_boundary_paths,
                profile=profile,
                reverse_orientation=reverse_orientation,
            )
        key = _asset_key(layer, label, index)
        if mode == reverse_patch.NORMAL_MODE:
            result = previous_tiff(
                self,
                asset_dir,
                layer,
                label,
                index,
                road_orientation_paths,
                terrain_boundary_paths,
                profile=profile,
                reverse_orientation=reverse_orientation,
            )
            try:
                vector = self._template_tiff_orientation_pixel_vector(
                    layer,
                    road_orientation_paths,
                    terrain_boundary_paths,
                    profile=profile,
                )
                rotate_180 = vector is not None and (
                    (float(vector[0]) * float(vector[0]))
                    + (float(vector[1]) * float(vector[1]))
                ) > 1e-6
            except Exception:
                rotate_180 = True
            with session["lock"]:
                session["normal_tiffs"][key] = (Path(result), bool(rotate_180))
            return result

        with session["lock"]:
            cached = session["normal_tiffs"].get(key)
        if cached is None or not Path(cached[0]).exists():
            return previous_tiff(
                self,
                asset_dir,
                layer,
                label,
                index,
                road_orientation_paths,
                terrain_boundary_paths,
                profile=profile,
                reverse_orientation=reverse_orientation,
            )
        started = time.perf_counter()
        result = _copy_or_rotate_reverse_tiff(
            self,
            Path(cached[0]),
            Path(asset_dir),
            str(label),
            rotate_180=bool(cached[1]),
        )
        pipeline._increment_pair_counter(
            "reverse_tiff_rotated_from_normal" if cached[1] else "reverse_tiff_reused_unchanged"
        )
        pipeline._add_pair_phase("reverse_raster_transform", time.perf_counter() - started)
        return result

    def _build_template_map_raster_v4(
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
        session = _session(self)
        mode = _mode(self)
        if session is None or mode not in {reverse_patch.NORMAL_MODE, reverse_patch.REVERSE_MODE}:
            return previous_map(
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
                reference_annotation=reference_annotation,
            )
        key = _asset_key(layer, label, index)
        if mode == reverse_patch.REVERSE_MODE:
            with session["lock"]:
                reverse_entry = session["reverse_maps"].get(key)
                normal_path = session["normal_maps"].get(key)
            if reverse_entry is not None:
                reverse_path, expected_annotation = reverse_entry
                if Path(reverse_path).exists() and _annotation_key(reference_annotation) == expected_annotation:
                    pipeline._increment_pair_counter("reverse_map_reused_with_moved_dot")
                    return Path(reverse_path)
            if normal_path is not None and reference_annotation is None and Path(normal_path).exists():
                pipeline._increment_pair_counter("reverse_map_reused_unchanged")
                return Path(normal_path)
            return previous_map(
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
                reference_annotation=reference_annotation,
            )

        # No direction marker means normal and reverse map pages are identical.
        if reference_annotation is None or page_exporter is None:
            result = previous_map(
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
                reference_annotation=reference_annotation,
            )
            with session["lock"]:
                session["normal_maps"][key] = Path(result)
                session["map_ready_layers"].add(id(layer))
            return result

        started = time.perf_counter()
        # Normal task receives the normal profile through the annotation helper's
        # caller; recover it from the task-local exporter context by rebuilding
        # only the cheap profile endpoint relation from the annotation source.
        profile = None
        try:
            # _build_template_map_raster itself does not receive profile. The
            # normal annotation is the profile start; derive reverse from the
            # virtual-trench/dataset reference only when the layer exposes an end
            # point through the shared payload. Generic layers safely fall back.
            from .virtual_trench import virtual_trench_endpoints, is_virtual_trench_layer

            if is_virtual_trench_layer(layer):
                start_row, end_row = virtual_trench_endpoints(layer)
                if start_row is not None and end_row is not None:
                    class _Point:
                        pass

                    class _Profile:
                        pass

                    point = _Point()
                    point.x = float(end_row.get("x"))
                    point.y = float(end_row.get("y"))
                    profile = _Profile()
                    profile.end_point = point
        except Exception:
            profile = None

        if profile is None:
            # Non-virtual maps are already cached by v1 when their reference
            # annotation is identical. Keep that mature path instead of guessing
            # a reverse endpoint.
            result = previous_map(
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
                reference_annotation=reference_annotation,
            )
            with session["lock"]:
                session["normal_maps"][key] = Path(result)
            return result

        normal_path, reverse_path, reverse_annotation = _render_normal_and_reverse_map_pair(
            self,
            asset_dir=Path(asset_dir),
            layer=layer,
            label=str(label),
            index=int(index),
            page_exporter=page_exporter,
            dxf_overlays=dxf_overlays,
            background_provider=background_provider,
            background_attribution=background_attribution,
            reference_annotation=reference_annotation,
            profile=profile,
        )
        with session["lock"]:
            session["normal_maps"][key] = normal_path
            session["map_ready_layers"].add(id(layer))
            if reverse_path is not None and reverse_annotation is not None:
                session["reverse_maps"][key] = (
                    reverse_path,
                    _annotation_key(reverse_annotation),
                )
                session["temporary_paths"].add(reverse_path)
        pipeline._increment_pair_counter("normal_map_render_shared_with_reverse")
        pipeline._add_pair_phase("map_raster", time.perf_counter() - started)
        return normal_path

    reverse_patch._call_original_export = _call_original_export_v4
    reverse_patch._cleanup_reverse_source = _cleanup_reverse_source_v4
    CadastralDxfExporter._prefetch_template_background_maps = _prefetch_template_background_maps_v4
    CadastralDxfExporter._reverse_geocoded_template_address = _reverse_geocoded_template_address_v4
    CadastralDxfExporter._build_template_tiff_raster = _build_template_tiff_raster_v4
    CadastralDxfExporter._build_template_map_raster = _build_template_map_raster_v4

    CadastralDxfExporter._sleufbase_dxf_template_pipeline_v4_version = PATCH_VERSION
    CadastralDxfExporter.SLEUFBASE_REVERSE_RASTER_REUSE = True
    CadastralDxfExporter.SLEUFBASE_REVERSE_TIFF_TRANSFORM = "normal-180"
    CadastralDxfExporter.SLEUFBASE_REVERSE_MAP_REUSE = "single-render-moved-reference-dot"
