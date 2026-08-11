from __future__ import annotations

import json
import re
import subprocess
import sys
import threading
import tkinter as tk
from pathlib import Path
from tkinter import messagebox, ttk

from PIL import ImageDraw, ImageFont, ImageTk
from pyproj import Transformer

from .ipc import send_paths_to_running_instance
from .kickthemap import KickTheMapClient, KickTheMapError, KickTheMapJob
from .models import Bounds
from .osm import OpenStreetMapTileClient
from .rounded_widgets import RoundedButton, RoundedEntry, install_rounded_buttons
from .settings import KickTheMapSavedAccount, load_settings


install_rounded_buttons()

WINDOW_TITLE = "SleufBase Jobs"
WINDOW_WIDTH = 1280
WINDOW_HEIGHT = 760

APP_BG = "#f5f7fb"
SURFACE = "#ffffff"
SURFACE_ALT = "#fafbfd"
SURFACE_SOFT = "#f3f6fa"
INPUT_BG = "#fbfcfe"
TEXT = "#111827"
MUTED = "#667085"
ACCENT = "#f97316"
ACCENT_DARK = "#c2410c"
ACCENT_LIGHT = "#fdba74"
BORDER = "#dde4ee"
BORDER_SOFT = "#e9eef5"

COLUMNS = (
    ("title", "Jobnaam", 220),
    ("address", "Adres", 330),
    ("coordinates", "Coordinaten", 150),
    ("client_date", "Aanmaak datum", 130),
    ("status_text", "Status", 150),
    ("municipality", "Gemeente", 150),
    ("job_id", "Job ID", 90),
)

STATUS_LABELS = {
    1: "Aangemaakt",
    2: "Wacht op verwerking",
    3: "In wachtrij",
    4: "In wachtrij",
    5: "Verwerken",
    6: "Controle nodig",
    7: "Upload bezig",
    8: "Mislukt",
    9: "Gereed",
    10: "Gereed",
    11: "Wacht op bestanden",
}


def _selected_account() -> KickTheMapSavedAccount | None:
    settings = load_settings()
    selected_email = str(settings.kickthemap_last_email or "").strip().lower()
    if not selected_email:
        return None
    for account in settings.kickthemap_saved_accounts or []:
        if str(account.email or "").strip().lower() == selected_email:
            return account
    return None


def _job_url(job: KickTheMapJob) -> str:
    owner_id = str(getattr(job, "user_id", "") or "").strip() or job.project_mail
    return f"{KickTheMapClient.BASE_URL}/viewJob/{owner_id}_{job.project_date}"


def _browser_launch_command(url: str, title: str | None = None) -> tuple[str, list[str]]:
    arguments = ["--kickthemap-browser-url", url]
    if title:
        arguments.extend(["--kickthemap-browser-title", title])
    if getattr(sys, "frozen", False):
        return sys.executable, arguments
    main_script = Path(__file__).resolve().parent.parent / "main.py"
    return sys.executable, [str(main_script)] + arguments


def _browser_prelogin_launch_command() -> tuple[str, list[str]]:
    arguments = ["--kickthemap-browser-prelogin"]
    if getattr(sys, "frozen", False):
        return sys.executable, arguments
    main_script = Path(__file__).resolve().parent.parent / "main.py"
    return sys.executable, [str(main_script)] + arguments


def _main_app_launch_command(paths: list[str]) -> tuple[str, list[str]]:
    if getattr(sys, "frozen", False):
        return sys.executable, paths
    main_script = Path(__file__).resolve().parent.parent / "main.py"
    return sys.executable, [str(main_script)] + paths


def _resource_path(*parts: str) -> Path:
    base = Path(getattr(sys, "_MEIPASS", Path(__file__).resolve().parent.parent))
    return base.joinpath(*parts)


_WGS84_TO_RD = Transformer.from_crs("EPSG:4326", "EPSG:28992", always_xy=True)


def _job_rd_point(job: KickTheMapJob) -> tuple[float, float] | None:
    values = re.findall(r"-?\d+(?:[.,]\d+)?", str(job.coordinates or ""))
    if len(values) < 2:
        return None
    try:
        first = float(values[0].replace(",", "."))
        second = float(values[1].replace(",", "."))
    except ValueError:
        return None
    if abs(first) <= 20 and abs(second) <= 90:
        try:
            x_coord, y_coord = _WGS84_TO_RD.transform(first, second)
            return float(x_coord), float(y_coord)
        except Exception:
            return None
    if abs(first) > 1000 and abs(second) > 1000:
        return first, second
    return None


