from __future__ import annotations

from math import pi
from pathlib import Path
from typing import Iterable
from uuid import uuid4

from . import autocad_dynamic_visibility as raw_dxf


BLOCK_NAME = "SLEUFBASE_VERWIJZING_POLAR_BOVENKANT"
LAYER_NAME = "X-XX-AL-VERWIJZING-SD"
BASE_POINT = (0.0, 0.0, 0.0)
TOP_POINT = (0.0, 12.5, 0.0)
MARKER_RADIUS = 1.4
TEXT_X = 1.75
DESCRIPTION_Y = 12.7
DEPTH_Y = 10.75
TEXT_HEIGHT = 1.45

# These evaluation IDs are SleufBase-owned values. They intentionally do not
# match any parameter/action IDs in the cadastral template.
PARAMETER_NODE = 101
GRIP_NODE = 102
GRIP_X_NODE = 103
GRIP_Y_NODE = 104
STRETCH_NODE = 105
MOVE_NODE = 106

_DYNAMIC_XDATA_APPS = frozenset(
    {
        "AcDbBlockRepETag",
        "AcDbDynamicBlockTrueName",
        "AcDbDynamicBlockGUID",
    }
)


class SyntheticPolarLeaderError(RuntimeError):
    pass


def ensure_synthetic_polar_leader_block(document, block_name: str = BLOCK_NAME) -> str:
    """Create the static geometry that our own Dynamic Block metadata will drive.

    The geometry is deliberately SleufBase-defined rather than derived from a
    template example: base (0, 0), top (0, 12.5), our own marker size and our own
    attribute positions. Raw AutoCAD parameter/action objects are attached only
    after the complete export has been assembled.
    """

    try:
        existing = document.blocks.get(block_name)
    except Exception:
        existing = None
    if existing is not None:
        return block_name

    block = document.blocks.new(name=block_name, base_point=BASE_POINT)
    block.add_circle(
        center=(BASE_POINT[0], BASE_POINT[1]),
        radius=MARKER_RADIUS,
        dxfattribs={"layer": "0", "color": 0, "lineweight": 0},
    )
    block.add_lwpolyline(
        [(BASE_POINT[0], BASE_POINT[1]), (TOP_POINT[0], TOP_POINT[1])],
        dxfattribs={"layer": "0", "color": 0, "lineweight": 0},
    )
    block.add_attdef(
        "OMSCHRIJVING",
        insert=(TEXT_X, DESCRIPTION_Y),
        text="",
        dxfattribs={
            "height": TEXT_HEIGHT,
            "rotation": 0.0,
            "layer": "0",
            "color": 0,
            "lock_position": 0,
        },
    )
    block.add_attdef(
        "HOOGTE",
        insert=(TEXT_X, DEPTH_Y),
        text="",
        dxfattribs={
            "height": TEXT_HEIGHT,
            "rotation": 0.0,
            "layer": "0",
            "color": 0,
            "lock_position": 0,
        },
    )
    return block_name


