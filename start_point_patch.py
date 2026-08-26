from __future__ import annotations

from dataclasses import replace
import json
import logging
import os
from pathlib import Path
from typing import Any, Iterable

START_POINT_KEY = "template_cross_section_start_point"
MANUAL_START_POINT_KEY = "template_cross_section_start_point_manual"
START_POINT_SOURCE_KEY = "template_cross_section_start_point_source"
START_POINT_SOURCE_MANUAL = "manual"
START_POINT_SOURCE_AUTOMATIC = "automatic"

PROJECT_START_POINTS_KEY = "_sleufbase_start_points"
PROJECT_START_POINTS_VERSION = 1
PATCH_VERSION = 2

_LOGGER = logging.getLogger("SleufBase.start_points")


def _normalized_xy(value: Any) -> tuple[float, float] | None:
    if not isinstance(value, (tuple, list)) or len(value) < 2:
        return None
    try:
        return float(value[0]), float(value[1])
    except (TypeError, ValueError):
        return None


def _metadata_for_layer(layer: Any) -> dict[str, Any] | None:
    metadata = getattr(layer, "metadata", None)
    return metadata if isinstance(metadata, dict) else None


def _is_manual_start_point(layer: Any) -> bool:
    metadata = _metadata_for_layer(layer)
    if metadata is None:
        return False
    return (
        metadata.get(START_POINT_SOURCE_KEY) == START_POINT_SOURCE_MANUAL
        or bool(metadata.get(MANUAL_START_POINT_KEY))
    ) and _normalized_xy(metadata.get(START_POINT_KEY)) is not None


def _set_start_point_source(layer: Any, source: str) -> None:
    metadata = _metadata_for_layer(layer)
    if metadata is None:
        return
    xy = _normalized_xy(metadata.get(START_POINT_KEY))
    if xy is None:
        metadata.pop(START_POINT_SOURCE_KEY, None)
        metadata.pop(MANUAL_START_POINT_KEY, None)
        return
    metadata[START_POINT_KEY] = xy
    metadata[START_POINT_SOURCE_KEY] = source
    if source == START_POINT_SOURCE_MANUAL:
        metadata[MANUAL_START_POINT_KEY] = True
    else:
        metadata.pop(MANUAL_START_POINT_KEY, None)


def _dataset_with_layer_start_point(dataset: Any, layer: Any) -> Any:
    if dataset is None:
        return None
    metadata = _metadata_for_layer(layer)
    if metadata is None:
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


def _path_text(value: Any) -> str:
    try:
        return str(Path(value))
    except (TypeError, ValueError):
        return str(value or "")


def _path_key(value: Any) -> str:
    text = _path_text(value).strip()
    if not text:
        return ""
    return os.path.normpath(text).replace("\\", "/").casefold()


def _layer_job_id(layer: Any) -> str:
    metadata = _metadata_for_layer(layer) or {}
    for key in ("kickthemap_job_id", "job_id"):
        value = metadata.get(key)
        if value not in (None, ""):
            return str(value)
    return ""


def _manual_start_point_entries(layers: Iterable[Any]) -> list[dict[str, Any]]:
    entries: list[dict[str, Any]] = []
    for index, layer in enumerate(layers):
        if not _is_manual_start_point(layer):
            continue
        metadata = _metadata_for_layer(layer) or {}
        xy = _normalized_xy(metadata.get(START_POINT_KEY))
        if xy is None:
            continue
        entries.append(
            {
                "index": index,
                "path": _path_text(getattr(layer, "path", "")),
                "name": str(getattr(layer, "name", "") or ""),
                "kickthemap_job_id": _layer_job_id(layer),
                "xy": [xy[0], xy[1]],
                "source": START_POINT_SOURCE_MANUAL,
            }
        )
    return entries


def _read_project_payload(path: Path) -> dict[str, Any] | None:
    try:
        payload = json.loads(path.read_text(encoding="utf-8-sig"))
    except (OSError, UnicodeError, json.JSONDecodeError):
        return None
    return payload if isinstance(payload, dict) else None


