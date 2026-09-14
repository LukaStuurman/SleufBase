from __future__ import annotations

import unittest

from SleufBase.cadastral_export import CadastralDxfExporter
from SleufBase.dxf_template_pipeline_v9_patch import _slot_asset_label


class DxfTemplatePipelineV9Tests(unittest.TestCase):
    def test_duplicate_visible_labels_get_unique_slot_asset_labels(self) -> None:
        first = _slot_asset_label("PS2", 2)
        second = _slot_asset_label("PS2", 3)

        self.assertEqual(first, "PS2__slot002")
        self.assertEqual(second, "PS2__slot003")
        self.assertNotEqual(first, second)

    def test_same_slot_label_is_deterministic_for_normal_and_reverse_pass(self) -> None:
        self.assertEqual(
            _slot_asset_label("PS2", 7),
            _slot_asset_label("PS2", 7),
        )

    def test_patch_is_installed_on_exporter(self) -> None:
        self.assertTrue(
            getattr(CadastralDxfExporter, "SLEUFBASE_TEMPLATE_RASTER_SLOT_NAMES", False)
        )
        self.assertGreaterEqual(
            int(getattr(CadastralDxfExporter, "_sleufbase_dxf_template_pipeline_v9_version", 0)),
            1,
        )


if __name__ == "__main__":
    unittest.main()
