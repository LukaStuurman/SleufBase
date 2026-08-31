from __future__ import annotations

import unittest

import ezdxf

from SleufBase import template_reverse_patch as reverse_patch
from SleufBase.autocad_profile_leader_donor import BLOCK_NAME


class NestedProfileLeaderVariantTests(unittest.TestCase):
    def test_top_level_workaround_is_no_longer_installed(self) -> None:
        self.assertEqual(
            reverse_patch._move_entities_to_variant_container.__module__,
            "SleufBase.template_reverse_patch",
        )
        self.assertFalse(hasattr(reverse_patch, "SLEUFBASE_TOP_LEVEL_PROFILE_LEADER_BLOCK"))

    def test_normal_and_reverse_keep_their_own_profile_leader_positions(self) -> None:
        document = ezdxf.new("R2018")
        modelspace = document.modelspace()
        document.blocks.new(BLOCK_NAME)

        normal_leader = modelspace.add_blockref(
            BLOCK_NAME,
            insert=(10.0, 20.0, 0.0),
            dxfattribs={"layer": "X-XX-AL-VERWIJZING-SD"},
        )
        reverse_leader = modelspace.add_blockref(
            BLOCK_NAME,
            insert=(30.0, 40.0, 0.0),
            dxfattribs={"layer": "X-XX-AL-VERWIJZING-SD"},
        )

        reverse_patch._move_entities_to_variant_container(
            document,
            modelspace,
            [normal_leader],
            label="PS1",
            slot_index=1,
            mode=reverse_patch.NORMAL_MODE,
        )
        reverse_patch._move_entities_to_variant_container(
            document,
            modelspace,
            [reverse_leader],
            label="PS1",
            slot_index=1,
            mode=reverse_patch.REVERSE_MODE,
        )

        self.assertNotIn(normal_leader, list(modelspace))
        self.assertNotIn(reverse_leader, list(modelspace))

        normal_container = document.blocks.get(
            reverse_patch.variant_block_name("PS1", 1, reverse_patch.NORMAL_MODE)
        )
        reverse_container = document.blocks.get(
            reverse_patch.variant_block_name("PS1", 1, reverse_patch.REVERSE_MODE)
        )

        self.assertIn(normal_leader, list(normal_container))
        self.assertNotIn(normal_leader, list(reverse_container))
        self.assertIn(reverse_leader, list(reverse_container))
        self.assertNotIn(reverse_leader, list(normal_container))

        self.assertAlmostEqual(float(normal_leader.dxf.insert.x), 10.0)
        self.assertAlmostEqual(float(normal_leader.dxf.insert.y), 20.0)
        self.assertAlmostEqual(float(reverse_leader.dxf.insert.x), 30.0)
        self.assertAlmostEqual(float(reverse_leader.dxf.insert.y), 40.0)


if __name__ == "__main__":
    unittest.main()