class KickTheMapJobsMapView(ttk.Frame):
    def __init__(
        self,
        master: tk.Misc,
        jobs: list[KickTheMapJob],
        *,
        selected_job_ids: set[str],
        back_callback,
        select_callback,
        open_callback,
    ) -> None:
        super().__init__(master, padding=18, style="App.TFrame")
        self.jobs = jobs
        self.points: list[tuple[KickTheMapJob, float, float]] = []
        self.bounds: Bounds | None = None
        self.display_bounds: Bounds | None = None
        self.current_image = None
        self._current_image_key: tuple[float, float, float, float, int, int] | None = None
        self.photo_image = None
        self.tile_client = OpenStreetMapTileClient(max_workers=8)
        self.status_var = tk.StringVar(value="Kaart voorbereiden...")
        self._pending_map_result = None
        self._map_request_id = 0
        self._finished_map_request_id = 0
        self._marker_positions: list[tuple[KickTheMapJob, int, int]] = []
        self.selected_job_ids = {str(job_id) for job_id in selected_job_ids}
        self._job_label_cache: dict[int, tuple[str, str]] = {}
        self._font_cache = None
        self.back_callback = back_callback
        self.select_callback = select_callback
        self.open_callback = open_callback
        self._drag_start: tuple[int, int, Bounds] | None = None
        self._drag_last: tuple[int, int] | None = None
        self._drag_moved = False
        self._selection_start: tuple[int, int] | None = None
        self._selection_rect_id: int | None = None
        self._configure_refresh_id: str | None = None

        self._build_layout()
        self._set_points(jobs)
        self.after(100, self.reset_view)

    def _build_layout(self) -> None:
        self.columnconfigure(0, weight=1)
        self.rowconfigure(1, weight=1)

        header = ttk.Frame(self, style="App.TFrame")
        header.grid(row=0, column=0, sticky="ew", pady=(0, 10))
        header.columnconfigure(0, weight=1)
        ttk.Label(header, textvariable=self.status_var, style="Muted.TLabel").grid(row=0, column=0, sticky="w")
        ttk.Button(header, text="Terug naar overzicht", command=self.back_callback, style="Compact.TButton").grid(row=0, column=1, sticky="e", padx=(0, 8))
        ttk.Button(header, text="Zoom naar selectie", command=self.zoom_to_selection, style="Compact.TButton").grid(row=0, column=2, sticky="e", padx=(0, 8))
        ttk.Button(header, text="Passend", command=self.reset_view, style="Compact.TButton").grid(row=0, column=3, sticky="e", padx=(0, 8))
        ttk.Button(header, text="Vernieuwen", command=self.refresh_map, style="Compact.TButton").grid(row=0, column=4, sticky="e")

        self.canvas = tk.Canvas(self, background="#eef2f7", highlightthickness=1, highlightbackground=BORDER, bd=0, takefocus=1)
        self.canvas.grid(row=1, column=0, sticky="nsew")
        self.canvas.bind("<Enter>", lambda _event: self.canvas.focus_set())
        self.canvas.bind("<ButtonPress-1>", self._on_select_press)
        self.canvas.bind("<B1-Motion>", self._on_select_drag)
        self.canvas.bind("<ButtonRelease-1>", self._on_select_release)
        self.canvas.bind("<ButtonPress-2>", self._on_pan_press)
        self.canvas.bind("<B2-Motion>", self._on_pan_drag)
        self.canvas.bind("<ButtonRelease-2>", self._on_pan_release)
        self.canvas.bind("<Double-1>", self._on_double_click)
        self.canvas.bind("<MouseWheel>", self._on_mousewheel)
        self.canvas.bind("<Configure>", self._on_configure)

    def _set_points(self, jobs: list[KickTheMapJob]) -> None:
        points: list[tuple[KickTheMapJob, float, float]] = []
        for job in jobs:
            point = _job_rd_point(job)
            if point is None:
                continue
            points.append((job, point[0], point[1]))
        self.points = points
        if not points:
            self.bounds = None
            return

        xs = [point[1] for point in points]
        ys = [point[2] for point in points]
        bounds = Bounds(min(xs), min(ys), max(xs), max(ys))
        padding = max(500.0, bounds.width * 0.18, bounds.height * 0.18)
        bounds = bounds.padded(padding)
        if bounds.width <= 1:
            bounds = Bounds(bounds.min_x - 500, bounds.min_y, bounds.max_x + 500, bounds.max_y)
        if bounds.height <= 1:
            bounds = Bounds(bounds.min_x, bounds.min_y - 500, bounds.max_x, bounds.max_y + 500)
        self.bounds = bounds

    def reset_view(self) -> None:
        if self.bounds is not None:
            self.display_bounds = self.bounds
        self.refresh_map()

    def zoom_to_selection(self) -> None:
        selected_points = [
            (x_coord, y_coord)
            for job, x_coord, y_coord in self.points
            if str(job.job_id) in self.selected_job_ids
        ]
        if not selected_points:
            messagebox.showinfo("SleufBase Jobs Kaart", "Selecteer eerst een of meer jobs op de kaart.")
            return
        xs = [point[0] for point in selected_points]
        ys = [point[1] for point in selected_points]
        bounds = Bounds(min(xs), min(ys), max(xs), max(ys))
        padding = max(150.0, bounds.width * 0.35, bounds.height * 0.35)
        if bounds.width <= 1:
            padding = max(padding, 250.0)
        if bounds.height <= 1:
            padding = max(padding, 250.0)
        self.display_bounds = bounds.padded(padding)
        self.refresh_map()

    def refresh_map(self) -> None:
        if not self.points or self.display_bounds is None:
            self.canvas.delete("all")
            self.status_var.set("Geen jobs met geldige coordinaten gevonden.")
            self.canvas.create_text(
                max(1, self.canvas.winfo_width()) / 2,
                max(1, self.canvas.winfo_height()) / 2,
                text="Geen jobs met geldige coordinaten.",
                fill=MUTED,
                font=("Segoe UI", 13, "bold"),
            )
            return
        width = max(420, self.canvas.winfo_width() or 1000)
        height = max(320, self.canvas.winfo_height() or 640)
        bounds = self.display_bounds.expand_to_aspect_ratio(width / height)
        image_key = self._map_image_key(bounds, width, height)
        if self.current_image is not None and self._current_image_key == image_key:
            self._show_map(self.current_image, bounds, update_base_image=False)
            return
        self.status_var.set(f"OpenStreetMap laden... {len(self.points)} punt(en)")
        self._map_request_id += 1
        request_id = self._map_request_id
        self._finished_map_request_id = 0
        threading.Thread(target=self._load_map_worker, args=(request_id, bounds, width, height), daemon=True).start()
        self.after(80, self._poll_map_result)

    def _load_map_worker(self, request_id: int, bounds: Bounds, width: int, height: int) -> None:
        try:
            def on_progress(preview_image) -> None:
                if self.current_image is not None:
                    return
                self._pending_map_result = (request_id, preview_image, bounds, None)

            image = self.tile_client.fetch_map(bounds, (width, height), on_progress=on_progress)
            self._pending_map_result = (request_id, image, bounds, None)
        except Exception as exc:
            self._pending_map_result = (request_id, None, bounds, exc)
        finally:
            self._finished_map_request_id = request_id

    def _poll_map_result(self) -> None:
        result = self._pending_map_result
        if result is None:
            if self.winfo_exists():
                self.after(80, self._poll_map_result)
            return
        self._pending_map_result = None
        request_id, image, bounds, exc = result
        if request_id != self._map_request_id:
            return
        if exc is not None:
            self._show_map_error(exc)
            return
        self._show_map(image, bounds, image_key=self._map_image_key(bounds, image.size[0], image.size[1]))
        if self._finished_map_request_id != request_id and self.winfo_exists():
            self.after(80, self._poll_map_result)

    @staticmethod
    def _map_image_key(bounds: Bounds, width: int, height: int) -> tuple[float, float, float, float, int, int]:
        return (
            round(bounds.min_x, 3),
            round(bounds.min_y, 3),
            round(bounds.max_x, 3),
            round(bounds.max_y, 3),
            int(width),
            int(height),
        )

    def _show_map_error(self, exc: Exception) -> None:
        self.status_var.set(f"Kaart laden mislukt: {exc}")
        if self.current_image is not None:
            self.canvas.create_text(
                max(1, self.canvas.winfo_width()) / 2,
                18,
                anchor="n",
                text="OpenStreetMap kon niet worden geladen. Laatste kaart blijft zichtbaar.",
                fill="#c62828",
                font=("Segoe UI", 10, "bold"),
                tags=("map_notice",),
            )
            return
        self.canvas.delete("all")
        self.canvas.create_text(
            max(1, self.canvas.winfo_width()) / 2,
            max(1, self.canvas.winfo_height()) / 2,
            text="OpenStreetMap kon niet worden geladen.",
            fill="#c62828",
            font=("Segoe UI", 13, "bold"),
        )

    def _show_map(self, image, bounds: Bounds, *, image_key=None, update_base_image: bool = True) -> None:
        self.display_bounds = bounds
        if update_base_image:
            self.current_image = image.copy()
            self._current_image_key = image_key or self._map_image_key(bounds, image.size[0], image.size[1])
        self._marker_positions = []
        width, height = image.size
        visible_points: list[tuple[KickTheMapJob, int, int]] = []
        for job, x_coord, y_coord in self.points:
            if bounds.width <= 0 or bounds.height <= 0:
                continue
            x_screen = int(round((x_coord - bounds.min_x) / bounds.width * width))
            y_screen = int(round((bounds.max_y - y_coord) / bounds.height * height))
            if -24 <= x_screen <= width + 24 and -24 <= y_screen <= height + 24:
                self._marker_positions.append((job, x_screen, y_screen))
                visible_points.append((job, x_screen, y_screen))
        draw_all_labels = len(visible_points) <= 180 or bounds.width <= 25000 or bounds.height <= 25000
        rendered = image.convert("RGBA")
        draw = ImageDraw.Draw(rendered)
        title_font, date_font, attribution_font = self._map_fonts()
        for job, x_screen, y_screen in visible_points:
            self._draw_job_marker(draw, x_screen, y_screen, job, draw_label=draw_all_labels, title_font=title_font, date_font=date_font)
        attribution = "(c) OpenStreetMap contributors"
        attr_box = draw.textbbox((0, 0), attribution, font=attribution_font)
        draw.text((width - (attr_box[2] - attr_box[0]) - 8, height - (attr_box[3] - attr_box[1]) - 8), attribution, fill="#475467", font=attribution_font)
        self.photo_image = ImageTk.PhotoImage(rendered)
        self.canvas.delete("all")
        self.canvas.create_image(0, 0, anchor="nw", image=self.photo_image, tags=("map_content",))
        self._set_selection_status()

    def _map_fonts(self):
        if self._font_cache is not None:
            return self._font_cache
        try:
            self._font_cache = (
                ImageFont.truetype("segoeui.ttf", 12),
                ImageFont.truetype("segoeui.ttf", 8),
                ImageFont.truetype("segoeui.ttf", 8),
            )
        except OSError:
            default_font = ImageFont.load_default()
            self._font_cache = (default_font, default_font, default_font)
        return self._font_cache

    @staticmethod
    def _job_date_label(job: KickTheMapJob) -> str:
        value = str(job.client_date or "").strip()
        if not value:
            return ""
        return re.split(r"[T\s]", value, maxsplit=1)[0]

    def _draw_job_marker(
        self,
        draw: ImageDraw.ImageDraw,
        x: int,
        y: int,
        job: KickTheMapJob,
        *,
        draw_label: bool,
        title_font,
        date_font,
    ) -> None:
        selected = str(job.job_id) in self.selected_job_ids
        radius = 8 if selected else 6
        fill = "#2563eb" if selected else ACCENT
        outline = "#1e40af" if selected else ACCENT_DARK
        draw.ellipse((x - radius, y - radius, x + radius, y + radius), fill=fill, outline=outline, width=2)
        if not (selected or draw_label):
            return
        title, date = self._job_label(job)
        title_box = draw.textbbox((0, 0), title, font=title_font)
        title_width = title_box[2] - title_box[0]
        title_height = title_box[3] - title_box[1]
        date_width = date_height = 0
        if date:
            date_box = draw.textbbox((0, 0), date, font=date_font)
            date_width = date_box[2] - date_box[0]
            date_height = date_box[3] - date_box[1]
        total_height = title_height + (date_height + 2 if date else 0)
        label_x = x - max(title_width, date_width) / 2
        label_y = y - radius - total_height - 8
        self._draw_text_with_outline(draw, (label_x, label_y), title, title_font, TEXT)
        if date:
            date_x = x - date_width / 2
            self._draw_text_with_outline(draw, (date_x, label_y + title_height + 2), date, date_font, MUTED)

    @staticmethod
    def _draw_text_with_outline(draw: ImageDraw.ImageDraw, position: tuple[float, float], text: str, font, fill: str) -> None:
        x, y = position
        for dx, dy in ((-1, 0), (1, 0), (0, -1), (0, 1)):
            draw.text((x + dx, y + dy), text, fill="#ffffff", font=font)
        draw.text((x, y), text, fill=fill, font=font)

    def _job_label(self, job: KickTheMapJob) -> tuple[str, str]:
        cached = self._job_label_cache.get(job.job_id)
        if cached is not None:
            return cached
        title = str(job.title or "").strip() or "Job"
        title = title if len(title) <= 42 else title[:39].rstrip() + "..."
        date = self._job_date_label(job)
        value = (title, date)
        self._job_label_cache[job.job_id] = value
        return value

    def _on_configure(self, _event) -> None:
        if self.display_bounds is not None:
            if self._configure_refresh_id is not None:
                self.after_cancel(self._configure_refresh_id)
            self._configure_refresh_id = self.after(180, self._refresh_after_configure)

    def _refresh_after_configure(self) -> None:
        self._configure_refresh_id = None
        self.refresh_map()

    def _on_mousewheel(self, event) -> None:
        if self.display_bounds is None:
            return
        factor = 0.72 if event.delta > 0 else 1.35
        self.display_bounds = self._scaled_bounds_around(event.x, event.y, factor)
        self.refresh_map()

    def _scaled_bounds_around(self, screen_x: int, screen_y: int, factor: float) -> Bounds:
        bounds = self.display_bounds
        if bounds is None:
            return self.bounds or Bounds(0, 0, 1, 1)
        width = max(1, self.canvas.winfo_width())
        height = max(1, self.canvas.winfo_height())
        x_ratio = max(0.0, min(1.0, screen_x / width))
        y_ratio = max(0.0, min(1.0, screen_y / height))
        world_x = bounds.min_x + bounds.width * x_ratio
        world_y = bounds.max_y - bounds.height * y_ratio
        new_width = max(50.0, bounds.width * factor)
        new_height = max(50.0, bounds.height * factor)
        return Bounds(
            world_x - new_width * x_ratio,
            world_y - new_height * (1 - y_ratio),
            world_x + new_width * (1 - x_ratio),
            world_y + new_height * y_ratio,
        )

    def _on_select_press(self, event) -> None:
        self._selection_start = (event.x, event.y)
        if self._selection_rect_id is not None:
            self.canvas.delete(self._selection_rect_id)
            self._selection_rect_id = None

    def _on_select_drag(self, event) -> None:
        if self._selection_start is None:
            return
        start_x, start_y = self._selection_start
        if self._selection_rect_id is None:
            self._selection_rect_id = self.canvas.create_rectangle(
                start_x,
                start_y,
                event.x,
                event.y,
                outline="#2563eb",
                width=2,
                dash=(4, 3),
                fill="#bfdbfe",
                stipple="gray25",
                tags=("selection_rect",),
            )
        else:
            self.canvas.coords(self._selection_rect_id, start_x, start_y, event.x, event.y)

    def _on_select_release(self, event) -> None:
        if self._selection_start is None:
            return
        start_x, start_y = self._selection_start
        self._selection_start = None
        x1, x2 = sorted((start_x, event.x))
        y1, y2 = sorted((start_y, event.y))
        if self._selection_rect_id is not None:
            self.canvas.delete(self._selection_rect_id)
            self._selection_rect_id = None
        if abs(x2 - x1) <= 4 and abs(y2 - y1) <= 4:
            job = self._nearest_marker(event.x, event.y)
            if job is not None:
                self._select_job_ids({str(job.job_id)}, event.state, single_click=True)
            return
        selected_ids = {
            str(job.job_id)
            for job, marker_x, marker_y in self._marker_positions
            if x1 <= marker_x <= x2 and y1 <= marker_y <= y2
        }
        self._select_job_ids(selected_ids, event.state, single_click=False)

    def _select_job_ids(self, job_ids: set[str], event_state: int, *, single_click: bool) -> None:
        shift_pressed = bool(event_state & 0x0001)
        ctrl_pressed = bool(event_state & 0x0004)
        if not job_ids and not (shift_pressed or ctrl_pressed):
            self.selected_job_ids.clear()
        elif ctrl_pressed:
            self.selected_job_ids.difference_update(job_ids)
        elif shift_pressed:
            self.selected_job_ids.update(job_ids)
        else:
            self.selected_job_ids = set(job_ids)
        self.select_callback(self.selected_job_ids)
        self._redraw_markers_only()

    def _on_pan_press(self, event) -> None:
        if self.display_bounds is None:
            return
        self._drag_start = (event.x, event.y, self.display_bounds)
        self._drag_last = (event.x, event.y)
        self._drag_moved = False

    def _on_pan_drag(self, event) -> None:
        if self._drag_last is None:
            return
        last_x, last_y = self._drag_last
        dx = event.x - last_x
        dy = event.y - last_y
        if dx or dy:
            self.canvas.move("map_content", dx, dy)
            self._marker_positions = [
                (job, marker_x + dx, marker_y + dy)
                for job, marker_x, marker_y in self._marker_positions
            ]
            self._drag_moved = True
        self._drag_last = (event.x, event.y)

    def _on_pan_release(self, event) -> None:
        if self._drag_start is None:
            return
        start_x, start_y, start_bounds = self._drag_start
        total_dx = event.x - start_x
        total_dy = event.y - start_y
        if self._drag_moved and (abs(total_dx) > 3 or abs(total_dy) > 3):
            width = max(1, self.canvas.winfo_width())
            height = max(1, self.canvas.winfo_height())
            shift_x = -total_dx / width * start_bounds.width
            shift_y = total_dy / height * start_bounds.height
            self.display_bounds = Bounds(
                start_bounds.min_x + shift_x,
                start_bounds.min_y + shift_y,
                start_bounds.max_x + shift_x,
                start_bounds.max_y + shift_y,
            )
            self.refresh_map()
        self._drag_start = None
        self._drag_last = None

    def _on_double_click(self, event) -> None:
        job = self._nearest_marker(event.x, event.y)
        if job is not None:
            self.open_callback(job)

    def _nearest_marker(self, x: int, y: int) -> KickTheMapJob | None:
        best_job = None
        best_distance = 18 * 18
        for job, marker_x, marker_y in self._marker_positions:
            dx = x - marker_x
            dy = y - marker_y
            distance = dx * dx + dy * dy
            if distance <= best_distance:
                best_distance = distance
                best_job = job
        return best_job

    def _redraw_markers_only(self) -> None:
        if self.current_image is None or self.display_bounds is None:
            return
        self._show_map(self.current_image, self.display_bounds, update_base_image=False)

    def _set_selection_status(self) -> None:
        selected = len(self.selected_job_ids)
        suffix = f" {selected} geselecteerd." if selected else " Klik op punten om jobs te selecteren."
        self.status_var.set(f"{len(self.points)} jobpunt(en) op kaart.{suffix}")


