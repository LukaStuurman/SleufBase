from __future__ import annotations

import unittest

from SleufBase.marxact_import import (
    MARXACT_OBJECT_LAYER_KEY,
    MARXACT_OBJECT_NAME_KEY,
    MarXactObject,
    MarXactTrench,
    build_marxact_virtual_layer,
    parse_marxact_leader_content,
    trench_centerline,
)
from SleufBase.settings import normalize_marxact_name_mappings
from SleufBase.virtual_trench import VIRTUAL_TRENCH_METADATA_KEY


class MarXactImportTests(unittest.TestCase):
    def test_parse_multileader_name_and_height(self) -> None:
        name, height = parse_marxact_leader_content(
            "Id=28^JName=glasvezel kabel (bundel)^JHeight=3.1441^J"
        )
        self.assertEqual(name, "glasvezel kabel (bundel)")
        self.assertAlmostEqual(height or 0.0, 3.1441)

    def test_block_layer_is_source_except_for_layer_zero(self) -> None:
        normal = MarXactObject(1.0, 2.0, 3.0, "Water", "water detail", 3.0, "marxact_point")
        layer_zero = MarXactObject(1.0, 2.0, 3.0, "0", "glasvezel kabel", 3.0, "marxact_point")
        self.assertEqual(normal.mapping_name, "Water")
        self.assertEqual(layer_zero.mapping_name, "glasvezel kabel")

    def test_mapping_normalizer_is_case_insensitive(self) -> None:
        normalized = normalize_marxact_name_mappings(
            {" Water ": "water", "water": "other", "": "ls", "Laagspanning": "ls"}
        )
        self.assertEqual(normalized, {"Water": "water", "Laagspanning": "ls"})

    def test_centerline_uses_long_axis_and_measured_width(self) -> None:
        trench = MarXactTrench(
            name="ps3",
            polygon=(
                (100.0, 200.0, 3.7),
                (104.0, 202.0, 3.6),
                (103.5, 203.0, 3.5),
                (99.5, 201.0, 3.6),
            ),
        )
        start, end, width = trench_centerline(trench)
        self.assertGreater(((end[0] - start[0]) ** 2 + (end[1] - start[1]) ** 2) ** 0.5, 4.0)
        self.assertGreater(width, 0.5)
        self.assertLess(width, 2.0)

    def test_virtual_layer_preserves_marxact_info_and_selected_source(self) -> None:
        trench = MarXactTrench(
            name="ps3",
            polygon=(
                (100.0, 200.0, 3.7),
                (104.0, 200.0, 3.7),
                (104.0, 201.0, 3.6),
                (100.0, 201.0, 3.6),
            ),
            objects=[
                MarXactObject(
                    102.0,
                    200.5,
                    3.1,
                    "Laagspanning",
                    "ls kabel",
                    3.1,
                    "marxact_point",
                )
            ],
        )
        layer = build_marxact_virtual_layer(
            trench,
            source_path="example.dxf",
            source_name_resolver=lambda _item: "ls",
            fallback_index=3,
        )
        payload = layer.metadata[VIRTUAL_TRENCH_METADATA_KEY]
        objects = [point for point in payload["points"] if point.get("role") == "object"]
        self.assertEqual(len(objects), 1)
        self.assertEqual(objects[0]["source_name"], "ls")
        self.assertEqual(objects[0][MARXACT_OBJECT_LAYER_KEY], "Laagspanning")
        self.assertEqual(objects[0][MARXACT_OBJECT_NAME_KEY], "ls kabel")
        self.assertEqual(layer.metadata["template_export_ps_number"], "3")


if __name__ == "__main__":
    unittest.main()
