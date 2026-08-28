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
    def test_real_template_keeps_normal_reverse_rasters_and_selector_separate(self) -> None:
        template_path = REPO_ROOT / "assets" / "cadastral_template.dxf"
        with tempfile.TemporaryDirectory() as temporary_directory:
            directory = Path(temporary_directory)
            normal_path = directory / "proefsleuven.dxf"
            reverse_path = directory / ".proefsleuven.sleufbase-reverse-source.dxf"
            normal_tiff = directory / "normal-tiff.png"
            reverse_tiff = directory / "reverse-tiff.png"
            normal_map = directory / "normal-kaart.png"
            reverse_map = directory / "reverse-kaart.png"
            Image.new("RGB", (8, 6), "white").save(normal_tiff)
            Image.new("RGB", (8, 6), "black").save(reverse_tiff)
            Image.new("RGB", (8, 6), "red").save(normal_map)
            Image.new("RGB", (8, 6), "blue").save(reverse_map)
            shutil.copy2(template_path, normal_path)
            shutil.copy2(template_path, reverse_path)

            normal_document = ezdxf.readfile(normal_path)
            normal_modelspace = normal_document.modelspace()
            normal_tiff_def = normal_document.add_image_def(
                filename=str(normal_tiff.resolve()),
                size_in_pixel=(8, 6),
                name="PS1_TIFF",
            )
            normal_map_def = normal_document.add_image_def(
                filename=str(normal_map.resolve()),
                size_in_pixel=(8, 6),
                name="PS1_KAART",
            )
            normal_images = [
                normal_modelspace.add_image(
                    insert=(10, 20),
                    size_in_units=(8, 6),
                    image_def=normal_tiff_def,
                ),
                normal_modelspace.add_image(
                    insert=(20, 20),
                    size_in_units=(8, 6),
                    image_def=normal_map_def,
                ),
            ]
            reverse_patch._move_entities_to_variant_container(
                normal_document,
                normal_modelspace,
                normal_images,
                label="PS1",
                slot_index=1,
                mode=reverse_patch.NORMAL_MODE,
            )
            normal_document.saveas(normal_path)

            reverse_document = ezdxf.readfile(reverse_path)
            reverse_modelspace = reverse_document.modelspace()
            reverse_tiff_def = reverse_document.add_image_def(
                filename=str(reverse_tiff.resolve()),
                size_in_pixel=(8, 6),
                name="PS1_TIFF",
            )
            reverse_map_def = reverse_document.add_image_def(
                filename=str(reverse_map.resolve()),
                size_in_pixel=(8, 6),
                name="PS1_KAART",
            )
            reverse_images = [
                reverse_modelspace.add_image(
                    insert=(10, 20),
                    size_in_units=(8, 6),
                    image_def=reverse_tiff_def,
                ),
                reverse_modelspace.add_image(
                    insert=(20, 20),
                    size_in_units=(8, 6),
                    image_def=reverse_map_def,
                ),
            ]
            reverse_patch._move_entities_to_variant_container(
                reverse_document,
                reverse_modelspace,
                reverse_images,
                label="PS1",
                slot_index=1,
                mode=reverse_patch.REVERSE_MODE,
            )
            reverse_document.saveas(reverse_path)

            renamed = dynamic_patch._make_reverse_image_definitions_unique(reverse_path)
            self.assertEqual(
                renamed,
                {
                    "PS1_TIFF": "PS1_TIFF_REVERSE",
                    "PS1_KAART": "PS1_KAART_REVERSE",
                },
            )

            self.assertEqual(
                reverse_patch._merge_reverse_variant_document(normal_path, reverse_path),
                1,
            )
            wrapper_names = dynamic_patch._promote_exported_variants_to_dynamic_blocks(normal_path)
            self.assertEqual(wrapper_names, [dynamic_patch.dynamic_block_name("PS1", 1)])

            details = dynamic_patch.inspect_dynamic_visibility_block(normal_path, wrapper_names[0])
            self.assertTrue(details["is_dynamic"])
            self.assertEqual(details["property_name"], "Versie")
            self.assertEqual(details["states"], ("Normaal", "Reverse"))
            self.assertEqual(details["default_state"], "Normaal")

            merged = ezdxf.readfile(normal_path)
            wrapper = merged.blocks.get(wrapper_names[0])
            child_refs = list(wrapper.query("INSERT"))
            self.assertEqual(len(child_refs), 2)
            children_by_mode = {
                "normal": next(
                    child for child in child_refs if child.dxf.name.endswith("_NORMAAL_CONTENT")
                ),
                "reverse": next(
                    child for child in child_refs if child.dxf.name.endswith("_REVERSE_CONTENT")
                ),
            }

            normal_block = merged.blocks.get(children_by_mode["normal"].dxf.name)
            reverse_block = merged.blocks.get(children_by_mode["reverse"].dxf.name)
            normal_files = {Path(image.image_def.dxf.filename).name for image in normal_block.query("IMAGE")}
            reverse_files = {Path(image.image_def.dxf.filename).name for image in reverse_block.query("IMAGE")}
            self.assertEqual(normal_files, {"normal-tiff.png", "normal-kaart.png"})
            self.assertEqual(reverse_files, {"reverse-tiff.png", "reverse-kaart.png"})

            reverse_asset_dir = directory / "proefsleuven_reverse_assets"
            for image in reverse_block.query("IMAGE"):
                linked_file = Path(image.image_def.dxf.filename)
                self.assertTrue(linked_file.exists())
                self.assertEqual(linked_file.parent, reverse_asset_dir)

            image_dict = merged.rootdict.get_required_dict("ACAD_IMAGE_DICT")
            image_keys = set(image_dict.keys())
            self.assertIn("PS1_TIFF", image_keys)
            self.assertIn("PS1_KAART", image_keys)
            self.assertIn("PS1_TIFF_REVERSE", image_keys)
            self.assertIn("PS1_KAART_REVERSE", image_keys)


if __name__ == "__main__":
    unittest.main()
