from __future__ import annotations

import unittest

import ezdxf

from SleufBase import template_reverse_patch as reverse_patch
from SleufBase.autocad_profile_leader_donor import BLOCK_NAME
from SleufBase.top_level_profile_leader_patch import _is_profile_leader_insert


class TopLevelProfileLeaderVariantTests(unittest.TestCase):
    def test_top_level_variant_patch_is_installed(self) -> None:
        self.assertEqual(
            reverse_patch._move_entities_to_variant_container.__module__,
            "SleufBase.top_level_profile_leader_patch",
        )
        self.assertEqual(reverse_patch.SLEUFBASE_TOP_LEVEL_PROFILE_LEADER_BLOCK, BLOCK_NAME)
        self.assertTrue(reverse_patch.SLEUFBASE_TOP_LEVEL_PROFILE_LEADER_VARIANT_LAYERS)
        self.assertGreaterEqual(
            reverse_patch._sleufbase_top_level_profile_leader_patch_version,
            2,
        )

    def test_normal_and_reverse_leaders_stay_top_level_on_their_variant_layers(self) -> None:
        document = ezdxf.new("R2018")
        modelspace = document.modelspace()
        document.blocks.new(BLOCK_NAME)

        normal_leader = modelspace.add_blockref(
            BLOCK_NAME,
            insert=(10.0, 20.0, 0.0),
            dxfattribs={"layer": "X-XX-AL-VERWIJZING-SD", "color": 3},
        )
        reverse_leader = modelspace.add_blockref(
            BLOCK_NAME,
            insert=(30.0, 40.0, 0.0),
            dxfattribs={"layer": "X-XX-AL-VERWIJZING-SD", "color": 5},
        )
        ordinary_normal = modelspace.add_line((0.0, 0.0), (1.0, 1.0))
        ordinary_reverse = modelspace.add_line((2.0, 2.0), (3.0, 3.0))

        reverse_patch._move_entities_to_variant_container(
            document,
            modelspace,
            [normal_leader, ordinary_normal],
            label="PS1",
            slot_index=1,
            mode=reverse_patch.NORMAL_MODE,
        )
        reverse_patch._move_entities_to_variant_container(
            document,
            modelspace,
            [reverse_leader, ordinary_reverse],
            label="PS1",
            slot_index=1,
            mode=reverse_patch.REVERSE_MODE,
        )

        normal_layer = reverse_patch.variant_layer_name("PS1", 1, reverse_patch.NORMAL_MODE)
        reverse_layer = reverse_patch.variant_layer_name("PS1", 1, reverse_patch.REVERSE_MODE)

        self.assertIn(normal_leader, list(modelspace))
        self.assertIn(reverse_leader, list(modelspace))
        self.assertTrue(_is_profile_leader_insert(normal_leader, document))
        self.assertTrue(_is_profile_leader_insert(reverse_leader, document))
        self.assertEqual(str(normal_leader.dxf.layer), normal_layer)
        self.assertEqual(str(reverse_leader.dxf.layer), reverse_layer)
        self.assertEqual(int(normal_leader.dxf.color), 3)
        self.assertEqual(int(reverse_leader.dxf.color), 5)

        normal_container = document.blocks.get(
            reverse_patch.variant_block_name("PS1", 1, reverse_patch.NORMAL_MODE)
        )
        reverse_container = document.blocks.get(
            reverse_patch.variant_block_name("PS1", 1, reverse_patch.REVERSE_MODE)
        )
        self.assertIn(ordinary_normal, list(normal_container))
        self.assertIn(ordinary_reverse, list(reverse_container))
        self.assertFalse(any(_is_profile_leader_insert(entity, document) for entity in normal_container))
        self.assertFalse(any(_is_profile_leader_insert(entity, document) for entity in reverse_container))

        self.assertAlmostEqual(float(normal_leader.dxf.insert.x), 10.0)
        self.assertAlmostEqual(float(normal_leader.dxf.insert.y), 20.0)
        self.assertAlmostEqual(float(reverse_leader.dxf.insert.x), 30.0)
        self.assertAlmostEqual(float(reverse_leader.dxf.insert.y), 40.0)

        # The normal/reverse visibility mechanism can now control the directly
        # selectable references without nesting away their native Polar grips.
        self.assertTrue(reverse_patch._is_variant_layer(normal_leader.dxf.layer, reverse_patch.NORMAL_MODE))
        self.assertTrue(reverse_patch._is_variant_layer(reverse_leader.dxf.layer, reverse_patch.REVERSE_MODE))
        self.assertFalse(document.layers.get(normal_layer).is_off())
        self.assertTrue(document.layers.get(reverse_layer).is_off())


if __name__ == "__main__":
    unittest.main()
