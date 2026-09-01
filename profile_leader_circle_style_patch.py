from __future__ import annotations

from pathlib import Path

from . import autocad_dynamic_visibility as dv
from . import autocad_profile_leader_donor as donor
from . import template_dynamic_profile_leader_patch as profile_patch
from .cadastral_export import CadastralDxfExporter


PATCH_VERSION = 1


def _matches_donor_circle(entity) -> bool:
    if getattr(entity, "dxftype", lambda: "")() != "CIRCLE":
        return False
    try:
        center = entity.dxf.center
        return (
            abs(float(center.x) - float(donor.CIRCLE_CENTER[0])) <= 1e-9
            and abs(float(center.y) - float(donor.CIRCLE_CENTER[1])) <= 1e-9
            and abs(float(entity.dxf.radius) - float(donor.CIRCLE_RADIUS)) <= 1e-9
        )
    except Exception:
        return False


def _set_document_circle_style(document) -> int:
    """Change only the donor ball to the reference layer + ByBlock colour."""

    try:
        block = document.blocks.get(donor.BLOCK_NAME)
    except Exception:
        return 0

    changed = 0
    for circle in block.query("CIRCLE"):
        if not _matches_donor_circle(circle):
            continue
        if str(circle.dxf.layer) != donor.LAYER_NAME:
            circle.dxf.layer = donor.LAYER_NAME
            changed += 1
        if int(circle.dxf.get("color", 256)) != 0:
            circle.dxf.color = 0
            changed += 1
    return changed


def _set_record_value(
    record: list[tuple[int, str]],
    code: int,
    value: str,
) -> tuple[list[tuple[int, str]], bool]:
    result: list[tuple[int, str]] = []
    changed = False
    found = False
    for item_code, item_value in record:
        if item_code == code and not found:
            found = True
            if item_value.strip() != value.strip():
                changed = True
            result.append((code, value))
        else:
            result.append((item_code, item_value))
    if not found:
        insert_at = next(
            (index + 1 for index, (item_code, _item_value) in enumerate(result) if item_code == 8),
            len(result),
        )
        result.insert(insert_at, (code, value))
        changed = True
    return result, changed


def _set_output_circle_style(path: str | Path, block_name: str = donor.BLOCK_NAME) -> int:
    """Apply the one allowed donor mutation after literal raw-DXF promotion."""

    dxf_path = Path(path)
    pairs, newline, had_bom = dv._read_pairs(dxf_path)
    records = dv._split_records(pairs)
    sections = dv._record_sections(records)
    _block_record_index, block_record = dv._target_block_record(records, sections, block_name)
    block_record_handle = (dv._record_handle(block_record) or "").upper()
    if not block_record_handle:
        return 0

    geometry_indexes = donor._target_geometry_indexes(records, sections, block_record_handle)
    circle_index = geometry_indexes["circle"]
    circle_record = records[circle_index]
    circle_record, layer_changed = _set_record_value(circle_record, 8, donor.LAYER_NAME)
    circle_record, color_changed = _set_record_value(circle_record, 62, "     0")
    if not (layer_changed or color_changed):
        return 0

    records[circle_index] = circle_record
    dv._write_pairs(
        dxf_path,
        dv._flatten_records(records),
        newline=newline,
        had_bom=had_bom,
    )
    return 1


def install_profile_leader_circle_style_patch() -> None:
    """Keep variant visibility on the INSERT while styling only the donor circle."""

    if getattr(CadastralDxfExporter, "_sleufbase_profile_leader_circle_style_patch_version", 0) >= PATCH_VERSION:
        return

    current_remove = CadastralDxfExporter._remove_template_legacy_profile_leader_blocks

    def _capture_then_style_circle(self, document):
        current_remove(self, document)
        _set_document_circle_style(document)

    CadastralDxfExporter._remove_template_legacy_profile_leader_blocks = _capture_then_style_circle

    current_promote = profile_patch.promote_profile_leader_block

    def _promote_then_style_circle(path, block_name=donor.BLOCK_NAME):
        changed = int(current_promote(path, block_name) or 0)
        style_changed = _set_output_circle_style(path, block_name)
        return 1 if changed or style_changed else 0

    _promote_then_style_circle._sleufbase_profile_leader_circle_style_wrapper = True
    profile_patch.promote_profile_leader_block = _promote_then_style_circle
    donor.promote_profile_leader_block = _promote_then_style_circle
    CadastralDxfExporter.SLEUFBASE_PROFILE_LEADER_CIRCLE_LAYER = donor.LAYER_NAME
    CadastralDxfExporter.SLEUFBASE_PROFILE_LEADER_CIRCLE_BYBLOCK_COLOR = True
    CadastralDxfExporter._sleufbase_profile_leader_circle_style_patch_version = PATCH_VERSION