def _replace_dynamic_block_record_data(
    record: list[tuple[int, str]],
    *,
    xdictionary_handle: str,
    block_name: str,
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
        if code == 1001 and value.strip() in _DYNAMIC_XDATA_APPS:
            index += 1
            while index < len(record):
                payload_code, _payload_value = record[index]
                if payload_code == 1001 or payload_code < 1000:
                    break
                index += 1
            continue

        if code == 281:
            result.append((281, "1"))
        else:
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
        raise SyntheticPolarLeaderError(f"BLOCK_RECORD {block_name!r} heeft geen handle.")

    result.extend(
        [
            (1001, "AcDbBlockRepETag"),
            (1070, "1"),
            (1071, "9137"),
            (1001, "AcDbDynamicBlockTrueName"),
            (1000, block_name),
            (1001, "AcDbDynamicBlockGUID"),
            (1000, "{" + str(uuid4()).upper() + "}"),
        ]
    )
    return result


def _target_geometry_handles(
    records: list[list[tuple[int, str]]],
    sections: list[str | None],
    block_record_handle: str,
) -> tuple[str, str, str]:
    owner = block_record_handle.upper()
    leader_handle: str | None = None
    description_handle: str | None = None
    depth_handle: str | None = None

    for record, section in zip(records, sections):
        if section != "BLOCKS" or (raw_dxf._record_owner(record) or "").upper() != owner:
            continue
        record_type = raw_dxf._record_type(record)
        handle = (raw_dxf._record_handle(record) or "").upper()
        if not handle:
            continue
        if record_type == "LWPOLYLINE" and leader_handle is None:
            leader_handle = handle
        elif record_type == "ATTDEF":
            tag = (raw_dxf._first_value(record, 2) or "").upper()
            if tag == "OMSCHRIJVING":
                description_handle = handle
            elif tag == "HOOGTE":
                depth_handle = handle

    if not leader_handle or not description_handle or not depth_handle:
        raise SyntheticPolarLeaderError(
            "Het SleufBase Polar-blok mist de eigen lijn of OMSCHRIJVING/HOOGTE-attributen."
        )
    return leader_handle, description_handle, depth_handle


def _extension_dictionary_record(
    handle: str,
    block_record_handle: str,
    eval_handle: str,
    sort_handle: str,
    purge_handle: str,
) -> list[tuple[int, str]]:
    return [
        (0, "DICTIONARY"),
        (5, handle),
        (330, block_record_handle),
        (100, "AcDbDictionary"),
        (280, "1"),
        (281, "1"),
        (3, "ACAD_ENHANCEDBLOCK"),
        (360, eval_handle),
        (3, "ACAD_SORTENTS"),
        (360, sort_handle),
        (3, "AcDbDynamicBlockRoundTripPurgePreventer"),
        (360, purge_handle),
    ]


def _node_pairs(index: int, eval_node: int, object_handle: str, adjacency: tuple[int, int, int, int]) -> list[tuple[int, str]]:
    return [
        (91, str(index)),
        (93, "32"),
        (95, str(eval_node)),
        (360, object_handle),
        *((92, str(value)) for value in adjacency),
    ]


def _edge_pairs(
    edge_index: int,
    edge_type: int,
    source_index: int,
    target_index: int,
    links: tuple[int, int, int, int, int],
) -> list[tuple[int, str]]:
    return [
        (92, str(edge_index)),
        (93, "0"),
        (94, str(edge_type)),
        (91, str(source_index)),
        (91, str(target_index)),
        *((92, str(value)) for value in links),
    ]


def _evaluation_graph_record(
    handle: str,
    xdict_handle: str,
    *,
    parameter_handle: str,
    grip_handle: str,
    grip_x_handle: str,
    grip_y_handle: str,
    stretch_handle: str,
    move_handle: str,
) -> list[tuple[int, str]]:
    record: list[tuple[int, str]] = [
        (0, "ACAD_EVALUATION_GRAPH"),
        (5, handle),
        (102, "{ACAD_REACTORS"),
        (330, xdict_handle),
        (102, "}"),
        (330, xdict_handle),
        (100, "AcDbEvalGraph"),
        (96, "6"),
        (97, "6"),
    ]
    # Own graph topology: grip -> parameter; parameter -> X/Y expressions,
    # stretch action and move action. None of these IDs/edges are copied from
    # the cadastral template donor.
    record.extend(_node_pairs(0, PARAMETER_NODE, parameter_handle, (2, 2, 0, 4)))
    record.extend(_node_pairs(1, GRIP_NODE, grip_handle, (-1, -1, 2, 2)))
    record.extend(_node_pairs(2, GRIP_X_NODE, grip_x_handle, (0, 0, -1, -1)))
    record.extend(_node_pairs(3, GRIP_Y_NODE, grip_y_handle, (1, 1, -1, -1)))
    record.extend(_node_pairs(4, STRETCH_NODE, stretch_handle, (3, 3, -1, -1)))
    record.extend(_node_pairs(5, MOVE_NODE, move_handle, (4, 4, -1, -1)))

    record.extend(_edge_pairs(0, 1, 0, 2, (-1, -1, -1, 1, -1)))
    record.extend(_edge_pairs(1, 1, 0, 3, (-1, -1, 0, 3, -1)))
    record.extend(_edge_pairs(2, 2, 1, 0, (-1, -1, -1, -1, -1)))
    record.extend(_edge_pairs(3, 2, 0, 4, (-1, -1, 1, 4, -1)))
    record.extend(_edge_pairs(4, 2, 0, 5, (-1, -1, 3, -1, -1)))
    return record


def _parameter_record(handle: str, eval_handle: str) -> list[tuple[int, str]]:
    return [
        (0, "BLOCKPOLARPARAMETER"),
        (5, handle),
        (330, eval_handle),
        (100, "AcDbEvalExpr"),
        (90, str(PARAMETER_NODE)),
        (98, "33"),
        (99, "378"),
        (100, "AcDbBlockElement"),
        (300, "SleufBase bovenkant"),
        (98, "33"),
        (99, "378"),
        (1071, "0"),
        (100, "AcDbBlockParameter"),
        (280, "1"),
        (281, "0"),
        (100, "AcDbBlock2PtParameter"),
        (1010, str(BASE_POINT[0])),
        (1020, str(BASE_POINT[1])),
        (1030, str(BASE_POINT[2])),
        (1011, str(TOP_POINT[0])),
        (1021, str(TOP_POINT[1])),
        (1031, str(TOP_POINT[2])),
        (170, "4"),
        (91, "0"),
        (91, str(GRIP_NODE)),
        (91, "0"),
        (91, "0"),
        (171, "0"),
        (172, "0"),
        (173, "1"),
        (94, str(GRIP_NODE)),
        (303, "DisplacementX"),
        (174, "1"),
        (95, str(GRIP_NODE)),
        (304, "DisplacementY"),
        (177, "0"),
        (100, "AcDbBlockPolarParameter"),
        (305, "Lengte"),
        (306, ""),
        (307, "Hoek"),
        (308, ""),
        (140, "0.0"),
        (309, ""),
        # Fine increments keep the top essentially free while still giving
        # AutoCAD an explicit distance value-set.
        (96, "1"),
        (141, "0.05"),
        (142, "0.0"),
        (143, "0.0"),
        (175, "0"),
        (410, ""),
        (97, "3"),
        (145, "0.0"),
        (146, str(2.0 * pi)),
        (147, "0.0"),
        (176, "0"),
        (1001, "ACAUTHENVIRON"),
        (1010, "2.75"),
        (1020, "6.25"),
        (1030, "0.0"),
    ]


def _grip_record(handle: str, eval_handle: str) -> list[tuple[int, str]]:
    return [
        (0, "BLOCKPOLARGRIP"),
        (5, handle),
        (330, eval_handle),
        (100, "AcDbEvalExpr"),
        (90, str(GRIP_NODE)),
        (98, "33"),
        (99, "378"),
        (100, "AcDbBlockElement"),
        (300, "Bovenkant grip"),
        (98, "33"),
        (99, "378"),
        (1071, "0"),
        (100, "AcDbBlockGrip"),
        (91, str(GRIP_X_NODE)),
        (92, str(GRIP_Y_NODE)),
        (1010, str(TOP_POINT[0])),
        (1020, str(TOP_POINT[1])),
        (1030, str(TOP_POINT[2])),
        (280, "1"),
        (93, "-1"),
        (100, "AcDbBlockPolarGrip"),
    ]


def _grip_component_record(handle: str, eval_handle: str, node: int, expression: str) -> list[tuple[int, str]]:
    return [
        (0, "BLOCKGRIPLOCATIONCOMPONENT"),
        (5, handle),
        (330, eval_handle),
        (100, "AcDbEvalExpr"),
        (90, str(node)),
        (98, "33"),
        (99, "378"),
        (1, ""),
        (70, "40"),
        (140, "1.797693134862313E+99"),
        (100, "AcDbBlockGripExpr"),
        (91, str(PARAMETER_NODE)),
        (300, expression),
    ]


def _stretch_record(handle: str, eval_handle: str, leader_handle: str) -> list[tuple[int, str]]:
    return [
        (0, "BLOCKSTRETCHACTION"),
        (5, handle),
        (330, eval_handle),
        (100, "AcDbEvalExpr"),
        (90, str(STRETCH_NODE)),
        (98, "33"),
        (99, "378"),
        (100, "AcDbBlockElement"),
        (300, "Strek verwijzingslijn"),
        (98, "33"),
        (99, "378"),
        (1071, "0"),
        (100, "AcDbBlockAction"),
        (70, "0"),
        (71, "1"),
        (330, leader_handle),
        (1010, "2.4"),
        (1020, "11.2"),
        (1030, "0.0"),
        (100, "AcDbBlockStretchAction"),
        (92, str(PARAMETER_NODE)),
        (301, "EndXDelta"),
        (93, str(PARAMETER_NODE)),
        (302, "EndYDelta"),
        (72, "2"),
        (1011, "-2.2"),
        (1021, "10.1"),
        (1011, "2.2"),
        (1021, "14.9"),
        (73, "1"),
        (331, leader_handle),
        (74, "1"),
        (94, "1"),
        (75, "0"),
        (140, "1.0"),
        (141, "0.0"),
        (280, "0"),
    ]


def _move_record(
    handle: str,
    eval_handle: str,
    description_handle: str,
    depth_handle: str,
) -> list[tuple[int, str]]:
    return [
        (0, "BLOCKMOVEACTION"),
        (5, handle),
        (330, eval_handle),
        (100, "AcDbEvalExpr"),
        (90, str(MOVE_NODE)),
        (98, "33"),
        (99, "378"),
        (100, "AcDbBlockElement"),
        (300, "Verplaats teksten"),
        (98, "33"),
        (99, "378"),
        (1071, "0"),
        (100, "AcDbBlockAction"),
        (70, "0"),
        (71, "2"),
        (330, description_handle),
        (330, depth_handle),
        (1010, "3.6"),
        (1020, "12.0"),
        (1030, "0.0"),
        (100, "AcDbBlockMoveAction"),
        (92, str(PARAMETER_NODE)),
        (301, "EndXDelta"),
        (93, str(PARAMETER_NODE)),
        (302, "EndYDelta"),
        (140, "1.0"),
        (141, "0.0"),
        (280, "0"),
    ]


def _sortents_record(handle: str, xdict_handle: str, block_record_handle: str) -> list[tuple[int, str]]:
    return [
        (0, "SORTENTSTABLE"),
        (5, handle),
        (102, "{ACAD_REACTORS"),
        (330, xdict_handle),
        (102, "}"),
        (330, xdict_handle),
        (100, "AcDbSortentsTable"),
        (330, block_record_handle),
    ]


def _purge_record(handle: str, xdict_handle: str) -> list[tuple[int, str]]:
    return [
        (0, "ACDB_DYNAMICBLOCKPURGEPREVENTER_VERSION"),
        (5, handle),
        (102, "{ACAD_REACTORS"),
        (330, xdict_handle),
        (102, "}"),
        (330, xdict_handle),
        (100, "AcDbDynamicBlockPurgePreventer"),
        (70, "1"),
    ]


def promote_synthetic_polar_leader(path: str | Path, block_name: str = BLOCK_NAME) -> bool:
    """Attach a completely SleufBase-authored native Polar grip/action graph."""

    dxf_path = Path(path)
    pairs, newline, had_bom = raw_dxf._read_pairs(dxf_path)
    records = raw_dxf._split_records(pairs)
    sections = raw_dxf._record_sections(records)
    block_index, block_record = raw_dxf._target_block_record(records, sections, block_name)
    block_record_handle = (raw_dxf._record_handle(block_record) or "").upper()
    if not block_record_handle:
        raise SyntheticPolarLeaderError(f"BLOCK_RECORD ontbreekt voor {block_name!r}.")

    leader_handle, description_handle, depth_handle = _target_geometry_handles(
        records, sections, block_record_handle
    )

    next_handle = max(raw_dxf._max_handle_value(pairs) + 1, raw_dxf._header_handseed(pairs))

    def allocate() -> str:
        nonlocal next_handle
        value = f"{next_handle:X}"
        next_handle += 1
        return value

    xdict = allocate()
    eval_graph = allocate()
    sortents = allocate()
    purge = allocate()
    parameter = allocate()
    grip = allocate()
    grip_x = allocate()
    grip_y = allocate()
    stretch = allocate()
    move = allocate()

    records[block_index] = _replace_dynamic_block_record_data(
        block_record,
        xdictionary_handle=xdict,
        block_name=block_name,
    )

    new_records = [
        _extension_dictionary_record(xdict, block_record_handle, eval_graph, sortents, purge),
        _evaluation_graph_record(
            eval_graph,
            xdict,
            parameter_handle=parameter,
            grip_handle=grip,
            grip_x_handle=grip_x,
            grip_y_handle=grip_y,
            stretch_handle=stretch,
            move_handle=move,
        ),
        _parameter_record(parameter, eval_graph),
        _grip_record(grip, eval_graph),
        _grip_component_record(grip_x, eval_graph, GRIP_X_NODE, "UpdatedEndX"),
        _grip_component_record(grip_y, eval_graph, GRIP_Y_NODE, "UpdatedEndY"),
        _stretch_record(stretch, eval_graph, leader_handle),
        _move_record(move, eval_graph, description_handle, depth_handle),
        _sortents_record(sortents, xdict, block_record_handle),
        _purge_record(purge, xdict),
    ]
    insert_at = raw_dxf._objects_end_index(records, sections)
    records[insert_at:insert_at] = new_records

    flattened = raw_dxf._flatten_records(records)
    raw_dxf._set_header_handseed(flattened, next_handle)
    raw_dxf._write_pairs(dxf_path, flattened, newline=newline, had_bom=had_bom)
    return True


def _point_from_record(record: list[tuple[int, str]], codes: tuple[int, int, int]) -> tuple[float, float, float] | None:
    values: list[float] = []
    for code in codes:
        raw = raw_dxf._first_value(record, code)
        if raw is None:
            return None
        try:
            values.append(float(raw))
        except ValueError:
            return None
    return values[0], values[1], values[2]


def inspect_synthetic_polar_leader(path: str | Path, block_name: str = BLOCK_NAME) -> dict[str, object]:
    pairs, _newline, _had_bom = raw_dxf._read_pairs(Path(path))
    records = raw_dxf._split_records(pairs)
    sections = raw_dxf._record_sections(records)
    _block_index, block_record = raw_dxf._target_block_record(records, sections, block_name)
    block_handle = (raw_dxf._record_handle(block_record) or "").upper()

    xdict: str | None = None
    for index, (code, value) in enumerate(block_record[:-1]):
        if code == 102 and value.strip().upper() == "{ACAD_XDICTIONARY":
            next_code, next_value = block_record[index + 1]
            if next_code == 360:
                xdict = next_value.strip().upper()
                break

    if not xdict:
        return {"block_name": block_name, "is_dynamic": False}

    metadata_handles = {xdict}
    changed = True
    while changed:
        changed = False
        for record, section in zip(records, sections):
            if section != "OBJECTS":
                continue
            handle = (raw_dxf._record_handle(record) or "").upper()
            owner = (raw_dxf._record_owner(record) or "").upper()
            if handle and owner in metadata_handles and handle not in metadata_handles:
                metadata_handles.add(handle)
                changed = True

    metadata = [
        record
        for record, section in zip(records, sections)
        if section == "OBJECTS"
        and (raw_dxf._record_handle(record) or "").upper() in metadata_handles
    ]
    by_type: dict[str, list[list[tuple[int, str]]]] = {}
    for record in metadata:
        by_type.setdefault(raw_dxf._record_type(record), []).append(record)

    parameter = (by_type.get("BLOCKPOLARPARAMETER") or [None])[0]
    grip = (by_type.get("BLOCKPOLARGRIP") or [None])[0]
    stretch = (by_type.get("BLOCKSTRETCHACTION") or [None])[0]
    move = (by_type.get("BLOCKMOVEACTION") or [None])[0]

    leader_handle, description_handle, depth_handle = _target_geometry_handles(
        records, sections, block_handle
    )

    parameter_labels: tuple[str, ...] = ()
    if parameter is not None:
        labels = [value for code, value in parameter if code in (305, 307)]
        parameter_labels = tuple(labels)

    def refs(record: list[tuple[int, str]] | None, code: int) -> tuple[str, ...]:
        if record is None:
            return ()
        return tuple(value.strip().upper() for pair_code, value in record if pair_code == code)

    return {
        "block_name": block_name,
        "is_dynamic": bool(parameter and grip and stretch and move),
        "block_record_handle": block_handle,
        "parameter_count": len(by_type.get("BLOCKPOLARPARAMETER", [])),
        "grip_count": len(by_type.get("BLOCKPOLARGRIP", [])),
        "grip_component_count": len(by_type.get("BLOCKGRIPLOCATIONCOMPONENT", [])),
        "stretch_count": len(by_type.get("BLOCKSTRETCHACTION", [])),
        "move_count": len(by_type.get("BLOCKMOVEACTION", [])),
        "parameter_base": _point_from_record(parameter, (1010, 1020, 1030)) if parameter else None,
        "parameter_top": _point_from_record(parameter, (1011, 1021, 1031)) if parameter else None,
        "grip_top": _point_from_record(grip, (1010, 1020, 1030)) if grip else None,
        "parameter_labels": parameter_labels,
        "leader_handle": leader_handle,
        "description_handle": description_handle,
        "depth_handle": depth_handle,
        "stretch_entity_refs": refs(stretch, 331),
        "move_entity_refs": refs(move, 330),
        "metadata_handles": tuple(sorted(metadata_handles)),
    }
