from __future__ import annotations

from concurrent.futures import FIRST_COMPLETED, ThreadPoolExecutor, as_completed, wait
import threading
from typing import Any, Callable

import requests

from .bgt_vector_tiles import BgtSurfaceFeature
from .cadastral_wfs import CadastralLinework, CadastralTextLabel, CadastralWfsClient, CadastralWfsError
from .template_asset_memory_patch import (
    TEMPLATE_UI_PUMP_INTERVAL_SECONDS,
    _contains_virtual_template_task,
    _pump_template_ui,
)
from .template_bgt_fetch_patch import (
    _LocalBoundsBgtClient,
    _LocalBoundsWfsClient,
    _dedupe_paths,
    _dedupe_text_labels,
)


PATCH_VERSION = 1
MAX_LOCAL_WFS_WORKERS = 4
MAX_LOCAL_BGT_WORKERS = 2
MAX_TEMPLATE_PATH_WORKERS = 4
MAX_VIRTUAL_TEMPLATE_MAP_WORKERS = 2


def _safe_status(callback, message: str) -> None:
    if callback is None:
        return
    try:
        callback(message)
    except Exception:
        pass


def _parallel_ordered(
    items: list[Any],
    worker: Callable[[Any], Any],
    *,
    max_workers: int,
    status_callback=None,
    status_label: str = "",
) -> list[Any]:
    if not items:
        return []
    if len(items) == 1 or max_workers <= 1:
        result = [worker(items[0])]
        if status_label:
            _safe_status(status_callback, f"{status_label}...")
        return result

    results: list[Any] = [None] * len(items)
    worker_count = max(1, min(int(max_workers), len(items)))
    with ThreadPoolExecutor(max_workers=worker_count, thread_name_prefix="template-fetch") as executor:
        future_map = {
            executor.submit(worker, item): index
            for index, item in enumerate(items)
        }
        completed = 0
        for future in as_completed(future_map):
            index = future_map[future]
            results[index] = future.result()
            completed += 1
            if status_label:
                if len(items) > 1:
                    _safe_status(status_callback, f"{status_label}... {completed}/{len(items)}")
                else:
                    _safe_status(status_callback, f"{status_label}...")
    return results


def _ensure_wfs_worker_state(client: CadastralWfsClient) -> tuple[threading.local, threading.RLock, set[requests.Session]]:
    thread_local = getattr(client, "_sleufbase_wfs_thread_local", None)
    session_lock = getattr(client, "_sleufbase_wfs_session_lock", None)
    worker_sessions = getattr(client, "_sleufbase_wfs_worker_sessions", None)
    if thread_local is None or session_lock is None or worker_sessions is None:
        # Instances normally receive these fields from the patched __init__, but
        # lazily initialize too for embedded/tests that created a client earlier.
        thread_local = threading.local()
        session_lock = threading.RLock()
        worker_sessions = set()
        client._sleufbase_wfs_thread_local = thread_local
        client._sleufbase_wfs_session_lock = session_lock
        client._sleufbase_wfs_worker_sessions = worker_sessions
    return thread_local, session_lock, worker_sessions


def _wfs_worker_session(client: CadastralWfsClient) -> requests.Session:
    # Keep main-thread compatibility with tests/callers that replace .session.
    if threading.current_thread() is threading.main_thread():
        return client.session
    thread_local, session_lock, worker_sessions = _ensure_wfs_worker_state(client)
    session = getattr(thread_local, "session", None)
    if session is not None:
        return session
    session = requests.Session()
    try:
        session.headers.update(dict(client.session.headers))
    except Exception:
        session.headers.update({"User-Agent": "SleufBase/0.2"})
    thread_local.session = session
    with session_lock:
        worker_sessions.add(session)
    return session


def _install_thread_safe_wfs_sessions() -> None:
    if int(getattr(CadastralWfsClient, "_sleufbase_parallel_session_version", 0) or 0) >= PATCH_VERSION:
        return

    original_init = CadastralWfsClient.__init__
    original_close = CadastralWfsClient.close

    def __init__(self, *args, **kwargs):
        original_init(self, *args, **kwargs)
        self._sleufbase_wfs_thread_local = threading.local()
        self._sleufbase_wfs_session_lock = threading.RLock()
        self._sleufbase_wfs_worker_sessions: set[requests.Session] = set()

    def close(self) -> None:
        try:
            original_close(self)
        finally:
            _thread_local, session_lock, worker_sessions = _ensure_wfs_worker_state(self)
            with session_lock:
                sessions = list(worker_sessions)
                worker_sessions.clear()
            for session in sessions:
                try:
                    session.close()
                except Exception:
                    pass

    def _get_json_parallel(self, params: dict[str, object]) -> dict[str, Any]:
        last_error: Exception | None = None
        for attempt in range(1, self.retries + 1):
            try:
                response = _wfs_worker_session(self).get(
                    self.BASE_URL,
                    params=params,
                    timeout=self.timeout,
                )
                response.raise_for_status()
                payload = response.json()
                if not isinstance(payload, dict):
                    raise CadastralWfsError("De WFS-server gaf geen JSON-object terug.")
                return payload
            except (requests.RequestException, ValueError, CadastralWfsError) as exc:
                last_error = exc
                if attempt < self.retries:
                    # Preserve the legacy retry cadence.
                    import time

                    time.sleep(0.7 * attempt)
        raise CadastralWfsError(str(last_error))

    CadastralWfsClient.__init__ = __init__
    CadastralWfsClient.close = close
    CadastralWfsClient._get_json = _get_json_parallel
    CadastralWfsClient._sleufbase_parallel_session_version = PATCH_VERSION


