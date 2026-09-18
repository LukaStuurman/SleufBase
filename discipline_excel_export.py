from __future__ import annotations

from collections import Counter
from dataclasses import dataclass
from pathlib import Path
import tkinter as tk
from tkinter import filedialog, messagebox, ttk
from typing import Any, Iterable, Mapping, Sequence
from xml.sax.saxutils import escape
from zipfile import ZIP_DEFLATED, ZipFile

from .kickthemap_dxf_export import ObjectLayerRule, build_object_layer_rules


PATCH_VERSION = 5
MENU_LABEL = "Discipline-overzicht naar Excel…"
TECHBASE_ORANGE_DARK = "FFC2410C"
TECHBASE_ORANGE_LIGHT = "FFFFF7ED"


@dataclass(frozen=True)
class DisciplineSummary:
    proefsleuf: str
    counts: Mapping[str, int]

    @property
    def distinct_count(self) -> int:
        return sum(1 for value in self.counts.values() if int(value) > 0)

    @property
    def total_cables_and_pipes(self) -> int:
        return sum(max(0, int(value)) for value in self.counts.values())


def _rule_label(rule: ObjectLayerRule) -> str:
    label = str(getattr(rule, "profile_label", "") or "").strip()
    if label:
        return label
    keywords = tuple(getattr(rule, "keywords", ()) or ())
    if keywords:
        first = str(keywords[0] or "").strip()
        if first:
            return first
    target_layer = str(getattr(rule, "target_layer", "") or "").strip()
    return target_layer or "Onbekend"


def normalize_layer_rules(raw_rules: Iterable[Any] | None) -> tuple[ObjectLayerRule, ...]:
    rows: list[tuple[Any, Any, Any, Any]] = []
    direct: list[ObjectLayerRule] = []

    for rule in list(raw_rules or ()):
        if isinstance(rule, ObjectLayerRule):
            direct.append(rule)
            continue
        if all(hasattr(rule, name) for name in ("keywords", "target_layer", "color")):
            keywords = getattr(rule, "keywords", "")
            if isinstance(keywords, (list, tuple)):
                keywords = ",".join(str(value) for value in keywords if str(value).strip())
            rows.append(
                (
                    keywords,
                    getattr(rule, "target_layer", ""),
                    getattr(rule, "color", 256),
                    getattr(rule, "profile_label", ""),
                )
            )
            continue
        try:
            values = tuple(rule)
        except TypeError:
            continue
        if len(values) >= 4:
            rows.append((values[0], values[1], values[2], values[3]))
        elif len(values) == 3:
            rows.append((values[0], values[1], values[2], ""))

    if direct and not rows:
        return tuple(direct)
    if direct:
        rows = [
            (
                ",".join(str(value) for value in item.keywords),
                item.target_layer,
                item.color,
                item.profile_label,
            )
            for item in direct
        ] + rows
    return build_object_layer_rules(rows if rows else None)


def discipline_columns(rules: Sequence[ObjectLayerRule]) -> tuple[str, ...]:
    columns: list[str] = []
    seen: set[str] = set()
    for rule in rules:
        label = _rule_label(rule)
        key = label.casefold()
        if key in seen:
            continue
        seen.add(key)
        columns.append(label)
    return tuple(columns)


def summarize_dataset(
    proefsleuf: str,
    dataset: Any,
    rules: Sequence[ObjectLayerRule],
) -> DisciplineSummary:
    counts: Counter[str] = Counter()
    features = [
        *tuple(getattr(dataset, "points", ()) or ()),
        *tuple(getattr(dataset, "polylines", ()) or ()),
    ]
    for feature in features:
        source_name = str(getattr(feature, "source_name", "") or "").strip()
        if not source_name:
            continue
        matched = next((rule for rule in rules if rule.matches(source_name)), None)
        if matched is None:
            continue
        counts[_rule_label(matched)] += 1
    return DisciplineSummary(proefsleuf=str(proefsleuf).strip(), counts=dict(counts))


