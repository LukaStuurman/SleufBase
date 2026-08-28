from __future__ import annotations

import ezdxf

from . import template_dynamic_visibility_patch as dynamic_visibility_patch
from .autocad_synthetic_polar_leader import (
    BASE_POINT,
    BLOCK_NAME as DYNAMIC_LEADER_BLOCK_NAME,
    LAYER_NAME as DYNAMIC_LEADER_LAYER,
    TOP_POINT,
    ensure_synthetic_polar_leader_block,
    inspect_synthetic_polar_leader,
    promote_synthetic_polar_leader,
)
from .cadastral_export import CadastralDxfExporter


PATCH_VERSION = 2


_ORIGINAL_REMOVE_LEGACY = CadastralDxfExporter._remove_template_legacy_profile_leader_blocks
_ORIGINAL_ADD_MULTILEADER = CadastralDxfExporter._add_template_profile_multileader
_ORIGINAL_DISTRIBUTE_LEADERS = CadastralDxfExporter._distribute_template_leader_labels
_ORIGINAL_ADD_MARKER = CadastralDxfExporter._add_template_profile_leader_marker


def _synthetic_dynamic_leader_available(document: ezdxf.EzDxfDocument | None) -> bool:
    if document is None:
        return False
    try:
        return DYNAMIC_LEADER_BLOCK_NAME in document.blocks
    except Exception:
        return False


def _prepare_synthetic_profile_leader(
    self: CadastralDxfExporter,
    document: ezdxf.EzDxfDocument,
) -> None:
    """Delete the old example donor and create SleufBase's own static block.

    No entity, grip coordinate, evaluation node or action from the cadastral
    example block is reused. The new block starts as ordinary geometry; its own
    AutoCAD Polar parameter/action graph is attached after the complete
    Normaal/Reverse export has been assembled.
    """

    _ORIGINAL_REMOVE_LEGACY(self, document)
    ensure_synthetic_polar_leader_block(document, DYNAMIC_LEADER_BLOCK_NAME)


def _add_synthetic_dynamic_template_profile_leader(
    self: CadastralDxfExporter,
    modelspace,
    description: str,
    depth_text: str,
    leader_start: tuple[float, float],
    text_insert: tuple[float, float],
    marker_scale: float,
    color: int | None = None,
) -> None:
    """Insert our own leader geometry with its fixed base at the profile point."""

    document = modelspace.doc
    if not _synthetic_dynamic_leader_available(document):
        return _ORIGINAL_ADD_MULTILEADER(
            self,
            modelspace,
            description,
            depth_text,
            leader_start,
            text_insert,
            marker_scale,
            color,
        )

    normalized_scale = max(0.005, float(marker_scale))
    self._insert_template_block(
        modelspace,
        DYNAMIC_LEADER_BLOCK_NAME,
        insert=(float(leader_start[0]), float(leader_start[1])),
        layer_name=DYNAMIC_LEADER_LAYER,
        attributes={
            "OMSCHRIJVING": str(description),
            "HOOGTE": str(depth_text),
        },
        color=color,
        scale=normalized_scale,
    )