def _atomic_write_project_payload(path: Path, payload: dict[str, Any]) -> None:
    temp_path = path.with_name(f".{path.name}.start-points.{os.getpid()}.tmp")
    try:
        temp_path.write_text(
            json.dumps(payload, ensure_ascii=False, indent=2),
            encoding="utf-8",
        )
        os.replace(temp_path, path)
    finally:
        try:
            temp_path.unlink(missing_ok=True)
        except OSError:
            pass


def _persist_manual_start_points_to_project(app: Any, path_value: Any) -> bool:
    path = Path(path_value)
    payload = _read_project_payload(path)
    if payload is None:
        return False

    layers = list(getattr(app, "tiff_layers", ()) or ())
    payload[PROJECT_START_POINTS_KEY] = {
        "version": PROJECT_START_POINTS_VERSION,
        "manual": _manual_start_point_entries(layers),
    }
    try:
        _atomic_write_project_payload(path, payload)
    except OSError:
        _LOGGER.exception("Handmatige beginpunten konden niet in project worden opgeslagen: %s", path)
        return False
    app._sleufbase_last_project_path = path
    return True


def _project_state_entries(path_value: Any) -> list[dict[str, Any]]:
    path = Path(path_value)
    payload = _read_project_payload(path)
    if payload is None:
        return []
    state = payload.get(PROJECT_START_POINTS_KEY)
    if not isinstance(state, dict):
        return []
    entries = state.get("manual")
    if not isinstance(entries, list):
        return []
    return [entry for entry in entries if isinstance(entry, dict)]


def _match_entry_to_layer(
    entries: list[dict[str, Any]],
    layer: Any,
    index: int,
    used: set[int],
) -> int | None:
    layer_path = _path_key(getattr(layer, "path", ""))
    layer_job = _layer_job_id(layer)
    layer_name = str(getattr(layer, "name", "") or "").casefold()

    def first_match(predicate) -> int | None:
        for entry_index, entry in enumerate(entries):
            if entry_index in used:
                continue
            if predicate(entry):
                return entry_index
        return None

    if layer_path:
        match = first_match(lambda entry: _path_key(entry.get("path")) == layer_path)
        if match is not None:
            return match

    if layer_job:
        matching_job_entries = [
            entry_index
            for entry_index, entry in enumerate(entries)
            if entry_index not in used and str(entry.get("kickthemap_job_id") or "") == layer_job
        ]
        if len(matching_job_entries) == 1:
            return matching_job_entries[0]

    if layer_name:
        matching_names = [
            entry_index
            for entry_index, entry in enumerate(entries)
            if entry_index not in used and str(entry.get("name") or "").casefold() == layer_name
        ]
        if len(matching_names) == 1:
            return matching_names[0]

    match = first_match(lambda entry: entry.get("index") == index)
    return match


def _restore_manual_start_points_from_project(app: Any, path_value: Any) -> int:
    path = Path(path_value)
    entries = _project_state_entries(path)
    if not entries:
        return 0

    restored = 0
    used: set[int] = set()
    layers = list(getattr(app, "tiff_layers", ()) or ())
    for index, layer in enumerate(layers):
        entry_index = _match_entry_to_layer(entries, layer, index, used)
        if entry_index is None:
            continue
        xy = _normalized_xy(entries[entry_index].get("xy"))
        if xy is None:
            continue
        metadata = _metadata_for_layer(layer)
        if metadata is None:
            continue
        metadata[START_POINT_KEY] = xy
        metadata[START_POINT_SOURCE_KEY] = START_POINT_SOURCE_MANUAL
        metadata[MANUAL_START_POINT_KEY] = True
        used.add(entry_index)
        restored += 1

    if restored:
        app._sleufbase_last_project_path = path
        _refresh_after_start_point_change(app)
    return restored


def _refresh_after_start_point_change(app: Any) -> None:
    refresh = getattr(app, "_refresh_map_edit_markers", None)
    if callable(refresh):
        try:
            refresh()
        except Exception:
            _LOGGER.debug("Beginpuntmarkers verversen mislukte", exc_info=True)
    request_render = getattr(app, "request_render", None)
    if callable(request_render):
        try:
            request_render(immediate=False)
        except Exception:
            _LOGGER.debug("Beginpuntrender verversen mislukte", exc_info=True)