def _column_name(index: int) -> str:
    if index < 1:
        raise ValueError("Excel-kolomindex moet minimaal 1 zijn.")
    letters = ""
    value = int(index)
    while value:
        value, remainder = divmod(value - 1, 26)
        letters = chr(65 + remainder) + letters
    return letters


def _inline_string_cell(reference: str, value: object, *, style: int = 0) -> str:
    text = str(value)
    preserve = ' xml:space="preserve"' if text != text.strip() else ""
    style_attr = f' s="{style}"' if style else ""
    return (
        f'<c r="{reference}" t="inlineStr"{style_attr}><is>'
        f"<t{preserve}>{escape(text)}</t></is></c>"
    )


def _number_cell(reference: str, value: int, *, style: int = 0) -> str:
    style_attr = f' s="{style}"' if style else ""
    return f'<c r="{reference}" t="n"{style_attr}><v>{int(value)}</v></c>'


def _worksheet_xml(
    summaries: Sequence[DisciplineSummary],
    disciplines: Sequence[str],
) -> str:
    headers = [
        "Proefsleuf",
        "Aantal verschillende disciplines",
        "Totaal aantal kabels/leidingen",
        *disciplines,
    ]
    rows: list[str] = []

    header_cells = [
        _inline_string_cell(f"{_column_name(column)}1", header, style=1)
        for column, header in enumerate(headers, start=1)
    ]
    rows.append(f'<row r="1" ht="24" customHeight="1">{"".join(header_cells)}</row>')

    for row_index, summary in enumerate(summaries, start=2):
        cells = [
            _inline_string_cell(f"A{row_index}", summary.proefsleuf, style=2),
            _number_cell(f"B{row_index}", summary.distinct_count, style=2),
            _number_cell(f"C{row_index}", summary.total_cables_and_pipes, style=2),
        ]
        for column_index, discipline in enumerate(disciplines, start=4):
            cells.append(
                _number_cell(
                    f"{_column_name(column_index)}{row_index}",
                    int(summary.counts.get(discipline, 0)),
                    style=2,
                )
            )
        rows.append(f'<row r="{row_index}">{"".join(cells)}</row>')

    total_row_index = len(summaries) + 2
    discipline_totals = {
        discipline: sum(int(summary.counts.get(discipline, 0)) for summary in summaries)
        for discipline in disciplines
    }
    total_cells = [
        _inline_string_cell(f"A{total_row_index}", "Totaal", style=3),
        _number_cell(
            f"B{total_row_index}",
            sum(1 for value in discipline_totals.values() if value > 0),
            style=3,
        ),
        _number_cell(
            f"C{total_row_index}",
            sum(discipline_totals.values()),
            style=3,
        ),
    ]
    for column_index, discipline in enumerate(disciplines, start=4):
        total_cells.append(
            _number_cell(
                f"{_column_name(column_index)}{total_row_index}",
                discipline_totals[discipline],
                style=3,
            )
        )
    rows.append(
        f'<row r="{total_row_index}" ht="22" customHeight="1">{"".join(total_cells)}</row>'
    )

    last_column = _column_name(len(headers))
    data_last_row = max(1, len(summaries) + 1)
    last_row = total_row_index
    column_widths = [
        24.0,
        31.0,
        29.0,
        *[max(12.0, min(28.0, len(name) + 3.0)) for name in disciplines],
    ]
    cols = "".join(
        f'<col min="{index}" max="{index}" width="{width:.1f}" customWidth="1"/>'
        for index, width in enumerate(column_widths, start=1)
    )
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">'
        f'<dimension ref="A1:{last_column}{last_row}"/>'
        '<sheetViews><sheetView workbookViewId="0">'
        '<pane ySplit="1" topLeftCell="A2" activePane="bottomLeft" state="frozen"/>'
        '</sheetView></sheetViews>'
        '<sheetFormatPr defaultRowHeight="15"/>'
        f"<cols>{cols}</cols>"
        f'<sheetData>{"".join(rows)}</sheetData>'
        f'<autoFilter ref="A1:{last_column}{data_last_row}"/>'
        '</worksheet>'
    )


