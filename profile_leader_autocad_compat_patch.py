from __future__ import annotations

from pathlib import Path

from . import autocad_dynamic_visibility as dv
from . import autocad_profile_leader_donor as donor
from . import literal_profile_leader_donor_patch as literal_patch
from . import template_dynamic_profile_leader_patch as profile_patch


PATCH_VERSION = 1
DONOR_DYNAMIC_BLOCK_GUID = "{E2960563-75C0-7641-A1E0-122EAD996DAC}"

# Exact CLASS records from the user-supplied working donor DXF.  These records
# are required for AutoCAD/ObjectDBX to instantiate the native Dynamic Block
# evaluation objects instead of treating them as unknown/proxy data.
_DONOR_CLASS_RECORDS: tuple[tuple[tuple[int, str], ...], ...] = (
    ((0, "CLASS"), (1, "ACAD_EVALUATION_GRAPH"), (2, "AcDbEvalGraph"), (3, "ObjectDBX Classes"), (90, "     1153"), (91, "        1"), (280, "     0"), (281, "     0")),
    ((0, "CLASS"), (1, "BLOCKLINEARPARAMETER"), (2, "AcDbBlockLinearParameter"), (3, "ObjectDBX Classes"), (90, "     1153"), (91, "        1"), (280, "     0"), (281, "     0")),
    ((0, "CLASS"), (1, "BLOCKLINEARGRIP"), (2, "AcDbBlockLinearGrip"), (3, "ObjectDBX Classes"), (90, "     1153"), (91, "        1"), (280, "     0"), (281, "     0")),
    ((0, "CLASS"), (1, "BLOCKGRIPLOCATIONCOMPONENT"), (2, "AcDbBlockGripExpr"), (3, "ObjectDBX Classes"), (90, "     1153"), (91, "        4"), (280, "     0"), (281, "     0")),
    ((0, "CLASS"), (1, "BLOCKSCALEACTION"), (2, "AcDbBlockScaleAction"), (3, "ObjectDBX Classes"), (90, "     1153"), (91, "        1"), (280, "     0"), (281, "     0")),
    ((0, "CLASS"), (1, "BLOCKPOLARPARAMETER"), (2, "AcDbBlockPolarParameter"), (3, "ObjectDBX Classes"), (90, "     1153"), (91, "        1"), (280, "     0"), (281, "     0")),
    ((0, "CLASS"), (1, "BLOCKPOLARGRIP"), (2, "AcDbBlockPolarGrip"), (3, "ObjectDBX Classes"), (90, "     1153"), (91, "        1"), (280, "     0"), (281, "     0")),
    ((0, "CLASS"), (1, "BLOCKSTRETCHACTION"), (2, "AcDbBlockStretchAction"), (3, "ObjectDBX Classes"), (90, "     1153"), (91, "        1"), (280, "     0"), (281, "     0")),
    ((0, "CLASS"), (1, "BLOCKMOVEACTION"), (2, "AcDbBlockMoveAction"), (3, "ObjectDBX Classes"), (90, "     1153"), (91, "        1"), (280, "     0"), (281, "     0")),
    ((0, "CLASS"), (1, "SORTENTSTABLE"), (2, "AcDbSortentsTable"), (3, "ObjectDBX Classes"), (90, "        0"), (91, "        2"), (280, "     0"), (281, "     0")),
    ((0, "CLASS"), (1, "ACDB_DYNAMICBLOCKPURGEPREVENTER_VERSION"), (2, "AcDbDynamicBlockPurgePreventer"), (3, "ObjectDBX Classes"), (90, "     1153"), (91, "        1"), (280, "     0"), (281, "     0")),
    ((0, "CLASS"), (1, "ACDB_BLOCKREPRESENTATION_DATA"), (2, "AcDbBlockRepresentationData"), (3, "ObjectDBX Classes"), (90, "     1153"), (91, "        1"), (280, "     0"), (281, "     0")),
)


def _first_value(record: list[tuple[int, str]], code: int) -> str | None:
    for item_code, value in record:
        if item_code == code:
            return value.strip()
    return None


