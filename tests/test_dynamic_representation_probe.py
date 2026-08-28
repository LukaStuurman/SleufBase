from __future__ import annotations

from pathlib import Path
import unittest

from SleufBase import autocad_dynamic_visibility as dv


class DynamicRepresentationProbe(unittest.TestCase):
    def test_probe_real_template_dynamic_representation(self) -> None:
        path = Path(__file__).resolve().parents[1] / "assets" / "cadastral_template.dxf"
        pairs, _newline, _bom = dv._read_pairs(path)
        records = dv._split_records(pairs)
        sections = dv._record_sections(records)
        donor = dv._discover_donor(records, sections)

        donor_record = dv._find_record_by_handle(records, donor.block_record_handle)
        donor_name = dv._record_name(donor_record or [])
        print("DONOR", donor_name, donor.block_record_handle)
        print("DONOR_REP_E", donor.block_rep_e_tag)

        print("STATE_ENTITIES")
        for state_handle in donor.state_entity_handles:
            record = dv._find_record_by_handle(records, state_handle)
            print("STATE", state_handle, record)

        print("DONOR_BLOCK_CONTENT")
        for record, section in zip(records, sections):
            if section != "BLOCKS":
                continue
            if (dv._record_owner(record) or "").upper() != donor.block_record_handle.upper():
                continue
            print("BLOCKREC", dv._record_type(record), record)

        print("DONOR_METADATA_FULL")
        for record, section in zip(records, sections):
            handle = (dv._record_handle(record) or "").upper()
            if section == "OBJECTS" and handle in set(donor.metadata_handles):
                print("META_FULL", dv._record_type(record), record)

        print("BLOCK_RECORD_XDATA")
        for record, section in zip(records, sections):
            if section != "TABLES" or dv._record_type(record) != "BLOCK_RECORD":
                continue
            apps = []
            for app in ("AcDbBlockRepETag", "AcDbBlockRepBTag", "AcDbDynamicBlockTrueName", "AcDbDynamicBlockGUID"):
                payload = dv._xdata_payload(record, app)
                if payload:
                    apps.append((app, payload))
            if apps:
                print("BR", dv._record_handle(record), dv._record_name(record), apps)

        print("INSERTS")
        anonymous_names = set()
        for record, section in zip(records, sections):
            if section != "TABLES" or dv._record_type(record) != "BLOCK_RECORD":
                continue
            payload = dv._xdata_payload(record, "AcDbBlockRepBTag")
            if payload:
                anonymous_names.add((dv._record_name(record) or "").upper())
        for record, section in zip(records, sections):
            if dv._record_type(record) != "INSERT":
                continue
            name = (dv._record_name(record) or "").upper()
            if name == str(donor_name or "").upper() or name in anonymous_names or name.startswith("*U"):
                print("INSERT", section, record)

        self.fail("dynamic representation probe; remove after inspection")


if __name__ == "__main__":
    unittest.main()
