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

        print("DONOR_VISIBILITY")
        for record, section in zip(records, sections):
            if section == "OBJECTS" and dv._record_type(record) == "BLOCKVISIBILITYPARAMETER":
                if dv._record_owner(record) in donor.metadata_handles:
                    interesting = [(c, v) for c, v in record if c in {5, 10, 20, 30, 11, 21, 31, 40, 70, 90, 280, 281, 301, 302, 303, 304, 305, 331, 332}]
                    print("VIS", interesting)

        print("DONOR_METADATA")
        for record, section in zip(records, sections):
            handle = (dv._record_handle(record) or "").upper()
            if section == "OBJECTS" and handle in set(donor.metadata_handles):
                interesting = [(c, v) for c, v in record if c in {0, 5, 330, 331, 332, 340, 360, 90, 91, 92, 93, 94, 95, 280, 281, 301, 302, 303}]
                print("META", dv._record_type(record), interesting)

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
                interesting = [(c, v) for c, v in record if c in {5, 330, 2, 10, 20, 30, 41, 42, 43, 50, 1001, 1000, 1005}]
                print("INSERT", section, interesting)

        self.fail("dynamic representation probe; remove after inspection")


if __name__ == "__main__":
    unittest.main()
