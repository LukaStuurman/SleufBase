from __future__ import annotations

import fnmatch
import math
import re
import tkinter as tk
from collections import Counter
from pathlib import Path
from tkinter import filedialog, messagebox, ttk
from typing import Any

from . import settings as settings_module
from .kickthemap_dxf_export import KickTheMapObjectDataset, KickTheMapObjectPoint
from .marxact_import import (
    MarXactImportError,
    MarXactObject,
    MarXactParseResult,
    build_marxact_virtual_layer,
    normalize_marxact_name,
    parse_marxact_dxf,
)
from .settings import (
    KICKTHEMAP_PROFILE_EXTRA_CHOICES_KEY,
    MARXACT_NAME_MAPPINGS_KEY,
    normalize_kickthemap_profile_extra_choices,
    normalize_marxact_name_mappings,
)
from .virtual_trench import VIRTUAL_TRENCH_METADATA_KEY, build_virtual_trench_render

PATCH_VERSION = 1
IGNORE_TARGET = "__IGNORE__"
IGNORE_LABEL = "Negeren"
DEFAULT_TARGET_LABELS = {
    "Water": "Waterleiding",
    "Datatransport": "Datakabel",
    "Gas_lage_druk": "Gasleiding LD",
    "Gas_hoge_druk": "Gasleiding HD",
    "Laagspanning": "Laagspanning",
    "Middenspanning": "Middenspanning",
    "Riool_vrijerval": "Riool vrijverval",
    "Riool_onder_over-_of_onderdruk": "Riool druk",
}


def _profile_options(settings: Any) -> list[dict[str, str]]:
    options: list[dict[str, str]] = []
    seen: set[str] = set()
    for rule in list(getattr(settings, "kickthemap_object_layer_rules", ()) or ()):
        keywords = str(getattr(rule, "keywords", "") or "")
        code = next((v.strip() for v in keywords.replace(";", ",").split(",") if v.strip()), "")
        if not code or code.casefold() in seen:
            continue
        label = str(getattr(rule, "profile_label", "") or "").strip() or code
        seen.add(code.casefold())
        options.append({"code": code, "label": label, "keywords": keywords})
    for choice in normalize_kickthemap_profile_extra_choices(
        getattr(settings, KICKTHEMAP_PROFILE_EXTRA_CHOICES_KEY, [])
    ):
        key = choice.casefold()
        if key in seen or any(key == option["label"].casefold() for option in options):
            continue
        seen.add(key)
        options.append({"code": choice, "label": choice, "keywords": choice})
    return options


def _stored(mappings: dict[str, str], source: str) -> str | None:
    key = normalize_marxact_name(source)
    for name, target in mappings.items():
        if normalize_marxact_name(name) == key:
            return str(target or "").strip() or None
    return None


def _put(mappings: dict[str, str], source: str, target: str) -> None:
    key = normalize_marxact_name(source)
    for name in list(mappings):
        if normalize_marxact_name(name) == key:
            del mappings[name]
    if source.strip() and target.strip():
        mappings[source.strip()] = target.strip()


def _keyword_match(source: str, keywords: str) -> bool:
    source = source.strip().upper()
    for raw in re.split(r"[,;\n]+", keywords):
        keyword = raw.strip().upper()
        if not keyword:
            continue
        if ("*" in keyword or "?" in keyword) and fnmatch.fnmatchcase(source, keyword):
            return True
        if "*" not in keyword and "?" not in keyword and keyword in source:
            return True
    return False


def _option_code(options: list[dict[str, str]], value: str) -> str | None:
    key = value.strip().casefold()
    for option in options:
        if key in {option["code"].casefold(), option["label"].casefold()}:
            return option["code"]
    return None


def _default_target(settings: Any, options: list[dict[str, str]], source: str) -> str:
    key = normalize_marxact_name(source)
    for marxact_name, label in DEFAULT_TARGET_LABELS.items():
        if normalize_marxact_name(marxact_name) == key:
            return _option_code(options, label) or IGNORE_TARGET
    for rule in list(getattr(settings, "kickthemap_object_layer_rules", ()) or ()):
        keywords = str(getattr(rule, "keywords", "") or "")
        if _keyword_match(source, keywords):
            return next((v.strip() for v in keywords.replace(";", ",").split(",") if v.strip()), IGNORE_TARGET)
    return IGNORE_TARGET


