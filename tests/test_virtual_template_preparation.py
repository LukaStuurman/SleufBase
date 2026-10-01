from __future__ import annotations

from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from SleufBase.cadastral_export import CadastralExportError
from SleufBase.virtual_trench import VIRTUAL_TRENCH_METADATA_KEY, build_virtual_trench_dataset
from SleufBase.virtual_trench_template_patch import (
    VIRTUAL_TEMPLATE_DATASET_ID_KEY,
    _augment_virtual_template_datasets,
    _restore_virtual_template_dataset_ids,
)


def _layer(name="PS test"):
    return SimpleNamespace(
        path=Path(f"{name}.marxact-virtual.tif"),
        metadata={
            "marxact_trench_name": name,
            VIRTUAL_TRENCH_METADATA_KEY: {"points": [
                {"role": "start", "x": 4.0, "y": 0.0, "z": 10.0},
                {"role": "object", "x": 4.0, "y": 0.5, "z": 9.4, "source_name": "Water"},
                {"role": "object", "x": 4.0, "y": 2.5, "z": 9.3, "source_name": "Datatransport"},
                {"role": "end", "x": 4.0, "y": 3.0, "z": 10.3},
            ]},
        },
    )


def _prepare(layers, **kwargs):
    return _augment_virtual_template_datasets(
        object(), layers, {}, lambda *_args: None, **kwargs,
    )


class VirtualTemplatePreparationTests(unittest.TestCase):
    def test_later_builder_failure_does_not_leak_earlier_private_ids(self):
        for old_marker in (None, -123):
            with self.subTest(old_marker=old_marker):
                first, second = _layer("PS 1"), _layer("PS 2")
                if old_marker is not None:
                    first.metadata[VIRTUAL_TEMPLATE_DATASET_ID_KEY] = old_marker

                def build(layer, **kwargs):
                    if layer is second:
                        raise RuntimeError("second dataset could not be read")
                    return build_virtual_trench_dataset(layer, **kwargs)

                with patch("SleufBase.virtual_trench_template_patch.build_virtual_trench_dataset", side_effect=build):
                    with self.assertRaisesRegex(RuntimeError, "second dataset"):
                        _prepare([first, second])

                self.assertEqual(first.metadata.get(VIRTUAL_TEMPLATE_DATASET_ID_KEY), old_marker)
                self.assertNotIn(VIRTUAL_TEMPLATE_DATASET_ID_KEY, second.metadata)

                # A corrected retry uses clean private ids and restores the
                # exact pre-export metadata after consuming the datasets.
                datasets, restore = _prepare([first, second])
                self.assertEqual(len(datasets), 2)
                _restore_virtual_template_dataset_ids(restore)
                self.assertEqual(first.metadata.get(VIRTUAL_TEMPLATE_DATASET_ID_KEY), old_marker)
                self.assertNotIn(VIRTUAL_TEMPLATE_DATASET_ID_KEY, second.metadata)

    def test_cross_section_rejects_missing_and_nonfinite_endpoint_heights(self):
        for endpoint, label in ((0, "beginpunt"), (3, "eindpunt")):
            for height in (None, "unknown", float("nan"), float("inf"), float("-inf")):
                with self.subTest(endpoint=endpoint, height=height):
                    layer = _layer()
                    layer.metadata[VIRTUAL_TRENCH_METADATA_KEY]["points"][endpoint]["z"] = height

                    with self.assertRaises(CadastralExportError) as error:
                        _prepare([layer], include_cross_sections=True)

                    self.assertIn("PS test", str(error.exception))
                    self.assertIn(label, str(error.exception))
                    self.assertNotIn(VIRTUAL_TEMPLATE_DATASET_ID_KEY, layer.metadata)

    def test_missing_both_heights_names_both_endpoints(self):
        layer = _layer()
        points = layer.metadata[VIRTUAL_TRENCH_METADATA_KEY]["points"]
        points[0]["z"] = points[3]["z"] = None

        with self.assertRaisesRegex(CadastralExportError, "beginpunt en eindpunt"):
            _prepare([layer], include_cross_sections=True)

    def test_direction_only_export_allows_missing_endpoint_heights(self):
        for height in (None, float("nan"), float("inf")):
            with self.subTest(height=height):
                layer = _layer()
                points = layer.metadata[VIRTUAL_TRENCH_METADATA_KEY]["points"]
                points[0]["z"] = points[3]["z"] = height

                datasets, restore = _prepare([layer], include_cross_sections=False)

                self.assertEqual(datasets, {})
                self.assertEqual(restore, [])
                self.assertNotIn(VIRTUAL_TEMPLATE_DATASET_ID_KEY, layer.metadata)

    def test_zero_and_finite_numeric_heights_remain_valid_in_both_directions(self):
        for reverse in (False, True):
            with self.subTest(reverse=reverse):
                layer = _layer()
                points = layer.metadata[VIRTUAL_TRENCH_METADATA_KEY]["points"]
                points[0]["z"], points[3]["z"] = 0.0, "10.3"
                datasets, restore = _prepare(
                    [layer], include_cross_sections=True, reverse_cross_sections=reverse,
                )

                dataset = next(iter(datasets.values()))
                self.assertEqual(dataset.cross_section_start_xy, (4.0, 3.0 if reverse else 0.0))
                self.assertEqual(dataset.points[0].z, 0.0)
                self.assertEqual(dataset.points[-1].z, 10.3)
                _restore_virtual_template_dataset_ids(restore)


if __name__ == "__main__":
    unittest.main()