class JobGridTable(ttk.Frame):
    row_height = 28
    header_height = 30

    def __init__(self, master: tk.Widget, *, sort_callback, open_callback, value_callback) -> None:
        super().__init__(master, style="PlainCard.TFrame")
        self.sort_callback = sort_callback
        self.open_callback = open_callback
        self.value_callback = value_callback
        self.jobs: list[KickTheMapJob] = []
        self.selected_indices: set[int] = set()
        self.anchor_index: int | None = None
        self.total_width = sum(width for _key, _label, width in COLUMNS)

        self.columnconfigure(0, weight=1)
        self.rowconfigure(1, weight=1)

        self.header = tk.Canvas(
            self,
            height=self.header_height,
            background=SURFACE_SOFT,
            highlightthickness=0,
            bd=0,
        )
        self.header.grid(row=0, column=0, sticky="ew")
        self.header.bind("<Button-1>", self._on_header_click)

        self.canvas = tk.Canvas(
            self,
            background=SURFACE,
            highlightthickness=1,
            highlightbackground=BORDER,
            bd=0,
        )
        self.canvas.grid(row=1, column=0, sticky="nsew")
        self.canvas.bind("<Button-1>", self._on_body_click)
        self.canvas.bind("<Double-1>", self._on_body_double_click)
        self.canvas.bind("<Configure>", lambda _event: self._draw())

        y_scroll = ttk.Scrollbar(self, orient=tk.VERTICAL, command=self._yview)
        y_scroll.grid(row=1, column=1, sticky="ns")
        x_scroll = ttk.Scrollbar(self, orient=tk.HORIZONTAL, command=self._xview)
        x_scroll.grid(row=2, column=0, sticky="ew")
        self.canvas.configure(yscrollcommand=y_scroll.set, xscrollcommand=x_scroll.set)
        self.canvas.bind("<MouseWheel>", self._on_mousewheel)

        self._draw_header()

    def set_jobs(self, jobs: list[KickTheMapJob]) -> None:
        selected_job_ids = {
            self.jobs[index].job_id
            for index in self.selected_indices
            if 0 <= index < len(self.jobs)
        }
        self.jobs = jobs
        self.selected_indices = {
            index
            for index, job in enumerate(jobs)
            if job.job_id in selected_job_ids
        }
        if self.anchor_index is not None and self.anchor_index >= len(jobs):
            self.anchor_index = None
        self._draw()

    def selected_job(self) -> KickTheMapJob | None:
        selected = self.selected_jobs()
        return selected[0] if selected else None

    def selected_jobs(self) -> list[KickTheMapJob]:
        if not self.selected_indices:
            return []
        return [
            self.jobs[index]
            for index in sorted(self.selected_indices)
            if 0 <= index < len(self.jobs)
        ]

    def _xview(self, *args) -> None:
        self.canvas.xview(*args)
        self.header.xview_moveto(self.canvas.xview()[0])

    def _yview(self, *args) -> None:
        self.canvas.yview(*args)
        self._draw()

    def _on_mousewheel(self, event) -> None:
        self.canvas.yview_scroll(int(-1 * (event.delta / 120)), "units")
        self._draw()

    def _column_at(self, x: float) -> str | None:
        x = self.header.canvasx(x)
        left = 0
        for key, _label, width in COLUMNS:
            right = left + width
            if left <= x < right:
                return key
            left = right
        return None

    def _draw_header(self) -> None:
        self.header.delete("all")
        self.header.configure(scrollregion=(0, 0, self.total_width, self.header_height))
        left = 0
        for key, label, width in COLUMNS:
            self.header.create_rectangle(left, 0, left + width, self.header_height, fill=SURFACE_SOFT, outline="")
            self.header.create_text(left + width / 2, self.header_height / 2, anchor="center", text=label, fill=TEXT, font=("Segoe UI", 9, "bold"))
            left += width

    def _draw(self) -> None:
        self.canvas.delete("all")
        height = max(self.canvas.winfo_height(), 1)
        content_height = max(len(self.jobs) * self.row_height, height)
        self.canvas.configure(scrollregion=(0, 0, self.total_width, content_height))

        first = max(0, int(self.canvas.canvasy(0) // self.row_height))
        last = min(len(self.jobs), int((self.canvas.canvasy(height) // self.row_height) + 2))
        for row_index in range(first, last):
            job = self.jobs[row_index]
            top = row_index * self.row_height
            bottom = top + self.row_height
            fill = "#fff7ed" if row_index in self.selected_indices else SURFACE
            self.canvas.create_rectangle(0, top, self.total_width, bottom, fill=fill, outline="")

            left = 0
            for key, _label, width in COLUMNS:
                value = self._fit_text(self.value_callback(job, key), width)
                color = self._row_color(job)
                self.canvas.create_text(left + 8, top + self.row_height / 2, anchor="w", text=value, fill=color, font=("Segoe UI", 10))
                left += width
                self.canvas.create_line(left, top, left, bottom, fill=BORDER_SOFT)
            self.canvas.create_line(0, bottom, self.total_width, bottom, fill=BORDER_SOFT)

    def _row_color(self, job: KickTheMapJob) -> str:
        if job.status in {9, 10}:
            return "#167a2f"
        if job.status == 8:
            return "#c62828"
        if job.status == 6:
            return ACCENT_DARK
        return TEXT

    @staticmethod
    def _fit_text(value: str, width: int) -> str:
        text = str(value)
        max_chars = max(4, int((width - 16) / 7))
        if len(text) <= max_chars:
            return text
        return text[: max_chars - 3].rstrip() + "..."

    def _on_header_click(self, event) -> None:
        column = self._column_at(event.x)
        if column is not None:
            self.sort_callback(column)

    def _on_body_click(self, event) -> None:
        row_index = int(self.canvas.canvasy(event.y) // self.row_height)
        if 0 <= row_index < len(self.jobs):
            if event.state & 0x0001:
                anchor = self.anchor_index if self.anchor_index is not None else row_index
                start, end = sorted((anchor, row_index))
                self.selected_indices = set(range(start, end + 1))
            elif event.state & 0x0004:
                if row_index in self.selected_indices:
                    self.selected_indices.remove(row_index)
                else:
                    self.selected_indices.add(row_index)
                self.anchor_index = row_index
            else:
                self.selected_indices = {row_index}
                self.anchor_index = row_index
            self._draw()

    def _on_body_double_click(self, event) -> None:
        self._on_body_click(event)
        self.open_callback()


class KickTheMapJobsWindow(tk.Tk):
    def __init__(self) -> None:
        super().__init__()
        self.title(WINDOW_TITLE)
        self.geometry(f"{WINDOW_WIDTH}x{WINDOW_HEIGHT}")
        self.minsize(980, 560)
        self._apply_window_icon()
        self._configure_styles()

        self.account = _selected_account()
        self.client = KickTheMapClient()
        self.jobs: list[KickTheMapJob] = []
        self.filter_vars: dict[str, tk.StringVar] = {}
        self.sort_column = "client_date"
        self.sort_reverse = True
        self.main_frame: ttk.Frame | None = None
        self.map_view: KickTheMapJobsMapView | None = None
        self.show_selected_only = False
        self.selected_only_ids: set[str] = set()
        self.show_loaded_jobs_only = False
        self.loaded_job_ids: set[str] = set()
        self._browser_prelogin_started = False

        self._build_layout()
        self.after(50, self.refresh_jobs)

    def _build_layout(self) -> None:
        self.configure(bg=APP_BG)
        outer = ttk.Frame(self, padding=18, style="App.TFrame")
        outer.pack(fill=tk.BOTH, expand=True)
        self.main_frame = outer
        outer.columnconfigure(0, weight=1)
        outer.rowconfigure(1, weight=1)

        header = ttk.Frame(outer, style="App.TFrame")
        header.grid(row=0, column=0, sticky="ew")
        header.columnconfigure(1, weight=1)

        title_block = ttk.Frame(header, style="App.TFrame")
        title_block.grid(row=0, column=0, sticky="w")
        ttk.Label(title_block, text="KickTheMap jobs", style="DialogTitle.TLabel").grid(row=0, column=0, sticky="w")
        ttk.Label(
            title_block,
            text="Filter jobs en open een job in de geintegreerde KickTheMap browser.",
            style="Muted.TLabel",
        ).grid(row=1, column=0, sticky="w", pady=(3, 0))
        self.summary_var = tk.StringVar(value="")
        self.selected_only_button_var = tk.StringVar(value="Alleen geselecteerd")
        self.loaded_jobs_button_var = tk.StringVar(value="Geladen Jobs")

        button_row = ttk.Frame(header, style="App.TFrame")
        button_row.grid(row=0, column=2, sticky="e")
        ttk.Button(button_row, text="Vernieuwen", command=self.refresh_jobs, style="Compact.TButton").pack(side=tk.LEFT, padx=(0, 8))
        ttk.Button(button_row, textvariable=self.loaded_jobs_button_var, command=self.toggle_loaded_jobs_only, style="Compact.TButton").pack(side=tk.LEFT, padx=(0, 8))
        ttk.Button(button_row, textvariable=self.selected_only_button_var, command=self.toggle_selected_only, style="Compact.TButton").pack(side=tk.LEFT, padx=(0, 8))
        ttk.Button(button_row, text="Kaart", command=self.open_jobs_map, style="Compact.TButton").pack(side=tk.LEFT)

        card = ttk.Frame(outer, padding=14, style="Card.TFrame")
        card.grid(row=1, column=0, sticky="nsew", pady=(14, 0))
        card.columnconfigure(0, weight=1)
        card.rowconfigure(1, weight=1)

        filter_frame = ttk.Frame(card, style="PlainCard.TFrame")
        filter_frame.grid(row=0, column=0, sticky="ew", pady=(0, 10))
        for index, (key, label, width) in enumerate(COLUMNS):
            filter_frame.columnconfigure(index, weight=1 if key in {"title", "address"} else 0)
            cell = ttk.Frame(filter_frame, style="PlainCard.TFrame")
            cell.grid(row=0, column=index, sticky="ew", padx=(0 if index == 0 else 6, 0))
            ttk.Label(cell, text=label, style="CardMuted.TLabel").pack(anchor="center")
            var = tk.StringVar()
            var.trace_add("write", lambda *_args: self._refresh_table())
            self.filter_vars[key] = var
            entry = RoundedEntry(cell, textvariable=var, width=max(8, width // 12))
            entry.pack(fill=tk.X)

        table_frame = ttk.Frame(card, style="PlainCard.TFrame")
        table_frame.grid(row=1, column=0, sticky="nsew")
        table_frame.columnconfigure(0, weight=1)
        table_frame.rowconfigure(0, weight=1)

        self.table = JobGridTable(
            table_frame,
            sort_callback=self._sort_by,
            open_callback=self.open_selected_job,
            value_callback=self._row_value,
        )
        self.table.grid(row=0, column=0, sticky="nsew")
        ttk.Label(card, textvariable=self.summary_var, style="CardMuted.TLabel").grid(row=2, column=0, sticky="w", pady=(8, 0))

        footer = ttk.Frame(outer, style="App.TFrame")
        footer.grid(row=2, column=0, sticky="ew", pady=(12, 0))
        footer.columnconfigure(0, weight=1)
        self.status_var = tk.StringVar(value="Jobs laden...")
        ttk.Label(footer, textvariable=self.status_var, style="Muted.TLabel").grid(row=0, column=0, sticky="w")
        ttk.Button(footer, text="Laad GeoTIFFs", command=self.load_selected_geotiffs, style="Compact.TButton").grid(row=0, column=1, sticky="e", padx=(0, 8))
        ttk.Button(footer, text="Open job", command=self.open_selected_job, style="Accent.TButton").grid(row=0, column=2, sticky="e")

    def _configure_styles(self) -> None:
        self.option_add("*Font", "{Segoe UI} 10")
        self.option_add("*Menu.background", SURFACE)
        self.option_add("*Menu.foreground", TEXT)
        self.option_add("*Menu.activeBackground", SURFACE_SOFT)
        self.option_add("*Menu.activeForeground", TEXT)

        style = ttk.Style(self)
        try:
            style.theme_use("clam")
        except tk.TclError:
            pass
        style.configure(".", background=APP_BG, foreground=TEXT, font=("Segoe UI", 10))
        style.configure("App.TFrame", background=APP_BG)
        style.configure("Card.TFrame", background=SURFACE, relief="solid", borderwidth=1)
        style.configure("PlainCard.TFrame", background=SURFACE, relief="flat", borderwidth=0)
        style.configure("TLabel", background=APP_BG, foreground=TEXT)
        style.configure("DialogTitle.TLabel", background=APP_BG, foreground=TEXT, font=("Segoe UI", 15, "bold"))
        style.configure("Muted.TLabel", background=APP_BG, foreground=MUTED, font=("Segoe UI", 9))
        style.configure("CardMuted.TLabel", background=SURFACE, foreground=MUTED, font=("Segoe UI", 9))
        style.configure("TButton", background=SURFACE, foreground=TEXT, bordercolor=BORDER, focusthickness=1, padding=(12, 7))
        style.map("TButton", background=[("active", SURFACE_SOFT), ("pressed", BORDER_SOFT)])
        style.configure("Compact.TButton", padding=(10, 6))
        style.configure("Accent.TButton", background=ACCENT, foreground=SURFACE, bordercolor=ACCENT_DARK, padding=(14, 8))
        style.map(
            "Accent.TButton",
            background=[("active", ACCENT_LIGHT), ("pressed", ACCENT_DARK)],
            foreground=[("active", SURFACE), ("pressed", SURFACE)],
        )
        style.configure("TEntry", fieldbackground=INPUT_BG, background=INPUT_BG, foreground=TEXT, bordercolor=BORDER, padding=5)
        style.configure(
            "Treeview",
            background=SURFACE,
            fieldbackground=SURFACE,
            foreground=TEXT,
            bordercolor=BORDER_SOFT,
            rowheight=27,
        )
        style.map("Treeview", background=[("selected", ACCENT)], foreground=[("selected", SURFACE)])
        style.configure("Treeview.Heading", background=SURFACE_SOFT, foreground=TEXT, bordercolor=BORDER, padding=(8, 6))
        style.map("Treeview.Heading", background=[("active", BORDER_SOFT)])
        style.configure("TScrollbar", background=SURFACE_SOFT, troughcolor=SURFACE, bordercolor=BORDER)

    def _apply_window_icon(self) -> None:
        icon_path = _resource_path("assets", "sleufbase_icon.ico")
        if not icon_path.exists():
            return
        try:
            self.iconbitmap(str(icon_path))
        except tk.TclError:
            pass

    def refresh_jobs(self) -> None:
        if self.account is None:
            self.status_var.set("Log eerst in bij KickTheMap.")
            messagebox.showinfo(
                "KickTheMap Jobs",
                "Log eerst minimaal een keer in bij KickTheMap zodat de joblijst weet welk account hij moet openen.",
            )
            return

        self.status_var.set("Jobs laden...")
        self._set_controls_enabled(False)
        threading.Thread(target=self._load_jobs_worker, daemon=True).start()

    def _load_jobs_worker(self) -> None:
        try:
            if not self.client.is_logged_in:
                jobs = self.client.login(self.account.email, self.account.password)
            else:
                jobs = self.client.fetch_jobs()
            self.after(0, lambda: self._set_jobs(jobs))
        except Exception as exc:
            self.after(0, lambda exc=exc: self._show_error(exc))

    def _set_jobs(self, jobs: list[KickTheMapJob]) -> None:
        self.jobs = jobs
        self._refresh_loaded_job_ids()
        self.status_var.set("Dubbelklik een job of gebruik Open job.")
        self._set_controls_enabled(True)
        self._refresh_table()
        self._prelogin_kickthemap_browser()

    def _prelogin_kickthemap_browser(self) -> None:
        if self._browser_prelogin_started or self.account is None:
            return
        self._browser_prelogin_started = True
        try:
            executable, arguments = _browser_prelogin_launch_command()
            subprocess.Popen(
                [executable] + arguments,
                cwd=str(Path(__file__).resolve().parent.parent),
            )
            self.status_var.set("KickTheMap browser sessie wordt voorbereid.")
        except Exception:
            self._browser_prelogin_started = False

    def _show_error(self, exc: Exception) -> None:
        message = str(exc)
        if isinstance(exc, KickTheMapError):
            title = "KickTheMap"
        else:
            title = "KickTheMap Jobs"
        self.status_var.set(message)
        self._set_controls_enabled(True)
        messagebox.showerror(title, message)

    def _set_controls_enabled(self, enabled: bool) -> None:
        state = tk.NORMAL if enabled else tk.DISABLED
        for child in self.winfo_children():
            self._set_child_state(child, state)

    def _set_child_state(self, widget: tk.Widget, state: str) -> None:
        try:
            if isinstance(widget, (RoundedButton, ttk.Button, ttk.Entry)):
                widget.configure(state=state)
        except tk.TclError:
            pass
        for child in widget.winfo_children():
            self._set_child_state(child, state)

    def _row_value(self, job: KickTheMapJob, key: str) -> str:
        if key == "job_id":
            return str(job.job_id)
        if key == "status_text":
            return STATUS_LABELS.get(job.status, f"Status {job.status}")
        return str(getattr(job, key, "") or "-")

    def _visible_jobs(self) -> list[KickTheMapJob]:
        active_filters = {
            key: var.get().strip().lower()
            for key, var in self.filter_vars.items()
            if var.get().strip()
        }
        jobs = [
            job
            for job in self.jobs
            if all(needle in self._row_value(job, key).lower() for key, needle in active_filters.items())
        ]
        if self.show_loaded_jobs_only:
            jobs = [job for job in jobs if str(job.job_id) in self.loaded_job_ids]
        if self.show_selected_only:
            jobs = [job for job in jobs if str(job.job_id) in self.selected_only_ids]
        jobs.sort(
            key=lambda job: self._row_value(job, self.sort_column).lower(),
            reverse=self.sort_reverse,
        )
        return jobs

    def _refresh_table(self) -> None:
        visible_jobs = self._visible_jobs()
        self.table.set_jobs(visible_jobs)
        self.summary_var.set(f"{len(visible_jobs)} van {len(self.jobs)}")

    def _loaded_jobs_manifest_path(self) -> Path:
        return KickTheMapClient.default_download_dir() / "loaded_jobs.json"

    def _read_loaded_jobs_manifest(self) -> dict[str, dict[str, str]]:
        manifest_path = self._loaded_jobs_manifest_path()
        if not manifest_path.exists():
            return {}
        try:
            data = json.loads(manifest_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            return {}
        if not isinstance(data, dict):
            return {}
        jobs = data.get("jobs", {})
        if not isinstance(jobs, dict):
            return {}
        return {
            str(job_id): record
            for job_id, record in jobs.items()
            if isinstance(record, dict)
        }

    def _write_loaded_jobs_manifest(self, records: dict[str, dict[str, str]]) -> None:
        manifest_path = self._loaded_jobs_manifest_path()
        manifest_path.parent.mkdir(parents=True, exist_ok=True)
        current_records = self._read_loaded_jobs_manifest()
        current_records.update(records)
        manifest_path.write_text(
            json.dumps({"jobs": current_records}, indent=2, ensure_ascii=False),
            encoding="utf-8",
        )

    def _write_loaded_job_sidecars(self, records: dict[str, dict[str, str]]) -> None:
        for record in records.values():
            path = Path(str(record.get("path", "")))
            if not path.exists():
                continue
            sidecar_path = path.with_suffix(path.suffix + ".job.json")
            try:
                sidecar_path.write_text(json.dumps(record, indent=2, ensure_ascii=False), encoding="utf-8")
            except OSError:
                pass

    def _refresh_loaded_job_ids(self) -> None:
        manifest_records = self._read_loaded_jobs_manifest()
        current_job_ids = {str(job.job_id) for job in self.jobs}
        self.loaded_job_ids = {
            job_id
            for job_id, record in manifest_records.items()
            if job_id in current_job_ids and Path(str(record.get("path", ""))).exists()
        }

    def toggle_loaded_jobs_only(self) -> None:
        if self.show_loaded_jobs_only:
            self.show_loaded_jobs_only = False
            self.loaded_jobs_button_var.set("Geladen Jobs")
            self._refresh_table()
            return
        self._refresh_loaded_job_ids()
        current_loaded_ids = {str(job.job_id) for job in self.jobs if str(job.job_id) in self.loaded_job_ids}
        if not current_loaded_ids:
            messagebox.showinfo("SleufBase Jobs", "Er zijn nog geen geladen GeoTIFF-jobs gevonden.")
            return
        self.show_loaded_jobs_only = True
        self.loaded_jobs_button_var.set("Alle Jobs")
        self._refresh_table()

    def toggle_selected_only(self) -> None:
        if self.show_selected_only:
            self.show_selected_only = False
            self.selected_only_ids.clear()
            self.selected_only_button_var.set("Alleen geselecteerd")
            self._refresh_table()
            return
        selected_ids = {str(job.job_id) for job in self._selected_jobs()}
        if not selected_ids:
            messagebox.showinfo("KickTheMap Jobs", "Selecteer eerst een of meer jobs.")
            return
        self.show_selected_only = True
        self.selected_only_ids = selected_ids
        self.selected_only_button_var.set("Alle jobs")
        self._refresh_table()

    def _sort_by(self, column: str) -> None:
        if self.sort_column == column:
            self.sort_reverse = not self.sort_reverse
        else:
            self.sort_column = column
            self.sort_reverse = False
        self._refresh_table()

    def _selected_job(self) -> KickTheMapJob | None:
        return self.table.selected_job()

    def _selected_jobs(self) -> list[KickTheMapJob]:
        return self.table.selected_jobs()

    def open_jobs_map(self) -> None:
        jobs = self._visible_jobs()
        if not jobs:
            messagebox.showinfo("KickTheMap Jobs Kaart", "Er zijn geen zichtbare jobs om op de kaart te tonen.")
            return
        selected_job_ids = {str(job.job_id) for job in self._selected_jobs()}
        if self.main_frame is not None:
            self.main_frame.pack_forget()
        if self.map_view is not None:
            self.map_view.destroy()
        self.map_view = KickTheMapJobsMapView(
            self,
            jobs,
            selected_job_ids=selected_job_ids,
            back_callback=self.close_jobs_map,
            select_callback=self.select_jobs_by_ids,
            open_callback=self.open_job_from_map,
        )
        self.map_view.pack(fill=tk.BOTH, expand=True)

    def close_jobs_map(self) -> None:
        if self.map_view is not None:
            self.map_view.destroy()
            self.map_view = None
        if self.main_frame is not None:
            self.main_frame.pack(fill=tk.BOTH, expand=True)

    def select_jobs_by_ids(self, job_ids: set[str]) -> None:
        selected_ids = {str(job_id) for job_id in job_ids}
        if self.show_selected_only:
            self.selected_only_ids = set(selected_ids)
            self._refresh_table()
        self.table.selected_indices = {
            index
            for index, job in enumerate(self.table.jobs)
            if str(job.job_id) in selected_ids
        }
        self.table.anchor_index = next(iter(sorted(self.table.selected_indices)), None)
        self.table._draw()
        count = len(self.table.selected_indices)
        if count:
            self.status_var.set(f"{count} job(s) geselecteerd.")
        else:
            self.status_var.set("Geen jobs geselecteerd.")

    def open_job_from_map(self, job: KickTheMapJob) -> None:
        self.select_jobs_by_ids({str(job.job_id)})
        self._open_url(_job_url(job), job.title)

    def open_selected_job(self) -> None:
        job = self._selected_job()
        if job is None:
            messagebox.showinfo("KickTheMap Jobs", "Selecteer eerst een job.")
            return
        self._open_url(_job_url(job), job.title)

    def load_selected_geotiffs(self) -> None:
        jobs = self._selected_jobs()
        if not jobs:
            messagebox.showinfo("KickTheMap Jobs", "Selecteer eerst een of meer jobs.")
            return
        self.status_var.set(f"{len(jobs)} GeoTIFF(s) downloaden...")
        self._set_controls_enabled(False)
        threading.Thread(target=self._load_selected_geotiffs_worker, args=(jobs,), daemon=True).start()

    def _load_selected_geotiffs_worker(self, jobs: list[KickTheMapJob]) -> None:
        total = len(jobs)
        try:
            if self.account is not None and not self.client.is_logged_in:
                self.client.login(self.account.email, self.account.password)
            def progress(completed: int, total: int) -> None:
                self.after(0, lambda completed=completed, total=total: self.status_var.set(f"GeoTIFFs downloaden... {completed} van {total}"))

            path_map, error_map = self.client.download_tiffs(jobs, max_workers=6, progress_callback=progress)
            paths = [str(path_map[job.job_id]) for job in jobs if job.job_id in path_map]
            errors = [
                f"{job.title}: {error_map[job.job_id]}"
                for job in jobs
                if job.job_id in error_map
            ]
            loaded_records = {
                str(job.job_id): {
                    "job_id": str(job.job_id),
                    "title": job.title,
                    "path": str(path_map[job.job_id]),
                }
                for job in jobs
                if job.job_id in path_map
            }
            self.after(0, lambda paths=paths, errors=errors, loaded_records=loaded_records: self._finish_load_geotiffs(paths, errors, loaded_records))
        except Exception as exc:
            self.after(0, lambda exc=exc: self._show_error(exc))

    def _finish_load_geotiffs(self, paths: list[str], errors: list[str], loaded_records: dict[str, dict[str, str]] | None = None) -> None:
        self._set_controls_enabled(True)
        if paths:
            delivered = send_paths_to_running_instance(paths, timeout=2.5)
            if not delivered:
                try:
                    executable, arguments = _main_app_launch_command(paths)
                    subprocess.Popen(
                        [executable] + arguments,
                        cwd=str(Path(__file__).resolve().parent.parent),
                    )
                    delivered = True
                except Exception as exc:
                    errors.append(f"GeoTIFFs openen in programma mislukt: {exc}")
            if delivered:
                records = loaded_records or {}
                self._write_loaded_jobs_manifest(records)
                self._write_loaded_job_sidecars(records)
                self.loaded_job_ids.update(records.keys())
                self._refresh_table()
                self.status_var.set(f"{len(paths)} GeoTIFF(s) geladen in het programma.")
        if errors:
            self.status_var.set(f"{len(paths)} GeoTIFF(s) geladen, {len(errors)} fout(en).")
            messagebox.showwarning("KickTheMap GeoTIFFs", "\n".join(errors[:8]))
        elif not paths:
            self.status_var.set("Geen GeoTIFFs geladen.")

    def _open_url(self, url: str, title: str | None = None) -> None:
        try:
            executable, arguments = _browser_launch_command(url, title)
            subprocess.Popen(
                [executable] + arguments,
                cwd=str(Path(__file__).resolve().parent.parent),
            )
            self.status_var.set("KickTheMap browser geopend.")
        except Exception as exc:
            self.status_var.set("KickTheMap browser openen mislukt.")
            messagebox.showerror("KickTheMap browser openen mislukt", str(exc))


def main() -> None:
    app = KickTheMapJobsWindow()
    app.mainloop()