def _target(settings: Any, options: list[dict[str, str]], mappings: dict[str, str], source: str) -> str:
    return _stored(mappings, source) or _default_target(settings, options, source)


def _mapping_counts(parsed: MarXactParseResult) -> dict[str, int]:
    counts: Counter[str] = Counter()
    names: dict[str, str] = {}
    for trench in parsed.trenches:
        for item in trench.objects:
            source = item.mapping_name.strip()
            key = normalize_marxact_name(source)
            if key:
                names.setdefault(key, source)
                counts[key] += 1
    return {names[key]: counts[key] for key in sorted(counts, key=lambda value: names[value].casefold())}


def _edit_mappings(
    owner: tk.Misc,
    settings: Any,
    mappings: dict[str, str],
    *,
    detected: dict[str, int] | None = None,
) -> dict[str, str] | None:
    import_mode = detected is not None
    detected = detected or {}
    options = _profile_options(settings)
    labels = [option["label"] for option in options]
    code_to_label = {option["code"].casefold(): option["label"] for option in options}
    label_to_code = {option["label"].casefold(): option["code"] for option in options}
    working = normalize_marxact_name_mappings(mappings)
    sources = list(detected) if import_mode else list(DEFAULT_TARGET_LABELS)
    if not import_mode:
        known = {normalize_marxact_name(v) for v in sources}
        for source in working:
            if normalize_marxact_name(source) not in known:
                sources.append(source)
                known.add(normalize_marxact_name(source))

    dialog = tk.Toplevel(owner)
    dialog.title("MarXact namen koppelen")
    dialog.transient(owner.winfo_toplevel())
    dialog.geometry("800x540")
    dialog.minsize(680, 440)
    try:
        dialog.grab_set()
    except tk.TclError:
        pass
    frame = ttk.Frame(dialog, padding=16)
    frame.pack(fill="both", expand=True)
    frame.columnconfigure(0, weight=1)
    frame.rowconfigure(2, weight=1)
    ttk.Label(frame, text="MarXact namen → Kabel/Leiding", font=("Segoe UI", 11, "bold")).grid(row=0, column=0, sticky="w")
    ttk.Label(
        frame,
        text="Blocks gebruiken hun laagnaam. Alleen bij laag 0 wordt Name uit de MULTILEADER gebruikt. De doelkeuzes zijn dezelfde als in KickTheMap.",
        wraplength=740,
        justify="left",
    ).grid(row=1, column=0, sticky="w", pady=(3, 10))

    tree = ttk.Treeview(frame, columns=("source", "count", "target"), show="headings", selectmode="browse")
    for column, title, width in (("source", "MarXact naam", 330), ("count", "Aantal", 70), ("target", "Kabel/Leiding", 250)):
        tree.heading(column, text=title)
        tree.column(column, width=width, stretch=column != "count", anchor="center" if column == "count" else "w")
    tree.grid(row=2, column=0, sticky="nsew")
    scroll = ttk.Scrollbar(frame, orient="vertical", command=tree.yview)
    scroll.grid(row=2, column=1, sticky="ns")
    tree.configure(yscrollcommand=scroll.set)

    controls = ttk.Frame(frame)
    controls.grid(row=3, column=0, columnspan=2, sticky="ew", pady=(12, 0))
    controls.columnconfigure(0, weight=1)
    source_var = tk.StringVar()
    target_var = tk.StringVar(value=IGNORE_LABEL)
    source_entry = ttk.Entry(controls, textvariable=source_var)
    source_entry.grid(row=0, column=0, sticky="ew")
    combo = ttk.Combobox(controls, textvariable=target_var, values=[IGNORE_LABEL, *labels], state="readonly", width=30)
    combo.grid(row=0, column=1, padx=(8, 0))
    result: dict[str, dict[str, str] | None] = {"value": None}
    iid_names: dict[str, str] = {}

    def label_for(source: str) -> str:
        code = _target(settings, options, working, source)
        if code == IGNORE_TARGET:
            return IGNORE_LABEL
        return code_to_label.get(code.casefold(), code)

    def sync(_event=None) -> None:
        selected = tree.selection()
        if not selected:
            return
        source = iid_names.get(selected[0], "")
        source_var.set(source)
        target_var.set(label_for(source))
        source_entry.configure(state="disabled" if import_mode else "normal")

    def refresh(select: str = "") -> None:
        if not import_mode:
            known = {normalize_marxact_name(v) for v in sources}
            for source in working:
                if normalize_marxact_name(source) not in known:
                    sources.append(source)
                    known.add(normalize_marxact_name(source))
        for iid in tree.get_children():
            tree.delete(iid)
        iid_names.clear()
        wanted = None
        for index, source in enumerate(sorted(sources, key=lambda value: value.casefold())):
            iid = f"m{index}"
            iid_names[iid] = source
            tree.insert("", "end", iid=iid, values=(source, detected.get(source, "") if import_mode else "", label_for(source)))
            if select and normalize_marxact_name(select) == normalize_marxact_name(source):
                wanted = iid
        children = tree.get_children()
        if wanted or children:
            iid = wanted or children[0]
            tree.selection_set(iid)
            tree.focus(iid)
            tree.see(iid)
        sync()

    def apply() -> None:
        source = source_var.get().strip()
        if not source:
            return
        label = target_var.get().strip()
        code = IGNORE_TARGET if label == IGNORE_LABEL else label_to_code.get(label.casefold())
        if not code:
            return
        _put(working, source, code)
        if not import_mode and normalize_marxact_name(source) not in {normalize_marxact_name(v) for v in sources}:
            sources.append(source)
        refresh(source)

    def remove() -> None:
        source = source_var.get().strip()
        if not source:
            return
        if import_mode:
            _put(working, source, IGNORE_TARGET)
            refresh(source)
            return
        key = normalize_marxact_name(source)
        for name in list(working):
            if normalize_marxact_name(name) == key:
                del working[name]
        if key not in {normalize_marxact_name(v) for v in DEFAULT_TARGET_LABELS}:
            sources[:] = [v for v in sources if normalize_marxact_name(v) != key]
        refresh()

    def accept() -> None:
        if import_mode:
            for source in sources:
                if _stored(working, source) is None:
                    _put(working, source, _target(settings, options, working, source))
        result["value"] = normalize_marxact_name_mappings(working)
        dialog.destroy()

    buttons = ttk.Frame(frame)
    buttons.grid(row=4, column=0, columnspan=2, sticky="ew", pady=(12, 0))
    ttk.Button(buttons, text="Toepassen", command=apply).pack(side="left")
    ttk.Button(buttons, text="Negeren" if import_mode else "Verwijderen", command=remove).pack(side="left", padx=(8, 0))
    ttk.Button(buttons, text="Annuleren", command=dialog.destroy).pack(side="right")
    ttk.Button(buttons, text="Importeren" if import_mode else "Gereed", command=accept).pack(side="right", padx=(0, 8))
    tree.bind("<<TreeviewSelect>>", sync)
    source_entry.bind("<Return>", lambda _event: apply())
    combo.bind("<<ComboboxSelected>>", lambda _event: apply())
    dialog.bind("<Escape>", lambda _event: dialog.destroy())
    dialog.protocol("WM_DELETE_WINDOW", dialog.destroy)
    refresh()
    dialog.wait_window()
    return result["value"]


