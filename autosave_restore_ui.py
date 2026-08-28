from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime
import inspect
from pathlib import Path
from typing import Any

from .autosave_backup_patch import BACKUP_PREFIX, backup_directory, open_backup_directory


@dataclass(frozen=True)
class AutosaveFileInfo:
    path: Path
    modified_timestamp: float
    size_bytes: int

    @property
    def modified_text(self) -> str:
        return datetime.fromtimestamp(self.modified_timestamp).strftime("%d-%m-%Y %H:%M:%S")

    @property
    def size_text(self) -> str:
        size = float(self.size_bytes)
        if size < 1024.0:
            return f"{int(size)} B"
        if size < 1024.0 * 1024.0:
            return f"{size / 1024.0:.1f} KB"
        return f"{size / (1024.0 * 1024.0):.1f} MB"


def list_autosaves(directory: Path | None = None) -> list[AutosaveFileInfo]:
    directory = directory or backup_directory()
    if not directory.is_dir():
        return []

    entries: list[AutosaveFileInfo] = []
    try:
        candidates = list(directory.iterdir())
    except OSError:
        return []

    for path in candidates:
        if not path.is_file() or not path.name.startswith(BACKUP_PREFIX):
            continue
        if ".tmp" in path.name:
            continue
        try:
            stat = path.stat()
        except OSError:
            continue
        entries.append(
            AutosaveFileInfo(
                path=path,
                modified_timestamp=float(stat.st_mtime),
                size_bytes=int(stat.st_size),
            )
        )
    entries.sort(key=lambda item: (item.modified_timestamp, item.path.name), reverse=True)
    return entries


def _method_accepts_path(method: Any, path: Path) -> bool:
    try:
        inspect.signature(method).bind(path)
    except (TypeError, ValueError):
        return False
    return True


def load_autosave(app: Any, path: str | Path) -> Any:
    """Load one autosave using the same project loaders as normal project opening."""

    autosave_path = Path(path)
    if not autosave_path.is_file():
        raise FileNotFoundError(f"Autosave niet gevonden: {autosave_path}")

    for method_name in (
        "_load_project_from_path",
        "load_project_from_path",
        "_load_project_path",
    ):
        method = getattr(app, method_name, None)
        if not callable(method) or not _method_accepts_path(method, autosave_path):
            continue
        result = method(autosave_path)
        setattr(app, "_sleufbase_last_project_path", autosave_path)
        return result

    load_project = getattr(app, "load_project", None)
    if callable(load_project):
        from tkinter import filedialog

        original_askopenfilename = filedialog.askopenfilename
        original_askopenfilenames = getattr(filedialog, "askopenfilenames", None)

        def choose_one(*_args: Any, **_kwargs: Any) -> str:
            return str(autosave_path)

        def choose_many(*_args: Any, **_kwargs: Any) -> tuple[str, ...]:
            return (str(autosave_path),)

        filedialog.askopenfilename = choose_one
        if callable(original_askopenfilenames):
            filedialog.askopenfilenames = choose_many
        try:
            result = load_project()
        finally:
            filedialog.askopenfilename = original_askopenfilename
            if callable(original_askopenfilenames):
                filedialog.askopenfilenames = original_askopenfilenames
        setattr(app, "_sleufbase_last_project_path", autosave_path)
        return result

    raise RuntimeError("SleufBase heeft geen ondersteunde projectlader voor autosaves.")


