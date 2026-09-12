from __future__ import annotations

from concurrent.futures import FIRST_COMPLETED, ThreadPoolExecutor, wait
from dataclasses import dataclass
import json
from pathlib import Path
import threading
import time
from tkinter import messagebox
from typing import Any, Callable, Iterable

from .resource_policy import get_resource_policy


PATCH_VERSION = 1
MAX_KICKTHEMAP_NETWORK_WORKERS = 12
MAX_INFLIGHT_MULTIPLIER = 2
DOWNLOAD_ATTEMPTS = 3


def _safe_unlink(path: Path) -> None:
    try:
        path.unlink(missing_ok=True)
    except OSError:
        pass


def _valid_job_features_file(path: Path) -> bool:
    try:
        if path.stat().st_size <= 2:
            return False
        with path.open("r", encoding="utf-8", errors="strict") as handle:
            payload = json.load(handle)
    except (OSError, UnicodeError, json.JSONDecodeError, TypeError, ValueError):
        return False
    return isinstance(payload, dict) and isinstance(payload.get("features"), list)


def _candidate_path(target_path: Path, kind: str) -> Path:
    return target_path.with_name(
        f".{target_path.name}.{kind}-{threading.get_ident()}-{time.time_ns()}"
    )


def _download_fresh_job_features(worker_client, job, target_path: Path, file_name: str) -> Path:
    last_error: Exception | None = None
    for attempt in range(DOWNLOAD_ATTEMPTS):
        candidate = _candidate_path(target_path, "fresh")
        try:
            worker_client._download_project_file(
                job,
                folder="cloud",
                remote_file_name="jobFeatures.json",
                target_path=candidate,
                export_name=file_name,
                legacy_file_key=worker_client._job_features_key(job),
            )
            if not _valid_job_features_file(candidate):
                raise RuntimeError("KickTheMap gaf geen geldig jobFeatures.json-bestand terug")
            candidate.replace(target_path)
            return target_path
        except Exception as exc:
            last_error = exc
            _safe_unlink(candidate)
            _safe_unlink(candidate.with_name(f"{candidate.name}.tmp"))
            if attempt + 1 < DOWNLOAD_ATTEMPTS:
                time.sleep(min(0.15 * (2**attempt), 0.6))
    assert last_error is not None
    raise last_error


def _download_fresh_tiff(worker_client, job, target_path: Path, file_name: str) -> Path:
    last_error: Exception | None = None
    for attempt in range(DOWNLOAD_ATTEMPTS):
        candidate = _candidate_path(target_path, "fresh-tiff")
        try:
            worker_client._download_project_file(
                job,
                folder="cloud",
                remote_file_name=f"{worker_client._job_storage_prefix(job)}.tiff",
                target_path=candidate,
                export_name=file_name,
                legacy_file_key=worker_client._job_tiff_key(job),
            )
            worker_client._validate_geotiff_file(candidate)
            candidate.replace(target_path)
            return target_path
        except Exception as exc:
            last_error = exc
            _safe_unlink(candidate)
            _safe_unlink(candidate.with_name(f"{candidate.name}.tmp"))
            if attempt + 1 < DOWNLOAD_ATTEMPTS:
                time.sleep(min(0.15 * (2**attempt), 0.6))
    assert last_error is not None
    raise last_error


def _unique_jobs(jobs: Iterable[Any]) -> list[Any]:
    result: list[Any] = []
    seen: set[int] = set()
    for job in jobs:
        try:
            job_id = int(job.job_id)
        except (AttributeError, TypeError, ValueError):
            continue
        if job_id in seen:
            continue
        seen.add(job_id)
        result.append(job)
    return result


