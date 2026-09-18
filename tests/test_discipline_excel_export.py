from __future__ import annotations

from pathlib import Path
from types import SimpleNamespace
from tempfile import TemporaryDirectory
import unittest
import xml.etree.ElementTree as ET
from zipfile import ZipFile

from SleufBase.discipline_excel_export import (
    DisciplineSummary,
    TECHBASE_ORANGE_DARK,
    TECHBASE_ORANGE_LIGHT,
    discipline_columns,
    ordered_nonempty_export_layers,
    summarize_dataset,
    summarize_template_export_plan,
    template_discipline_excel_path,
    template_export_plan,
    write_discipline_workbook,
)
from SleufBase.settings import (
    DEFAULT_TEMPLATE_AUTO_EXPORT_DISCIPLINE_EXCEL,
    DXF_TRENCH_EXPORT_NONE,
    DXF_TRENCH_EXPORT_OPTIONS,
    TEMPLATE_AUTO_EXPORT_DISCIPLINE_EXCEL_KEY,
    dxf_trench_export_label,
    dxf_trench_export_value_from_label,
)
from SleufBase.cadastral_export import CadastralDxfExporter
from SleufBase.models import Bounds

from SleufBase.kickthemap_dxf_export import (
    KickTheMapObjectPoint,
    KickTheMapObjectPolyline,
    KickTheMapPolylineVertex,
    build_object_layer_rules,
)


NS = {"x": "http://schemas.openxmlformats.org/spreadsheetml/2006/main"}


