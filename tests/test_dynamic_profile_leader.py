from __future__ import annotations

from pathlib import Path
import tempfile
import unittest

import ezdxf

from SleufBase import autocad_dynamic_visibility as dv
from SleufBase.autocad_synthetic_polar_leader import (
    BASE_POINT,
    BLOCK_NAME,
    GRIP_NODE,
    LAYER_NAME,
    MOVE_NODE,
    PARAMETER_NODE,
    STRETCH_NODE,
    TOP_POINT,
    inspect_synthetic_polar_leader,
    promote_synthetic_polar_leader,
)
from SleufBase.cadastral_export import CadastralDxfExporter
from SleufBase import template_dynamic_profile_leader_patch as leader_patch


ASSET = Path(__file__).resolve().parents[1] / "assets" / "cadastral_template.dxf"
OLD_DONOR = CadastralDxfExporter.TEMPLATE_PROFILE_LEGACY_DYNAMIC_LEADER_BLOCK_NAME


def _raw_records(path: Path):
    pairs, _newline, _bom = dv._read_pairs(path)
    records = dv._split_records(pairs)
    sections = dv._record_sections(records)
    return pairs, records, sections


def _metadata_records_for_block(path: Path, block_name: str):
    _pairs, records, sections = _raw_records(path)
    _index, block_record = dv._target_block_record(records, sections, block_name)
    block_handle = (dv._record_handle(block_record) or "").upper()

    xdict_handle = None
    for index, (code, value) in enumerate(block_record[:-1]):
        if code == 102 and value.strip().upper() == "{ACAD_XDICTIONARY":
            next_code, next_value = block_record[index + 1]
            if next_code == 360:
                xdict_handle = next_value.strip().upper()
                break
    if not xdict_handle:
        raise AssertionError(f"{block_name} mist ACAD_XDICTIONARY")

    metadata_handles = {xdict_handle}
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

    metadata = [
        record
        for record, section in zip(records, sections)
        if section == "OBJECTS"
        and (dv._record_handle(record) or "").upper() in metadata_handles
    ]
    block_entities = [
        record
        for record, section in zip(records, sections)
        if section == "BLOCKS" and (dv._record_owner(record) or "").upper() == block_handle
    ]
    return metadata, block_entities


def _action_dependency_refs(record: list[tuple[int, str]]) -> tuple[str, ...]:
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


