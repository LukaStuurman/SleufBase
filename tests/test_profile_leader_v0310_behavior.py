from __future__ import annotations

import unittest

from SleufBase.cadastral_export import CadastralDxfExporter


class ProfileLeaderV0310BehaviorTests(unittest.TestCase):
    def test_profile_leader_methods_use_original_cadastral_export_implementation(self) -> None:
        expected_module = "SleufBase.cadastral_export"
        self.assertEqual(
            CadastralDxfExporter._remove_template_legacy_profile_leader_blocks.__module__,
            expected_module,
        )
        self.assertEqual(
            CadastralDxfExporter._add_template_profile_multileader.__module__,
            expected_module,
        )
        self.assertEqual(
            CadastralDxfExporter._distribute_template_leader_labels.__module__,
            expected_module,
        )
        self.assertEqual(
            CadastralDxfExporter._add_template_profile_leader_marker.__module__,
            expected_module,
        )

    def test_post_v0310_profile_leader_patches_are_not_active(self) -> None:
        self.assertFalse(hasattr(CadastralDxfExporter, "SLEUFBASE_DYNAMIC_PROFILE_LEADER_POLAR"))
        self.assertFalse(hasattr(CadastralDxfExporter, "SLEUFBASE_DYNAMIC_PROFILE_LEADER_PRESERVES_TEMPLATE_DEFINITION"))
        self.assertFalse(hasattr(CadastralDxfExporter, "SLEUFBASE_TOP_LEVEL_PROFILE_LEADER_BLOCK"))
        self.assertFalse(hasattr(CadastralDxfExporter, "SLEUFBASE_PROFILE_LEADER_CIRCLE_LAYER"))


if __name__ == "__main__":
    unittest.main()
