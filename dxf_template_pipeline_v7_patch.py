from __future__ import annotations

from pathlib import Path
import time
from typing import Any

from . import dxf_template_pipeline_patch as pipeline
from . import dxf_template_pipeline_v3_patch as pipeline_v3
from . import dxf_template_pipeline_v4_patch as pipeline_v4
from . import dxf_template_pipeline_v5_patch as pipeline_v5
from . import dxf_template_pipeline_v6_patch as pipeline_v6
from . import template_reverse_patch as reverse_patch


PATCH_VERSION = 1
_MISSING = object()
_CACHE_MISS = object()
_V6_PROFILE_BUILDER = None


def _captured_normal_document(final_output_path: Path):
    """Take the normal export document from the active pair cache, if present."""

    cache = pipeline._pair_cache()
    if cache is None:
        return None
    captured_path = cache.get("normal_document_path")
    document = cache.get("normal_document")
    if document is None or not pipeline_v3._same_path(captured_path, final_output_path):
        return None
    cache.pop("normal_document", None)
    cache.pop("normal_document_path", None)
    pipeline._increment_pair_counter("normal_dxf_reads_skipped")
    return document


def _install_normal_save_capture() -> None:
    """Keep the first (normal) template Drawing in memory until reverse merge."""

    from ezdxf.document import Drawing

    if int(getattr(Drawing, "_sleufbase_normal_save_capture_version", 0) or 0) >= PATCH_VERSION:
        return
    previous_saveas = Drawing.saveas

    def saveas_with_normal_capture(self, filename, *args, **kwargs):
        capture = bool(getattr(reverse_patch._EXPORT_CONTEXT, "capture_normal_save", False))
        expected_path = getattr(reverse_patch._EXPORT_CONTEXT, "capture_normal_path", None)
        if capture and expected_path is not None and pipeline_v3._same_path(filename, expected_path):
            cache = pipeline._pair_cache()
            if cache is not None:
                try:
                    self.filename = str(filename)
                except Exception:
                    pass
                cache["normal_document"] = self
                cache["normal_document_path"] = Path(filename)
                pipeline._increment_pair_counter("normal_dxf_writes_skipped")
                return None
        return previous_saveas(self, filename, *args, **kwargs)

    Drawing.saveas = saveas_with_normal_capture
    Drawing._sleufbase_normal_save_capture_version = PATCH_VERSION


def _install_normal_call_capture() -> None:
    """Scope normal save capture to the normal half of a pair export only."""

    if int(getattr(reverse_patch, "_sleufbase_normal_call_capture_version", 0) or 0) >= PATCH_VERSION:
        return
    previous_call = reverse_patch._call_original_export

    def _call_original_export_v7(
        original_export,
        instance,
        call_arguments,
        *,
        mode,
        output_path,
        reverse_cross_sections,
    ):
        if str(mode).upper() != reverse_patch.NORMAL_MODE:
            return previous_call(
                original_export,
                instance,
                call_arguments,
                mode=mode,
                output_path=output_path,
                reverse_cross_sections=reverse_cross_sections,
            )

        previous_enabled = getattr(reverse_patch._EXPORT_CONTEXT, "capture_normal_save", _MISSING)
        previous_path = getattr(reverse_patch._EXPORT_CONTEXT, "capture_normal_path", _MISSING)
        reverse_patch._EXPORT_CONTEXT.capture_normal_save = True
        reverse_patch._EXPORT_CONTEXT.capture_normal_path = Path(output_path)
        try:
            return previous_call(
                original_export,
                instance,
                call_arguments,
                mode=mode,
                output_path=output_path,
                reverse_cross_sections=reverse_cross_sections,
            )
        finally:
            if previous_enabled is _MISSING:
                reverse_patch._EXPORT_CONTEXT.__dict__.pop("capture_normal_save", None)
            else:
                reverse_patch._EXPORT_CONTEXT.capture_normal_save = previous_enabled
            if previous_path is _MISSING:
                reverse_patch._EXPORT_CONTEXT.__dict__.pop("capture_normal_path", None)
            else:
                reverse_patch._EXPORT_CONTEXT.capture_normal_path = previous_path

    reverse_patch._call_original_export = _call_original_export_v7
    reverse_patch._sleufbase_normal_call_capture_version = PATCH_VERSION


