from __future__ import annotations

from pathlib import Path
import tempfile
import unittest

import ezdxf

from SleufBase.autocad_profile_leader_donor import (
    BLOCK_NAME,
    DONOR_SHA256,
    promote_profile_leader_block,
)
from SleufBase.cadastral_export import CadastralDxfExporter
from SleufBase.literal_profile_leader_donor_patch import (
    LITERAL_DONOR_PAYLOAD_SHA256,
    LITERAL_DONOR_SOURCE_SHA256,
    inspect_literal_profile_leader_geometry,
)


ASSET = Path(__file__).resolve().parents[1] / "assets" / "cadastral_template.dxf"
APPROVED_DONOR_SHA256 = "f975ccf4cbe8d98aba66dc52300c7c255e9c2d0a86aff4f78f31b7c4c0dc4c3b"
APPROVED_LITERAL_PAYLOAD_SHA256 = "0f8cd6d5bc53a81ff2a85ad2ed2eb9438f6da687c86690b426620d6b12f44893"


class LiteralProfileLeaderDonorTests(unittest.TestCase):
    def _document(self):
        document = ezdxf.readfile(ASSET)
        exporter = CadastralDxfExporter.__new__(CadastralDxfExporter)
        exporter._remove_template_legacy_profile_leader_blocks(document)
        return exporter, document

    def test_embedded_literal_payload_is_from_the_approved_uploaded_dxf(self) -> None:
        self.assertEqual(DONOR_SHA256, APPROVED_DONOR_SHA256)
        self.assertEqual(LITERAL_DONOR_SOURCE_SHA256, APPROVED_DONOR_SHA256)
        self.assertEqual(LITERAL_DONOR_PAYLOAD_SHA256, APPROVED_LITERAL_PAYLOAD_SHA256)

    def test_final_block_uses_literal_attdef_and_polyline_records(self) -> None:
        exporter, document = self._document()
        exporter._add_template_profile_multileader(
            document.modelspace(),
            description="Datakabel",
            depth_text="22.46",
            leader_start=(10.0, 20.02),
            text_insert=(10.0, 20.20),
            marker_scale=0.02,
            color=3,
        )

        with tempfile.TemporaryDirectory() as temp_dir:
            output = Path(temp_dir) / "literal-profile-leader.dxf"
            document.saveas(output)
            self.assertEqual(promote_profile_leader_block(output, BLOCK_NAME), 1)

            literal = inspect_literal_profile_leader_geometry(output, BLOCK_NAME)
            self.assertTrue(literal["is_literal"])
            self.assertEqual(literal["source_sha256"], APPROVED_DONOR_SHA256)
            self.assertEqual(literal["payload_sha256"], APPROVED_LITERAL_PAYLOAD_SHA256)
            self.assertEqual(literal["description_prompt"], "Omschrijving")
            self.assertEqual(literal["depth_prompt"], "Hoogte")
            self.assertTrue(literal["description_annotative"])
            self.assertTrue(literal["depth_annotative"])
            self.assertTrue(literal["description_xdict"])
            self.assertTrue(literal["depth_xdict"])
            self.assertEqual(literal["leader_constant_width"], 0.0)

            # The literal block has to remain a valid ezdxf/AutoCAD drawing and
            # a second finalization pass may not append duplicate dictionaries.
            ezdxf.readfile(output)
            self.assertEqual(promote_profile_leader_block(output, BLOCK_NAME), 0)
            ezdxf.readfile(output)

    def test_one_literal_definition_can_be_pasted_with_different_attributes(self) -> None:
        exporter, document = self._document()
        modelspace = document.modelspace()
        exporter._add_template_profile_multileader(
            modelspace,
            description="Datakabel",
            depth_text="22.46",
            leader_start=(10.0, 20.02),
            text_insert=(10.0, 20.20),
            marker_scale=0.02,
            color=3,
        )
        exporter._add_template_profile_multileader(
            modelspace,
            description="LS-kabel",
            depth_text="21.91",
            leader_start=(30.0, 40.02),
            text_insert=(30.0, 40.20),
            marker_scale=0.02,
            color=1,
        )

        with tempfile.TemporaryDirectory() as temp_dir:
            output = Path(temp_dir) / "literal-profile-leader-copies.dxf"
            document.saveas(output)
            promote_profile_leader_block(output, BLOCK_NAME)
            reopened = ezdxf.readfile(output)

            values = []
            for block_ref in reopened.modelspace().query("INSERT"):
                if str(block_ref.dxf.name) != BLOCK_NAME:
                    continue
                values.append(
                    {
                        str(attribute.dxf.tag): str(attribute.dxf.text)
                        for attribute in block_ref.attribs
                    }
                )
            self.assertIn({"OMSCHRIJVING": "Datakabel", "HOOGTE": "22.46"}, values)
            self.assertIn({"OMSCHRIJVING": "LS-kabel", "HOOGTE": "21.91"}, values)


if __name__ == "__main__":
    unittest.main()
