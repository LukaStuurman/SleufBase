from __future__ import annotations

from pathlib import Path

import ezdxf
from ezdxf.entities import Insert

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


PATCH_VERSION = 4
DYNAMIC_LEADER_BLOCK_NAME = BLOCK_NAME
DYNAMIC_LEADER_LAYER = LAYER_NAME
POLAR_BASE_TO_LINE_START = 1.0
_PROFILE_LEADER_PROTOTYPE_ATTR = "_sleufbase_profile_leader_template_prototype"
_PROFILE_LEADER_PROTOTYPE_NAME_ATTR = "_sleufbase_profile_leader_template_prototype_name"


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


def _profile_leader_reference_names(document: ezdxf.EzDxfDocument) -> set[str]:
    """Return the native block name plus template wrappers that reference it.

    AutoCAD can store a Dynamic Block reference as an anonymous/wrapper block
    instead of a plain INSERT of the authored block name.  The old SleufBase
    cleanup already treated blocks containing an INSERT of the authored leader
    as dependent blocks.  Reuse that relationship here so the actual template
    example can be found without reconstructing it.
    """

    names = {DYNAMIC_LEADER_BLOCK_NAME.upper()}
    try:
        blocks = list(document.blocks)
    except Exception:
        return names

    changed = True
    while changed:
        changed = False
        for block in blocks:
            block_name = str(getattr(block, "name", "") or "").upper()
            if not block_name or block_name in names:
                continue
            for entity in block:
                if entity.dxftype() != "INSERT":
                    continue
                if str(entity.dxf.name).upper() in names:
                    names.add(block_name)
                    changed = True
                    break
    return names


def _find_template_profile_leader_reference(
    document: ezdxf.EzDxfDocument,
):
    """Find the real example INSERT that already exists in the DXF template.

    Layout references are preferred deliberately.  A child INSERT inside an
    anonymous block is part of the Dynamic Block representation and must not be
    mistaken for the user-visible reference that AutoCAD created.
    """

    reference_names = _profile_leader_reference_names(document)
    try:
        layouts = list(document.layouts)
    except Exception:
        layouts = [document.modelspace()]

    for layout in layouts:
        try:
            inserts = list(layout.query("INSERT"))
        except Exception:
            continue
        for entity in inserts:
            try:
                name = str(entity.dxf.name).upper()
            except Exception:
                continue
            if name in reference_names:
                return layout, entity
    return None, None


def _capture_template_profile_leader_prototype(
    self: CadastralDxfExporter,
    document: ezdxf.EzDxfDocument,
) -> bool:
    """Keep a literal copy of the template example before any cleanup.

    ``DXFEntity.copy()`` copies the INSERT, its attached ATTRIB/SEQEND entities,
    XDATA and extension dictionary.  That is materially different from creating
    a fresh ``add_blockref()`` and is the important part of this patch: the
    generated leader starts from the exact AutoCAD-created reference.
    """

    layout, source = _find_template_profile_leader_reference(document)
    if source is None:
        setattr(self, _PROFILE_LEADER_PROTOTYPE_ATTR, None)
        setattr(self, _PROFILE_LEADER_PROTOTYPE_NAME_ATTR, "")
        return False

    prototype = source.copy()
    setattr(self, _PROFILE_LEADER_PROTOTYPE_ATTR, prototype)
    setattr(self, _PROFILE_LEADER_PROTOTYPE_NAME_ATTR, str(source.dxf.name))

    # The template example itself is only a donor.  Remove that one visible
    # reference, but intentionally keep every referenced block definition and
    # anonymous Dynamic Block representation intact for the clones.
    try:
        layout.delete_entity(source)
    except Exception:
        try:
            source.destroy()
        except Exception:
            pass
    return True