def _resolver(mappings: dict[str, str]):
    lookup = {normalize_marxact_name(k): v for k, v in mappings.items()}

    def resolve(item: MarXactObject) -> str | None:
        value = str(lookup.get(normalize_marxact_name(item.mapping_name), "") or "")
        return None if not value or value == IGNORE_TARGET else value

    return resolve


def _auto_dataset(layer: Any) -> KickTheMapObjectDataset | None:
    payload = layer.metadata.get(VIRTUAL_TRENCH_METADATA_KEY)
    raw_points = payload.get("points") if isinstance(payload, dict) else None
    if not isinstance(raw_points, list):
        return None
    points: list[KickTheMapObjectPoint] = []
    for point in raw_points:
        if not isinstance(point, dict) or str(point.get("role", "")).lower() != "object":
            continue
        try:
            x, y = float(point["x"]), float(point["y"])
        except (KeyError, TypeError, ValueError):
            continue
        try:
            z = None if point.get("z") in (None, "") else float(point["z"])
        except (TypeError, ValueError):
            z = None
        points.append(KickTheMapObjectPoint(
            object_name=str(point.get("object_name", "") or "Object"),
            source_name=str(point.get("source_name", "") or ""), x=x, y=y, z=z,
            attribute_1=str(point.get("attribute_1", "") or ""),
            attribute_2=str(point.get("attribute_2", "") or ""),
            attribute_3=str(point.get("attribute_3", "") or ""),
        ))
    if not points:
        return None
    return KickTheMapObjectDataset(
        job_id=-1,
        job_title=str(layer.metadata.get("marxact_trench_name", "") or layer.path.stem),
        source_path=Path(layer.metadata.get("marxact_source_path", layer.path)),
        points=tuple(points), polylines=(), cross_section_start_xy=None,
    )


