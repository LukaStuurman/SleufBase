from __future__ import annotations

from pathlib import Path

import ezdxf

from . import template_dynamic_visibility_patch as dynamic_visibility_patch
from .autocad_profile_leader_donor import (
    BLOCK_NAME,
    DONOR_SHA256,
    LAYER_NAME,
    POLAR_BASE,
    POLAR_TOP,
    ensure_profile_leader_geometry,
    inspect_profile_leader_block,
    promote_profile_leader_block,
)
from .cadastral_export import CadastralDxfExporter


PATCH_VERSION = 3
DYNAMIC_LEADER_BLOCK_NAME = BLOCK_NAME
DYNAMIC_LEADER_LAYER = LAYER_NAME
POLAR_BASE_TO_LINE_START = 1.0


_ORIGINAL_REMOVE_LEGACY = CadastralDxfExporter._remove_template_legacy_profile_leader_blocks
_ORIGINAL_ADD_MULTILEADER = CadastralDxfExporter._add_template_profile_multileader
_ORIGINAL_DISTRIBUTE_LEADERS = CadastralDxfExporter._distribute_template_leader_labels
_ORIGINAL_ADD_MARKER = CadastralDxfExporter._add_template_profile_leader_marker


def _dynamic_leader_available(document: ezdxf.EzDxfDocument | None) -> bool:
    if document is None:
        return False
    try:
        return DYNAMIC_LEADER_BLOCK_NAME in document.blocks
    except Exception:
        return False


def _install_approved_profile_leader_geometry(
    self: CadastralDxfExporter,
    document: ezdxf.EzDxfDocument,
) -> None:
    """Replace the old template example with the approved user-supplied geometry.

    The v0.3.10 exporter deliberately removes the legacy dynamic leader from the
    cadastral template. Keep that cleanup intact first, then add a clean static
    copy of the exact geometry from the approved AutoCAD donor. Its native
    Dynamic Block metadata is transplanted only after all normal/reverse export
    and ezdxf saves have finished.
    """

    _ORIGINAL_REMOVE_LEGACY(self, document)
    ensure_profile_leader_geometry(document, DYNAMIC_LEADER_BLOCK_NAME)


def _add_dynamic_template_profile_leader(
    self: CadastralDxfExporter,
    modelspace,
    description: str,
    depth_text: str,
    leader_start: tuple[float, float],
    text_insert: tuple[float, float],
    marker_scale: float,
    color: int | None = None,
) -> None:
    """Insert the approved leader using the existing v0.3.10 profile placement.

    In the approved AutoCAD donor the block base is at local (0, 0), while the
    visible stretchable line starts at local y=1. Shifting the INSERT down by one
    scaled unit keeps the lower end of the visible line at the exact v0.3.10
    leader start. The donor itself supplies the lower marker, Polar grip and
    moving attributes.
    """

    document = getattr(modelspace, "doc", None)
    if not _dynamic_leader_available(document):
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
    insert_x = float(leader_start[0])
    insert_y = float(leader_start[1]) - (POLAR_BASE_TO_LINE_START * normalized_scale)

    self._insert_template_block(
        modelspace,
        DYNAMIC_LEADER_BLOCK_NAME,
        insert=(insert_x, insert_y),
        layer_name=DYNAMIC_LEADER_LAYER,
        attributes={
            "OMSCHRIJVING": str(description),
            "HOOGTE": str(depth_text),
        },
        color=color,
        scale=normalized_scale,
    )


