from __future__ import annotations

import unittest

from PIL import Image

from SleufBase.cadastral_export import CadastralDxfExporter
from SleufBase import template_png_performance_patch as png_patch


class TemplatePngPerformancePatchTests(unittest.TestCase):
    def test_patch_is_installed_on_exporter(self) -> None:
        self.assertGreaterEqual(
            int(getattr(CadastralDxfExporter, "_sleufbase_template_png_performance_version", 0) or 0),
            1,
        )
        self.assertTrue(CadastralDxfExporter.SLEUFBASE_TEMPLATE_PNG_FAST_COMPRESSION)

    def test_fast_png_parameters_are_scoped_and_lossless_mode_only(self) -> None:
        calls = []
        original = png_patch._ORIGINAL_IMAGE_SAVE

        def fake_save(image, fp, format=None, **params):
            calls.append((format, dict(params)))
            return None

        png_patch._ORIGINAL_IMAGE_SAVE = fake_save
        image = Image.new("RGBA", (2, 2), (1, 2, 3, 255))
        try:
            image.save("outside.png", format="PNG")
            with png_patch._template_png_fast_save_scope():
                image.save("inside.png", format="PNG")
                image.save("inside.jpg", format="JPEG")
        finally:
            image.close()
            png_patch._ORIGINAL_IMAGE_SAVE = original

        self.assertEqual(calls[0], ("PNG", {}))
        self.assertEqual(calls[1][0], "PNG")
        self.assertEqual(calls[1][1].get("compress_level"), 1)
        self.assertFalse(calls[1][1].get("optimize", True))
        self.assertEqual(calls[2], ("JPEG", {}))


if __name__ == "__main__":
    unittest.main()
