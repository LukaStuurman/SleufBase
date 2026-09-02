from __future__ import annotations

import tkinter as tk
from pathlib import Path
from tkinter import messagebox, ttk
from typing import Any

from .marxact_direction_patch import (
    alignment_rotation_degrees,
    apply_virtual_layer_alignment_rotation,
    recalculate_virtual_layer_automatic_alignment,
)
from .virtual_trench import VIRTUAL_TRENCH_METADATA_KEY, refresh_virtual_trench_layer


PATCH_VERSION = 1


def _is_marxact_layer(layer: Any) -> bool:
    metadata = getattr(layer, "metadata", None)
    if not isinstance(metadata, dict):
        return False
    if metadata.get("marxact_source_path"):
        return True
    payload = metadata.get(VIRTUAL_TRENCH_METADATA_KEY)
    return isinstance(payload, dict) and str(payload.get("source", "")).casefold() == "marxact"


def _marxact_layers(app: Any) -> list[Any]:
    return [
        layer
        for layer in list(getattr(app, "tiff_layers", ()) or ())
        if _is_marxact_layer(layer)
    ]


def _layer_label(layer: Any, index: int) -> str:
    metadata = getattr(layer, "metadata", {}) or {}
    name = str(
        metadata.get("marxact_trench_name")
        or metadata.get("template_proefsleuf_label")
        or getattr(layer, "name", "")
        or getattr(getattr(layer, "path", None), "stem", "")
        or f"Proefsleuf {index + 1}"
    ).strip()
    source = str(metadata.get("marxact_source_path", "") or "").strip()
    source_name = Path(source).name if source else ""
    prefix = f"{index + 1}. {name}"
    return f"{prefix} — {source_name}" if source_name else prefix


def _refresh_layer(app: Any, layer: Any) -> None:
    old_image = getattr(layer, "image", None)
    try:
        refresh_virtual_trench_layer(layer)
        invalidate = getattr(layer, "invalidate_native_rgba_cache", None)
        if callable(invalidate):
            invalidate()
    finally:
        new_image = getattr(layer, "image", None)
        if old_image is not None and old_image is not new_image:
            try:
                old_image.close()
            except Exception:
                pass
    request_render = getattr(app, "request_render", None)
    if callable(request_render):
        try:
            request_render(False)
        except TypeError:
            request_render(immediate=False)


