from __future__ import annotations

from pathlib import Path
import tempfile
import unittest

import ezdxf

from SleufBase import template_reverse_patch as reverse_patch
from SleufBase.autocad_profile_leader_donor import BLOCK_NAME, LAYER_NAME, promote_profile_leader_block
from SleufBase.cadastral_export import CadastralDxfExporter
from SleufBase.profile_leader_circle_style_patch import _set_output_circle_style


ASSET = Path(__file__).resolve().parents[1] / "assets" / "cadastral_template.dxf"


class ProfileLeaderCircleStylePatchTests(unittest.TestCase):
    def test_patch_is_installed_after_literal_donor_patch(self) -> None:
        self.assertEqual(
            promote_profile_leader_block.__module__,
            "SleufBase.profile_leader_circle_style_patch",
        )
        self.assertEqual(CadastralDxfExporter.SLEUFBASE_PROFILE_LEADER_CIRCLE_LAYER, LAYER_NAME)
        self.assertTrue(CadastralDxfExporter.SLEUFBASE_PROFILE_LEADER_CIRCLE_BYBLOCK_COLOR)
        self.assertTrue(reverse_patch.SLEUFBASE_TOP_LEVEL_PROFILE_LEADER_VARIANT_LAYERS)

    def test_in_memory_template_changes_only_donor_circle_style(self) -> None:
        document = ezdxf.readfile(ASSET)
        exporter = CadastralDxfExporter.__new__(CadastralDxfExporter)
        exporter._remove_template_legacy_profile_leader_blocks(document)

        block = document.blocks.get(BLOCK_NAME)
        circles = list(block.query("CIRCLE"))
        self.assertEqual(len(circles), 1)
        circle = circles[0]
        self.assertEqual(str(circle.dxf.layer), LAYER_NAME)
        self.assertEqual(int(circle.dxf.color), 0)

    def test_literal_finalizer_keeps_circle_on_reference_layer(self) -> None:
        document = ezdxf.readfile(ASSET)
        exporter = CadastralDxfExporter.__new__(CadastralDxfExporter)
        exporter._remove_template_legacy_profile_leader_blocks(document)
        exporter._add_template_profile_multileader(
            document.modelspace(),
            description="Datakabel",
            depth_text="0.85",
            leader_start=(1.0, 2.02),
            text_insert=(1.0, 2.2),
            marker_scale=0.02,
            color=3,
        )

        with tempfile.TemporaryDirectory() as temp_dir:
            output = Path(temp_dir) / "circle-style.dxf"
            document.saveas(output)
            self.assertEqual(promote_profile_leader_block(output, BLOCK_NAME), 1)
            reopened = ezdxf.readfile(output)
            circle = list(reopened.blocks.get(BLOCK_NAME).query("CIRCLE"))[0]
            self.assertEqual(str(circle.dxf.layer), LAYER_NAME)
            self.assertEqual(int(circle.dxf.color), 0)
            self.assertEqual(_set_output_circle_style(output, BLOCK_NAME), 0)


if __name__ == "__main__":
    unittest.main()
