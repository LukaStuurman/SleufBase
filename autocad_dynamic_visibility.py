from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Iterable
from uuid import uuid4


NORMAL_STATE = "Normaal"
REVERSE_STATE = "Reverse"
PROPERTY_NAME = "Versie"
_DONOR_STATE_NAMES = ("Ter goedkeuring", "Definitief")
_HANDLE_REFERENCE_CODES = frozenset(
    list(range(320, 370))
    + list(range(390, 400))
    + [480, 481, 1005]
)
_DYNAMIC_BLOCK_XDATA_APPS = frozenset(
    {
        "AcDbBlockRepETag",
        "AcDbDynamicBlockTrueName",
        "AcDbDynamicBlockGUID",
    }
)


class DynamicVisibilityError(RuntimeError):
    pass


@dataclass
class _Donor:
    block_record_handle: str
    xdictionary_handle: str
    metadata_handles: tuple[str, ...]
    state_entity_handles: tuple[str, str]
    block_rep_e_tag: tuple[tuple[int, str], ...]


def _read_pairs(path: Path) -> tuple[list[tuple[int, str]], str, bool]:
    raw = path.read_bytes()
    had_bom = raw.startswith(b"\xef\xbb\xbf")
    text = raw.decode("utf-8-sig")
    newline = "\r\n" if "\r\n" in text else "\n"
    lines = text.splitlines()
    if len(lines) % 2:
        raise DynamicVisibilityError(
            f"DXF heeft een oneven aantal regels en kan niet veilig worden aangepast: {path}"
        )
    pairs: list[tuple[int, str]] = []
    for index in range(0, len(lines), 2):
        try:
            code = int(lines[index].strip())
        except ValueError as exc:
            raise DynamicVisibilityError(
                f"Ongeldige DXF-groepcode op regel {index + 1}: {lines[index]!r}"
            ) from exc
        pairs.append((code, lines[index + 1]))
    return pairs, newline, had_bom


def _write_pairs(
    path: Path,
    pairs: Iterable[tuple[int, str]],
    *,
    newline: str,
    had_bom: bool,
) -> None:
    body = "".join(f"{code:>3}{newline}{value}{newline}" for code, value in pairs)
    if had_bom:
        body = "\ufeff" + body
    temporary = path.with_name(f".{path.name}.dynamic.tmp")
    temporary.write_text(body, encoding="utf-8", newline="")
    temporary.replace(path)


def _split_records(pairs: Iterable[tuple[int, str]]) -> list[list[tuple[int, str]]]:
    records: list[list[tuple[int, str]]] = []
    current: list[tuple[int, str]] = []
    for pair in pairs:
        if pair[0] == 0 and current:
            records.append(current)
            current = [pair]
        else:
            current.append(pair)
    if current:
        records.append(current)
    return records


def _flatten_records(records: Iterable[Iterable[tuple[int, str]]]) -> list[tuple[int, str]]:
    return [pair for record in records for pair in record]


def _record_type(record: list[tuple[int, str]]) -> str:
    if record and record[0][0] == 0:
        return record[0][1].strip().upper()
    return ""


def _first_value(record: list[tuple[int, str]], code: int) -> str | None:
    for pair_code, value in record:
        if pair_code == code:
            return value.strip()
    return None


def _record_handle(record: list[tuple[int, str]]) -> str | None:
    return _first_value(record, 5)


def _record_owner(record: list[tuple[int, str]]) -> str | None:
    return _first_value(record, 330)


def _record_name(record: list[tuple[int, str]]) -> str | None:
    return _first_value(record, 2)


def _xdata_payload(
    record: list[tuple[int, str]],
    app_name: str,
) -> tuple[tuple[int, str], ...]:
    """Return one XData application's payload, excluding the 1001 app marker."""

    for index, (code, value) in enumerate(record):
        if code != 1001 or value.strip() != app_name:
            continue
        payload: list[tuple[int, str]] = []
        for pair_code, pair_value in record[index + 1 :]:
            if pair_code == 1001 or pair_code < 1000:
                break
            payload.append((pair_code, pair_value))
        return tuple(payload)
    return ()