def _distribute_dynamic_profile_leaders(
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
    """Keep the donor's initial Polar state geometrically consistent.

    The v0.3.10 collision solver can shift only the label top. That is correct for
    a static LEADER but would make a native Dynamic Block open in a state that no
    longer matches its Polar parameter. With the approved donor present the
    initial state therefore remains vertical; the AutoCAD Polar grip is the
    explicit way to reposition that top afterwards.
    """

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
        avoid_collisions=False if _dynamic_leader_available(document) else avoid_collisions,
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
    """Do not draw the v0.3.10 marker twice; the approved block contains it."""

    if _dynamic_leader_available(getattr(modelspace, "doc", None)):
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


def _validate_promoted_profile_leader(output_path: Path) -> None:
    details = inspect_profile_leader_block(output_path, DYNAMIC_LEADER_BLOCK_NAME)
    if details.get("donor_sha256") != DONOR_SHA256:
        raise RuntimeError("De profielverwijzing gebruikt niet de goedgekeurde donorversie.")
    if not details.get("is_dynamic"):
        raise RuntimeError("De goedgekeurde kabel/leiding-verwijzing is niet Dynamic in de DXF.")
    if details.get("polar_base") != POLAR_BASE:
        raise RuntimeError(f"Polar-basispunt is gewijzigd: {details.get('polar_base')!r}.")
    if details.get("polar_top") != POLAR_TOP or details.get("polar_grip") != POLAR_TOP:
        raise RuntimeError(
            "De Polar-parameter/grip staat niet meer exact op de donor-bovenkant."
        )

    entity_handles = dict(details.get("entity_handles") or {})
    leader_handle = str(entity_handles.get("leader") or "").upper()
    circle_handle = str(entity_handles.get("circle") or "").upper()
    description_handle = str(entity_handles.get("description") or "").upper()
    depth_handle = str(entity_handles.get("depth") or "").upper()
    if tuple(details.get("stretch_refs") or ()) != (leader_handle,):
        raise RuntimeError("Stretch Action wijst niet exact naar de donor-verwijzingslijn.")
    if set(details.get("move_refs") or ()) != {description_handle, depth_handle}:
        raise RuntimeError("Move Action wijst niet exact naar OMSCHRIJVING en HOOGTE.")
    if tuple(details.get("scale_refs") or ()) != (circle_handle,):
        raise RuntimeError("Diameter Scale Action wijst niet exact naar de donorcirkel.")

    block_rep = tuple(details.get("block_rep_tag") or ())
    if block_rep[:2] != ((1070, "1"), (1071, "4")):
        raise RuntimeError(f"Dynamic Block RepETag is niet donorconform: {block_rep!r}.")
    entity_tags = dict(details.get("entity_rep_tags") or {})
    for key, expected_index in (
        ("circle", "0"),
        ("description", "1"),
        ("leader", "2"),
        ("depth", "3"),
    ):
        tag = tuple(entity_tags.get(key) or ())
        if len(tag) < 2 or tag[0] != (1070, "1") or tag[1] != (1071, expected_index):
            raise RuntimeError(f"RepETag voor {key} is niet donorconform: {tag!r}.")


def install_dynamic_profile_leader_patch() -> None:
    """Use the exact native AutoCAD donor supplied by the user for profile leaders."""

    if (
        getattr(CadastralDxfExporter, "_sleufbase_dynamic_profile_leader_patch_version", 0)
        >= PATCH_VERSION
    ):
        return

    CadastralDxfExporter._remove_template_legacy_profile_leader_blocks = (
        _install_approved_profile_leader_geometry
    )
    CadastralDxfExporter._add_template_profile_multileader = _add_dynamic_template_profile_leader
    CadastralDxfExporter._distribute_template_leader_labels = _distribute_dynamic_profile_leaders
    CadastralDxfExporter._add_template_profile_leader_marker = _skip_duplicate_profile_marker
    CadastralDxfExporter.SLEUFBASE_DYNAMIC_PROFILE_LEADER_BLOCK = DYNAMIC_LEADER_BLOCK_NAME
    CadastralDxfExporter.SLEUFBASE_DYNAMIC_PROFILE_LEADER_POLAR = True
    CadastralDxfExporter.SLEUFBASE_DYNAMIC_PROFILE_LEADER_DONOR_SHA256 = DONOR_SHA256
    CadastralDxfExporter._sleufbase_dynamic_profile_leader_patch_version = PATCH_VERSION

    # dynamic_visibility_finalize_patch is installed before this patch. Capture
    # that already-finalized function and append donor promotion afterwards. This
    # is intentionally the final raw-DXF mutation, so no later ezdxf save can
    # discard the native AutoCAD parameter/action objects from the donor.
    current_promote = dynamic_visibility_patch._promote_exported_variants_to_dynamic_blocks
    if not getattr(current_promote, "_sleufbase_profile_leader_donor_wrapper", False):

        def _promote_visibility_then_profile_leader(output_path):
            wrapper_names = current_promote(output_path)
            path = Path(output_path)
            promote_profile_leader_block(path, DYNAMIC_LEADER_BLOCK_NAME)
            _validate_promoted_profile_leader(path)
            return wrapper_names

        _promote_visibility_then_profile_leader._sleufbase_profile_leader_donor_wrapper = True
        dynamic_visibility_patch._promote_exported_variants_to_dynamic_blocks = (
            _promote_visibility_then_profile_leader
        )
