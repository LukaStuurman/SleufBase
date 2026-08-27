from __future__ import annotations

from dataclasses import asdict, dataclass
import json
import logging
import os
from pathlib import Path
import subprocess
import sys
from typing import Any
import uuid

_LOGGER = logging.getLogger("SleufBase.autosave")

DEFAULT_INTERVAL_MINUTES = 10
DEFAULT_MAX_BACKUPS = 20
MIN_INTERVAL_MINUTES = 1
MAX_INTERVAL_MINUTES = 1440
MIN_BACKUPS = 1
MAX_BACKUPS = 200
CONFIG_FILENAME = "autosave-settings.json"
BACKUP_DIRNAME = "Backups"
BACKUP_PREFIX = "autosave_"
PATCH_VERSION = 1

_STATE_ATTRS = (
    "current_project_path",
    "_current_project_path",
    "project_path",
    "_project_path",
    "_sleufbase_last_project_path",
    "dirty",
    "_dirty",
    "project_dirty",
    "_project_dirty",
    "is_dirty",
    "_is_dirty",
)


@dataclass(frozen=True)
class AutosaveSettings:
    enabled: bool = True
    interval_minutes: int = DEFAULT_INTERVAL_MINUTES
    max_backups: int = DEFAULT_MAX_BACKUPS

    def normalized(self) -> "AutosaveSettings":
        return AutosaveSettings(
            enabled=bool(self.enabled),
            interval_minutes=max(
                MIN_INTERVAL_MINUTES,
                min(MAX_INTERVAL_MINUTES, int(self.interval_minutes)),
            ),
            max_backups=max(MIN_BACKUPS, min(MAX_BACKUPS, int(self.max_backups))),
        )


def _app_data_dir() -> Path:
    local_appdata = os.environ.get("LOCALAPPDATA")
    if local_appdata:
        return Path(local_appdata) / "SleufBase"
    return Path.home() / ".sleufbase"


def settings_path() -> Path:
    return _app_data_dir() / CONFIG_FILENAME


def backup_directory() -> Path:
    return _app_data_dir() / BACKUP_DIRNAME


def load_autosave_settings(path: Path | None = None) -> AutosaveSettings:
    path = path or settings_path()
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
        if not isinstance(payload, dict):
            raise ValueError("settings payload is not an object")
        return AutosaveSettings(
            enabled=bool(payload.get("enabled", True)),
            interval_minutes=int(
                payload.get("interval_minutes", DEFAULT_INTERVAL_MINUTES)
            ),
            max_backups=int(payload.get("max_backups", DEFAULT_MAX_BACKUPS)),
        ).normalized()
    except (OSError, ValueError, TypeError, json.JSONDecodeError):
        return AutosaveSettings()


