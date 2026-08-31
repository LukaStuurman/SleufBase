from __future__ import annotations

import base64
import json
import zlib
from pathlib import Path

from . import autocad_dynamic_visibility as dv
from . import autocad_profile_leader_donor as donor
from . import template_dynamic_profile_leader_patch as profile_patch


PATCH_VERSION = 1
LITERAL_DONOR_SOURCE_SHA256 = donor.DONOR_SHA256
LITERAL_DONOR_PAYLOAD_SHA256 = "0f8cd6d5bc53a81ff2a85ad2ed2eb9438f6da687c86690b426620d6b12f44893"

# These records are copied directly from the user supplied DXF whose SHA-256 is
# ``f975ccf4cbe8d98aba66dc52300c7c255e9c2d0a86aff4f78f31b7c4c0dc4c3b``.
# They contain the native BLOCK/CIRCLE/ATTDEF/LWPOLYLINE/ATTDEF/ENDBLK records
# plus the two ATTDEF annotation extension dictionaries.  We keep the DXF values
# byte-for-byte at group-value level and remap only handles/owners that have to
# be unique in the destination drawing.
_LITERAL_DONOR_ZLIB_B64 = (
    "eNrVVWtvmzAU/SsVn0nEo3mwb8Z2GlqKK0DpuiiKCGEZXQYTIdWiqv99NhgCCXk0WlUtX7Avzr3nnHsufhVmy9j/OU0CP07mK+HLeDyWREE3CbwTJuK4IwrKoMNWqkrjCs7WskTXwEczHKVhumGhvihItVc6y6sHizBiYUUUHGC2Rth+NG6/GdbN1MQGYk+djLB1Byy35RDEjvZogiv2U/J8NHE7S61sl2pleX5mWRSECX0yhtCwoYlLit13UOxSfTYZvWyncLg5mgJ9S6llgWHiL4MGQi2p3dlhdJ0tCxRyRU47+I1db5G/KXWS+V7m+wIL/XPBjZMGrovwoCTdy09RAq8AAjT9igzoGsQC9lMGqcvkGPTLU2/vEwnGNBqt4/WqplOvptNV3TVu8CctVCqkYTLJ+41nMslcJtZYSopGNP62JwqWCZ2W4ZBaAZCmSThbpwEKvodRmIZx7s9+DQ81Ffm18n8k4fNLGC24g8m9A4e2cTui7qo5VaqnkKut8+YgiuLUS8MX3v8MShlDXurxOGvEgd7uqlV041KDyFuDSKU9zMcHYj6ZhrWdC/3jW/4QLzfLMMrE0aQtRGVf4mu17P7OGMnN0fLwZSop1THSD40RPGuM0GeNkdzW6r/Pm6lhHC/SgE/TkJAbF//nc6Q2zRG2kG4W16cqSRdfnzia06QTUfA4zTiaxrPnwE/Lm7rutU75wc7rFZ94nhSFPsvhJZsGoZW+XN2q/OKi7qNmYuree5G3CJKKp7WScgMMVIEBPxQGPgZD25lOGwPoEtupoGu84YroAcx7ICHSp8CyiAsYAgcCE+c1Olk2cAwjPo2x8fNRRP8RxsExjOA0Rq0Ro3YuxsO1B6dr48ba+Ozak7e/Ie/DFw=="
)

_DONOR_BLOCK_HANDLE = "2F5"
_DONOR_CIRCLE_HANDLE = "2F6"
_DONOR_DESCRIPTION_HANDLE = "2F7"
_DONOR_LEADER_HANDLE = "2FB"
_DONOR_DEPTH_HANDLE = "2FC"
_DONOR_ENDBLK_HANDLE = "300"
_DONOR_ANNOTATION_OBJECT_HANDLES = ("2F8", "2F9", "2FA", "2FD", "2FE", "2FF")

_ORIGINAL_PROMOTE_PROFILE_LEADER = donor.promote_profile_leader_block