def _remove_xdata_app(record: list[tuple[int, str]], app_name: str) -> list[tuple[int, str]]:
    result: list[tuple[int, str]] = []
    index = 0
    wanted = app_name.casefold()
    while index < len(record):
        code, value = record[index]
        if code == 1001 and value.strip().casefold() == wanted:
            index += 1
            while index < len(record):
                payload_code, _payload_value = record[index]
                if payload_code == 1001 or payload_code < 1000:
                    break
                index += 1
            continue
        result.append((code, value))
        index += 1
    return result


def _remove_braced_group(record: list[tuple[int, str]], group_name: str) -> list[tuple[int, str]]:
    result: list[tuple[int, str]] = []
    index = 0
    wanted = "{" + group_name.upper()
    while index < len(record):
        code, value = record[index]
        if code == 102 and value.strip().upper() == wanted:
            index += 1
            while index < len(record):
                close_code, close_value = record[index]
                index += 1
                if close_code == 102 and close_value.strip() == "}":
                    break
            continue
        result.append((code, value))
        index += 1
    return result


def _insert_blkrefs(record: list[tuple[int, str]], reference_handles: list[str]) -> list[tuple[int, str]]:
    cleaned = _remove_braced_group(record, "BLKREFS")
    if not reference_handles:
        return cleaned

    payload = [(102, "{BLKREFS")]
    payload.extend((331, handle) for handle in reference_handles)
    payload.append((102, "}"))

    # The donor stores BLKREFS directly after the block-layout handle (340).  If
    # that handle is absent, place it after the block name record instead.
    insert_after = None
    for index, (code, _value) in enumerate(cleaned):
        if code == 340:
            insert_after = index + 1
            break
    if insert_after is None:
        for index, (code, _value) in enumerate(cleaned):
            if code == 2:
                insert_after = index + 1
                break
    if insert_after is None:
        insert_after = len(cleaned)
    return cleaned[:insert_after] + payload + cleaned[insert_after:]


def _set_donor_guid(record: list[tuple[int, str]]) -> list[tuple[int, str]]:
    result = _remove_xdata_app(record, "AcDbDynamicBlockGUID")
    result.extend(
        [
            (1001, "AcDbDynamicBlockGUID"),
            (1000, DONOR_DYNAMIC_BLOCK_GUID),
        ]
    )
    return result


def _reference_handles(
    records: list[list[tuple[int, str]]],
    block_name: str,
) -> list[str]:
    wanted = block_name.casefold()
    handles: list[str] = []
    for record in records:
        if dv._record_type(record) != "INSERT":
            continue
        name = (_first_value(record, 2) or "").casefold()
        if name != wanted:
            continue
        handle = (dv._record_handle(record) or "").upper()
        if handle and handle not in handles:
            handles.append(handle)
    return handles


def _inject_required_classes(
    records: list[list[tuple[int, str]]],
    sections: list[str | None],
) -> int:
    """Replace the relevant CLASS records with the donor records verbatim.

    Merely having a class with the same DXF name is not enough. The cadastral
    template contains older/different registration records for these ObjectDBX
    classes. AutoCAD must see the same proxy/version flags as the working donor,
    so existing records are deliberately replaced rather than only filling in
    missing names.
    """

    donor_by_name = {
        (_first_value(list(donor_record), 1) or "").upper(): list(donor_record)
        for donor_record in _DONOR_CLASS_RECORDS
    }
    seen: set[str] = set()
    changes = 0

    for index, (record, section) in enumerate(zip(records, sections)):
        if section != "CLASSES" or dv._record_type(record) != "CLASS":
            continue
        name = (_first_value(record, 1) or "").upper()
        expected = donor_by_name.get(name)
        if expected is None:
            continue
        seen.add(name)
        if record != expected:
            records[index] = list(expected)
            changes += 1

    missing = [
        list(donor_by_name[name])
        for name in donor_by_name
        if name not in seen
    ]
    if missing:
        classes_end = next(
            (
                index
                for index, (record, section) in enumerate(zip(records, sections))
                if section == "CLASSES" and dv._record_type(record) == "ENDSEC"
            ),
            None,
        )
        if classes_end is None:
            raise RuntimeError("DXF bevat geen afsluiting van de CLASSES-sectie.")
        records[classes_end:classes_end] = missing
        changes += len(missing)

    return changes