def _record_sections(records: list[list[tuple[int, str]]]) -> list[str | None]:
    sections: list[str | None] = []
    current: str | None = None
    for record in records:
        record_type = _record_type(record)
        if record_type == "SECTION":
            current = (_first_value(record, 2) or "").upper() or None
            sections.append(current)
            continue
        sections.append(current)
        if record_type == "ENDSEC":
            current = None
    return sections


def _visibility_states(
    record: list[tuple[int, str]],
) -> list[tuple[str, tuple[str, ...]]]:
    states: list[tuple[str, tuple[str, ...]]] = []
    current_name: str | None = None
    current_refs: list[str] = []
    for code, value in record:
        if code == 303:
            if current_name is not None:
                states.append((current_name, tuple(current_refs)))
            current_name = value
            current_refs = []
        elif code == 332 and current_name is not None:
            current_refs.append(value.strip())
    if current_name is not None:
        states.append((current_name, tuple(current_refs)))
    return states


def _find_record_by_handle(
    records: list[list[tuple[int, str]]],
    handle: str,
) -> list[tuple[int, str]] | None:
    normalized = handle.upper()
    for record in records:
        if (_record_handle(record) or "").upper() == normalized:
            return record
    return None


def _discover_donor(
    records: list[list[tuple[int, str]]],
    sections: list[str | None],
) -> _Donor:
    candidates: list[
        tuple[int, int, list[tuple[int, str]], list[tuple[str, tuple[str, ...]]]]
    ] = []
    for index, (record, section) in enumerate(zip(records, sections)):
        if section != "OBJECTS" or _record_type(record) != "BLOCKVISIBILITYPARAMETER":
            continue
        states = _visibility_states(record)
        if len(states) != 2 or any(len(refs) != 1 for _name, refs in states):
            continue
        score = 100 if tuple(name for name, _refs in states) == _DONOR_STATE_NAMES else 10
        candidates.append((score, index, record, states))

    if not candidates:
        raise DynamicVisibilityError(
            "Het AutoCAD-sjabloon bevat geen bruikbaar tweestanden Visibility-donorblock."
        )

    _score, _index, parameter_record, states = max(candidates, key=lambda item: item[0])
    eval_handle = _record_owner(parameter_record)
    if not eval_handle:
        raise DynamicVisibilityError("Visibility-donor mist een evaluation-graph owner.")
    eval_record = _find_record_by_handle(records, eval_handle)
    if eval_record is None:
        raise DynamicVisibilityError("Evaluation graph van Visibility-donor ontbreekt.")
    xdictionary_handle = _record_owner(eval_record)
    if not xdictionary_handle:
        raise DynamicVisibilityError("Visibility-donor mist een extension dictionary.")
    xdictionary = _find_record_by_handle(records, xdictionary_handle)
    if xdictionary is None or _record_type(xdictionary) != "DICTIONARY":
        raise DynamicVisibilityError("Extension dictionary van Visibility-donor is ongeldig.")
    block_record_handle = _record_owner(xdictionary)
    if not block_record_handle:
        raise DynamicVisibilityError("Visibility-donor is niet gekoppeld aan een BLOCK_RECORD.")
    block_record = _find_record_by_handle(records, block_record_handle)
    if block_record is None or _record_type(block_record) != "BLOCK_RECORD":
        raise DynamicVisibilityError("BLOCK_RECORD van Visibility-donor ontbreekt.")
    block_rep_e_tag = _xdata_payload(block_record, "AcDbBlockRepETag")
    if not block_rep_e_tag:
        raise DynamicVisibilityError(
            "Visibility-donor mist AcDbBlockRepETag voor de initiële AutoCAD-evaluatie."
        )

    metadata_handles: set[str] = {xdictionary_handle.upper()}
    changed = True
    while changed:
        changed = False
        for record, section in zip(records, sections):
            if section != "OBJECTS":
                continue
            handle = _record_handle(record)
            owner = _record_owner(record)
            if not handle or not owner:
                continue
            if owner.upper() in metadata_handles and handle.upper() not in metadata_handles:
                metadata_handles.add(handle.upper())
                changed = True

    parameter_handle = (_record_handle(parameter_record) or "").upper()
    if parameter_handle not in metadata_handles:
        raise DynamicVisibilityError(
            "Visibility-parameter valt buiten de donor extension-dictionary."
        )

    ordered_metadata_handles = tuple(
        (_record_handle(record) or "").upper()
        for record, section in zip(records, sections)
        if section == "OBJECTS"
        and (_record_handle(record) or "").upper() in metadata_handles
    )
    state_handles = tuple(refs[0].upper() for _name, refs in states)
    return _Donor(
        block_record_handle=block_record_handle.upper(),
        xdictionary_handle=xdictionary_handle.upper(),
        metadata_handles=ordered_metadata_handles,
        state_entity_handles=(state_handles[0], state_handles[1]),
        block_rep_e_tag=block_rep_e_tag,
    )