def write_discipline_workbook(
    path: str | Path,
    summaries: Sequence[DisciplineSummary],
    *,
    disciplines: Sequence[str] | None = None,
) -> Path:
    target = Path(path)
    target.parent.mkdir(parents=True, exist_ok=True)

    if disciplines is None:
        discovered = {
            str(name)
            for summary in summaries
            for name, count in summary.counts.items()
            if int(count) > 0
        }
        discipline_order = tuple(sorted(discovered, key=str.casefold))
    else:
        discipline_order = tuple(dict.fromkeys(str(name) for name in disciplines if str(name).strip()))

    content_types = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="xml" ContentType="application/xml"/>'
        '<Override PartName="/xl/workbook.xml" '
        'ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>'
        '<Override PartName="/xl/worksheets/sheet1.xml" '
        'ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>'
        '<Override PartName="/xl/styles.xml" '
        'ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>'
        '</Types>'
    )
    root_rels = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" '
        'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" '
        'Target="xl/workbook.xml"/>'
        '</Relationships>'
    )
    workbook = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" '
        'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">'
        '<sheets><sheet name="Disciplines" sheetId="1" r:id="rId1"/></sheets>'
        '</workbook>'
    )
    workbook_rels = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" '
        'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" '
        'Target="worksheets/sheet1.xml"/>'
        '<Relationship Id="rId2" '
        'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" '
        'Target="styles.xml"/>'
        '</Relationships>'
    )
    styles = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">'
        '<fonts count="3">'
        '<font><sz val="11"/><name val="Calibri"/><family val="2"/></font>'
        '<font><b/><color rgb="FFFFFFFF"/><sz val="11"/><name val="Calibri"/><family val="2"/></font>'
        '<font><b/><sz val="11"/><name val="Calibri"/><family val="2"/></font>'
        '</fonts>'
        '<fills count="4">'
        '<fill><patternFill patternType="none"/></fill>'
        '<fill><patternFill patternType="gray125"/></fill>'
        f'<fill><patternFill patternType="solid"><fgColor rgb="{TECHBASE_ORANGE_DARK}"/><bgColor rgb="{TECHBASE_ORANGE_DARK}"/></patternFill></fill>'
        f'<fill><patternFill patternType="solid"><fgColor rgb="{TECHBASE_ORANGE_LIGHT}"/><bgColor rgb="{TECHBASE_ORANGE_LIGHT}"/></patternFill></fill>'
        '</fills>'
        '<borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders>'
        '<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>'
        '<cellXfs count="4">'
        '<xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/>'
        '<xf numFmtId="0" fontId="1" fillId="2" borderId="0" xfId="0" applyFont="1" applyFill="1" applyAlignment="1">'
        '<alignment horizontal="center" vertical="center"/>'
        '</xf>'
        '<xf numFmtId="0" fontId="0" fillId="3" borderId="0" xfId="0" applyFill="1"/>'
        '<xf numFmtId="0" fontId="2" fillId="3" borderId="0" xfId="0" applyFont="1" applyFill="1"/>'
        '</cellXfs>'
        '<cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles>'
        '</styleSheet>'
    )

    worksheet = _worksheet_xml(summaries, discipline_order)
    with ZipFile(target, "w", compression=ZIP_DEFLATED) as archive:
        archive.writestr("[Content_Types].xml", content_types)
        archive.writestr("_rels/.rels", root_rels)
        archive.writestr("xl/workbook.xml", workbook)
        archive.writestr("xl/_rels/workbook.xml.rels", workbook_rels)
        archive.writestr("xl/styles.xml", styles)
        archive.writestr("xl/worksheets/sheet1.xml", worksheet)
    return target


