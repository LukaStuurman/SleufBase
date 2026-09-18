from __future__ import annotations

from pathlib import Path
from types import SimpleNamespace
from tempfile import TemporaryDirectory
import unittest
import xml.etree.ElementTree as ET
from zipfile import ZipFile

from SleufBase.discipline_excel_export import (
    DisciplineSummary,
    discipline_columns,
    summarize_dataset,
    write_discipline_workbook,
)
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
        self.assertNotIn("PUNT", summary.counts)
        self.assertNotIn("niet-ingedeeld", summary.counts)

    def test_configured_discipline_columns_keep_rule_order(self) -> None:
        self.assertEqual(
            discipline_columns(self.rules),
            ("Waterleiding", "Datakabel", "Laagspanning"),
        )

    def test_writer_creates_real_xlsx_with_expected_table(self) -> None:
        summaries = [
            DisciplineSummary(
                "PS1",
                {
                    "Waterleiding": 2,
                    "Datakabel": 1,
                    "Laagspanning": 0,
                },
            )
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
        self.assertEqual(inline_text("C1"), "Waterleiding")
        self.assertEqual(inline_text("D1"), "Datakabel")
        self.assertEqual(inline_text("E1"), "Laagspanning")
        self.assertEqual(inline_text("A2"), "PS1")
        self.assertEqual(number("B2"), 2)
        self.assertEqual(number("C2"), 2)
        self.assertEqual(number("D2"), 1)
        self.assertEqual(number("E2"), 0)


if __name__ == "__main__":
    unittest.main()
