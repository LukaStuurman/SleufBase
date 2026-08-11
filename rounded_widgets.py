from __future__ import annotations

import tkinter as tk
from tkinter import font as tkfont
from tkinter import ttk


_ORIGINAL_TK_BUTTON = tk.Button
_ORIGINAL_TTK_BUTTON = ttk.Button
_ORIGINAL_TK_ENTRY = tk.Entry
_ORIGINAL_TTK_ENTRY = ttk.Entry
_ORIGINAL_TK_CHECKBUTTON = tk.Checkbutton
_ORIGINAL_TTK_CHECKBUTTON = ttk.Checkbutton
_INSTALLED = False


def _option(widget: tk.Widget, name: str, fallback: str) -> str:
    try:
        value = widget.cget(name)
        return str(value) if value else fallback
    except tk.TclError:
        return fallback


class RoundedButton(tk.Canvas):
    def __init__(self, master=None, **kwargs) -> None:
        self._text = str(kwargs.pop("text", ""))
        self._textvariable = kwargs.pop("textvariable", None)
        self._command = kwargs.pop("command", None)
        self._style = str(kwargs.pop("style", "TButton") or "TButton")
        self._state = str(kwargs.pop("state", tk.NORMAL))
        self._width_chars = kwargs.pop("width", None)
        self._height_chars = kwargs.pop("height", None)
        self._padding = kwargs.pop("padding", None)
        self._image = kwargs.pop("image", None)
        self._compound = kwargs.pop("compound", tk.LEFT)
        self._takefocus = kwargs.pop("takefocus", True)
        for unsupported in (
            "default",
            "underline",
            "overrelief",
            "activebackground",
            "activeforeground",
            "disabledforeground",
            "repeatdelay",
            "repeatinterval",
        ):
            kwargs.pop(unsupported, None)

        background = kwargs.pop("background", None) or kwargs.pop("bg", None) or self._parent_bg(master)
        super().__init__(
            master,
            highlightthickness=0,
            bd=0,
            background=background,
            takefocus=1 if self._takefocus else 0,
            **kwargs,
        )

        self._pressed = False
        self._hover = False
        self._font = tkfont.nametofont("TkDefaultFont")
        self._textvariable_trace = None
        if self._textvariable is not None:
            try:
                self._text = str(self._textvariable.get())
                self._textvariable_trace = self._textvariable.trace_add("write", self._on_textvariable_changed)
            except Exception:
                self._textvariable = None
        self._bind_events()
        self._redraw()

    @staticmethod
    def _parent_bg(master) -> str:
        if master is None:
            return "#f0f0f0"
        background = _option(master, "background", "")
        if background:
            return background
        try:
            style_name = str(master.cget("style") or "")
            if style_name:
                style_background = ttk.Style(master).lookup(style_name, "background", default="")
                if style_background:
                    return str(style_background)
        except tk.TclError:
            pass
        try:
            class_background = ttk.Style(master).lookup(master.winfo_class(), "background", default="")
            if class_background:
                return str(class_background)
        except tk.TclError:
            pass
        return "#f0f0f0"

    def _bind_events(self) -> None:
        self.bind("<Enter>", self._on_enter)
        self.bind("<Leave>", self._on_leave)
        self.bind("<ButtonPress-1>", self._on_press)
        self.bind("<ButtonRelease-1>", self._on_release)
        self.bind("<space>", lambda _event: self.invoke())
        self.bind("<Return>", lambda _event: self.invoke())
        self.bind("<Configure>", lambda _event: self._redraw())

    def _style_value(self, option: str, fallback: str) -> str:
        try:
            value = ttk.Style(self).lookup(self._style, option, default="")
            return str(value) if value else fallback
        except tk.TclError:
            return fallback

    def _colors(self) -> tuple[str, str, str]:
        if self._state == tk.DISABLED:
            return "#e5e7eb", "#9ca3af", "#e5e7eb"
        bg = self._style_value("background", "#ffffff")
        fg = self._style_value("foreground", "#111827")
        border = bg
        if "Accent" in self._style:
            bg = bg if bg not in {"", "#ffffff"} else "#f97316"
            fg = fg if fg else "#ffffff"
            border = bg
        if self._pressed:
            bg = "#c2410c" if "Accent" in self._style else "#e5e7eb"
        elif self._hover:
            bg = "#fdba74" if "Accent" in self._style else "#f3f4f6"
        border = bg
        return bg, fg, border

    def _padding_xy(self) -> tuple[int, int]:
        if isinstance(self._padding, tuple) and len(self._padding) >= 2:
            return int(self._padding[0]), int(self._padding[1])
        if "Compact" in self._style:
            return 10, 6
        if "Accent" in self._style:
            return 14, 8
        return 12, 7

    def _requested_size(self) -> tuple[int, int]:
        pad_x, pad_y = self._padding_xy()
        text_width = self._font.measure(self._text)
        if self._width_chars is not None:
            try:
                text_width = max(text_width, self._font.measure("0") * int(self._width_chars))
            except (TypeError, ValueError):
                pass
        image_width = int(self._image.width()) if self._image is not None and hasattr(self._image, "width") else 0
        image_height = int(self._image.height()) if self._image is not None and hasattr(self._image, "height") else 0
        width = max(28, text_width + image_width + pad_x * 2 + (6 if text_width and image_width else 0))
        height = max(28, max(self._font.metrics("linespace"), image_height) + pad_y * 2)
        if self._height_chars is not None:
            try:
                height = max(height, self._font.metrics("linespace") * int(self._height_chars))
            except (TypeError, ValueError):
                pass
        return width, height

    def _rounded_rect(self, x1: int, y1: int, x2: int, y2: int, radius: int, **kwargs) -> None:
        radius = max(1, min(radius, (x2 - x1) // 2, (y2 - y1) // 2))
        points = [
            x1 + radius, y1, x2 - radius, y1, x2, y1, x2, y1 + radius,
            x2, y2 - radius, x2, y2, x2 - radius, y2, x1 + radius, y2,
            x1, y2, x1, y2 - radius, x1, y1 + radius, x1, y1,
        ]
        self.create_polygon(points, smooth=True, splinesteps=12, **kwargs)

    def _redraw(self) -> None:
        self.delete("all")
        req_width, req_height = self._requested_size()
        tk.Canvas.configure(self, width=req_width, height=req_height)
        width = max(req_width, self.winfo_width() or req_width)
        height = max(req_height, self.winfo_height() or req_height)
        bg, fg, border = self._colors()
        tk.Canvas.configure(self, background=self._parent_bg(self.master))
        self._rounded_rect(0, 0, width, height, 13, fill=bg, outline=border)
        self.create_text(width / 2, height / 2, text=self._text, fill=fg, font=self._font)

    def _on_enter(self, _event) -> None:
        self._hover = True
        self._redraw()

    def _on_leave(self, _event) -> None:
        self._hover = False
        self._pressed = False
        self._redraw()

    def _on_press(self, _event) -> None:
        if self._state == tk.DISABLED:
            return
        self._pressed = True
        self._redraw()

    def _on_release(self, _event) -> None:
        if self._state == tk.DISABLED:
            return
        was_pressed = self._pressed
        self._pressed = False
        self._redraw()
        if was_pressed:
            self.invoke()

    def _on_textvariable_changed(self, *_args) -> None:
        if self._textvariable is None:
            return
        try:
            self._text = str(self._textvariable.get())
        except Exception:
            return
        self._redraw()

    def invoke(self):
        if self._state == tk.DISABLED or self._command is None:
            return None
        return self._command()

    def state(self, statespec=None):
        if statespec is None:
            return ("disabled",) if self._state == tk.DISABLED else ()
        if isinstance(statespec, str):
            specs = (statespec,)
        else:
            specs = tuple(statespec)
        previous = self.state()
        for spec in specs:
            if spec == "disabled":
                self._state = tk.DISABLED
            elif spec == "!disabled":
                self._state = tk.NORMAL
        self._redraw()
        return previous

    def instate(self, statespec, callback=None, *args):
        if isinstance(statespec, str):
            specs = (statespec,)
        else:
            specs = tuple(statespec)
        disabled = self._state == tk.DISABLED
        matches = all((spec == "disabled" and disabled) or (spec == "!disabled" and not disabled) for spec in specs)
        if matches and callback is not None:
            return callback(*args)
        return matches

    def configure(self, cnf=None, **kwargs):  # type: ignore[override]
        options = {}
        if cnf:
            options.update(cnf)
        options.update(kwargs)
        redraw = False
        for key in ("text", "textvariable", "command", "style", "state", "width", "height", "padding"):
            if key not in options:
                continue
            value = options.pop(key)
            if key == "text":
                self._text = str(value)
            elif key == "textvariable":
                if self._textvariable is not None and self._textvariable_trace is not None:
                    try:
                        self._textvariable.trace_remove("write", self._textvariable_trace)
                    except Exception:
                        pass
                self._textvariable = value
                self._textvariable_trace = None
                if self._textvariable is not None:
                    try:
                        self._text = str(self._textvariable.get())
                        self._textvariable_trace = self._textvariable.trace_add("write", self._on_textvariable_changed)
                    except Exception:
                        self._textvariable = None
            elif key == "command":
                self._command = value
            elif key == "style":
                self._style = str(value or "TButton")
            elif key == "state":
                self._state = str(value)
            elif key == "width":
                self._width_chars = value
            elif key == "height":
                self._height_chars = value
            elif key == "padding":
                self._padding = value
            redraw = True
        for unsupported in (
            "default",
            "underline",
            "overrelief",
            "activebackground",
            "activeforeground",
            "disabledforeground",
            "repeatdelay",
            "repeatinterval",
        ):
            options.pop(unsupported, None)
        result = super().configure(**options) if options else None
        if redraw:
            self._redraw()
        return result

    config = configure

    def cget(self, key):  # type: ignore[override]
        if key == "text":
            return self._text
        if key == "textvariable":
            return self._textvariable
        if key == "command":
            return self._command
        if key == "style":
            return self._style
        if key == "state":
            return self._state
        return super().cget(key)


class RoundedEntry(tk.Frame):
    def __init__(self, master=None, **kwargs) -> None:
        self._style = str(kwargs.pop("style", "TEntry") or "TEntry")
        self._width_chars = kwargs.get("width", 20)
        self._padding = kwargs.pop("padding", 5)
        self._entry_options = dict(kwargs)

        background = kwargs.pop("background", None) or kwargs.pop("bg", None) or RoundedButton._parent_bg(master)
        super().__init__(master, background=background, highlightthickness=0, bd=0)

        self._canvas = tk.Canvas(self, highlightthickness=0, bd=0, background=background)
        self._canvas.place(x=0, y=0, relwidth=1, relheight=1)

        entry_kwargs = dict(kwargs)
        entry_kwargs.pop("style", None)
        entry_kwargs.pop("padding", None)
        entry_kwargs.pop("borderwidth", None)
        entry_kwargs.pop("bd", None)
        entry_kwargs.pop("relief", None)
        entry_kwargs.pop("highlightthickness", None)
        entry_kwargs["borderwidth"] = 0
        entry_kwargs["highlightthickness"] = 0
        entry_kwargs["relief"] = tk.FLAT
        entry_kwargs["background"] = self._field_background()
        entry_kwargs["foreground"] = self._style_value("foreground", "#111827")
        entry_kwargs["insertbackground"] = entry_kwargs["foreground"]
        entry_kwargs["selectbackground"] = "#bfdbfe"
        entry_kwargs["selectforeground"] = "#111827"
        self._entry = _ORIGINAL_TK_ENTRY(self, **entry_kwargs)
        self._entry.place(x=self._pad_x(), y=self._pad_y())

        self.bind("<Configure>", lambda _event: self._layout())
        self._entry.bind("<FocusIn>", lambda _event: self._redraw())
        self._entry.bind("<FocusOut>", lambda _event: self._redraw())
        self._layout()

    def _style_value(self, option: str, fallback: str) -> str:
        try:
            value = ttk.Style(self).lookup(self._style, option, default="")
            return str(value) if value else fallback
        except tk.TclError:
            return fallback

    def _field_background(self) -> str:
        return self._style_value("fieldbackground", self._style_value("background", "#ffffff"))

    def _pad_x(self) -> int:
        if isinstance(self._padding, tuple) and self._padding:
            return int(self._padding[0])
        try:
            return int(self._padding)
        except (TypeError, ValueError):
            return 5

    def _pad_y(self) -> int:
        if isinstance(self._padding, tuple) and len(self._padding) > 1:
            return int(self._padding[1])
        return 5

    def _requested_size(self) -> tuple[int, int]:
        try:
            font = tkfont.nametofont(str(self._entry.cget("font")) or "TkDefaultFont")
        except tk.TclError:
            font = tkfont.nametofont("TkDefaultFont")
        try:
            width_chars = int(self._width_chars)
        except (TypeError, ValueError):
            width_chars = 20
        width = max(36, font.measure("0") * width_chars + (2 * self._pad_x()) + 4)
        height = max(28, font.metrics("linespace") + (2 * self._pad_y()) + 4)
        return width, height

    def _rounded_rect(self, x1: int, y1: int, x2: int, y2: int, radius: int, **kwargs) -> None:
        radius = max(1, min(radius, (x2 - x1) // 2, (y2 - y1) // 2))
        points = [
            x1 + radius, y1, x2 - radius, y1, x2, y1, x2, y1 + radius,
            x2, y2 - radius, x2, y2, x2 - radius, y2, x1 + radius, y2,
            x1, y2, x1, y2 - radius, x1, y1 + radius, x1, y1,
        ]
        self._canvas.create_polygon(points, smooth=True, splinesteps=12, **kwargs)

    def _layout(self) -> None:
        req_width, req_height = self._requested_size()
        tk.Frame.configure(self, width=req_width, height=req_height)
        width = max(req_width, self.winfo_width() or req_width)
        height = max(req_height, self.winfo_height() or req_height)
        self._entry.place_configure(
            x=self._pad_x(),
            y=self._pad_y(),
            width=max(8, width - (2 * self._pad_x())),
            height=max(8, height - (2 * self._pad_y())),
        )
        self._redraw()

    def _redraw(self) -> None:
        width = max(1, self.winfo_width())
        height = max(1, self.winfo_height())
        parent_bg = RoundedButton._parent_bg(self.master)
        bg = self._field_background()
        border = bg
        tk.Frame.configure(self, background=parent_bg)
        self._canvas.configure(background=parent_bg)
        self._entry.configure(background=bg)
        self._canvas.delete("all")
        self._rounded_rect(0, 0, width, height, 11, fill=bg, outline=border)

    def configure(self, cnf=None, **kwargs):  # type: ignore[override]
        options = {}
        if cnf:
            options.update(cnf)
        options.update(kwargs)
        frame_options = {}
        redraw = False
        if "style" in options:
            self._style = str(options.pop("style") or "TEntry")
            redraw = True
        if "padding" in options:
            self._padding = options.pop("padding")
            redraw = True
        if "width" in options:
            self._width_chars = options["width"]
        for key in ("background", "bg"):
            if key in options:
                frame_options[key] = options.pop(key)
                redraw = True
        entry_options = {}
        for key, value in list(options.items()):
            entry_options[key] = value
            options.pop(key, None)
        result = tk.Frame.configure(self, **frame_options) if frame_options else None
        if entry_options:
            self._entry.configure(**entry_options)
            redraw = True
        if redraw:
            self._layout()
        return result

    config = configure

    def cget(self, key):  # type: ignore[override]
        if key == "style":
            return self._style
        if key == "padding":
            return self._padding
        if key == "width":
            return self._width_chars
        try:
            return self._entry.cget(key)
        except tk.TclError:
            return tk.Frame.cget(self, key)

    def bind(self, sequence=None, func=None, add=None):  # type: ignore[override]
        frame_bind = tk.Frame.bind(self, sequence, func, add)
        if hasattr(self, "_entry"):
            self._entry.bind(sequence, func, add)
        return frame_bind

    def focus_set(self) -> None:
        self._entry.focus_set()

    focus = focus_set

    def __getattr__(self, name: str):
        entry = self.__dict__.get("_entry")
        if entry is not None and hasattr(entry, name):
            return getattr(entry, name)
        raise AttributeError(name)


class RoundedCheckbutton(tk.Canvas):
    def __init__(self, master=None, **kwargs) -> None:
        self._text = str(kwargs.pop("text", ""))
        self._textvariable = kwargs.pop("textvariable", None)
        self._variable = kwargs.pop("variable", None)
        self._command = kwargs.pop("command", None)
        self._style = str(kwargs.pop("style", "TCheckbutton") or "TCheckbutton")
        self._state = str(kwargs.pop("state", tk.NORMAL))
        self._onvalue = kwargs.pop("onvalue", True)
        self._offvalue = kwargs.pop("offvalue", False)
        self._padding = kwargs.pop("padding", None)
        self._takefocus = kwargs.pop("takefocus", True)
        for unsupported in (
            "indicatoron",
            "selectcolor",
            "activebackground",
            "activeforeground",
            "disabledforeground",
            "highlightthickness",
            "borderwidth",
            "bd",
            "relief",
            "underline",
        ):
            kwargs.pop(unsupported, None)

        if self._variable is None:
            self._variable = tk.BooleanVar(master=master, value=False)

        background = kwargs.pop("background", None) or kwargs.pop("bg", None) or RoundedButton._parent_bg(master)
        super().__init__(
            master,
            highlightthickness=0,
            bd=0,
            background=background,
            takefocus=1 if self._takefocus else 0,
            **kwargs,
        )

        self._hover = False
        self._pressed = False
        self._font = tkfont.nametofont("TkDefaultFont")
        self._textvariable_trace = None
        self._variable_trace = None
        if self._textvariable is not None:
            try:
                self._text = str(self._textvariable.get())
                self._textvariable_trace = self._textvariable.trace_add("write", self._on_textvariable_changed)
            except Exception:
                self._textvariable = None
        try:
            self._variable_trace = self._variable.trace_add("write", self._on_variable_changed)
        except Exception:
            self._variable_trace = None
        self._bind_events()
        self._redraw()

    def _bind_events(self) -> None:
        self.bind("<Enter>", self._on_enter)
        self.bind("<Leave>", self._on_leave)
        self.bind("<ButtonPress-1>", self._on_press)
        self.bind("<ButtonRelease-1>", self._on_release)
        self.bind("<space>", lambda _event: self.invoke())
        self.bind("<Return>", lambda _event: self.invoke())
        self.bind("<Configure>", lambda _event: self._redraw())

    def _style_value(self, option: str, fallback: str) -> str:
        try:
            value = ttk.Style(self).lookup(self._style, option, default="")
            return str(value) if value else fallback
        except tk.TclError:
            return fallback

    def _padding_xy(self) -> tuple[int, int]:
        if isinstance(self._padding, tuple) and len(self._padding) >= 2:
            return int(self._padding[0]), int(self._padding[1])
        return 10, 7

    def _selected(self) -> bool:
        try:
            value = self._variable.get()
        except Exception:
            return False
        if value == self._onvalue or str(value) == str(self._onvalue):
            return True
        if self._onvalue in (True, 1, "1") and value in (True, 1, "1"):
            return True
        return False

    def _requested_size(self) -> tuple[int, int]:
        if not self._text:
            return 18, 18
        _pad_x, pad_y = self._padding_xy()
        text_width = self._font.measure(self._text)
        width = max(44, text_width + 18 + 8)
        height = max(32, self._font.metrics("linespace") + pad_y * 2)
        return width, height

    def _rounded_rect(self, x1: int, y1: int, x2: int, y2: int, radius: int, **kwargs) -> None:
        radius = max(1, min(radius, (x2 - x1) // 2, (y2 - y1) // 2))
        points = [
            x1 + radius, y1, x2 - radius, y1, x2, y1, x2, y1 + radius,
            x2, y2 - radius, x2, y2, x2 - radius, y2, x1 + radius, y2,
            x1, y2, x1, y2 - radius, x1, y1 + radius, x1, y1,
        ]
        self.create_polygon(points, smooth=True, splinesteps=12, **kwargs)

    def _redraw(self) -> None:
        self.delete("all")
        req_width, req_height = self._requested_size()
        tk.Canvas.configure(self, width=req_width, height=req_height)
        width = max(req_width, self.winfo_width() or req_width)
        height = max(req_height, self.winfo_height() or req_height)
        parent_bg = RoundedButton._parent_bg(self.master)
        tk.Canvas.configure(self, background=parent_bg)

        selected = self._selected()
        disabled = self._state == tk.DISABLED or self._state == "disabled"
        text_color = self._style_value("foreground", "#111827")
        if disabled:
            box_fill, box_outline, text_color = "#e5e7eb", "#d1d5db", "#9ca3af"
        elif selected:
            box_fill, box_outline = "#f97316", "#f97316"
        elif self._pressed:
            box_fill, box_outline = "#f8fafc", "#cbd5e1"
        elif self._hover:
            box_fill, box_outline = "#ffffff", "#f97316"
        else:
            box_fill, box_outline = "#ffffff", "#cbd5e1"

        box_size = 18
        pad_x = 0
        box_x = pad_x
        box_y = int((height - box_size) / 2)
        self._rounded_rect(box_x, box_y, box_x + box_size, box_y + box_size, 6, fill=box_fill, outline=box_outline)
        if selected:
            self.create_line(
                box_x + 4,
                box_y + 9,
                box_x + 8,
                box_y + 13,
                box_x + 14,
                box_y + 5,
                fill="#ffffff",
                width=2,
                capstyle=tk.ROUND,
                joinstyle=tk.ROUND,
            )
        if self._text:
            self.create_text(
                box_x + box_size + 8,
                height / 2,
                text=self._text,
                fill=text_color,
                font=self._font,
                anchor="w",
            )

    def _on_enter(self, _event) -> None:
        self._hover = True
        self._redraw()

    def _on_leave(self, _event) -> None:
        self._hover = False
        self._pressed = False
        self._redraw()

    def _on_press(self, _event) -> None:
        if self._state == tk.DISABLED or self._state == "disabled":
            return
        self._pressed = True
        self._redraw()

    def _on_release(self, _event) -> None:
        if self._state == tk.DISABLED or self._state == "disabled":
            return
        was_pressed = self._pressed
        self._pressed = False
        if was_pressed:
            self.invoke()
        else:
            self._redraw()

    def _on_textvariable_changed(self, *_args) -> None:
        if self._textvariable is None:
            return
        try:
            self._text = str(self._textvariable.get())
        except Exception:
            return
        self._redraw()

    def _on_variable_changed(self, *_args) -> None:
        self._redraw()

    def invoke(self):
        if self._state == tk.DISABLED or self._state == "disabled":
            return None
        self.toggle()
        if self._command is not None:
            return self._command()
        return None

    def toggle(self) -> None:
        try:
            self._variable.set(self._offvalue if self._selected() else self._onvalue)
        except Exception:
            pass
        self._redraw()

    def select(self) -> None:
        try:
            self._variable.set(self._onvalue)
        except Exception:
            pass
        self._redraw()

    def deselect(self) -> None:
        try:
            self._variable.set(self._offvalue)
        except Exception:
            pass
        self._redraw()

    def state(self, statespec=None):
        if statespec is None:
            return ("disabled",) if self._state in {tk.DISABLED, "disabled"} else ()
        specs = (statespec,) if isinstance(statespec, str) else tuple(statespec)
        previous = self.state()
        for spec in specs:
            if spec == "disabled":
                self._state = tk.DISABLED
            elif spec == "!disabled":
                self._state = tk.NORMAL
        self._redraw()
        return previous

    def instate(self, statespec, callback=None, *args):
        specs = (statespec,) if isinstance(statespec, str) else tuple(statespec)
        disabled = self._state in {tk.DISABLED, "disabled"}
        selected = self._selected()
        matches = all(
            (spec == "disabled" and disabled)
            or (spec == "!disabled" and not disabled)
            or (spec == "selected" and selected)
            or (spec == "!selected" and not selected)
            for spec in specs
        )
        if matches and callback is not None:
            return callback(*args)
        return matches

    def configure(self, cnf=None, **kwargs):  # type: ignore[override]
        options = {}
        if cnf:
            options.update(cnf)
        options.update(kwargs)
        redraw = False
        for key in ("text", "textvariable", "variable", "command", "style", "state", "onvalue", "offvalue", "padding"):
            if key not in options:
                continue
            value = options.pop(key)
            if key == "text":
                self._text = str(value)
            elif key == "textvariable":
                if self._textvariable is not None and self._textvariable_trace is not None:
                    try:
                        self._textvariable.trace_remove("write", self._textvariable_trace)
                    except Exception:
                        pass
                self._textvariable = value
                self._textvariable_trace = None
                if self._textvariable is not None:
                    try:
                        self._text = str(self._textvariable.get())
                        self._textvariable_trace = self._textvariable.trace_add("write", self._on_textvariable_changed)
                    except Exception:
                        self._textvariable = None
            elif key == "variable":
                if self._variable is not None and self._variable_trace is not None:
                    try:
                        self._variable.trace_remove("write", self._variable_trace)
                    except Exception:
                        pass
                self._variable = value
                try:
                    self._variable_trace = self._variable.trace_add("write", self._on_variable_changed)
                except Exception:
                    self._variable_trace = None
            elif key == "command":
                self._command = value
            elif key == "style":
                self._style = str(value or "TCheckbutton")
            elif key == "state":
                self._state = str(value)
            elif key == "onvalue":
                self._onvalue = value
            elif key == "offvalue":
                self._offvalue = value
            elif key == "padding":
                self._padding = value
            redraw = True
        for unsupported in (
            "indicatoron",
            "selectcolor",
            "activebackground",
            "activeforeground",
            "disabledforeground",
            "highlightthickness",
            "borderwidth",
            "bd",
            "relief",
            "underline",
        ):
            options.pop(unsupported, None)
        result = super().configure(**options) if options else None
        if redraw:
            self._redraw()
        return result

    config = configure

    def cget(self, key):  # type: ignore[override]
        if key == "text":
            return self._text
        if key == "textvariable":
            return self._textvariable
        if key == "variable":
            return self._variable
        if key == "command":
            return self._command
        if key == "style":
            return self._style
        if key == "state":
            return self._state
        if key == "onvalue":
            return self._onvalue
        if key == "offvalue":
            return self._offvalue
        if key == "padding":
            return self._padding
        return super().cget(key)


def install_rounded_buttons() -> None:
    global _INSTALLED
    if _INSTALLED:
        return
    tk.Button = RoundedButton  # type: ignore[assignment]
    ttk.Button = RoundedButton  # type: ignore[assignment]
    tk.Entry = RoundedEntry  # type: ignore[assignment]
    tk.Checkbutton = RoundedCheckbutton  # type: ignore[assignment]
    ttk.Checkbutton = RoundedCheckbutton  # type: ignore[assignment]
    _INSTALLED = True