def _layer_display_name(app: Any, layer: Any, fallback_index: int) -> str:
    metadata = getattr(layer, "metadata", {}) or {}
    for key in ("template_proefsleuf_label", "kickthemap_job_title", "marxact_trench_name"):
        value = str(metadata.get(key, "") or "").strip()
        if value:
            return value

    exporter = getattr(app, "cadastral_exporter", None)
    for method_name in ("_template_proefsleuf_label", "_proefsleuf_base_name"):
        method = getattr(exporter, method_name, None)
        if callable(method):
            try:
                value = str(method(layer, fallback_index) or "").strip()
            except Exception:
                value = ""
            if value:
                return value

    path = getattr(layer, "path", None)
    if path is not None:
        stem = str(getattr(path, "stem", "") or "").strip()
        if stem:
            return stem
    return f"Proefsleuf {fallback_index}"


def template_discipline_excel_path(dxf_path: str | Path) -> Path:
    """Return the automatic Excel companion path for a DXF template export."""

    return Path(dxf_path).with_suffix(".xlsx")


def ordered_nonempty_export_layers(ordered_layers: Sequence[Any] | None) -> tuple[Any, ...]:
    """Return the selected non-empty layers in exactly the chosen export order."""

    return tuple(layer for layer in tuple(ordered_layers or ()) if layer is not None)


def template_export_plan(
    app: Any,
    ordered_layers: Sequence[Any],
) -> tuple[tuple[str, Any], ...]:
    """Capture the exact included DXF-template order and visible PS labels."""

    plan: list[tuple[str, Any]] = []
    for index, layer in enumerate(ordered_layers, start=1):
        if layer is None:
            continue
        metadata = getattr(layer, "metadata", {}) or {}
        label = str(metadata.get("template_proefsleuf_label", "") or "").strip()
        if not label:
            label = _layer_display_name(app, layer, index)
        plan.append((label, layer))
    return tuple(plan)


def summarize_template_export_plan(
    app: Any,
    plan: Sequence[tuple[str, Any]],
    rules: Sequence[ObjectLayerRule],
) -> tuple[list[DisciplineSummary], list[str]]:
    """Summarize every exported slot without ever shifting the DXF order."""

    summaries: list[DisciplineSummary] = []
    warnings: list[str] = []
    total = len(plan)
    for index, (name, layer) in enumerate(plan, start=1):
        try:
            app.set_status(f"Excel-disciplines tellen ({index}/{total}): {name}")
        except Exception:
            pass
        try:
            app.update_idletasks()
        except Exception:
            pass
        try:
            dataset = app._load_maaiveld_dataset_for_layer(layer)
        except Exception as exc:
            warnings.append(f"{name}: {exc}")
            summaries.append(DisciplineSummary(name, {}))
            continue
        if dataset is None:
            warnings.append(f"{name}: geen kabel/leidinggegevens beschikbaar.")
            summaries.append(DisciplineSummary(name, {}))
            continue
        summaries.append(summarize_dataset(name, dataset, rules))
    return summaries, warnings


def _add_actions_menu_item(app: Any) -> None:
    try:
        menu_bar = app.nametowidget(app.cget("menu"))
        end_index = menu_bar.index("end")
    except Exception:
        return

    actions_menu = None
    if end_index is not None:
        for index in range(end_index + 1):
            try:
                if menu_bar.type(index) != "cascade":
                    continue
                if str(menu_bar.entrycget(index, "label")) != "Acties":
                    continue
                actions_menu = app.nametowidget(menu_bar.entrycget(index, "menu"))
                break
            except Exception:
                continue

    if actions_menu is None:
        actions_menu = tk.Menu(menu_bar, tearoff=0)
        menu_bar.add_cascade(label="Acties", menu=actions_menu)

    try:
        submenu_end = actions_menu.index("end")
    except Exception:
        submenu_end = None
    if submenu_end is not None:
        for index in range(submenu_end + 1):
            try:
                if actions_menu.type(index) == "command" and str(actions_menu.entrycget(index, "label")) == MENU_LABEL:
                    return
            except Exception:
                continue

    actions_menu.add_command(label=MENU_LABEL, command=app.export_discipline_counts_excel)

    try:
        if hasattr(app, "_capture_modern_menu_specs"):
            app._modern_menu_specs = app._capture_modern_menu_specs(menu_bar)
            if getattr(app, "_modern_menu_bar", None) is not None:
                app.after(0, app._build_modern_menu_bar)
    except Exception:
        pass