def _install_local_bounds_parallel_fetch() -> None:
    def fetch_parcel_boundaries_parallel(self, _combined_bounds) -> CadastralLinework | None:
        bounds_list = list(self._bounds_list)
        linework_items = _parallel_ordered(
            bounds_list,
            self._delegate.fetch_parcel_boundaries,
            max_workers=MAX_LOCAL_WFS_WORKERS,
            status_callback=self._status_callback,
            status_label="Haal kadastrale perceelgrenzen op",
        )
        paths: list[list[tuple[float, float]]] = []
        for linework in linework_items:
            if linework is not None:
                paths.extend(linework.paths)
        paths = _dedupe_paths(paths)
        if not paths:
            return None
        return CadastralLinework(layer_name="KAD_GRENS", paths=paths)

    def fetch_text_labels_parallel(self, _combined_bounds) -> list[CadastralTextLabel]:
        bounds_list = list(self._bounds_list)
        label_groups = _parallel_ordered(
            bounds_list,
            self._delegate.fetch_text_labels,
            max_workers=MAX_LOCAL_WFS_WORKERS,
            status_callback=self._status_callback,
            status_label="Haal kadastrale teksten op",
        )
        labels: list[CadastralTextLabel] = []
        for group in label_groups:
            labels.extend(group)
        return _dedupe_text_labels(labels)

    def fetch_bgt_paths_parallel(self, _combined_bounds) -> list[list[tuple[float, float]]]:
        bounds_list = list(self._bounds_list)
        path_groups = _parallel_ordered(
            bounds_list,
            self._delegate.fetch_paths,
            max_workers=MAX_LOCAL_BGT_WORKERS,
            status_callback=self._status_callback,
            status_label="Haal BGT-vector tiles op",
        )
        paths: list[list[tuple[float, float]]] = []
        for group in path_groups:
            paths.extend(group)
        return _dedupe_paths(paths)

    def fetch_bgt_surface_features_parallel(self, _combined_bounds) -> list[BgtSurfaceFeature]:
        bounds_list = list(self._bounds_list)
        feature_groups = _parallel_ordered(
            bounds_list,
            self._delegate.fetch_surface_features,
            max_workers=MAX_LOCAL_BGT_WORKERS,
            status_callback=self._status_callback,
            status_label="Haal BGT-ondergrondnamen op",
        )
        merged: dict[tuple[str, str, str], BgtSurfaceFeature] = {}
        extras: list[BgtSurfaceFeature] = []
        for group in feature_groups:
            for feature in group:
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

    _LocalBoundsWfsClient.fetch_parcel_boundaries = fetch_parcel_boundaries_parallel
    _LocalBoundsWfsClient.fetch_text_labels = fetch_text_labels_parallel
    _LocalBoundsBgtClient.fetch_paths = fetch_bgt_paths_parallel
    _LocalBoundsBgtClient.fetch_surface_features = fetch_bgt_surface_features_parallel
    _LocalBoundsWfsClient._sleufbase_parallel_fetch_version = PATCH_VERSION
    _LocalBoundsBgtClient._sleufbase_parallel_fetch_version = PATCH_VERSION


def _install_template_path_parallel_fetch(exporter_class) -> None:
    original = exporter_class._fetch_template_paths_for_bounds_with_empty

    def _fetch_template_paths_for_bounds_with_empty_parallel(
        self,
        client,
        bounds_list,
        *,
        status_callback=None,
        status_label: str,
    ):
        bounds = list(bounds_list)
        if len(bounds) <= 1:
            return original(
                self,
                client,
                bounds,
                status_callback=status_callback,
                status_label=status_label,
            )
        path_groups = _parallel_ordered(
            bounds,
            client.fetch_paths,
            max_workers=MAX_TEMPLATE_PATH_WORKERS,
            status_callback=status_callback,
            status_label=status_label,
        )
        paths: list[list[tuple[float, float]]] = []
        empty_bounds = []
        for item_bounds, fetched_paths in zip(bounds, path_groups):
            if not fetched_paths:
                empty_bounds.append(item_bounds)
            paths.extend(fetched_paths)
        return self._dedupe_template_paths(paths), empty_bounds

    exporter_class._fetch_template_paths_for_bounds_with_empty = (
        _fetch_template_paths_for_bounds_with_empty_parallel
    )


