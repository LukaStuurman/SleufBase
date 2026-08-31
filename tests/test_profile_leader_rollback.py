from __future__ import annotations

import unittest

from SleufBase.autocad_profile_leader_donor import BLOCK_NAME, DONOR_SHA256
from SleufBase.cadastral_export import CadastralDxfExporter


class ProfileLeaderDonorActivationTests(unittest.TestCase):
    def test_profile_leader_methods_use_approved_donor_patch(self) -> None:
        expected_module = "SleufBase.template_dynamic_profile_leader_patch"
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

    def test_approved_polar_profile_patch_is_active(self) -> None:
        self.assertTrue(CadastralDxfExporter.SLEUFBASE_DYNAMIC_PROFILE_LEADER_POLAR)
        self.assertEqual(
            CadastralDxfExporter.SLEUFBASE_DYNAMIC_PROFILE_LEADER_BLOCK,
            BLOCK_NAME,
        )
        self.assertEqual(
            CadastralDxfExporter.SLEUFBASE_DYNAMIC_PROFILE_LEADER_DONOR_SHA256,
            DONOR_SHA256,
        )
        self.assertGreaterEqual(
            CadastralDxfExporter._sleufbase_dynamic_profile_leader_patch_version,
            3,
        )


if __name__ == "__main__":
    unittest.main()
