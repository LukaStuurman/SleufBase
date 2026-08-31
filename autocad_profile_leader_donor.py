from __future__ import annotations

import base64
import json
import zlib
from pathlib import Path
from uuid import uuid4

from . import autocad_dynamic_visibility as dv


BLOCK_NAME = "SAL-VERWIJZING_LEIDING_BOVENKANT-SOD"
LAYER_NAME = "X-XX-AL-VERWIJZING-SD"
DONOR_SHA256 = "f975ccf4cbe8d98aba66dc52300c7c255e9c2d0a86aff4f78f31b7c4c0dc4c3b"
DONOR_BLOCK_RECORD_HANDLE = "2E5"
DONOR_ENTITY_HANDLES = {
    "circle": "2F6",
    "description": "2F7",
    "leader": "2FB",
    "depth": "2FC",
}
DONOR_REP_TAG_INDEX = {
    "circle": 0,
    "description": 1,
    "leader": 2,
    "depth": 3,
}
BLOCK_REP_TAG_INDEX = 4
POLAR_BASE = (0.0, 0.0, 0.0)
POLAR_TOP = (0.0, 10.0, 0.0)
LINE_START = (0.0, 1.0)
LINE_TOP = (0.0, 10.0)
CIRCLE_CENTER = (0.0, -0.5)
CIRCLE_RADIUS = 0.5
DESCRIPTION_INSERT = (-0.5, 10.0)
DEPTH_INSERT = (2.0, 10.0)
TEXT_HEIGHT = 1.5
TEXT_ROTATION = 90.0
TEXT_STYLE = "NLCS-ISO"

# Exact AutoCAD Dynamic Block metadata extracted from the user-supplied donor DXF.
# Only object/entity handles are remapped when the metadata is transplanted into
# a freshly exported SleufBase DXF. Parameter/action values and graph topology are
# intentionally preserved byte-for-byte at the DXF value level.
_DONOR_METADATA_ZLIB_B64 = (
    "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"
)

_DYNAMIC_BLOCK_XDATA_APPS = frozenset(
    {"AcDbBlockRepETag", "AcDbDynamicBlockTrueName", "AcDbDynamicBlockGUID"}
)


class ProfileLeaderDonorError(RuntimeError):
    pass


def _donor_metadata_records() -> list[list[tuple[int, str]]]:
    raw = zlib.decompress(base64.b64decode(_DONOR_METADATA_ZLIB_B64))
    data = json.loads(raw.decode("utf-8"))
    return [[(int(code), str(value)) for code, value in record] for record in data]


def ensure_profile_leader_geometry(document, block_name: str = BLOCK_NAME) -> str:
    """Create the exact block geometry from the approved user-supplied donor."""

    try:
        existing = document.blocks.get(block_name)
    except Exception:
        existing = None
    if existing is not None:
        try:
            document.blocks.delete_block(block_name, safe=False)
        except Exception as exc:
            raise ProfileLeaderDonorError(
                f"Bestaand profielverwijzingsblock {block_name!r} kon niet worden vervangen."
            ) from exc

    block = document.blocks.new(name=block_name, base_point=POLAR_BASE)
    block.add_circle(
        center=CIRCLE_CENTER,
        radius=CIRCLE_RADIUS,
        dxfattribs={"layer": "0", "linetype": "ByBlock", "color": 0, "lineweight": -2},
    )
    block.add_attdef(
        "OMSCHRIJVING",
        insert=DESCRIPTION_INSERT,
        text="",
        dxfattribs={
            "height": TEXT_HEIGHT,
            "rotation": TEXT_ROTATION,
            "style": TEXT_STYLE,
            "layer": "0",
            "linetype": "Continuous",
            "color": 7,
            "lineweight": 0,
            "lock_position": 1,
        },
    )
    block.add_lwpolyline(
        [LINE_START, LINE_TOP],
        dxfattribs={"layer": "0", "linetype": "Continuous", "color": 7, "lineweight": 0},
    )
    block.add_attdef(
        "HOOGTE",
        insert=DEPTH_INSERT,
        text="",
        dxfattribs={
            "height": TEXT_HEIGHT,
            "rotation": TEXT_ROTATION,
            "style": TEXT_STYLE,
            "layer": "0",
            "linetype": "Continuous",
            "color": 7,
            "lineweight": 0,
            "lock_position": 1,
        },
    )
    return block_name