def _orient(app: Any, layer: Any) -> None:
    payload = layer.metadata.get(VIRTUAL_TRENCH_METADATA_KEY)
    points = payload.get("points") if isinstance(payload, dict) else None
    if not isinstance(points, list):
        return
    start = next((p for p in points if isinstance(p, dict) and p.get("role") == "start"), None)
    end = next((p for p in points if isinstance(p, dict) and p.get("role") == "end"), None)
    if start is None or end is None:
        return
    dataset = _auto_dataset(layer)
    candidate = None
    if dataset is not None:
        try:
            candidate = app._auto_cross_section_start_candidate_for_layer(
                layer, dataset, app._resolved_cross_section_layer_rules()
            )
        except Exception:
            candidate = None
    if candidate is not None:
        try:
            ds = math.hypot(float(candidate.x) - float(start["x"]), float(candidate.y) - float(start["y"]))
            de = math.hypot(float(candidate.x) - float(end["x"]), float(candidate.y) - float(end["y"]))
            if de < ds:
                start["role"], end["role"] = "end", "start"
                start, end = end, start
        except (AttributeError, KeyError, TypeError, ValueError):
            pass
    try:
        setter = getattr(app, "_set_automatic_template_cross_section_start_metadata", None)
        if not callable(setter):
            setter = getattr(app, "_set_template_cross_section_start_metadata", None)
        if callable(setter):
            setter(layer, float(start["x"]), float(start["y"]))
    except Exception:
        pass
    try:
        layer.image, layer.bounds, layer.transform = build_virtual_trench_render(layer)
        layer.invalidate_native_rgba_cache()
    except Exception:
        pass


def _widget_text(widget: tk.Misc) -> str:
    try:
        return str(widget.cget("text") or "")
    except (AttributeError, tk.TclError):
        return ""


def _descendants(widget: tk.Misc):
    for child in widget.winfo_children():
        yield child
        yield from _descendants(child)


def _next_row(parent: tk.Misc) -> int:
    rows = []
    try:
        for child in parent.grid_slaves():
            info = child.grid_info()
            rows.append(int(info.get("row", 0)) + max(1, int(info.get("rowspan", 1))))
    except (TypeError, ValueError, tk.TclError):
        pass
    return max(rows, default=0)


