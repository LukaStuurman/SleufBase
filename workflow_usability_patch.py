from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime
import tkinter as tk
from tkinter import messagebox, ttk
from typing import Any, Callable, Iterable


PATCH_VERSION = 1
MAX_ACTIVITY_ITEMS = 120
MAX_PALETTE_RESULTS = 40


@dataclass(frozen=True)
class CommandAction:
    path: str
    label: str
    command: Callable[[], Any]
    shortcut: str = ""


@dataclass(frozen=True)
class ShortcutSpec:
    keys: str
    description: str
    method_name: str | None = None


SHORTCUT_SPECS = (
    ShortcutSpec("Ctrl+K", "Actie zoeken"),
    ShortcutSpec("Ctrl+Shift+P", "Actie zoeken"),
    ShortcutSpec("Ctrl+,", "Instellingen openen", "open_settings_dialog"),
    ShortcutSpec("Ctrl+Shift+E", "DXF-sjabloon exporteren", "export_cadastral_template_dxf"),
    ShortcutSpec("Ctrl+Shift+M", "marXact-DXF importeren", "import_marxact_dxf"),
    ShortcutSpec("Ctrl+Shift+J", "KickTheMap Jobs openen", "open_kickthemap_jobs_browser_window"),
    ShortcutSpec("Ctrl+Shift+L", "Activiteiten bekijken"),
    ShortcutSpec("F1", "Sneltoetsen tonen"),
)


def _normalize_text(value: object) -> str:
    text = str(value or "").strip().casefold()
    text = text.replace("…", "...")
    text = " ".join(text.split())
    return text


def score_action(query: str, action: CommandAction) -> int:
    """Return a stable search score for the command palette."""

    normalized_query = _normalize_text(query)
    if not normalized_query:
        return 1

    label = _normalize_text(action.label)
    path = _normalize_text(action.path)
    if label == normalized_query:
        return 1000
    if label.startswith(normalized_query):
        return 900 - min(100, len(label) - len(normalized_query))
    if normalized_query in label:
        return 800 - min(100, label.index(normalized_query))
    if path.startswith(normalized_query):
        return 720
    if normalized_query in path:
        return 680 - min(120, path.index(normalized_query))

    tokens = [token for token in normalized_query.replace(">", " ").split() if token]
    if tokens and all(token in path for token in tokens):
        label_hits = sum(1 for token in tokens if token in label)
        return 520 + (label_hits * 25)

    # Lightweight subsequence matching catches queries such as "ktm jobs" while
    # still ranking normal substring/token matches higher.
    cursor = 0
    matched = 0
    for char in normalized_query.replace(" ", ""):
        found = path.find(char, cursor)
        if found < 0:
            break
        matched += 1
        cursor = found + 1
    compact_length = max(1, len(normalized_query.replace(" ", "")))
    if matched == compact_length:
        return 250
    return 0


def collect_actions_from_specs(specs: Iterable[dict[str, Any]]) -> list[CommandAction]:
    """Flatten SleufBase's modern menu specs into searchable actions."""

    actions: list[CommandAction] = []
    seen_paths: set[str] = set()
    for spec in specs:
        menu_label = str(spec.get("label") or "").strip()
        top_command = spec.get("command")
        if menu_label and callable(top_command):
            path = menu_label
            if path not in seen_paths:
                seen_paths.add(path)
                actions.append(CommandAction(path=path, label=menu_label, command=top_command))

        for item in spec.get("items") or ():
            if item.get("type") != "command":
                continue
            if str(item.get("state") or tk.NORMAL) == tk.DISABLED:
                continue
            label = str(item.get("label") or "").strip()
            command = item.get("command")
            if not label or not callable(command):
                continue
            if _normalize_text(label) in {"actie zoeken...", "actie zoeken"}:
                continue
            path = f"{menu_label} › {label}" if menu_label else label
            if path in seen_paths:
                continue
            seen_paths.add(path)
            actions.append(CommandAction(path=path, label=label, command=command))
    return actions