def _rolling_download(
    specs: list[tuple[Any, Path, str]],
    worker: Callable[[tuple[Any, Path, str]], tuple[int, Path]],
    *,
    max_workers: int,
    progress_callback: Callable[[int, int], None] | None,
    completed_start: int = 0,
    total: int,
) -> tuple[dict[int, Path], dict[int, Exception]]:
    paths: dict[int, Path] = {}
    errors: dict[int, Exception] = {}
    if not specs:
        return paths, errors

    worker_count = max(1, min(int(max_workers), len(specs)))
    inflight_limit = max(
        worker_count,
        min(len(specs), worker_count * MAX_INFLIGHT_MULTIPLIER),
    )
    iterator = iter(specs)
    future_map: dict[Any, Any] = {}
    completed = int(completed_start)
    executor = ThreadPoolExecutor(
        max_workers=worker_count,
        thread_name_prefix="kickthemap-bounded",
    )

    def fill_queue() -> None:
        while len(future_map) < inflight_limit:
            try:
                spec = next(iterator)
            except StopIteration:
                return
            future_map[executor.submit(worker, spec)] = spec[0]

    try:
        fill_queue()
        while future_map:
            done, _pending = wait(
                set(future_map),
                return_when=FIRST_COMPLETED,
            )
            for future in done:
                job = future_map.pop(future)
                try:
                    job_id, path = future.result()
                    paths[int(job_id)] = path
                except Exception as exc:
                    errors[int(job.job_id)] = exc
                completed += 1
                if progress_callback is not None:
                    progress_callback(completed, total)
            fill_queue()
    finally:
        executor.shutdown(wait=True, cancel_futures=True)
    return paths, errors


def install_kickthemap_client_scalability_patch() -> None:
    from .kickthemap import KickTheMapClient, KickTheMapError

    if int(getattr(KickTheMapClient, "_sleufbase_scalability_patch_version", 0) or 0) >= PATCH_VERSION:
        return

    def download_job_features_files_bounded(
        self,
        jobs,
        target_dir=None,
        *,
        max_workers: int = 6,
        progress_callback=None,
        force_refresh: bool = False,
    ):
        self._ensure_logged_in()
        job_list = _unique_jobs(jobs)
        if not job_list:
            return {}, {}

        target_root = target_dir or self.default_download_dir()
        target_root.mkdir(parents=True, exist_ok=True)
        paths: dict[int, Path] = {}
        specs: list[tuple[Any, Path, str]] = []
        completed = 0

        for job in job_list:
            target_path = target_root / f"{job.safe_file_stem}_{job.job_id}_jobFeatures.json"
            file_name = f"{job.safe_file_stem}_jobFeatures.json"
            if not force_refresh and self._recent_job_features_path(job.job_id, target_path) is not None:
                paths[int(job.job_id)] = target_path
                completed += 1
                if progress_callback is not None:
                    progress_callback(completed, len(job_list))
                continue
            specs.append((job, target_path, file_name))

        policy = get_resource_policy()
        workers = min(
            max(1, int(max_workers)),
            max(1, int(policy.network_workers)),
            MAX_KICKTHEMAP_NETWORK_WORKERS,
        )

        def download_one(spec):
            job, target_path, file_name = spec
            worker_client = self._parallel_worker_client()
            path = _download_fresh_job_features(worker_client, job, target_path, file_name)
            return int(job.job_id), path

        downloaded, errors = _rolling_download(
            specs,
            download_one,
            max_workers=workers,
            progress_callback=progress_callback,
            completed_start=completed,
            total=len(job_list),
        )
        paths.update(downloaded)
        for job_id, path in downloaded.items():
            self._remember_job_features_path(job_id, path)

        wrapped_errors = {
            job_id: (
                exc
                if isinstance(exc, KickTheMapError)
                else KickTheMapError(f"KickTheMap-objectdata kon niet vers worden opgehaald ({exc}).")
            )
            for job_id, exc in errors.items()
        }
        return paths, wrapped_errors

    def download_tiffs_bounded(
        self,
        jobs,
        target_dir=None,
        *,
        max_workers: int = 6,
        progress_callback=None,
    ):
        self._ensure_logged_in()
        job_list = _unique_jobs(jobs)
        if not job_list:
            return {}, {}

        target_root = target_dir or self.default_download_dir()
        target_root.mkdir(parents=True, exist_ok=True)
        specs = [
            (
                job,
                target_root / f"{job.safe_file_stem}_{job.job_id}.tiff",
                f"{job.safe_file_stem}.tiff",
            )
            for job in job_list
        ]
        policy = get_resource_policy()
        workers = min(
            max(1, int(max_workers)),
            max(1, int(policy.network_workers)),
            MAX_KICKTHEMAP_NETWORK_WORKERS,
        )

        def download_one(spec):
            job, target_path, file_name = spec
            worker_client = self._parallel_worker_client()
            path = _download_fresh_tiff(worker_client, job, target_path, file_name)
            return int(job.job_id), path

        paths, errors = _rolling_download(
            specs,
            download_one,
            max_workers=workers,
            progress_callback=progress_callback,
            total=len(job_list),
        )
        wrapped_errors = {
            job_id: (
                exc
                if isinstance(exc, KickTheMapError)
                else KickTheMapError(f"GeoTIFF kon niet worden gedownload ({exc}).")
            )
            for job_id, exc in errors.items()
        }
        return paths, wrapped_errors

    KickTheMapClient.download_job_features_files = download_job_features_files_bounded
    KickTheMapClient.download_tiffs = download_tiffs_bounded
    KickTheMapClient._sleufbase_scalability_patch_version = PATCH_VERSION


