from __future__ import annotations

from pathlib import Path
import tempfile
import unittest

import ezdxf

from SleufBase.autocad_profile_leader_donor import BLOCK_NAME
from SleufBase.cadastral_export import CadastralDxfExporter
from SleufBase.profile_leader_autocad_compat_patch import (
    DONOR_DYNAMIC_BLOCK_GUID,
    inspect_profile_leader_autocad_compat,
    promote_profile_leader_autocad_compat,
)


ASSET = Path(__file__).resolve().parents[1] / "assets" / "cadastral_template.dxf"


class ProfileLeaderAutoCadCompatibilityTests(unittest.TestCase):
    def _document_with_two_refs(self):
        document = ezdxf.readfile(ASSET)
        exporter = CadastralDxfExporter.__new__(CadastralDxfExporter)
        exporter._remove_template_legacy_profile_leader_blocks(document)
        modelspace = document.modelspace()
        for index in range(2):
            exporter._add_template_profile_multileader(
                modelspace,
                description=f"Datakabel {index + 1}",
                depth_text=f"{0.8 + index * 0.1:.2f}",
                leader_start=(10.0 + index, 20.02),
                text_insert=(10.0 + index, 20.20),
                marker_scale=0.02,
                color=3,
            )
        return document

    def test_final_dxf_contains_exact_donor_classes_guid_and_blkrefs(self) -> None:
        document = self._document_with_two_refs()
        with tempfile.TemporaryDirectory() as temp_dir:
            output = Path(temp_dir) / "profile-leader-compat.dxf"
            document.saveas(output)
            self.assertEqual(promote_profile_leader_autocad_compat(output, BLOCK_NAME), 1)

            details = inspect_profile_leader_autocad_compat(output, BLOCK_NAME)
            self.assertEqual(details["guid"], DONOR_DYNAMIC_BLOCK_GUID)
            self.assertFalse(details["missing_classes"])
            self.assertFalse(details["mismatched_classes"])
            self.assertTrue(details["actual_refs"])
            self.assertEqual(set(details["blkrefs"]), set(details["actual_refs"]))

            # The final file must remain readable after the raw AutoCAD donor
            # registration records have been inserted.
            reopened = ezdxf.readfile(output)
            self.assertIn(BLOCK_NAME, reopened.blocks)

    def test_compatibility_finalizer_is_idempotent(self) -> None:
        document = self._document_with_two_refs()
        with tempfile.TemporaryDirectory() as temp_dir:
            output = Path(temp_dir) / "profile-leader-idempotent.dxf"
            document.saveas(output)
            self.assertEqual(promote_profile_leader_autocad_compat(output, BLOCK_NAME), 1)
            self.assertEqual(promote_profile_leader_autocad_compat(output, BLOCK_NAME), 0)

            details = inspect_profile_leader_autocad_compat(output, BLOCK_NAME)
            self.assertEqual(details["guid"], DONOR_DYNAMIC_BLOCK_GUID)
            self.assertEqual(set(details["blkrefs"]), set(details["actual_refs"]))
            self.assertFalse(details["missing_classes"])
            self.assertFalse(details["mismatched_classes"])


if __name__ == "__main__":
    unittest.main()