def append_activity(history: list[tuple[str, str]], message: object) -> bool:
    text = str(message or "").strip()
    if not text:
        return False
    if history and history[-1][1] == text:
        return False
    history.append((datetime.now().strftime("%H:%M:%S"), text))
    if len(history) > MAX_ACTIVITY_ITEMS:
        del history[: len(history) - MAX_ACTIVITY_ITEMS]
    return True


def available_shortcuts(app: Any) -> list[ShortcutSpec]:
    result: list[ShortcutSpec] = []
    for spec in SHORTCUT_SPECS:
        if spec.method_name is None or callable(getattr(app, spec.method_name, None)):
            result.append(spec)
    return result


def _refresh_modern_specs(app: Any, menu_bar: Any) -> None:
    capture = getattr(app, "_capture_modern_menu_specs", None)
    if not callable(capture):
        return
    try:
        app._modern_menu_specs = capture(menu_bar)
    except Exception:
        return


def install_workflow_usability_patch(viewer_class: type) -> None:
    """Install discoverability and workflow feedback improvements on KlicViewerApp."""

    if int(getattr(viewer_class, "_sleufbase_workflow_usability_version", 0) or 0) >= PATCH_VERSION:
        return

    original_init = viewer_class.__init__
    original_build_menu = viewer_class._build_menu
    original_set_status = viewer_class.set_status

    def _activity_history(self) -> list[tuple[str, str]]:
        history = getattr(self, "_sleufbase_activity_history", None)
        if not isinstance(history, list):
            history = []
            self._sleufbase_activity_history = history
        return history

    def _refresh_activity_window(self) -> None:
        text_widget = getattr(self, "_sleufbase_activity_text", None)
        if text_widget is None:
            return
        try:
            if not text_widget.winfo_exists():
                return
            lines = [f"{timestamp}  {message}" for timestamp, message in self._activity_history()]
            text_widget.configure(state="normal")
            text_widget.delete("1.0", "end")
            text_widget.insert("1.0", "\n".join(lines) if lines else "Nog geen activiteiten in deze sessie.")
            text_widget.configure(state="disabled")
            text_widget.see("end")
        except tk.TclError:
            return

    def _record_activity(self, message: object) -> None:
        if not append_activity(self._activity_history(), message):
            return
        try:
            self.after_idle(self._refresh_activity_window)
        except Exception:
            pass

    def _set_status_with_history(self, *args, **kwargs):
        result = original_set_status(self, *args, **kwargs)
        message = args[0] if args else kwargs.get("message", kwargs.get("text", ""))
        self._record_activity(message)
        return result

    def _copy_activity_history(self) -> None:
        lines = [f"{timestamp}  {message}" for timestamp, message in self._activity_history()]
        try:
            self.clipboard_clear()
            self.clipboard_append("\n".join(lines))
            self.update_idletasks()
            self.set_status("Activiteiten gekopieerd naar het klembord.")
        except Exception as exc:
            messagebox.showerror("Activiteiten kopiëren", str(exc), parent=self)

    def _clear_activity_history(self) -> None:
        self._activity_history().clear()
        self._refresh_activity_window()

    def show_activity_history(self) -> None:
        existing = getattr(self, "_sleufbase_activity_window", None)
        try:
            if existing is not None and existing.winfo_exists():
                existing.deiconify()
                existing.lift()
                existing.focus_force()
                self._refresh_activity_window()
                return
        except tk.TclError:
            pass

        window = tk.Toplevel(self)
        window.title("Activiteiten")
        window.transient(self)
        window.geometry("760x420")
        window.minsize(560, 300)
        self._sleufbase_activity_window = window

        frame = ttk.Frame(window, padding=14)
        frame.pack(fill="both", expand=True)
        ttk.Label(
            frame,
            text="Activiteiten deze sessie",
            font=("Segoe UI", 11, "bold"),
        ).pack(anchor="w")
        ttk.Label(
            frame,
            text="Statusmeldingen blijven hier beschikbaar, ook nadat de statusbalk alweer is gewijzigd.",
        ).pack(anchor="w", pady=(2, 10))

        text = tk.Text(
            frame,
            wrap="word",
            height=16,
            borderwidth=0,
            padx=10,
            pady=10,
            background=getattr(self, "INPUT_BG", "#fbfcfe"),
            foreground=getattr(self, "TEXT", "#111827"),
            selectbackground=getattr(self, "ACCENT", "#f97316"),
            selectforeground="#ffffff",
        )
        text.pack(fill="both", expand=True)
        text.configure(state="disabled")
        self._sleufbase_activity_text = text

        footer = ttk.Frame(frame)
        footer.pack(fill="x", pady=(10, 0))
        ttk.Button(footer, text="Kopiëren", command=self._copy_activity_history).pack(side="left")
        ttk.Button(footer, text="Wissen", command=self._clear_activity_history).pack(side="left", padx=(8, 0))
        ttk.Button(footer, text="Sluiten", command=window.destroy).pack(side="right")
        window.bind("<Escape>", lambda _event: window.destroy())
        window.protocol("WM_DELETE_WINDOW", window.destroy)
        self._refresh_activity_window()

    def show_shortcuts(self) -> None:
        existing = getattr(self, "_sleufbase_shortcuts_window", None)
        try:
            if existing is not None and existing.winfo_exists():
                existing.deiconify()
                existing.lift()
                existing.focus_force()
                return
        except tk.TclError:
            pass

        window = tk.Toplevel(self)
        window.title("Sneltoetsen")
        window.transient(self)
        window.geometry("560x390")
        window.minsize(480, 320)
        self._sleufbase_shortcuts_window = window

        frame = ttk.Frame(window, padding=16)
        frame.pack(fill="both", expand=True)
        ttk.Label(frame, text="SleufBase sneltoetsen", font=("Segoe UI", 11, "bold")).pack(anchor="w")
        ttk.Label(
            frame,
            text="Gebruik Ctrl+K als je niet meer weet in welk menu een functie staat.",
        ).pack(anchor="w", pady=(2, 12))

        tree = ttk.Treeview(frame, columns=("keys", "action"), show="headings", height=10)
        tree.heading("keys", text="Sneltoets")
        tree.heading("action", text="Actie")
        tree.column("keys", width=150, stretch=False)
        tree.column("action", width=350, stretch=True)
        for spec in available_shortcuts(self):
            tree.insert("", "end", values=(spec.keys, spec.description))
        tree.pack(fill="both", expand=True)
        ttk.Button(frame, text="Sluiten", command=window.destroy).pack(anchor="e", pady=(12, 0))
        window.bind("<Escape>", lambda _event: window.destroy())

    def _command_actions(self) -> list[CommandAction]:
        specs = getattr(self, "_modern_menu_specs", ())
        actions = collect_actions_from_specs(specs)
        self._sleufbase_command_actions = actions
        return actions

    def _ranked_command_actions(self, query: str) -> list[CommandAction]:
        actions = self._command_actions()
        recent = list(getattr(self, "_sleufbase_recent_action_paths", ()))
        recent_index = {path: index for index, path in enumerate(recent)}
        normalized_query = _normalize_text(query)
        ranked: list[tuple[int, int, str, CommandAction]] = []
        for action in actions:
            score = score_action(query, action)
            if normalized_query and score <= 0:
                continue
            recency_bonus = max(0, 120 - (recent_index.get(action.path, 99) * 15)) if action.path in recent_index else 0
            ranked.append((score + recency_bonus, recency_bonus, action.path.casefold(), action))
        ranked.sort(key=lambda item: (-item[0], -item[1], item[2]))
        return [item[3] for item in ranked[:MAX_PALETTE_RESULTS]]

    def _remember_command_action(self, action: CommandAction) -> None:
        recent = list(getattr(self, "_sleufbase_recent_action_paths", ()))
        recent = [path for path in recent if path != action.path]
        recent.insert(0, action.path)
        self._sleufbase_recent_action_paths = recent[:10]

    def _execute_command_action(self, action: CommandAction) -> None:
        self._remember_command_action(action)
        try:
            action.command()
        except Exception as exc:
            messagebox.showerror(
                "Actie uitvoeren mislukt",
                f"{action.path}\n\n{exc}",
                parent=self,
            )
            try:
                self.set_status(f"Actie mislukt: {action.label}")
            except Exception:
                pass

    def show_command_palette(self) -> None:
        existing = getattr(self, "_sleufbase_command_palette", None)
        try:
            if existing is not None and existing.winfo_exists():
                existing.deiconify()
                existing.lift()
                entry = getattr(self, "_sleufbase_command_palette_entry", None)
                if entry is not None:
                    entry.focus_set()
                    entry.selection_range(0, "end")
                return
        except tk.TclError:
            pass

        window = tk.Toplevel(self)
        window.title("Actie zoeken")
        window.transient(self)
        window.geometry("680x460")
        window.minsize(540, 340)
        self._sleufbase_command_palette = window

        frame = ttk.Frame(window, padding=14)
        frame.pack(fill="both", expand=True)
        ttk.Label(frame, text="Actie zoeken", font=("Segoe UI", 12, "bold")).pack(anchor="w")
        ttk.Label(
            frame,
            text="Zoek door alle SleufBase-menu's. Typ bijvoorbeeld ‘export’, ‘marXact’, ‘StreetSmart’ of ‘beginpunt’.",
        ).pack(anchor="w", pady=(2, 10))

        query_var = tk.StringVar()
        entry = ttk.Entry(frame, textvariable=query_var)
        entry.pack(fill="x")
        self._sleufbase_command_palette_entry = entry

        result_list = tk.Listbox(
            frame,
            activestyle="none",
            borderwidth=0,
            highlightthickness=0,
            background=getattr(self, "INPUT_BG", "#fbfcfe"),
            foreground=getattr(self, "TEXT", "#111827"),
            selectbackground=getattr(self, "ACCENT", "#f97316"),
            selectforeground="#ffffff",
            font=("Segoe UI", 10),
        )
        result_list.pack(fill="both", expand=True, pady=(10, 0))
        self._sleufbase_palette_results: list[CommandAction] = []

        hint = ttk.Label(frame, text="Enter uitvoeren  •  Esc sluiten  •  ↑/↓ selecteren")
        hint.pack(anchor="w", pady=(8, 0))

        def refresh(*_args) -> None:
            actions = self._ranked_command_actions(query_var.get())
            self._sleufbase_palette_results = actions
            result_list.delete(0, "end")
            for action in actions:
                result_list.insert("end", action.path)
            if actions:
                result_list.selection_set(0)
                result_list.activate(0)

        def run_selected(_event=None):
            selection = result_list.curselection()
            if not selection:
                return "break"
            index = int(selection[0])
            actions = getattr(self, "_sleufbase_palette_results", [])
            if index >= len(actions):
                return "break"
            action = actions[index]
            window.destroy()
            self._execute_command_action(action)
            return "break"

        def move_selection(delta: int):
            actions = getattr(self, "_sleufbase_palette_results", [])
            if not actions:
                return "break"
            selection = result_list.curselection()
            current = int(selection[0]) if selection else 0
            target = max(0, min(len(actions) - 1, current + delta))
            result_list.selection_clear(0, "end")
            result_list.selection_set(target)
            result_list.activate(target)
            result_list.see(target)
            return "break"

        query_var.trace_add("write", refresh)
        entry.bind("<Return>", run_selected)
        entry.bind("<Down>", lambda _event: move_selection(1))
        entry.bind("<Up>", lambda _event: move_selection(-1))
        result_list.bind("<Return>", run_selected)
        result_list.bind("<Double-Button-1>", run_selected)
        window.bind("<Escape>", lambda _event: window.destroy())
        window.protocol("WM_DELETE_WINDOW", window.destroy)
        refresh()
        entry.focus_set()

    def _invoke_named_method(self, method_name: str, label: str):
        method = getattr(self, method_name, None)
        if not callable(method):
            try:
                self.set_status(f"{label} is in deze configuratie niet beschikbaar.")
            except Exception:
                pass
            return "break"
        try:
            method()
        except Exception as exc:
            messagebox.showerror(f"{label} mislukt", str(exc), parent=self)
        return "break"

    def _install_workflow_shortcuts(self) -> None:
        bindings = {
            "<Control-k>": lambda _event: (self.show_command_palette(), "break")[1],
            "<Control-K>": lambda _event: (self.show_command_palette(), "break")[1],
            "<Control-Shift-P>": lambda _event: (self.show_command_palette(), "break")[1],
            "<Control-comma>": lambda _event: self._invoke_named_method("open_settings_dialog", "Instellingen openen"),
            "<Control-Shift-E>": lambda _event: self._invoke_named_method("export_cadastral_template_dxf", "DXF-sjabloon exporteren"),
            "<Control-Shift-M>": lambda _event: self._invoke_named_method("import_marxact_dxf", "marXact import"),
            "<Control-Shift-J>": lambda _event: self._invoke_named_method("open_kickthemap_jobs_browser_window", "KickTheMap Jobs"),
            "<Control-Shift-L>": lambda _event: (self.show_activity_history(), "break")[1],
            "<F1>": lambda _event: (self.show_shortcuts(), "break")[1],
        }
        for pattern, callback in bindings.items():
            try:
                self.bind_all(pattern, callback, add="+")
            except Exception:
                continue

    def _add_quick_menu(self) -> None:
        try:
            menu_name = self.cget("menu")
            menu_bar = self.nametowidget(menu_name)
            end_index = menu_bar.index("end")
            if end_index is not None:
                for index in range(end_index, -1, -1):
                    if menu_bar.type(index) == "cascade" and str(menu_bar.entrycget(index, "label")) == "Snel":
                        menu_bar.delete(index)
            quick_menu = tk.Menu(menu_bar, tearoff=0)
            quick_menu.add_command(label="Actie zoeken…", accelerator="Ctrl+K", command=self.show_command_palette)
            quick_menu.add_command(label="Activiteiten", accelerator="Ctrl+Shift+L", command=self.show_activity_history)
            quick_menu.add_separator()
            quick_menu.add_command(label="Sneltoetsen", accelerator="F1", command=self.show_shortcuts)
            menu_bar.add_cascade(label="Snel", menu=quick_menu)
            _refresh_modern_specs(self, menu_bar)
            self._command_actions()
        except Exception:
            return

    def _build_menu_with_workflow_usability(self) -> None:
        original_build_menu(self)
        self._add_quick_menu()

    def _init_with_workflow_usability(self, *args, **kwargs):
        self._sleufbase_activity_history = []
        self._sleufbase_recent_action_paths = []
        original_init(self, *args, **kwargs)
        self._install_workflow_shortcuts()
        try:
            self.after_idle(self._command_actions)
        except Exception:
            pass

    viewer_class.__init__ = _init_with_workflow_usability
    viewer_class._build_menu = _build_menu_with_workflow_usability
    viewer_class.set_status = _set_status_with_history
    viewer_class._activity_history = _activity_history
    viewer_class._record_activity = _record_activity
    viewer_class._refresh_activity_window = _refresh_activity_window
    viewer_class._copy_activity_history = _copy_activity_history
    viewer_class._clear_activity_history = _clear_activity_history
    viewer_class.show_activity_history = show_activity_history
    viewer_class.show_shortcuts = show_shortcuts
    viewer_class._command_actions = _command_actions
    viewer_class._ranked_command_actions = _ranked_command_actions
    viewer_class._remember_command_action = _remember_command_action
    viewer_class._execute_command_action = _execute_command_action
    viewer_class.show_command_palette = show_command_palette
    viewer_class._invoke_named_method = _invoke_named_method
    viewer_class._install_workflow_shortcuts = _install_workflow_shortcuts
    viewer_class._add_quick_menu = _add_quick_menu
    viewer_class._sleufbase_workflow_usability_version = PATCH_VERSION