@dataclass(frozen=True)
class TemplateKickTheMapSnapshot:
    jobs_by_id: dict[int, Any]
    layers_by_job: dict[int, tuple[Any, ...]]
    paths: dict[int, Path]
    errors: dict[int, Exception]
    missing_job_ids: tuple[int, ...]


def _template_kickthemap_layers(app) -> list[Any]:
    layers: list[Any] = []
    for layer in list(getattr(app, "tiff_layers", ()) or ()):
        try:
            if app._is_virtual_trench_layer(layer):
                continue
            job_id = app._kickthemap_job_id_for_layer(layer)
        except Exception:
            continue
        if job_id is not None:
            layers.append(layer)
    return layers


def _refresh_template_kickthemap_snapshot(
    app,
    layers: Iterable[Any],
    *,
    progress_callback=None,
) -> TemplateKickTheMapSnapshot:
    layers_by_job: dict[int, list[Any]] = {}
    for layer in layers:
        job_id = app._kickthemap_job_id_for_layer(layer)
        if job_id is not None:
            layers_by_job.setdefault(int(job_id), []).append(layer)

    jobs_by_id = {int(job.job_id): job for job in app.kickthemap_client.fetch_jobs()}
    missing = tuple(sorted(job_id for job_id in layers_by_job if job_id not in jobs_by_id))
    requested_jobs = [jobs_by_id[job_id] for job_id in layers_by_job if job_id in jobs_by_id]

    policy = get_resource_policy()
    workers = min(
        MAX_KICKTHEMAP_NETWORK_WORKERS,
        max(1, int(policy.network_workers)),
        max(1, len(requested_jobs)),
    )
    paths, errors = app.kickthemap_client.download_job_features_files(
        requested_jobs,
        app.kickthemap_client.default_download_dir(),
        max_workers=workers,
        progress_callback=progress_callback,
        force_refresh=True,
    )
    return TemplateKickTheMapSnapshot(
        jobs_by_id=jobs_by_id,
        layers_by_job={job_id: tuple(items) for job_id, items in layers_by_job.items()},
        paths=paths,
        errors=errors,
        missing_job_ids=missing,
    )


def _apply_template_kickthemap_snapshot(app, snapshot: TemplateKickTheMapSnapshot) -> None:
    for job_id, layers in snapshot.layers_by_job.items():
        job = snapshot.jobs_by_id[job_id]
        path = snapshot.paths[job_id]
        metadata = app._kickthemap_tiff_metadata(job, path)
        for layer in layers:
            layer.metadata.update(metadata)
            layer.metadata["kickthemap_features_refreshed_at"] = time.time()

    cache_lock = getattr(app, "_kickthemap_dataset_cache_lock", None)
    cache = getattr(app, "_kickthemap_dataset_cache", None)
    if isinstance(cache, dict):
        if cache_lock is None:
            cache.clear()
        else:
            with cache_lock:
                cache.clear()


def _schedule_ui(app, callback) -> None:
    try:
        app.after(0, callback)
    except Exception:
        callback()