def _prepare_light_template_assets(exporter, task_kwargs: dict[str, object]) -> dict[str, object]:
    layer = task_kwargs["layer"]
    local_location_client = exporter._clone_template_location_client(task_kwargs.get("location_client"))
    local_background_provider = exporter._clone_template_background_provider(task_kwargs.get("background_provider"))
    local_page_exporter = exporter._clone_template_page_exporter(
        task_kwargs.get("page_exporter"),
        local_background_provider,
    )
    return {
        "formatted_address": exporter._reverse_geocoded_template_address(layer, local_location_client),
        "comments_text": exporter._template_comments_text(layer, task_kwargs.get("map_comments")),
        "map_raster_path": exporter._build_template_map_raster(
            task_kwargs["asset_dir"],
            layer,
            task_kwargs["label"],
            task_kwargs["index"],
            trench_mode=task_kwargs["trench_mode"],
            centerline_color=task_kwargs["centerline_color"],
            label_color=task_kwargs["label_color"],
            page_exporter=local_page_exporter,
            dxf_overlays=task_kwargs.get("dxf_overlays") or [],
            map_comments=task_kwargs.get("map_comments"),
            background_provider=local_background_provider,
            background_attribution=task_kwargs.get("background_attribution"),
            reference_annotation=task_kwargs.get("reference_annotation"),
        ),
    }


def _build_heavy_template_raster(exporter, task_kwargs: dict[str, object]):
    return exporter._build_template_tiff_raster(
        task_kwargs["asset_dir"],
        task_kwargs["layer"],
        task_kwargs["label"],
        task_kwargs["index"],
        task_kwargs.get("road_centerline_paths") or [],
        task_kwargs.get("terrain_boundary_paths") or [],
        profile=task_kwargs.get("profile"),
        reverse_orientation=bool(task_kwargs.get("reverse_tiff_orientation", False)),
    )


def _wait_with_ui_pump(future_map, status_callback, status_prefix: str) -> dict[int, Any]:
    pending = set(future_map)
    results: dict[int, Any] = {}
    completed = 0
    while pending:
        done, pending = wait(
            pending,
            timeout=TEMPLATE_UI_PUMP_INTERVAL_SECONDS,
            return_when=FIRST_COMPLETED,
        )
        if not done:
            _pump_template_ui(status_callback)
            continue
        for future in done:
            layer_index = future_map[future]
            results[layer_index] = future.result()
            completed += 1
            _safe_status(status_callback, f"{status_prefix}... {completed}/{len(future_map)}")
        _pump_template_ui(status_callback)
    return results


def _install_parallel_virtual_map_assets(exporter_class, prepared_assets_type) -> None:
    previous_batch = exporter_class._prepare_template_slot_assets_batch

    def _prepare_template_slot_assets_batch_parallel_maps(
        self,
        tasks: list[tuple[int, dict[str, object]]],
        *,
        status_callback=None,
    ):
        if not tasks or not _contains_virtual_template_task(tasks):
            return previous_batch(self, tasks, status_callback=status_callback)

        # Background prefetching already happens before this batch. Render the
        # comparatively light map/address assets two-at-a-time, then keep the
        # high-resolution TIFF rotate/crop stage strictly single-worker. This
        # preserves the memory fix while removing the old serial map bottleneck.
        light_workers = max(1, min(MAX_VIRTUAL_TEMPLATE_MAP_WORKERS, len(tasks)))
        with ThreadPoolExecutor(
            max_workers=light_workers,
            thread_name_prefix="template-maps-virtual",
        ) as executor:
            light_future_map = {
                executor.submit(_prepare_light_template_assets, self, task_kwargs): layer_index
                for layer_index, task_kwargs in tasks
            }
            light_assets = _wait_with_ui_pump(
                light_future_map,
                status_callback,
                "Bereid sjabloonkaarten voor",
            )

        with ThreadPoolExecutor(
            max_workers=1,
            thread_name_prefix="template-tiff-virtual",
        ) as executor:
            raster_future_map = {
                executor.submit(_build_heavy_template_raster, self, task_kwargs): layer_index
                for layer_index, task_kwargs in tasks
            }
            raster_paths = _wait_with_ui_pump(
                raster_future_map,
                status_callback,
                "Bereid hoge-res TIFF-afbeeldingen voor",
            )

        prepared = {}
        for layer_index, _task_kwargs in tasks:
            light = light_assets[layer_index]
            prepared[layer_index] = prepared_assets_type(
                formatted_address=light["formatted_address"],
                comments_text=light["comments_text"],
                raster_path=raster_paths[layer_index],
                map_raster_path=light["map_raster_path"],
            )
        return prepared

    exporter_class._prepare_template_slot_assets_batch = (
        _prepare_template_slot_assets_batch_parallel_maps
    )


def install_template_export_performance_patch() -> None:
    from .cadastral_export import CadastralDxfExporter, PreparedTemplateSlotAssets

    if int(getattr(CadastralDxfExporter, "_sleufbase_template_export_performance_version", 0) or 0) >= PATCH_VERSION:
        return

    _install_thread_safe_wfs_sessions()
    _install_local_bounds_parallel_fetch()
    _install_template_path_parallel_fetch(CadastralDxfExporter)
    _install_parallel_virtual_map_assets(CadastralDxfExporter, PreparedTemplateSlotAssets)
    CadastralDxfExporter._sleufbase_template_export_performance_version = PATCH_VERSION
