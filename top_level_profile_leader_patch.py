from __future__ import annotations

from . import template_reverse_patch as reverse_patch
from .autocad_profile_leader_donor import BLOCK_NAME, DONOR_SHA256


PATCH_VERSION = 1


_ORIGINAL_MOVE_ENTITIES = reverse_patch._move_entities_to_variant_container


def _is_profile_leader_insert(entity) -> bool:
    if getattr(entity, "dxftype", lambda: "")() != "INSERT":
        return False
    try:
        return str(entity.dxf.name).upper() == BLOCK_NAME.upper()
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
    """Never nest the approved AutoCAD Dynamic Leader inside the slot block.

    AutoCAD does not expose dynamic parameters/grips of a Dynamic Block that is
    nested inside another block reference.  The working user-supplied donor has
    ``SAL-VERWIJZING_LEIDING_BOVENKANT-SOD`` directly in modelspace.  Preserve
    that same topology: profile-leader INSERTs stay top-level while all other
    generated slot entities still move into the normal/reverse content block.

    The reverse export is merged by selecting only the top-level variant
    container INSERTs.  Therefore these top-level leader INSERTs are taken from
    the normal export only and cannot accidentally be duplicated by the reverse
    merge.
    """

    live_entities = [entity for entity in entities if getattr(entity, "is_alive", True)]
    nested_entities = [
        entity for entity in live_entities if not _is_profile_leader_insert(entity)
    ]
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
    reverse_patch._sleufbase_top_level_profile_leader_patch_version = PATCH_VERSION
