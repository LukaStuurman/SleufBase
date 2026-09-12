from __future__ import annotations

from .cadastral_export import CadastralDxfExporter


PATCH_VERSION = 3
TEMPLATE_CONTEXT_SCALE = 10.0


def _base_orientation_padding(layer) -> float:
    span = max(float(layer.bounds.width), float(layer.bounds.height))
    return max(35.0, min(75.0, span * 8.0))


def _expanded_orientation_padding(layer) -> float:
    return _base_orientation_padding(layer) * TEMPLATE_CONTEXT_SCALE


def _merge_local_context_bounds(exporter, boxes):
    """Merge only genuinely overlapping local areas.

    Do not force an arbitrary low count. For large, geographically spread
    KickTheMap selections the old max_count=12 reduction could union distant
    trenches into enormous rectangles and make BGT/WFS fetches explode.
    """
    return exporter._merge_intersecting_bounds(list(boxes))


def install_template_wireframe_context_patch() -> None:
    """Fetch 10x more local map/BGT context around every template trench.

    Context remains local for very large selections: overlapping areas are
    merged, but distant trenches are never combined just to satisfy a hard
    request-count cap. The bounded fetch scheduler limits concurrency instead.
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
        return _merge_local_context_bounds(self, boxes)

    exporter_class._template_orientation_fetch_bounds = _template_orientation_fetch_bounds_expanded
    exporter_class._sleufbase_template_wireframe_context_patch_version = PATCH_VERSION
