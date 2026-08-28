from __future__ import annotations

from pathlib import Path
import sys
import tempfile
import unittest

import ezdxf

REPO_ROOT = Path(__file__).resolve().parents[1]
PACKAGE_PARENT = REPO_ROOT.parent
if str(PACKAGE_PARENT) not in sys.path:
    sys.path.insert(0, str(PACKAGE_PARENT))

from SleufBase import autocad_dynamic_visibility as dynamic_visibility
from SleufBase import template_dynamic_visibility_patch as dynamic_patch
from SleufBase import template_reverse_patch as reverse_patch


class AutoCadDynamicVisibilityTests(unittest.TestCase):
    @property
    def template_path(self) -> Path:
        return REPO_ROOT / "assets" / "cadastral_template.dxf"

    def _donor_rep_e_tag(self) -> tuple[tuple[int, str], ...]:
        pairs, _newline, _had_bom = dynamic_visibility._read_pairs(self.template_path)
        records = dynamic_visibility._split_records(pairs)
        sections = dynamic_visibility._record_sections(records)
        donor = dynamic_visibility._discover_donor(records, sections)
        return tuple((code, value.strip()) for code, value in donor.block_rep_e_tag)

    def test_real_template_contains_native_two_state_visibility_donor(self) -> None:
        pairs, _newline, _had_bom = dynamic_visibility._read_pairs(self.template_path)
        records = dynamic_visibility._split_records(pairs)
        sections = dynamic_visibility._record_sections(records)
        donor = dynamic_visibility._discover_donor(records, sections)

        self.assertTrue(donor.block_record_handle)
        self.assertTrue(donor.xdictionary_handle)
        self.assertGreaterEqual(len(donor.metadata_handles), 4)
        self.assertEqual(len(donor.state_entity_handles), 2)
        self.assertNotEqual(
            donor.state_entity_handles[0],
            donor.state_entity_handles[1],
        )
        self.assertTrue(donor.block_rep_e_tag)
        self.assertEqual(self._donor_rep_e_tag(), ((1070, "1"), (1071, "2")))

    def test_variant_pair_is_wrapped_without_changing_child_layers(self) -> None:
        document = ezdxf.new("R2018")
        document.layers.add("KABELS", color=3)
        modelspace = document.modelspace()
        normal_line = modelspace.add_line(
            (1, 2),
            (3, 4),
            dxfattribs={"layer": "KABELS"},
        )
        reverse_patch._move_entities_to_variant_container(
            document,
            modelspace,
            [normal_line],
            label="PS7",
            slot_index=7,
            mode=reverse_patch.NORMAL_MODE,
        )
        reverse_line = modelspace.add_line(
            (1, 3),
            (3, 5),
            dxfattribs={"layer": "KABELS"},
        )
        reverse_patch._move_entities_to_variant_container(
            document,
            modelspace,
            [reverse_line],
            label="PS7",
            slot_index=7,
            mode=reverse_patch.REVERSE_MODE,
        )

        wrapper_names = dynamic_patch._wrap_variant_pairs_as_static_blocks(document)
        expected_name = dynamic_patch.dynamic_block_name("PS7", 7)
        self.assertEqual(wrapper_names, [expected_name])

        top_level = [
            entity
            for entity in modelspace
            if entity.dxftype() == "INSERT" and entity.dxf.name == expected_name
        ]
        self.assertEqual(len(top_level), 1)
        self.assertEqual(top_level[0].dxf.layer, "0")

        wrapper = document.blocks.get(expected_name)
        children = list(wrapper.query("INSERT"))
        self.assertEqual(len(children), 2)
        self.assertTrue(children[0].dxf.name.endswith("_NORMAAL_CONTENT"))
        self.assertTrue(children[1].dxf.name.endswith("_REVERSE_CONTENT"))
        self.assertEqual(children[0].dxf.layer, "0")
        self.assertEqual(children[1].dxf.layer, "0")

        normal_content = document.blocks.get(children[0].dxf.name)
        reverse_content = document.blocks.get(children[1].dxf.name)
        self.assertEqual(next(iter(normal_content.query("LINE"))).dxf.layer, "KABELS")
        self.assertEqual(next(iter(reverse_content.query("LINE"))).dxf.layer, "KABELS")

    def test_real_template_promotes_pair_to_native_versie_dropdown(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            output_path = Path(temporary_directory) / "dynamic-proefsleuf.dxf"
            document = ezdxf.readfile(self.template_path)
            if "KABELS_TEST" not in document.layers:
                document.layers.add("KABELS_TEST", color=3)
            modelspace = document.modelspace()

            normal_line = modelspace.add_line(
                (194660, 433600),
                (194670, 433600),
                dxfattribs={"layer": "KABELS_TEST"},
            )
            reverse_patch._move_entities_to_variant_container(
                document,
                modelspace,
                [normal_line],
                label="PS_DYNAMIC_TEST",
                slot_index=77,
                mode=reverse_patch.NORMAL_MODE,
            )
            reverse_line = modelspace.add_line(
                (194660, 433601),
                (194670, 433601),
                dxfattribs={"layer": "KABELS_TEST"},
            )
            reverse_patch._move_entities_to_variant_container(
                document,
                modelspace,
                [reverse_line],
                label="PS_DYNAMIC_TEST",
                slot_index=77,
                mode=reverse_patch.REVERSE_MODE,
            )
            document.saveas(output_path)

            wrapper_names = dynamic_patch._promote_exported_variants_to_dynamic_blocks(
                output_path
            )
            expected_name = dynamic_patch.dynamic_block_name("PS_DYNAMIC_TEST", 77)
            self.assertEqual(wrapper_names, [expected_name])

            details = dynamic_visibility.inspect_dynamic_visibility_block(
                output_path,
                expected_name,
            )
            self.assertTrue(details["is_dynamic"])
            self.assertEqual(details["property_name"], "Versie")
            self.assertEqual(details["states"], ("Normaal", "Reverse"))
            self.assertEqual(details["default_state"], "Normaal")
            self.assertEqual(details["block_rep_e_tag"], self._donor_rep_e_tag())

            # ezdxf must still be able to parse the finished file, while the
            # unsupported AutoCAD dynamic objects are preserved as DXF data.
            reopened = ezdxf.readfile(output_path)
            wrappers = [
                entity
                for entity in reopened.modelspace()
                if entity.dxftype() == "INSERT" and entity.dxf.name == expected_name
            ]
            self.assertEqual(len(wrappers), 1)
            child_refs = list(reopened.blocks.get(expected_name).query("INSERT"))
            self.assertEqual(len(child_refs), 2)
            self.assertTrue(child_refs[0].dxf.name.endswith("_NORMAAL_CONTENT"))
            self.assertTrue(child_refs[1].dxf.name.endswith("_REVERSE_CONTENT"))


if __name__ == "__main__":
    unittest.main()