def save_autosave_settings(
    settings: AutosaveSettings, path: Path | None = None
) -> AutosaveSettings:
    normalized = settings.normalized()
    path = path or settings_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    temp_path = path.with_name(
        f".{path.name}.{os.getpid()}.{uuid.uuid4().hex}.tmp"
    )
    try:
        temp_path.write_text(
            json.dumps(asdict(normalized), ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )
        os.replace(temp_path, path)
    finally:
        try:
            temp_path.unlink(missing_ok=True)
        except OSError:
            pass
    return normalized


def _project_suffix(app: Any) -> str:
    for name in (
        "current_project_path",
        "_current_project_path",
        "project_path",
        "_project_path",
        "_sleufbase_last_project_path",
    ):
        value = getattr(app, name, None)
        if not value:
            continue
        try:
            suffix = Path(value).suffix
        except (TypeError, ValueError):
            continue
        if suffix and len(suffix) <= 16:
            return suffix
    return ".json"


def _new_backup_path(app: Any, directory: Path | None = None) -> Path:
    directory = directory or backup_directory()
    return directory / f"{BACKUP_PREFIX}{uuid.uuid4().hex}{_project_suffix(app)}"


def _backup_files(directory: Path | None = None) -> list[Path]:
    directory = directory or backup_directory()
    if not directory.is_dir():
        return []
    files: list[Path] = []
    try:
        candidates = directory.iterdir()
    except OSError:
        return []
    for path in candidates:
        if not path.is_file() or not path.name.startswith(BACKUP_PREFIX):
            continue
        try:
            path.stat()
        except OSError:
            continue
        files.append(path)
    return files


def prune_backups(max_backups: int, directory: Path | None = None) -> list[Path]:
    directory = directory or backup_directory()
    limit = max(MIN_BACKUPS, min(MAX_BACKUPS, int(max_backups)))
    files = sorted(
        _backup_files(directory),
        key=lambda path: (path.stat().st_mtime_ns, path.name),
    )
    removed: list[Path] = []
    for path in files[:-limit]:
        try:
            path.unlink()
            removed.append(path)
        except OSError:
            _LOGGER.exception("Oude automatische back-up kon niet worden verwijderd: %s", path)
    return removed


def _capture_app_state(app: Any) -> tuple[dict[str, Any], str | None]:
    state: dict[str, Any] = {}
    for name in _STATE_ATTRS:
        if hasattr(app, name):
            try:
                state[name] = getattr(app, name)
            except Exception:
                pass
    title: str | None = None
    title_method = getattr(app, "title", None)
    if callable(title_method):
        try:
            title = str(title_method())
        except Exception:
            pass
    return state, title


def _restore_app_state(
    app: Any, state: dict[str, Any], title: str | None
) -> None:
    for name, value in state.items():
        try:
            setattr(app, name, value)
        except Exception:
            pass
    if title is not None:
        title_method = getattr(app, "title", None)
        if callable(title_method):
            try:
                title_method(title)
            except Exception:
                pass


def create_backup(
    app: Any,
    *,
    directory: Path | None = None,
    max_backups: int | None = None,
) -> Path:
    """Serialize the current in-memory project to an atomic rotating backup."""
    serializer = getattr(app, "_save_project_to_path", None)
    if not callable(serializer):
        raise RuntimeError("SleufBase projectserializer ontbreekt")

    directory = directory or backup_directory()
    directory.mkdir(parents=True, exist_ok=True)
    final_path = _new_backup_path(app, directory)
    temp_path = final_path.with_name(
        f".{final_path.name}.{os.getpid()}.{uuid.uuid4().hex}.tmp{final_path.suffix}"
    )
    state, title = _capture_app_state(app)
    setattr(app, "_sleufbase_autosave_in_progress", True)
    try:
        serializer(temp_path)
        if not temp_path.is_file():
            raise RuntimeError(
                f"Projectserializer heeft geen back-upbestand gemaakt: {temp_path}"
            )
        os.replace(temp_path, final_path)
    except Exception:
        try:
            temp_path.unlink(missing_ok=True)
        except OSError:
            pass
        raise
    finally:
        _restore_app_state(app, state, title)
        setattr(app, "_sleufbase_autosave_in_progress", False)

    limit = (
        load_autosave_settings().max_backups
        if max_backups is None
        else int(max_backups)
    )
    prune_backups(limit, directory)
    return final_path


def open_backup_directory(directory: Path | None = None) -> Path:
    directory = directory or backup_directory()
    directory.mkdir(parents=True, exist_ok=True)
    if os.name == "nt":
        os.startfile(str(directory))  # type: ignore[attr-defined]
    elif sys.platform == "darwin":
        subprocess.Popen(["open", str(directory)])
    else:
        subprocess.Popen(["xdg-open", str(directory)])
    return directory


class AutosaveManager:
    def __init__(self, app: Any) -> None:
        self.app = app
        self.settings = load_autosave_settings()
        self._after_id: Any = None
        self._closed = False

    def start(self) -> None:
        self._install_settings_menu()
        self.reschedule()

    def close(self) -> None:
        self._closed = True
        self._cancel_scheduled()

    def _cancel_scheduled(self) -> None:
        if self._after_id is None:
            return
        cancel = getattr(self.app, "after_cancel", None)
        if callable(cancel):
            try:
                cancel(self._after_id)
            except Exception:
                pass
        self._after_id = None

    def reschedule(self) -> None:
        self._cancel_scheduled()
        if self._closed or not self.settings.enabled:
            return
        after = getattr(self.app, "after", None)
        if not callable(after):
            return
        delay_ms = int(self.settings.interval_minutes * 60 * 1000)
        self._after_id = after(delay_ms, self._run_scheduled_backup)

    def _run_scheduled_backup(self) -> None:
        self._after_id = None
        try:
            if self.settings.enabled and not bool(
                getattr(self.app, "_sleufbase_autosave_in_progress", False)
            ):
                create_backup(
                    self.app,
                    max_backups=self.settings.max_backups,
                )
        except Exception:
            _LOGGER.exception("Automatische SleufBase-back-up is mislukt")
        finally:
            self.reschedule()

    def apply_settings(self, settings: AutosaveSettings) -> None:
        self.settings = save_autosave_settings(settings)
        prune_backups(self.settings.max_backups)
        self.reschedule()

    def show_settings(self) -> None:
        from tkinter import BooleanVar, IntVar, StringVar, Toplevel, messagebox, ttk

        window = Toplevel(self.app)
        window.title("Automatische back-ups")
        window.resizable(False, False)
        try:
            window.transient(self.app)
            window.grab_set()
        except Exception:
            pass

        enabled_var = BooleanVar(window, value=self.settings.enabled)
        interval_var = IntVar(window, value=self.settings.interval_minutes)
        max_var = IntVar(window, value=self.settings.max_backups)
        folder_var = StringVar(window, value=str(backup_directory()))

        frame = ttk.Frame(window, padding=14)
        frame.grid(row=0, column=0, sticky="nsew")
        ttk.Checkbutton(
            frame,
            text="Automatische back-ups inschakelen",
            variable=enabled_var,
        ).grid(row=0, column=0, columnspan=3, sticky="w", pady=(0, 10))

        ttk.Label(frame, text="Interval (minuten):").grid(
            row=1, column=0, sticky="w", pady=4
        )
        ttk.Spinbox(
            frame,
            from_=MIN_INTERVAL_MINUTES,
            to=MAX_INTERVAL_MINUTES,
            textvariable=interval_var,
            width=8,
        ).grid(row=1, column=1, sticky="w", pady=4)

        ttk.Label(frame, text="Maximaal aantal back-ups:").grid(
            row=2, column=0, sticky="w", pady=4
        )
        ttk.Spinbox(
            frame,
            from_=MIN_BACKUPS,
            to=MAX_BACKUPS,
            textvariable=max_var,
            width=8,
        ).grid(row=2, column=1, sticky="w", pady=4)

        ttk.Label(frame, text="Back-upmap:").grid(
            row=3, column=0, sticky="nw", pady=(10, 4)
        )
        ttk.Entry(
            frame,
            textvariable=folder_var,
            width=52,
            state="readonly",
        ).grid(row=3, column=1, columnspan=2, sticky="ew", pady=(10, 4))
        ttk.Button(
            frame,
            text="Back-upmap openen",
            command=lambda: open_backup_directory(),
        ).grid(row=4, column=1, sticky="w", pady=(2, 12))

        ttk.Label(
            frame,
            text=(
                "Bestandsnamen zijn willekeurig. Wanneer het maximum wordt "
                "overschreden, wordt automatisch de oudste back-up verwijderd."
            ),
            wraplength=440,
        ).grid(row=5, column=0, columnspan=3, sticky="w", pady=(0, 12))

        buttons = ttk.Frame(frame)
        buttons.grid(row=6, column=0, columnspan=3, sticky="e")

        def save_and_close() -> None:
            try:
                new_settings = AutosaveSettings(
                    enabled=enabled_var.get(),
                    interval_minutes=int(interval_var.get()),
                    max_backups=int(max_var.get()),
                ).normalized()
                self.apply_settings(new_settings)
            except Exception as exc:
                messagebox.showerror(
                    "Automatische back-ups",
                    f"Instellingen konden niet worden opgeslagen:\n{exc}",
                    parent=window,
                )
                return
            window.destroy()

        ttk.Button(buttons, text="Annuleren", command=window.destroy).grid(
            row=0, column=0, padx=(0, 8)
        )
        ttk.Button(buttons, text="Opslaan", command=save_and_close).grid(
            row=0, column=1
        )

        try:
            window.protocol("WM_DELETE_WINDOW", window.destroy)
            window.focus_set()
        except Exception:
            pass

    def _install_settings_menu(self) -> None:
        try:
            import tkinter as tk
        except Exception:
            return

        try:
            menu_name = self.app.cget("menu")
        except Exception:
            try:
                menu_name = self.app["menu"]
            except Exception:
                return
        if not menu_name:
            return
        try:
            root_menu = self.app.nametowidget(menu_name)
        except Exception:
            return
        if not isinstance(root_menu, tk.Menu):
            return

        if self._append_to_settings_cascade(root_menu):
            return

        settings_menu = tk.Menu(root_menu, tearoff=False)
        settings_menu.add_command(
            label="Automatische back-ups…",
            command=self.show_settings,
        )
        root_menu.add_cascade(label="Instellingen", menu=settings_menu)

    def _append_to_settings_cascade(self, menu: Any) -> bool:
        end = menu.index("end")
        if end is None:
            return False
        for index in range(int(end) + 1):
            try:
                label = str(menu.entrycget(index, "label") or "").strip().casefold()
                entry_type = menu.type(index)
            except Exception:
                continue
            if label not in {"instellingen", "settings"}:
                continue
            if entry_type != "cascade":
                continue
            try:
                submenu_name = menu.entrycget(index, "menu")
                submenu = menu.nametowidget(submenu_name)
                last = submenu.index("end")
                if last is not None:
                    for sub_index in range(int(last) + 1):
                        try:
                            existing = str(
                                submenu.entrycget(sub_index, "label") or ""
                            ).casefold()
                        except Exception:
                            continue
                        if "automatische back-up" in existing:
                            return True
                submenu.add_separator()
                submenu.add_command(
                    label="Automatische back-ups…",
                    command=self.show_settings,
                )
                return True
            except Exception:
                return False
        return False


def _patch_viewer_class(viewer_class: type[Any]) -> None:
    if int(
        viewer_class.__dict__.get("_sleufbase_autosave_patch_version", 0) or 0
    ) >= PATCH_VERSION:
        return

    original_init = viewer_class.__init__

    def _init_with_autosave(self: Any, *args: Any, **kwargs: Any) -> None:
        original_init(self, *args, **kwargs)
        manager = AutosaveManager(self)
        self._sleufbase_autosave_manager = manager
        manager.start()

    viewer_class.__init__ = _init_with_autosave

    original_destroy = getattr(viewer_class, "destroy", None)
    if callable(original_destroy):
        def _destroy_with_autosave(self: Any, *args: Any, **kwargs: Any) -> Any:
            manager = getattr(self, "_sleufbase_autosave_manager", None)
            if isinstance(manager, AutosaveManager):
                manager.close()
            return original_destroy(self, *args, **kwargs)

        viewer_class.destroy = _destroy_with_autosave

    viewer_class._sleufbase_autosave_patch_version = PATCH_VERSION


def install_autosave_backup_patch() -> None:
    from .app import KlicViewerApp

    _patch_viewer_class(KlicViewerApp)
