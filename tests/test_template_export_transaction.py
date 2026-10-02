from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

from SleufBase.cadastral_export import CadastralDxfExporter, CadastralExportError
from SleufBase import template_reverse_patch as reverse
from SleufBase.template_export_transaction import staged_template_output


class TemplateExportTransactionTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.output = self.root / "test.dxf"
        self.output.write_bytes(b"previous valid drawing")
        self.assets = self.root / "test_assets"
        self.assets.mkdir()
        self.old_image = self.assets / "logo.png"
        self.old_image.write_bytes(b"previous image")

    def assert_previous_export_intact(self):
        self.assertEqual(self.output.read_bytes(), b"previous valid drawing")
        self.assertEqual(self.old_image.read_bytes(), b"previous image")
        self.assertEqual(list(self.assets.iterdir()), [self.old_image])

    def test_failed_export_removes_only_its_bundle(self):
        for phase in ("raster", "normal", "reverse", "dynamic validation"):
            with self.subTest(phase=phase):
                with self.assertRaisesRegex(RuntimeError, phase):
                    with staged_template_output(self.output) as staged:
                        (staged.parent / "logo.png").write_bytes(b"new image")
                        staged.write_bytes(b"partial drawing")
                        raise RuntimeError(phase)
                self.assert_previous_export_intact()

    def test_locked_destination_preserves_previous_export(self):
        with patch("SleufBase.template_export_transaction.os.replace", side_effect=PermissionError("locked")):
            with self.assertRaises(PermissionError):
                with staged_template_output(self.output) as staged:
                    staged.write_bytes(b"new drawing")
        self.assert_previous_export_intact()

    def test_missing_result_is_not_published(self):
        with self.assertRaisesRegex(RuntimeError, "compleet bestand"):
            with staged_template_output(self.output):
                pass
        self.assert_previous_export_intact()

    def test_empty_result_is_not_published(self):
        with self.assertRaisesRegex(RuntimeError, "compleet bestand"):
            with staged_template_output(self.output) as staged:
                staged.touch()
        self.assert_previous_export_intact()

    def test_success_keeps_permanent_images_and_saved_copies(self):
        image_paths = []
        for content in (b"first image", b"second image"):
            with staged_template_output(self.output) as staged:
                image = staged.parent / "logo.png"
                image.write_bytes(content)
                staged.write_text(str(image), encoding="utf-8")
                image_paths.append(image)
                self.assertEqual(self.output.read_bytes(), b"previous valid drawing" if len(image_paths) == 1 else str(image_paths[0]).encode())
        self.assertNotEqual(image_paths[0], image_paths[1])
        self.assertEqual(image_paths[0].read_bytes(), b"first image")
        self.assertEqual(image_paths[1].read_bytes(), b"second image")
        self.assertEqual(Path(self.output.read_text()).read_bytes(), b"second image")
        self.assertEqual(self.old_image.read_bytes(), b"previous image")

    def test_overlapping_attempts_have_separate_assets(self):
        with staged_template_output(self.output) as first:
            first.write_bytes(b"first drawing")
            with staged_template_output(self.output) as second:
                self.assertNotEqual(first.parent, second.parent)
                second.write_bytes(b"second drawing")
            self.assertEqual(self.output.read_bytes(), b"second drawing")
        self.assertEqual(self.output.read_bytes(), b"first drawing")

    def test_reverse_failure_uses_transaction_in_actual_export_entrypoint(self):
        reverse.install_template_reverse_export_patch()
        exporter = CadastralDxfExporter(SimpleNamespace())

        def export_half(original, instance, arguments, *, mode, output_path, **kwargs):
            if mode == reverse.REVERSE_MODE:
                raise CadastralExportError("reverse failed")
            output_path.write_bytes(b"normal half")
            (output_path.parent / "logo.png").write_bytes(b"new logo")
            return output_path

        with patch.object(reverse, "_call_original_export", side_effect=export_half):
            with self.assertRaisesRegex(CadastralExportError, "reverse failed"):
                exporter.export_template_sheet(self.output, "template.dxf", [])
        self.assert_previous_export_intact()