def _remove_xdata_apps(
    record: list[tuple[int, str]],
    app_names: frozenset[str],
) -> list[tuple[int, str]]:
    result: list[tuple[int, str]] = []
    index = 0
    while index < len(record):
        code, value = record[index]
        if code == 1001 and value.strip() in app_names:
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


def _extension_dictionary_handle(record: list[tuple[int, str]]) -> str | None:
    for index, (code, value) in enumerate(record[:-1]):
        if code == 102 and value.strip().upper() == "{ACAD_XDICTIONARY":
            next_code, next_value = record[index + 1]
            if next_code == 360:
                return next_value.strip().upper()
    return None


def _inject_block_record(
    record: list[tuple[int, str]],
    *,
    xdictionary_handle: str,
    block_name: str,
) -> list[tuple[int, str]]:
    stripped = _remove_xdata_apps(record, _DYNAMIC_BLOCK_XDATA_APPS)
    result: list[tuple[int, str]] = []
    index = 0
    inserted_xdict = False
    seen_280 = False
    seen_281 = False
    while index < len(stripped):
        code, value = stripped[index]
        if code == 102 and value.strip().upper() == "{ACAD_XDICTIONARY":
            index += 1
            while index < len(stripped):
                close_code, close_value = stripped[index]
                index += 1
                if close_code == 102 and close_value.strip() == "}":
                    break
            continue
        if code == 280:
            result.append((280, "     1"))
            seen_280 = True
        elif code == 281:
            result.append((281, "     1"))
            seen_281 = True
        else:
            result.append((code, value))
        if code == 5 and not inserted_xdict:
            result.extend(
                [(102, "{ACAD_XDICTIONARY"), (360, xdictionary_handle), (102, "}")]
            )
            inserted_xdict = True
        index += 1

    if not inserted_xdict:
        raise ProfileLeaderDonorError(f"BLOCK_RECORD {block_name!r} heeft geen handle.")
    if not seen_280:
        result.append((280, "     1"))
    if not seen_281:
        result.append((281, "     1"))
    result.extend(
        [
            (1001, "AcDbDynamicBlockTrueName"),
            (1000, block_name),
            (1001, "AcDbDynamicBlockGUID"),
            (1000, "{" + str(uuid4()).upper() + "}"),
            (1001, "AcDbBlockRepETag"),
            (1070, "     1"),
            (1071, f"{BLOCK_REP_TAG_INDEX:9d}"),
        ]
    )
    return result


def _set_entity_rep_tag(
    record: list[tuple[int, str]],
    *,
    state_index: int,
    self_reference: bool,
) -> list[tuple[int, str]]:
    handle = (dv._record_handle(record) or "").upper()
    if not handle:
        raise ProfileLeaderDonorError("Profile leader entity mist een handle.")
    result = _remove_xdata_apps(record, frozenset({"AcDbBlockRepETag"}))
    result.extend(
        [
            (1001, "AcDbBlockRepETag"),
            (1070, "     1"),
            (1071, f"{int(state_index):9d}"),
            (1005, handle if self_reference else "0"),
        ]
    )
    return result


def _target_geometry_indexes(
    records: list[list[tuple[int, str]]],
    sections: list[str | None],
    block_record_handle: str,
) -> dict[str, int]:
    owner = block_record_handle.upper()
    result: dict[str, int] = {}
    for index, (record, section) in enumerate(zip(records, sections)):
        if section != "BLOCKS" or (dv._record_owner(record) or "").upper() != owner:
            continue
        record_type = dv._record_type(record)
        if record_type == "CIRCLE" and "circle" not in result:
            result["circle"] = index
        elif record_type == "LWPOLYLINE" and "leader" not in result:
            result["leader"] = index
        elif record_type == "ATTDEF":
            tag = (dv._first_value(record, 2) or "").upper()
            if tag == "OMSCHRIJVING":
                result["description"] = index
            elif tag == "HOOGTE":
                result["depth"] = index
    missing = set(DONOR_ENTITY_HANDLES) - set(result)
    if missing:
        raise ProfileLeaderDonorError(
            f"Profielverwijzingsblock mist donor-geometrie: {', '.join(sorted(missing))}."
        )
    return result


