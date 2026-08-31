from __future__ import annotations

import unittest

import ezdxf

from SleufBase import template_reverse_patch as reverse_patch
from SleufBase.autocad_profile_leader_donor import BLOCK_NAME, DONOR_SHA256
from SleufBase.top_level_profile_leader_patch import _is_profile_leader_insert


class TopLevelProfileLeaderPatchTests(unittest.TestCase):
    def test_patch_is_installed_after_dynamic_leader_patch(self) -> None:
        self.assertEqual(
            reverse_patch._move_entities_to_variant_container.__module__,
            "SleufBase.top_level_profile_leader_patch",
        )
        self.assertEqual(
            reverse_patch.SLEUFBASE_TOP_LEVEL_PROFILE_LEADER_BLOCK,
            BLOCK_NAME,
        )
        self.assertEqual(
            reverse_patch.SLEUFBASE_TOP_LEVEL_PROFILE_LEADER_DONOR_SHA256,
            DONOR_SHA256,
        )
        self.assertGreaterEqual(
            reverse_patch._sleufbase_top_level_profile_leader_patch_version,
            1,
        )

    def test_dynamic_leader_stays_in_modelspace_while_other_entities_are_nested(self) -> None:
        document = ezdxf.new("R2018")
        modelspace = document.modelspace()
        document.blocks.new(BLOCK_NAME)

        leader = modelspace.add_blockref(
            BLOCK_NAME,
            insert=(10.0, 20.0, 0.0),
            dxfattribs={"layer": "X-XX-AL-VERWIJZING-SD"},
        )
        ordinary = modelspace.add_line((0.0, 0.0), (1.0, 1.0))

        reverse_patch._move_entities_to_variant_container(
            document,
            modelspace,
            [leader, ordinary],
            label="PS1",
            slot_index=1,
            mode=reverse_patch.NORMAL_MODE,
        )

        modelspace_entities = list(modelspace)
        self.assertIn(leader, modelspace_entities)
        self.assertTrue(_is_profile_leader_insert(leader))
        self.assertNotIn(ordinary, modelspace_entities)

        container_name = reverse_patch.variant_block_name(
            "PS1",
            1,
            reverse_patch.NORMAL_MODE,
        )
        container = document.blocks.get(container_name)
        self.assertIn(ordinary, list(container))
        self.assertFalse(
            any(_is_profile_leader_insert(entity) for entity in container)
        )

    def test_reverse_merge_filter_will_not_pick_top_level_profile_leader(self) -> None:
        document = ezdxf.new("R2018")
        document.blocks.new(BLOCK_NAME)
        leader = document.modelspace().add_blockref(
            BLOCK_NAME,
            insert=(0.0, 0.0),
            dxfattribs={"layer": "X-XX-AL-VERWIJZING-SD"},
        )
        self.assertFalse(
            reverse_patch._is_variant_layer(
                leader.dxf.layer,
                reverse_patch.REVERSE_MODE,
            )
        )


if __name__ == "__main__":
    unittest.main()
