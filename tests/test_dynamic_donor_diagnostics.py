from pathlib import Path
import unittest

from SleufBase import autocad_dynamic_visibility as dv


class DynamicDonorDiagnostics(unittest.TestCase):
    def test_print_dynamic_donor_reference_structure(self):
        path = Path(__file__).resolve().parents[1] / "assets" / "cadastral_template.dxf"
        pairs, _newline, _bom = dv._read_pairs(path)
        records = dv._split_records(pairs)
        sections = dv._record_sections(records)
        donor = dv._discover_donor(records, sections)

        root_record = dv._find_record_by_handle(records, donor.block_record_handle)
        root_name = dv._record_name(root_record or [])
        print("DONOR_ROOT", donor.block_record_handle, root_name)

        visibility = next(
            record for record, section in zip(records, sections)
            if section == "OBJECTS"
            and dv._record_type(record) == "BLOCKVISIBILITYPARAMETER"
            and (dv._record_handle(record) or "").upper() in set(donor.metadata_handles)
        )
        print("VISIBILITY_RECORD", visibility)

        anonymous = []
        for record, section in zip(records, sections):
            if section != "TABLES" or dv._record_type(record) != "BLOCK_RECORD":
                continue
            values = [(code, value.strip()) for code, value in record]
            app_positions = [i for i, item in enumerate(values) if item == (1001, "AcDbBlockRepBTag")]
            if not app_positions:
                continue
            if any(code == 1005 and value.upper() == donor.block_record_handle.upper() for code, value in values):
                anonymous.append((dv._record_name(record), dv._record_handle(record), record))

        print("ANON_COUNT", len(anonymous))
        for name, handle, record in anonymous:
            print("ANON_BLOCK_RECORD", name, handle, record)

        names = {str(root_name or "").upper()} | {str(name or "").upper() for name, _h, _r in anonymous}
        for record, section in zip(records, sections):
            if dv._record_type(record) != "INSERT":
                continue
            name = str(dv._record_name(record) or "").upper()
            if name in names:
                print("INSERT_RECORD", section, record)

        self.assertTrue(donor.block_record_handle)


if __name__ == "__main__":
    unittest.main()
