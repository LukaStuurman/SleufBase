from __future__ import annotations

from pathlib import Path
import sys
import tempfile
import unittest

import ezdxf
from PIL import Image

REPO_ROOT = Path(__file__).resolve().parents[1]
PACKAGE_PARENT = REPO_ROOT.parent
if str(PACKAGE_PARENT) not in sys.path:
    sys.path.insert(0, str(PACKAGE_PARENT))

from SleufBase import template_reverse_patch as reverse_patch


class TemplateReversePatchTests(unittest.TestCase):
    def test_layer_names_are_slot_specific_and_dxf_safe(self) -> None:
        self.assertEqual(
            reverse_patch.variant_layer_name('PS 5/A?', 3, reverse_patch.NORMAL_MODE),
            'SLEUFBASE_PS_5_A_VAK03_NORMAAL',
        )
        self.assertEqual(
            reverse_patch.variant_layer_name('PS 5/A?', 3, reverse_patch.REVERSE_MODE),
            'SLEUFBASE_PS_5_A_VAK03_REVERSE',
        )

    def test_variant_container_preserves_child_layer_and_controls_visibility(self) -> None:
        document = ezdxf.new('R2010')
        document.layers.add('KABELS', color=3)
        modelspace = document.modelspace()
        line = modelspace.add_line(
            (1.0, 2.0),
            (3.0, 4.0),
            dxfattribs={'layer': 'KABELS', 'color': 256},
        )

        reverse_patch._move_entities_to_variant_container(
            document,
            modelspace,
            [line],
            label='PS5',
            slot_index=2,
            mode=reverse_patch.NORMAL_MODE,
        )

        layer_name = reverse_patch.variant_layer_name('PS5', 2, reverse_patch.NORMAL_MODE)
        block_name = reverse_patch.variant_block_name('PS5', 2, reverse_patch.NORMAL_MODE)
        self.assertTrue(document.layers.get(layer_name).is_on())
        inserts = [
            entity
            for entity in modelspace
            if entity.dxftype() == 'INSERT' and entity.dxf.name == block_name
        ]
        self.assertEqual(len(inserts), 1)
        self.assertEqual(inserts[0].dxf.layer, layer_name)
        self.assertEqual(len(list(modelspace.query('LINE'))), 0)

        block_lines = list(document.blocks.get(block_name).query('LINE'))
        self.assertEqual(len(block_lines), 1)
        self.assertEqual(block_lines[0].dxf.layer, 'KABELS')
        self.assertEqual(block_lines[0].dxf.color, 256)

    def test_reverse_layer_starts_hidden(self) -> None:
        document = ezdxf.new('R2010')
        modelspace = document.modelspace()
        line = modelspace.add_line((0, 0), (1, 0))
        reverse_patch._move_entities_to_variant_container(
            document,
            modelspace,
            [line],
            label='PS1',
            slot_index=1,
            mode=reverse_patch.REVERSE_MODE,
        )
        layer_name = reverse_patch.variant_layer_name('PS1', 1, reverse_patch.REVERSE_MODE)
        self.assertTrue(document.layers.get(layer_name).is_off())

    def test_merge_keeps_normal_visible_and_imports_reverse_hidden(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            directory = Path(temporary_directory)
            normal_path = directory / 'proefsleuven.dxf'
            reverse_path = directory / '.proefsleuven.sleufbase-reverse-source.dxf'

            normal_document = ezdxf.new('R2010')
            normal_document.layers.add('KABELS', color=3)
            normal_modelspace = normal_document.modelspace()
            normal_line = normal_modelspace.add_line(
                (0, 0), (5, 0), dxfattribs={'layer': 'KABELS'}
            )
            reverse_patch._move_entities_to_variant_container(
                normal_document,
                normal_modelspace,
                [normal_line],
                label='PS1',
                slot_index=1,
                mode=reverse_patch.NORMAL_MODE,
            )
            normal_document.saveas(normal_path)

            reverse_document = ezdxf.new('R2010')
            reverse_document.layers.add('KABELS', color=3)
            reverse_modelspace = reverse_document.modelspace()
            reverse_line = reverse_modelspace.add_line(
                (0, 1), (5, 1), dxfattribs={'layer': 'KABELS'}
            )
            reverse_patch._move_entities_to_variant_container(
                reverse_document,
                reverse_modelspace,
                [reverse_line],
                label='PS1',
                slot_index=1,
                mode=reverse_patch.REVERSE_MODE,
            )
            reverse_document.saveas(reverse_path)

            merged_count = reverse_patch._merge_reverse_variant_document(
                normal_path,
                reverse_path,
            )
            self.assertEqual(merged_count, 1)

            merged = ezdxf.readfile(normal_path)
            normal_layer = reverse_patch.variant_layer_name(
                'PS1', 1, reverse_patch.NORMAL_MODE
            )
            reverse_layer = reverse_patch.variant_layer_name(
                'PS1', 1, reverse_patch.REVERSE_MODE
            )
            self.assertTrue(merged.layers.get(normal_layer).is_on())
            self.assertTrue(merged.layers.get(reverse_layer).is_off())

            inserts_by_layer = {
                entity.dxf.layer: entity
                for entity in merged.modelspace()
                if entity.dxftype() == 'INSERT'
            }
            self.assertIn(normal_layer, inserts_by_layer)
            self.assertIn(reverse_layer, inserts_by_layer)
            reverse_block = merged.blocks.get(inserts_by_layer[reverse_layer].dxf.name)
            imported_lines = list(reverse_block.query('LINE'))
            self.assertEqual(len(imported_lines), 1)
            self.assertEqual(imported_lines[0].dxf.layer, 'KABELS')

    def test_reverse_image_dependency_is_preserved_and_relinked(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            directory = Path(temporary_directory)
            normal_path = directory / 'proefsleuven.dxf'
            reverse_path = directory / '.proefsleuven.sleufbase-reverse-source.dxf'
            source_png = directory / 'reverse-kaart.png'
            Image.new('RGB', (8, 6), 'white').save(source_png)

            normal_document = ezdxf.new('R2010')
            normal_modelspace = normal_document.modelspace()
            normal_line = normal_modelspace.add_line((0, 0), (1, 0))
            reverse_patch._move_entities_to_variant_container(
                normal_document,
                normal_modelspace,
                [normal_line],
                label='PS2',
                slot_index=2,
                mode=reverse_patch.NORMAL_MODE,
            )
            normal_document.saveas(normal_path)

            reverse_document = ezdxf.new('R2010')
            reverse_modelspace = reverse_document.modelspace()
            image_def = reverse_document.add_image_def(
                filename=str(source_png.resolve()),
                size_in_pixel=(8, 6),
            )
            reverse_image = reverse_modelspace.add_image(
                insert=(10, 20),
                size_in_units=(8, 6),
                image_def=image_def,
            )
            reverse_patch._move_entities_to_variant_container(
                reverse_document,
                reverse_modelspace,
                [reverse_image],
                label='PS2',
                slot_index=2,
                mode=reverse_patch.REVERSE_MODE,
            )
            reverse_document.saveas(reverse_path)

            self.assertEqual(
                reverse_patch._merge_reverse_variant_document(normal_path, reverse_path),
                1,
            )

            merged = ezdxf.readfile(normal_path)
            reverse_layer = reverse_patch.variant_layer_name(
                'PS2', 2, reverse_patch.REVERSE_MODE
            )
            reverse_insert = next(
                entity
                for entity in merged.modelspace()
                if entity.dxftype() == 'INSERT' and entity.dxf.layer == reverse_layer
            )
            reverse_block = merged.blocks.get(reverse_insert.dxf.name)
            images = list(reverse_block.query('IMAGE'))
            self.assertEqual(len(images), 1)
            linked_file = Path(images[0].image_def.dxf.filename)
            self.assertTrue(linked_file.exists())
            self.assertEqual(linked_file.parent, directory / 'proefsleuven_reverse_assets')
            self.assertEqual(linked_file.name, source_png.name)
            self.assertTrue(merged.layers.get(reverse_layer).is_off())


if __name__ == '__main__':
    unittest.main()
