from __future__ import annotations

import marshal
import re
import subprocess
import sys
import threading
import time
import tkinter as tk
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path
from tkinter import font as tkfont
from tkinter import messagebox, ttk
from urllib.parse import quote

from .ai_maaiveld import (
    AiMaaiveldError,
    AiMaaiveldPoint,
    build_numbered_maaiveld_image,
    number_key_to_segment_key,
    request_maaiveld_from_vllm,
)
from .rounded_widgets import _ORIGINAL_TK_ENTRY, install_rounded_buttons
from .models import MapMarker
from .streetsmart import STREETSMART_WEB_URL, save_streetsmart_state


install_rounded_buttons()
_ORIGINAL_TK_TOPLEVEL = tk.Toplevel
_ORIGINAL_TTK_COMBOBOX = ttk.Combobox


def _load_cached_module() -> None:
    cache_tag = sys.implementation.cache_tag
    if not cache_tag:
        raise ImportError("Python cache tag is niet beschikbaar.")
    pyc_path = Path(__file__).with_name("_bytecode") / f"app.{cache_tag}.pyc"
    if not pyc_path.exists():
        raise ImportError(f"Bytecode voor app.app niet gevonden: {pyc_path}")
    code = marshal.loads(pyc_path.read_bytes()[16:])
    exec(code, globals())


def _install_kickthemap_jobs_browser_patch() -> None:
    viewer_class = globals().get("KlicViewerApp")
    if viewer_class is None or getattr(viewer_class, "_kickthemap_jobs_browser_patch", False):
        return

    original_build_menu = viewer_class._build_menu

    def _kickthemap_jobs_browser_launch_command(self):
        if getattr(sys, "frozen", False):
            return sys.executable, ["--kickthemap-jobs-browser"]
        main_script = Path(__file__).resolve().parent.parent / "main.py"
        return sys.executable, [str(main_script), "--kickthemap-jobs-browser"]

    def open_kickthemap_jobs_browser_window(self) -> None:
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
            self._register_child_process("kickthemap-jobs-browser", process)
        except Exception as exc:
            messagebox.showerror("KickTheMap Jobs openen mislukt", str(exc))
            self.set_status("KickTheMap Jobs openen mislukt.")
            return
        self.set_status("KickTheMap Jobs geopend.")

    def _build_menu_with_kickthemap_jobs_browser(self) -> None:
        original_build_menu(self)
        try:
            menu_name = self.cget("menu")
            menu_bar = self.nametowidget(menu_name)
            end_index = menu_bar.index("end")
            if end_index is None:
                return
            for index in range(end_index + 1):
                if menu_bar.type(index) != "cascade":
                    continue
                if menu_bar.entrycget(index, "label") != "KickTheMap":
                    continue
                submenu = self.nametowidget(menu_bar.entrycget(index, "menu"))
                submenu_end = submenu.index("end")
                for submenu_index in range(submenu_end if submenu_end is not None else -1, -1, -1):
                    if submenu.type(submenu_index) != "command":
                        continue
                    label = submenu.entrycget(submenu_index, "label")
                    if label in {"Browser", "Laad GeoTIFF uit job", "Laad GeoTIFF uit job...", "Alternatieve browser", "Jobs"}:
                        submenu.delete(submenu_index)
                insert_at = 0
                submenu.insert_command(
                    insert_at,
                    label="Jobs",
                    command=self.open_kickthemap_jobs_browser_window,
                )
                return
        except Exception:
            return

    viewer_class._kickthemap_jobs_browser_launch_command = _kickthemap_jobs_browser_launch_command
    viewer_class.open_kickthemap_jobs_browser_window = open_kickthemap_jobs_browser_window
    viewer_class._build_menu = _build_menu_with_kickthemap_jobs_browser
    viewer_class._kickthemap_jobs_browser_patch = True


def _install_sleufbase_branding_patch() -> None:
    viewer_class = globals().get("KlicViewerApp")
    if viewer_class is None or getattr(viewer_class, "_sleufbase_branding_patch", False):
        return

    original_init = viewer_class.__init__

    def _init_with_sleufbase_branding(self, *args, **kwargs):
        original_init(self, *args, **kwargs)
        try:
            self.title("SleufBase")
        except Exception:
            pass
        icon_path = Path(getattr(sys, "_MEIPASS", Path(__file__).resolve().parent.parent)) / "assets" / "sleufbase_icon.ico"
        if icon_path.exists():
            try:
                self.iconbitmap(str(icon_path))
            except Exception:
                pass

    viewer_class.__init__ = _init_with_sleufbase_branding
    viewer_class._sleufbase_branding_patch = True


def _install_modern_combobox_patch() -> None:
    viewer_class = globals().get("KlicViewerApp")
    if viewer_class is None or getattr(viewer_class, "_modern_combobox_patch", False):
        return

    original_create_rounded_combobox = viewer_class._create_rounded_combobox

    def _configure_modern_combobox_style(self) -> None:
        try:
            style = ttk.Style(self)
            style.configure(
                "RoundedField.TCombobox",
                background=self.INPUT_BG,
                fieldbackground=self.INPUT_BG,
                foreground=self.TEXT,
                selectbackground=self.ACCENT,
                selectforeground="#ffffff",
                bordercolor=self.INPUT_BG,
                darkcolor=self.INPUT_BG,
                lightcolor=self.INPUT_BG,
                arrowcolor=getattr(self, "MUTED", self.TEXT),
                relief="flat",
                borderwidth=0,
                padding=(8, 4, 8, 4),
            )
            style.map(
                "RoundedField.TCombobox",
                fieldbackground=[("readonly", self.INPUT_BG), ("focus", self.INPUT_BG)],
                background=[("readonly", self.INPUT_BG), ("active", self.INPUT_BG)],
                foreground=[("readonly", self.TEXT)],
                bordercolor=[("focus", self.INPUT_BG), ("active", self.INPUT_BG)],
                arrowcolor=[("active", self.ACCENT), ("readonly", getattr(self, "MUTED", self.TEXT))],
            )
            self.option_add("*TCombobox*Listbox.background", self.SURFACE)
            self.option_add("*TCombobox*Listbox.foreground", self.TEXT)
            self.option_add("*TCombobox*Listbox.selectBackground", self.ACCENT)
            self.option_add("*TCombobox*Listbox.selectForeground", "#ffffff")
            self.option_add("*TCombobox*Listbox.borderWidth", 0)
            self.option_add("*TCombobox*Listbox.relief", "flat")
        except Exception:
            pass

    def _create_modern_rounded_combobox(self, *args, **kwargs):
        _configure_modern_combobox_style(self)
        shell, combo = original_create_rounded_combobox(self, *args, **kwargs)
        try:
            combo.configure(style="RoundedField.TCombobox", takefocus=True)
            combo["justify"] = "left"
            master = args[0] if args else kwargs.get("master")
            if (
                master is not None
                and master.winfo_toplevel().title() == "KickTheMap inloggen"
                and not combo.cget("values")
            ):
                # Dit veld is zowel accountkiezer als invoerveld voor een nieuw e-mailadres.
                # Een readonly combobox met een lege accountlijst kan helemaal geen invoer ontvangen.
                combo.configure(state="normal")
        except Exception:
            pass
        return shell, combo

    viewer_class._configure_modern_combobox_style = _configure_modern_combobox_style
    viewer_class._create_rounded_combobox = _create_modern_rounded_combobox
    viewer_class._modern_combobox_patch = True


def _install_modern_dialog_style_patch() -> None:
    viewer_class = globals().get("KlicViewerApp")
    if viewer_class is None or getattr(viewer_class, "_modern_dialog_style_patch", False):
        return

    def _configure_modern_dialog_styles(widget: tk.Misc) -> None:
        surface = getattr(viewer_class, "SURFACE", "#ffffff")
        text = getattr(viewer_class, "TEXT", "#111827")
        accent = getattr(viewer_class, "ACCENT", "#f97316")
        try:
            widget.option_add("*TCombobox*Listbox.background", surface)
            widget.option_add("*TCombobox*Listbox.foreground", text)
            widget.option_add("*TCombobox*Listbox.selectBackground", accent)
            widget.option_add("*TCombobox*Listbox.selectForeground", "#ffffff")
            widget.option_add("*TCombobox*Listbox.borderWidth", 0)
            widget.option_add("*TCombobox*Listbox.relief", "flat")
        except tk.TclError:
            pass

    def _apply_sleufbase_window_chrome(window: tk.Misc) -> None:
        try:
            window.configure(background=getattr(viewer_class, "APP_BG", "#f5f7fb"))
        except tk.TclError:
            pass
        icon_path = Path(__file__).resolve().parent.parent / "assets" / "sleufbase_icon.ico"
        if icon_path.exists():
            try:
                window.iconbitmap(str(icon_path))
            except tk.TclError:
                pass
        _configure_modern_dialog_styles(window)

    class SleufBaseToplevel(_ORIGINAL_TK_TOPLEVEL):
        def __init__(self, *args, **kwargs):
            super().__init__(*args, **kwargs)
            _apply_sleufbase_window_chrome(self)

    def _style_dialog_children(widget: tk.Misc) -> None:
        for child in widget.winfo_children():
            try:
                if isinstance(child, tk.Listbox):
                    child.configure(
                        background=getattr(viewer_class, "INPUT_BG", "#fbfcfe"),
                        foreground=getattr(viewer_class, "TEXT", "#111827"),
                        selectbackground=getattr(viewer_class, "ACCENT", "#f97316"),
                        selectforeground="#ffffff",
                        borderwidth=0,
                        highlightthickness=0,
                        relief=tk.FLAT,
                    )
            except tk.TclError:
                pass
            _style_dialog_children(child)

    def _schedule_style_dialog_children(dialog: tk.Misc) -> None:
        try:
            dialog.after_idle(lambda: _style_dialog_children(dialog))
        except tk.TclError:
            pass

    tk.Toplevel = SleufBaseToplevel  # type: ignore[assignment]
    viewer_class._configure_modern_dialog_styles = staticmethod(_configure_modern_dialog_styles)
    viewer_class._apply_sleufbase_window_chrome = staticmethod(_apply_sleufbase_window_chrome)
    viewer_class._style_dialog_children = staticmethod(_style_dialog_children)
    viewer_class._schedule_style_dialog_children = staticmethod(_schedule_style_dialog_children)
    viewer_class._modern_dialog_style_patch = True