def _known_project_path(app: Any) -> Path | None:
    for attribute in (
        "_sleufbase_last_project_path",
        "current_project_path",
        "_current_project_path",
        "project_path",
        "_project_path",
        "project_file",
        "_project_file",
    ):
        value = getattr(app, attribute, None)
        if not value:
            continue
        try:
            path = Path(value)
        except (TypeError, ValueError):
            continue
        try:
            if path.is_file():
                return path
        except OSError:
            continue
    return None


def _set_status_quiet(app: Any, message: str) -> None:
    set_status = getattr(app, "set_status", None)
    if callable(set_status):
        try:
            set_status(message)
        except Exception:
            pass


def _autosave_manual_start_points(app: Any) -> bool:
    if getattr(app, "_sleufbase_start_point_persist_active", False):
        return False
    path = _known_project_path(app)
    if path is None:
        _set_status_quiet(
            app,
            "Handmatig beginpunt gewijzigd; het wordt meegenomen zodra het project wordt opgeslagen.",
        )
        return False
    app._sleufbase_start_point_persist_active = True
    try:
        saved = _persist_manual_start_points_to_project(app, path)
    finally:
        app._sleufbase_start_point_persist_active = False
    if saved:
        _set_status_quiet(app, "Handmatig beginpunt direct opgeslagen in project.")
    else:
        _set_status_quiet(
            app,
            "Beginpunt gewijzigd, maar automatisch opslaan in het projectbestand mislukte. Gebruik Project > Opslaan.",
        )
    return saved


class _PreloadedPathClient:
    def __init__(self, paths: Any) -> None:
        self._paths = list(paths or [])

    def fetch_paths(self, _bounds: Any) -> list[Any]:
        return list(self._paths)


