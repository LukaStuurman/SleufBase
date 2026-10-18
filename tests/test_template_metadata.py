from datetime import datetime
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import ezdxf

from SleufBase.cadastral_export import CadastralDxfExporter
from SleufBase.template_metadata import project_name, update_title_blocks


class TemplateMetadataTests(unittest.TestCase):
    def document(self):
        doc = ezdxf.new("R2018")
        block = doc.blocks.new("Title")
        block.add_attdef("DATUM1", (0, 0), text="definition")
        inserts = []
        for name in ("Layout1", "Blad2", "Unused"):
            layout = doc.layouts.get(name) if name == "Layout1" else doc.layouts.new(name)
            insert = layout.add_blockref("Title", (0, 0))
            tags = ["DATUM", "DATUM", "PROJECTOMSCHRIJVING"] if name == "Layout1" else ["DATUM1", "DATUM2", "GEM-PROJECT"]
            for index, tag in enumerate(tags):
                insert.add_attrib(tag, f"before{index}")
            inserts.append(insert)
        return doc, inserts

    def test_multiple_sheets_fill_only_first_revision_and_survive_save(self):
        doc, inserts = self.document()
        update_title_blocks(doc, ["Layout1", "Blad2", "Missing"], project="  Zuid Ø 漢😀  ", export_date=datetime(2026, 10, 18))
        for insert in inserts[:2]:
            self.assertEqual([a.dxf.text for a in insert.attribs], ["18-10-2026", "before1", "Zuid Ø 漢😀"])
        self.assertEqual(inserts[2].attribs[0].dxf.text, "before0")
        self.assertEqual(doc.blocks["Title"].query("ATTDEF")[0].dxf.text, "definition")
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "export.dxf"
            doc.saveas(path)
            reloaded = ezdxf.readfile(path)
            self.assertEqual(reloaded.layouts.get("Blad2").query("INSERT")[0].get_attrib_text("GEM-PROJECT"), "Zuid Ø 漢😀")

    def test_exporter_receives_dialog_project_and_explicit_override_or_clear(self):
        exporter = CadastralDxfExporter(wfs_client=None)
        exporter.template_project_name = "Dialog project"
        doc, inserts = self.document()
        exporter._update_template_title_block_attributes(doc, ["Blad2"], template_drawn_by="")
        self.assertEqual(inserts[1].get_attrib_text("GEM-PROJECT"), "Dialog project")
        self.assertEqual(inserts[1].get_attrib_text("DATUM1"), datetime.now().strftime("%d-%m-%Y"))
        exporter._update_template_title_block_attributes(doc, ["Blad2"], template_drawn_by="", template_project_name="Override")
        self.assertEqual(inserts[1].get_attrib_text("GEM-PROJECT"), "Override")
        exporter._update_template_title_block_attributes(doc, ["Blad2"], template_drawn_by="", template_project_name="")
        self.assertEqual(inserts[1].get_attrib_text("GEM-PROJECT"), "")

    def test_date_captured_once_and_blank_project_can_be_preserved(self):
        doc, inserts = self.document()
        with patch("SleufBase.template_metadata.datetime") as clock:
            clock.now.return_value = datetime(2026, 12, 31, 23, 59)
            update_title_blocks(doc, ["Layout1", "Blad2"])
            clock.now.assert_called_once_with()
        self.assertEqual(inserts[0].attribs[0].dxf.text, "31-12-2026")
        self.assertEqual(inserts[1].attribs[0].dxf.text, "31-12-2026")
        self.assertEqual(inserts[1].get_attrib_text("GEM-PROJECT"), "before2")

    def test_invalid_project_fails_before_changing_any_sheet(self):
        doc, inserts = self.document()
        for name in ["x\ny", "a\x85b", "😀" * 1025]:
            with self.subTest(name=name[:12]):
                with self.assertRaises(ValueError):
                    update_title_blocks(doc, ["Layout1", "Blad2"], project=name)
                self.assertEqual(inserts[0].attribs[0].dxf.text, "before0")
        self.assertEqual(project_name(" Zuid "), "Zuid")
