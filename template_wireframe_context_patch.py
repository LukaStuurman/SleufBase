from __future__ import annotations

from .cadastral_export import CadastralDxfExporter


PATCH_VERSION = 2
TEMPLATE_CONTEXT_SCALE = 10.0


def _base_orientation_padding(layer) -> float:
    span = max(float(layer.bounds.width), float(layer.bounds.height))
    return max(35.0, min(75.0, span * 8.0))


def _expanded_orientation_padding(layer) -> float:
    return _base_orientation_padding(layer) * TEMPLATE_CONTEXT_SCALE


def install_template_wireframe_context_patch() -> None:
    """Fetch 10x more local map/BGT context around every template trench.

    The template exporter deliberately fetches BGT/WFS data per trench instead of
    over one huge combined project extent. The core local margin is 35-75 m;
    for the DXF template export we make that exact margin ten times larger while
    leaving the paper-space viewport/scale untouched.
    """

    exporter_class = CadastralDxfExporter
    installed_version = int(
        getattr(exporter_class, "_sleufbase_template_wireframe_context_patch_version", 0) or 0
    )
    if installed_version >= PATCH_VERSION:
        return

    original_orientation_bounds = exporter_class._template_orientation_fetch_bounds

    def _template_orientation_fetch_bounds_expanded(self, layers, fallback_bounds):
        boxes = []
        for layer in layers:
            padding = _expanded_orientation_padding(layer)
            # Do not clip the expanded local box back to fallback_bounds: the
            # local BGT/WFS proxy consumes these boxes directly. Clipping here
            # would silently shrink the requested 10x context.
            boxes.append(layer.bounds.padded(padding))
        if not boxes:
            return original_orientation_bounds(self, layers, fallback_bounds)
        # Preserve the core performance guard: merge nearby/local boxes and cap
        # the number of fetch areas, just as the stock template exporter does.
        return self._merge_template_fetch_bounds(boxes, max_count=12)

    exporter_class._template_orientation_fetch_bounds = _template_orientation_fetch_bounds_expanded
    exporter_class._sleufbase_template_wireframe_context_patch_version = PATCH_VERSION