def promote_profile_leader_autocad_compat(
    path: str | Path,
    block_name: str = donor.BLOCK_NAME,
) -> int:
    """Finish the literal donor with AutoCAD registration data from the same DXF."""

    changed = literal_patch.promote_literal_profile_leader_block(path, block_name)
    dxf_path = Path(path)
    pairs, newline, had_bom = dv._read_pairs(dxf_path)
    records = dv._split_records(pairs)
    sections = dv._record_sections(records)

    class_changes = _inject_required_classes(records, sections)
    if class_changes:
        sections = dv._record_sections(records)

    block_record_index, block_record = dv._target_block_record(records, sections, block_name)
    refs = _reference_handles(records, block_name)
    updated_block_record = _set_donor_guid(_insert_blkrefs(block_record, refs))
    block_changed = updated_block_record != block_record
    if block_changed:
        records[block_record_index] = updated_block_record

    if not class_changes and not block_changed:
        return changed

    flattened = dv._flatten_records(records)
    dv._write_pairs(dxf_path, flattened, newline=newline, had_bom=had_bom)
    return max(1, changed)


def inspect_profile_leader_autocad_compat(
    path: str | Path,
    block_name: str = donor.BLOCK_NAME,
) -> dict[str, object]:
    pairs, _newline, _had_bom = dv._read_pairs(Path(path))
    records = dv._split_records(pairs)
    sections = dv._record_sections(records)
    class_records = {
        (_first_value(record, 1) or "").upper(): tuple(record)
        for record, section in zip(records, sections)
        if section == "CLASSES" and dv._record_type(record) == "CLASS"
    }
    _block_record_index, block_record = dv._target_block_record(records, sections, block_name)

    guid = None
    for index, (code, value) in enumerate(block_record[:-1]):
        if code == 1001 and value.strip() == "AcDbDynamicBlockGUID":
            next_code, next_value = block_record[index + 1]
            if next_code == 1000:
                guid = next_value.strip()
                break

    blkrefs: list[str] = []
    in_blkrefs = False
    for code, value in block_record:
        if code == 102 and value.strip().upper() == "{BLKREFS":
            in_blkrefs = True
            continue
        if in_blkrefs and code == 102 and value.strip() == "}":
            break
        if in_blkrefs and code == 331:
            blkrefs.append(value.strip().upper())

    expected_classes = {
        (_first_value(list(record), 1) or "").upper(): tuple(record)
        for record in _DONOR_CLASS_RECORDS
    }
    return {
        "guid": guid,
        "blkrefs": tuple(blkrefs),
        "actual_refs": tuple(_reference_handles(records, block_name)),
        "required_classes": tuple(expected_classes),
        "missing_classes": tuple(
            name for name in expected_classes if name not in class_records
        ),
        "mismatched_classes": tuple(
            name
            for name, expected in expected_classes.items()
            if name in class_records and class_records[name] != expected
        ),
    }


def install_profile_leader_autocad_compat_patch() -> None:
    if getattr(donor, "_sleufbase_profile_leader_autocad_compat_patch_version", 0) >= PATCH_VERSION:
        return

    # Installed after the literal donor patch.  The final export wrapper resolves
    # profile_patch.promote_profile_leader_block at runtime, so replacing both
    # module globals makes this the last raw-DXF mutation before validation.
    donor.promote_profile_leader_block = promote_profile_leader_autocad_compat
    profile_patch.promote_profile_leader_block = promote_profile_leader_autocad_compat
    donor.SLEUFBASE_PROFILE_LEADER_DONOR_GUID = DONOR_DYNAMIC_BLOCK_GUID
    donor._sleufbase_profile_leader_autocad_compat_patch_version = PATCH_VERSION
