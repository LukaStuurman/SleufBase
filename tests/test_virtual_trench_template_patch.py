from __future__ import annotations

from math import hypot
import unittest

from SleufBase.cadastral_export import CadastralDxfExporter
from SleufBase.marxact_import import MarXactObject, MarXactTrench, build_marxact_virtual_layer
from SleufBase.virtual_trench import (
    VIRTUAL_TRENCH_METADATA_KEY,
    build_virtual_trench_dataset,
    virtual_trench_centerline,
    virtual_trench_endpoints,
)
from SleufBase.virtual_trench_template_patch import (
    MISSING_KICKTHEMAP_JOB_TEXT,
    VIRTUAL_TEMPLATE_DATASET_ID_KEY,
    _augment_virtual_template_datasets,
    _filter_virtual_missing_job_warning,
    _restore_virtual_template_dataset_ids,
)


class VirtualTrenchTemplatePatchTests(unittest.TestCase):
    @staticmethod
    def _marxact_layer():
        # Polygon is wider east-west, while the measured cable/pipe blocks run
        # north-south. The MarXact direction patch therefore makes the virtual
        # trench axis north-south, exactly as the template image/profile should.
        trench = MarXactTrench(
            name="ps-template",
            polygon=(
                (0.0, 0.0, 10.0),
                (8.0, 0.0, 10.8),
                (8.0, 3.0, 11.2),
                (0.0, 3.0, 10.4),
            ),
            objects=[
                MarXactObject(4.0, 0.5, 9.4, "Water", "water", 9.4, "marxact_point"),
                MarXactObject(4.0, 1.5, 9.2, "Laagspanning", "ls", 9.2, "marxact_point"),
                MarXactObject(4.0, 2.5, 9.3, "Datatransport", "data", 9.3, "marxact_point"),
            ],
        )
        return build_marxact_virtual_layer(
            trench,
            source_path="template-profile-test.dxf",
            source_name_resolver=lambda item: item.mapping_name,
            fallback_index=1,
        )

    def test_runtime_patch_is_installed(self) -> None:
        self.assertGreaterEqual(
            int(
                getattr(
                    CadastralDxfExporter,
                    "_sleufbase_virtual_trench_template_patch_version",
                    0,
                )
                or 0
            ),
            2,
        )

    def test_marxact_virtual_dataset_builds_profile_without_kickthemap_job(self) -> None:
        layer = self._marxact_layer()
        self.assertNotIn("kickthemap_job_id", layer.metadata)
        dataset = build_virtual_trench_dataset(layer, include_endpoints=True)
        self.assertIsNotNone(dataset)
        assert dataset is not None
        self.assertEqual(dataset.job_id, -1)
        self.assertIsNotNone(dataset.cross_section_start_xy)

        exporter = CadastralDxfExporter(wfs_client=object())
        profile = exporter._build_template_cross_section_profile(
            dataset,
            (),
            [],
            [],
            0.02,
        )
        self.assertIsNotNone(profile)
        assert profile is not None

        payload = layer.metadata[VIRTUAL_TRENCH_METADATA_KEY]
        start = next(point for point in payload["points"] if point.get("role") == "start")
        end = next(point for point in payload["points"] if point.get("role") == "end")
        self.assertAlmostEqual(profile.start_point.x, float(start["x"]), places=6)
        self.assertAlmostEqual(profile.start_point.y, float(start["y"]), places=6)
        self.assertAlmostEqual(float(profile.start_point.z or 0.0), float(start["z"]), places=6)
        self.assertAlmostEqual(float(profile.end_point.z or 0.0), float(end["z"]), places=6)

        # The existing virtual-trench profile builder reserves half the marker
        # diameter past the far endpoint. That can extend the displayed profile
        # by 1 cm for a 0.02 m marker, but it must stay exactly on the measured
        # MarXact start->end axis and may never meaningfully lengthen it.
        measured_dx = float(end["x"]) - float(start["x"])
        measured_dy = float(end["y"]) - float(start["y"])
        measured_length = hypot(measured_dx, measured_dy)
        profile_dx = float(profile.end_point.x) - float(profile.start_point.x)
        profile_dy = float(profile.end_point.y) - float(profile.start_point.y)
        profile_length = hypot(profile_dx, profile_dy)
        cross_track = abs((profile_dx * measured_dy) - (profile_dy * measured_dx)) / measured_length
        self.assertAlmostEqual(cross_track, 0.0, places=6)
        self.assertGreaterEqual(profile_length + 1e-9, measured_length)
        self.assertLessEqual(profile_length - measured_length, 0.010001)
        self.assertEqual(len([point for point in profile.points if not point.is_endpoint]), 3)

    def test_virtual_dataset_is_injected_with_private_id_and_restored(self) -> None:
        layer = self._marxact_layer()
        exporter = CadastralDxfExporter(wfs_client=object())
        datasets, restore_entries = _augment_virtual_template_datasets(
            exporter,
            [layer],
            {},
            lambda _exporter, _layer: None,
        )
        self.assertEqual(len(datasets), 1)
        synthetic_id = next(iter(datasets))
        self.assertLess(synthetic_id, 0)
        self.assertEqual(layer.metadata[VIRTUAL_TEMPLATE_DATASET_ID_KEY], synthetic_id)
        self.assertEqual(exporter._kickthemap_job_id(layer), synthetic_id)
        self.assertEqual(datasets[synthetic_id].job_id, synthetic_id)

        _restore_virtual_template_dataset_ids(restore_entries)
        self.assertNotIn(VIRTUAL_TEMPLATE_DATASET_ID_KEY, layer.metadata)
        self.assertIsNone(exporter._kickthemap_job_id(layer))

    def test_reverse_virtual_dataset_forces_measured_end_as_profile_start(self) -> None:
        layer = self._marxact_layer()
        exporter = CadastralDxfExporter(wfs_client=object())
        datasets, restore_entries = _augment_virtual_template_datasets(
            exporter,
            [layer],
            {},
            lambda _exporter, _layer: None,
            reverse_cross_sections=True,
        )
        try:
            self.assertEqual(len(datasets), 1)
            dataset = next(iter(datasets.values()))
            start_point, end_point = virtual_trench_endpoints(layer)
            assert start_point is not None and end_point is not None
            self.assertEqual(
                dataset.cross_section_start_xy,
                (float(end_point["x"]), float(end_point["y"])),
            )

            profile = exporter._build_template_cross_section_profile(
                dataset,
                (),
                [],
                [],
                0.02,
            )
            self.assertIsNotNone(profile)
            assert profile is not None
            self.assertAlmostEqual(profile.start_point.x, float(end_point["x"]), places=6)
            self.assertAlmostEqual(profile.start_point.y, float(end_point["y"]), places=6)
            self.assertAlmostEqual(float(profile.start_point.z or 0.0), float(end_point["z"]), places=6)
            self.assertAlmostEqual(float(profile.end_point.z or 0.0), float(start_point["z"]), places=6)
            self.assertEqual(len([point for point in profile.points if not point.is_endpoint]), 3)
        finally:
            _restore_virtual_template_dataset_ids(restore_entries)

    def test_normal_and_reverse_profile_axes_point_in_opposite_directions(self) -> None:
        layer = self._marxact_layer()
        exporter = CadastralDxfExporter(wfs_client=object())

        normal_sets, normal_restore = _augment_virtual_template_datasets(
            exporter,
            [layer],
            {},
            lambda _exporter, _layer: None,
            reverse_cross_sections=False,
        )
        try:
            normal_dataset = next(iter(normal_sets.values()))
            normal_profile = exporter._build_template_cross_section_profile(
                normal_dataset, (), [], [], 0.02
            )
        finally:
            _restore_virtual_template_dataset_ids(normal_restore)

        reverse_sets, reverse_restore = _augment_virtual_template_datasets(
            exporter,
            [layer],
            {},
            lambda _exporter, _layer: None,
            reverse_cross_sections=True,
        )
        try:
            reverse_dataset = next(iter(reverse_sets.values()))
            reverse_profile = exporter._build_template_cross_section_profile(
                reverse_dataset, (), [], [], 0.02
            )
        finally:
            _restore_virtual_template_dataset_ids(reverse_restore)

        self.assertIsNotNone(normal_profile)
        self.assertIsNotNone(reverse_profile)
        assert normal_profile is not None and reverse_profile is not None
        dot = (
            float(normal_profile.axis_dx) * float(reverse_profile.axis_dx)
            + float(normal_profile.axis_dy) * float(reverse_profile.axis_dy)
        )
        self.assertLess(dot, 0.0)

    def test_first_template_image_uses_exact_virtual_axis_without_profile(self) -> None:
        layer = self._marxact_layer()
        exporter = CadastralDxfExporter(wfs_client=object())
        vector = exporter._template_tiff_orientation_pixel_vector(layer, [], [], profile=None)
        self.assertIsNotNone(vector)
        assert vector is not None

        start, end = virtual_trench_centerline(layer)
        start_px = layer.transform.world_to_pixel(start[0], start[1])
        end_px = layer.transform.world_to_pixel(end[0], end[1])
        expected = (end_px[0] - start_px[0], end_px[1] - start_px[1])
        self.assertAlmostEqual(vector[0], expected[0], places=6)
        self.assertAlmostEqual(vector[1], expected[1], places=6)

    def test_obsolete_marxact_missing_job_warning_is_removed_only_for_marxact(self) -> None:
        warning = (
            f"ps1.marxact-virtual.tif: {MISSING_KICKTHEMAP_JOB_TEXT}.\n"
            f"gewone_proefsleuf.tif: {MISSING_KICKTHEMAP_JOB_TEXT}."
        )
        filtered = _filter_virtual_missing_job_warning(warning)
        self.assertNotIn("ps1.marxact-virtual.tif", filtered)
        self.assertIn("gewone_proefsleuf.tif", filtered)

        only_marxact = _filter_virtual_missing_job_warning(
            f"ps2.marxact-virtual.tif: {MISSING_KICKTHEMAP_JOB_TEXT}."
        )
        self.assertEqual(only_marxact, "")


if __name__ == "__main__":
    unittest.main()