def _patch_viewer_class(viewer_class: Any) -> None:
    if int(getattr(viewer_class, "_sleufbase_discipline_excel_export_patch_version", 0) or 0) >= PATCH_VERSION:
        return

    from .settings import (
        DEFAULT_TEMPLATE_AUTO_EXPORT_DISCIPLINE_EXCEL,
        TEMPLATE_AUTO_EXPORT_DISCIPLINE_EXCEL_KEY,
    )

    original_build_menu = viewer_class._build_menu
    original_open_settings_dialog = viewer_class.open_settings_dialog
    original_choose_template_order = viewer_class._choose_template_export_order
    original_export_cadastral = viewer_class.export_cadastral_dxf
    original_export_template = viewer_class.export_cadastral_template_dxf

    def _auto_template_excel_enabled(self) -> bool:
        return bool(
            getattr(
                self.settings,
                TEMPLATE_AUTO_EXPORT_DISCIPLINE_EXCEL_KEY,
                DEFAULT_TEMPLATE_AUTO_EXPORT_DISCIPLINE_EXCEL,
            )
        )

    def _widget_descendants(widget):
        for child in widget.winfo_children():
            yield child
            yield from _widget_descendants(child)

    def _widget_text(widget) -> str:
        try:
            return str(widget.cget("text") or "")
        except (AttributeError, tk.TclError):
            return ""

    def _open_settings_dialog_with_discipline_excel(self) -> None:
        existing_dialogs = {
            str(child)
            for child in self.winfo_children()
            if isinstance(child, tk.Toplevel)
        }
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
                in {
                    "Gebruik kaartpunten voor maaiveldtekst en -kleur in sjabloonexport",
                    "Vul de drie maaiveldvakken automatisch met BGT fysiek_voorkomen",
                }
            ),
            None,
        )
        save_button = next(
            (widget for widget in descendants if _widget_text(widget) == "Opslaan"),
            None,
        )
        if anchor is None or save_button is None:
            return

        content = anchor.master
        while content is not dialog:
            if any(
                _widget_text(widget) == "Proefsleuven-sjabloon"
                for widget in content.winfo_children()
            ):
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

        option_var = tk.BooleanVar(value=_auto_template_excel_enabled(self))
        ttk.Checkbutton(
            content,
            text="Maak bij DXF-sjabloonexport automatisch ook een discipline-Exceloverzicht",
            variable=option_var,
        ).grid(row=row, column=0, sticky="w", pady=(12, 0))
        ttk.Label(
            content,
            text=(
                "Het Excelbestand krijgt dezelfde bestandsnaam als de DXF en gebruikt exact "
                "dezelfde PS-namen en volgorde als de gekozen sjabloonexport."
            ),
            wraplength=430,
            justify="left",
        ).grid(row=row + 1, column=0, sticky="w", pady=(0, 12))
        setattr(dialog, "_discipline_excel_template_export_var", option_var)

        original_save_command = save_button.cget("command")
        if original_save_command:
            def save_with_discipline_excel_option():
                setattr(
                    self.settings,
                    TEMPLATE_AUTO_EXPORT_DISCIPLINE_EXCEL_KEY,
                    bool(option_var.get()),
                )
                if callable(original_save_command):
                    return original_save_command()
                return dialog.tk.call(str(original_save_command))

            save_button.configure(command=save_with_discipline_excel_option)

    def _choose_template_export_order_with_excel_capture(self, *args, **kwargs):
        ordered_layers = original_choose_template_order(self, *args, **kwargs)
        if ordered_layers is None:
            plan = ()
        else:
            plan = template_export_plan(self, list(ordered_layers))
        self._sleufbase_template_discipline_excel_plan = plan
        return ordered_layers

    def _export_template_with_discipline_excel(self, *args, **kwargs):
        if not _auto_template_excel_enabled(self):
            return original_export_template(self, *args, **kwargs)

        chosen_paths: list[str] = []
        original_asksaveasfilename = filedialog.asksaveasfilename

        def recording_asksaveasfilename(*dialog_args, **dialog_kwargs):
            selected = original_asksaveasfilename(*dialog_args, **dialog_kwargs)
            if selected:
                chosen_paths.append(str(selected))
            return selected

        filedialog.asksaveasfilename = recording_asksaveasfilename
        try:
            result = original_export_template(self, *args, **kwargs)
        finally:
            filedialog.asksaveasfilename = original_asksaveasfilename

        dxf_path: Path | None = None
        candidates: list[object] = []
        if isinstance(result, (str, Path)):
            candidates.append(result)
        candidates.extend(reversed(chosen_paths))
        for candidate in candidates:
            try:
                path = Path(candidate)
            except (TypeError, ValueError):
                continue
            if path.suffix.casefold() == ".dxf" and path.exists():
                dxf_path = path
                break
        if dxf_path is None:
            return result

        plan = tuple(getattr(self, "_sleufbase_template_discipline_excel_plan", ()) or ())
        if not plan:
            return result

        try:
            rules = normalize_layer_rules(self._resolved_cross_section_layer_rules())
            if not rules:
                raise RuntimeError("Er zijn geen kabel/leiding-disciplines geconfigureerd.")
            summaries, warnings = summarize_template_export_plan(self, plan, rules)
            output_path = write_discipline_workbook(
                template_discipline_excel_path(dxf_path),
                summaries,
                disciplines=discipline_columns(rules),
            )
        except Exception as exc:
            messagebox.showwarning(
                "DXF-sjabloonexport",
                "De DXF is opgeslagen, maar het automatische discipline-Exceloverzicht "
                f"kon niet worden gemaakt:\n{exc}",
                parent=self,
            )
            return result

        self.set_status(
            f"DXF-sjabloon en discipline-Excel opgeslagen: {dxf_path.name} / {output_path.name}"
        )
        if warnings:
            details = "\n".join(warnings[:10])
            if len(warnings) > 10:
                details += f"\n… en nog {len(warnings) - 10} proefsleuf/proefsleuven."
            messagebox.showwarning(
                "Discipline-overzicht",
                "DXF en Excel zijn opgeslagen. Voor enkele proefsleuven konden de "
                "disciplinegegevens niet worden uitgelezen; hun Excel-rij is behouden met nullen:\n\n"
                + details
                + f"\n\nExcel: {output_path}",
                parent=self,
            )
        return result

    def _export_cadastral_with_template_order(self, *args, **kwargs):
        layers = list(getattr(self, "tiff_layers", ()) or ())
        if not layers:
            return original_export_cadastral(self, *args, **kwargs)

        ordered_layers = self._choose_template_export_order()
        if ordered_layers is None:
            self.set_status("Kadastrale DXF-export geannuleerd.")
            return None
        selected_layers = list(ordered_nonempty_export_layers(ordered_layers))
        if not selected_layers:
            messagebox.showinfo(
                "Kadastrale export",
                "Selecteer minimaal één proefsleuf om mee te nemen.",
                parent=self,
            )
            self.set_status("Kadastrale DXF-export geannuleerd.")
            return None

        original_layers = self.tiff_layers
        self.tiff_layers = selected_layers
        try:
            return original_export_cadastral(self, *args, **kwargs)
        finally:
            self.tiff_layers = original_layers

    def export_discipline_counts_excel(self) -> None:
        layers = list(getattr(self, "tiff_layers", ()) or ())
        if not layers:
            messagebox.showinfo(
                "Discipline-overzicht",
                "Laad eerst één of meer proefsleuven.",
                parent=self,
            )
            return

        ordered_layers = self._choose_template_export_order()
        if ordered_layers is None:
            self.set_status("Discipline-overzicht exporteren geannuleerd.")
            return
        plan = template_export_plan(self, list(ordered_layers))
        if not plan:
            messagebox.showinfo(
                "Discipline-overzicht",
                "Selecteer minimaal één proefsleuf om mee te nemen.",
                parent=self,
            )
            self.set_status("Discipline-overzicht exporteren geannuleerd.")
            return

        target = filedialog.asksaveasfilename(
            parent=self,
            title="Discipline-overzicht exporteren",
            defaultextension=".xlsx",
            filetypes=(("Excel-werkmap", "*.xlsx"),),
            initialfile="SleufBase_disciplines.xlsx",
        )
        if not target:
            return

        try:
            raw_rules = self._resolved_cross_section_layer_rules()
            rules = normalize_layer_rules(raw_rules)
        except Exception as exc:
            messagebox.showerror(
                "Discipline-overzicht",
                f"De kabel/leiding-regels konden niet worden gelezen:\n{exc}",
                parent=self,
            )
            self.set_status("Discipline-overzicht exporteren mislukt.")
            return

        if not rules:
            messagebox.showerror(
                "Discipline-overzicht",
                "Er zijn geen kabel/leiding-disciplines geconfigureerd.",
                parent=self,
            )
            self.set_status("Discipline-overzicht exporteren mislukt.")
            return

        summaries, warnings = summarize_template_export_plan(self, plan, rules)
        if not summaries:
            messagebox.showerror(
                "Discipline-overzicht",
                "Geen geselecteerde proefsleuf kon worden verwerkt.",
                parent=self,
            )
            self.set_status("Discipline-overzicht exporteren mislukt.")
            return

        try:
            output_path = write_discipline_workbook(
                target,
                summaries,
                disciplines=discipline_columns(rules),
            )
        except Exception as exc:
            messagebox.showerror(
                "Discipline-overzicht",
                f"Het Excelbestand kon niet worden opgeslagen:\n{exc}",
                parent=self,
            )
            self.set_status("Discipline-overzicht exporteren mislukt.")
            return

        self.set_status(
            f"Discipline-overzicht opgeslagen voor {len(summaries)} proefsleuf/proefsleuven."
        )
        if warnings:
            details = "\n".join(warnings[:10])
            if len(warnings) > 10:
                details += f"\n… en nog {len(warnings) - 10} proefsleuf/proefsleuven."
            messagebox.showwarning(
                "Discipline-overzicht",
                "Het Excelbestand is gemaakt. Voor enkele geselecteerde proefsleuven konden de "
                "disciplinegegevens niet worden uitgelezen; hun rij blijft op de gekozen plaats staan "
                "met nullen:\n\n"
                + details
                + f"\n\nBestand: {output_path}",
                parent=self,
            )
        else:
            messagebox.showinfo(
                "Discipline-overzicht",
                f"Excelbestand opgeslagen:\n{output_path}",
                parent=self,
            )

    def _build_menu_with_discipline_excel(self) -> None:
        original_build_menu(self)
        _add_actions_menu_item(self)

    viewer_class._auto_template_discipline_excel_enabled = _auto_template_excel_enabled
    viewer_class.open_settings_dialog = _open_settings_dialog_with_discipline_excel
    viewer_class._choose_template_export_order = _choose_template_export_order_with_excel_capture
    viewer_class.export_cadastral_dxf = _export_cadastral_with_template_order
    viewer_class.export_cadastral_template_dxf = _export_template_with_discipline_excel
    viewer_class.export_discipline_counts_excel = export_discipline_counts_excel
    viewer_class._sleufbase_cadastral_export_order_dialog = True
    viewer_class._sleufbase_discipline_export_order_dialog = True
    viewer_class._build_menu = _build_menu_with_discipline_excel
    viewer_class._sleufbase_discipline_excel_export_patch_version = PATCH_VERSION


def install_discipline_excel_export_patch() -> None:
    from .app import KlicViewerApp

    _patch_viewer_class(KlicViewerApp)
