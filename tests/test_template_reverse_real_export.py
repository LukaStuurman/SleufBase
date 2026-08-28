from __future__ import annotations

from pathlib import Path
import shutil
import sys
import tempfile
import unittest

import ezdxf
from PIL import Image

REPO_ROOT = Path(__file__).resolve().parents[1]
PACKAGE_PARENT = REPO_ROOT.parent
if str(PACKAGE_PARENT) not in sys.path:
    sys.path.insert(0, str(PACKAGE_PARENT))

from SleufBase import template_dynamic_visibility_patch as dynamic_patch
from SleufBase import template_reverse_patch as reverse_patch


class RealTemplateReverseMergeTests(unittest.TestCase):
    def test_real_template_merges_same_named_image_defs_and_promotes_dynamic_pair(self) -> None:
        template_path = REPO_ROOT / "assets" / "cadastral_template.dxf"
        with tempfile.TemporaryDirectory() as temporary_directory:
            directory = Path(temporary_directory)
            normal_path = directory / "proefsleuven.dxf"
            reverse_path = directory / ".proefsleuven.sleufbase-reverse-source.dxf"
            normal_png = directory / "normal-kaart.png"
            reverse_png = directory / "reverse-kaart.png"
            Image.new("RGB", (8, 6), "white").save(normal_png)
            Image.new("RGB", (8, 6), "black").save(reverse_png)
            shutil.copy2(template_path, normal_path)
            shutil.copy2(template_path, reverse_path)

            normal_document = ezdxf.readfile(normal_path)
            normal_modelspace = normal_document.modelspace()
            normal_def = normal_document.add_image_def(
                filename=str(normal_png.resolve()),
                size_in_pixel=(8, 6),
                name="PS1_KAART",
            )
            normal_image = normal_modelspace.add_image(
                insert=(10, 20),
                size_in_units=(8, 6),
                image_def=normal_def,
            )
            reverse_patch._move_entities_to_variant_container(
                normal_document,
                normal_modelspace,
                [normal_image],
                label="PS1",
                slot_index=1,
                mode=reverse_patch.NORMAL_MODE,
            )
            normal_document.saveas(normal_path)

            reverse_document = ezdxf.readfile(reverse_path)
            reverse_modelspace = reverse_document.modelspace()
            reverse_def = reverse_document.add_image_def(
                filename=str(reverse_png.resolve()),
                size_in_pixel=(8, 6),
                name="PS1_KAART",
            )
            reverse_image = reverse_modelspace.add_image(
                insert=(10, 20),
                size_in_units=(8, 6),
                image_def=reverse_def,
            )
            reverse_patch._move_entities_to_variant_container(
                reverse_document,
                reverse_modelspace,
                [reverse_image],
                label="PS1",
                slot_index=1,
                mode=reverse_patch.REVERSE_MODE,
            )
            reverse_document.saveas(reverse_path)

            self.assertEqual(
                reverse_patch._merge_reverse_variant_document(normal_path, reverse_path),
                1,
            )
            wrapper_names = dynamic_patch._promote_exported_variants_to_dynamic_blocks(normal_path)
            self.assertEqual(wrapper_names, [dynamic_patch.dynamic_block_name("PS1", 1)])

            merged = ezdxf.readfile(normal_path)
            wrapper = merged.blocks.get(wrapper_names[0])
            child_refs = list(wrapper.query("INSERT"))
            self.assertEqual(len(child_refs), 2)
            reverse_block = merged.blocks.get(child_refs[1].dxf.name)
            images = list(reverse_block.query("IMAGE"))
            self.assertEqual(len(images), 1)
            self.assertTrue(Path(images[0].image_def.dxf.filename).exists())


if __name__ == "__main__":
    unittest.main()