def _distribute_synthetic_profile_leaders(
    self: CadastralDxfExporter,
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
    """Leave generated leaders in their own well-defined default Polar state."""

    return _ORIGINAL_DISTRIBUTE_LEADERS(
        self,
        document,
        modelspace,
        entries,
        marker_scale,
        hard_static_line_segments=hard_static_line_segments,
        soft_static_line_segments=soft_static_line_segments,
        static_text_boxes=static_text_boxes,
        min_text_x=min_text_x,
        max_text_x=max_text_x,
        avoid_collisions=False if _synthetic_dynamic_leader_available(document) else avoid_collisions,
    )


def _skip_duplicate_profile_marker(
    self: CadastralDxfExporter,
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
    if _synthetic_dynamic_leader_available(getattr(modelspace, "doc", None)):
        return None
    return _ORIGINAL_ADD_MARKER(
        self,
        modelspace,
        insert_x,
        insert_y,
        marker_scale,
        layer_name,
        color,
        clip_left,
        clip_right,
        clip_bottom,
    )


def _install_final_dxf_promotion() -> None:
    if getattr(dynamic_visibility_patch, "_sleufbase_synthetic_polar_profile_wrapped", False):
        return

    original_promote = dynamic_visibility_patch._promote_exported_variants_to_dynamic_blocks

    def _promote_visibility_and_synthetic_polar(output_path):
        result = original_promote(output_path)

        # Generic Dynamic-Visibility tests and helper DXFs do not necessarily
        # contain a profile leader. Only promote our Polar block when the
        # complete export actually contains that SleufBase-owned block.
        document = ezdxf.readfile(output_path)
        if not _synthetic_dynamic_leader_available(document):
            return result

        promote_synthetic_polar_leader(output_path, DYNAMIC_LEADER_BLOCK_NAME)

        # Structural release guard: the final merged file itself must contain
        # our top grip and both actions before it is handed to the user.
        details = inspect_synthetic_polar_leader(output_path, DYNAMIC_LEADER_BLOCK_NAME)
        expected_top = tuple(float(value) for value in TOP_POINT)
        expected_base = tuple(float(value) for value in BASE_POINT)
        if not details.get("is_dynamic"):
            raise RuntimeError("SleufBase Polar-grip ontbreekt in de definitieve DXF.")
        if details.get("parameter_base") != expected_base:
            raise RuntimeError(f"Polar-basis is ongeldig: {details.get('parameter_base')!r}")
        if details.get("parameter_top") != expected_top or details.get("grip_top") != expected_top:
            raise RuntimeError(f"Polar-bovengrip staat niet op de eigen SleufBase-top: {details!r}")
        if details.get("parameter_labels") != ("Lengte", "Hoek"):
            raise RuntimeError(f"Polar-eigenschappen zijn ongeldig: {details.get('parameter_labels')!r}")
        if details.get("parameter_count") != 1 or details.get("grip_count") != 1:
            raise RuntimeError("De verwijzing heeft niet exact één eigen Polar-parameter en -grip.")
        if details.get("stretch_count") != 1 or details.get("move_count") != 1:
            raise RuntimeError("De verwijzing mist zijn eigen Stretch- of Move-action.")

        # Re-open after raw Dynamic Block creation. This catches malformed DXF
        # records immediately during export/CI rather than in AutoCAD.
        ezdxf.readfile(output_path)
        return result

    dynamic_visibility_patch._promote_exported_variants_to_dynamic_blocks = (
        _promote_visibility_and_synthetic_polar
    )
    dynamic_visibility_patch._sleufbase_synthetic_polar_profile_wrapped = True


def install_dynamic_profile_leader_patch() -> None:
    if getattr(CadastralDxfExporter, "_sleufbase_dynamic_profile_leader_patch_version", 0) >= PATCH_VERSION:
        return

    CadastralDxfExporter._remove_template_legacy_profile_leader_blocks = _prepare_synthetic_profile_leader
    CadastralDxfExporter._add_template_profile_multileader = _add_synthetic_dynamic_template_profile_leader
    CadastralDxfExporter._distribute_template_leader_labels = _distribute_synthetic_profile_leaders
    CadastralDxfExporter._add_template_profile_leader_marker = _skip_duplicate_profile_marker
    CadastralDxfExporter.SLEUFBASE_DYNAMIC_PROFILE_LEADER_BLOCK = DYNAMIC_LEADER_BLOCK_NAME
    CadastralDxfExporter.SLEUFBASE_DYNAMIC_PROFILE_LEADER_POLAR = True
    CadastralDxfExporter.SLEUFBASE_DYNAMIC_PROFILE_LEADER_SOURCE = "synthetic"
    CadastralDxfExporter._sleufbase_dynamic_profile_leader_patch_version = PATCH_VERSION

    _install_final_dxf_promotion()
