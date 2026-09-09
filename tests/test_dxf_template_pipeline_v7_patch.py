from __future__ import annotations

from pathlib import Path
import tempfile
import threading
import unittest
from types import SimpleNamespace

import ezdxf

from SleufBase.cadastral_export import CadastralDxfExporter
from SleufBase import dxf_template_pipeline_patch as pipeline
from SleufBase import dxf_template_pipeline_v4_patch as pipeline_v4
from SleufBase import dxf_template_pipeline_v5_patch as pipeline_v5
from SleufBase import dxf_template_pipeline_v7_patch as pipeline_v7
from SleufBase import template_reverse_patch as reverse_patch
from SleufBase.kickthemap_dxf_export import (
    KickTheMapObjectDataset,
    KickTheMapObjectPoint,
    build_object_layer_rules,
)


def _dataset(*, forced_start=(10.0, 0.0)) -> KickTheMapObjectDataset:
    return KickTheMapObjectDataset(
        job_id=142,
        job_title="PS v7 maaiveld",
        source_path=Path("jobFeatures.json"),
        points=(
            KickTheMapObjectPoint("Begin", "0", 0.0, 0.0, 3.12),
            KickTheMapObjectPoint("Water", "water", 3.0, 0.0, 2.30, "PVC", "110", ""),
            KickTheMapObjectPoint("Data", "data", 7.0, 0.0, 2.48, "PE", "50", ""),
            KickTheMapObjectPoint("Eind", "0", 10.0, 0.0, 3.67),
        ),
        cross_section_start_xy=forced_start,
    )


def _snapshot(profile):
    if profile is None:
        return None
    return {
        "start": (
            round(float(profile.start_point.x), 7),
            round(float(profile.start_point.y), 7),
            round(float(profile.start_point.z or 0.0), 7),
        ),
        "end": (
            round(float(profile.end_point.x), 7),
            round(float(profile.end_point.y), 7),
            round(float(profile.end_point.z or 0.0), 7),
        ),
        "axis": (
            round(float(profile.axis_dx), 7),
            round(float(profile.axis_dy), 7),
            round(float(profile.axis_length), 7),
        ),
        "reference": round(float(profile.reference_level), 7),
        "points": tuple(
            (
                round(float(item.chainage), 7),
                round(float(item.point_z), 7),
                str(item.layer_name),
                int(item.color),
                str(item.description),
                bool(item.is_endpoint),
                str(getattr(item.point, "object_name", "")),
            )
            for item in profile.points
        ),
    }


