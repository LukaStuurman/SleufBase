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

from SleufBase import dynamic_visibility_finalize_patch as finalize_patch
from SleufBase import template_dynamic_visibility_patch as dynamic_patch
from SleufBase import template_reverse_patch as reverse_patch


class DynamicVisibilityFinalizeTests(unittest.TestCase):
    @property
    def template_path(self) -> Path:
        return REPO_ROOT / "assets" / "cadastral_template.dxf"

    def test_normal_is_base_visible_reverse_hidden_and_selector_is_near_profile(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            output_path = Path(temporary_directory) / "dynamic-finalized.dxf"
            document = ezdxf.readfile(self.template_path)
            modelspace = document.modelspace()
            if "KABELS_FINALIZE_TEST" not in document.layers:
                document.layers.add("KABELS_FINALIZE_TEST", color=3)

            normal_line = modelspace.add_line(
                (194660.0, 433600.0),
                (194670.0, 433600.0),
                dxfattribs={"layer": "KABELS_FINALIZE_TEST"},
            )
            reverse_patch._move_entities_to_variant_container(
                document,
                modelspace,
                [normal_line],
                label="PS_FINALIZE",
                slot_index=88,
                mode=reverse_patch.NORMAL_MODE,
            )
            reverse_line = modelspace.add_line(
                (194660.0, 433601.0),
                (194670.0, 433601.0),
                dxfattribs={"layer": "KABELS_FINALIZE_TEST"},
            )
            reverse_patch._move_entities_to_variant_container(
                document,
                modelspace,
                [reverse_line],
                label="PS_FINALIZE",
                slot_index=88,
                mode=reverse_patch.REVERSE_MODE,
            )
            document.saveas(output_path)

            names = dynamic_patch._promote_exported_variants_to_dynamic_blocks(output_path)
            expected_name = dynamic_patch.dynamic_block_name("PS_FINALIZE", 88)
            self.assertEqual(names, [expected_name])

            details = finalize_patch.inspect_finalized_dynamic_block(output_path, expected_name)
            self.assertFalse(details["normal_invisible"])
            self.assertTrue(details["reverse_invisible"])

            normal_tag = details["normal_state_tag"]
            reverse_tag = details["reverse_state_tag"]
            self.assertEqual(normal_tag[:2], ((1070, "1"), (1071, "0")))
            self.assertEqual(reverse_tag[:2], ((1070, "1"), (1071, "1")))
            self.assertEqual(normal_tag[2][0], 1005)
            self.assertEqual(reverse_tag[2][0], 1005)
            self.assertNotEqual(normal_tag[2][1], reverse_tag[2][1])

            grip = details["grip_point"]
            parameter = details["parameter_point"]
            self.assertIsNotNone(grip)
            self.assertIsNotNone(parameter)
            grip_x, grip_y, _grip_z = grip
            parameter_x, parameter_y, _parameter_z = parameter

            # The old cloned donor position was around (0, 5). The selector
            # must instead sit immediately next to the generated profile.
            self.assertGreater(grip_x, 194660.0)
            self.assertLess(grip_x, 194675.0)
            self.assertGreater(grip_y, 433599.0)
            self.assertLess(grip_y, 433605.0)

            # Parameter and grip are translated together, preserving the
            # genuine donor relationship AutoCAD already understands.
            self.assertAlmostEqual(grip_x - parameter_x, 0.0, places=6)
            self.assertAlmostEqual(grip_y - parameter_y, 5.0, places=6)

            # Finished output must still be readable by ezdxf after the raw
            # AutoCAD metadata/state initialization pass.
            reopened = ezdxf.readfile(output_path)
            wrappers = [
                entity
                for entity in reopened.modelspace()
                if entity.dxftype() == "INSERT" and entity.dxf.name == expected_name
            ]
            self.assertEqual(len(wrappers), 1)

    def test_finalize_patch_is_installed_for_template_exports(self) -> None:
        self.assertGreaterEqual(
            int(getattr(dynamic_patch, "_sleufbase_dynamic_finalize_patch_version", 0)),
            finalize_patch.PATCH_VERSION,
        )


if __name__ == "__main__":
    unittest.main()
