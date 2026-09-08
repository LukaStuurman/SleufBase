from __future__ import annotations

from pathlib import Path
import tempfile
import unittest

import ezdxf

from SleufBase.template_footprint import analyze_template_footprint


class TemplateFootprintTests(unittest.TestCase):
    def test_analyzer_reports_size_and_largest_blocks_without_mutating_template(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            path = Path(temporary_directory) / "template.dxf"
            document = ezdxf.new("R2018")
            block = document.blocks.new(name="TEST_FOOTPRINT", base_point=(0.0, 0.0, 0.0))
            block.add_line((0.0, 0.0), (1.0, 1.0))
            block.add_line((1.0, 1.0), (2.0, 1.0))
            document.modelspace().add_blockref("TEST_FOOTPRINT", (0.0, 0.0))
            document.saveas(path)
            before = path.read_bytes()

            report = analyze_template_footprint(path, largest_block_limit=5)

            self.assertGreater(report.file_size_bytes, 0)
            self.assertGreaterEqual(report.modelspace_entity_count, 1)
            self.assertGreaterEqual(report.block_count, 1)
            self.assertTrue(any(item.name == "TEST_FOOTPRINT" for item in report.largest_blocks))
            self.assertEqual(path.read_bytes(), before)


if __name__ == "__main__":
    unittest.main()
