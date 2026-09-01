from __future__ import annotations

from . import autocad_profile_leader_donor as donor
from . import template_dynamic_profile_leader_patch as profile_patch
from .cadastral_export import CadastralDxfExporter


PATCH_VERSION = 1


def _definition_signature(document) -> tuple[str, tuple[str, ...]] | None:
    try:
        block = document.blocks.get(donor.BLOCK_NAME)
    except Exception:
        return None
    if block is None:
        return None
    block_record_handle = str(getattr(block, "block_record_handle", "") or "").upper()
    entity_handles = tuple(
        str(getattr(entity.dxf, "handle", "") or "").upper()
        for entity in block
    )
    return block_record_handle, entity_handles


def install_profile_leader_original_definition_patch() -> None:
    """Never rebuild the production template's original AutoCAD leader block."""

    if getattr(CadastralDxfExporter, "_sleufbase_profile_leader_original_definition_patch_version", 0) >= PATCH_VERSION:
        return

    current_remove = CadastralDxfExporter._remove_template_legacy_profile_leader_blocks

    def _preserve_original_definition_or_fallback(self, document):
        signature = _definition_signature(document)
        if signature is not None:
            # The production template has the original AutoCAD-authored BLOCK but
            # no placed example INSERT.  Preserve that definition byte-for-byte
            # in memory instead of deleting/rebuilding it through ezdxf.
            setattr(self, profile_patch._PROFILE_LEADER_PROTOTYPE_ATTR, None)
            setattr(self, profile_patch._PROFILE_LEADER_PROTOTYPE_NAME_ATTR, donor.BLOCK_NAME)
            setattr(self, "_sleufbase_profile_leader_original_definition_signature", signature)
            return None
        return current_remove(self, document)

    # Keep compatibility with the existing activation contract/tests: this is an
    # extension of the dynamic profile-leader patch, not an unrelated exporter.
    _preserve_original_definition_or_fallback.__module__ = profile_patch.__name__
    CadastralDxfExporter._remove_template_legacy_profile_leader_blocks = (
        _preserve_original_definition_or_fallback
    )
    CadastralDxfExporter.SLEUFBASE_DYNAMIC_PROFILE_LEADER_PRESERVES_TEMPLATE_DEFINITION = True
    CadastralDxfExporter._sleufbase_profile_leader_original_definition_patch_version = PATCH_VERSION
