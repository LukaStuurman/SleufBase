from __future__ import annotations

from pathlib import Path
from typing import Iterable

from . import autocad_dynamic_visibility as dv
from . import template_dynamic_visibility_patch as dynamic_patch


PATCH_VERSION = 1
_STATE_XDATA_APP = "AcDbBlockRepETag"


def _remove_xdata_app(
    record: list[tuple[int, str]],
    app_name: str,
) -> list[tuple[int, str]]:
    """Remove one XData application without touching following DXF data."""

    result: list[tuple[int, str]] = []
    index = 0
    while index < len(record):
        code, value = record[index]
        if code == 1001 and value.strip() == app_name:
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


def _set_insert_initial_visibility(
    record: list[tuple[int, str]],
    *,
    state_index: int,
    visible: bool,
) -> list[tuple[int, str]]:
    """Match AutoCAD's own base-state representation for a visibility entity.

    The real template donor keeps state 0 visible, stores state 1 with DXF group
    code 60=1, and tags both entities with ``AcDbBlockRepETag`` state indices.
    AutoCAD then has a correct visual base representation before the first
    dynamic-block evaluation takes place.
    """

    if dv._record_type(record) != "INSERT":
        raise dv.DynamicVisibilityError("Visibility-state entity is geen INSERT.")
    handle = (dv._record_handle(record) or "").upper()
    if not handle:
        raise dv.DynamicVisibilityError("Visibility-state INSERT heeft geen handle.")

    result = [(code, value) for code, value in record if code != 60]
    result = _remove_xdata_app(result, _STATE_XDATA_APP)

    if not visible:
        insert_at = next(
            (
                index
                for index, (code, value) in enumerate(result)
                if code == 100 and value.strip() == "AcDbBlockReference"
            ),
            None,
        )
        if insert_at is None:
            raise dv.DynamicVisibilityError(
                "Visibility-state INSERT mist de AcDbBlockReference subclass."
            )
        result.insert(insert_at, (60, "     1"))

    result.extend(
        [
            (1001, _STATE_XDATA_APP),
            (1070, "     1"),
            (1071, f"{int(state_index):9d}"),
            (1005, handle),
        ]
    )
    return result


def _target_state_record_indexes(
    records: list[list[tuple[int, str]]],
    sections: list[str | None],
    block_record_handle: str,
) -> tuple[int, int]:
    normal_index: int | None = None
    reverse_index: int | None = None
    owner = block_record_handle.upper()
    for index, (record, section) in enumerate(zip(records, sections)):
        if section != "BLOCKS" or dv._record_type(record) != "INSERT":
            continue
        if (dv._record_owner(record) or "").upper() != owner:
            continue
        name = (dv._record_name(record) or "").upper()
        if name.endswith("_NORMAAL_CONTENT"):
            normal_index = index
        elif name.endswith("_REVERSE_CONTENT"):
            reverse_index = index
    if normal_index is None or reverse_index is None:
        raise dv.DynamicVisibilityError(
            "Dynamic wrapper mist de NORMAAL/REVERSE state-INSERTs."
        )
    return normal_index, reverse_index


def _extension_dictionary_handle(block_record: list[tuple[int, str]]) -> str | None:
    for index, (code, value) in enumerate(block_record[:-1]):
        if code != 102 or value.strip().upper() != "{ACAD_XDICTIONARY":
            continue
        next_code, next_value = block_record[index + 1]
        if next_code == 360:
            return next_value.strip().upper()
    return None


