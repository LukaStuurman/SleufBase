from __future__ import annotations

from pathlib import Path
import tempfile
import unittest

import ezdxf

from SleufBase import autocad_dynamic_visibility as dv
from SleufBase.cadastral_export import CadastralDxfExporter
from SleufBase import template_dynamic_profile_leader_patch as leader_patch


ASSET = Path(__file__).resolve().parents[1] / "assets" / "cadastral_template.dxf"


def _metadata_records_for_block(path: Path, block_name: str):
    pairs, _newline, _bom = dv._read_pairs(path)
    records = dv._split_records(pairs)
    sections = dv._record_sections(records)
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


class DynamicProfileLeaderTests(unittest.TestCase):
    def test_template_contains_native_polar_stretch_move_donor(self) -> None:
        metadata, block_entities = _metadata_records_for_block(
            ASSET,
            leader_patch.DYNAMIC_LEADER_BLOCK_NAME,
        )
        metadata_by_type = {dv._record_type(record): record for record in metadata}
        self.assertIn("BLOCKPOLARPARAMETER", metadata_by_type)
        self.assertIn("BLOCKPOLARGRIP", metadata_by_type)
        self.assertIn("BLOCKSTRETCHACTION", metadata_by_type)
        self.assertIn("BLOCKMOVEACTION", metadata_by_type)

        polyline = next(record for record in block_entities if dv._record_type(record) == "LWPOLYLINE")
        attdefs = [record for record in block_entities if dv._record_type(record) == "ATTDEF"]
        self.assertEqual(len(attdefs), 2)
        polyline_handle = (dv._record_handle(polyline) or "").upper()
        attdef_handles = {(dv._record_handle(record) or "").upper() for record in attdefs}

        stretch_refs = {
            value.strip().upper()
            for code, value in metadata_by_type["BLOCKSTRETCHACTION"]
            if code in {330, 331}
        }
        move_refs = {
            value.strip().upper()
            for code, value in metadata_by_type["BLOCKMOVEACTION"]
            if code == 330
        }
        self.assertIn(polyline_handle, stretch_refs)
        self.assertTrue(attdef_handles.issubset(move_refs))

        polar = metadata_by_type["BLOCKPOLARPARAMETER"]
        base_x = next(float(value) for code, value in polar if code == 1010)
        base_y = next(float(value) for code, value in polar if code == 1020)
        end_x = next(float(value) for code, value in polar if code == 1011)
        end_y = next(float(value) for code, value in polar if code == 1021)
        self.assertAlmostEqual(base_x, 0.0)
        self.assertAlmostEqual(base_y, 0.0)
        self.assertAlmostEqual(end_x, 0.0)
        self.assertAlmostEqual(end_y, 10.0)

    def test_exporter_preserves_and_inserts_dynamic_leader(self) -> None:
        document = ezdxf.readfile(ASSET)
        exporter = CadastralDxfExporter.__new__(CadastralDxfExporter)
        exporter._remove_template_legacy_profile_leader_blocks(document)
        self.assertIn(leader_patch.DYNAMIC_LEADER_BLOCK_NAME, document.blocks)

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
            if str(entity.dxf.name) == leader_patch.DYNAMIC_LEADER_BLOCK_NAME
        ]
        self.assertTrue(refs)
        block_ref = refs[-1]
        self.assertAlmostEqual(float(block_ref.dxf.insert.x), 10.0)
        self.assertAlmostEqual(float(block_ref.dxf.insert.y), 20.0)
        self.assertAlmostEqual(float(block_ref.dxf.xscale), 0.02)
        self.assertAlmostEqual(float(block_ref.dxf.yscale), 0.02)
        self.assertAlmostEqual(float(block_ref.dxf.rotation), 0.0)
        self.assertEqual(str(block_ref.dxf.layer), leader_patch.DYNAMIC_LEADER_LAYER)

        values = {str(attribute.dxf.tag): str(attribute.dxf.text) for attribute in block_ref.attribs}
        self.assertEqual(values["OMSCHRIJVING"], "Datakabel")
        self.assertEqual(values["HOOGTE"], "Diepte: 0.85")

        with tempfile.TemporaryDirectory() as temp_dir:
            output = Path(temp_dir) / "dynamic-leader.dxf"
            document.saveas(output)
            reopened = ezdxf.readfile(output)
            self.assertIn(leader_patch.DYNAMIC_LEADER_BLOCK_NAME, reopened.blocks)
            metadata, _entities = _metadata_records_for_block(
                output,
                leader_patch.DYNAMIC_LEADER_BLOCK_NAME,
            )
            types = {dv._record_type(record) for record in metadata}
            self.assertIn("BLOCKPOLARPARAMETER", types)
            self.assertIn("BLOCKSTRETCHACTION", types)
            self.assertIn("BLOCKMOVEACTION", types)

    def test_old_standalone_marker_is_skipped_when_dynamic_donor_is_available(self) -> None:
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
