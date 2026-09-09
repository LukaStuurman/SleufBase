from __future__ import annotations

import unittest
from pathlib import Path
from types import SimpleNamespace

from SleufBase.cadastral_export import CadastralDxfExporter
from SleufBase.dxf_template_pipeline_v5_patch import (
    _UNSUPPORTED,
    _augment_profile_with_polylines,
    _build_forced_profile_fast,
    _reverse_profile_from_normal,
)
from SleufBase.kickthemap_dxf_export import (
    KickTheMapObjectDataset,
    KickTheMapObjectPoint,
    build_object_layer_rules,
)


def _dataset(*, forced_start=None) -> KickTheMapObjectDataset:
    return KickTheMapObjectDataset(
        job_id=42,
        job_title="PS 1",
        source_path=Path("jobFeatures.json"),
        points=(
            KickTheMapObjectPoint("Begin", "0", 0.0, 0.0, 3.20),
            KickTheMapObjectPoint("Water", "water", 3.25, 0.0, 2.35, "PVC", "110", ""),
            KickTheMapObjectPoint("Data", "data", 7.50, 0.0, 2.55, "PE", "50", ""),
            KickTheMapObjectPoint("Eind", "0", 10.0, 0.0, 3.40),
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


def _distance_offsets(exporter, layer, profile):
    reference_chainage = exporter._template_reference_chainage(layer, profile)
    return {
        str(getattr(item.point, "object_name", "")): round(
            float(item.chainage) - float(reference_chainage),
            7,
        )
        for item in profile.points
        if not item.is_endpoint
    }


class DxfTemplatePipelineV5Tests(unittest.TestCase):
    def setUp(self) -> None:
        self.exporter = CadastralDxfExporter(SimpleNamespace())
        self.rules = build_object_layer_rules()

    def test_pipeline_v5_is_installed(self) -> None:
        self.assertGreaterEqual(
            int(getattr(CadastralDxfExporter, "_sleufbase_dxf_template_pipeline_v5_version", 0) or 0),
            1,
        )
        self.assertTrue(getattr(CadastralDxfExporter, "SLEUFBASE_REVERSE_PROFILE_REUSE", False))
        self.assertTrue(getattr(CadastralDxfExporter, "SLEUFBASE_FORCED_PROFILE_FAST_PATH", False))
        self.assertTrue(getattr(CadastralDxfExporter, "SLEUFBASE_DEKBAND_PROFILE_AUGMENT", False))

    def test_reverse_profile_derived_from_normal_matches_core_reverse(self) -> None:
        dataset = _dataset()
        normal = self.exporter._build_template_cross_section_profile(
            dataset, self.rules, [], [], 0.02, False
        )
        expected_reverse = self.exporter._build_template_cross_section_profile(
            dataset, self.rules, [], [], 0.02, True
        )
        derived_reverse = _reverse_profile_from_normal(normal)
        self.assertEqual(_snapshot(derived_reverse), _snapshot(expected_reverse))

    def test_reverse_distance_reference_chainage_is_recalculated_per_direction(self) -> None:
        dataset = _dataset()
        normal = self.exporter._build_template_cross_section_profile(
            dataset, self.rules, [], [], 0.02, False
        )
        reverse = _reverse_profile_from_normal(normal)
        layer = SimpleNamespace(
            metadata={
                self.exporter.TEMPLATE_REFERENCE_POINT_METADATA_KEY: {
                    "x": 2.0,
                    "y": 0.0,
                }
            }
        )

        normal_reference = self.exporter._template_reference_chainage(layer, normal)
        reverse_reference = self.exporter._template_reference_chainage(layer, reverse)

        self.assertAlmostEqual(normal_reference, 2.0)
        self.assertAlmostEqual(reverse_reference, 8.0)

    def test_reverse_distance_labels_match_full_core_reverse_and_not_normal_values(self) -> None:
        dataset = _dataset()
        normal = self.exporter._build_template_cross_section_profile(
            dataset, self.rules, [], [], 0.02, False
        )
        expected_reverse = self.exporter._build_template_cross_section_profile(
            dataset, self.rules, [], [], 0.02, True
        )
        derived_reverse = _reverse_profile_from_normal(normal)
        layer = SimpleNamespace(
            metadata={
                self.exporter.TEMPLATE_REFERENCE_POINT_METADATA_KEY: {
                    "x": 2.0,
                    "y": 0.0,
                }
            }
        )

        normal_offsets = _distance_offsets(self.exporter, layer, normal)
        expected_reverse_offsets = _distance_offsets(
            self.exporter, layer, expected_reverse
        )
        derived_reverse_offsets = _distance_offsets(
            self.exporter, layer, derived_reverse
        )

        self.assertEqual(derived_reverse_offsets, expected_reverse_offsets)
        self.assertNotEqual(derived_reverse_offsets, normal_offsets)
        self.assertEqual(normal_offsets["Water"], 1.25)
        self.assertEqual(derived_reverse_offsets["Water"], -1.25)
        self.assertEqual(normal_offsets["Data"], 5.5)
        self.assertEqual(derived_reverse_offsets["Data"], -5.5)

    def test_forced_start_fast_path_matches_core_profile(self) -> None:
        dataset = _dataset(forced_start=(0.0, 0.0))
        expected = self.exporter._build_template_cross_section_profile(
            dataset, self.rules, [], [], 0.02, False
        )
        fast = _build_forced_profile_fast(self.exporter, dataset, self.rules, 0.02)
        self.assertIsNot(fast, _UNSUPPORTED)
        self.assertEqual(_snapshot(fast), _snapshot(expected))

    def test_forced_reverse_start_fast_path_matches_core_profile(self) -> None:
        dataset = _dataset(forced_start=(10.0, 0.0))
        expected = self.exporter._build_template_cross_section_profile(
            dataset, self.rules, [], [], 0.02, True
        )
        fast = _build_forced_profile_fast(self.exporter, dataset, self.rules, 0.02)
        self.assertIsNot(fast, _UNSUPPORTED)
        self.assertEqual(_snapshot(fast), _snapshot(expected))

    def test_dekband_profile_augmentation_matches_full_rebuild(self) -> None:
        dataset = _dataset()
        base = self.exporter._build_template_cross_section_profile(
            dataset, self.rules, [], [], 0.02, False
        )
        polyline = self.exporter._template_dekband_polyline(
            {
                "start_chainage": 2.0,
                "end_chainage": 6.5,
                "start_depth": 0.25,
                "end_depth": 0.35,
                "source_name": "Betondekband",
            },
            base,
            1,
        )
        self.assertIsNotNone(polyline)
        prepared = KickTheMapObjectDataset(
            job_id=dataset.job_id,
            job_title=dataset.job_title,
            source_path=dataset.source_path,
            points=dataset.points,
            polylines=(polyline,),
            cross_section_start_xy=dataset.cross_section_start_xy,
        )
        expected = self.exporter._build_template_cross_section_profile(
            prepared, self.rules, [], [], 0.02, False
        )
        augmented = _augment_profile_with_polylines(
            self.exporter, base, (polyline,), self.rules
        )
        self.assertEqual(_snapshot(augmented), _snapshot(expected))


if __name__ == "__main__":
    unittest.main()
