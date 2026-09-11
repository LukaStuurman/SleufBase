from __future__ import annotations

from types import SimpleNamespace

import pytest

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


def test_template_wireframe_padding_is_ten_times_larger() -> None:
    exporter = _exporter()

    assert TEMPLATE_CONTEXT_SCALE == 10.0
    assert _scaled_wireframe_padding(exporter, exporter.LABEL_GAP) == pytest.approx(180.0)


def test_template_fetch_padding_expands_only_while_template_context_is_active() -> None:
    exporter = _exporter()
    bounds = Bounds(0.0, 0.0, 10.0, 10.0)

    assert exporter._overview_padding(bounds) == pytest.approx(60.0)

    exporter._sleufbase_template_context_scale_active = True
    exporter._sleufbase_template_context_label_gap = exporter.LABEL_GAP
    assert exporter._overview_padding(bounds) == pytest.approx(180.0)


def test_template_wireframe_viewport_uses_expanded_bounds() -> None:
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

    assert scale == 5000
    padded = captured["bounds"]
    assert padded.min_x == pytest.approx(-80.0)
    assert padded.min_y == pytest.approx(20.0)
    assert padded.max_x == pytest.approx(290.0)
    assert padded.max_y == pytest.approx(390.0)
    assert viewport.dxf.view_center_point == pytest.approx((105.0, 205.0, 0.0))
    assert viewport.dxf.view_height == pytest.approx(250.0)