def _safe_load_all_start_points(app: Any) -> None:
    """Sequential fallback for the all-start-points action.

    It deliberately keeps KlicViewerApp-bound dataset/candidate calls on the GUI
    thread. Network downloads and map lookups are also performed sequentially in
    this fallback: it is used only when the optimized implementation failed.
    """
    source_layers = [
        layer
        for layer in list(getattr(app, "tiff_layers", ()) or ())
        if not app._is_virtual_trench_layer(layer)
        and app._kickthemap_job_id_for_layer(layer) is not None
        and not _is_manual_start_point(layer)
    ]
    manual_count = sum(
        1
        for layer in list(getattr(app, "tiff_layers", ()) or ())
        if _is_manual_start_point(layer)
    )
    if not source_layers:
        if manual_count:
            app.set_status("Handmatige beginpunten blijven behouden; niets automatisch gewijzigd.")
        else:
            app.set_status("Geen geladen KickTheMap-proefsleuven om beginpunten voor te laden.")
        return

    warnings: list[str] = []
    ready_layers: list[Any] = []
    missing_layers: list[Any] = []
    for layer in source_layers:
        try:
            local_path = app._local_kickthemap_job_features_path(layer)
        except Exception as exc:
            warnings.append(f"{layer.name}: lokaal objectpuntenbestand kon niet worden gecontroleerd ({exc}).")
            local_path = None
        if local_path is None:
            missing_layers.append(layer)
        else:
            ready_layers.append(layer)

    if missing_layers:
        app.set_status(
            f"KickTheMap-objectpunten veilig voorbereiden voor {len(missing_layers)} proefsleuf/proefsleuven..."
        )
        app.update_idletasks()
        try:
            logged_in = bool(app.kickthemap_client.is_logged_in)
        except Exception:
            logged_in = False
        if not logged_in:
            try:
                logged_in = bool(app._try_auto_login_kickthemap())
            except Exception as exc:
                warnings.append(f"Automatisch inloggen bij KickTheMap mislukte ({exc}).")
                logged_in = False
        if not logged_in:
            warnings.extend(
                f"{layer.name}: log eerst in bij KickTheMap om objectpunten te laden."
                for layer in missing_layers
            )
            missing_layers = []

    if missing_layers:
        try:
            jobs_by_id = {job.job_id: job for job in app.kickthemap_client.fetch_jobs()}
        except Exception as exc:
            jobs_by_id = {}
            warnings.extend(
                f"{layer.name}: KickTheMap-jobs ophalen mislukt ({exc})."
                for layer in missing_layers
            )

        download_dir = app.kickthemap_client.default_download_dir()
        downloaded_by_job: dict[int, Any] = {}
        for layer in missing_layers:
            job_id = app._kickthemap_job_id_for_layer(layer)
            if job_id is None:
                continue
            job = jobs_by_id.get(job_id)
            if job is None:
                warnings.append(f"{layer.name}: KickTheMap-job {job_id} is niet gevonden.")
                continue
            if int(job_id) not in downloaded_by_job:
                try:
                    downloaded_by_job[int(job_id)] = app.kickthemap_client.download_job_features_file(
                        job, download_dir
                    )
                except Exception as exc:
                    downloaded_by_job[int(job_id)] = None
                    warnings.append(f"{layer.name}: objectpunten konden niet worden geladen ({exc}).")
            downloaded_path = downloaded_by_job.get(int(job_id))
            if downloaded_path is None:
                continue
            try:
                layer.metadata.update(app._kickthemap_tiff_metadata(job, downloaded_path))
                ready_layers.append(layer)
            except Exception as exc:
                warnings.append(f"{layer.name}: KickTheMap-metadata kon niet worden bijgewerkt ({exc}).")

    datasets_by_layer: dict[int, Any] = {}
    for index, layer in enumerate(ready_layers, start=1):
        app.set_status(f"Objectpunten veilig inlezen ({index}/{len(ready_layers)}): {layer.name}")
        app.update_idletasks()
        try:
            datasets_by_layer[id(layer)] = app._load_local_maaiveld_dataset_for_layer(layer)
        except Exception as exc:
            warnings.append(f"{layer.name}: objectpunten konden niet worden gelezen ({exc}).")

    candidate_layers = [layer for layer in ready_layers if id(layer) in datasets_by_layer]
    if not candidate_layers:
        _show_start_point_warnings(app, warnings)
        app.set_status("Er konden geen beginpunten worden geladen.")
        return

    try:
        resolved_rules = app._resolved_cross_section_layer_rules()
    except Exception as exc:
        _show_start_point_warnings(app, warnings + [f"Laagregels konden niet worden gelezen ({exc})."])
        app.set_status("Beginpunten laden is mislukt.")
        return

    results = 0
    for index, layer in enumerate(candidate_layers, start=1):
        app.set_status(f"Beginpunt veilig bepalen ({index}/{len(candidate_layers)}): {layer.name}")
        app.update_idletasks()
        try:
            fetch_bounds = layer.bounds.padded(app.cadastral_exporter._overview_padding(layer.bounds))
            road_paths = (
                app.road_centerline_client.fetch_paths(fetch_bounds)
                if getattr(app, "road_centerline_client", None) is not None
                else []
            )
        except Exception:
            road_paths = []
        try:
            terrain_paths = (
                app.bgt_terrain_boundary_client.fetch_paths(fetch_bounds)
                if getattr(app, "bgt_terrain_boundary_client", None) is not None
                else []
            )
        except Exception:
            terrain_paths = []
        try:
            candidate = app._auto_cross_section_start_candidate_for_layer(
                layer,
                datasets_by_layer[id(layer)],
                resolved_rules,
                road_centerline_client=_PreloadedPathClient(road_paths),
                terrain_boundary_client=_PreloadedPathClient(terrain_paths),
            )
        except Exception as exc:
            warnings.append(f"{layer.name}: beginpunt bepalen mislukt ({exc}).")
            continue
        if candidate is None:
            warnings.append(
                f"{layer.name}: onvoldoende profielpunten met hoogte of geen beginpuntkandidaten gevonden."
            )
            continue
        app._set_automatic_template_cross_section_start_metadata(
            layer, float(candidate.x), float(candidate.y)
        )
        results += 1

    _refresh_after_start_point_change(app)
    _show_start_point_warnings(app, warnings)
    if results:
        suffix = f"; {manual_count} handmatig behouden" if manual_count else ""
        app.set_status(f"Beginpunten geladen voor {results} KickTheMap-proefsleuf/proefsleuven{suffix}.")
    else:
        app.set_status("Er konden geen automatische beginpunten worden geladen.")


