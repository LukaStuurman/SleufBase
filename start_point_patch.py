from __future__ import annotations

from dataclasses import replace
from typing import Any

START_POINT_KEY = "template_cross_section_start_point"
MANUAL_START_POINT_KEY = "template_cross_section_start_point_manual"


def _normalized_xy(value: Any) -> tuple[float, float] | None:
    if not isinstance(value, (tuple, list)) or len(value) < 2:
        return None
    try:
        return float(value[0]), float(value[1])
    except (TypeError, ValueError):
        return None


def _dataset_with_layer_start_point(dataset: Any, layer: Any) -> Any:
    if dataset is None:
        return None
    metadata = getattr(layer, "metadata", None)
    if not isinstance(metadata, dict):
        return dataset
    forced_xy = _normalized_xy(metadata.get(START_POINT_KEY))
    if forced_xy is None:
        return dataset
    if _normalized_xy(getattr(dataset, "cross_section_start_xy", None)) == forced_xy:
        return dataset
    try:
        return replace(dataset, cross_section_start_xy=forced_xy)
    except (TypeError, ValueError):
        return dataset


def install_manual_start_point_patch() -> None:
    from .app import KlicViewerApp

    viewer_class = KlicViewerApp
    if getattr(viewer_class, "_manual_cross_section_start_patch", False):
        return

    original_set_start = getattr(viewer_class, "_set_template_cross_section_start_metadata", None)
    if callable(original_set_start):
        def _set_template_cross_section_start_metadata(self, layer, start_x, start_y, *args, **kwargs):
            result = original_set_start(self, layer, start_x, start_y, *args, **kwargs)
            metadata = getattr(layer, "metadata", None)
            if isinstance(metadata, dict):
                if getattr(self, "_sleufbase_auto_start_points_active", False):
                    metadata.pop(MANUAL_START_POINT_KEY, None)
                else:
                    metadata[MANUAL_START_POINT_KEY] = True
            return result

        viewer_class._set_template_cross_section_start_metadata = _set_template_cross_section_start_metadata

    original_load_all = getattr(viewer_class, "load_all_kickthemap_start_points", None)
    if callable(original_load_all):
        def _load_all_kickthemap_start_points_preserving_manual(self, *args, **kwargs):
            manual_points: list[tuple[Any, tuple[float, float]]] = []
            for layer in getattr(self, "tiff_layers", ()) or ():
                metadata = getattr(layer, "metadata", None)
                if not isinstance(metadata, dict) or not metadata.get(MANUAL_START_POINT_KEY):
                    continue
                xy = _normalized_xy(metadata.get(START_POINT_KEY))
                if xy is not None:
                    manual_points.append((layer, xy))

            self._sleufbase_auto_start_points_active = True
            try:
                result = original_load_all(self, *args, **kwargs)
            finally:
                self._sleufbase_auto_start_points_active = False
                for layer, xy in manual_points:
                    metadata = getattr(layer, "metadata", None)
                    if isinstance(metadata, dict):
                        metadata[START_POINT_KEY] = xy
                        metadata[MANUAL_START_POINT_KEY] = True

                if manual_points:
                    refresh = getattr(self, "_refresh_map_edit_markers", None)
                    if callable(refresh):
                        try:
                            refresh()
                        except Exception:
                            pass
                    request_render = getattr(self, "request_render", None)
                    if callable(request_render):
                        try:
                            request_render(immediate=False)
                        except Exception:
                            pass
            return result

        viewer_class.load_all_kickthemap_start_points = _load_all_kickthemap_start_points_preserving_manual

    for method_name in ("_load_local_maaiveld_dataset_for_layer", "_load_maaiveld_dataset_for_layer"):
        original_loader = getattr(viewer_class, method_name, None)
        if not callable(original_loader):
            continue

        def _make_loader(loader):
            def _loader(self, layer, *args, **kwargs):
                dataset = loader(self, layer, *args, **kwargs)
                return _dataset_with_layer_start_point(dataset, layer)
            return _loader

        setattr(viewer_class, method_name, _make_loader(original_loader))

    viewer_class._manual_cross_section_start_patch = True
