from __future__ import annotations

import unittest

from PIL import Image

from SleufBase.cadastral_export import CadastralDxfExporter
from SleufBase.template_asset_memory_patch import TEMPLATE_PNG_COMPRESS_LEVEL


class TemplatePngPerformanceTests(unittest.TestCase):
    def test_fast_png_behavior_is_owned_by_exporter_patch(self) -> None:
        self.assertTrue(CadastralDxfExporter.SLEUFBASE_TEMPLATE_PNG_FAST_COMPRESSION)
        self.assertEqual(TEMPLATE_PNG_COMPRESS_LEVEL, 1)

    def test_pillow_save_is_not_globally_monkey_patched(self) -> None:
        # Template raster compression is now passed directly by the exporter.
        # Unrelated PIL saves must continue to use Pillow's own implementation.
        self.assertNotEqual(getattr(Image.Image.save, "__name__", ""), "save_scoped")
        self.assertNotIn("template_png_performance_patch", getattr(Image.Image.save, "__module__", ""))


if __name__ == "__main__":
    unittest.main()
