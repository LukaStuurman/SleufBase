from __future__ import annotations

from types import SimpleNamespace
import unittest

from SleufBase.cadastral_export import CadastralDxfExporter
from SleufBase.models import Bounds
from SleufBase.template_wireframe_context_patch import (
    PATCH_VERSION,
    TEMPLATE_CONTEXT_SCALE,
    _base_orientation_padding,
    _expanded_orientation_padding,
    install_template_wireframe_context_patch,
)


def _exporter() -> CadastralDxfExporter:
    install_template_wireframe_context_patch()
    return CadastralDxfExporter.__new__(CadastralDxfExporter)


class TemplateWireframeContextPatchTests(unittest.TestCase):
    def test_local_template_context_margin_is_exactly_ten_times_larger(self) -> None:
        layer = SimpleNamespace(bounds=Bounds(100.0, 200.0, 110.0, 210.0))

        self.assertEqual(PATCH_VERSION, 2)
        self.assertEqual(TEMPLATE_CONTEXT_SCALE, 10.0)
        self.assertAlmostEqual(_base_orientation_padding(layer), 75.0)
        self.assertAlmostEqual(_expanded_orientation_padding(layer), 750.0)

    def test_small_trench_uses_ten_times_minimum_local_margin(self) -> None:
        layer = SimpleNamespace(bounds=Bounds(0.0, 0.0, 1.0, 1.0))

        self.assertAlmostEqual(_base_orientation_padding(layer), 35.0)
        self.assertAlmostEqual(_expanded_orientation_padding(layer), 350.0)

    def test_orientation_fetch_bounds_are_not_clipped_back_to_old_fallback(self) -> None:
        exporter = _exporter()
        layer = SimpleNamespace(bounds=Bounds(100.0, 200.0, 110.0, 210.0))
        old_fallback = Bounds(40.0, 140.0, 170.0, 270.0)

        boxes = exporter._template_orientation_fetch_bounds([layer], old_fallback)

        self.assertEqual(len(boxes), 1)
        expanded = boxes[0]
        self.assertAlmostEqual(expanded.min_x, -650.0)
        self.assertAlmostEqual(expanded.min_y, -550.0)
        self.assertAlmostEqual(expanded.max_x, 860.0)
        self.assertAlmostEqual(expanded.max_y, 960.0)

    def test_empty_layer_list_keeps_core_fallback_behavior(self) -> None:
        exporter = _exporter()
        fallback = Bounds(0.0, 0.0, 100.0, 100.0)

        self.assertEqual(exporter._template_orientation_fetch_bounds([], fallback), [fallback])


if __name__ == "__main__":
    unittest.main()