def _max_handle_value(pairs: Iterable[tuple[int, str]]) -> int:
    maximum = 0
    for code, value in pairs:
        if code != 5:
            continue
        try:
            maximum = max(maximum, int(value.strip(), 16))
        except ValueError:
            continue
    return maximum


def _header_handseed(pairs: list[tuple[int, str]]) -> int:
    for index, (code, value) in enumerate(pairs[:-1]):
        if code == 9 and value.strip().upper() == "$HANDSEED":
            next_code, next_value = pairs[index + 1]
            if next_code == 5:
                try:
                    return int(next_value.strip(), 16)
                except ValueError:
                    return 0
    return 0


def _set_header_handseed(pairs: list[tuple[int, str]], value: int) -> None:
    for index, (code, current) in enumerate(pairs[:-1]):
        if code == 9 and current.strip().upper() == "$HANDSEED":
            next_code, _next_value = pairs[index + 1]
            if next_code == 5:
                pairs[index + 1] = (5, f"{value:X}")
                return
    raise DynamicVisibilityError("DXF-header bevat geen $HANDSEED.")


def _target_block_record(
    records: list[list[tuple[int, str]]],
    sections: list[str | None],
    block_name: str,
) -> tuple[int, list[tuple[int, str]]]:
    normalized = block_name.upper()
    for index, (record, section) in enumerate(zip(records, sections)):
        if (
            section == "TABLES"
            and _record_type(record) == "BLOCK_RECORD"
            and (_record_name(record) or "").upper() == normalized
        ):
            return index, record
    raise DynamicVisibilityError(f"BLOCK_RECORD ontbreekt voor dynamic block {block_name!r}.")


def _target_nested_insert_handles(
    records: list[list[tuple[int, str]]],
    sections: list[str | None],
    block_record_handle: str,
) -> tuple[str, str]:
    normal_handle: str | None = None
    reverse_handle: str | None = None
    normalized_owner = block_record_handle.upper()
    for record, section in zip(records, sections):
        if section != "BLOCKS" or _record_type(record) != "INSERT":
            continue
        if (_record_owner(record) or "").upper() != normalized_owner:
            continue
        name = (_record_name(record) or "").upper()
        handle = (_record_handle(record) or "").upper()
        if name.endswith("_NORMAAL_CONTENT"):
            normal_handle = handle
        elif name.endswith("_REVERSE_CONTENT"):
            reverse_handle = handle
    if not normal_handle or not reverse_handle:
        raise DynamicVisibilityError(
            "Dynamic wrapper bevat niet exact de verwachte NORMAAL/REVERSE blockrefs."
        )
    return normal_handle, reverse_handle


