from __future__ import annotations

import subprocess
from pathlib import Path
from tkinter import messagebox
from typing import Any


PATCH_VERSION = 1
JOBS_MAP_TILE_CACHE_LIMIT = 96


def _clear_tile_client_memory(tile_client: Any) -> None:
    """Release in-memory map tiles and the current-thread HTTP session."""

    cache = getattr(tile_client, "_memory_cache", None)
    lock = getattr(tile_client, "_memory_cache_lock", None)
    if cache is not None:
        try:
            if lock is not None:
                with lock:
                    cache.clear()
            else:
                cache.clear()
        except Exception:
            pass

    thread_local = getattr(tile_client, "_thread_local", None)
    session = getattr(thread_local, "session", None) if thread_local is not None else None
    if session is not None:
        try:
            session.close()
        except Exception:
            pass
        try:
            delattr(thread_local, "session")
        except Exception:
            pass


def _limit_tile_client_memory(tile_client: Any) -> None:
    """Keep the Jobs map cache small enough for repeated open/close cycles."""

    try:
        current_limit = int(getattr(tile_client, "memory_cache_limit", JOBS_MAP_TILE_CACHE_LIMIT))
        tile_client.memory_cache_limit = min(current_limit, JOBS_MAP_TILE_CACHE_LIMIT)
    except Exception:
        pass


def _release_map_view_memory(view: Any) -> None:
    """Drop large PIL/Tk references as soon as the map view is closed."""

    try:
        view._sleufbase_jobs_destroyed = True
    except Exception:
        pass
    try:
        view._map_request_id = int(getattr(view, "_map_request_id", 0)) + 1
    except Exception:
        pass

    for attribute in (
        "_pending_map_result",
        "current_image",
        "photo_image",
        "_current_image_key",
        "_font_cache",
        "_drag_start",
        "_drag_last",
        "_selection_start",
    ):
        try:
            setattr(view, attribute, None)
        except Exception:
            pass

    for attribute in ("_marker_positions", "points"):
        value = getattr(view, attribute, None)
        try:
            if value is not None:
                value.clear()
        except Exception:
            pass

    label_cache = getattr(view, "_job_label_cache", None)
    try:
        if label_cache is not None:
            label_cache.clear()
    except Exception:
        pass

    tile_client = getattr(view, "tile_client", None)
    if tile_client is not None:
        _clear_tile_client_memory(tile_client)


def install_jobs_memory_patch() -> None:
    """Make the standalone Jobs process cheap to open and deterministic to close.

    The Jobs list used to start a hidden pywebview/WebView2 process immediately
    after every successful refresh. That browser was only a login warm-up and
    could remain alive when the login flow did not reach its final state. The
    real browser already knows how to log in when a job is opened, so warming a
    hidden browser is unnecessary and expensive.
    """

    from . import kickthemap_jobs_browser as jobs_module

    window_class = jobs_module.KickTheMapJobsWindow
    map_class = jobs_module.KickTheMapJobsMapView
    if int(getattr(window_class, "_sleufbase_jobs_memory_patch_version", 0) or 0) >= PATCH_VERSION:
        return

    def _skip_hidden_prelogin(self) -> None:
        # Mark it as handled so legacy refresh paths cannot try again later.
        self._browser_prelogin_started = True

    original_map_init = map_class.__init__
    original_map_destroy = map_class.destroy
    original_map_worker = map_class._load_map_worker

    def _memory_bounded_map_init(self, *args, **kwargs) -> None:
        original_map_init(self, *args, **kwargs)
        self._sleufbase_jobs_destroyed = False
        tile_client = getattr(self, "tile_client", None)
        if tile_client is not None:
            _limit_tile_client_memory(tile_client)

    def _memory_safe_map_worker(self, *args, **kwargs) -> None:
        if bool(getattr(self, "_sleufbase_jobs_destroyed", False)):
            return
        original_map_worker(self, *args, **kwargs)
        if bool(getattr(self, "_sleufbase_jobs_destroyed", False)):
            try:
                self._pending_map_result = None
            except Exception:
                pass

    def _memory_safe_map_destroy(self) -> None:
        _release_map_view_memory(self)
        try:
            original_map_destroy(self)
        except Exception:
            pass

    original_window_destroy = window_class.destroy

    def _memory_safe_window_destroy(self) -> None:
        map_view = getattr(self, "map_view", None)
        if map_view is not None:
            try:
                map_view.destroy()
            except Exception:
                pass
            try:
                self.map_view = None
            except Exception:
                pass
        try:
            original_window_destroy(self)
        except Exception:
            pass

    window_class._prelogin_kickthemap_browser = _skip_hidden_prelogin
    map_class.__init__ = _memory_bounded_map_init
    map_class._load_map_worker = _memory_safe_map_worker
    map_class.destroy = _memory_safe_map_destroy
    window_class.destroy = _memory_safe_window_destroy
    window_class._sleufbase_jobs_memory_patch_version = PATCH_VERSION
    map_class._sleufbase_jobs_memory_patch_version = PATCH_VERSION


def install_jobs_launcher_guard(viewer_class) -> None:
    """Allow only one standalone Jobs process per SleufBase main window."""

    if int(getattr(viewer_class, "_sleufbase_jobs_launcher_guard_version", 0) or 0) >= PATCH_VERSION:
        return
    if not callable(getattr(viewer_class, "open_kickthemap_jobs_browser_window", None)):
        return

    def _forget_finished_process(self, process) -> None:
        current = getattr(self, "_sleufbase_jobs_browser_process", None)
        if current is not process:
            return
        try:
            if process.poll() is None:
                self.after(1000, lambda: _forget_finished_process(self, process))
                return
        except Exception:
            pass
        try:
            self._sleufbase_jobs_browser_process = None
        except Exception:
            pass

    def _open_single_jobs_window(self) -> None:
        existing = getattr(self, "_sleufbase_jobs_browser_process", None)
        if existing is not None:
            try:
                if existing.poll() is None:
                    try:
                        self.set_status("KickTheMap Jobs staat al open.")
                    except Exception:
                        pass
                    return
            except Exception:
                pass

        account = self._selected_kickthemap_auto_login_account()
        if account is None:
            self.open_kickthemap_login_dialog()
            try:
                messagebox.showinfo(
                    "KickTheMap Jobs",
                    "Log eerst minimaal één keer in bij KickTheMap zodat de joblijst weet welk account hij moet openen.",
                )
            except Exception:
                pass
            return

        try:
            executable, arguments = self._kickthemap_jobs_browser_launch_command()
            process = subprocess.Popen(
                [executable] + arguments,
                cwd=str(Path(__file__).resolve().parent.parent),
            )
            self._sleufbase_jobs_browser_process = process
            self._register_child_process("kickthemap-jobs-browser", process)
            try:
                self.after(1000, lambda: _forget_finished_process(self, process))
            except Exception:
                pass
        except Exception as exc:
            messagebox.showerror("KickTheMap Jobs openen mislukt", str(exc))
            try:
                self.set_status("KickTheMap Jobs openen mislukt.")
            except Exception:
                pass
            return

        try:
            self.set_status("KickTheMap Jobs geopend.")
        except Exception:
            pass

    viewer_class.open_kickthemap_jobs_browser_window = _open_single_jobs_window
    viewer_class._sleufbase_jobs_launcher_guard_version = PATCH_VERSION