class DynamicProfileLeaderTests(unittest.TestCase):
    def test_synthetic_polar_metadata_is_built_from_our_own_values(self) -> None:
        document = ezdxf.readfile(ASSET)
        exporter = CadastralDxfExporter.__new__(CadastralDxfExporter)
        exporter._remove_template_legacy_profile_leader_blocks(document)

        self.assertIn(BLOCK_NAME, document.blocks)
        self.assertNotEqual(BLOCK_NAME, OLD_DONOR)

        with tempfile.TemporaryDirectory() as temp_dir:
            output = Path(temp_dir) / "sleufbase-owned-polar.dxf"
            document.saveas(output)
            promote_synthetic_polar_leader(output, BLOCK_NAME)
            reopened = ezdxf.readfile(output)
            self.assertIn(BLOCK_NAME, reopened.blocks)

            details = inspect_synthetic_polar_leader(output, BLOCK_NAME)
            self.assertTrue(details["is_dynamic"])
            self.assertEqual(details["parameter_count"], 1)
            self.assertEqual(details["grip_count"], 1)
            self.assertEqual(details["grip_component_count"], 2)
            self.assertEqual(details["stretch_count"], 1)
            self.assertEqual(details["move_count"], 1)
            self.assertEqual(details["parameter_base"], BASE_POINT)
            self.assertEqual(details["parameter_top"], TOP_POINT)
            self.assertEqual(details["grip_top"], TOP_POINT)
            self.assertEqual(details["parameter_labels"], ("Lengte", "Hoek"))

            metadata, block_entities = _metadata_records_for_block(output, BLOCK_NAME)
            metadata_by_type = {dv._record_type(record): record for record in metadata}
            polar = metadata_by_type["BLOCKPOLARPARAMETER"]
            grip = metadata_by_type["BLOCKPOLARGRIP"]
            stretch = metadata_by_type["BLOCKSTRETCHACTION"]
            move = metadata_by_type["BLOCKMOVEACTION"]

            self.assertEqual(next(int(value) for code, value in polar if code == 90), PARAMETER_NODE)
            self.assertEqual(next(int(value) for code, value in grip if code == 90), GRIP_NODE)
            self.assertEqual(next(int(value) for code, value in stretch if code == 90), STRETCH_NODE)
            self.assertEqual(next(int(value) for code, value in move if code == 90), MOVE_NODE)

            polyline = next(record for record in block_entities if dv._record_type(record) == "LWPOLYLINE")
            attdefs = [record for record in block_entities if dv._record_type(record) == "ATTDEF"]
            self.assertEqual(len(attdefs), 2)
            polyline_handle = (dv._record_handle(polyline) or "").upper()
            attdef_handles = {(dv._record_handle(record) or "").upper() for record in attdefs}
            stretch_refs = _action_dependency_refs(stretch)
            move_refs = set(_action_dependency_refs(move))
            self.assertEqual(stretch_refs, (polyline_handle,))
            self.assertEqual(move_refs, attdef_handles)

            # The synthetic parameter/action records must not reference the old
            # example block name. The only accepted source is our new block.
            metadata_text = "\n".join(value for record in metadata for _code, value in record)
            self.assertNotIn(OLD_DONOR, metadata_text)
            self.assertIn("SleufBase bovenkant", metadata_text)
            self.assertIn("Bovenkant grip", metadata_text)

    def test_exporter_removes_example_and_inserts_our_own_leader(self) -> None:
        document = ezdxf.readfile(ASSET)
        exporter = CadastralDxfExporter.__new__(CadastralDxfExporter)
        exporter._remove_template_legacy_profile_leader_blocks(document)
        self.assertIn(BLOCK_NAME, document.blocks)
        self.assertEqual(leader_patch.DYNAMIC_LEADER_BLOCK_NAME, BLOCK_NAME)
        self.assertEqual(leader_patch.DYNAMIC_LEADER_LAYER, LAYER_NAME)

        modelspace = document.modelspace()
        before_leaders = len(list(modelspace.query("LEADER")))
        exporter._add_template_profile_multileader(
            modelspace,
            description="Datakabel",
            depth_text="Diepte: 0.85",
            leader_start=(10.0, 20.02),
            text_insert=(10.0, 20.20),
            marker_scale=0.02,
            color=3,
        )
        after_leaders = len(list(modelspace.query("LEADER")))
        self.assertEqual(before_leaders, after_leaders)

        refs = [
            entity
            for entity in modelspace.query("INSERT")
            if str(entity.dxf.name) == BLOCK_NAME
        ]
        self.assertTrue(refs)
        block_ref = refs[-1]
        self.assertAlmostEqual(float(block_ref.dxf.insert.x), 10.0)
        self.assertAlmostEqual(float(block_ref.dxf.insert.y), 20.02)
        self.assertAlmostEqual(float(block_ref.dxf.xscale), 0.02)
        self.assertAlmostEqual(float(block_ref.dxf.yscale), 0.02)
        self.assertAlmostEqual(float(block_ref.dxf.rotation), 0.0)
        self.assertEqual(str(block_ref.dxf.layer), LAYER_NAME)

        values = {str(attribute.dxf.tag): str(attribute.dxf.text) for attribute in block_ref.attribs}
        self.assertEqual(values["OMSCHRIJVING"], "Datakabel")
        self.assertEqual(values["HOOGTE"], "Diepte: 0.85")

    def test_old_standalone_marker_is_skipped_for_synthetic_dynamic_leader(self) -> None:
        document = ezdxf.readfile(ASSET)
        exporter = CadastralDxfExporter.__new__(CadastralDxfExporter)
        exporter._remove_template_legacy_profile_leader_blocks(document)
        modelspace = document.modelspace()
        before = len(list(modelspace))
        exporter._add_template_profile_leader_marker(
            modelspace,
            insert_x=1.0,
            insert_y=2.0,
            marker_scale=0.02,
            layer_name="TEST",
            color=3,
        )
        self.assertEqual(len(list(modelspace)), before)


if __name__ == "__main__":
    unittest.main()
