from __future__ import annotations

from copy import copy
from functools import wraps
from typing import Any, Iterable

from .bgt_vector_tiles import BgtSurfaceFeature
from .cadastral_wfs import CadastralLinework, CadastralTextLabel
from .models import Bounds


PATCH_VERSION = 2


def _dedupe_paths(paths: Iterable[list[tuple[float, float]]]) -> list[list[tuple[float, float]]]:
    result: list[list[tuple[float, float]]] = []
    seen: set[tuple[tuple[float, float], ...]] = set()
    for path in paths:
        if len(path) < 2:
            continue
        rounded = tuple((round(float(x), 3), round(float(y), 3)) for x, y in path)
        reverse = tuple(reversed(rounded))
        key = rounded if rounded <= reverse else reverse
        if key in seen:
            continue
        seen.add(key)
        result.append(path)
    return result


def _dedupe_bounds(bounds_list: Iterable[Bounds]) -> list[Bounds]:
    result: list[Bounds] = []
    seen: set[tuple[float, float, float, float]] = set()
    for bounds in bounds_list:
        key = (
            round(float(bounds.min_x), 3),
            round(float(bounds.min_y), 3),
            round(float(bounds.max_x), 3),
            round(float(bounds.max_y), 3),
        )
        if key in seen:
            continue
        seen.add(key)
        result.append(bounds)
    return result


def _dedupe_text_labels(labels: Iterable[CadastralTextLabel]) -> list[CadastralTextLabel]:
    result: list[CadastralTextLabel] = []
    seen: set[tuple[str, str, float, float, float]] = set()
    for label in labels:
        key = (
            str(label.layer_name),
            str(label.text),
            round(float(label.position[0]), 3),
            round(float(label.position[1]), 3),
            round(float(label.rotation), 3),
        )
        if key in seen:
            continue
        seen.add(key)
        result.append(label)
    return result


class _LocalBoundsBgtClient:
    """Proxy a BGT client while restricting template fetches to local trench areas."""

    def __init__(self, delegate: Any, bounds_list: list[Bounds], status_callback=None) -> None:
        self._delegate = delegate
        self._bounds_list = _dedupe_bounds(bounds_list)
        self._status_callback = status_callback

    def __getattr__(self, name: str):
        return getattr(self._delegate, name)

    def _status(self, label: str, index: int, total: int) -> None:
        if self._status_callback is None:
            return
        try:
            if total > 1:
                self._status_callback(f"{label}... {index}/{total}")
            else:
                self._status_callback(f"{label}...")
        except Exception:
            pass

    def fetch_paths(self, _combined_bounds: Bounds) -> list[list[tuple[float, float]]]:
        paths: list[list[tuple[float, float]]] = []
        total = len(self._bounds_list)
        for index, bounds in enumerate(self._bounds_list, start=1):
            self._status("Haal BGT-vector tiles op", index, total)
            paths.extend(self._delegate.fetch_paths(bounds))
        return _dedupe_paths(paths)

    def fetch_surface_features(self, _combined_bounds: Bounds) -> list[BgtSurfaceFeature]:
        total = len(self._bounds_list)
        merged: dict[tuple[str, str, str], BgtSurfaceFeature] = {}
        extras: list[BgtSurfaceFeature] = []
        for index, bounds in enumerate(self._bounds_list, start=1):
            self._status("Haal BGT-ondergrondnamen op", index, total)
            for feature in self._delegate.fetch_surface_features(bounds):
                key = (
                    str(feature.layer_name),
                    str(feature.feature_id),
                    str(feature.physical_appearance),
                )
                existing = merged.get(key)
                if existing is None:
                    merged[key] = feature
                    continue
                try:
                    merged[key] = BgtSurfaceFeature(
                        layer_name=existing.layer_name,
                        feature_id=existing.feature_id,
                        physical_appearance=existing.physical_appearance,
                        geometry=existing.geometry.union(feature.geometry),
                    )
                except Exception:
                    extras.append(feature)
        return [*merged.values(), *extras]


class _LocalBoundsWfsClient:
    """Proxy cadastral WFS requests through only the local trench areas."""

    def __init__(self, delegate: Any, bounds_list: list[Bounds], status_callback=None) -> None:
        self._delegate = delegate
        self._bounds_list = _dedupe_bounds(bounds_list)
        self._status_callback = status_callback

    def __getattr__(self, name: str):
        return getattr(self._delegate, name)

    def _status(self, label: str, index: int, total: int) -> None:
        if self._status_callback is None:
            return
        try:
            if total > 1:
                self._status_callback(f"{label}... {index}/{total}")
            else:
                self._status_callback(f"{label}...")
        except Exception:
            pass

    def fetch_parcel_boundaries(self, _combined_bounds: Bounds) -> CadastralLinework | None:
        paths: list[list[tuple[float, float]]] = []
        total = len(self._bounds_list)
        for index, bounds in enumerate(self._bounds_list, start=1):
            self._status("Haal kadastrale perceelgrenzen op", index, total)
            linework = self._delegate.fetch_parcel_boundaries(bounds)
            if linework is not None:
                paths.extend(linework.paths)
        paths = _dedupe_paths(paths)
        if not paths:
            return None
        return CadastralLinework(layer_name="KAD_GRENS", paths=paths)

    def fetch_text_labels(self, _combined_bounds: Bounds) -> list[CadastralTextLabel]:
        labels: list[CadastralTextLabel] = []
        total = len(self._bounds_list)
        for index, bounds in enumerate(self._bounds_list, start=1):
            self._status("Haal kadastrale teksten op", index, total)
            labels.extend(self._delegate.fetch_text_labels(bounds))
        return _dedupe_text_labels(labels)


def install_template_bgt_fetch_patch() -> None:
    from .cadastral_export import CadastralDxfExporter

    if int(getattr(CadastralDxfExporter, "_sleufbase_bgt_fetch_patch_version", 0) or 0) >= PATCH_VERSION:
        return

    original = CadastralDxfExporter._fetch_template_server_data_single

    @wraps(original)
    def _fetch_template_server_data_local_bgt(self, bounds: Bounds, *args, **kwargs):
        orientation_bounds = kwargs.get("orientation_bounds")
        if not orientation_bounds:
            return original(self, bounds, *args, **kwargs)

        local_bounds = _dedupe_bounds(orientation_bounds)
        if not local_bounds:
            return original(self, bounds, *args, **kwargs)

        # Do not mutate the live exporter. Template server data is fetched only
        # around the actual proefsleuven instead of across the potentially huge
        # combined extent between far-apart trenches.
        exporter = copy(self)
        status_callback = kwargs.get("status_callback")
        exporter.bgt_vector_tile_client = _LocalBoundsBgtClient(
            self.bgt_vector_tile_client,
            local_bounds,
            status_callback=status_callback,
        )
        exporter.wfs_client = _LocalBoundsWfsClient(
            self.wfs_client,
            local_bounds,
            status_callback=status_callback,
        )
        return original(exporter, bounds, *args, **kwargs)

    CadastralDxfExporter._fetch_template_server_data_single = _fetch_template_server_data_local_bgt
    CadastralDxfExporter._sleufbase_bgt_fetch_patch_version = PATCH_VERSION