class LiteralProfileLeaderDonorError(RuntimeError):
    pass


def _literal_payload() -> dict[str, list[list[tuple[int, str]]]]:
    raw = zlib.decompress(base64.b64decode(_LITERAL_DONOR_ZLIB_B64))
    decoded = json.loads(raw.decode("utf-8"))
    return {
        key: [[(int(code), str(value)) for code, value in record] for record in records]
        for key, records in decoded.items()
    }


def _first_str(record: list[tuple[int, str]], code: int) -> str | None:
    for item_code, value in record:
        if item_code == code:
            return value.strip()
    return None


def _extension_dictionary_handle(record: list[tuple[int, str]]) -> str | None:
    for index, (code, value) in enumerate(record[:-1]):
        if code == 102 and value.strip().upper() == "{ACAD_XDICTIONARY":
            next_code, next_value = record[index + 1]
            if next_code == 360:
                return next_value.strip().upper()
    return None


def _block_boundary_indexes(
    records: list[list[tuple[int, str]]],
    sections: list[str | None],
    block_record_handle: str,
) -> tuple[int, int]:
    owner = block_record_handle.upper()
    begin = end = None
    for index, (record, section) in enumerate(zip(records, sections)):
        if section != "BLOCKS" or (dv._record_owner(record) or "").upper() != owner:
            continue
        record_type = dv._record_type(record)
        if record_type == "BLOCK" and begin is None:
            begin = index
        elif record_type == "ENDBLK":
            end = index
    if begin is None or end is None:
        raise LiteralProfileLeaderDonorError("Profielverwijzingsblock mist BLOCK/ENDBLK-records.")
    return begin, end


def _has_xdata_app(record: list[tuple[int, str]], app_name: str) -> bool:
    wanted = app_name.casefold()
    return any(code == 1001 and value.strip().casefold() == wanted for code, value in record)


def _is_literal_geometry(
    records: list[list[tuple[int, str]]],
    *,
    block_begin_index: int,
    geometry_indexes: dict[str, int],
) -> bool:
    block = records[block_begin_index]
    description = records[geometry_indexes["description"]]
    depth = records[geometry_indexes["depth"]]
    leader = records[geometry_indexes["leader"]]

    return (
        _first_str(block, 70) == "2"
        and _first_str(description, 3) == "Omschrijving"
        and _first_str(depth, 3) == "Hoogte"
        and _has_xdata_app(description, "AcadAnnotative")
        and _has_xdata_app(depth, "AcadAnnotative")
        and _extension_dictionary_handle(description) is not None
        and _extension_dictionary_handle(depth) is not None
        and any(code == 43 and float(value.strip()) == 0.0 for code, value in leader)
    )


def _remap_literal_record(
    record: list[tuple[int, str]],
    *,
    handle_map: dict[str, str],
    block_name: str,
) -> list[tuple[int, str]]:
    own_handle = (dv._record_handle(record) or "").upper()
    result: list[tuple[int, str]] = []
    for code, value in record:
        stripped = value.strip()
        upper = stripped.upper()
        if code == 5 and own_handle in handle_map:
            result.append((5, handle_map[own_handle]))
        elif code in dv._HANDLE_REFERENCE_CODES and upper in handle_map:
            result.append((code, handle_map[upper]))
        elif code in {2, 3} and stripped == donor.BLOCK_NAME:
            result.append((code, block_name))
        else:
            result.append((code, value))
    return result