def _settings_row(app: Any, dialog: tk.Misc) -> None:
    if getattr(dialog, "_marxact_settings_row", False):
        return
    widgets = list(_descendants(dialog))
    general = next((w for w in widgets if isinstance(w, ttk.LabelFrame) and (
        str(getattr(w, "_settings_original_section_title", "") or "") == "Algemeen"
        or _widget_text(w) == "Algemeen"
    )), None)
    rules = next((w for w in widgets if isinstance(w, ttk.LabelFrame) and (
        str(getattr(w, "_settings_original_section_title", "") or "") in {"KickTheMap woord-naar-laag", "KickTheMap kabeltype, materiaal en DXF-laag"}
        or _widget_text(w) in {"KickTheMap woord-naar-laag", "KickTheMap kabeltype, materiaal en DXF-laag", "KickTheMap – kabels"}
    )), None)
    save = next((w for w in widgets if _widget_text(w) == "Opslaan"), None)
    parent = general or rules
    if parent is None or save is None:
        return
    working = normalize_marxact_name_mappings(getattr(app.settings, MARXACT_NAME_MAPPINGS_KEY, {}))
    count = tk.StringVar(value=f"{len(working)} opgeslagen koppeling(en)")

    def edit() -> None:
        nonlocal working
        value = _edit_mappings(dialog, app.settings, working)
        if value is not None:
            working = value
            count.set(f"{len(working)} opgeslagen koppeling(en)")

    box = ttk.Frame(parent)
    box.grid(row=_next_row(parent), column=0, sticky="ew", pady=(10, 2))
    box.columnconfigure(0, weight=1)
    ttk.Separator(box, orient="horizontal").grid(row=0, column=0, columnspan=2, sticky="ew", pady=(0, 8))
    ttk.Label(box, text="MarXact namen", font=("Segoe UI", 10, "bold")).grid(row=1, column=0, sticky="w")
    ttk.Button(box, text="Namen koppelen…", command=edit).grid(row=1, column=1, sticky="e", padx=(12, 0))
    ttk.Label(box, textvariable=count).grid(row=2, column=0, columnspan=2, sticky="w", pady=(2, 0))
    ttk.Label(box, text="Blocklaag bepaalt het type; bij laag 0 wordt MULTILEADER Name gebruikt.", wraplength=620).grid(row=3, column=0, columnspan=2, sticky="w", pady=(2, 0))
    original = save.cget("command")

    def save_all():
        setattr(app.settings, MARXACT_NAME_MAPPINGS_KEY, normalize_marxact_name_mappings(working))
        if callable(original):
            return original()
        return dialog.tk.call(str(original))

    save.configure(command=save_all)
    dialog._marxact_settings_row = True


def _add_menu(app: Any) -> None:
    try:
        bar = app.nametowidget(app.cget("menu"))
        end = bar.index("end")
    except Exception:
        return
    target = None
    if end is not None:
        for index in range(end + 1):
            try:
                if bar.type(index) == "cascade" and str(bar.entrycget(index, "label") or "") in {"Bestand", "File", "Project"}:
                    target = app.nametowidget(bar.entrycget(index, "menu"))
                    break
            except Exception:
                pass
    if target is None:
        target = tk.Menu(bar, tearoff=0)
        target.add_command(label="marXact import", command=app.import_marxact_dxf)
        bar.add_cascade(label="marXact", menu=target)
    else:
        try:
            last = target.index("end")
            if last is not None:
                for index in range(last, -1, -1):
                    if target.type(index) == "command" and target.entrycget(index, "label") == "marXact import":
                        target.delete(index)
                target.add_separator()
        except Exception:
            pass
        target.add_command(label="marXact import", command=app.import_marxact_dxf)
    try:
        if hasattr(app, "_capture_modern_menu_specs"):
            app._modern_menu_specs = app._capture_modern_menu_specs(bar)
            if getattr(app, "_modern_menu_bar", None) is not None:
                app.after(0, app._build_modern_menu_bar)
    except Exception:
        pass


def _refresh(app: Any) -> None:
    try:
        app._refresh_map_edit_markers()
    except Exception:
        pass
    for name in ("_refresh_tiff_list", "_refresh_layer_list", "_update_tiff_list", "_update_layer_list"):
        callback = getattr(app, name, None)
        if callable(callback):
            try:
                callback()
                break
            except Exception:
                pass
    callback = getattr(app, "request_render", None)
    if callable(callback):
        for kwargs in ({"refetch_background": False}, {"immediate": False}, {}):
            try:
                callback(**kwargs)
                break
            except TypeError:
                continue
            except Exception:
                break