def show_autosave_selector(app: Any, *, parent: Any | None = None) -> None:
    from tkinter import messagebox, ttk
    import tkinter as tk

    owner = parent or app
    window = tk.Toplevel(owner)
    window.title("Autosave laden")
    window.geometry("760x430")
    window.minsize(620, 340)
    window.resizable(True, True)
    try:
        window.transient(owner)
        window.grab_set()
    except tk.TclError:
        pass

    frame = ttk.Frame(window, padding=14)
    frame.grid(row=0, column=0, sticky="nsew")
    window.rowconfigure(0, weight=1)
    window.columnconfigure(0, weight=1)
    frame.rowconfigure(1, weight=1)
    frame.columnconfigure(0, weight=1)

    ttk.Label(
        frame,
        text="Kies een autosave om als project te laden.",
    ).grid(row=0, column=0, sticky="w", pady=(0, 8))

    tree = ttk.Treeview(
        frame,
        columns=("date", "file", "size"),
        show="headings",
        selectmode="browse",
        height=12,
    )
    tree.heading("date", text="Datum / tijd")
    tree.heading("file", text="Bestand")
    tree.heading("size", text="Grootte")
    tree.column("date", width=155, stretch=False)
    tree.column("file", width=410, stretch=True)
    tree.column("size", width=90, stretch=False, anchor="e")
    tree.grid(row=1, column=0, sticky="nsew")

    scrollbar = ttk.Scrollbar(frame, orient="vertical", command=tree.yview)
    scrollbar.grid(row=1, column=1, sticky="ns")
    tree.configure(yscrollcommand=scrollbar.set)

    entries = list_autosaves()
    by_iid: dict[str, AutosaveFileInfo] = {}
    for index, entry in enumerate(entries):
        iid = f"autosave-{index}"
        by_iid[iid] = entry
        tree.insert(
            "",
            "end",
            iid=iid,
            values=(entry.modified_text, entry.path.name, entry.size_text),
        )
    if entries:
        tree.selection_set("autosave-0")
        tree.focus("autosave-0")

    status_text = (
        f"{len(entries)} autosave(s) gevonden. Nieuwste staat bovenaan."
        if entries
        else "Er zijn nog geen autosaves beschikbaar."
    )
    ttk.Label(frame, text=status_text).grid(row=2, column=0, sticky="w", pady=(8, 0))

    buttons = ttk.Frame(frame)
    buttons.grid(row=3, column=0, columnspan=2, sticky="ew", pady=(12, 0))
    buttons.columnconfigure(0, weight=1)

    def selected_entry() -> AutosaveFileInfo | None:
        selection = tree.selection()
        return by_iid.get(selection[0]) if selection else None

    def load_selected() -> None:
        entry = selected_entry()
        if entry is None:
            messagebox.showinfo("Autosave laden", "Selecteer eerst een autosave.", parent=window)
            return
        if not messagebox.askyesno(
            "Autosave laden",
            (
                f"Autosave van {entry.modified_text} laden?\n\n"
                "Het huidige project in SleufBase wordt vervangen."
            ),
            parent=window,
        ):
            return

        try:
            window.grab_release()
        except tk.TclError:
            pass
        try:
            window.destroy()
        except tk.TclError:
            pass
        if parent is not None and parent is not app:
            try:
                parent.destroy()
            except Exception:
                pass

        try:
            load_autosave(app, entry.path)
        except Exception as exc:
            messagebox.showerror(
                "Autosave laden mislukt",
                str(exc),
                parent=app,
            )
            return

        set_status = getattr(app, "set_status", None)
        if callable(set_status):
            try:
                set_status(f"Autosave geladen: {entry.modified_text}")
            except Exception:
                pass
        messagebox.showinfo(
            "Autosave geladen",
            "De autosave is geladen. Gebruik eventueel Project > Opslaan als om hem onder een vaste projectnaam te bewaren.",
            parent=app,
        )

    ttk.Button(
        buttons,
        text="Back-upmap openen",
        command=lambda: open_backup_directory(),
    ).grid(row=0, column=0, sticky="w")
    ttk.Button(
        buttons,
        text="Sluiten",
        command=window.destroy,
    ).grid(row=0, column=1, padx=(8, 0))
    load_button = ttk.Button(
        buttons,
        text="Autosave laden",
        command=load_selected,
    )
    load_button.grid(row=0, column=2, padx=(8, 0))
    if not entries:
        load_button.state(["disabled"])

    tree.bind("<Double-Button-1>", lambda _event: load_selected())
    tree.bind("<Return>", lambda _event: load_selected())