def _merge_reverse_variant_document_v7(
    final_output_path: Path,
    reverse_source_path: Path,
) -> int:
    """Merge both already-built Drawings and write the large DXF only once here."""

    import ezdxf
    from ezdxf import xref

    started = time.perf_counter()
    target_document = _captured_normal_document(final_output_path)
    if target_document is None:
        target_document = ezdxf.readfile(final_output_path)
        pipeline._increment_pair_counter("normal_dxf_capture_fallback_reads")

    reverse_document = pipeline_v3._captured_reverse_document(reverse_source_path)
    if reverse_document is None:
        reverse_document = ezdxf.readfile(reverse_source_path)

    pipeline._make_reverse_image_definitions_unique_document(reverse_document)
    pipeline_v3._transfer_reverse_image_assets(
        reverse_document,
        reverse_source_path,
        final_output_path,
    )

    reverse_inserts = [
        entity
        for entity in reverse_document.modelspace()
        if entity.dxftype() == "INSERT"
        and reverse_patch._is_variant_layer(entity.dxf.layer, reverse_patch.REVERSE_MODE)
    ]
    if not reverse_inserts:
        raise RuntimeError("De reverse export bevat geen proefsleuf-variantblokken.")
    reverse_handles = {str(entity.dxf.handle) for entity in reverse_inserts}

    def include_reverse_variant(entity) -> bool:
        return str(getattr(entity.dxf, "handle", "")) in reverse_handles

    xref.load_modelspace(
        reverse_document,
        target_document,
        filter_fn=include_reverse_variant,
        conflict_policy=xref.ConflictPolicy.KEEP,
    )
    del reverse_document

    for layer in target_document.layers:
        layer_name = str(layer.dxf.name)
        if reverse_patch._is_variant_layer(layer_name, reverse_patch.NORMAL_MODE):
            layer.on()
        elif reverse_patch._is_variant_layer(layer_name, reverse_patch.REVERSE_MODE):
            layer.off()

    target_document.saveas(final_output_path)
    del target_document
    pipeline._add_pair_phase("reverse_merge", time.perf_counter() - started)
    return len(reverse_inserts)


def _session_cached_rule(exporter: Any, point: Any, layer_rules: Any, resolver):
    session = pipeline_v4._session(exporter)
    if session is None:
        return resolver(exporter, point, layer_rules)
    try:
        rules_key = tuple(layer_rules)
        hash(rules_key)
    except (TypeError, ValueError):
        return resolver(exporter, point, layer_rules)

    lock = session.get("lock")
    if lock is None:
        return resolver(exporter, point, layer_rules)
    key = (id(point), rules_key)
    with lock:
        cache = session.setdefault("profile_rule_cache_v7", {})
        cached = cache.get(key, _CACHE_MISS)
    if cached is not _CACHE_MISS:
        pipeline._increment_pair_counter("profile_rule_cache_hits_v7")
        return cached
    result = resolver(exporter, point, layer_rules)
    with lock:
        cache[key] = result
    return result


def _session_cached_description(
    exporter: Any,
    point: Any,
    layer_name: str,
    profile_label: str,
    resolver,
):
    session = pipeline_v4._session(exporter)
    if session is None:
        return resolver(exporter, point, layer_name, profile_label)
    lock = session.get("lock")
    if lock is None:
        return resolver(exporter, point, layer_name, profile_label)
    key = (id(point), str(layer_name), str(profile_label))
    with lock:
        cache = session.setdefault("profile_description_cache_v7", {})
        cached = cache.get(key, _CACHE_MISS)
    if cached is not _CACHE_MISS:
        pipeline._increment_pair_counter("profile_description_cache_hits_v7")
        return cached
    result = resolver(exporter, point, layer_name, profile_label)
    with lock:
        cache[key] = result
    return result


