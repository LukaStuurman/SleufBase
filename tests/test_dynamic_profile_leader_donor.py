from __future__ import annotations

from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import ezdxf

from SleufBase.autocad_profile_leader_donor import (
    BLOCK_NAME,
    CIRCLE_CENTER,
    CIRCLE_RADIUS,
    DESCRIPTION_INSERT,
    DEPTH_INSERT,
    DONOR_SHA256,
    LINE_START,
    LINE_TOP,
    POLAR_BASE,
    POLAR_TOP,
    ensure_profile_leader_geometry,
    inspect_profile_leader_block,
    promote_profile_leader_block,
)
from SleufBase.cadastral_export import CadastralDxfExporter
from SleufBase.template_dynamic_profile_leader_patch import (
    _PROFILE_LEADER_PROTOTYPE_ATTR,
    _PROFILE_LEADER_PROTOTYPE_NAME_ATTR,
    _find_template_profile_leader_reference,
)


ASSET = Path(__file__).resolve().parents[1] / "assets" / "cadastral_template.dxf"
APPROVED_DONOR_SHA256 = "f975ccf4cbe8d98aba66dc52300c7c255e9c2d0a86aff4f78f31b7c4c0dc4c3b"


class ApprovedDynamicProfileLeaderTests(unittest.TestCase):
    def _prepared_document(self):
        document = ezdxf.readfile(ASSET)
        exporter = CadastralDxfExporter.__new__(CadastralDxfExporter)
        exporter._remove_template_legacy_profile_leader_blocks(document)
        return exporter, document

    @staticmethod
    def _generated_profile_refs(modelspace):
        refs = []
        for entity in modelspace.query("INSERT"):
            values = {
                str(attribute.dxf.tag).upper(): str(attribute.dxf.text)
                for attribute in entity.attribs
            }
            if {"OMSCHRIJVING", "HOOGTE"}.issubset(values):
                refs.append(entity)
        return refs

    def test_real_template_example_is_captured_instead_of_reconstructed(self) -> None:
        document = ezdxf.readfile(ASSET)
        source_layout, source = _find_template_profile_leader_reference(document)
        self.assertIsNotNone(source_layout)
        self.assertIsNotNone(source, "Het productiesjabloon moet een echt AutoCAD-voorbeeld-INSERT bevatten.")
        source_name = str(source.dxf.name)
        source_attribute_tags = tuple(str(attribute.dxf.tag) for attribute in source.attribs)

        exporter = CadastralDxfExporter.__new__(CadastralDxfExporter)
        with patch(
            "SleufBase.template_dynamic_profile_leader_patch.ensure_profile_leader_geometry"
        ) as rebuild:
            exporter._remove_template_legacy_profile_leader_blocks(document)
            rebuild.assert_not_called()

        prototype = getattr(exporter, _PROFILE_LEADER_PROTOTYPE_ATTR, None)
        self.assertIsNotNone(prototype)
        self.assertIsNot(prototype, source)
        self.assertEqual(str(prototype.dxf.name), source_name)
        self.assertEqual(
            tuple(str(attribute.dxf.tag) for attribute in prototype.attribs),
            source_attribute_tags,
        )
        self.assertEqual(
            getattr(exporter, _PROFILE_LEADER_PROTOTYPE_NAME_ATTR, ""),
            source_name,
        )
        self.assertIn(BLOCK_NAME, document.blocks)
        self.assertFalse(source.is_alive, "Alleen het zichtbare donorvoorbeeld zelf hoort verwijderd te worden.")

    def test_donor_identity_and_geometry_are_exact(self) -> None:
        self.assertEqual(DONOR_SHA256, APPROVED_DONOR_SHA256)
        exporter, document = self._prepared_document()
        self.assertIsNotNone(exporter)
        self.assertIn(BLOCK_NAME, document.blocks)

        block = document.blocks.get(BLOCK_NAME)
        circles = list(block.query("CIRCLE"))
        polylines = list(block.query("LWPOLYLINE"))
        attdefs = {str(entity.dxf.tag): entity for entity in block.query("ATTDEF")}
        self.assertEqual(len(circles), 1)
        self.assertEqual(len(polylines), 1)
        self.assertEqual(set(attdefs), {"OMSCHRIJVING", "HOOGTE"})

        circle = circles[0]
        self.assertAlmostEqual(float(circle.dxf.center.x), CIRCLE_CENTER[0])
        self.assertAlmostEqual(float(circle.dxf.center.y), CIRCLE_CENTER[1])
        self.assertAlmostEqual(float(circle.dxf.radius), CIRCLE_RADIUS)

        points = [(float(point[0]), float(point[1])) for point in polylines[0].get_points("xy")]
        self.assertEqual(points, [LINE_START, LINE_TOP])

        description = attdefs["OMSCHRIJVING"]
        depth = attdefs["HOOGTE"]
        self.assertAlmostEqual(float(description.dxf.insert.x), DESCRIPTION_INSERT[0])
        self.assertAlmostEqual(float(description.dxf.insert.y), DESCRIPTION_INSERT[1])
        self.assertAlmostEqual(float(depth.dxf.insert.x), DEPTH_INSERT[0])
        self.assertAlmostEqual(float(depth.dxf.insert.y), DEPTH_INSERT[1])
        self.assertAlmostEqual(float(description.dxf.rotation), 90.0)
        self.assertAlmostEqual(float(depth.dxf.rotation), 90.0)

    def test_export_clones_real_template_insert_without_static_leader_or_duplicate_marker(self) -> None:
        exporter, document = self._prepared_document()
        modelspace = document.modelspace()
        before_leaders = len(list(modelspace.query("LEADER")))
        before_refs = set(id(entity) for entity in self._generated_profile_refs(modelspace))
        prototype = getattr(exporter, _PROFILE_LEADER_PROTOTYPE_ATTR, None)
        self.assertIsNotNone(prototype)
        prototype_name = str(prototype.dxf.name)

        exporter._add_template_profile_multileader(
            modelspace,
            description="Datakabel",
            depth_text="Diepte: 0.85",
            leader_start=(10.0, 20.02),
            text_insert=(10.0, 20.20),
            marker_scale=0.02,
            color=3,
        )
        self.assertEqual(len(list(modelspace.query("LEADER"))), before_leaders)

        refs = [
            entity
            for entity in self._generated_profile_refs(modelspace)
            if id(entity) not in before_refs
        ]
        self.assertEqual(len(refs), 1)
        block_ref = refs[0]
        self.assertEqual(str(block_ref.dxf.name), prototype_name)
        self.assertAlmostEqual(float(block_ref.dxf.insert.x), 10.0)
        self.assertAlmostEqual(float(block_ref.dxf.insert.y), 20.0)
        self.assertAlmostEqual(float(block_ref.dxf.xscale), 0.02)
        self.assertAlmostEqual(float(block_ref.dxf.yscale), 0.02)
        self.assertAlmostEqual(float(block_ref.dxf.rotation), 0.0)
        self.assertEqual(str(block_ref.dxf.layer), "X-XX-AL-VERWIJZING-SD")
        self.assertEqual(int(block_ref.dxf.color), 3)

        values = {
            str(attribute.dxf.tag): str(attribute.dxf.text)
            for attribute in block_ref.attribs
        }
        self.assertEqual(values["OMSCHRIJVING"], "Datakabel")
        self.assertEqual(values["HOOGTE"], "Diepte: 0.85")

        before_marker = len(list(modelspace))
        exporter._add_template_profile_leader_marker(
            modelspace,
            insert_x=10.0,
            insert_y=20.02,
            marker_scale=0.02,
            layer_name="TEST",
            color=3,
        )
        self.assertEqual(len(list(modelspace)), before_marker)

    def test_exact_native_donor_metadata_survives_save_and_reopen(self) -> None:
        exporter, document = self._prepared_document()
        modelspace = document.modelspace()
        exporter._add_template_profile_multileader(
            modelspace,
            description="LS Datakabel",
            depth_text="0.85",
            leader_start=(1.0, 2.02),
            text_insert=(1.0, 2.2),
            marker_scale=0.02,
            color=3,
        )

        with tempfile.TemporaryDirectory() as temp_dir:
            output = Path(temp_dir) / "approved-profile-leader.dxf"
            document.saveas(output)
            self.assertEqual(promote_profile_leader_block(output, BLOCK_NAME), 1)

            reopened = ezdxf.readfile(output)
            self.assertIn(BLOCK_NAME, reopened.blocks)
            details = inspect_profile_leader_block(output, BLOCK_NAME)

            self.assertEqual(details["donor_sha256"], APPROVED_DONOR_SHA256)
            self.assertTrue(details["is_dynamic"])
            self.assertEqual(details["polar_base"], POLAR_BASE)
            self.assertEqual(details["polar_top"], POLAR_TOP)
            self.assertEqual(details["polar_grip"], POLAR_TOP)

            metadata_types = set(details["metadata_types"])
            self.assertTrue(
                {
                    "BLOCKLINEARPARAMETER",
                    "BLOCKLINEARGRIP",
                    "BLOCKSCALEACTION",
                    "BLOCKPOLARPARAMETER",
                    "BLOCKPOLARGRIP",
                    "BLOCKSTRETCHACTION",
                    "BLOCKMOVEACTION",
                    "ACAD_EVALUATION_GRAPH",
                }.issubset(metadata_types)
            )

            handles = details["entity_handles"]
            self.assertEqual(details["stretch_refs"], (handles["leader"],))
            self.assertEqual(
                set(details["move_refs"]),
                {handles["description"], handles["depth"]},
            )
            self.assertEqual(details["scale_refs"], (handles["circle"],))
            self.assertEqual(
                details["block_rep_tag"][:2],
                ((1070, "1"), (1071, "4")),
            )

            expected_indexes = {
                "circle": "0",
                "description": "1",
                "leader": "2",
                "depth": "3",
            }
            for key, expected_index in expected_indexes.items():
                tag = details["entity_rep_tags"][key]
                self.assertGreaterEqual(len(tag), 2)
                self.assertEqual(tag[0], (1070, "1"))
                self.assertEqual(tag[1], (1071, expected_index))

            # Promotion is deliberately idempotent so a second finalization pass
            # can never stack a duplicate Dynamic Block graph onto the donor.
            self.assertEqual(promote_profile_leader_block(output, BLOCK_NAME), 0)
            ezdxf.readfile(output)

    def test_geometry_helper_remains_the_fallback_for_minimal_templates(self) -> None:
        document = ezdxf.new("R2018")
        exporter = CadastralDxfExporter.__new__(CadastralDxfExporter)
        exporter._remove_template_legacy_profile_leader_blocks(document)
        self.assertIn(BLOCK_NAME, document.blocks)
        self.assertIsNone(getattr(exporter, _PROFILE_LEADER_PROTOTYPE_ATTR, None))
        self.assertEqual(ensure_profile_leader_geometry.__module__, "SleufBase.autocad_profile_leader_donor")


if __name__ == "__main__":
    unittest.main()