def _install_approved_profile_leader_geometry(
    self: CadastralDxfExporter,
    document: ezdxf.EzDxfDocument,
) -> None:
    """Prefer the untouched AutoCAD template example over reconstructed geometry.

    Older versions deleted the template's original Dynamic Block and rebuilt a
    static block through ezdxf before transplanting Dynamic Block metadata at the
    end.  That can produce DXF records that look correct to our metadata tests
    while still losing instance-specific information that AutoCAD uses for its
    Polar grip.

    When the example INSERT exists, preserve its complete block/anonymous-block
    graph and cache a true entity copy.  The previous reconstruction path remains
    only as a compatibility fallback for synthetic/minimal templates without an
    example reference.
    """

    if _capture_template_profile_leader_prototype(self, document):
        return

    _ORIGINAL_REMOVE_LEGACY(self, document)
    ensure_profile_leader_geometry(document, DYNAMIC_LEADER_BLOCK_NAME)


def _clone_template_profile_leader(
    self: CadastralDxfExporter,
    modelspace,
    *,
    insert: tuple[float, float],
    scale: float,
    attributes: dict[str, str],
    color: int | None,
):
    """Clone the cached AutoCAD INSERT and only retarget placement/data/style.

    The prototype's Dynamic Block identity, anonymous block reference, XDATA,
    extension dictionary and attached attribute entities are retained.  We do
    not synthesize a new INSERT when a real template prototype is available.
    """

    prototype = getattr(self, _PROFILE_LEADER_PROTOTYPE_ATTR, None)
    if prototype is None or not getattr(prototype, "is_alive", True):
        return None

    document = getattr(modelspace, "doc", None)
    if document is None:
        return None

    clone = prototype.copy()
    try:
        source_matrix_inverse = clone.matrix44().copy()
        source_matrix_inverse.inverse()
        target_probe = Insert.new(
            dxfattribs={
                "name": str(clone.dxf.name),
                "insert": (float(insert[0]), float(insert[1]), 0.0),
                "xscale": float(scale),
                "yscale": float(scale),
                "zscale": float(scale),
                "rotation": 0.0,
                "extrusion": clone.dxf.extrusion,
            },
            doc=document,
        )
        target_matrix = target_probe.matrix44()
        clone.transform(source_matrix_inverse @ target_matrix)
    except Exception:
        return None

    # Match the existing exporter contract.  Geometry and Dynamic Block state are
    # untouched; only the reference style and requested attribute text change.
    clone.dxf.layer = DYNAMIC_LEADER_LAYER
    if color is not None:
        clone.dxf.color = int(color)
    else:
        try:
            clone.dxf.discard("color")
        except Exception:
            pass

    wanted = {str(key).upper(): str(value) for key, value in attributes.items()}
    found: set[str] = set()
    for attribute in clone.attribs:
        tag = str(attribute.dxf.tag).upper()
        if tag not in wanted:
            continue
        attribute.dxf.text = wanted[tag]
        found.add(tag)

    # A real approved example contains both attributes.  If it does not, do not
    # silently downgrade to a partly reconstructed clone; the compatibility path
    # below can create a conventional reference instead.
    if found != set(wanted):
        return None

    modelspace.add_entity(clone)
    return clone


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
    values = {
        "OMSCHRIJVING": str(description),
        "HOOGTE": str(depth_text),
    }

    cloned = _clone_template_profile_leader(
        self,
        modelspace,
        insert=(insert_x, insert_y),
        scale=normalized_scale,
        attributes=values,
        color=color,
    )
    if cloned is not None:
        return None

    # Compatibility fallback for tests/third-party templates that do not carry
    # the original AutoCAD example INSERT.  Production cadastral_template.dxf is
    # expected to take the clone path above.
    self._insert_template_block(
        modelspace,
        DYNAMIC_LEADER_BLOCK_NAME,
        insert=(insert_x, insert_y),
        layer_name=DYNAMIC_LEADER_LAYER,
        attributes=values,
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
    CadastralDxfExporter.SLEUFBASE_DYNAMIC_PROFILE_LEADER_CLONES_TEMPLATE_INSERT = True
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