def promote_literal_profile_leader_block(
    path: str | Path,
    block_name: str = donor.BLOCK_NAME,
) -> int:
    """Promote the leader and then replace its geometry with literal donor DXF records.

    The older promoter already transplants the exact AutoCAD Dynamic Block
    evaluation graph.  This function deliberately keeps that graph, but replaces
    the geometry definition itself with the original donor's raw DXF records.
    The target entity handles stay unchanged so all Stretch/Move/Scale action
    references remain valid.  The ATTDEF annotation dictionaries from the donor
    are cloned as well; these were the important donor records that were lost by
    recreating the block through ezdxf.
    """

    changed = _ORIGINAL_PROMOTE_PROFILE_LEADER(path, block_name)
    dxf_path = Path(path)
    pairs, newline, had_bom = dv._read_pairs(dxf_path)
    records = dv._split_records(pairs)
    sections = dv._record_sections(records)

    _block_record_index, block_record = dv._target_block_record(records, sections, block_name)
    block_record_handle = (dv._record_handle(block_record) or "").upper()
    if not block_record_handle:
        raise LiteralProfileLeaderDonorError(f"BLOCK_RECORD {block_name!r} mist een handle.")

    geometry_indexes = donor._target_geometry_indexes(records, sections, block_record_handle)
    block_begin_index, endblk_index = _block_boundary_indexes(
        records,
        sections,
        block_record_handle,
    )

    if _is_literal_geometry(
        records,
        block_begin_index=block_begin_index,
        geometry_indexes=geometry_indexes,
    ):
        return changed

    target_handles = {
        "block": (dv._record_handle(records[block_begin_index]) or "").upper(),
        "circle": (dv._record_handle(records[geometry_indexes["circle"]]) or "").upper(),
        "description": (dv._record_handle(records[geometry_indexes["description"]]) or "").upper(),
        "leader": (dv._record_handle(records[geometry_indexes["leader"]]) or "").upper(),
        "depth": (dv._record_handle(records[geometry_indexes["depth"]]) or "").upper(),
        "endblk": (dv._record_handle(records[endblk_index]) or "").upper(),
    }
    if any(not handle for handle in target_handles.values()):
        raise LiteralProfileLeaderDonorError("Profielverwijzingsblock bevat een record zonder handle.")

    payload = _literal_payload()
    donor_block_records = {
        (dv._record_handle(record) or "").upper(): record
        for record in payload["block_records"]
    }
    expected_block_handles = {
        _DONOR_BLOCK_HANDLE,
        _DONOR_CIRCLE_HANDLE,
        _DONOR_DESCRIPTION_HANDLE,
        _DONOR_LEADER_HANDLE,
        _DONOR_DEPTH_HANDLE,
        _DONOR_ENDBLK_HANDLE,
    }
    if set(donor_block_records) != expected_block_handles:
        raise LiteralProfileLeaderDonorError("Ingesloten donor-blockrecords zijn onvolledig.")

    cursor = max(dv._max_handle_value(pairs) + 1, dv._header_handseed(pairs))
    handle_map = {
        donor.DONOR_BLOCK_RECORD_HANDLE: block_record_handle,
        _DONOR_BLOCK_HANDLE: target_handles["block"],
        _DONOR_CIRCLE_HANDLE: target_handles["circle"],
        _DONOR_DESCRIPTION_HANDLE: target_handles["description"],
        _DONOR_LEADER_HANDLE: target_handles["leader"],
        _DONOR_DEPTH_HANDLE: target_handles["depth"],
        _DONOR_ENDBLK_HANDLE: target_handles["endblk"],
    }
    for donor_handle in _DONOR_ANNOTATION_OBJECT_HANDLES:
        handle_map[donor_handle] = f"{cursor:X}"
        cursor += 1

    replacement_by_index = {
        block_begin_index: donor_block_records[_DONOR_BLOCK_HANDLE],
        geometry_indexes["circle"]: donor_block_records[_DONOR_CIRCLE_HANDLE],
        geometry_indexes["description"]: donor_block_records[_DONOR_DESCRIPTION_HANDLE],
        geometry_indexes["leader"]: donor_block_records[_DONOR_LEADER_HANDLE],
        geometry_indexes["depth"]: donor_block_records[_DONOR_DEPTH_HANDLE],
        endblk_index: donor_block_records[_DONOR_ENDBLK_HANDLE],
    }
    for index, donor_record in replacement_by_index.items():
        records[index] = _remap_literal_record(
            donor_record,
            handle_map=handle_map,
            block_name=block_name,
        )

    annotation_objects = [
        _remap_literal_record(record, handle_map=handle_map, block_name=block_name)
        for record in payload["annotation_objects"]
    ]
    if len(annotation_objects) != len(_DONOR_ANNOTATION_OBJECT_HANDLES):
        raise LiteralProfileLeaderDonorError("Ingesloten donor-annotatieobjecten zijn onvolledig.")

    # Recalculate sections after replacing records, then append the donor ATTDEF
    # dictionaries immediately before OBJECTS/ENDSEC.
    sections = dv._record_sections(records)
    objects_end = next(
        (
            index
            for index, (record, section) in enumerate(zip(records, sections))
            if section == "OBJECTS" and dv._record_type(record) == "ENDSEC"
        ),
        None,
    )
    if objects_end is None:
        raise LiteralProfileLeaderDonorError("DXF bevat geen afsluiting van de OBJECTS-sectie.")
    records[objects_end:objects_end] = annotation_objects

    flattened = dv._flatten_records(records)
    dv._set_header_handseed(flattened, cursor)
    dv._write_pairs(dxf_path, flattened, newline=newline, had_bom=had_bom)
    return 1


