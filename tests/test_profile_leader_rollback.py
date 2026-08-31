from __future__ import annotations

import unittest

from SleufBase.cadastral_export import CadastralDxfExporter


class ProfileLeaderRollbackTests(unittest.TestCase):
    def test_profile_leader_methods_are_the_original_v0310_exporter_methods(self) -> None:
        self.assertEqual(
            CadastralDxfExporter._remove_template_legacy_profile_leader_blocks.__module__,
            "SleufBase.cadastral_export",
        )
        self.assertEqual(
            CadastralDxfExporter._add_template_profile_multileader.__module__,
            "SleufBase.cadastral_export",
        )
        self.assertEqual(
            CadastralDxfExporter._distribute_template_leader_labels.__module__,
            "SleufBase.cadastral_export",
        )
        self.assertEqual(
            CadastralDxfExporter._add_template_profile_leader_marker.__module__,
            "SleufBase.cadastral_export",
        )

    def test_no_later_polar_profile_patch_is_active(self) -> None:
        self.assertFalse(
            hasattr(CadastralDxfExporter, "SLEUFBASE_DYNAMIC_PROFILE_LEADER_POLAR")
        )
        self.assertFalse(
            hasattr(CadastralDxfExporter, "SLEUFBASE_DYNAMIC_PROFILE_LEADER_BLOCK")
        )
        self.assertFalse(
            hasattr(CadastralDxfExporter, "_sleufbase_dynamic_profile_leader_patch_version")
        )


if __name__ == "__main__":
    unittest.main()