def _show_start_point_warnings(app: Any, warnings: list[str]) -> None:
    if not warnings:
        return
    try:
        from tkinter import messagebox

        messagebox.showwarning(
            "Beginpunten laden",
            "\n".join(warnings[:15]),
            parent=app,
        )
    except Exception:
        _LOGGER.warning("Beginpunten laden waarschuwingen: %s", " | ".join(warnings[:15]))


def _extract_path_candidates(args: tuple[Any, ...], kwargs: dict[str, Any]) -> list[Path]:
    values: list[Any] = list(args)
    values.extend(kwargs.values())
    paths: list[Path] = []
    for value in values:
        if isinstance(value, (str, os.PathLike)):
            try:
                path = Path(value)
            except (TypeError, ValueError):
                continue
            if path.is_file():
                paths.append(path)
    return paths


def _restore_first_matching_project(app: Any, candidates: Iterable[Path]) -> int:
    seen: set[str] = set()
    for path in candidates:
        key = _path_key(path)
        if not key or key in seen:
            continue
        seen.add(key)
        entries = _project_state_entries(path)
        if not entries:
            continue
        app._sleufbase_last_project_path = path
        return _restore_manual_start_points_from_project(app, path)
    return 0


def _patch_viewer_class(viewer_class: type[Any]) -> None:
    if viewer_class.__dict__.get("_manual_cross_section_start_patch", False) and int(
        viewer_class.__dict__.get("_sleufbase_start_point_patch_version", 0) or 0
    ) >= PATCH_VERSION:
        return

    original_set_start = getattr(viewer_class, "_set_template_cross_section_start_metadata", None)
    if callable(original_set_start):
        def _set_template_cross_section_start_metadata(
            self: Any, layer: Any, start_x: Any, start_y: Any, *args: Any, **kwargs: Any
        ) -> Any:
            automatic = bool(getattr(self, "_sleufbase_auto_start_points_active", False))
            if automatic and _is_manual_start_point(layer):
                # Hard invariant: automatic code never gets to mutate a manual point,
                # not even temporarily before a later restore.
                return None

            result = original_set_start(self, layer, start_x, start_y, *args, **kwargs)
            metadata = _metadata_for_layer(layer)
            if metadata is not None and _normalized_xy(metadata.get(START_POINT_KEY)) is None:
                xy = _normalized_xy((start_x, start_y))
                if xy is not None:
                    metadata[START_POINT_KEY] = xy
            _set_start_point_source(
                layer,
                START_POINT_SOURCE_AUTOMATIC if automatic else START_POINT_SOURCE_MANUAL,
            )
            if not automatic:
                _autosave_manual_start_points(self)
            return result

        viewer_class._set_template_cross_section_start_metadata = _set_template_cross_section_start_metadata

        def _set_automatic_template_cross_section_start_metadata(
            self: Any, layer: Any, start_x: Any, start_y: Any, *args: Any, **kwargs: Any
        ) -> Any:
            previous = bool(getattr(self, "_sleufbase_auto_start_points_active", False))
            self._sleufbase_auto_start_points_active = True
            try:
                return self._set_template_cross_section_start_metadata(
                    layer, start_x, start_y, *args, **kwargs
                )
            finally:
                self._sleufbase_auto_start_points_active = previous

        viewer_class._set_automatic_template_cross_section_start_metadata = (
            _set_automatic_template_cross_section_start_metadata
        )

    original_load_all = getattr(viewer_class, "load_all_kickthemap_start_points", None)
    if callable(original_load_all):
        def _load_all_kickthemap_start_points_preserving_manual(
            self: Any, *args: Any, **kwargs: Any
        ) -> Any:
            all_layers = getattr(self, "tiff_layers", None)
            manual_points: list[tuple[Any, tuple[float, float]]] = []
            if isinstance(all_layers, list):
                for layer in all_layers:
                    if not _is_manual_start_point(layer):
                        continue
                    metadata = _metadata_for_layer(layer) or {}
                    xy = _normalized_xy(metadata.get(START_POINT_KEY))
                    if xy is not None:
                        manual_points.append((layer, xy))

            if manual_points:
                try:
                    has_automatic_work = any(
                        not _is_manual_start_point(layer)
                        and not self._is_virtual_trench_layer(layer)
                        and self._kickthemap_job_id_for_layer(layer) is not None
                        for layer in all_layers
                    )
                except Exception:
                    has_automatic_work = True
                if not has_automatic_work:
                    self.set_status(
                        "Alle beschikbare beginpunten zijn handmatig gekozen; niets automatisch gewijzigd."
                    )
                    _refresh_after_start_point_change(self)
                    return None

            previous_auto = bool(getattr(self, "_sleufbase_auto_start_points_active", False))
            self._sleufbase_auto_start_points_active = True
            filtered = False
            try:
                # Skip manual layers entirely in the optimized automatic loader.
                # This both improves speed and removes any opportunity to overwrite.
                if isinstance(all_layers, list) and manual_points:
                    try:
                        self.tiff_layers = [
                            layer for layer in all_layers if not _is_manual_start_point(layer)
                        ]
                        filtered = True
                    except Exception:
                        filtered = False
                try:
                    return original_load_all(self, *args, **kwargs)
                except Exception:
                    _LOGGER.exception(
                        "Snelle 'alle beginpunten laden'-methode faalde; veilige fallback wordt gebruikt."
                    )
                    if filtered:
                        self.tiff_layers = all_layers
                        filtered = False
                    return _safe_load_all_start_points(self)
            finally:
                self._sleufbase_auto_start_points_active = previous_auto
                if filtered:
                    self.tiff_layers = all_layers
                for layer, xy in manual_points:
                    metadata = _metadata_for_layer(layer)
                    if metadata is not None:
                        metadata[START_POINT_KEY] = xy
                        metadata[START_POINT_SOURCE_KEY] = START_POINT_SOURCE_MANUAL
                        metadata[MANUAL_START_POINT_KEY] = True
                if manual_points:
                    _refresh_after_start_point_change(self)

        viewer_class.load_all_kickthemap_start_points = (
            _load_all_kickthemap_start_points_preserving_manual
        )

    for method_name in ("_load_local_maaiveld_dataset_for_layer", "_load_maaiveld_dataset_for_layer"):
        original_loader = getattr(viewer_class, method_name, None)
        if not callable(original_loader):
            continue

        def _make_loader(loader):
            def _loader(self: Any, layer: Any, *args: Any, **kwargs: Any) -> Any:
                dataset = loader(self, layer, *args, **kwargs)
                return _dataset_with_layer_start_point(dataset, layer)
            return _loader

        setattr(viewer_class, method_name, _make_loader(original_loader))

    original_save_to_path = getattr(viewer_class, "_save_project_to_path", None)
    if callable(original_save_to_path):
        def _save_project_to_path_with_start_points(
            self: Any, path_value: Any, *args: Any, **kwargs: Any
        ) -> Any:
            result = original_save_to_path(self, path_value, *args, **kwargs)
            try:
                path = Path(path_value)
            except (TypeError, ValueError):
                return result
            if path.is_file():
                self._sleufbase_last_project_path = path
                if not _persist_manual_start_points_to_project(self, path):
                    _LOGGER.warning(
                        "Project is opgeslagen, maar beginpuntstate kon niet apart worden verankerd: %s",
                        path,
                    )
            return result

        viewer_class._save_project_to_path = _save_project_to_path_with_start_points

    load_method_names = (
        "load_project",
        "_load_project_from_path",
        "load_project_from_path",
        "_load_project_path",
    )
    for method_name in load_method_names:
        original_load = getattr(viewer_class, method_name, None)
        if not callable(original_load):
            continue

        def _make_project_loader(loader, loader_name: str):
            def _project_loader(self: Any, *args: Any, **kwargs: Any) -> Any:
                candidates = _extract_path_candidates(args, kwargs)
                captured: list[Path] = []
                filedialog_module = None
                original_askopen = None
                original_askopens = None
                if loader_name == "load_project" and not candidates:
                    try:
                        from tkinter import filedialog as filedialog_module

                        original_askopen = filedialog_module.askopenfilename
                        original_askopens = getattr(filedialog_module, "askopenfilenames", None)

                        def _capture_selected(selected: Any) -> Any:
                            values = selected if isinstance(selected, (tuple, list)) else (selected,)
                            for value in values:
                                if not value:
                                    continue
                                try:
                                    captured.append(Path(value))
                                except (TypeError, ValueError):
                                    pass
                            return selected

                        def _capture_askopenfilename(*dialog_args: Any, **dialog_kwargs: Any):
                            return _capture_selected(
                                original_askopen(*dialog_args, **dialog_kwargs)
                            )

                        filedialog_module.askopenfilename = _capture_askopenfilename
                        if callable(original_askopens):
                            def _capture_askopenfilenames(*dialog_args: Any, **dialog_kwargs: Any):
                                return _capture_selected(
                                    original_askopens(*dialog_args, **dialog_kwargs)
                                )

                            filedialog_module.askopenfilenames = _capture_askopenfilenames
                    except Exception:
                        filedialog_module = None
                        original_askopen = None
                        original_askopens = None
                try:
                    result = loader(self, *args, **kwargs)
                finally:
                    if filedialog_module is not None and original_askopen is not None:
                        filedialog_module.askopenfilename = original_askopen
                    if filedialog_module is not None and callable(original_askopens):
                        filedialog_module.askopenfilenames = original_askopens

                candidates.extend(captured)
                known_path = _known_project_path(self)
                if known_path is not None:
                    candidates.append(known_path)
                for candidate in candidates:
                    try:
                        if candidate.is_file():
                            self._sleufbase_last_project_path = candidate
                            break
                    except OSError:
                        continue
                if candidates:
                    _restore_first_matching_project(self, candidates)
                return result

            return _project_loader

        setattr(viewer_class, method_name, _make_project_loader(original_load, method_name))

    for method_name in (
        "clear_all_reference_points",
        "clear_all_kickthemap_start_points",
        "clear_template_cross_section_start_point",
        "_clear_template_cross_section_start_metadata",
    ):
        original_clear = getattr(viewer_class, method_name, None)
        if not callable(original_clear):
            continue

        def _make_clearer(clearer):
            def _clearer(self: Any, *args: Any, **kwargs: Any) -> Any:
                result = clearer(self, *args, **kwargs)
                changed = False
                for layer in list(getattr(self, "tiff_layers", ()) or ()):
                    metadata = _metadata_for_layer(layer)
                    if metadata is None or START_POINT_KEY in metadata:
                        continue
                    if (
                        MANUAL_START_POINT_KEY in metadata
                        or START_POINT_SOURCE_KEY in metadata
                    ):
                        metadata.pop(MANUAL_START_POINT_KEY, None)
                        metadata.pop(START_POINT_SOURCE_KEY, None)
                        changed = True
                if changed:
                    _autosave_manual_start_points(self)
                return result
            return _clearer

        setattr(viewer_class, method_name, _make_clearer(original_clear))

    viewer_class._is_manual_template_cross_section_start = staticmethod(_is_manual_start_point)
    viewer_class._persist_manual_start_points_to_project = _persist_manual_start_points_to_project
    viewer_class._restore_manual_start_points_from_project = _restore_manual_start_points_from_project
    viewer_class._manual_cross_section_start_patch = True
    viewer_class._sleufbase_start_point_patch_version = PATCH_VERSION


def install_manual_start_point_patch() -> None:
    from .app import KlicViewerApp

    _patch_viewer_class(KlicViewerApp)