def inspect_literal_profile_leader_geometry(
    path: str | Path,
    block_name: str = donor.BLOCK_NAME,
) -> dict[str, object]:
    pairs, _newline, _had_bom = dv._read_pairs(Path(path))
    records = dv._split_records(pairs)
    sections = dv._record_sections(records)
    _block_record_index, block_record = dv._target_block_record(records, sections, block_name)
    block_record_handle = (dv._record_handle(block_record) or "").upper()
    geometry_indexes = donor._target_geometry_indexes(records, sections, block_record_handle)
    block_begin_index, _endblk_index = _block_boundary_indexes(records, sections, block_record_handle)

    description = records[geometry_indexes["description"]]
    depth = records[geometry_indexes["depth"]]
    leader = records[geometry_indexes["leader"]]
    return {
        "source_sha256": LITERAL_DONOR_SOURCE_SHA256,
        "payload_sha256": LITERAL_DONOR_PAYLOAD_SHA256,
        "is_literal": _is_literal_geometry(
            records,
            block_begin_index=block_begin_index,
            geometry_indexes=geometry_indexes,
        ),
        "description_prompt": _first_str(description, 3),
        "depth_prompt": _first_str(depth, 3),
        "description_annotative": _has_xdata_app(description, "AcadAnnotative"),
        "depth_annotative": _has_xdata_app(depth, "AcadAnnotative"),
        "description_xdict": _extension_dictionary_handle(description),
        "depth_xdict": _extension_dictionary_handle(depth),
        "leader_constant_width": next(
            (float(value.strip()) for code, value in leader if code == 43),
            None,
        ),
    }


def install_literal_profile_leader_donor_patch() -> None:
    if getattr(donor, "_sleufbase_literal_profile_leader_patch_version", 0) >= PATCH_VERSION:
        return

    # The dynamic-profile wrapper resolves this module global at export time, so
    # replacing both names makes every final export use the literal donor path.
    donor.promote_profile_leader_block = promote_literal_profile_leader_block
    profile_patch.promote_profile_leader_block = promote_literal_profile_leader_block
    donor.SLEUFBASE_LITERAL_PROFILE_LEADER_DONOR_SHA256 = LITERAL_DONOR_SOURCE_SHA256
    donor.SLEUFBASE_LITERAL_PROFILE_LEADER_PAYLOAD_SHA256 = LITERAL_DONOR_PAYLOAD_SHA256
    donor._sleufbase_literal_profile_leader_patch_version = PATCH_VERSION