def _remap_metadata_record(
    record: list[tuple[int, str]],
    *,
    handle_map: dict[str, str],
) -> list[tuple[int, str]]:
    own_handle = (dv._record_handle(record) or "").upper()
    remapped: list[tuple[int, str]] = []
    for code, value in record:
        stripped = value.strip().upper()
        if code == 5 and own_handle in handle_map:
            remapped.append((5, handle_map[own_handle]))
        elif code in dv._HANDLE_REFERENCE_CODES and stripped in handle_map:
            remapped.append((code, handle_map[stripped]))
        else:
            remapped.append((code, value))
    return remapped


def _metadata_descendant_handles(
    records: list[list[tuple[int, str]]],
    sections: list[str | None],
    xdict_handle: str,
) -> set[str]:
    handles = {xdict_handle.upper()}
    changed = True
    while changed:
        changed = False
        for record, section in zip(records, sections):
            if section != "OBJECTS":
                continue
            handle = (dv._record_handle(record) or "").upper()
            owner = (dv._record_owner(record) or "").upper()
            if handle and owner in handles and handle not in handles:
                handles.add(handle)
                changed = True
    return handles


def promote_profile_leader_block(path: str | Path, block_name: str = BLOCK_NAME) -> int:
    """Attach the exact approved donor's AutoCAD Dynamic Block metadata."""

    dxf_path = Path(path)
    pairs, newline, had_bom = dv._read_pairs(dxf_path)
    records = dv._split_records(pairs)
    sections = dv._record_sections(records)
    block_index, block_record = dv._target_block_record(records, sections, block_name)
    block_handle = (dv._record_handle(block_record) or "").upper()
    if not block_handle:
        raise ProfileLeaderDonorError(f"BLOCK_RECORD {block_name!r} mist een handle.")

    existing_xdict = _extension_dictionary_handle(block_record)
    if existing_xdict:
        existing_handles = _metadata_descendant_handles(records, sections, existing_xdict)
        existing_types = {
            dv._record_type(record)
            for record, section in zip(records, sections)
            if section == "OBJECTS"
            and (dv._record_handle(record) or "").upper() in existing_handles
        }
        if {"BLOCKPOLARPARAMETER", "BLOCKPOLARGRIP", "BLOCKSTRETCHACTION", "BLOCKMOVEACTION"}.issubset(existing_types):
            return 0

    geometry_indexes = _target_geometry_indexes(records, sections, block_handle)
    target_entity_handles = {
        key: (dv._record_handle(records[index]) or "").upper()
        for key, index in geometry_indexes.items()
    }
    if any(not handle for handle in target_entity_handles.values()):
        raise ProfileLeaderDonorError("Een donor-geometrieentity mist een DXF-handle.")

    donor_records = _donor_metadata_records()
    donor_metadata_handles = [
        (dv._record_handle(record) or "").upper() for record in donor_records
    ]
    if any(not handle for handle in donor_metadata_handles):
        raise ProfileLeaderDonorError("Donor metadata bevat een record zonder handle.")

    cursor = max(dv._max_handle_value(pairs) + 1, dv._header_handseed(pairs))
    handle_map: dict[str, str] = {}
    for donor_handle in donor_metadata_handles:
        handle_map[donor_handle] = f"{cursor:X}"
        cursor += 1
    handle_map[DONOR_BLOCK_RECORD_HANDLE] = block_handle
    for key, donor_handle in DONOR_ENTITY_HANDLES.items():
        handle_map[donor_handle] = target_entity_handles[key]

    xdict_handle = handle_map["2E6"]
    records[block_index] = _inject_block_record(
        records[block_index],
        xdictionary_handle=xdict_handle,
        block_name=block_name,
    )

    for key, index in geometry_indexes.items():
        records[index] = _set_entity_rep_tag(
            records[index],
            state_index=DONOR_REP_TAG_INDEX[key],
            self_reference=key in {"circle", "leader"},
        )

    transplanted = [
        _remap_metadata_record(record, handle_map=handle_map) for record in donor_records
    ]
    objects_end = next(
        (
            index
            for index, (record, section) in enumerate(zip(records, sections))
            if section == "OBJECTS" and dv._record_type(record) == "ENDSEC"
        ),
        None,
    )
    if objects_end is None:
        raise ProfileLeaderDonorError("DXF bevat geen afsluiting van de OBJECTS-sectie.")
    records[objects_end:objects_end] = transplanted

    flattened = dv._flatten_records(records)
    dv._set_header_handseed(flattened, cursor)
    dv._write_pairs(dxf_path, flattened, newline=newline, had_bom=had_bom)
    return 1