def install_dxf_template_pipeline_v7_patch() -> None:
    """Install safe v7 optimizations without normal->reverse profile reuse.

    V7 removes one large normal-DXF save/read roundtrip, caches direction-neutral
    profile metadata inside the short-lived normal/reverse pair session and
    restores the forced-start profile fast path for reverse datasets. The latter
    still rebuilds the reverse profile independently from the reverse dataset;
    it never mirrors or derives geometry from the normal profile.
    """

    from .cadastral_export import CadastralDxfExporter

    global _V6_PROFILE_BUILDER

    pipeline_v6.install_dxf_template_pipeline_v6_patch()
    if int(
        getattr(CadastralDxfExporter, "_sleufbase_dxf_template_pipeline_v7_version", 0)
        or 0
    ) >= PATCH_VERSION:
        return

    _install_normal_save_capture()
    _install_normal_call_capture()

    # The Dynamic Visibility merge wrapper resolves this module global at call
    # time, so replacing it keeps the existing post-merge safety/finalize pass.
    pipeline._merge_reverse_variant_document_one_read = _merge_reverse_variant_document_v7

    previous_profile = CadastralDxfExporter._build_template_cross_section_profile
    previous_rule = CadastralDxfExporter._resolve_cross_section_point_rule
    previous_description = CadastralDxfExporter._cross_section_description
    _V6_PROFILE_BUILDER = previous_profile

    def _build_template_cross_section_profile_v7(
        self: Any,
        dataset: Any,
        layer_rules: Any,
        road_centerline_paths: Any,
        terrain_boundary_paths: Any,
        fallback_marker_scale: float,
        reverse_profile_direction: bool = False,
    ):
        if bool(reverse_profile_direction) and getattr(dataset, "cross_section_start_xy", None) is not None:
            # Independent reverse rebuild: this consumes the reverse dataset's
            # own forced start/end and Z values. It deliberately does not use
            # V5's _reverse_profile_from_normal helper.
            result = pipeline_v5._build_forced_profile_fast(
                self,
                dataset,
                tuple(layer_rules),
                float(fallback_marker_scale),
            )
            if result is not pipeline_v5._UNSUPPORTED:
                pipeline._increment_pair_counter("reverse_forced_profile_fast_path_v7")
                return result

        # Non-forced reverse paths still go through V6, which hides the pair
        # session and forces the original/core independent reverse builder.
        return previous_profile(
            self,
            dataset,
            layer_rules,
            road_centerline_paths,
            terrain_boundary_paths,
            fallback_marker_scale,
            reverse_profile_direction,
        )

    def _resolve_cross_section_point_rule_v7(self, point, layer_rules):
        return _session_cached_rule(self, point, layer_rules, previous_rule)

    def _cross_section_description_v7(self, point, layer_name, profile_label=""):
        return _session_cached_description(
            self,
            point,
            layer_name,
            profile_label,
            previous_description,
        )

    CadastralDxfExporter._build_template_cross_section_profile = (
        _build_template_cross_section_profile_v7
    )
    CadastralDxfExporter._resolve_cross_section_point_rule = _resolve_cross_section_point_rule_v7
    CadastralDxfExporter._cross_section_description = _cross_section_description_v7
    CadastralDxfExporter._sleufbase_dxf_template_pipeline_v7_version = PATCH_VERSION
    CadastralDxfExporter.SLEUFBASE_NORMAL_DXF_IN_MEMORY = True
    CadastralDxfExporter.SLEUFBASE_REVERSE_PROFILE_REUSE = False
    CadastralDxfExporter.SLEUFBASE_REVERSE_PROFILE_FULL_REBUILD = True
    CadastralDxfExporter.SLEUFBASE_REVERSE_PROFILE_INDEPENDENT_BUILD = True
    CadastralDxfExporter.SLEUFBASE_REVERSE_FORCED_PROFILE_FAST_PATH = True