def _dynamic_metadata_indexes(
    records: list[list[tuple[int, str]]],
    sections: list[str | None],
    block_record: list[tuple[int, str]],
) -> dict[str, int]:
    xdict = _extension_dictionary_handle(block_record)
    if not xdict:
        raise dv.DynamicVisibilityError("Dynamic block mist een extension dictionary.")

    metadata_handles = {xdict}
    changed = True
    while changed:
        changed = False
        for record, section in zip(records, sections):
            if section != "OBJECTS":
                continue
            handle = (dv._record_handle(record) or "").upper()
            owner = (dv._record_owner(record) or "").upper()
            if handle and owner in metadata_handles and handle not in metadata_handles:
                metadata_handles.add(handle)
                changed = True

    indexes: dict[str, int] = {}
    for index, (record, section) in enumerate(zip(records, sections)):
        if section != "OBJECTS":
            continue
        if (dv._record_handle(record) or "").upper() not in metadata_handles:
            continue
        record_type = dv._record_type(record)
        if record_type in {"BLOCKVISIBILITYPARAMETER", "BLOCKVISIBILITYGRIP"}:
            indexes[record_type] = index
    if "BLOCKVISIBILITYPARAMETER" not in indexes or "BLOCKVISIBILITYGRIP" not in indexes:
        raise dv.DynamicVisibilityError(
            "Dynamic block mist de Visibility parameter of grip metadata."
        )
    return indexes


def _point_1010(record: list[tuple[int, str]]) -> tuple[float, float, float] | None:
    values: dict[int, float] = {}
    for code, value in record:
        if code not in {1010, 1020, 1030} or code in values:
            continue
        try:
            values[code] = float(value.strip())
        except ValueError:
            return None
    if 1010 not in values or 1020 not in values:
        return None
    return (values[1010], values[1020], values.get(1030, 0.0))


def _translate_point_1010(
    record: list[tuple[int, str]],
    delta: tuple[float, float, float],
) -> list[tuple[int, str]]:
    replacements = {1010: delta[0], 1020: delta[1], 1030: delta[2]}
    seen: set[int] = set()
    result: list[tuple[int, str]] = []
    for code, value in record:
        if code in replacements and code not in seen:
            try:
                base = float(value.strip())
            except ValueError:
                result.append((code, value))
            else:
                result.append((code, f"{base + replacements[code]:.12g}"))
                seen.add(code)
            continue
        result.append((code, value))
    return result


def _selector_locations(
    path: Path,
    block_names: Iterable[str],
) -> dict[str, tuple[float, float, float]]:
    """Place the dropdown just above/right of each cross-section profile.

    Normal variant content contains the slot images plus, when enabled, the
    generated cross-section entities.  Excluding IMAGE entities therefore gives
    us the actual profile bounds; image bounds are only used as a fallback for a
    slot without a cross-section.
    """

    import ezdxf
    from ezdxf import bbox

    document = ezdxf.readfile(path)
    result: dict[str, tuple[float, float, float]] = {}
    for block_name in block_names:
        wrapper = document.blocks.get(block_name)
        normal_ref = next(
            (
                entity
                for entity in wrapper.query("INSERT")
                if str(entity.dxf.name).upper().endswith("_NORMAAL_CONTENT")
            ),
            None,
        )
        if normal_ref is None:
            continue
        content = document.blocks.get(str(normal_ref.dxf.name))
        all_entities = list(content)
        profile_entities = [entity for entity in all_entities if entity.dxftype() != "IMAGE"]
        candidates = profile_entities or all_entities
        if not candidates:
            continue
        try:
            bounds = bbox.extents(candidates, fast=True)
            if not bounds.has_data:
                continue
            minimum = bounds.extmin
            maximum = bounds.extmax
            width = max(0.0, float(maximum.x - minimum.x))
            height = max(0.0, float(maximum.y - minimum.y))
            span = max(width, height)
            margin = max(0.15, min(0.75, span * 0.08))
            insert = normal_ref.dxf.insert
            result[block_name] = (
                float(maximum.x) + float(insert.x) + margin,
                float(maximum.y) + float(insert.y) + margin,
                float(maximum.z) + float(insert.z),
            )
        except Exception:
            continue
    return result


