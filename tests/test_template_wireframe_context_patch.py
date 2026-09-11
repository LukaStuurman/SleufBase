from __future__ import annotations

from types import SimpleNamespace
import unittest

from SleufBase.cadastral_export import CadastralDxfExporter
from SleufBase.models import Bounds
from SleufBase.template_wireframe_context_patch import (
    TEMPLATE_CONTEXT_SCALE,
    _scaled_wireframe_padding,
    install_template_wireframe_context_patch,
)


def _exporter() -> CadastralDxfExporter:
    install_template_wireframe_context_patch()
    return CadastralDxfExporter.__new__(CadastralDxfExporter)


class TemplateWireframeContextPatchTests(unittest.TestCase):
    def test_template_wireframe_padding_is_ten_times_larger(self) -> None:
        exporter = _exporter()

        self.assertEqual(TEMPLATE_CONTEXT_SCALE, 10.0)
        self.assertAlmostEqual(_scaled_wireframe_padding(exporter, exporter.LABEL_GAP), 180.0)

    def test_template_fetch_padding_expands_only_while_template_context_is_active(self) -> None:
        exporter = _exporter()
        bounds = Bounds(0.0, 0.0, 10.0, 10.0)

        self.assertAlmostEqual(exporter._overview_padding(bounds), 60.0)

        exporter._sleufbase_template_context_scale_active = True
        exporter._sleufbase_template_context_label_gap = exporter.LABEL_GAP
        self.assertAlmostEqual(exporter._overview_padding(bounds), 180.0)

    def test_template_wireframe_viewport_uses_expanded_bounds(self) -> None:
        exporter = _exporter()
        captured: dict[str, Bounds] = {}
        source_bounds = Bounds(100.0, 200.0, 110.0, 210.0)

        exporter._combined_bounds = lambda _layers: source_bounds

        def _choose_scale(bounds: Bounds, _width: float, _height: float) -> int:
            captured["bounds"] = bounds
            return 5000

        exporter._choose_template_wireframe_scale = _choose_scale
        viewport = SimpleNamespace(dxf=SimpleNamespace(width=100.0, height=50.0))

        scale = exporter._fit_template_wireframe_viewport(viewport, [object()], exporter.LABEL_GAP)

        self.assertEqual(scale, 5000)
        padded = captured["bounds"]
        self.assertAlmostEqual(padded.min_x, -80.0)
        self.assertAlmostEqual(padded.min_y, 20.0)
        self.assertAlmostEqual(padded.max_x, 290.0)
        self.assertAlmostEqual(padded.max_y, 390.0)
        self.assertEqual(viewport.dxf.view_center_point, (105.0, 205.0, 0.0))
        self.assertAlmostEqual(viewport.dxf.view_height, 250.0)


if __name__ == "__main__":
    unittest.main()