def _action_entity_refs(record: list[tuple[int, str]]) -> tuple[str, ...]:
    in_action = False
    refs: list[str] = []
    for code, value in record:
        if code == 100 and value.strip() == "AcDbBlockAction":
            in_action = True
            continue
        if in_action and code == 100:
            break
        if in_action and code == 330:
            refs.append(value.strip().upper())
    return tuple(refs)


def _point_pair(record: list[tuple[int, str]]) -> tuple[tuple[float, float, float], tuple[float, float, float]]:
    first: dict[int, float] = {}
    second: dict[int, float] = {}
    for code, value in record:
        if code in {1010, 1020, 1030} and code not in first:
            first[code] = float(value.strip())
        elif code in {1011, 1021, 1031} and code not in second:
            second[code] = float(value.strip())
    return (
        (first.get(1010, 0.0), first.get(1020, 0.0), first.get(1030, 0.0)),
        (second.get(1011, 0.0), second.get(1021, 0.0), second.get(1031, 0.0)),
    )


def inspect_profile_leader_block(path: str | Path, block_name: str = BLOCK_NAME) -> dict[str, object]:
    pairs, _newline, _bom = dv._read_pairs(Path(path))
    records = dv._split_records(pairs)
    sections = dv._record_sections(records)
    block_index, block_record = dv._target_block_record(records, sections, block_name)
    block_handle = (dv._record_handle(block_record) or "").upper()
    geometry_indexes = _target_geometry_indexes(records, sections, block_handle)
    entity_handles = {
        key: (dv._record_handle(records[index]) or "").upper()
        for key, index in geometry_indexes.items()
    }
    xdict = _extension_dictionary_handle(records[block_index])
    metadata_handles = _metadata_descendant_handles(records, sections, xdict) if xdict else set()
    metadata = [
        record
        for record, section in zip(records, sections)
        if section == "OBJECTS" and (dv._record_handle(record) or "").upper() in metadata_handles
    ]
    by_type: dict[str, list[list[tuple[int, str]]]] = {}
    for record in metadata:
        by_type.setdefault(dv._record_type(record), []).append(record)

    polar = (by_type.get("BLOCKPOLARPARAMETER") or [None])[0]
    polar_grip = (by_type.get("BLOCKPOLARGRIP") or [None])[0]
    stretch = (by_type.get("BLOCKSTRETCHACTION") or [None])[0]
    move = (by_type.get("BLOCKMOVEACTION") or [None])[0]
    scale = (by_type.get("BLOCKSCALEACTION") or [None])[0]

    entity_rep_tags: dict[str, tuple[tuple[int, str], ...]] = {}
    for key, index in geometry_indexes.items():
        entity_rep_tags[key] = tuple(
            (code, value.strip())
            for code, value in dv._xdata_payload(records[index], "AcDbBlockRepETag")
        )

    polar_base = polar_top = None
    if polar is not None:
        polar_base, polar_top = _point_pair(polar)
    grip_point = None
    if polar_grip is not None:
        values = {1010: 0.0, 1020: 0.0, 1030: 0.0}
        for code, value in polar_grip:
            if code in values:
                values[code] = float(value.strip())
        grip_point = (values[1010], values[1020], values[1030])

    return {
        "donor_sha256": DONOR_SHA256,
        "is_dynamic": bool(polar and polar_grip and stretch and move and scale),
        "metadata_types": tuple(sorted(by_type)),
        "polar_base": polar_base,
        "polar_top": polar_top,
        "polar_grip": grip_point,
        "stretch_refs": _action_entity_refs(stretch) if stretch else (),
        "move_refs": _action_entity_refs(move) if move else (),
        "scale_refs": _action_entity_refs(scale) if scale else (),
        "entity_handles": entity_handles,
        "entity_rep_tags": entity_rep_tags,
        "block_rep_tag": tuple(
            (code, value.strip())
            for code, value in dv._xdata_payload(records[block_index], "AcDbBlockRepETag")
        ),
    }
