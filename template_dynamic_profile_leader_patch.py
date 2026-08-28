from __future__ import annotations

import ezdxf

from .cadastral_export import CadastralDxfExporter


PATCH_VERSION = 1
DYNAMIC_LEADER_BLOCK_NAME = CadastralDxfExporter.TEMPLATE_PROFILE_LEGACY_DYNAMIC_LEADER_BLOCK_NAME
DYNAMIC_LEADER_LAYER = "X-XX-AL-VERWIJZING-SD"
POLAR_BASE_TO_LINE_START = 1.0
POLAR_BASE_TO_TOP = 10.0


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


def _preserve_dynamic_profile_leader_donor(
    self: CadastralDxfExporter,
    document: ezdxf.EzDxfDocument,
) -> None:
    """Remove stale example/profile blocks but keep the genuine AutoCAD donor.

    The cadastral template already contains ``SAL-VERWIJZING_LEIDING_BOVENKANT-SOD``
    as a native AutoCAD Dynamic Block.  Its Polar parameter drives a Stretch
    action on the leader polyline and a Move action on the two attribute
    definitions.  Older SleufBase code deleted this donor before every export,
    which forced generated profile leaders back to a static INSERT + LEADER.
    """

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
    """Insert the template's native Polar/Stretch/Move Dynamic Block.

    ``leader_start`` is the old static LEADER start.  In the native donor the
    actual stretchable line starts one local unit above the block base and the
    Polar end grip is ten local units above that base.  Moving the insertion
    point down by one scaled unit therefore preserves the existing profile
    geometry exactly while restoring the AutoCAD action grip.

    The generated initial state intentionally stays vertical.  The user can then
    drag the Polar grip freely: the lower end stays fixed, the line stretches and
    changes angle, and OMSCHRIJVING/HOOGTE move with the top without rotating.
    """

    document = modelspace.doc
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
    """Keep native dynamic leaders in their donor default state on export.

    SleufBase's former static leader could pre-shift label tops to avoid
    collisions.  A native Dynamic Block should instead open in a valid default
    Polar state, otherwise the line and the dynamic attributes would disagree
    before AutoCAD evaluates the action.  The initial leader is therefore kept
    vertical; collision correction is now an explicit grip edit in AutoCAD.
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
    """Let the native leader block provide the fixed lower marker.

    The donor contains the same circle marker at its fixed base.  Drawing the old
    standalone marker as well would create two coincident entities on different
    layers and make AutoCAD selection unnecessarily confusing.
    """

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


def install_dynamic_profile_leader_patch() -> None:
    if getattr(CadastralDxfExporter, "_sleufbase_dynamic_profile_leader_patch_version", 0) >= PATCH_VERSION:
        return

    CadastralDxfExporter._remove_template_legacy_profile_leader_blocks = _preserve_dynamic_profile_leader_donor
    CadastralDxfExporter._add_template_profile_multileader = _add_dynamic_template_profile_leader
    CadastralDxfExporter._distribute_template_leader_labels = _distribute_dynamic_profile_leaders
    CadastralDxfExporter._add_template_profile_leader_marker = _skip_duplicate_profile_marker
    CadastralDxfExporter.SLEUFBASE_DYNAMIC_PROFILE_LEADER_BLOCK = DYNAMIC_LEADER_BLOCK_NAME
    CadastralDxfExporter.SLEUFBASE_DYNAMIC_PROFILE_LEADER_POLAR = True
    CadastralDxfExporter._sleufbase_dynamic_profile_leader_patch_version = PATCH_VERSION