def _install_inline_location_search_patch() -> None:
    viewer_class = globals().get("KlicViewerApp")
    if viewer_class is None or getattr(viewer_class, "_inline_location_search_patch", False):
        return

    original_build_layout = viewer_class._build_layout

    def _parse_inline_coordinate_query(self, query: str):
        text = query.strip()
        rd_coordinates = self.location_client.parse_rd_input(text)
        if rd_coordinates is not None:
            x_coord, y_coord = rd_coordinates
            return "RD locatie: X %.2f, Y %.2f" % (x_coord, y_coord), float(x_coord), float(y_coord), "rd"

        coordinate_text = re.sub(r"(?i)\bwgs\s*84\b|\bepsg\s*:?\s*4326\b", " ", text)
        values = [float(match.replace(",", ".")) for match in re.findall(r"-?\d+(?:[.,]\d+)?", coordinate_text)]
        if len(values) < 2:
            return None
        first, second = values[0], values[1]
        lower = text.casefold()
        explicit_wgs = any(part in lower for part in ("wgs", "gps", "lat", "lon", "lng"))

        lat = lon = None
        if -90.0 <= first <= 90.0 and -180.0 <= second <= 180.0 and (explicit_wgs or (50.0 <= first <= 54.5 and 2.5 <= second <= 8.0)):
            lat, lon = first, second
        elif -180.0 <= first <= 180.0 and -90.0 <= second <= 90.0 and (explicit_wgs or (2.5 <= first <= 8.0 and 50.0 <= second <= 54.5)):
            lon, lat = first, second

        if lat is None or lon is None:
            return None
        try:
            from pyproj import Transformer

            transformer = getattr(self, "_inline_wgs84_to_rd_transformer", None)
            if transformer is None:
                transformer = Transformer.from_crs("EPSG:4326", "EPSG:28992", always_xy=True)
                self._inline_wgs84_to_rd_transformer = transformer
            x_coord, y_coord = transformer.transform(lon, lat)
        except Exception:
            return None
        return "GPS locatie: %.6f, %.6f" % (lat, lon), float(x_coord), float(y_coord), "wgs84"

    def _location_world_to_screen(self, x_coord: float, y_coord: float) -> tuple[float, float]:
        bounds = self._current_view_bounds()
        width, height = self._canvas_size()
        screen_x = ((x_coord - bounds.min_x) / max(bounds.width, 1e-9)) * width
        screen_y = height - ((y_coord - bounds.min_y) / max(bounds.height, 1e-9)) * height
        return screen_x, screen_y

    def _start_location_pulse(self, x_coord: float, y_coord: float) -> None:
        canvas = getattr(self, "canvas", None)
        if canvas is None:
            return
        old_items = getattr(self, "_inline_location_pulse_items", ())
        for item in old_items:
            try:
                canvas.delete(item)
            except tk.TclError:
                pass
        token = getattr(self, "_inline_location_pulse_token", 0) + 1
        self._inline_location_pulse_token = token
        self._inline_location_pulse_world = (float(x_coord), float(y_coord))
        ring = canvas.create_oval(0, 0, 1, 1, outline="#f97316", width=3, tags=("location_pulse",))
        dot = canvas.create_oval(0, 0, 1, 1, fill="#f97316", outline="#ffffff", width=2, tags=("location_pulse",))
        self._inline_location_pulse_items = (ring, dot)

        def animate(step: int = 0) -> None:
            if getattr(self, "_inline_location_pulse_token", None) != token:
                return
            try:
                sx, sy = self._location_world_to_screen(x_coord, y_coord)
                phase = step % 18
                radius = 8 + phase * 1.2
                ring_width = 3 if phase < 9 else 2
                canvas.coords(ring, sx - radius, sy - radius, sx + radius, sy + radius)
                canvas.itemconfigure(ring, width=ring_width, state="normal" if step < 100 else "hidden")
                canvas.coords(dot, sx - 4, sy - 4, sx + 4, sy + 4)
                canvas.tag_raise(ring)
                canvas.tag_raise(dot)
                if step >= 100:
                    canvas.delete(ring)
                    canvas.delete(dot)
                    self._inline_location_pulse_items = ()
                    return
                self.after(50, lambda: animate(step + 1))
            except tk.TclError:
                return

        animate()

    def _build_inline_location_search(self) -> None:
        if getattr(self, "_inline_location_search_frame", None) is not None:
            return
        canvas = getattr(self, "canvas", None)
        if canvas is None:
            return

        suggestions = tk.Listbox(
            canvas,
            activestyle="none",
            borderwidth=0,
            height=0,
            highlightthickness=0,
            exportselection=False,
            background="#ffffff",
            foreground="#111827",
            selectbackground="#f97316",
            selectforeground="#ffffff",
        )

        entry = _ORIGINAL_TK_ENTRY(
            canvas,
            textvariable=self.location_query_var,
            width=34,
            borderwidth=0,
            highlightthickness=0,
            relief=tk.FLAT,
            background="#ffffff",
            foreground="#111827",
            insertbackground="#111827",
            selectbackground="#bfdbfe",
            selectforeground="#111827",
        )
        try:
            ttk.Style(self).configure("LocationPulse.TButton", background="#ffffff", foreground="#f97316")
        except tk.TclError:
            pass
        pulse_button = tk.Button(canvas, text="●", command=self._pulse_inline_location_only, style="LocationPulse.TButton", width=2)
        close_button = tk.Button(canvas, text="x", command=self._hide_inline_location_search, style="Compact.TButton", width=2)

        background_item = canvas.create_polygon(0, 0, 1, 1, smooth=True, splinesteps=12, fill="#ffffff", outline="#dde4ee", state="hidden")
        entry_window = canvas.create_window(0, 0, anchor="nw", window=entry, state="hidden")
        pulse_window = canvas.create_window(0, 0, anchor="nw", window=pulse_button, state="hidden")
        close_window = canvas.create_window(0, 0, anchor="nw", window=close_button, state="hidden")
        suggestions_background_item = canvas.create_polygon(
            0, 0, 1, 1, smooth=True, splinesteps=12, fill="#ffffff", outline="#dde4ee", state="hidden"
        )
        suggestions_window = canvas.create_window(0, 0, anchor="nw", window=suggestions, state="hidden")

        self._inline_location_search_frame = canvas
        self._inline_location_search_entry = entry
        self._inline_location_search_suggestions = suggestions
        self._inline_location_search_items = {
            "background": background_item,
            "entry": entry_window,
            "pulse": pulse_window,
            "close": close_window,
            "suggestions_background": suggestions_background_item,
            "suggestions": suggestions_window,
        }
        self._inline_location_search_results = []
        self._inline_location_search_after = None
        self._inline_location_search_pending = None
        self._inline_location_search_token = 0
        self._inline_location_search_visible = False

        entry.bind("<Return>", self._inline_location_search_submit)
        entry.bind("<Escape>", lambda _event: self._hide_inline_location_search())
        entry.bind("<Down>", self._inline_location_search_focus_suggestions)
        entry.bind("<Button-1>", lambda _event: entry.after_idle(entry.focus_force), add="+")
        suggestions.bind("<Return>", self._inline_location_search_choose_selected)
        suggestions.bind("<Double-Button-1>", self._inline_location_search_choose_selected)
        suggestions.bind("<Escape>", lambda _event: self._hide_inline_location_search())
        suggestions.bind("<Up>", self._inline_location_search_return_to_entry)
        canvas.bind("<Configure>", lambda _event: self._layout_inline_location_search(), add="+")
        self.location_query_var.trace_add("write", lambda *_args: self._schedule_inline_location_search())

    def _rounded_rect_points(x1: int, y1: int, x2: int, y2: int, radius: int) -> list[int]:
        radius = max(1, min(radius, (x2 - x1) // 2, (y2 - y1) // 2))
        return [
            x1 + radius, y1, x2 - radius, y1, x2, y1, x2, y1 + radius,
            x2, y2 - radius, x2, y2, x2 - radius, y2, x1 + radius, y2,
            x1, y2, x1, y2 - radius, x1, y1 + radius, x1, y1,
        ]

    def _layout_inline_location_search(self) -> None:
        if not getattr(self, "_inline_location_search_visible", False):
            return
        canvas = getattr(self, "canvas", None)
        items = getattr(self, "_inline_location_search_items", None)
        if canvas is None or not items:
            return
        width = max(1, canvas.winfo_width())
        height = max(1, canvas.winfo_height())
        bar_width = min(460, max(320, width - 40))
        bar_height = 42
        x1 = int((width - bar_width) / 2)
        y1 = max(12, height - bar_height - 18)
        x2 = x1 + bar_width
        y2 = y1 + bar_height
        canvas.coords(items["background"], *_rounded_rect_points(x1, y1, x2, y2, 14))
        canvas.coords(items["entry"], x1 + 12, y1 + 7)
        canvas.itemconfigure(items["entry"], width=bar_width - 96, height=28)
        canvas.coords(items["pulse"], x2 - 76, y1 + 7)
        canvas.itemconfigure(items["pulse"], width=30, height=28)
        canvas.coords(items["close"], x2 - 42, y1 + 7)
        canvas.itemconfigure(items["close"], width=30, height=28)

        results = getattr(self, "_inline_location_search_results", [])
        if results:
            rows = min(4, len(results))
            suggestion_height = rows * 24 + 10
            sy1 = max(8, y1 - suggestion_height - 6)
            sy2 = sy1 + suggestion_height
            canvas.coords(items["suggestions_background"], *_rounded_rect_points(x1, sy1, x2, sy2, 12))
            canvas.coords(items["suggestions"], x1 + 6, sy1 + 5)
            canvas.itemconfigure(items["suggestions"], width=bar_width - 12, height=suggestion_height - 10)

    def _show_inline_location_search(self) -> None:
        self._build_inline_location_search()
        entry = getattr(self, "_inline_location_search_entry", None)
        items = getattr(self, "_inline_location_search_items", None)
        canvas = getattr(self, "canvas", None)
        if canvas is None or entry is None or not items:
            return
        self._inline_location_search_visible = True
        for key in ("background", "entry", "pulse", "close"):
            canvas.itemconfigure(items[key], state="normal")
            canvas.tag_raise(items[key])
        self._layout_inline_location_search()
        def focus_entry() -> None:
            try:
                entry.focus_force()
                entry.icursor(tk.END)
                entry.selection_range(0, tk.END)
            except tk.TclError:
                pass
        focus_entry()
        self.after_idle(focus_entry)
        self.after(80, focus_entry)
        self._schedule_inline_location_search()

    def _hide_inline_location_search(self) -> None:
        canvas = getattr(self, "canvas", None)
        items = getattr(self, "_inline_location_search_items", None)
        if canvas is not None and items:
            for item in items.values():
                canvas.itemconfigure(item, state="hidden")
        self._inline_location_search_results = []
        self._inline_location_search_pending = None
        self._inline_location_search_token += 1
        self._inline_location_search_visible = False

    def _build_layout_with_inline_location_search(self) -> None:
        original_build_layout(self)
        self._build_inline_location_search()

    def open_location_dialog(self) -> None:
        turn_off_streetsmart = getattr(self, "_turn_off_streetsmart_selector", None)
        if callable(turn_off_streetsmart):
            turn_off_streetsmart(status_text=None, render=True)
        try:
            grab_widget = self.grab_current()
            if grab_widget is not None and not isinstance(grab_widget, tk.Toplevel):
                grab_widget.grab_release()
        except tk.TclError:
            pass
        self._show_inline_location_search()

    def _schedule_inline_location_search(self) -> None:
        if not getattr(self, "_inline_location_search_visible", False):
            return
        after_id = getattr(self, "_inline_location_search_after", None)
        if after_id is not None:
            try:
                self.after_cancel(after_id)
            except tk.TclError:
                pass
        self._inline_location_search_after = self.after(250, self._start_inline_location_search)

    def _start_inline_location_search(self) -> None:
        self._inline_location_search_after = None
        query = self.location_query_var.get().strip()
        self._inline_location_search_token += 1
        token = self._inline_location_search_token
        if len(query) < 3:
            self._set_inline_location_suggestions(token, [])
            return

        coordinate_result = self._parse_inline_coordinate_query(query)
        if coordinate_result is not None:
            label, x_coord, y_coord, location_type = coordinate_result
            self._set_inline_location_suggestions(
                token,
                [(label, x_coord, y_coord, location_type)],
            )
            return

        def worker() -> None:
            try:
                results = self.location_client.search(query, rows=6)
                suggestions = [(result.label, result.x, result.y, result.location_type) for result in results]
            except Exception:
                suggestions = []
            self._inline_location_search_pending = (token, suggestions)

        threading.Thread(target=worker, daemon=True).start()
        self.after(60, self._poll_inline_location_search)

    def _poll_inline_location_search(self) -> None:
        pending = getattr(self, "_inline_location_search_pending", None)
        if pending is not None:
            self._inline_location_search_pending = None
            token, suggestions = pending
            self._set_inline_location_suggestions(token, suggestions)
            return
        if getattr(self, "_inline_location_search_visible", False):
            self.after(60, self._poll_inline_location_search)

    def _set_inline_location_suggestions(self, token: int, suggestions_data: list[tuple[str, float, float, str]]) -> None:
        if token != getattr(self, "_inline_location_search_token", token):
            return
        if not getattr(self, "_inline_location_search_visible", False):
            canvas = getattr(self, "canvas", None)
            items = getattr(self, "_inline_location_search_items", None)
            if canvas is not None and items:
                canvas.itemconfigure(items["suggestions_background"], state="hidden")
                canvas.itemconfigure(items["suggestions"], state="hidden")
            return
        suggestions = getattr(self, "_inline_location_search_suggestions", None)
        canvas = getattr(self, "canvas", None)
        items = getattr(self, "_inline_location_search_items", None)
        if suggestions is None or canvas is None or not items:
            return
        self._inline_location_search_results = suggestions_data
        suggestions.delete(0, tk.END)
        for label, _x, _y, location_type in suggestions_data:
            suffix = f" ({location_type})" if location_type and location_type != "rd" else ""
            suggestions.insert(tk.END, f"{label}{suffix}")
        if suggestions_data:
            suggestions.configure(height=min(4, len(suggestions_data)))
            canvas.itemconfigure(items["suggestions_background"], state="normal")
            canvas.itemconfigure(items["suggestions"], state="normal")
            canvas.tag_raise(items["suggestions_background"])
            canvas.tag_raise(items["suggestions"])
        else:
            canvas.itemconfigure(items["suggestions_background"], state="hidden")
            canvas.itemconfigure(items["suggestions"], state="hidden")
        self._layout_inline_location_search()

    def _inline_location_search_focus_suggestions(self, _event=None):
        suggestions = getattr(self, "_inline_location_search_suggestions", None)
        if suggestions is None or not getattr(self, "_inline_location_search_results", None):
            return "break"
        suggestions.focus_set()
        suggestions.selection_clear(0, tk.END)
        suggestions.selection_set(0)
        suggestions.activate(0)
        return "break"

    def _inline_location_search_return_to_entry(self, _event=None):
        suggestions = getattr(self, "_inline_location_search_suggestions", None)
        entry = getattr(self, "_inline_location_search_entry", None)
        if suggestions is not None and suggestions.index(tk.ACTIVE) > 0:
            return None
        if entry is not None:
            entry.focus_set()
            return "break"
        return None

    def _inline_location_search_submit(self, _event=None):
        results = getattr(self, "_inline_location_search_results", [])
        if results:
            self._open_inline_location_result(0)
            return "break"
        query = self.location_query_var.get().strip()
        if not query:
            self.set_status("Geen locatie ingevoerd.")
            return "break"
        coordinate_result = self._parse_inline_coordinate_query(query)
        if coordinate_result is not None:
            label, x_coord, y_coord, location_type = coordinate_result
            turn_off_streetsmart = getattr(self, "_turn_off_streetsmart_selector", None)
            if callable(turn_off_streetsmart):
                turn_off_streetsmart(status_text=None, render=False)
            self.center_x = x_coord
            self.center_y = y_coord
            self.meters_per_pixel = 0.2 if location_type in {"rd", "wgs84"} else self._zoom_for_location_type(location_type)
            self.set_status(f"Gesprongen naar: {label}")
            self.request_render(True)
            self.after(120, lambda: self._start_location_pulse(x_coord, y_coord))
            self._hide_inline_location_search()
            return "break"
        self.location_query_var.set(query)
        turn_off_streetsmart = getattr(self, "_turn_off_streetsmart_selector", None)
        if callable(turn_off_streetsmart):
            turn_off_streetsmart(status_text=None, render=False)
        self._go_to_location()
        self.after(120, lambda: self._start_location_pulse(self.center_x, self.center_y))
        self._hide_inline_location_search()
        return "break"

    def _pulse_inline_location_only(self):
        query = self.location_query_var.get().strip()
        target = None
        if query:
            target = self._parse_inline_coordinate_query(query)
        if target is None:
            results = getattr(self, "_inline_location_search_results", [])
            if results:
                target = results[0]
        if target is None:
            self.set_status("Geen locatie om te markeren.")
            return
        label, x_coord, y_coord, _location_type = target
        self.set_status(f"Locatie gemarkeerd: {label}")
        self._start_location_pulse(x_coord, y_coord)

    def _inline_location_search_choose_selected(self, _event=None):
        suggestions = getattr(self, "_inline_location_search_suggestions", None)
        if suggestions is None:
            return "break"
        selection = suggestions.curselection()
        index = int(selection[0]) if selection else int(suggestions.index(tk.ACTIVE) or 0)
        self._open_inline_location_result(index)
        return "break"

    def _open_inline_location_result(self, index: int) -> None:
        results = getattr(self, "_inline_location_search_results", [])
        if index < 0 or index >= len(results):
            return
        label, x_coord, y_coord, location_type = results[index]
        if location_type not in {"rd", "wgs84"}:
            self.location_query_var.set(label)
        turn_off_streetsmart = getattr(self, "_turn_off_streetsmart_selector", None)
        if callable(turn_off_streetsmart):
            turn_off_streetsmart(status_text=None, render=False)
        self.center_x = x_coord
        self.center_y = y_coord
        self.meters_per_pixel = 0.2 if location_type in {"rd", "wgs84"} else self._zoom_for_location_type(location_type)
        self.set_status(f"Gesprongen naar: {label}")
        self.request_render(True)
        self.after(120, lambda: self._start_location_pulse(x_coord, y_coord))
        self._hide_inline_location_search()

    viewer_class._build_inline_location_search = _build_inline_location_search
    viewer_class._parse_inline_coordinate_query = _parse_inline_coordinate_query
    viewer_class._location_world_to_screen = _location_world_to_screen
    viewer_class._start_location_pulse = _start_location_pulse
    viewer_class._show_inline_location_search = _show_inline_location_search
    viewer_class._hide_inline_location_search = _hide_inline_location_search
    viewer_class._layout_inline_location_search = _layout_inline_location_search
    viewer_class._schedule_inline_location_search = _schedule_inline_location_search
    viewer_class._start_inline_location_search = _start_inline_location_search
    viewer_class._poll_inline_location_search = _poll_inline_location_search
    viewer_class._set_inline_location_suggestions = _set_inline_location_suggestions
    viewer_class._inline_location_search_focus_suggestions = _inline_location_search_focus_suggestions
    viewer_class._inline_location_search_return_to_entry = _inline_location_search_return_to_entry
    viewer_class._inline_location_search_submit = _inline_location_search_submit
    viewer_class._pulse_inline_location_only = _pulse_inline_location_only
    viewer_class._inline_location_search_choose_selected = _inline_location_search_choose_selected
    viewer_class._open_inline_location_result = _open_inline_location_result
    viewer_class.open_location_dialog = open_location_dialog
    viewer_class._build_layout = _build_layout_with_inline_location_search
    viewer_class._inline_location_search_patch = True


def _install_modern_dropdown_menu_patch() -> None:
    viewer_class = globals().get("KlicViewerApp")
    if viewer_class is None or getattr(viewer_class, "_modern_dropdown_menu_patch", False):
        return

    original_build_menu = viewer_class._build_menu
    original_build_layout = viewer_class._build_layout

    def _rounded_menu_rect_points(x1: int, y1: int, x2: int, y2: int, radius: int) -> list[int]:
        radius = max(1, min(radius, (x2 - x1) // 2, (y2 - y1) // 2))
        return [
            x1 + radius, y1, x2 - radius, y1, x2, y1, x2, y1 + radius,
            x2, y2 - radius, x2, y2, x2 - radius, y2, x1 + radius, y2,
            x1, y2, x1, y2 - radius, x1, y1 + radius, x1, y1,
        ]

    def _capture_modern_menu_specs(self, menu_bar) -> list[dict]:
        specs = []
        try:
            end_index = menu_bar.index("end")
        except tk.TclError:
            end_index = None
        if end_index is None:
            return specs
        for index in range(end_index + 1):
            try:
                menu_type = menu_bar.type(index)
                label = str(menu_bar.entrycget(index, "label") or "").strip()
                if menu_type == "command" and label:
                    specs.append(
                        {
                            "label": label,
                            "items": [],
                            "command": lambda menu=menu_bar, idx=index: menu.invoke(idx),
                        }
                    )
                    continue
                if menu_type != "cascade":
                    continue
                submenu = self.nametowidget(menu_bar.entrycget(index, "menu"))
                submenu_end = submenu.index("end")
            except tk.TclError:
                continue
            items = []
            if submenu_end is not None:
                for item_index in range(submenu_end + 1):
                    try:
                        item_type = submenu.type(item_index)
                        if item_type == "separator":
                            items.append({"type": "separator"})
                            continue
                        if item_type != "command":
                            continue
                        item_label = str(submenu.entrycget(item_index, "label") or "").strip()
                        state = str(submenu.entrycget(item_index, "state") or tk.NORMAL)
                    except tk.TclError:
                        continue
                    if item_label:
                        items.append(
                            {
                                "type": "command",
                                "label": item_label,
                                "state": state,
                                "command": lambda menu=submenu, idx=item_index: menu.invoke(idx),
                            }
                        )
            if label and items:
                specs.append({"label": label, "items": items})
        return specs

    def _build_menu_with_modern_dropdowns(self) -> None:
        original_build_menu(self)
        try:
            menu_name = self.cget("menu")
            menu_bar = self.nametowidget(menu_name)
            self._modern_menu_specs = self._capture_modern_menu_specs(menu_bar)
            try:
                self.after(0, self._build_modern_menu_bar)
            except Exception:
                pass
            menu_bar.configure(
                background=self.APP_BG,
                foreground=self.TEXT,
                activebackground="#fff4ed",
                activeforeground=self.TEXT,
                borderwidth=0,
                relief=tk.FLAT,
            )
            end_index = menu_bar.index("end")
            if end_index is None:
                return
            for index in range(end_index + 1):
                if menu_bar.type(index) != "cascade":
                    continue
                submenu = self.nametowidget(menu_bar.entrycget(index, "menu"))
                submenu.configure(
                    background=self.SURFACE,
                    foreground=self.TEXT,
                    activebackground="#fff4ed",
                    activeforeground=self.TEXT,
                    borderwidth=1,
                    relief=tk.FLAT,
                    tearoff=False,
                font=("Segoe UI", 9),
                )
        except Exception:
            return

    def _close_modern_menu_dropdown(self) -> None:
        popup = getattr(self, "_modern_menu_dropdown", None)
        if popup is not None:
            try:
                popup.destroy()
            except tk.TclError:
                pass
        self._modern_menu_dropdown = None
        active = getattr(self, "_modern_menu_active_button", None)
        if active is not None:
            try:
                active.configure(style="Compact.TButton")
            except Exception:
                pass
        self._modern_menu_active_button = None

    def _show_modern_menu_dropdown(self, button: tk.Widget, spec: dict) -> None:
        self._close_modern_menu_dropdown()
        items = list(spec.get("items", []))
        command_items = [item for item in items if item.get("type") == "command"]
        if not command_items:
            return
        menu_font = tkfont.nametofont("TkMenuFont")
        row_height = 30
        separator_height = 9
        width = max(190, max(menu_font.measure(str(item.get("label", ""))) + 42 for item in command_items))
        height = 12 + sum(separator_height if item.get("type") == "separator" else row_height for item in items)
        popup = tk.Toplevel(self)
        popup.withdraw()
        popup.overrideredirect(True)
        popup.configure(background=self.APP_BG)
        x = button.winfo_rootx()
        y = button.winfo_rooty() + button.winfo_height() + 4
        popup.geometry(f"{width}x{height}+{x}+{y}")

        canvas = tk.Canvas(popup, width=width, height=height, bd=0, highlightthickness=0, background=self.APP_BG)
        canvas.pack(fill="both", expand=True)
        canvas.create_polygon(
            _rounded_menu_rect_points(1, 1, width - 2, height - 2, 12),
            smooth=True,
            splinesteps=12,
            fill=self.SURFACE,
            outline=self.BORDER_SOFT,
        )

        y_cursor = 6
        for item in items:
            if item.get("type") == "separator":
                canvas.create_line(12, y_cursor + 4, width - 12, y_cursor + 4, fill=self.BORDER_SOFT)
                y_cursor += separator_height
                continue
            disabled = item.get("state") == tk.DISABLED
            fill = self.SURFACE
            foreground = getattr(self, "MUTED", "#64748b") if disabled else self.TEXT
            top = y_cursor
            bottom = y_cursor + row_height
            row_id = canvas.create_rectangle(7, top, width - 7, bottom, fill=fill, outline="")
            text_id = canvas.create_text(
                18,
                top + row_height / 2,
                anchor="w",
                text=str(item.get("label", "")),
                fill=foreground,
                font=("Segoe UI", 9),
            )

            if not disabled:
                def on_enter(_event, row=row_id):
                    canvas.itemconfigure(row, fill="#fff4ed")

                def on_leave(_event, row=row_id):
                    canvas.itemconfigure(row, fill=self.SURFACE)

                def on_click(_event, command=item.get("command")):
                    self._close_modern_menu_dropdown()
                    if command is not None:
                        command()

                for canvas_item in (row_id, text_id):
                    canvas.tag_bind(canvas_item, "<Enter>", on_enter)
                    canvas.tag_bind(canvas_item, "<Leave>", on_leave)
                    canvas.tag_bind(canvas_item, "<Button-1>", on_click)
            y_cursor = bottom

        popup.bind("<Escape>", lambda _event: self._close_modern_menu_dropdown())
        popup.bind("<FocusOut>", lambda _event: self._close_modern_menu_dropdown())
        popup.deiconify()
        popup.focus_force()
        self._modern_menu_dropdown = popup
        self._modern_menu_active_button = button

    def _build_modern_menu_bar(self) -> None:
        if getattr(self, "_modern_menu_bar", None) is not None:
            return
        specs = getattr(self, "_modern_menu_specs", [])
        if not specs:
            return
        for slave in list(self.grid_slaves()):
            try:
                info = slave.grid_info()
                slave.grid_configure(row=int(info.get("row", 0)) + 1)
            except Exception:
                pass
        bar = tk.Frame(self, background=self.APP_BG, height=30)
        bar.grid(row=0, column=0, columnspan=20, sticky="ew")
        bar.grid_propagate(False)
        try:
            self.grid_rowconfigure(0, weight=0)
            self.grid_rowconfigure(1, weight=1)
            self.grid_columnconfigure(0, weight=1)
        except tk.TclError:
            pass
        self._modern_menu_bar = bar
        for column, spec in enumerate(specs):
            button = tk.Button(bar, text=spec["label"], style="Compact.TButton", padding=(10, 4))
            button.grid(row=0, column=column, padx=(18 if column == 0 else 2, 2), pady=(2, 0), sticky="w")
            if spec.get("label") == "Instellingen":
                command_items = [item for item in spec.get("items", []) if item.get("type") == "command" and item.get("state") != tk.DISABLED]
                if command_items:
                    button.configure(command=command_items[0].get("command"))
                elif spec.get("command") is not None:
                    button.configure(command=spec["command"])
            elif spec.get("command") is not None:
                button.configure(command=spec["command"])
            else:
                button.configure(command=lambda widget=button, menu_spec=spec: self._show_modern_menu_dropdown(widget, menu_spec))
        try:
            self.configure(menu=tk.Menu(self))
        except tk.TclError:
            pass

    def _build_layout_with_modern_menu_bar(self) -> None:
        original_build_layout(self)
        self._build_modern_menu_bar()

    viewer_class._capture_modern_menu_specs = _capture_modern_menu_specs
    viewer_class._close_modern_menu_dropdown = _close_modern_menu_dropdown
    viewer_class._show_modern_menu_dropdown = _show_modern_menu_dropdown
    viewer_class._build_modern_menu_bar = _build_modern_menu_bar
    viewer_class._build_menu = _build_menu_with_modern_dropdowns
    viewer_class._build_layout = _build_layout_with_modern_menu_bar
    viewer_class._modern_dropdown_menu_patch = True


def _install_streetsmart_selector_patch() -> None:
    viewer_class = globals().get("KlicViewerApp")
    if viewer_class is None or getattr(viewer_class, "_streetsmart_selector_patch", False):
        return

    original_init = viewer_class.__init__
    original_build_menu = viewer_class._build_menu
    original_request_render = viewer_class.request_render
    original_active_map_markers = viewer_class._active_map_markers
    original_on_canvas_release = viewer_class._on_canvas_release
    original_on_canvas_move = viewer_class._on_canvas_move
    original_set_streetsmart_panel_visible = viewer_class._set_streetsmart_panel_visible
    original_streetsmart_logout = viewer_class.streetsmart_logout

    STREETSMART_RECORDING_MAX_MPP = 20.0
    STREETSMART_RECORDING_WFS_URL = "https://atlasapi.cyclomedia.com/api/recording/wfs"
    STREETSMART_RECORDING_RADIUS_PX = 5
    STREETSMART_LATEST_YEAR_OPTION = "Nieuwste"
    STREETSMART_RECORDING_MAX_FEATURES = "2500"

    try:
        from . import streetsmart as streetsmart_mod
        from . import streetsmart_browser as streetsmart_browser_mod
        from . import streetsmart_panel as streetsmart_panel_mod

        original_streetsmart_selection_url = streetsmart_mod.streetsmart_selection_url

        def _streetsmart_selection_url_with_image_id(selection):
            if isinstance(selection, dict):
                image_id = str(selection.get("imageId") or "").strip()
                if image_id:
                    return f"{STREETSMART_WEB_URL}?q={quote(image_id, safe='')}&_ktk={time.time_ns()}"
            url = original_streetsmart_selection_url(selection)
            separator = "&" if "?" in url else "?"
            return f"{url}{separator}_ktk={time.time_ns()}"

        streetsmart_mod.streetsmart_selection_url = _streetsmart_selection_url_with_image_id
        streetsmart_browser_mod.streetsmart_selection_url = _streetsmart_selection_url_with_image_id
        streetsmart_panel_mod.streetsmart_selection_url = _streetsmart_selection_url_with_image_id
    except Exception:
        pass

    def _init_with_streetsmart_selector(self, *args, **kwargs):
        original_init(self, *args, **kwargs)
        self.streetsmart_selector_enabled = False
        self._streetsmart_recording_all_markers = []
        self._streetsmart_recording_all_payloads = {}
        self._streetsmart_recording_markers = []
        self._streetsmart_recording_payloads = {}
        self._streetsmart_recording_years = []
        self.streetsmart_recording_year_var = tk.StringVar(value=STREETSMART_LATEST_YEAR_OPTION)
        self._streetsmart_recording_request_key = None
        self._streetsmart_recording_inflight_key = None
        self._streetsmart_recording_pending = None
        self._streetsmart_recording_polling = False
        self._streetsmart_prelogin_panel = None
        self._streetsmart_prelogin_host = None
        self._build_streetsmart_year_selector()
        try:
            self.after(900, self._streetsmart_prepare_session_on_startup)
        except Exception:
            pass

    def _has_streetsmart_selector_zoom(self) -> bool:
        try:
            return float(self.meters_per_pixel) <= STREETSMART_RECORDING_MAX_MPP
        except (TypeError, ValueError):
            return False

    def _streetview_selector_label(self) -> str:
        enabled = bool(getattr(self, "streetsmart_selector_enabled", False))
        return "Streetview selecter uit" if enabled else "Streetview selecter"

    def _build_menu_with_streetsmart_selector(self) -> None:
        original_build_menu(self)
        try:
            menu_name = self.cget("menu")
            menu_bar = self.nametowidget(menu_name)
            end_index = menu_bar.index("end")
            if end_index is None:
                return
            for index in range(end_index + 1):
                if menu_bar.type(index) != "cascade":
                    continue
                if menu_bar.entrycget(index, "label") != "StreetSmart":
                    continue
                submenu = self.nametowidget(menu_bar.entrycget(index, "menu"))
                submenu_end = submenu.index("end")
                for submenu_index in range(submenu_end if submenu_end is not None else -1, -1, -1):
                    if submenu.type(submenu_index) == "command" and submenu.entrycget(submenu_index, "label") in {
                        "Streetview selecter",
                        "Streetview selecter uit",
                    }:
                        submenu.delete(submenu_index)
                submenu.insert_command(
                    1,
                    label=self._streetview_selector_label(),
                    command=self.toggle_streetsmart_selector,
                )
                return
        except Exception:
            return

    def _update_streetsmart_selector_menu_label(self) -> None:
        try:
            menu_name = self.cget("menu")
            menu_bar = self.nametowidget(menu_name)
            end_index = menu_bar.index("end")
            if end_index is None:
                return
            for index in range(end_index + 1):
                if menu_bar.type(index) != "cascade" or menu_bar.entrycget(index, "label") != "StreetSmart":
                    continue
                submenu = self.nametowidget(menu_bar.entrycget(index, "menu"))
                submenu_end = submenu.index("end")
                if submenu_end is None:
                    return
                for submenu_index in range(submenu_end + 1):
                    if submenu.type(submenu_index) != "command":
                        continue
                    if submenu.entrycget(submenu_index, "label") in {"Streetview selecter", "Streetview selecter uit"}:
                        submenu.entryconfigure(submenu_index, label=self._streetview_selector_label())
                        return
        except Exception:
            return

    def toggle_streetsmart_selector(self) -> None:
        if not self._has_streetsmart_credentials():
            self.open_streetsmart_login_dialog(open_browser_on_success=False)
            return
        if bool(getattr(self, "streetsmart_selector_enabled", False)):
            self._turn_off_streetsmart_selector(status_text="Streetview selecter uit.")
            return
        hide_location_search = getattr(self, "_hide_inline_location_search", None)
        if callable(hide_location_search):
            hide_location_search()
        self.streetsmart_selector_enabled = True
        self.streetsmart_recording_year_var.set(STREETSMART_LATEST_YEAR_OPTION)
        self._update_streetsmart_selector_menu_label()
        self._prepare_streetsmart_panel_session()
        if self.streetsmart_selector_enabled:
            if self._has_streetsmart_selector_zoom():
                self.set_status("Streetview selecter aan. StreetSmart-opnamepunten worden geladen.")
                self._schedule_streetsmart_recordings_fetch()
            else:
                self.set_status("Streetview selecter aan. Zoom verder in om StreetSmart-opnamepunten te tonen.")
        self.request_render(False)

    def _clear_streetsmart_recording_state(self) -> None:
        self._streetsmart_recording_all_markers = []
        self._streetsmart_recording_all_payloads = {}
        self._streetsmart_recording_markers = []
        self._streetsmart_recording_payloads = {}
        self._streetsmart_recording_years = []
        self._streetsmart_recording_request_key = None
        self._streetsmart_recording_inflight_key = None
        self._streetsmart_recording_pending = None
        self.streetsmart_recording_year_var.set(STREETSMART_LATEST_YEAR_OPTION)

    def _turn_off_streetsmart_selector(self, status_text: str | None = "Streetview selecter uit.", render: bool = True) -> bool:
        was_enabled = bool(getattr(self, "streetsmart_selector_enabled", False))
        self.streetsmart_selector_enabled = False
        self._clear_streetsmart_recording_state()
        self._update_streetsmart_selector_menu_label()
        self._refresh_streetsmart_year_selector()
        if status_text:
            self.set_status(status_text)
        if render and was_enabled:
            self.request_render(False)
        return was_enabled

    def _prepare_streetsmart_panel_session(self) -> bool:
        if not self._has_streetsmart_credentials():
            return False
        if getattr(self, "streetsmart_panel", None) is not None:
            try:
                self.streetsmart_panel.update_credentials(
                    str(self.settings.streetsmart_username or "").strip(),
                    str(self.settings.streetsmart_password or ""),
                )
                self.streetsmart_panel.navigate_url(STREETSMART_WEB_URL)
                return True
            except Exception:
                self.streetsmart_panel = None
        host = getattr(self, "streetsmart_host", None)
        if host is None:
            return False
        try:
            panel = EmbeddedStreetSmartPanel(host, status_callback=self._set_streetsmart_panel_status)
            panel.update_credentials(
                str(self.settings.streetsmart_username or "").strip(),
                str(self.settings.streetsmart_password or ""),
            )
            panel.navigate_url(STREETSMART_WEB_URL)
            self.streetsmart_panel = panel
            self.set_status("StreetSmart sessie wordt voorbereid...")
            return True
        except Exception as exc:
            self.streetsmart_panel = None
            self._set_streetsmart_panel_status(f"StreetSmart paneel fout: {exc}")
            return False

    def _streetsmart_prepare_session_on_startup(self) -> None:
        if self._prepare_streetsmart_panel_session():
            return
        self._streetsmart_auto_login_on_startup()

    def _streetsmart_auto_login_on_startup(self) -> None:
        if not self._has_streetsmart_credentials():
            return
        try:
            if getattr(self, "streetsmart_panel", None) is not None or getattr(self, "_streetsmart_prelogin_panel", None) is not None:
                return
            host = tk.Frame(self, width=1, height=1, background=getattr(self, "APP_BG", "#ffffff"))
            host.place(x=-24, y=-24, width=1, height=1)
            panel = EmbeddedStreetSmartPanel(host, status_callback=self._set_streetsmart_panel_status)
            panel.update_credentials(
                str(self.settings.streetsmart_username or "").strip(),
                str(self.settings.streetsmart_password or ""),
            )
            panel.navigate_url(STREETSMART_WEB_URL)
            self._streetsmart_prelogin_host = host
            self._streetsmart_prelogin_panel = panel
            self.set_status("StreetSmart sessie wordt op de achtergrond voorbereid...")
        except Exception:
            self._destroy_streetsmart_prelogin_panel()
            return

    def _destroy_streetsmart_prelogin_panel(self) -> None:
        panel = getattr(self, "_streetsmart_prelogin_panel", None)
        self._streetsmart_prelogin_panel = None
        if panel is not None:
            try:
                panel.destroy()
            except Exception:
                pass
        host = getattr(self, "_streetsmart_prelogin_host", None)
        self._streetsmart_prelogin_host = None
        if host is not None:
            try:
                host.destroy()
            except Exception:
                pass

    def _set_streetsmart_panel_visible_with_prelogin(self, visible, *args, **kwargs):
        if visible:
            self._destroy_streetsmart_prelogin_panel()
        return original_set_streetsmart_panel_visible(self, visible, *args, **kwargs)

    def streetsmart_logout_with_prelogin(self):
        self._destroy_streetsmart_prelogin_panel()
        return original_streetsmart_logout(self)

    def _build_streetsmart_year_selector(self) -> None:
        if getattr(self, "_streetsmart_year_selector_items", None) is not None:
            return
        canvas = getattr(self, "canvas", None)
        if canvas is None:
            return
        try:
            configure_combo_style = getattr(self, "_configure_modern_combobox_style", None)
            if callable(configure_combo_style):
                configure_combo_style()
        except Exception:
            pass
        label = tk.Label(canvas, text="StreetSmart jaar", background="#ffffff", foreground="#111827", font=("Segoe UI", 9))
        combo = ttk.Combobox(
            canvas,
            textvariable=self.streetsmart_recording_year_var,
            state="readonly",
            width=10,
            values=[STREETSMART_LATEST_YEAR_OPTION],
            style="RoundedField.TCombobox",
        )
        close_button = tk.Button(
            canvas,
            text="x",
            command=lambda: self._turn_off_streetsmart_selector(status_text="Streetview selecter uit."),
            style="Compact.TButton",
            width=2,
        )
        background_item = canvas.create_polygon(0, 0, 1, 1, smooth=True, splinesteps=12, fill="#ffffff", outline="#dde4ee", state="hidden")
        label_window = canvas.create_window(0, 0, anchor="nw", window=label, state="hidden")
        combo_window = canvas.create_window(0, 0, anchor="nw", window=combo, state="hidden")
        close_window = canvas.create_window(0, 0, anchor="nw", window=close_button, state="hidden")
        self._streetsmart_year_selector_items = {
            "background": background_item,
            "label": label_window,
            "combo": combo_window,
            "close": close_window,
        }
        self._streetsmart_year_selector_combo = combo
        canvas.bind("<Configure>", lambda _event: self._layout_streetsmart_year_selector(), add="+")
        self.streetsmart_recording_year_var.trace_add("write", lambda *_args: self._on_streetsmart_year_changed())

    def _streetsmart_year_selector_rect_points(x1: int, y1: int, x2: int, y2: int, radius: int) -> list[int]:
        radius = max(1, min(radius, (x2 - x1) // 2, (y2 - y1) // 2))
        return [
            x1 + radius, y1, x2 - radius, y1, x2, y1, x2, y1 + radius,
            x2, y2 - radius, x2, y2, x2 - radius, y2, x1 + radius, y2,
            x1, y2, x1, y2 - radius, x1, y1 + radius, x1, y1,
        ]

    def _layout_streetsmart_year_selector(self) -> None:
        canvas = getattr(self, "canvas", None)
        items = getattr(self, "_streetsmart_year_selector_items", None)
        if canvas is None or not items:
            return
        width = max(1, canvas.winfo_width())
        height = max(1, canvas.winfo_height())
        bar_width = min(360, max(300, width - 40))
        bar_height = 42
        x1 = int((width - bar_width) / 2)
        y1 = max(12, height - bar_height - 18)
        x2 = x1 + bar_width
        y2 = y1 + bar_height
        canvas.coords(items["background"], *_streetsmart_year_selector_rect_points(x1, y1, x2, y2, 14))
        canvas.coords(items["label"], x1 + 14, y1 + 11)
        canvas.itemconfigure(items["label"], width=108, height=22)
        canvas.coords(items["combo"], x1 + 126, y1 + 7)
        canvas.itemconfigure(items["combo"], width=174, height=28)
        canvas.coords(items["close"], x2 - 42, y1 + 7)
        canvas.itemconfigure(items["close"], width=30, height=28)

    def _refresh_streetsmart_year_selector(self) -> None:
        self._build_streetsmart_year_selector()
        canvas = getattr(self, "canvas", None)
        items = getattr(self, "_streetsmart_year_selector_items", None)
        combo = getattr(self, "_streetsmart_year_selector_combo", None)
        if canvas is None or not items or combo is None:
            return
        years = list(getattr(self, "_streetsmart_recording_years", []) or [])
        values = [STREETSMART_LATEST_YEAR_OPTION] + [year for year in years if year != STREETSMART_LATEST_YEAR_OPTION]
        combo.configure(values=values)
        if self._selected_streetsmart_recording_year() not in values:
            self.streetsmart_recording_year_var.set(STREETSMART_LATEST_YEAR_OPTION)
        visible = bool(getattr(self, "streetsmart_selector_enabled", False)) and self._has_streetsmart_selector_zoom()
        for item in items.values():
            canvas.itemconfigure(item, state="normal" if visible else "hidden")
        if visible:
            self._layout_streetsmart_year_selector()
            for item in items.values():
                canvas.tag_raise(item)

    def _selected_streetsmart_recording_year(self) -> str:
        return str(self.streetsmart_recording_year_var.get() or "").strip()

    def _latest_streetsmart_recording_year(self) -> str:
        years = list(getattr(self, "_streetsmart_recording_years", []) or [])
        return years[0] if years else ""

    def _apply_streetsmart_recording_year_filter(self) -> None:
        selected_year = self._selected_streetsmart_recording_year()
        if selected_year == STREETSMART_LATEST_YEAR_OPTION:
            selected_year = self._latest_streetsmart_recording_year()
        all_markers = list(getattr(self, "_streetsmart_recording_all_markers", []) or [])
        all_payloads = dict(getattr(self, "_streetsmart_recording_all_payloads", {}) or {})
        if selected_year:
            markers = [marker for marker in all_markers if str(all_payloads.get(marker.marker_id, {}).get("year") or "") == selected_year]
        else:
            markers = all_markers
        self._streetsmart_recording_markers = markers
        self._streetsmart_recording_payloads = {marker.marker_id: all_payloads.get(marker.marker_id) for marker in markers}
        self._streetsmart_recording_payloads = {key: value for key, value in self._streetsmart_recording_payloads.items() if value is not None}

    def _on_streetsmart_year_changed(self) -> None:
        if not bool(getattr(self, "streetsmart_selector_enabled", False)):
            return
        self._apply_streetsmart_recording_year_filter()
        self._refresh_streetsmart_year_selector()
        self.request_render(False)

    def _streetsmart_recordings_request_key(self):
        if not bool(getattr(self, "streetsmart_selector_enabled", False)):
            return None
        if not self._has_streetsmart_selector_zoom():
            return None
        if not self._has_streetsmart_credentials():
            return None
        bounds = self._current_view_bounds()
        padding = max(float(getattr(self, "meters_per_pixel", 1.0)) * 120.0, 25.0)
        min_x = bounds.min_x - padding
        min_y = bounds.min_y - padding
        max_x = bounds.max_x + padding
        max_y = bounds.max_y + padding
        tile = max(float(getattr(self, "meters_per_pixel", 1.0)) * 256.0, 50.0)
        return (
            round(min_x / tile),
            round(min_y / tile),
            round(max_x / tile),
            round(max_y / tile),
            round(tile, 3),
        )

    def _streetsmart_recordings_bbox_for_key(self, key):
        tile = float(key[4])
        return (
            float(key[0]) * tile,
            float(key[1]) * tile,
            float(key[2]) * tile,
            float(key[3]) * tile,
        )

    def _schedule_streetsmart_recordings_fetch(self) -> None:
        key = self._streetsmart_recordings_request_key()
        if key is None:
            self._streetsmart_recording_all_markers = []
            self._streetsmart_recording_all_payloads = {}
            self._streetsmart_recording_markers = []
            self._streetsmart_recording_payloads = {}
            self._streetsmart_recording_years = []
            self._refresh_streetsmart_year_selector()
            return
        if key == getattr(self, "_streetsmart_recording_request_key", None):
            return
        if key == getattr(self, "_streetsmart_recording_inflight_key", None):
            return
        self._streetsmart_recording_inflight_key = key
        bbox = self._streetsmart_recordings_bbox_for_key(key)
        username = str(self.settings.streetsmart_username or "").strip()
        password = str(self.settings.streetsmart_password or "")

        def worker() -> None:
            markers = []
            payloads = {}
            years = []
            error_text = None
            try:
                markers, payloads, years = self._fetch_streetsmart_recordings(bbox, username, password)
            except Exception as exc:
                error_text = str(exc)
            self._streetsmart_recording_pending = (key, markers, payloads, years, error_text)

        threading.Thread(target=worker, daemon=True).start()
        if not getattr(self, "_streetsmart_recording_polling", False):
            self._streetsmart_recording_polling = True
            self.after(100, self._poll_streetsmart_recordings)

    def _poll_streetsmart_recordings(self) -> None:
        pending = getattr(self, "_streetsmart_recording_pending", None)
        if pending is None:
            if getattr(self, "_streetsmart_recording_inflight_key", None) is not None:
                self.after(100, self._poll_streetsmart_recordings)
                return
            self._streetsmart_recording_polling = False
            return
        self._streetsmart_recording_pending = None
        key, markers, payloads, years, error_text = pending
        self._streetsmart_recording_inflight_key = None
        if key == self._streetsmart_recordings_request_key():
            self._streetsmart_recording_request_key = key
            self._streetsmart_recording_all_markers = markers
            self._streetsmart_recording_all_payloads = payloads
            self._streetsmart_recording_years = years
            selected_year = self._selected_streetsmart_recording_year()
            available_year_values = [STREETSMART_LATEST_YEAR_OPTION] + years
            if selected_year not in available_year_values:
                self.streetsmart_recording_year_var.set(STREETSMART_LATEST_YEAR_OPTION)
            elif not years:
                self.streetsmart_recording_year_var.set(STREETSMART_LATEST_YEAR_OPTION)
            self._apply_streetsmart_recording_year_filter()
            self._refresh_streetsmart_year_selector()
            if error_text:
                self.set_status("StreetSmart-opnamepunten laden mislukt.")
            else:
                selected_label = self._selected_streetsmart_recording_year()
                selected_year_text = self._latest_streetsmart_recording_year() if selected_label == STREETSMART_LATEST_YEAR_OPTION else selected_label
                year_suffix = f" voor {selected_label} ({selected_year_text})" if selected_label == STREETSMART_LATEST_YEAR_OPTION and selected_year_text else f" voor {selected_label}" if selected_label else ""
                self.set_status(f"{len(self._streetsmart_recording_markers)} StreetSmart-opnamepunt(en){year_suffix} geladen.")
            self.request_render(False)
        self._streetsmart_recording_polling = False

    def _fetch_streetsmart_recordings(self, bbox, username: str, password: str):
        import requests

        min_x, min_y, max_x, max_y = bbox
        params = {
            "service": "WFS",
            "version": "1.1.0",
            "request": "GetFeature",
            "typename": "atlas:Recording",
            "srsName": "EPSG:28992",
            "bbox": f"{min_x:.2f},{min_y:.2f},{max_x:.2f},{max_y:.2f},EPSG:28992",
            "outputFormat": "application/json",
            "maxFeatures": STREETSMART_RECORDING_MAX_FEATURES,
        }
        data = None
        last_error = None
        sort_variants = ("recordedAt+D", "recordedAt D", "year+D", None)
        for sort_variant in sort_variants:
            request_params = dict(params)
            if sort_variant:
                request_params["sortBy"] = sort_variant
            try:
                response = requests.get(
                    STREETSMART_RECORDING_WFS_URL,
                    params=request_params,
                    auth=(username, password),
                    timeout=18,
                    headers={"Accept": "application/json"},
                )
                response.raise_for_status()
                candidate_data = response.json()
                if isinstance(candidate_data, dict) and candidate_data.get("exception") and sort_variant:
                    last_error = RuntimeError(str(candidate_data["exception"]))
                    continue
                data = candidate_data
                break
            except requests.RequestException as exc:
                last_error = exc
                if not sort_variant:
                    raise
        if data is None:
            raise RuntimeError(str(last_error) if last_error else "StreetSmart-opnamepunten laden mislukt.")
        if isinstance(data, dict) and data.get("exception"):
            raise RuntimeError(str(data["exception"]))
        markers = []
        payloads = {}
        years_seen = set()
        features = data.get("features") if isinstance(data, dict) else None
        if not isinstance(features, list):
            return markers, payloads, []
        def _feature_recorded_sort_key(feature_data) -> tuple[int, str]:
            properties_data = feature_data.get("properties") if isinstance(feature_data, dict) and isinstance(feature_data.get("properties"), dict) else {}
            recorded_text = str(properties_data.get("recordedAt") or "")
            year_text = str(properties_data.get("year") or recorded_text[:4] or "")
            try:
                year_number = int(year_text[:4])
            except (TypeError, ValueError):
                year_number = 0
            return year_number, recorded_text

        features = sorted(features, key=_feature_recorded_sort_key, reverse=True)
        for index, feature in enumerate(features):
            if not isinstance(feature, dict):
                continue
            geometry = feature.get("geometry") or {}
            coordinates = geometry.get("coordinates") if isinstance(geometry, dict) else None
            if not isinstance(coordinates, (list, tuple)) or len(coordinates) < 2:
                continue
            try:
                x_coord = float(coordinates[0])
                y_coord = float(coordinates[1])
            except (TypeError, ValueError):
                continue
            if -180.0 <= x_coord <= 180.0 and -90.0 <= y_coord <= 90.0:
                try:
                    from pyproj import Transformer

                    transformer = getattr(self, "_streetsmart_wgs84_to_rd_transformer", None)
                    if transformer is None:
                        transformer = Transformer.from_crs("EPSG:4326", "EPSG:28992", always_xy=True)
                        self._streetsmart_wgs84_to_rd_transformer = transformer
                    x_coord, y_coord = transformer.transform(x_coord, y_coord)
                except Exception:
                    continue
            properties = feature.get("properties") if isinstance(feature.get("properties"), dict) else {}
            image_id = str(properties.get("imageId") or feature.get("id") or f"recording-{index}")
            recorded_at = properties.get("recordedAt")
            year_value = properties.get("year")
            year_text = str(year_value or "").strip()
            if not year_text and recorded_at:
                year_text = str(recorded_at).strip()[:4]
            if year_text and re.fullmatch(r"\d{4}", year_text):
                years_seen.add(year_text)
            else:
                year_text = ""
            marker_id = f"streetsmart-recording:{image_id}:{index}"
            marker = MapMarker(
                marker_id=marker_id,
                x=x_coord,
                y=y_coord,
                fill_color=(132, 204, 22),
                outline_color=(22, 101, 52),
                radius_px=STREETSMART_RECORDING_RADIUS_PX,
            )
            markers.append(marker)
            payloads[marker_id] = {
                "name": image_id,
                "imageId": image_id,
                "center": [float(x_coord), float(y_coord)],
                "recordedAt": recorded_at,
                "year": year_text,
            }
        years = sorted(years_seen, key=lambda value: int(value), reverse=True)
        return markers, payloads, years

    def _active_map_markers_with_streetsmart(self):
        markers = list(original_active_map_markers(self) or [])
        if bool(getattr(self, "streetsmart_selector_enabled", False)) and self._has_streetsmart_selector_zoom():
            markers.extend(getattr(self, "_streetsmart_recording_markers", []) or [])
        return markers

    def _find_streetsmart_recording_near_screen(self, screen_x: float, screen_y: float):
        if not bool(getattr(self, "streetsmart_selector_enabled", False)) or not self._has_streetsmart_selector_zoom():
            return None
        markers = getattr(self, "_streetsmart_recording_markers", []) or []
        payloads = getattr(self, "_streetsmart_recording_payloads", {}) or {}
        if not markers:
            return None
        bounds = self._current_view_bounds()
        width, height = self._canvas_size()
        best = None
        best_distance = 1.0e9
        for marker in markers:
            if not bounds.contains(marker.x, marker.y):
                continue
            sx = ((marker.x - bounds.min_x) / max(bounds.width, 1e-9)) * width
            sy = height - ((marker.y - bounds.min_y) / max(bounds.height, 1e-9)) * height
            distance = ((sx - screen_x) ** 2 + (sy - screen_y) ** 2) ** 0.5
            if distance <= max(marker.radius_px + 6, 12) and distance < best_distance:
                best = payloads.get(marker.marker_id)
                best_distance = distance
        return best

    def _open_streetsmart_recording_selection(self, selection: dict) -> None:
        if not isinstance(selection, dict):
            return
        try:
            save_streetsmart_state(selection)
        except OSError:
            pass
        self._prepare_streetsmart_panel_session()
        self._set_streetsmart_panel_visible(True, sync_selection=False)
        self._sync_streetsmart_panel(selection, reload_view=True)
        self._schedule_streetsmart_selection_navigation(selection)
        self.set_status(f"StreetSmart geopend op opnamepunt {selection.get('name', '')}.")

    def _schedule_streetsmart_selection_navigation(self, selection: dict) -> None:
        if not isinstance(selection, dict):
            return

        def navigate_once() -> None:
            panel = getattr(self, "streetsmart_panel", None)
            if panel is None:
                return
            try:
                panel.update_credentials(
                    str(self.settings.streetsmart_username or "").strip(),
                    str(self.settings.streetsmart_password or ""),
                )
                panel.navigate_selection(selection)
            except Exception:
                pass

        navigate_once()
        for delay_ms in (700, 1800, 3200, 5200, 7600):
            try:
                self.after(delay_ms, navigate_once)
            except Exception:
                break

    def _on_canvas_release_with_streetsmart_selector(self, event):
        if getattr(self, "click_origin", None) is not None:
            try:
                if abs(event.x - self.click_origin[0]) <= 5 and abs(event.y - self.click_origin[1]) <= 5:
                    selection = self._find_streetsmart_recording_near_screen(float(event.x), float(event.y))
                    if selection is not None:
                        self._open_streetsmart_recording_selection(selection)
                        self.click_origin = None
                        return
            except Exception:
                pass
        return original_on_canvas_release(self, event)

    def _on_canvas_move_with_streetsmart_selector(self, event):
        result = original_on_canvas_move(self, event)
        try:
            selection = self._find_streetsmart_recording_near_screen(float(event.x), float(event.y))
            if selection is not None:
                self.canvas.configure(cursor="hand2")
        except Exception:
            pass
        return result

    def _request_render_with_streetsmart_selector(self, *args, **kwargs):
        result = original_request_render(self, *args, **kwargs)
        if bool(getattr(self, "streetsmart_selector_enabled", False)):
            if self._has_streetsmart_selector_zoom():
                self._schedule_streetsmart_recordings_fetch()
            else:
                self._streetsmart_recording_all_markers = []
                self._streetsmart_recording_all_payloads = {}
                self._streetsmart_recording_markers = []
                self._streetsmart_recording_payloads = {}
                self._streetsmart_recording_years = []
                self._refresh_streetsmart_year_selector()
        return result

    viewer_class.__init__ = _init_with_streetsmart_selector
    viewer_class._build_menu = _build_menu_with_streetsmart_selector
    viewer_class.request_render = _request_render_with_streetsmart_selector
    viewer_class._active_map_markers = _active_map_markers_with_streetsmart
    viewer_class._on_canvas_release = _on_canvas_release_with_streetsmart_selector
    viewer_class._on_canvas_move = _on_canvas_move_with_streetsmart_selector
    viewer_class._set_streetsmart_panel_visible = _set_streetsmart_panel_visible_with_prelogin
    viewer_class.streetsmart_logout = streetsmart_logout_with_prelogin
    viewer_class._has_streetsmart_selector_zoom = _has_streetsmart_selector_zoom
    viewer_class._streetview_selector_label = _streetview_selector_label
    viewer_class._update_streetsmart_selector_menu_label = _update_streetsmart_selector_menu_label
    viewer_class.toggle_streetsmart_selector = toggle_streetsmart_selector
    viewer_class._clear_streetsmart_recording_state = _clear_streetsmart_recording_state
    viewer_class._turn_off_streetsmart_selector = _turn_off_streetsmart_selector
    viewer_class._prepare_streetsmart_panel_session = _prepare_streetsmart_panel_session
    viewer_class._streetsmart_prepare_session_on_startup = _streetsmart_prepare_session_on_startup
    viewer_class._streetsmart_auto_login_on_startup = _streetsmart_auto_login_on_startup
    viewer_class._destroy_streetsmart_prelogin_panel = _destroy_streetsmart_prelogin_panel
    viewer_class._build_streetsmart_year_selector = _build_streetsmart_year_selector
    viewer_class._layout_streetsmart_year_selector = _layout_streetsmart_year_selector
    viewer_class._refresh_streetsmart_year_selector = _refresh_streetsmart_year_selector
    viewer_class._selected_streetsmart_recording_year = _selected_streetsmart_recording_year
    viewer_class._latest_streetsmart_recording_year = _latest_streetsmart_recording_year
    viewer_class._apply_streetsmart_recording_year_filter = _apply_streetsmart_recording_year_filter
    viewer_class._on_streetsmart_year_changed = _on_streetsmart_year_changed
    viewer_class._streetsmart_recordings_request_key = _streetsmart_recordings_request_key
    viewer_class._streetsmart_recordings_bbox_for_key = _streetsmart_recordings_bbox_for_key
    viewer_class._schedule_streetsmart_recordings_fetch = _schedule_streetsmart_recordings_fetch
    viewer_class._poll_streetsmart_recordings = _poll_streetsmart_recordings
    viewer_class._fetch_streetsmart_recordings = _fetch_streetsmart_recordings
    viewer_class._find_streetsmart_recording_near_screen = _find_streetsmart_recording_near_screen
    viewer_class._open_streetsmart_recording_selection = _open_streetsmart_recording_selection
    viewer_class._schedule_streetsmart_selection_navigation = _schedule_streetsmart_selection_navigation
    viewer_class._streetsmart_selector_patch = True


def _install_template_variant_order_patch() -> None:
    viewer_class = globals().get("KlicViewerApp")
    if viewer_class is None or getattr(viewer_class, "_template_variant_order_patch", False):
        return

    original_normalize_slot_entries = viewer_class._normalize_template_export_slot_entries
    original_store_order_state = viewer_class._store_template_export_order_state
    variant_metadata_key = "template_export_variant"
    variant_code_metadata_key = "template_export_variant_code"
    ps_number_metadata_key = "template_export_ps_number"
    label_metadata_key = "template_proefsleuf_label"

    def _normalize_variant_code(value) -> str:
        if isinstance(value, bool):
            return "AUTO" if value else ""
        if value is None:
            return ""
        if isinstance(value, str):
            normalized = value.strip().upper()
            if normalized.casefold() in {"", "0", "false", "nee", "no", "off", "geen"}:
                return ""
            if normalized.casefold() in {"1", "true", "ja", "yes", "on", "auto", "automatisch"}:
                return "AUTO"
            if re.fullmatch(r"[A-Z]+", normalized):
                return normalized
        return "AUTO" if bool(value) else ""

    def _coerce_variant_flag(value) -> bool:
        return bool(_normalize_variant_code(value))

    def _normalize_ps_number(value) -> str:
        normalized = str(value or "").strip().upper()
        match = re.fullmatch(r"(?:PS\s*)?(\d+)", normalized)
        if match is None:
            return ""
        return str(int(match.group(1)))

    def _normalize_template_export_slot_entries_with_variants(self, value):
        entries = original_normalize_slot_entries(self, value)
        raw_entries = [item for item in value if isinstance(item, dict)] if isinstance(value, list) else []
        for entry, raw_entry in zip(entries, raw_entries):
            raw_code = raw_entry.get("variant_code", raw_entry.get("variant", False))
            variant_code = _normalize_variant_code(raw_code)
            entry["variant"] = bool(variant_code)
            entry["variant_code"] = variant_code
            ps_number = _normalize_ps_number(raw_entry.get("ps_number", ""))
            if ps_number:
                entry["ps_number"] = ps_number
        return entries

    def _template_export_variant_suffix(position: int) -> str:
        value = max(1, int(position))
        letters = ""
        while value > 0:
            value, remainder = divmod(value - 1, 26)
            letters = chr(ord("A") + remainder) + letters
        return letters

    def _template_export_variant_labels(
        ordered_layers,
        included_flags,
        variant_codes,
        ps_numbers=None,
    ):
        labels: list[str | None] = [None] * len(ordered_layers)
        invalid_indices: list[int] = []
        if ps_numbers is not None:
            automatic_variant_positions: dict[str, int] = {}
            for index, (layer, included, raw_variant_code, raw_ps_number) in enumerate(
                zip(ordered_layers, included_flags, variant_codes, ps_numbers)
            ):
                if layer is None or not included:
                    continue
                ps_number = _normalize_ps_number(raw_ps_number)
                if not ps_number:
                    invalid_indices.append(index)
                    continue
                variant_code = _normalize_variant_code(raw_variant_code)
                if variant_code == "AUTO":
                    next_position = automatic_variant_positions.get(ps_number, 1) + 1
                    automatic_variant_positions[ps_number] = next_position
                    variant_code = _template_export_variant_suffix(next_position)
                labels[index] = f"PS{ps_number}{variant_code}"
            return labels, invalid_indices

        next_number = 1
        current_number: int | None = None
        variant_position = 1
        for index, (layer, included, raw_variant_code) in enumerate(
            zip(ordered_layers, included_flags, variant_codes)
        ):
            if layer is None or not included:
                continue
            variant_code = _normalize_variant_code(raw_variant_code)
            if variant_code:
                if current_number is None:
                    invalid_indices.append(index)
                    continue
                if variant_code == "AUTO":
                    variant_position += 1
                    variant_code = _template_export_variant_suffix(variant_position)
                labels[index] = f"PS{current_number}{variant_code}"
                continue
            current_number = next_number
            next_number += 1
            variant_position = 1
            labels[index] = f"PS{current_number}"
        return labels, invalid_indices

    def _default_template_export_ps_number(self, layer, fallback_index: int) -> str:
        base_name = str(self.cadastral_exporter._proefsleuf_base_name(layer, fallback_index) or "")
        match = re.search(r"(?i)PS\s*(\d+)", base_name)
        if match is not None:
            return str(int(match.group(1)))
        digits = re.search(r"(\d+)", str(getattr(layer, "path", "")))
        if digits is not None:
            return str(int(digits.group(1)))
        return str(int(fallback_index))

    def _template_export_ps_numbers_for_order(self, ordered_layers) -> list[str]:
        numbers_by_key: dict[str, list[str]] = {}
        for entry in getattr(self, "template_export_slot_order", []):
            if not isinstance(entry, dict):
                continue
            layer_key = entry.get("layer_key")
            ps_number = _normalize_ps_number(entry.get("ps_number", ""))
            if layer_key is None or not ps_number:
                continue
            numbers_by_key.setdefault(str(layer_key), []).append(ps_number)

        numbers: list[str] = []
        for fallback_index, layer in enumerate(ordered_layers, start=1):
            if layer is None:
                numbers.append("")
                continue
            layer_key = str(self._template_export_layer_key(layer))
            saved_numbers = numbers_by_key.get(layer_key, [])
            if saved_numbers:
                numbers.append(saved_numbers.pop(0))
                continue
            metadata_number = _normalize_ps_number(layer.metadata.get(ps_number_metadata_key, ""))
            numbers.append(
                metadata_number or self._default_template_export_ps_number(layer, fallback_index)
            )
        return numbers

    def _template_export_variant_codes_for_order(self, ordered_layers) -> list[str]:
        variants_by_key: dict[str, list[str]] = {}
        for entry in getattr(self, "template_export_slot_order", []):
            if not isinstance(entry, dict):
                continue
            layer_key = entry.get("layer_key")
            if layer_key is None:
                continue
            variants_by_key.setdefault(str(layer_key), []).append(
                _normalize_variant_code(entry.get("variant_code", entry.get("variant", False)))
            )

        codes: list[str] = []
        for layer in ordered_layers:
            if layer is None:
                codes.append("")
                continue
            layer_key = str(self._template_export_layer_key(layer))
            saved_codes = variants_by_key.get(layer_key, [])
            if saved_codes:
                codes.append(_normalize_variant_code(saved_codes.pop(0)))
            else:
                metadata_code = _normalize_variant_code(
                    layer.metadata.get(
                        variant_code_metadata_key,
                        layer.metadata.get(variant_metadata_key, False),
                    )
                )
                filename_match = re.search(
                    r"(?i)(?:^|[^A-Z0-9])ps[\s._-]*\d+([A-Z]{1,2})\b",
                    str(getattr(getattr(layer, "path", None), "stem", "")),
                )
                filename_code = _normalize_variant_code(filename_match.group(1)) if filename_match else ""
                codes.append(metadata_code or filename_code)
        return codes

    def _template_export_variant_flags_for_order(self, ordered_layers) -> list[bool]:
        return [bool(code) for code in self._template_export_variant_codes_for_order(ordered_layers)]

    def _store_template_export_variant_state(
        self,
        ordered_layers,
        included_flags,
        scale_denominators,
        ps_numbers,
        variant_codes,
        labels,
    ) -> None:
        original_store_order_state(self, ordered_layers, included_flags, scale_denominators)
        saved_entries = getattr(self, "template_export_slot_order", [])
        for index, (layer, included, raw_ps_number, raw_variant_code, label) in enumerate(
            zip(ordered_layers, included_flags, ps_numbers, variant_codes, labels)
        ):
            normalized_number = _normalize_ps_number(raw_ps_number) if layer is not None else ""
            normalized_code = _normalize_variant_code(raw_variant_code) if layer is not None else ""
            if index < len(saved_entries) and isinstance(saved_entries[index], dict):
                saved_entries[index]["ps_number"] = normalized_number
                saved_entries[index]["variant"] = bool(normalized_code)
                saved_entries[index]["variant_code"] = normalized_code
            if layer is None:
                continue
            layer.metadata[ps_number_metadata_key] = normalized_number
            layer.metadata[variant_metadata_key] = bool(normalized_code)
            if normalized_code:
                layer.metadata[variant_code_metadata_key] = normalized_code
            else:
                layer.metadata.pop(variant_code_metadata_key, None)
            if included and label:
                layer.metadata[label_metadata_key] = label
            else:
                layer.metadata.pop(label_metadata_key, None)

    def _choose_template_export_order_with_variants(self):
        ordered_layers, included_flags, scale_denominators = self._initial_template_export_order_state()
        if not ordered_layers:
            return []
        ps_numbers = self._template_export_ps_numbers_for_order(ordered_layers)
        variant_codes = self._template_export_variant_codes_for_order(ordered_layers)

        dialog = tk.Toplevel(self)
        dialog.title("Volgorde proefsleuven")
        dialog.transient(self)
        dialog.grab_set()
        dialog.resizable(False, False)
        dialog.configure(background=self.APP_BG)

        frame = ttk.Frame(dialog, padding=18, style="Card.TFrame")
        frame.grid(row=0, column=0, sticky="nsew")
        frame.columnconfigure(0, weight=1)
        frame.rowconfigure(2, weight=1)

        ttk.Label(
            frame,
            text="Kies de volgorde voor het sjabloon",
            font=("Segoe UI", 13, "bold"),
        ).grid(row=0, column=0, sticky="w")
        ttk.Label(
            frame,
            text=(
                "Het PS-nummer komt standaard uit de GeoTIFF-bestandsnaam; dubbele nummers mogen. "
                "Selecteer een of meer regels om het PS-nummer en de variantcode handmatig te wijzigen."
            ),
            wraplength=660,
            justify="left",
        ).grid(row=1, column=0, sticky="w", pady=(6, 12))

        content = ttk.Frame(frame, style="Card.TFrame")
        content.grid(row=2, column=0, sticky="nsew")
        content.columnconfigure(0, weight=1)
        content.columnconfigure(1, weight=0)
        content.rowconfigure(0, weight=1)

        listbox = tk.Listbox(
            content,
            activestyle="none",
            height=min(18, max(8, len(ordered_layers) + 1)),
            width=78,
            exportselection=False,
            selectmode=tk.EXTENDED,
            font=("Segoe UI", 10),
            background=self.INPUT_BG,
            foreground=self.TEXT,
            borderwidth=1,
            highlightbackground=self.BORDER,
            selectbackground=self.ACCENT,
            highlightcolor=self.ACCENT,
            selectforeground="#ffffff",
        )
        listbox.grid(row=0, column=0, sticky="nsew")
        scrollbar = ttk.Scrollbar(content, orient="vertical", command=listbox.yview)
        scrollbar.grid(row=0, column=1, sticky="ns", padx=(8, 0))
        listbox.configure(yscrollcommand=scrollbar.set)

        button_col = ttk.Frame(content, style="Card.TFrame")
        button_col.grid(row=0, column=2, sticky="ns", padx=(12, 0))
        button_col.columnconfigure(0, weight=1)

        info_var = tk.StringVar(value="")
        ttk.Label(frame, textvariable=info_var, wraplength=660, justify="left").grid(
            row=3, column=0, sticky="w", pady=(10, 0)
        )

        scale_var = tk.StringVar(value="Auto")
        scale_options = ["Auto", *[f"1:{denominator}" for denominator in self.TEMPLATE_EXPORT_SCALE_OPTIONS]]
        ps_number_var = tk.StringVar(value="")
        variant_option_by_code = {
            "": "Geen variant",
            "AUTO": "Automatisch (B/C/D...)",
            **{chr(letter): f"Variant {chr(letter)}" for letter in range(ord("B"), ord("Z") + 1)},
        }
        variant_code_by_option = {label: code for code, label in variant_option_by_code.items()}
        variant_var = tk.StringVar(value=variant_option_by_code[""])
        variant_options = list(variant_option_by_code.values())
        result = {"layers": None}

        def selected_indices() -> list[int]:
            return sorted(
                {
                    int(index)
                    for index in listbox.curselection()
                    if 0 <= int(index) < len(ordered_layers)
                }
            )

        def sync_selection_controls() -> None:
            selection = [index for index in selected_indices() if ordered_layers[index] is not None]
            if not selection:
                scale_var.set("Auto")
                variant_var.set(variant_option_by_code[""])
                ps_number_var.set("")
                return
            selected_scales = {scale_denominators[index] for index in selection}
            if len(selected_scales) == 1:
                scale_var.set(self._format_template_export_scale_denominator(selected_scales.pop()))
            else:
                scale_var.set("Gemengd")
            selected_numbers = {_normalize_ps_number(ps_numbers[index]) for index in selection}
            if len(selected_numbers) == 1:
                ps_number_var.set(selected_numbers.pop())
            else:
                ps_number_var.set("Gemengd")
            selected_codes = {_normalize_variant_code(variant_codes[index]) for index in selection}
            if len(selected_codes) == 1:
                selected_code = selected_codes.pop()
                variant_var.set(
                    variant_option_by_code.get(selected_code, f"Variant {selected_code}")
                )
            else:
                variant_var.set("Gemengd")

        def refresh_listbox(selected=None) -> None:
            listbox.delete(0, tk.END)
            labels, invalid_indices = self._template_export_variant_labels(
                ordered_layers, included_flags, variant_codes, ps_numbers
            )
            for index, (layer, included, scale, raw_ps_number, raw_variant_code) in enumerate(
                zip(ordered_layers, included_flags, scale_denominators, ps_numbers, variant_codes), start=1
            ):
                scale_text = self._format_template_export_scale_denominator(scale)
                item_index = index - 1
                variant_code = _normalize_variant_code(raw_variant_code)
                variant_text = f" [Variant {variant_code}]" if variant_code else ""
                if layer is None:
                    row_text = f"{index:02d}. [Leeg vak]"
                elif not included:
                    row_text = f"{index:02d}. [Niet meenemen]{variant_text} [{scale_text}] {layer.path.name}"
                else:
                    preview_label = labels[item_index] or "ONGELDIG PS-NUMMER"
                    row_text = f"{index:02d}. [{preview_label}]{variant_text} [{scale_text}] {layer.path.name}"
                listbox.insert(tk.END, row_text)

            included_count = sum(
                1 for layer, included in zip(ordered_layers, included_flags) if layer is not None and included
            )
            excluded_count = sum(
                1 for layer, included in zip(ordered_layers, included_flags) if layer is not None and not included
            )
            empty_count = sum(1 for layer in ordered_layers if layer is None)
            variant_count = sum(
                1
                for layer, included, variant_code in zip(ordered_layers, included_flags, variant_codes)
                if layer is not None and included and _normalize_variant_code(variant_code)
            )
            invalid_text = " | Ongeldig PS-nummer" if invalid_indices else ""
            info_var.set(
                f"Meenemen: {included_count} | Varianten: {variant_count} | "
                f"Niet meenemen: {excluded_count} | Lege vakken: {empty_count}{invalid_text}"
            )

            if not ordered_layers:
                return
            if selected is None:
                indices_to_select = [0]
            elif isinstance(selected, int):
                indices_to_select = [selected]
            else:
                indices_to_select = list(selected)
            indices_to_select = sorted(
                {index for index in indices_to_select if 0 <= index < len(ordered_layers)}
            )
            if not indices_to_select:
                indices_to_select = [0]
            for index in indices_to_select:
                listbox.selection_set(index)
            listbox.activate(indices_to_select[0])
            listbox.see(indices_to_select[0])
            sync_selection_controls()

        def move_selected(offset: int) -> None:
            selection = set(selected_indices())
            if not selection or offset == 0:
                return

            def swap(left: int, right: int) -> None:
                for values in (ordered_layers, included_flags, scale_denominators, ps_numbers, variant_codes):
                    values[left], values[right] = values[right], values[left]

            if offset < 0:
                for index in range(1, len(ordered_layers)):
                    if index in selection and (index - 1) not in selection:
                        swap(index - 1, index)
                        selection.remove(index)
                        selection.add(index - 1)
            else:
                for index in range(len(ordered_layers) - 2, -1, -1):
                    if index in selection and (index + 1) not in selection:
                        swap(index, index + 1)
                        selection.remove(index)
                        selection.add(index + 1)
            refresh_listbox(sorted(selection))

        def insert_empty_slot() -> None:
            selection = selected_indices()
            insert_index = selection[0] if selection else len(ordered_layers)
            ordered_layers.insert(insert_index, None)
            included_flags.insert(insert_index, True)
            scale_denominators.insert(insert_index, None)
            ps_numbers.insert(insert_index, "")
            variant_codes.insert(insert_index, "")
            refresh_listbox(insert_index)

        def remove_empty_slot() -> None:
            selection = selected_indices()
            removable = [index for index in selection if ordered_layers[index] is None]
            if not removable:
                return
            first_removed = removable[0]
            for index in reversed(removable):
                ordered_layers.pop(index)
                included_flags.pop(index)
                scale_denominators.pop(index)
                ps_numbers.pop(index)
                variant_codes.pop(index)
            refresh_listbox(min(first_removed, max(0, len(ordered_layers) - 1)))

        def set_selected_included(included: bool) -> None:
            selection = selected_indices()
            for index in selection:
                if ordered_layers[index] is not None:
                    included_flags[index] = included
            refresh_listbox(selection)

        def set_selected_label() -> None:
            selection = selected_indices()
            if not selection:
                return
            number_text = str(ps_number_var.get() or "").strip()
            apply_number = number_text.casefold() != "gemengd"
            ps_number = _normalize_ps_number(number_text) if apply_number else ""
            if apply_number and not ps_number:
                messagebox.showerror(
                    "PS-nummer ongeldig",
                    "Vul een positief of nul PS-nummer in, bijvoorbeeld 5 of PS5.",
                    parent=dialog,
                )
                return
            selected_option = str(variant_var.get() or "").strip()
            apply_variant = selected_option.casefold() != "gemengd"
            variant_code = variant_code_by_option.get(selected_option) if apply_variant else ""
            if variant_code is None and selected_option.upper().startswith("VARIANT "):
                variant_code = _normalize_variant_code(selected_option[8:])
            elif variant_code is None:
                variant_code = _normalize_variant_code(selected_option)
            variant_code = _normalize_variant_code(variant_code)
            for index in selection:
                if ordered_layers[index] is not None and included_flags[index]:
                    if apply_number:
                        ps_numbers[index] = ps_number
                    if apply_variant:
                        variant_codes[index] = variant_code
            refresh_listbox(selection)

        def set_selected_scale() -> None:
            selection = selected_indices()
            if not selection:
                return
            try:
                scale = self._parse_template_export_scale_text(scale_var.get())
            except ValueError as exc:
                messagebox.showerror("Schaal ongeldig", str(exc), parent=dialog)
                return
            for index in selection:
                if ordered_layers[index] is not None:
                    scale_denominators[index] = scale
            refresh_listbox(selection)

        def confirm_order() -> None:
            labels, invalid_indices = self._template_export_variant_labels(
                ordered_layers, included_flags, variant_codes, ps_numbers
            )
            if invalid_indices:
                refresh_listbox(invalid_indices)
                messagebox.showerror(
                    "PS-nummer ongeldig",
                    "Vul voor iedere meegenomen proefsleuf een geldig PS-nummer in.",
                    parent=dialog,
                )
                return
            self._store_template_export_variant_state(
                ordered_layers,
                included_flags,
                scale_denominators,
                ps_numbers,
                variant_codes,
                labels,
            )
            result["layers"] = [
                layer
                for layer, included in zip(ordered_layers, included_flags)
                if layer is None or included
            ]
            dialog.destroy()

        def cancel_order() -> None:
            dialog.destroy()

        ttk.Button(
            button_col, text="Omhoog", style="Compact.TButton", command=lambda: move_selected(-1)
        ).grid(row=0, column=0, sticky="ew")
        ttk.Button(
            button_col, text="Omlaag", style="Compact.TButton", command=lambda: move_selected(1)
        ).grid(row=1, column=0, sticky="ew", pady=(8, 0))
        ttk.Button(
            button_col, text="Wel meenemen", style="Compact.TButton", command=lambda: set_selected_included(True)
        ).grid(row=2, column=0, sticky="ew", pady=(12, 0))
        ttk.Button(
            button_col, text="Niet meenemen", style="Compact.TButton", command=lambda: set_selected_included(False)
        ).grid(row=3, column=0, sticky="ew", pady=(8, 0))

        ttk.Label(button_col, text="PS-nummer").grid(row=4, column=0, sticky="w", pady=(12, 0))
        ps_number_entry = ttk.Entry(button_col, textvariable=ps_number_var, width=22)
        ps_number_entry.grid(row=5, column=0, sticky="ew", pady=(4, 0))

        ttk.Label(button_col, text="Variantcode").grid(row=6, column=0, sticky="w", pady=(8, 0))
        variant_combo = ttk.Combobox(
            button_col,
            textvariable=variant_var,
            values=variant_options,
            width=22,
        )
        variant_combo.grid(row=7, column=0, sticky="ew", pady=(4, 0))
        ttk.Button(
            button_col,
            text="Nummer en variant toepassen",
            style="Compact.TButton",
            command=set_selected_label,
        ).grid(row=8, column=0, sticky="ew", pady=(8, 0))

        ttk.Label(button_col, text="Schaal").grid(row=9, column=0, sticky="w", pady=(12, 0))
        scale_combo = ttk.Combobox(
            button_col, textvariable=scale_var, values=scale_options, width=16
        )
        scale_combo.grid(row=10, column=0, sticky="ew", pady=(4, 0))
        ttk.Button(
            button_col,
            text="Schaal toepassen",
            style="Compact.TButton",
            command=set_selected_scale,
        ).grid(row=11, column=0, sticky="ew", pady=(8, 0))
        ttk.Button(
            button_col,
            text="Leeg vak invoegen",
            style="Compact.TButton",
            command=insert_empty_slot,
        ).grid(row=12, column=0, sticky="ew", pady=(12, 0))
        ttk.Button(
            button_col,
            text="Leeg vak verwijderen",
            style="Compact.TButton",
            command=remove_empty_slot,
        ).grid(row=13, column=0, sticky="ew", pady=(8, 0))

        listbox.bind("<<ListboxSelect>>", lambda _event: sync_selection_controls())
        ps_number_entry.bind("<Return>", lambda _event: set_selected_label())
        variant_combo.bind("<Return>", lambda _event: set_selected_label())
        scale_combo.bind("<Return>", lambda _event: set_selected_scale())

        footer = ttk.Frame(frame, style="Card.TFrame")
        footer.grid(row=4, column=0, sticky="e", pady=(14, 0))
        ttk.Button(footer, text="Annuleren", command=cancel_order).grid(row=0, column=0, padx=(0, 8))
        ttk.Button(footer, text="Exporteren", command=confirm_order).grid(row=0, column=1)

        dialog.bind("<Escape>", lambda _event: cancel_order())
        dialog.protocol("WM_DELETE_WINDOW", cancel_order)
        refresh_listbox(0)
        dialog.update_idletasks()
        dialog.geometry(f"+{self.winfo_rootx() + 120}+{self.winfo_rooty() + 80}")
        self.wait_window(dialog)
        return result["layers"]

    viewer_class.TEMPLATE_EXPORT_VARIANT_METADATA_KEY = variant_metadata_key
    viewer_class.TEMPLATE_EXPORT_VARIANT_CODE_METADATA_KEY = variant_code_metadata_key
    viewer_class.TEMPLATE_EXPORT_PS_NUMBER_METADATA_KEY = ps_number_metadata_key
    viewer_class.TEMPLATE_PROEFSLEUF_LABEL_METADATA_KEY = label_metadata_key
    viewer_class._normalize_template_export_slot_entries = _normalize_template_export_slot_entries_with_variants
    viewer_class._template_export_variant_suffix = staticmethod(_template_export_variant_suffix)
    viewer_class._template_export_variant_labels = staticmethod(_template_export_variant_labels)
    viewer_class._default_template_export_ps_number = _default_template_export_ps_number
    viewer_class._template_export_ps_numbers_for_order = _template_export_ps_numbers_for_order
    viewer_class._template_export_variant_codes_for_order = _template_export_variant_codes_for_order
    viewer_class._template_export_variant_flags_for_order = _template_export_variant_flags_for_order
    viewer_class._store_template_export_variant_state = _store_template_export_variant_state
    viewer_class._choose_template_export_order = _choose_template_export_order_with_variants
    viewer_class._template_variant_order_patch = True


def _install_ai_maaiveld_patch() -> None:
    viewer_class = globals().get("KlicViewerApp")
    if viewer_class is None or getattr(viewer_class, "_ai_maaiveld_patch", False):
        return

    original_build_menu = viewer_class._build_menu

    def _ai_maaiveld_points_for_layer(self, layer):
        profile = self._build_maaiveld_profile_for_layer(layer)
        start_x = float(profile.start_point.x)
        start_y = float(profile.start_point.y)
        end_x = float(profile.end_point.x)
        end_y = float(profile.end_point.y)
        middle_x = (start_x + end_x) * 0.5
        middle_y = (start_y + end_y) * 0.5
        try:
            rotation_degrees = float(layer.metadata.get("rotation_degrees", 0.0) or 0.0)
        except (TypeError, ValueError):
            rotation_degrees = 0.0
        if abs(rotation_degrees) >= 1e-9:
            center_x, center_y = self._tiff_layer_center_world(layer)
            start_x, start_y = self._rotate_point_around_center(start_x, start_y, center_x, center_y, rotation_degrees)
            middle_x, middle_y = self._rotate_point_around_center(middle_x, middle_y, center_x, center_y, rotation_degrees)
            end_x, end_y = self._rotate_point_around_center(end_x, end_y, center_x, center_y, rotation_degrees)
        return [
            AiMaaiveldPoint("start", "Beginpunt", start_x, start_y),
            AiMaaiveldPoint("middle", "Middenpunt", middle_x, middle_y),
            AiMaaiveldPoint("end", "Eindpunt", end_x, end_y),
        ]

    def _apply_ai_maaiveld_result(self, layer, result) -> int:
        applied = 0
        for number_key, text_value in result.text_by_key.items():
            segment_key = number_key_to_segment_key(number_key)
            if segment_key is None:
                continue
            current = self._maaiveld_segment_metadata_for_layer(layer, segment_key)
            color_value = str(current.get("color", self.MAAIVELD_DEFAULT_COLOR))
            self._set_maaiveld_segment_metadata(layer, segment_key, str(text_value).strip() or "INVULLEN", color_value)
            applied += 1
        self._refresh_map_edit_markers()
        self.request_render(refetch_background=False)
        return applied

    def run_ai_maaiveld_for_selected_layer(self) -> None:
        layer = self._current_tiff_layer_for_maaiveld()
        if layer is None:
            messagebox.showinfo("Maaiveld via Gemma", "Kies eerst een proefsleuf.")
            return
        if getattr(self, "_ai_maaiveld_running", False):
            messagebox.showinfo("Maaiveld via Gemma", "Er loopt al een maaiveld-analyse.")
            return

        self._ai_maaiveld_running = True
        self.set_status(f"Maaiveld via Gemma voorbereiden voor {layer.name}...")

        def worker():
            try:
                points = self._ai_maaiveld_points_for_layer(layer)
                image = build_numbered_maaiveld_image(
                    layer=layer,
                    points=points,
                    renderer=self.renderer,
                    dxf_overlays=self._map_export_dxf_overlays(),
                    background_provider=self._map_export_background_provider(),
                )
                result = request_maaiveld_from_vllm(image)
            except Exception as exc:
                error = exc if isinstance(exc, AiMaaiveldError) else AiMaaiveldError(str(exc))

                def fail():
                    self._ai_maaiveld_running = False
                    messagebox.showerror("Maaiveld via Gemma mislukt", str(error))
                    self.set_status("Maaiveld via Gemma mislukt.")

                self.after(0, fail)
                return

            def succeed():
                self._ai_maaiveld_running = False
                applied = self._apply_ai_maaiveld_result(layer, result)
                self.set_status(f"Maaiveld via Gemma ingevuld voor {applied} punt(en) van {layer.name}.")
                messagebox.showinfo(
                    "Maaiveld via Gemma",
                    "Gemma heeft de maaiveldteksten ingevuld:\n"
                    + "\n".join(f"{key}: {value}" for key, value in result.text_by_key.items()),
                )

            self.after(0, succeed)

        threading.Thread(target=worker, name="ai-maaiveld-vllm", daemon=True).start()

    def _build_menu_with_ai_maaiveld(self) -> None:
        original_build_menu(self)
        try:
            menu_name = self.cget("menu")
            menu_bar = self.nametowidget(menu_name)
            end_index = menu_bar.index("end")
            if end_index is None:
                return
            for index in range(end_index + 1):
                if menu_bar.type(index) == "cascade" and menu_bar.entrycget(index, "label") == "AI":
                    menu_bar.delete(index)
                    break
            ai_menu = tk.Menu(menu_bar, tearoff=0)
            ai_menu.add_command(label="Maaiveld via Gemma", command=self.run_ai_maaiveld_for_selected_layer)
            insert_at = max(0, menu_bar.index("end") or 0)
            menu_bar.insert_cascade(insert_at, label="AI", menu=ai_menu)
            if hasattr(self, "_capture_modern_menu_specs"):
                self._modern_menu_specs = self._capture_modern_menu_specs(menu_bar)
                if getattr(self, "_modern_menu_bar", None) is not None:
                    self.after(0, self._build_modern_menu_bar)
        except Exception:
            return

    viewer_class._ai_maaiveld_points_for_layer = _ai_maaiveld_points_for_layer
    viewer_class._apply_ai_maaiveld_result = _apply_ai_maaiveld_result
    viewer_class.run_ai_maaiveld_for_selected_layer = run_ai_maaiveld_for_selected_layer
    viewer_class._build_menu = _build_menu_with_ai_maaiveld
    viewer_class._ai_maaiveld_patch = True


def _install_bgt_surface_autofill_patch() -> None:
    viewer_class = globals().get("KlicViewerApp")
    if viewer_class is None or getattr(viewer_class, "_bgt_surface_autofill_patch", False):
        return

    from .settings import (
        DEFAULT_TEMPLATE_AUTO_FILL_BGT_FYSIEK_VOORKOMEN,
        TEMPLATE_AUTO_FILL_BGT_FYSIEK_VOORKOMEN_KEY,
    )

    original_open_settings_dialog = viewer_class.open_settings_dialog
    original_export_template = viewer_class.export_cadastral_template_dxf

    def _widget_descendants(widget):
        for child in widget.winfo_children():
            yield child
            yield from _widget_descendants(child)

    def _widget_text(widget) -> str:
        try:
            return str(widget.cget("text") or "")
        except (AttributeError, tk.TclError):
            return ""

    def _setting_enabled(self) -> bool:
        return bool(
            getattr(
                self.settings,
                TEMPLATE_AUTO_FILL_BGT_FYSIEK_VOORKOMEN_KEY,
                DEFAULT_TEMPLATE_AUTO_FILL_BGT_FYSIEK_VOORKOMEN,
            )
        )

    def _open_settings_dialog_with_bgt_surface_autofill(self) -> None:
        existing_dialogs = {str(child) for child in self.winfo_children() if isinstance(child, tk.Toplevel)}
        original_open_settings_dialog(self)
        dialog = next(
            (
                child
                for child in self.winfo_children()
                if isinstance(child, tk.Toplevel)
                and str(child) not in existing_dialogs
                and child.title() == "Instellingen"
            ),
            None,
        )
        if dialog is None:
            return

        descendants = list(_widget_descendants(dialog))
        anchor = next(
            (
                widget
                for widget in descendants
                if _widget_text(widget)
                == "Gebruik kaartpunten voor maaiveldtekst en -kleur in sjabloonexport"
            ),
            None,
        )
        save_button = next(
            (
                widget
                for widget in descendants
                if _widget_text(widget) == "Opslaan"
            ),
            None,
        )
        if anchor is None or save_button is None:
            return

        content = anchor.master
        while content is not dialog:
            if any(_widget_text(widget) == "Proefsleuven-sjabloon" for widget in content.winfo_children()):
                break
            content = content.master
        if content is dialog:
            return
        occupied_rows: list[int] = []
        for widget in content.grid_slaves():
            try:
                occupied_rows.append(int(widget.grid_info().get("row", 0)))
            except (TypeError, ValueError, tk.TclError):
                continue
        row = (max(occupied_rows) + 1) if occupied_rows else 0
        option_var = tk.BooleanVar(value=_setting_enabled(self))
        ttk.Checkbutton(
            content,
            text="Vul de drie maaiveldvakken automatisch met BGT fysiek_voorkomen",
            variable=option_var,
        ).grid(row=row, column=0, sticky="w", pady=(12, 0))
        ttk.Label(
            content,
            text=(
                "Bepaalt de middelste tekst over de volledige proefsleuflijn. Voor links en rechts "
                "wordt de lijn aan beide uiteinden denkbeeldig 1 meter verlengd. Handmatig ingevulde "
                "teksten blijven behouden."
            ),
            wraplength=430,
            justify="left",
        ).grid(row=row + 1, column=0, sticky="w", pady=(0, 12))
        setattr(dialog, "_bgt_surface_autofill_var", option_var)

        original_save_command = save_button.cget("command")
        if original_save_command:
            def save_with_bgt_surface_option():
                setattr(
                    self.settings,
                    TEMPLATE_AUTO_FILL_BGT_FYSIEK_VOORKOMEN_KEY,
                    bool(option_var.get()),
                )
                if callable(original_save_command):
                    return original_save_command()
                return dialog.tk.call(str(original_save_command))

            save_button.configure(command=save_with_bgt_surface_option)

    def _export_template_with_bgt_surface_autofill(self) -> None:
        self.cadastral_exporter.auto_fill_bgt_fysiek_voorkomen = _setting_enabled(self)
        return original_export_template(self)

    viewer_class._bgt_surface_autofill_enabled = _setting_enabled
    viewer_class.open_settings_dialog = _open_settings_dialog_with_bgt_surface_autofill
    viewer_class.export_cadastral_template_dxf = _export_template_with_bgt_surface_autofill
    viewer_class._bgt_surface_autofill_patch = True


def _install_kickthemap_material_choices_patch() -> None:
    viewer_class = globals().get("KlicViewerApp")
    if viewer_class is None or getattr(viewer_class, "_kickthemap_material_choices_patch", False):
        return

    from .settings import (
        KICKTHEMAP_MATERIAL_CHOICES_KEY,
        normalize_kickthemap_material_choices,
    )

    original_open_settings_dialog = viewer_class.open_settings_dialog

    def _widget_descendants(widget):
        for child in widget.winfo_children():
            yield child
            yield from _widget_descendants(child)

    def _widget_text(widget) -> str:
        try:
            return str(widget.cget("text") or "")
        except (AttributeError, tk.TclError):
            return ""

    def _open_settings_dialog_with_material_choices(self) -> None:
        existing_dialogs = {str(child) for child in self.winfo_children() if isinstance(child, tk.Toplevel)}
        original_open_settings_dialog(self)
        dialog = next(
            (
                child
                for child in self.winfo_children()
                if isinstance(child, tk.Toplevel)
                and str(child) not in existing_dialogs
                and child.title() == "Instellingen"
            ),
            None,
        )
        if dialog is None:
            return

        descendants = list(_widget_descendants(dialog))
        rules_panel = next(
            (
                widget
                for widget in descendants
                if isinstance(widget, ttk.LabelFrame)
                and _widget_text(widget) == "KickTheMap woord-naar-laag"
            ),
            None,
        )
        save_button = next(
            (widget for widget in descendants if _widget_text(widget) == "Opslaan"),
            None,
        )
        if rules_panel is None or save_button is None:
            return

        rules_panel.configure(text="KickTheMap kabeltype, materiaal en DXF-laag")
        ttk.Separator(rules_panel, orient="horizontal").grid(
            row=4, column=0, sticky="ew", pady=(10, 8)
        )
        ttk.Label(
            rules_panel,
            text="Materiaalkeuzes in de KickTheMap-browser",
            font=("Segoe UI", 10, "bold"),
        ).grid(row=5, column=0, sticky="w")

        controls = ttk.Frame(rules_panel)
        controls.grid(row=6, column=0, sticky="ew", pady=(6, 5))
        controls.columnconfigure(0, weight=1)
        material_var = tk.StringVar()
        material_entry = ttk.Entry(controls, textvariable=material_var)
        material_entry.grid(row=0, column=0, sticky="ew")

        list_frame = ttk.Frame(rules_panel)
        list_frame.grid(row=7, column=0, sticky="nsew")
        list_frame.columnconfigure(0, weight=1)
        material_list = tk.Listbox(
            list_frame,
            height=4,
            exportselection=False,
            background=getattr(self, "INPUT_BG", "#fbfcfe"),
            foreground=getattr(self, "TEXT", "#111827"),
            selectbackground=getattr(self, "ACCENT", "#f97316"),
            selectforeground="#ffffff",
            borderwidth=0,
            highlightthickness=1,
            highlightbackground=getattr(self, "BORDER_SOFT", "#e5e7eb"),
            relief=tk.FLAT,
        )
        material_list.grid(row=0, column=0, sticky="nsew")
        material_scrollbar = ttk.Scrollbar(list_frame, orient="vertical", command=material_list.yview)
        material_scrollbar.grid(row=0, column=1, sticky="ns", padx=(6, 0))
        material_list.configure(yscrollcommand=material_scrollbar.set)

        for material in normalize_kickthemap_material_choices(
            getattr(self.settings, KICKTHEMAP_MATERIAL_CHOICES_KEY, [])
        ):
            material_list.insert(tk.END, material)

        def add_material() -> None:
            material = material_var.get().strip()
            if not material:
                material_entry.focus_set()
                return
            existing = {
                str(material_list.get(index)).strip().casefold()
                for index in range(material_list.size())
            }
            if material.casefold() not in existing:
                material_list.insert(tk.END, material)
                material_list.see(tk.END)
            material_var.set("")
            material_entry.focus_set()

        def remove_material() -> None:
            selection = material_list.curselection()
            if not selection:
                return
            material_list.delete(selection[0])

        ttk.Button(
            controls,
            text="Toevoegen",
            style="Compact.TButton",
            command=add_material,
        ).grid(row=0, column=1, padx=(8, 0))
        ttk.Button(
            list_frame,
            text="Verwijderen",
            style="Compact.TButton",
            command=remove_material,
        ).grid(row=0, column=2, padx=(8, 0), sticky="n")
        material_entry.bind("<Return>", lambda _event: add_material())
        material_list.bind("<Delete>", lambda _event: remove_material())
        setattr(dialog, "_kickthemap_material_list", material_list)

        original_save_command = save_button.cget("command")

        def save_with_material_choices():
            choices = [material_list.get(index) for index in range(material_list.size())]
            setattr(
                self.settings,
                KICKTHEMAP_MATERIAL_CHOICES_KEY,
                normalize_kickthemap_material_choices(choices),
            )
            if callable(original_save_command):
                return original_save_command()
            return dialog.tk.call(str(original_save_command))

        save_button.configure(command=save_with_material_choices)

    viewer_class.open_settings_dialog = _open_settings_dialog_with_material_choices
    viewer_class._kickthemap_material_choices_patch = True


def _install_fast_kickthemap_start_points_patch() -> None:
    viewer_class = globals().get("KlicViewerApp")
    if viewer_class is None or getattr(viewer_class, "_fast_kickthemap_start_points_patch", False):
        return

    original_load_local_dataset = viewer_class._load_local_maaiveld_dataset_for_layer
    original_load_dataset = viewer_class._load_maaiveld_dataset_for_layer

    class _PreloadedPathClient:
        def __init__(self, paths) -> None:
            self._paths = list(paths or [])

        def fetch_paths(self, _bounds):
            return list(self._paths)

    def _cached_local_maaiveld_dataset_for_layer(self, layer):
        if self._is_virtual_trench_layer(layer):
            return original_load_local_dataset(self, layer)

        features_path = self._local_kickthemap_job_features_path(layer)
        if features_path is None:
            return original_load_local_dataset(self, layer)
        try:
            file_stat = features_path.stat()
            resolved_path = str(features_path.resolve())
        except OSError:
            return original_load_local_dataset(self, layer)

        metadata = getattr(layer, "metadata", {})
        cache_key = (
            resolved_path,
            int(file_stat.st_mtime_ns),
            int(file_stat.st_size),
            self._kickthemap_job_id_for_layer(layer),
            str(metadata.get("kickthemap_job_title", "")),
            repr(metadata.get("template_cross_section_start_point")),
        )
        cache_lock = getattr(self, "_kickthemap_dataset_cache_lock", None)
        if cache_lock is None:
            cache_lock = threading.RLock()
            self._kickthemap_dataset_cache_lock = cache_lock
        with cache_lock:
            cache = getattr(self, "_kickthemap_dataset_cache", None)
            if cache is None:
                cache = {}
                self._kickthemap_dataset_cache = cache
            cached_dataset = cache.get(cache_key)
        if cached_dataset is not None:
            return cached_dataset

        dataset = original_load_local_dataset(self, layer)
        with cache_lock:
            cache[cache_key] = dataset
            if len(cache) > 128:
                for stale_key in list(cache)[: len(cache) - 96]:
                    cache.pop(stale_key, None)
        return dataset

    def _load_maaiveld_dataset_with_local_cache(self, layer):
        if not self._is_virtual_trench_layer(layer):
            try:
                if self._local_kickthemap_job_features_path(layer) is not None:
                    return self._load_local_maaiveld_dataset_for_layer(layer)
            except Exception:
                pass
        return original_load_dataset(self, layer)

    def _clone_path_client(base_client, client_type):
        if base_client is None or client_type is None:
            return None
        return client_type(timeout=base_client.timeout, page_size=base_client.page_size)

    def _load_all_kickthemap_start_points_fast(self) -> None:
        source_layers = [
            layer
            for layer in self.tiff_layers
            if not self._is_virtual_trench_layer(layer)
            and self._kickthemap_job_id_for_layer(layer) is not None
        ]
        if not source_layers:
            self.set_status("Geen geladen KickTheMap-proefsleuven om beginpunten voor te laden.")
            return

        warnings: list[str] = []
        ready_layers = []
        missing_layers = []
        for layer in source_layers:
            if self._local_kickthemap_job_features_path(layer) is None:
                missing_layers.append(layer)
            else:
                ready_layers.append(layer)

        if missing_layers:
            self.set_status(
                f"KickTheMap-objectpunten voorbereiden voor {len(missing_layers)} proefsleuf/proefsleuven..."
            )
            self.update_idletasks()
            if not self.kickthemap_client.is_logged_in and not self._try_auto_login_kickthemap():
                warnings.extend(
                    f"{layer.name}: log eerst in bij KickTheMap om objectpunten te laden."
                    for layer in missing_layers
                )
                missing_layers = []

        if missing_layers:
            jobs_fetch_failed = False
            try:
                jobs_by_id = {job.job_id: job for job in self.kickthemap_client.fetch_jobs()}
            except Exception as exc:
                warnings.extend(
                    f"{layer.name}: KickTheMap-jobs ophalen mislukt ({exc})."
                    for layer in missing_layers
                )
                jobs_by_id = {}
                jobs_fetch_failed = True

            layers_by_job: dict[int, list] = {}
            for layer in missing_layers:
                if jobs_fetch_failed:
                    continue
                job_id = self._kickthemap_job_id_for_layer(layer)
                if job_id is None:
                    continue
                if job_id not in jobs_by_id:
                    warnings.append(f"{layer.name}: KickTheMap-job {job_id} is niet gevonden.")
                    continue
                layers_by_job.setdefault(int(job_id), []).append(layer)

            jobs_to_download = [jobs_by_id[job_id] for job_id in layers_by_job]
            downloaded_paths: dict[int, Path] = {}
            download_errors: dict[int, Exception] = {}
            if jobs_to_download:
                def update_download_progress(completed: int, total: int) -> None:
                    self.set_status(f"KickTheMap-objectpunten downloaden ({completed}/{total})...")
                    self.update_idletasks()

                try:
                    downloaded_paths, download_errors = self.kickthemap_client.download_job_features_files(
                        jobs_to_download,
                        self.kickthemap_client.default_download_dir(),
                        max_workers=min(8, len(jobs_to_download)),
                        progress_callback=update_download_progress,
                    )
                except Exception as exc:
                    download_errors = {job.job_id: exc for job in jobs_to_download}

            for job_id, layers in layers_by_job.items():
                job = jobs_by_id[job_id]
                downloaded_path = downloaded_paths.get(job_id)
                if downloaded_path is None:
                    error = download_errors.get(job_id)
                    detail = f" ({error})" if error is not None else ""
                    warnings.extend(
                        f"{layer.name}: objectpunten konden niet worden geladen{detail}."
                        for layer in layers
                    )
                    continue
                for layer in layers:
                    layer.metadata.update(self._kickthemap_tiff_metadata(job, downloaded_path))
                    ready_layers.append(layer)

        if not ready_layers:
            if warnings:
                messagebox.showwarning("Beginpunten laden", "\n".join(warnings[:15]), parent=self)
            self.set_status("Er konden geen beginpunten worden geladen.")
            return

        datasets_by_layer: dict[int, object] = {}
        self.set_status(f"Objectpunten inlezen voor {len(ready_layers)} proefsleuf/proefsleuven...")
        self.update_idletasks()

        dataset_workers = max(1, min(8, len(ready_layers)))
        with ThreadPoolExecutor(max_workers=dataset_workers, thread_name_prefix="start-point-data") as executor:
            future_map = {
                executor.submit(self._load_local_maaiveld_dataset_for_layer, layer): layer
                for layer in ready_layers
            }
            for completed_index, future in enumerate(as_completed(future_map), start=1):
                layer = future_map[future]
                self.set_status(f"Objectpunten inlezen ({completed_index}/{len(ready_layers)}): {layer.name}")
                self.update_idletasks()
                try:
                    datasets_by_layer[id(layer)] = future.result()
                except Exception as exc:
                    warnings.append(f"{layer.name}: objectpunten konden niet worden gelezen ({exc}).")

        candidate_layers = [layer for layer in ready_layers if id(layer) in datasets_by_layer]
        if not candidate_layers:
            if warnings:
                messagebox.showwarning("Beginpunten laden", "\n".join(warnings[:15]), parent=self)
            self.set_status("Er konden geen beginpunten worden geladen.")
            return

        road_paths_by_layer: dict[int, list] = {id(layer): [] for layer in candidate_layers}
        terrain_paths_by_layer: dict[int, list] = {id(layer): [] for layer in candidate_layers}
        road_type = globals().get("RoadCenterlineClient")
        terrain_type = globals().get("BgtTerrainBoundaryClient")
        orientation_tasks = []
        for layer in candidate_layers:
            fetch_bounds = layer.bounds.padded(self.cadastral_exporter._overview_padding(layer.bounds))
            if self.road_centerline_client is not None and road_type is not None:
                orientation_tasks.append((layer, "road", fetch_bounds))
            if self.bgt_terrain_boundary_client is not None and terrain_type is not None:
                orientation_tasks.append((layer, "terrain", fetch_bounds))

        def fetch_orientation_paths(task):
            layer, path_kind, fetch_bounds = task
            try:
                if path_kind == "road":
                    client = _clone_path_client(self.road_centerline_client, road_type)
                else:
                    client = _clone_path_client(self.bgt_terrain_boundary_client, terrain_type)
                if client is None:
                    return layer, path_kind, []
                return layer, path_kind, client.fetch_paths(fetch_bounds)
            except Exception:
                return layer, path_kind, []

        if orientation_tasks:
            self.set_status(
                f"Kaartlijnen voor beginpunten tegelijk ophalen (0/{len(orientation_tasks)})..."
            )
            self.update_idletasks()
            orientation_workers = max(1, min(8, len(orientation_tasks)))
            with ThreadPoolExecutor(
                max_workers=orientation_workers,
                thread_name_prefix="start-point-map-data",
            ) as executor:
                future_map = {
                    executor.submit(fetch_orientation_paths, task): task
                    for task in orientation_tasks
                }
                for completed_index, future in enumerate(as_completed(future_map), start=1):
                    layer, path_kind, paths = future.result()
                    if path_kind == "road":
                        road_paths_by_layer[id(layer)] = paths
                    else:
                        terrain_paths_by_layer[id(layer)] = paths
                    self.set_status(
                        f"Kaartlijnen voor beginpunten tegelijk ophalen "
                        f"({completed_index}/{len(orientation_tasks)})..."
                    )
                    self.update_idletasks()

        resolved_rules = self._resolved_cross_section_layer_rules()

        def resolve_layer_start_point(layer):
            dataset = datasets_by_layer[id(layer)]
            candidate = self._auto_cross_section_start_candidate_for_layer(
                layer,
                dataset,
                resolved_rules,
                road_centerline_client=_PreloadedPathClient(road_paths_by_layer[id(layer)]),
                terrain_boundary_client=_PreloadedPathClient(terrain_paths_by_layer[id(layer)]),
            )
            if candidate is None:
                return layer, None, (
                    f"{layer.name}: onvoldoende profielpunten met hoogte of geen "
                    "beginpuntkandidaten gevonden."
                )
            return layer, (float(candidate.x), float(candidate.y)), None

        results = []
        candidate_workers = max(1, min(8, len(candidate_layers)))
        with ThreadPoolExecutor(max_workers=candidate_workers, thread_name_prefix="start-points") as executor:
            future_map = {
                executor.submit(resolve_layer_start_point, layer): layer
                for layer in candidate_layers
            }
            for completed_index, future in enumerate(as_completed(future_map), start=1):
                layer = future_map[future]
                self.set_status(f"Beginpunten bepalen ({completed_index}/{len(candidate_layers)}): {layer.name}")
                self.update_idletasks()
                try:
                    target_layer, coordinates, warning = future.result()
                except Exception as exc:
                    warnings.append(f"{layer.name}: beginpunt bepalen mislukt ({exc}).")
                    continue
                if warning:
                    warnings.append(warning)
                elif coordinates is not None:
                    results.append((target_layer, coordinates[0], coordinates[1]))

        for layer, start_x, start_y in results:
            self._set_template_cross_section_start_metadata(layer, start_x, start_y)

        self._refresh_map_edit_markers()
        self.request_render(immediate=False)
        if warnings:
            messagebox.showwarning("Beginpunten laden", "\n".join(warnings[:15]), parent=self)
        if results:
            self.set_status(
                f"Beginpunten geladen voor {len(results)} KickTheMap-proefsleuf/proefsleuven."
            )
        else:
            self.set_status("Er konden geen beginpunten worden geladen.")

    viewer_class._load_local_maaiveld_dataset_for_layer = _cached_local_maaiveld_dataset_for_layer
    viewer_class._load_maaiveld_dataset_for_layer = _load_maaiveld_dataset_with_local_cache
    viewer_class.load_all_kickthemap_start_points = _load_all_kickthemap_start_points_fast
    viewer_class._fast_kickthemap_start_points_patch = True


_load_cached_module()
_install_sleufbase_branding_patch()
_install_modern_combobox_patch()
_install_modern_dialog_style_patch()
_install_template_variant_order_patch()
_install_bgt_surface_autofill_patch()
_install_kickthemap_material_choices_patch()
_install_fast_kickthemap_start_points_patch()
_install_inline_location_search_patch()
_install_kickthemap_jobs_browser_patch()
_install_modern_dropdown_menu_patch()
_install_ai_maaiveld_patch()