class DisciplineExcelExportTests(unittest.TestCase):
    def setUp(self) -> None:
        self.rules = build_object_layer_rules(
            [
                ("water", "WATER", 5, "Waterleiding"),
                ("data", "DATA", 3, "Datakabel"),
                ("ls", "LS", 190, "Laagspanning"),
            ]
        )

    def test_summary_counts_distinct_disciplines_and_occurrences(self) -> None:
        dataset = SimpleNamespace(
            points=(
                KickTheMapObjectPoint("Begin", "PUNT", 0.0, 0.0, 3.0),
                KickTheMapObjectPoint("Water 1", "water", 1.0, 0.0, 2.0),
                KickTheMapObjectPoint("Water 2", "water pvc", 2.0, 0.0, 2.0),
                KickTheMapObjectPoint("Data", "data", 3.0, 0.0, 2.0),
                KickTheMapObjectPoint("Onbekend", "niet-ingedeeld", 4.0, 0.0, 2.0),
            ),
            polylines=(
                KickTheMapObjectPolyline(
                    object_name="LS",
                    source_name="ls",
                    vertices=(
                        KickTheMapPolylineVertex(5.0, 0.0, 2.0),
                        KickTheMapPolylineVertex(6.0, 0.0, 2.0),
                    ),
                ),
            ),
        )

        summary = summarize_dataset("PS12", dataset, self.rules)

        self.assertEqual(summary.proefsleuf, "PS12")
        self.assertEqual(summary.distinct_count, 3)
        self.assertEqual(summary.counts["Waterleiding"], 2)
        self.assertEqual(summary.counts["Datakabel"], 1)
        self.assertEqual(summary.counts["Laagspanning"], 1)
        self.assertEqual(summary.total_cables_and_pipes, 4)
        self.assertNotIn("PUNT", summary.counts)
        self.assertNotIn("niet-ingedeeld", summary.counts)

    def test_configured_discipline_columns_keep_rule_order(self) -> None:
        self.assertEqual(
            discipline_columns(self.rules),
            ("Waterleiding", "Datakabel", "Laagspanning"),
        )

    def test_template_excel_is_enabled_by_default_and_uses_dxf_basename(self) -> None:
        self.assertTrue(DEFAULT_TEMPLATE_AUTO_EXPORT_DISCIPLINE_EXCEL)
        self.assertEqual(
            TEMPLATE_AUTO_EXPORT_DISCIPLINE_EXCEL_KEY,
            "template_auto_export_discipline_excel",
        )
        self.assertEqual(
            template_discipline_excel_path(Path("C:/export/Project_sjabloon.dxf")),
            Path("C:/export/Project_sjabloon.xlsx"),
        )

    def test_template_export_plan_keeps_exact_dxf_names_and_order(self) -> None:
        first = SimpleNamespace(
            metadata={"template_proefsleuf_label": "PS7B"},
            path=Path("later.tiff"),
        )
        second = SimpleNamespace(
            metadata={"template_proefsleuf_label": "PS2"},
            path=Path("eerder.tiff"),
        )
        app = SimpleNamespace(cadastral_exporter=None)

        plan = template_export_plan(app, [first, None, second])

        self.assertEqual([name for name, _layer in plan], ["PS7B", "PS2"])
        self.assertIs(plan[0][1], first)
        self.assertIs(plan[1][1], second)

    def test_shared_order_filters_only_empty_slots(self) -> None:
        first = object()
        second = object()

        ordered = ordered_nonempty_export_layers([second, None, first])

        self.assertEqual(ordered, (second, first))

    def test_none_trench_mode_is_available_in_settings(self) -> None:
        self.assertEqual(DXF_TRENCH_EXPORT_NONE, "none")
        self.assertIn(("none", "Geen"), tuple(DXF_TRENCH_EXPORT_OPTIONS))
        self.assertEqual(dxf_trench_export_label("none"), "Geen")
        self.assertEqual(dxf_trench_export_value_from_label("Geen"), "none")

    def test_cadastral_label_honors_selected_ps_name_and_variant(self) -> None:
        exporter = CadastralDxfExporter(wfs_client=object())
        layer = SimpleNamespace(
            metadata={"template_proefsleuf_label": "PS7B"},
            path=Path("origineel_ps1.tif"),
        )

        self.assertEqual(exporter._proefsleuf_label(layer, 1), "PS7B")

    def test_none_trench_mode_keeps_geotiff_when_raster_export_is_enabled(self) -> None:
        exporter = CadastralDxfExporter(wfs_client=object())
        layer = SimpleNamespace(
            metadata={"template_proefsleuf_label": "PS3C"},
            path=Path("PS3.tif"),
            bounds=Bounds(0.0, 0.0, 10.0, 5.0),
        )
        prepared = SimpleNamespace(layer=layer, raster_path=Path("PS3.png"))
        raster_calls: list[tuple[object, object, object, int]] = []

        exporter._prepare_tiff_raster_files = lambda _output, _layers: [prepared]
        exporter._add_tiff_image = (
            lambda _document, _modelspace, raster_layer, raster_path, index:
            raster_calls.append((raster_layer, raster_path, _modelspace, index))
        )
        exporter._proefsleuf_polygon = lambda *_args, **_kwargs: (
            (_ for _ in ()).throw(AssertionError("Geen mag geen proefsleufpolygon tekenen"))
        )
        exporter._proefsleuf_centerline = lambda *_args, **_kwargs: (
            (_ for _ in ()).throw(AssertionError("Geen mag geen proefsleufhartlijn tekenen"))
        )

        modelspace = object()
        exporter._populate_overview_modelspace(
            object(),
            modelspace,
            Path("overzicht.dxf"),
            [layer],
            [],
            [],
            layer.bounds,
            trench_mode=exporter.TRENCH_MODE_NONE,
            include_tiff_images=True,
            label_gap=0.0,
            centerline_color=(0, 0, 0),
            label_color=(0, 0, 0),
        )

        self.assertEqual(len(raster_calls), 1)
        self.assertIs(raster_calls[0][0], layer)
        self.assertEqual(raster_calls[0][1], Path("PS3.png"))
        self.assertIs(raster_calls[0][2], modelspace)
        self.assertEqual(raster_calls[0][3], 1)

    def test_template_summary_preserves_rows_when_dataset_is_missing(self) -> None:
        first = object()
        missing = object()
        datasets = {
            first: SimpleNamespace(
                points=(KickTheMapObjectPoint("Water", "water", 1.0, 0.0, 2.0),),
                polylines=(),
            ),
            missing: None,
        }

        class FakeApp:
            def set_status(self, _text: str) -> None:
                return None

            def update_idletasks(self) -> None:
                return None

            def _load_maaiveld_dataset_for_layer(self, layer):
                return datasets[layer]

        summaries, warnings = summarize_template_export_plan(
            FakeApp(),
            (("PS4", first), ("PS4B", missing)),
            self.rules,
        )

        self.assertEqual([summary.proefsleuf for summary in summaries], ["PS4", "PS4B"])
        self.assertEqual(summaries[0].counts["Waterleiding"], 1)
        self.assertEqual(dict(summaries[1].counts), {})
        self.assertEqual(len(warnings), 1)
        self.assertIn("PS4B", warnings[0])

    def test_writer_creates_real_xlsx_with_expected_table(self) -> None:
        summaries = [
            DisciplineSummary(
                "PS1",
                {
                    "Waterleiding": 2,
                    "Datakabel": 1,
                    "Laagspanning": 0,
                },
            ),
            DisciplineSummary(
                "PS2",
                {
                    "Waterleiding": 1,
                    "Datakabel": 0,
                    "Laagspanning": 4,
                },
            ),
        ]
        with TemporaryDirectory() as temporary_directory:
            output = Path(temporary_directory) / "disciplines.xlsx"
            write_discipline_workbook(
                output,
                summaries,
                disciplines=("Waterleiding", "Datakabel", "Laagspanning"),
            )

            self.assertTrue(output.exists())
            with ZipFile(output) as archive:
                self.assertIn("[Content_Types].xml", archive.namelist())
                self.assertIn("xl/workbook.xml", archive.namelist())
                self.assertIn("xl/worksheets/sheet1.xml", archive.namelist())
                worksheet = ET.fromstring(archive.read("xl/worksheets/sheet1.xml"))
                styles = ET.fromstring(archive.read("xl/styles.xml"))

        cells = {
            cell.attrib["r"]: cell
            for cell in worksheet.findall(".//x:c", NS)
        }

        def inline_text(reference: str) -> str:
            node = cells[reference].find("x:is/x:t", NS)
            return "" if node is None or node.text is None else node.text

        def number(reference: str) -> int:
            node = cells[reference].find("x:v", NS)
            return int(node.text) if node is not None and node.text is not None else -1

        self.assertEqual(inline_text("A1"), "Proefsleuf")
        self.assertEqual(inline_text("B1"), "Aantal verschillende disciplines")
        self.assertEqual(inline_text("C1"), "Totaal aantal kabels/leidingen")
        self.assertEqual(inline_text("D1"), "Waterleiding")
        self.assertEqual(inline_text("E1"), "Datakabel")
        self.assertEqual(inline_text("F1"), "Laagspanning")
        self.assertEqual(inline_text("A2"), "PS1")
        self.assertEqual(number("B2"), 2)
        self.assertEqual(number("C2"), 3)
        self.assertEqual(number("D2"), 2)
        self.assertEqual(number("E2"), 1)
        self.assertEqual(number("F2"), 0)
        self.assertEqual(inline_text("A3"), "PS2")
        self.assertEqual(number("B3"), 2)
        self.assertEqual(number("C3"), 5)
        self.assertEqual(number("D3"), 1)
        self.assertEqual(number("E3"), 0)
        self.assertEqual(number("F3"), 4)

        self.assertEqual(inline_text("A4"), "Totaal")
        self.assertEqual(number("B4"), 3)
        self.assertEqual(number("C4"), 8)
        self.assertEqual(number("D4"), 3)
        self.assertEqual(number("E4"), 1)
        self.assertEqual(number("F4"), 4)

        self.assertEqual(cells["A1"].attrib.get("s"), "1")
        self.assertEqual(cells["A2"].attrib.get("s"), "2")
        self.assertEqual(cells["A4"].attrib.get("s"), "3")

        fills = styles.findall("x:fills/x:fill/x:patternFill/x:fgColor", NS)
        fill_colors = [fill.attrib.get("rgb") for fill in fills]
        self.assertIn(TECHBASE_ORANGE_DARK, fill_colors)
        self.assertIn(TECHBASE_ORANGE_LIGHT, fill_colors)

        cell_xfs = styles.findall("x:cellXfs/x:xf", NS)
        self.assertEqual(cell_xfs[1].attrib.get("fillId"), "2")
        self.assertEqual(cell_xfs[2].attrib.get("fillId"), "3")
        self.assertEqual(cell_xfs[3].attrib.get("fillId"), "3")

        auto_filter = worksheet.find("x:autoFilter", NS)
        self.assertIsNotNone(auto_filter)
        self.assertEqual(auto_filter.attrib.get("ref"), "A1:F3")


if __name__ == "__main__":
    unittest.main()