def _patch_viewer_class(viewer_class) -> None:
    if int(getattr(viewer_class, "_sleufbase_kickthemap_template_freshness_version", 0) or 0) >= PATCH_VERSION:
        return

    original_export = viewer_class.export_cadastral_template_dxf

    def export_with_fresh_kickthemap_snapshot(self, *args, **kwargs):
        if bool(getattr(self, "_sleufbase_kickthemap_template_refresh_active", False)):
            try:
                self.set_status("De nieuwste KickTheMap kabels/leidingen worden al opgehaald...")
            except Exception:
                pass
            return None

        layers = _template_kickthemap_layers(self)
        if not layers:
            return original_export(self, *args, **kwargs)

        try:
            logged_in = bool(self.kickthemap_client.is_logged_in)
        except Exception:
            logged_in = False
        if not logged_in:
            try:
                logged_in = bool(self._try_auto_login_kickthemap())
            except Exception:
                logged_in = False
        if not logged_in:
            messagebox.showerror(
                "DXF-sjabloonexport",
                "Log eerst in bij KickTheMap. Voor de sjabloonexport worden de nieuwste "
                "kabels/leidingen verplicht opnieuw opgehaald.",
                parent=self,
            )
            return None

        self._sleufbase_kickthemap_template_refresh_active = True
        unique_jobs = {
            int(self._kickthemap_job_id_for_layer(layer))
            for layer in layers
            if self._kickthemap_job_id_for_layer(layer) is not None
        }
        try:
            self.set_status(
                f"Nieuwste KickTheMap kabels/leidingen ophalen... 0/{len(unique_jobs)}"
            )
            self.update_idletasks()
        except Exception:
            pass

        def progress(completed: int, total: int) -> None:
            _schedule_ui(
                self,
                lambda: self.set_status(
                    f"Nieuwste KickTheMap kabels/leidingen ophalen... {completed}/{total}"
                ),
            )

        def fail(exc: Exception) -> None:
            self._sleufbase_kickthemap_template_refresh_active = False
            try:
                self.set_status("DXF-sjabloonexport geannuleerd: KickTheMap-data niet volledig vers.")
            except Exception:
                pass
            messagebox.showerror(
                "DXF-sjabloonexport",
                "De nieuwste KickTheMap kabels/leidingen konden niet volledig worden opgehaald.\n\n"
                f"{exc}\n\nDe export is gestopt zodat geen verouderde kabeldata wordt gebruikt.",
                parent=self,
            )

        def finish(snapshot: TemplateKickTheMapSnapshot) -> None:
            problems: list[str] = []
            for job_id in snapshot.missing_job_ids:
                problems.append(f"KickTheMap-job {job_id} is niet meer gevonden.")
            for job_id, error in sorted(snapshot.errors.items()):
                problems.append(f"Job {job_id}: {error}")
            for job_id in snapshot.layers_by_job:
                if job_id not in snapshot.paths and job_id not in snapshot.missing_job_ids:
                    problems.append(f"Job {job_id}: geen vers jobFeatures-bestand ontvangen.")
            if problems:
                fail(RuntimeError("\n".join(problems[:12])))
                return

            try:
                _apply_template_kickthemap_snapshot(self, snapshot)
                self.set_status(
                    f"Nieuwste KickTheMap kabels/leidingen geladen voor {len(snapshot.paths)} job(s)."
                )
                original_export(self, *args, **kwargs)
            finally:
                self._sleufbase_kickthemap_template_refresh_active = False

        def worker() -> None:
            try:
                snapshot = _refresh_template_kickthemap_snapshot(
                    self,
                    layers,
                    progress_callback=progress,
                )
            except Exception as exc:
                _schedule_ui(self, lambda exc=exc: fail(exc))
                return
            _schedule_ui(self, lambda snapshot=snapshot: finish(snapshot))

        threading.Thread(
            target=worker,
            name="kickthemap-template-freshness",
            daemon=True,
        ).start()
        return None

    viewer_class.export_cadastral_template_dxf = export_with_fresh_kickthemap_snapshot
    viewer_class._sleufbase_kickthemap_template_freshness_version = PATCH_VERSION


def install_kickthemap_template_export_hook() -> None:
    """Apply the app patch after the normal manual-start-point installer imports app.py."""

    from . import start_point_patch

    if bool(getattr(start_point_patch, "_kickthemap_template_export_hook_installed", False)):
        return
    original_install = start_point_patch.install_manual_start_point_patch

    def install_start_points_and_fresh_template_export() -> None:
        original_install()
        from .app import KlicViewerApp

        _patch_viewer_class(KlicViewerApp)

    start_point_patch.install_manual_start_point_patch = install_start_points_and_fresh_template_export
    start_point_patch._kickthemap_template_export_hook_installed = True