def _inject_dynamic_block_record_data(
    record: list[tuple[int, str]],
    *,
    xdictionary_handle: str,
    block_name: str,
    block_rep_e_tag: tuple[tuple[int, str], ...],
) -> list[tuple[int, str]]:
    result: list[tuple[int, str]] = []
    inserted_xdict = False
    index = 0
    while index < len(record):
        code, value = record[index]
        if code == 102 and value.strip().upper() == "{ACAD_XDICTIONARY":
            index += 1
            while index < len(record):
                close_code, close_value = record[index]
                index += 1
                if close_code == 102 and close_value.strip() == "}":
                    break
            continue
        if code == 1001 and value.strip() in _DYNAMIC_BLOCK_XDATA_APPS:
            index += 1
            while index < len(record):
                payload_code, _payload_value = record[index]
                if payload_code == 1001 or payload_code < 1000:
                    break
                index += 1
            continue

        result.append((code, value))
        if code == 5 and not inserted_xdict:
            result.extend(
                [
                    (102, "{ACAD_XDICTIONARY"),
                    (360, xdictionary_handle),
                    (102, "}"),
                ]
            )
            inserted_xdict = True
        index += 1

    if not inserted_xdict:
        raise DynamicVisibilityError(f"BLOCK_RECORD {block_name!r} heeft geen handle.")
    result.extend(
        [
            (1001, "AcDbBlockRepETag"),
            *block_rep_e_tag,
            (1001, "AcDbDynamicBlockTrueName"),
            (1000, block_name),
            (1001, "AcDbDynamicBlockGUID"),
            (1000, "{" + str(uuid4()).upper() + "}"),
        ]
    )
    return result


def _remap_metadata_record(
    record: list[tuple[int, str]],
    *,
    handle_map: dict[str, str],
    donor: _Donor,
    target_block_record_handle: str,
    normal_insert_handle: str,
    reverse_insert_handle: str,
) -> list[tuple[int, str]]:
    own_handle = (_record_handle(record) or "").upper()
    donor_to_target = dict(handle_map)
    donor_to_target[donor.block_record_handle] = target_block_record_handle
    donor_to_target[donor.state_entity_handles[0]] = normal_insert_handle
    donor_to_target[donor.state_entity_handles[1]] = reverse_insert_handle

    remapped: list[tuple[int, str]] = []
    state_index = 0
    is_visibility = _record_type(record) == "BLOCKVISIBILITYPARAMETER"
    for code, value in record:
        stripped = value.strip()
        if code == 5 and own_handle:
            remapped.append((5, handle_map[own_handle]))
            continue
        if code in _HANDLE_REFERENCE_CODES:
            replacement = donor_to_target.get(stripped.upper())
            if replacement:
                remapped.append((code, replacement))
                continue
        if is_visibility and code == 301:
            remapped.append((301, PROPERTY_NAME))
            continue
        if is_visibility and code == 303:
            replacement_state = NORMAL_STATE if state_index == 0 else REVERSE_STATE
            state_index += 1
            remapped.append((303, replacement_state))
            continue
        remapped.append((code, value))
    return remapped


def _objects_end_index(
    records: list[list[tuple[int, str]]],
    sections: list[str | None],
) -> int:
    for index, (record, section) in enumerate(zip(records, sections)):
        if section == "OBJECTS" and _record_type(record) == "ENDSEC":
            return index
    raise DynamicVisibilityError("DXF bevat geen volledige OBJECTS-sectie.")