class DxfTemplatePipelineV7Tests(unittest.TestCase):
    def setUp(self) -> None:
        self.exporter = CadastralDxfExporter(SimpleNamespace())
        self.rules = build_object_layer_rules()

    def tearDown(self) -> None:
        for name in (
            "pair_cache",
            "capture_normal_save",
            "capture_normal_path",
        ):
            reverse_patch._EXPORT_CONTEXT.__dict__.pop(name, None)
        for name in (pipeline_v4._SESSION_ATTR, pipeline_v4._MODE_ATTR):
            try:
                delattr(self.exporter, name)
            except AttributeError:
                pass

    def _activate_reverse_pair(self):
        session = {"lock": threading.RLock()}
        pair_cache = {"values": {}, "phases": {}, "counters": {}}
        setattr(self.exporter, pipeline_v4._SESSION_ATTR, session)
        setattr(self.exporter, pipeline_v4._MODE_ATTR, reverse_patch.REVERSE_MODE)
        reverse_patch._EXPORT_CONTEXT.pair_cache = pair_cache
        return session, pair_cache

    def test_v7_is_installed_and_keeps_reverse_independent(self) -> None:
        self.assertGreaterEqual(
            int(getattr(CadastralDxfExporter, "_sleufbase_dxf_template_pipeline_v7_version", 0) or 0),
            1,
        )
        self.assertTrue(CadastralDxfExporter.SLEUFBASE_NORMAL_DXF_IN_MEMORY)
        self.assertFalse(CadastralDxfExporter.SLEUFBASE_REVERSE_PROFILE_REUSE)
        self.assertTrue(CadastralDxfExporter.SLEUFBASE_REVERSE_PROFILE_INDEPENDENT_BUILD)
        self.assertTrue(CadastralDxfExporter.SLEUFBASE_REVERSE_FORCED_PROFILE_FAST_PATH)

    def test_normal_saveas_is_captured_without_disk_roundtrip(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            target = Path(temporary_directory) / "normal.dxf"
            cache = {"values": {}, "phases": {}, "counters": {}}
            reverse_patch._EXPORT_CONTEXT.pair_cache = cache
            reverse_patch._EXPORT_CONTEXT.capture_normal_save = True
            reverse_patch._EXPORT_CONTEXT.capture_normal_path = target

            document = ezdxf.new("R2018")
            document.modelspace().add_line((0, 0), (1, 1))
            document.saveas(target)

            self.assertFalse(target.exists())
            self.assertIs(cache.get("normal_document"), document)
            self.assertEqual(Path(cache.get("normal_document_path")), target)
            self.assertGreaterEqual(cache["counters"].get("normal_dxf_writes_skipped", 0), 1)

            captured = pipeline_v7._captured_normal_document(target)
            self.assertIs(captured, document)
            self.assertGreaterEqual(cache["counters"].get("normal_dxf_reads_skipped", 0), 1)

    def test_reverse_forced_fast_path_matches_v6_full_core_with_different_ground_levels(self) -> None:
        dataset = _dataset(forced_start=(10.0, 0.0))
        session, pair_cache = self._activate_reverse_pair()

        expected = pipeline_v7._V6_PROFILE_BUILDER(
            self.exporter,
            dataset,
            self.rules,
            [],
            [],
            0.02,
            True,
        )
        self.assertIs(getattr(self.exporter, pipeline_v4._SESSION_ATTR), session)

        original_mirror = pipeline_v5._reverse_profile_from_normal

        def forbidden_mirror(_profile):
            raise AssertionError("V7 reverse fast path must not mirror normal geometry")

        pipeline_v5._reverse_profile_from_normal = forbidden_mirror
        try:
            actual = self.exporter._build_template_cross_section_profile(
                dataset,
                self.rules,
                [],
                [],
                0.02,
                True,
            )
        finally:
            pipeline_v5._reverse_profile_from_normal = original_mirror

        self.assertEqual(_snapshot(actual), _snapshot(expected))
        self.assertAlmostEqual(float(actual.start_point.z), 3.67)
        self.assertAlmostEqual(float(actual.end_point.z), 3.12)
        self.assertGreaterEqual(
            pair_cache["counters"].get("reverse_forced_profile_fast_path_v7", 0),
            1,
        )

    def test_profile_rule_and_description_resolution_are_cached_within_pair(self) -> None:
        dataset = _dataset(forced_start=(10.0, 0.0))
        _session, pair_cache = self._activate_reverse_pair()
        point = dataset.points[1]

        first_rule = self.exporter._resolve_cross_section_point_rule(point, self.rules)
        second_rule = self.exporter._resolve_cross_section_point_rule(point, self.rules)
        self.assertIs(first_rule, second_rule)

        first_text = self.exporter._cross_section_description(point, "WATER", "Water")
        second_text = self.exporter._cross_section_description(point, "WATER", "Water")
        self.assertEqual(first_text, second_text)
        self.assertGreaterEqual(pair_cache["counters"].get("profile_rule_cache_hits_v7", 0), 1)
        self.assertGreaterEqual(
            pair_cache["counters"].get("profile_description_cache_hits_v7", 0),
            1,
        )

    def test_nonforced_reverse_still_uses_v6_independent_core_path(self) -> None:
        dataset = _dataset(forced_start=None)
        session, _pair_cache = self._activate_reverse_pair()

        original_fast = pipeline_v5._build_forced_profile_fast

        def forbidden_fast(*_args, **_kwargs):
            raise AssertionError("non-forced reverse must stay on V6/core path")

        pipeline_v5._build_forced_profile_fast = forbidden_fast
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
            pipeline_v5._build_forced_profile_fast = original_fast

        self.assertIsNotNone(profile)
        self.assertIs(getattr(self.exporter, pipeline_v4._SESSION_ATTR), session)
        self.assertAlmostEqual(float(profile.start_point.z), 3.67)
        self.assertAlmostEqual(float(profile.end_point.z), 3.12)


if __name__ == "__main__":
    unittest.main()
