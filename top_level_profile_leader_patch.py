from __future__ import annotations

from . import template_reverse_patch as reverse_patch
from .autocad_profile_leader_donor import BLOCK_NAME, DONOR_SHA256
from .template_dynamic_profile_leader_patch import _profile_leader_reference_names


PATCH_VERSION = 2


_ORIGINAL_MOVE_ENTITIES = reverse_patch._move_entities_to_variant_container


def _is_profile_leader_insert(entity, document=None) -> bool:
    if getattr(entity, "dxftype", lambda: "")() != "INSERT":
        return False
    try:
        name = str(entity.dxf.name).upper()
    except Exception:
        return False
    if name == BLOCK_NAME.upper():
        return True
    if document is None:
        return False
    try:
        return name in _profile_leader_reference_names(document)
    except Exception:
        return False


def _move_entities_but_keep_profile_leaders_top_level(
    document,
    modelspace,
    entities,
    *,
    label: object,
    slot_index: int,
    mode: str,
) -> None:
    """Keep native Dynamic Block references selectable while retaining variants.

    A Dynamic Block nested inside the generated Normaal/Reverse container does
    not expose its own AutoCAD grips as a directly selectable reference.  The
    previous top-level workaround solved that part but left both modes on the
    ordinary leader layer, so the reverse merge could not retain its own leader
    positions.

    This version keeps each real template-cloned profile leader in modelspace and
    assigns the *reference* to the same per-slot Normaal/Reverse layer that its
    container would have used.  The reverse merge already imports top-level
    INSERTs on the REVERSE variant layer, so both sets of leader positions survive
    and the normal/reverse layer switch still controls visibility.
    """

    live_entities = [entity for entity in entities if getattr(entity, "is_alive", True)]
    profile_leaders = [
        entity for entity in live_entities if _is_profile_leader_insert(entity, document)
    ]
    nested_entities = [entity for entity in live_entities if entity not in profile_leaders]

    if profile_leaders:
        layer_name = reverse_patch.variant_layer_name(label, slot_index, mode)
        reverse_patch._ensure_layer(
            document,
            layer_name,
            visible=(str(mode).upper() != reverse_patch.REVERSE_MODE),
        )
        for leader in profile_leaders:
            leader.dxf.layer = layer_name

    if nested_entities:
        _ORIGINAL_MOVE_ENTITIES(
            document,
            modelspace,
            nested_entities,
            label=label,
            slot_index=slot_index,
            mode=mode,
        )


def install_top_level_profile_leader_patch() -> None:
    if getattr(reverse_patch, "_sleufbase_top_level_profile_leader_patch_version", 0) >= PATCH_VERSION:
        return

    reverse_patch._move_entities_to_variant_container = (
        _move_entities_but_keep_profile_leaders_top_level
    )
    reverse_patch.SLEUFBASE_TOP_LEVEL_PROFILE_LEADER_BLOCK = BLOCK_NAME
    reverse_patch.SLEUFBASE_TOP_LEVEL_PROFILE_LEADER_DONOR_SHA256 = DONOR_SHA256
    reverse_patch.SLEUFBASE_TOP_LEVEL_PROFILE_LEADER_VARIANT_LAYERS = True
    reverse_patch._sleufbase_top_level_profile_leader_patch_version = PATCH_VERSION