def finalize_dynamic_visibility_blocks(
    path: str | Path,
    block_names: Iterable[str],
) -> int:
    """Finalize native Dynamic Blocks for correct first-open rendering."""

    dxf_path = Path(path)
    names = list(block_names)
    selector_locations = _selector_locations(dxf_path, names)
    pairs, newline, had_bom = dv._read_pairs(dxf_path)
    records = dv._split_records(pairs)
    sections = dv._record_sections(records)

    finalized = 0
    for block_name in names:
        block_index, block_record = dv._target_block_record(records, sections, block_name)
        block_handle = (dv._record_handle(block_record) or "").upper()
        if not block_handle:
            raise dv.DynamicVisibilityError(
                f"Dynamic block {block_name!r} heeft geen BLOCK_RECORD-handle."
            )

        normal_index, reverse_index = _target_state_record_indexes(
            records,
            sections,
            block_handle,
        )
        records[normal_index] = _set_insert_initial_visibility(
            records[normal_index],
            state_index=0,
            visible=True,
        )
        records[reverse_index] = _set_insert_initial_visibility(
            records[reverse_index],
            state_index=1,
            visible=False,
        )

        target_grip = selector_locations.get(block_name)
        if target_grip is not None:
            metadata = _dynamic_metadata_indexes(records, sections, records[block_index])
            grip_index = metadata["BLOCKVISIBILITYGRIP"]
            parameter_index = metadata["BLOCKVISIBILITYPARAMETER"]
            current_grip = _point_1010(records[grip_index])
            if current_grip is not None:
                delta = (
                    target_grip[0] - current_grip[0],
                    target_grip[1] - current_grip[1],
                    target_grip[2] - current_grip[2],
                )
                # Translate both together so the donor's tested parameter/grip
                # relationship stays intact while the actual dropdown moves.
                records[grip_index] = _translate_point_1010(records[grip_index], delta)
                records[parameter_index] = _translate_point_1010(
                    records[parameter_index],
                    delta,
                )
        finalized += 1

    dv._write_pairs(
        dxf_path,
        dv._flatten_records(records),
        newline=newline,
        had_bom=had_bom,
    )
    return finalized


def inspect_finalized_dynamic_block(
    path: str | Path,
    block_name: str,
) -> dict[str, object]:
    pairs, _newline, _bom = dv._read_pairs(Path(path))
    records = dv._split_records(pairs)
    sections = dv._record_sections(records)
    block_index, block_record = dv._target_block_record(records, sections, block_name)
    block_handle = (dv._record_handle(block_record) or "").upper()
    normal_index, reverse_index = _target_state_record_indexes(records, sections, block_handle)
    metadata = _dynamic_metadata_indexes(records, sections, records[block_index])
    normal = records[normal_index]
    reverse = records[reverse_index]
    return {
        "normal_invisible": dv._first_value(normal, 60) == "1",
        "reverse_invisible": dv._first_value(reverse, 60) == "1",
        "normal_state_tag": tuple(
            (code, value.strip()) for code, value in dv._xdata_payload(normal, _STATE_XDATA_APP)
        ),
        "reverse_state_tag": tuple(
            (code, value.strip()) for code, value in dv._xdata_payload(reverse, _STATE_XDATA_APP)
        ),
        "grip_point": _point_1010(records[metadata["BLOCKVISIBILITYGRIP"]]),
        "parameter_point": _point_1010(records[metadata["BLOCKVISIBILITYPARAMETER"]]),
    }


def install_dynamic_visibility_finalize_patch() -> None:
    """Post-process each promoted wrapper with AutoCAD-compatible base state."""

    if getattr(dynamic_patch, "_sleufbase_dynamic_finalize_patch_version", 0) >= PATCH_VERSION:
        return

    original_promote = dynamic_patch._promote_exported_variants_to_dynamic_blocks

    def _promote_and_finalize(output_path):
        names = original_promote(output_path)
        finalized = finalize_dynamic_visibility_blocks(Path(output_path), names)
        if finalized != len(names):
            raise RuntimeError(
                f"Slechts {finalized} van {len(names)} Dynamic Blocks zijn geïnitialiseerd."
            )
        return names

    dynamic_patch._promote_exported_variants_to_dynamic_blocks = _promote_and_finalize
    dynamic_patch._sleufbase_dynamic_finalize_patch_version = PATCH_VERSION
