from __future__ import annotations

import threading
import unittest
from pathlib import Path
from types import SimpleNamespace

from SleufBase.cadastral_export import CadastralDxfExporter
from SleufBase import dxf_template_pipeline_v4_patch as pipeline_v4
from SleufBase import dxf_template_pipeline_v5_patch as pipeline_v5
from SleufBase import template_reverse_patch as reverse_patch
from SleufBase.kickthemap_dxf_export import (
    KickTheMapObjectDataset,
    KickTheMapObjectPoint,
    build_object_layer_rules,
)


def _dataset() -> KickTheMapObjectDataset:
    return KickTheMapObjectDataset(
        job_id=84,
        job_title="PS maaiveld",
        source_path=Path("jobFeatures.json"),
        points=(
            KickTheMapObjectPoint("Begin", "0", 0.0, 0.0, 3.12),
            KickTheMapObjectPoint("Water", "water", 3.0, 0.0, 2.30, "PVC", "110", ""),
            KickTheMapObjectPoint("Data", "data", 7.0, 0.0, 2.48, "PE", "50", ""),
            KickTheMapObjectPoint("Eind", "0", 10.0, 0.0, 3.67),
        ),
    )


class DxfTemplatePipelineV6Tests(unittest.TestCase):
    def setUp(self) -> None:
        self.exporter = CadastralDxfExporter(SimpleNamespace())
        self.rules = build_object_layer_rules()

    def test_v6_is_installed_and_requires_reverse_full_rebuild(self) -> None:
        self.assertGreaterEqual(
            int(getattr(CadastralDxfExporter, "_sleufbase_dxf_template_pipeline_v6_version", 0) or 0),
            1,
        )
        self.assertFalse(getattr(CadastralDxfExporter, "SLEUFBASE_REVERSE_PROFILE_REUSE", True))
        self.assertTrue(
            getattr(CadastralDxfExporter, "SLEUFBASE_REVERSE_PROFILE_FULL_REBUILD", False)
        )

    def test_reverse_pair_export_never_calls_v5_mirror_helper(self) -> None:
        dataset = _dataset()
        session = {"lock": threading.RLock()}
        setattr(self.exporter, pipeline_v4._SESSION_ATTR, session)
        setattr(self.exporter, pipeline_v4._MODE_ATTR, reverse_patch.REVERSE_MODE)

        original_mirror = pipeline_v5._reverse_profile_from_normal

        def forbidden_mirror(_profile):
            raise AssertionError("reverse geometry must not be derived from normal")

        pipeline_v5._reverse_profile_from_normal = forbidden_mirror
        try:
            profile = self.exporter._build_template_cross_section_profile(
                dataset,
                self.rules,
                [],
                [],
                0.02,
                True,
            )
        finally:
            pipeline_v5._reverse_profile_from_normal = original_mirror

        self.assertIsNotNone(profile)
        self.assertIs(getattr(self.exporter, pipeline_v4._SESSION_ATTR), session)
        self.assertAlmostEqual(float(profile.start_point.z), 3.67)
        self.assertAlmostEqual(float(profile.end_point.z), 3.12)

    def test_normal_profile_keeps_v5_fast_path_available(self) -> None:
        dataset = _dataset()
        profile = self.exporter._build_template_cross_section_profile(
            dataset,
            self.rules,
            [],
            [],
            0.02,
            False,
        )
        self.assertIsNotNone(profile)
        self.assertAlmostEqual(float(profile.start_point.z), 3.12)
        self.assertAlmostEqual(float(profile.end_point.z), 3.67)


if __name__ == "__main__":
    unittest.main()