def _patch_viewer_class(viewer_class: type) -> None:
    if int(getattr(viewer_class, "_marxact_import_patch_version", 0) or 0) >= PATCH_VERSION:
        return
    original_menu = viewer_class._build_menu
    original_settings = viewer_class.open_settings_dialog

    def build_menu(self) -> None:
        original_menu(self)
        _add_menu(self)

    def open_settings(self) -> None:
        existing = {str(child) for child in self.winfo_children() if isinstance(child, tk.Toplevel)}
        original_settings(self)
        dialog = next((child for child in self.winfo_children() if isinstance(child, tk.Toplevel) and str(child) not in existing and child.title() == "Instellingen"), None)
        if dialog is None:
            return
        # settings_ui/general_layout runs delayed passes up to 400 ms and hides
        # the legacy KickTheMap rule panel. Add MarXact afterwards so the row is
        # placed in the visible General panel next to the KickTheMap controls.
        for delay in (450, 700):
            try:
                dialog.after(delay, lambda target=dialog: _settings_row(self, target))
            except tk.TclError:
                break

    def import_marxact_dxf(self) -> None:
        source = filedialog.askopenfilename(parent=self, title="marXact import", filetypes=(("DXF-bestanden", "*.dxf"), ("Alle bestanden", "*.*")))
        if not source:
            return
        self.set_status("MarXact-DXF analyseren...")
        try:
            parsed = parse_marxact_dxf(source)
        except Exception as exc:
            error = exc if isinstance(exc, MarXactImportError) else MarXactImportError(str(exc))
            messagebox.showerror("marXact import", str(error), parent=self)
            self.set_status("marXact import mislukt.")
            return
        counts = _mapping_counts(parsed)
        current = normalize_marxact_name_mappings(getattr(self.settings, MARXACT_NAME_MAPPINGS_KEY, {}))
        mappings = _edit_mappings(self, self.settings, current, detected=counts)
        if mappings is None:
            self.set_status("marXact import geannuleerd.")
            return
        setattr(self.settings, MARXACT_NAME_MAPPINGS_KEY, mappings)
        try:
            settings_module.save_settings(self.settings)
        except Exception as exc:
            messagebox.showerror("marXact import", f"MarXact-koppelingen opslaan mislukt: {exc}", parent=self)
            return

        resolver = _resolver(mappings)
        occurrences: Counter[str] = Counter()
        layers = []
        try:
            for index, trench in enumerate(parsed.trenches, start=1):
                key = normalize_marxact_name(trench.name) or f"#{index}"
                occurrences[key] += 1
                self.set_status(f"MarXact proefsleuven opbouwen ({index}/{len(parsed.trenches)}): {trench.name}")
                layer = build_marxact_virtual_layer(
                    trench,
                    source_path=parsed.source_path,
                    source_name_resolver=resolver,
                    fallback_index=index,
                    occurrence=occurrences[key],
                )
                _orient(self, layer)
                layers.append(layer)
        except Exception as exc:
            messagebox.showerror("marXact import", f"Proefsleuf opbouwen mislukt: {exc}", parent=self)
            self.set_status("marXact import afgebroken.")
            return
        if not layers:
            messagebox.showwarning("marXact import", "Geen bruikbare proefsleuven gevonden.", parent=self)
            return
        self.tiff_layers.extend(layers)
        _refresh(self)
        ignored = sum(count for source_name, count in counts.items() if _stored(mappings, source_name) == IGNORE_TARGET)
        self.set_status(f"marXact import gereed: {len(layers)} proefsleuven.")
        messagebox.showinfo(
            "marXact import",
            f"{len(layers)} virtuele proefsleuven geïmporteerd.\n{parsed.assigned_insert_count} blocks gevonden; {ignored} genegeerd via de MarXact-koppeling.",
            parent=self,
        )

    viewer_class.import_marxact_dxf = import_marxact_dxf
    viewer_class._build_menu = build_menu
    viewer_class.open_settings_dialog = open_settings
    viewer_class._marxact_import_patch = True
    viewer_class._marxact_import_patch_version = PATCH_VERSION


def install_marxact_import_patch() -> None:
    from .app import KlicViewerApp

    _patch_viewer_class(KlicViewerApp)