def promote_dynamic_visibility_blocks(
    path: str | Path,
    block_names: Iterable[str],
) -> int:
    """Promote static two-child wrappers to native AutoCAD visibility blocks.

    The evaluation graph is cloned from a genuine AutoCAD-generated two-state
    visibility block that already exists in the project template. The donor
    records stay byte-for-byte equivalent apart from ownership/handle remapping,
    the property/state labels and the two state entity handles.
    """

    dxf_path = Path(path)
    pairs, newline, had_bom = _read_pairs(dxf_path)
    records = _split_records(pairs)
    sections = _record_sections(records)
    donor = _discover_donor(records, sections)

    donor_handle_set = set(donor.metadata_handles)
    metadata_by_handle = {
        (_record_handle(record) or "").upper(): record
        for record, section in zip(records, sections)
        if section == "OBJECTS"
        and (_record_handle(record) or "").upper() in donor_handle_set
    }
    if set(metadata_by_handle) != donor_handle_set:
        missing = sorted(donor_handle_set - set(metadata_by_handle))
        raise DynamicVisibilityError(
            f"Visibility-donor metadata is onvolledig; ontbrekende handles: {missing}"
        )

    next_handle = max(_max_handle_value(pairs) + 1, _header_handseed(pairs))
    promoted = 0
    for block_name in block_names:
        block_index, block_record = _target_block_record(records, sections, block_name)
        target_block_record_handle = (_record_handle(block_record) or "").upper()
        if not target_block_record_handle:
            raise DynamicVisibilityError(f"Dynamic block {block_name!r} heeft geen handle.")

        normal_insert, reverse_insert = _target_nested_insert_handles(
            records,
            sections,
            target_block_record_handle,
        )

        handle_map: dict[str, str] = {}
        for old_handle in donor.metadata_handles:
            handle_map[old_handle] = f"{next_handle:X}"
            next_handle += 1

        records[block_index] = _inject_dynamic_block_record_data(
            block_record,
            xdictionary_handle=handle_map[donor.xdictionary_handle],
            block_name=block_name,
            block_rep_e_tag=donor.block_rep_e_tag,
        )

        cloned_records = [
            _remap_metadata_record(
                metadata_by_handle[old_handle],
                handle_map=handle_map,
                donor=donor,
                target_block_record_handle=target_block_record_handle,
                normal_insert_handle=normal_insert,
                reverse_insert_handle=reverse_insert,
            )
            for old_handle in donor.metadata_handles
        ]
        insert_at = _objects_end_index(records, sections)
        records[insert_at:insert_at] = cloned_records
        sections = _record_sections(records)
        promoted += 1

    flattened = _flatten_records(records)
    _set_header_handseed(flattened, next_handle)
    _write_pairs(dxf_path, flattened, newline=newline, had_bom=had_bom)
    return promoted


def inspect_dynamic_visibility_block(
    path: str | Path,
    block_name: str,
) -> dict[str, object]:
    """Return structural details for regression/release validation."""

    pairs, _newline, _had_bom = _read_pairs(Path(path))
    records = _split_records(pairs)
    sections = _record_sections(records)
    _index, block_record = _target_block_record(records, sections, block_name)
    block_handle = (_record_handle(block_record) or "").upper()
    block_rep_e_tag = tuple(
        (code, value.strip())
        for code, value in _xdata_payload(block_record, "AcDbBlockRepETag")
    )
    xdict_handle: str | None = None
    for index, (code, value) in enumerate(block_record[:-1]):
        if code == 102 and value.strip().upper() == "{ACAD_XDICTIONARY":
            next_code, next_value = block_record[index + 1]
            if next_code == 360:
                xdict_handle = next_value.strip().upper()
                break
    if not xdict_handle:
        return {
            "block_name": block_name,
            "is_dynamic": False,
            "property_name": None,
            "states": (),
            "block_rep_e_tag": block_rep_e_tag,
        }

    metadata_handles = {xdict_handle}
    changed = True
    while changed:
        changed = False
        for record, section in zip(records, sections):
            if section != "OBJECTS":
                continue
            handle = (_record_handle(record) or "").upper()
            owner = (_record_owner(record) or "").upper()
            if handle and owner in metadata_handles and handle not in metadata_handles:
                metadata_handles.add(handle)
                changed = True

    visibility = next(
        (
            record
            for record, section in zip(records, sections)
            if section == "OBJECTS"
            and (_record_handle(record) or "").upper() in metadata_handles
            and _record_type(record) == "BLOCKVISIBILITYPARAMETER"
        ),
        None,
    )
    if visibility is None:
        return {
            "block_name": block_name,
            "block_handle": block_handle,
            "is_dynamic": False,
            "property_name": None,
            "states": (),
            "block_rep_e_tag": block_rep_e_tag,
        }

    states = tuple(name for name, _refs in _visibility_states(visibility))
    return {
        "block_name": block_name,
        "block_handle": block_handle,
        "is_dynamic": True,
        "property_name": _first_value(visibility, 301),
        "states": states,
        "default_state": states[0] if states else None,
        "block_rep_e_tag": block_rep_e_tag,
    }
