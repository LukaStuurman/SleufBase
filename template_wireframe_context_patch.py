from __future__ import annotations

from typing import Any

from .cadastral_export import CadastralDxfExporter


PATCH_VERSION = 1
TEMPLATE_CONTEXT_SCALE = 10.0
_ACTIVE_ATTR = "_sleufbase_template_context_scale_active"
_LABEL_GAP_ATTR = "_sleufbase_template_context_label_gap"


def _base_wireframe_padding(exporter: CadastralDxfExporter, label_gap: float) -> float:
    return max(2.0, float(label_gap) + (float(exporter.LABEL_HEIGHT) * 2.0))


def _scaled_wireframe_padding(exporter: CadastralDxfExporter, label_gap: float) -> float:
    return _base_wireframe_padding(exporter, label_gap) * TEMPLATE_CONTEXT_SCALE


def _restore_instance_attr(instance: Any, name: str, had_attr: bool, value: Any) -> None:
    if had_attr:
        setattr(instance, name, value)
        return
    try:
        delattr(instance, name)
    except AttributeError:
        pass


def install_template_wireframe_context_patch() -> None:
    """Show and fetch 10x more map/BGT context around template trench locations."""

    exporter_class = CadastralDxfExporter
    installed_version = int(
        getattr(exporter_class, "_sleufbase_template_wireframe_context_patch_version", 0) or 0
    )
    if installed_version >= PATCH_VERSION:
        return

    original_export_template_sheet = exporter_class.export_template_sheet
    original_overview_padding = exporter_class._overview_padding

    def _export_template_sheet_with_expanded_context(self, *args, **kwargs):
        label_gap = kwargs.get("label_gap")
        if label_gap is None:
            # Positional parameters after self: output_path, template_path, tiff_layers,
            # status_callback, trench_mode, include_tiff_images, label_gap.
            label_gap = args[6] if len(args) > 6 else self.LABEL_GAP

        had_active = hasattr(self, _ACTIVE_ATTR)
        previous_active = getattr(self, _ACTIVE_ATTR, None)
        had_label_gap = hasattr(self, _LABEL_GAP_ATTR)
        previous_label_gap = getattr(self, _LABEL_GAP_ATTR, None)
        setattr(self, _ACTIVE_ATTR, True)
        setattr(self, _LABEL_GAP_ATTR, float(label_gap))
        try:
            return original_export_template_sheet(self, *args, **kwargs)
        finally:
            _restore_instance_attr(self, _ACTIVE_ATTR, had_active, previous_active)
            _restore_instance_attr(self, _LABEL_GAP_ATTR, had_label_gap, previous_label_gap)

    def _overview_padding_with_template_context(self, bounds):
        padding = float(original_overview_padding(self, bounds))
        if not bool(getattr(self, _ACTIVE_ATTR, False)):
            return padding

        label_gap = float(getattr(self, _LABEL_GAP_ATTR, self.LABEL_GAP))
        # Fetch at least the full expanded viewport context. Keep the existing
        # larger adaptive fetch padding when it already exceeds that value.
        return max(padding, _scaled_wireframe_padding(self, label_gap))

    def _fit_template_wireframe_viewport_expanded(self, viewport, page_layers, label_gap: float) -> int:
        bounds = self._combined_bounds(page_layers)
        padding = _scaled_wireframe_padding(self, label_gap)
        padded_bounds = bounds.padded(padding)
        scale_denominator = self._choose_template_wireframe_scale(
            padded_bounds,
            float(viewport.dxf.width),
            float(viewport.dxf.height),
        )
        viewport.dxf.view_target_point = (0.0, 0.0, 0.0)
        viewport.dxf.view_center_point = (padded_bounds.center_x, padded_bounds.center_y, 0.0)
        viewport.dxf.view_direction_vector = (0.0, 0.0, 1.0)
        viewport.dxf.view_twist_angle = 0.0
        viewport.dxf.view_height = (float(viewport.dxf.height) * scale_denominator) / 1000.0
        return scale_denominator

    exporter_class.export_template_sheet = _export_template_sheet_with_expanded_context
    exporter_class._overview_padding = _overview_padding_with_template_context
    exporter_class._fit_template_wireframe_viewport = _fit_template_wireframe_viewport_expanded
    exporter_class._sleufbase_template_wireframe_context_patch_version = PATCH_VERSION