def _patch_viewer_class(viewer_class: type[Any]) -> None:
    if int(getattr(viewer_class, "_marxact_alignment_ui_patch_version", 0) or 0) >= PATCH_VERSION:
        return

    original_build_menu = viewer_class._build_menu

    def show_marxact_alignment_dialog(self) -> None:
        layers = _marxact_layers(self)
        if not layers:
            messagebox.showinfo(
                "MarXact oriëntatie",
                "Er zijn geen geladen MarXact-proefsleuven.",
                parent=self,
            )
            return

        dialog = tk.Toplevel(self)
        dialog.title("MarXact kabel-/leidingoriëntatie")
        dialog.transient(self)
        dialog.geometry("650x310")
        dialog.minsize(590, 290)
        try:
            dialog.grab_set()
        except tk.TclError:
            pass

        frame = ttk.Frame(dialog, padding=16)
        frame.pack(fill="both", expand=True)
        frame.columnconfigure(1, weight=1)

        ttk.Label(
            frame,
            text="MarXact kabel-/leidingoriëntatie",
            font=("Segoe UI", 11, "bold"),
        ).grid(row=0, column=0, columnspan=3, sticky="w")
        ttk.Label(
            frame,
            text=(
                "De automatische richting gebruikt de gemeten blocks en wordt bij een kleine "
                "hoekafwijking op de echte 3D-POLYLINE-randen gestabiliseerd. Gebruik de "
                "rotatiecorrectie alleen als je hem visueel nog iets wilt bijdraaien."
            ),
            wraplength=610,
            justify="left",
        ).grid(row=1, column=0, columnspan=3, sticky="ew", pady=(4, 12))

        labels = [_layer_label(layer, index) for index, layer in enumerate(layers)]
        label_to_layer = {label: layer for label, layer in zip(labels, layers)}
        selected_var = tk.StringVar(value=labels[0])
        rotation_var = tk.DoubleVar(value=alignment_rotation_degrees(layers[0]))
        degree_text = tk.StringVar()
        pending_after: dict[str, str | None] = {"id": None}
        changing_selection = {"active": False}

        ttk.Label(frame, text="Proefsleuf").grid(row=2, column=0, sticky="w", padx=(0, 10))
        layer_combo = ttk.Combobox(
            frame,
            textvariable=selected_var,
            values=labels,
            state="readonly",
        )
        layer_combo.grid(row=2, column=1, columnspan=2, sticky="ew")

        ttk.Label(frame, text="Rotatiecorrectie").grid(
            row=3, column=0, sticky="w", padx=(0, 10), pady=(18, 0)
        )
        slider = ttk.Scale(
            frame,
            from_=-90.0,
            to=90.0,
            variable=rotation_var,
        )
        slider.grid(row=3, column=1, sticky="ew", pady=(18, 0))
        spinbox = ttk.Spinbox(
            frame,
            from_=-90.0,
            to=90.0,
            increment=0.1,
            textvariable=rotation_var,
            width=8,
            justify="right",
        )
        spinbox.grid(row=3, column=2, sticky="e", padx=(10, 0), pady=(18, 0))

        value_label = ttk.Label(frame, textvariable=degree_text)
        value_label.grid(row=4, column=1, columnspan=2, sticky="w", pady=(3, 0))

        button_row = ttk.Frame(frame)
        button_row.grid(row=5, column=0, columnspan=3, sticky="ew", pady=(18, 0))
        button_row.columnconfigure(5, weight=1)

        def selected_layer() -> Any | None:
            return label_to_layer.get(selected_var.get())

        def sync_degree_text(*_args) -> None:
            try:
                value = float(rotation_var.get())
            except (TypeError, ValueError, tk.TclError):
                value = 0.0
            degree_text.set(f"{value:+.1f}° ten opzichte van automatisch")

        def apply_now(*, status: bool = True) -> None:
            if changing_selection["active"]:
                return
            layer = selected_layer()
            if layer is None:
                return
            try:
                rotation = max(-90.0, min(90.0, float(rotation_var.get())))
            except (TypeError, ValueError, tk.TclError):
                return
            rotation_var.set(rotation)
            sync_degree_text()
            if not apply_virtual_layer_alignment_rotation(layer, rotation):
                if status:
                    messagebox.showwarning(
                        "MarXact oriëntatie",
                        "De uitlijning van deze proefsleuf kon niet worden aangepast.",
                        parent=dialog,
                    )
                return
            _refresh_layer(self, layer)
            if status:
                name = str(
                    getattr(layer, "metadata", {}).get("marxact_trench_name", "")
                    or getattr(getattr(layer, "path", None), "stem", "MarXact")
                )
                setter = getattr(self, "set_status", None)
                if callable(setter):
                    setter(f"MarXact oriëntatie {name}: {rotation:+.1f}°.")

        def schedule_apply(_value: object = None) -> None:
            if changing_selection["active"]:
                return
            sync_degree_text()
            after_id = pending_after["id"]
            if after_id is not None:
                try:
                    dialog.after_cancel(after_id)
                except tk.TclError:
                    pass
            try:
                pending_after["id"] = dialog.after(
                    90, lambda: (pending_after.__setitem__("id", None), apply_now(status=False))
                )
            except tk.TclError:
                pending_after["id"] = None

        def nudge(delta: float) -> None:
            try:
                current = float(rotation_var.get())
            except (TypeError, ValueError, tk.TclError):
                current = 0.0
            rotation_var.set(max(-90.0, min(90.0, current + float(delta))))
            apply_now(status=False)

        def select_layer(_event=None) -> None:
            layer = selected_layer()
            if layer is None:
                return
            changing_selection["active"] = True
            try:
                rotation_var.set(alignment_rotation_degrees(layer))
                sync_degree_text()
            finally:
                changing_selection["active"] = False

        def recalculate() -> None:
            layer = selected_layer()
            if layer is None:
                return
            if not recalculate_virtual_layer_automatic_alignment(layer):
                messagebox.showwarning(
                    "MarXact oriëntatie",
                    "Automatisch herberekenen is voor deze proefsleuf mislukt.",
                    parent=dialog,
                )
                return
            rotation_var.set(0.0)
            sync_degree_text()
            _refresh_layer(self, layer)
            setter = getattr(self, "set_status", None)
            if callable(setter):
                setter("MarXact oriëntatie automatisch opnieuw bepaald.")

        def reset_rotation() -> None:
            rotation_var.set(0.0)
            apply_now(status=False)

        def close_dialog() -> None:
            after_id = pending_after["id"]
            if after_id is not None:
                try:
                    dialog.after_cancel(after_id)
                except tk.TclError:
                    pass
            dialog.destroy()

        ttk.Button(button_row, text="−1°", command=lambda: nudge(-1.0)).grid(row=0, column=0)
        ttk.Button(button_row, text="−0,1°", command=lambda: nudge(-0.1)).grid(
            row=0, column=1, padx=(6, 0)
        )
        ttk.Button(button_row, text="+0,1°", command=lambda: nudge(0.1)).grid(
            row=0, column=2, padx=(6, 0)
        )
        ttk.Button(button_row, text="+1°", command=lambda: nudge(1.0)).grid(
            row=0, column=3, padx=(6, 0)
        )
        ttk.Button(button_row, text="Correctie 0°", command=reset_rotation).grid(
            row=0, column=4, padx=(10, 0)
        )
        ttk.Button(
            frame,
            text="Automatisch opnieuw bepalen",
            command=recalculate,
        ).grid(row=6, column=0, columnspan=2, sticky="w", pady=(12, 0))
        ttk.Button(frame, text="Sluiten", command=close_dialog).grid(
            row=6, column=2, sticky="e", pady=(12, 0)
        )

        slider.configure(command=schedule_apply)
        layer_combo.bind("<<ComboboxSelected>>", select_layer)
        spinbox.bind("<Return>", lambda _event: apply_now())
        spinbox.bind("<FocusOut>", lambda _event: apply_now(status=False))
        rotation_var.trace_add("write", lambda *_args: sync_degree_text())
        dialog.bind("<Escape>", lambda _event: close_dialog())
        dialog.protocol("WM_DELETE_WINDOW", close_dialog)
        sync_degree_text()

    def _build_menu_with_marxact_alignment(self) -> None:
        original_build_menu(self)
        try:
            menu_bar = self.nametowidget(self.cget("menu"))
            end_index = menu_bar.index("end")
            if end_index is None:
                return
            target = None
            for index in range(end_index + 1):
                if menu_bar.type(index) != "cascade":
                    continue
                if str(menu_bar.entrycget(index, "label")).casefold() != "marxact":
                    continue
                target = self.nametowidget(menu_bar.entrycget(index, "menu"))
                break
            if target is None:
                return

            existing_labels: set[str] = set()
            target_end = target.index("end")
            if target_end is not None:
                for index in range(target_end + 1):
                    if target.type(index) == "command":
                        existing_labels.add(str(target.entrycget(index, "label")))
            if "Oriëntatie aanpassen..." in existing_labels:
                return
            if target_end is not None:
                target.add_separator()
            target.add_command(
                label="Oriëntatie aanpassen...",
                command=self.show_marxact_alignment_dialog,
            )
        except Exception:
            return

    viewer_class.show_marxact_alignment_dialog = show_marxact_alignment_dialog
    viewer_class._build_menu = _build_menu_with_marxact_alignment
    viewer_class._marxact_alignment_ui_patch_version = PATCH_VERSION


def install_marxact_alignment_ui_patch() -> None:
    from .app import KlicViewerApp

    _patch_viewer_class(KlicViewerApp)


def install_marxact_alignment_ui_hook() -> None:
    """Chain the UI patch onto the existing MarXact runtime installer."""

    from . import marxact_import_patch as import_patch

    if bool(getattr(import_patch, "_marxact_alignment_ui_hook_installed", False)):
        return
    original_install = import_patch.install_marxact_import_patch

    def install_marxact_import_and_alignment_patch() -> None:
        original_install()
        install_marxact_alignment_ui_patch()

    import_patch.install_marxact_import_patch = install_marxact_import_and_alignment_patch
    import_patch._marxact_alignment_ui_hook_installed = True
