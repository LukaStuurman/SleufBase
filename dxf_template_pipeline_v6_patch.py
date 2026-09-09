from __future__ import annotations

from typing import Any

from . import dxf_template_pipeline_patch as pipeline
from . import dxf_template_pipeline_v4_patch as pipeline_v4
from . import dxf_template_pipeline_v5_patch as pipeline_v5


PATCH_VERSION = 1
_MISSING = object()


def install_dxf_template_pipeline_v6_patch() -> None:
    """Force a full profile rebuild for reverse cross sections.

    Normal and reverse template variants may have different ground levels at the
    physical profile start/end.  The rendered terrain line also depends on
    direction-sensitive maaiveld metadata.  Therefore a reverse profile is not
    generally guaranteed to be an exact mirror of the normal profile geometry.

    V5 remains active for normal-profile optimisations and same-variant deckband
    augmentation.  For every reverse profile build we temporarily hide the V4
    pair session from the V5 wrapper.  V5 consequently takes its proven fallback
    path and calls the original core profile builder instead of deriving geometry
    from the cached normal profile.
    """

    from .cadastral_export import CadastralDxfExporter

    pipeline_v5.install_dxf_template_pipeline_v5_patch()
    if int(
        getattr(CadastralDxfExporter, "_sleufbase_dxf_template_pipeline_v6_version", 0)
        or 0
    ) >= PATCH_VERSION:
        return

    previous_profile = CadastralDxfExporter._build_template_cross_section_profile

    def _build_template_cross_section_profile_v6(
        self: Any,
        dataset: Any,
        layer_rules: Any,
        road_centerline_paths: Any,
        terrain_boundary_paths: Any,
        fallback_marker_scale: float,
        reverse_profile_direction: bool = False,
    ):
        if not bool(reverse_profile_direction):
            return previous_profile(
                self,
                dataset,
                layer_rules,
                road_centerline_paths,
                terrain_boundary_paths,
                fallback_marker_scale,
                reverse_profile_direction,
            )

        # V5 only derives reverse geometry when the pair session is visible.
        # Hide that session for this one call so V5 delegates to the original
        # core builder. Restore the exact previous value even on exceptions.
        session_value = getattr(self, pipeline_v4._SESSION_ATTR, _MISSING)
        if session_value is not _MISSING:
            delattr(self, pipeline_v4._SESSION_ATTR)
        try:
            profile = previous_profile(
                self,
                dataset,
                layer_rules,
                road_centerline_paths,
                terrain_boundary_paths,
                fallback_marker_scale,
                reverse_profile_direction,
            )
        finally:
            if session_value is not _MISSING:
                setattr(self, pipeline_v4._SESSION_ATTR, session_value)

        pipeline._increment_pair_counter("reverse_profile_full_rebuild_maaiveld_safe")
        return profile

    CadastralDxfExporter._build_template_cross_section_profile = (
        _build_template_cross_section_profile_v6
    )
    CadastralDxfExporter._sleufbase_dxf_template_pipeline_v6_version = PATCH_VERSION
    CadastralDxfExporter.SLEUFBASE_REVERSE_PROFILE_REUSE = False
    CadastralDxfExporter.SLEUFBASE_REVERSE_PROFILE_FULL_REBUILD = True
